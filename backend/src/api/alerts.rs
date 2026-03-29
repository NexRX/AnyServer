use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::AppError;
use crate::monitoring::email;
use crate::types::*;
use crate::AppState;

/// GET /api/admin/smtp
pub async fn get_smtp_config(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Option<SmtpConfigPublic>>, AppError> {
    if !auth.is_admin() {
        return Err(AppError::Forbidden(
            "Only admins can view SMTP settings".into(),
        ));
    }

    let config = state.db.get_smtp_config().await?;
    Ok(Json(config.as_ref().map(SmtpConfigPublic::from)))
}

/// PUT /api/admin/smtp
pub async fn save_smtp_config(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(req): Json<SaveSmtpConfigRequest>,
) -> Result<Json<SmtpConfigPublic>, AppError> {
    if !auth.is_admin() {
        return Err(AppError::Forbidden(
            "Only admins can configure SMTP settings".into(),
        ));
    }

    if req.host.trim().is_empty() {
        return Err(AppError::BadRequest("SMTP host is required".into()));
    }
    if req.from_address.trim().is_empty() {
        return Err(AppError::BadRequest("From address is required".into()));
    }

    let password = match req.password {
        Some(p) => p,
        None => state
            .db
            .get_smtp_config()
            .await?
            .map(|c| c.password)
            .unwrap_or_default(),
    };

    let config = SmtpConfig {
        host: req.host.trim().to_string(),
        port: req.port,
        tls: req.tls,
        username: req.username.trim().to_string(),
        password,
        from_address: req.from_address.trim().to_string(),
    };

    state.db.save_smtp_config(&config).await?;

    tracing::info!("Admin '{}' updated SMTP configuration", auth.username);

    Ok(Json(SmtpConfigPublic::from(&config)))
}

/// DELETE /api/admin/smtp
pub async fn delete_smtp_config(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
) -> Result<Json<DeleteSmtpConfigResponse>, AppError> {
    if !auth.is_admin() {
        return Err(AppError::Forbidden(
            "Only admins can configure SMTP settings".into(),
        ));
    }

    state.db.delete_smtp_config().await?;

    tracing::info!("Admin '{}' removed SMTP configuration", auth.username);

    Ok(Json(DeleteSmtpConfigResponse { deleted: true }))
}

/// POST /api/admin/smtp/test
pub async fn send_test_email(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(req): Json<TestEmailRequest>,
) -> Result<Json<TestEmailResponse>, AppError> {
    if !auth.is_admin() {
        return Err(AppError::Forbidden(
            "Only admins can send test emails".into(),
        ));
    }

    if req.recipient.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Recipient email address is required".into(),
        ));
    }

    let config = state
        .db
        .get_smtp_config()
        .await?
        .ok_or_else(|| AppError::BadRequest("SMTP is not configured yet".into()))?;

    match email::send_test_email(&config, req.recipient.trim()).await {
        Ok(()) => {
            tracing::info!(
                "Admin '{}' sent test email to '{}'",
                auth.username,
                req.recipient
            );
            Ok(Json(TestEmailResponse {
                success: true,
                error: None,
            }))
        }
        Err(e) => {
            tracing::warn!("Test email to '{}' failed: {}", req.recipient, e);
            Ok(Json(TestEmailResponse {
                success: false,
                error: Some(format!("{}", e)),
            }))
        }
    }
}

/// GET /api/admin/alerts
pub async fn get_alert_config(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
) -> Result<Json<AlertConfig>, AppError> {
    if !auth.is_admin() {
        return Err(AppError::Forbidden(
            "Only admins can view alert settings".into(),
        ));
    }

    let config = state.db.get_alert_config().await?;
    Ok(Json(config))
}

/// PUT /api/admin/alerts
pub async fn save_alert_config(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(req): Json<SaveAlertConfigRequest>,
) -> Result<Json<AlertConfig>, AppError> {
    if !auth.is_admin() {
        return Err(AppError::Forbidden(
            "Only admins can configure alert settings".into(),
        ));
    }

    let config = AlertConfig {
        enabled: req.enabled,
        recipients: req
            .recipients
            .iter()
            .map(|r| r.trim().to_string())
            .filter(|r| !r.is_empty())
            .collect(),
        base_url: req
            .base_url
            .map(|u| u.trim().to_string())
            .filter(|u| !u.is_empty()),
        cooldown_secs: req.cooldown_secs,
        triggers: req.triggers,
    };

    state.db.save_alert_config(&config).await?;

    tracing::info!(
        "Admin '{}' updated alert configuration (enabled={}, recipients={})",
        auth.username,
        config.enabled,
        config.recipients.len(),
    );

    Ok(Json(config))
}

/// GET /api/servers/:id/alerts
pub async fn get_server_alerts(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<Uuid>,
) -> Result<Json<ServerAlertConfig>, AppError> {
    let server = state.db.require_server(server_id).await?;

    auth.require_permission(&state, &server).await?;

    let config = state
        .db
        .get_server_alert_config(&server_id)
        .await?
        .unwrap_or(ServerAlertConfig {
            server_id,
            muted: false,
        });
    Ok(Json(config))
}

/// PUT /api/servers/:id/alerts
pub async fn update_server_alerts(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<Uuid>,
    Json(req): Json<UpdateServerAlertRequest>,
) -> Result<Json<ServerAlertConfig>, AppError> {
    let server = state.db.require_server(server_id).await?;

    auth.require_level(&state, &server, PermissionLevel::Manager)
        .await?;

    let config = ServerAlertConfig {
        server_id,
        muted: req.muted,
    };

    state.db.save_server_alert_config(&config).await?;

    tracing::info!(
        "User '{}' {} alerts for server '{}' ({})",
        auth.username,
        if req.muted { "muted" } else { "unmuted" },
        server.config.name,
        server_id,
    );

    Ok(Json(config))
}
