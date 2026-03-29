use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth;
use crate::error::AppError;
use crate::types::{PermissionLevel, WsMessage};
use crate::AppState;

/// GET /api/ws/events — streams status changes for all servers.
pub async fn global_events_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, AppError> {
    if let Some(ticket_str) = &query.ticket {
        let scope = "/api/ws/events";
        let _ticket = state
            .ws_ticket_store
            .redeem(ticket_str, Some(scope))
            .map_err(AppError::Unauthorized)?;
    } else if query.token.is_some() {
        return Err(AppError::Unauthorized(
            "The ?token= query parameter has been removed. \
             Use the ticket-based flow instead: POST /api/auth/ws-ticket to mint a \
             single-use ticket, then connect with ?ticket=<value>."
                .into(),
        ));
    } else {
        return Err(AppError::Unauthorized(
            "Missing authentication: provide a ?ticket= query parameter \
             (mint one via POST /api/auth/ws-ticket)"
                .into(),
        ));
    }

    Ok(ws.on_upgrade(move |socket| handle_global_socket(state, socket)))
}

async fn handle_global_socket(state: Arc<AppState>, socket: WebSocket) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    let mut broadcast_rx = state.process_manager.subscribe_global();

    let send_task = tokio::spawn(async move {
        loop {
            match broadcast_rx.recv().await {
                Ok(ws_msg) => {
                    if !matches!(ws_msg, WsMessage::StatusChange(_)) {
                        continue;
                    }
                    match serde_json::to_string(&ws_msg) {
                        Ok(json) => {
                            if ws_tx.send(Message::Text(json)).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to serialize global WS message: {}", e);
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::debug!("Global WS consumer lagged by {} messages", n);
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    });

    while let Some(result) = ws_rx.next().await {
        match result {
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(_) => break,
        }
    }

    send_task.abort();
    tracing::debug!("Global events WebSocket closed");
}

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    pub token: Option<String>,
    pub ticket: Option<String>,
}

/// GET /api/servers/:id/ws — per-server console WebSocket.
pub async fn ws_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, AppError> {
    let auth_user = if let Some(ticket_str) = &query.ticket {
        let scope = format!("/api/servers/{}/ws", id);
        let ticket = state
            .ws_ticket_store
            .redeem(ticket_str, Some(&scope))
            .map_err(AppError::Unauthorized)?;

        auth::AuthUser {
            user_id: ticket.user_id,
            username: String::new(),
            role: ticket.role,
        }
    } else if query.token.is_some() {
        return Err(AppError::Unauthorized(
            "The ?token= query parameter has been removed. \
             Use the ticket-based flow instead: POST /api/auth/ws-ticket to mint a \
             single-use ticket, then connect with ?ticket=<value>."
                .into(),
        ));
    } else {
        return Err(AppError::Unauthorized(
            "Missing authentication: provide a ?ticket= query parameter \
             (mint one via POST /api/auth/ws-ticket)"
                .into(),
        ));
    };

    let server = state.db.require_server(id).await?;

    auth_user
        .require_level(&state, &server, PermissionLevel::Viewer)
        .await?;

    Ok(ws.on_upgrade(move |socket| handle_socket(state, id, socket)))
}

async fn handle_socket(state: Arc<AppState>, server_id: Uuid, socket: WebSocket) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    // Replay buffered log history so the client catches up immediately.
    let history = state.process_manager.get_log_buffer(&server_id);
    for log_line in history {
        let msg = WsMessage::Log(log_line);
        match serde_json::to_string(&msg) {
            Ok(json) => {
                if ws_tx.send(Message::Text(json)).await.is_err() {
                    return;
                }
            }
            Err(e) => {
                tracing::warn!("Failed to serialize log history entry: {}", e);
            }
        }
    }

    // Replay phase logs if a pipeline is running or completed within the
    // last 60s (covers the race where the pipeline finishes before we subscribe).
    let should_replay_phase_logs = state.pipeline_manager.is_running(&server_id)
        || state
            .pipeline_manager
            .get_progress(&server_id)
            .and_then(|p| p.completed_at)
            .is_some_and(|t| (chrono::Utc::now() - t).num_seconds() < 60);
    if should_replay_phase_logs {
        let phase_logs = state.pipeline_manager.get_phase_log_buffer(&server_id);
        for log_line in phase_logs {
            let msg = WsMessage::PhaseLog(log_line);
            match serde_json::to_string(&msg) {
                Ok(json) => {
                    if ws_tx.send(Message::Text(json)).await.is_err() {
                        return;
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to serialize phase log history entry: {}", e);
                }
            }
        }
    }

    if let Some(progress) = state.pipeline_manager.get_progress(&server_id) {
        let msg = WsMessage::PhaseProgress(progress);
        if let Ok(json) = serde_json::to_string(&msg) {
            let _ = ws_tx.send(Message::Text(json)).await;
        }
    }

    let current_runtime = state.process_manager.get_runtime(&server_id);
    if let Ok(json) = serde_json::to_string(&WsMessage::StatusChange(current_runtime)) {
        let _ = ws_tx.send(Message::Text(json)).await;
    }

    // Poll until a broadcast channel appears (server may not have started yet).
    let mut broadcast_rx = loop {
        if let Some(rx) = state.process_manager.subscribe(&server_id) {
            break rx;
        }

        if let Some(rx) = state.pipeline_manager.subscribe(&server_id) {
            break rx;
        }

        tokio::select! {
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => return,
                    _ => continue,
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(250)) => {
                continue;
            }
        }
    };

    // Re-send runtime to close the race window between initial replay
    // and subscription — a status change may have been broadcast in between.
    let refreshed_runtime = state.process_manager.get_runtime(&server_id);
    if let Ok(json) = serde_json::to_string(&WsMessage::StatusChange(refreshed_runtime)) {
        let _ = ws_tx.send(Message::Text(json)).await;
    }

    // The send task forwards broadcast messages to the WebSocket client.
    // When a broadcast channel closes (e.g., a ProcessHandle is replaced
    // during install → start transitions), we re-subscribe to the new
    // channel instead of silently dying.  This prevents the client from
    // missing status changes that arrive on the replacement channel.
    let send_state = Arc::clone(&state);
    let send_server_id = server_id;
    let send_task = tokio::spawn(async move {
        'outer: loop {
            loop {
                match broadcast_rx.recv().await {
                    Ok(ws_msg) => match serde_json::to_string(&ws_msg) {
                        Ok(json) => {
                            if ws_tx.send(Message::Text(json)).await.is_err() {
                                break 'outer;
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to serialize WS message: {}", e);
                        }
                    },
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::debug!("WebSocket consumer lagged by {} messages", n);
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::debug!(
                            "Broadcast channel closed for server {}, attempting re-subscribe",
                            send_server_id,
                        );
                        break; // break inner loop to re-subscribe
                    }
                }
            }

            // Channel closed — try to find a new one.  Poll for up to 5s
            // to cover the window where a handle is being replaced.
            let mut resubscribed = false;
            for _ in 0..20 {
                if let Some(rx) = send_state.process_manager.subscribe(&send_server_id) {
                    broadcast_rx = rx;
                    resubscribed = true;
                    break;
                }
                if let Some(rx) = send_state.pipeline_manager.subscribe(&send_server_id) {
                    broadcast_rx = rx;
                    resubscribed = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }

            if !resubscribed {
                tracing::debug!(
                    "Could not re-subscribe to broadcast for server {} — closing send task",
                    send_server_id,
                );
                break 'outer;
            }

            // Replay log buffer to fill the gap between channel close and
            // re-subscribe — same as the initial connection replay.
            let history = send_state.process_manager.get_log_buffer(&send_server_id);
            for log_line in history {
                let msg = WsMessage::Log(log_line);
                if let Ok(json) = serde_json::to_string(&msg) {
                    if ws_tx.send(Message::Text(json)).await.is_err() {
                        break 'outer;
                    }
                }
            }

            // Also replay phase logs if a pipeline is active or recently completed.
            let should_replay = send_state.pipeline_manager.is_running(&send_server_id)
                || send_state
                    .pipeline_manager
                    .get_progress(&send_server_id)
                    .and_then(|p| p.completed_at)
                    .is_some_and(|t| (chrono::Utc::now() - t).num_seconds() < 60);
            if should_replay {
                let phase_logs = send_state
                    .pipeline_manager
                    .get_phase_log_buffer(&send_server_id);
                for log_line in phase_logs {
                    let msg = WsMessage::PhaseLog(log_line);
                    if let Ok(json) = serde_json::to_string(&msg) {
                        if ws_tx.send(Message::Text(json)).await.is_err() {
                            break 'outer;
                        }
                    }
                }
            }

            // Send phase progress if applicable.
            if let Some(progress) = send_state.pipeline_manager.get_progress(&send_server_id) {
                let msg = WsMessage::PhaseProgress(progress);
                if let Ok(json) = serde_json::to_string(&msg) {
                    if ws_tx.send(Message::Text(json)).await.is_err() {
                        break 'outer;
                    }
                }
            }

            // After re-subscribing, send the current runtime so the client
            // doesn't miss any status change that was broadcast on the old
            // channel between close and re-subscribe.
            let runtime = send_state.process_manager.get_runtime(&send_server_id);
            if let Ok(json) = serde_json::to_string(&WsMessage::StatusChange(runtime)) {
                if ws_tx.send(Message::Text(json)).await.is_err() {
                    break 'outer;
                }
            }
        }
    });

    while let Some(result) = ws_rx.next().await {
        match result {
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(_)) => {}
            Ok(_) => {}
            Err(e) => {
                tracing::debug!("WebSocket receive error for server {}: {}", server_id, e);
                break;
            }
        }
    }

    send_task.abort();
    tracing::debug!("WebSocket closed for server {}", server_id);
}
