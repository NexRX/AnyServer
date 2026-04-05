use std::sync::{Arc, OnceLock};

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use axum_extra::extract::cookie::{Cookie, SameSite};
use chrono::Utc;
use uuid::Uuid;

use crate::auth::{
    create_access_token, create_refresh_token, generate_family_id, hash_password,
    hash_refresh_token, refresh_token_expiry, validate_password, validate_token, validate_username,
    verify_password, AuthUser,
};
use crate::error::AppError;
use crate::types::*;
use crate::AppState;

const REFRESH_COOKIE_NAME: &str = "anyserver_refresh";

/// Resolve whether the `Secure` flag should be set on cookies.
///
/// Checks `ANYSERVER_COOKIE_SECURE` env var:
/// - `"true"` — always set `Secure` (for HTTPS deployments)
/// - `"false"` — never set `Secure` (for plain HTTP / LAN setups)
/// - `"auto"` or unset — set `Secure` when `bundle-frontend` is enabled,
///   omit it in dev mode.
///
/// The result is cached after the first call.
fn cookie_secure() -> bool {
    static SECURE: OnceLock<bool> = OnceLock::new();

    *SECURE.get_or_init(|| {
        let value = std::env::var("ANYSERVER_COOKIE_SECURE").unwrap_or_else(|_| "auto".to_string());

        match value.to_lowercase().as_str() {
            "true" | "1" | "yes" => {
                tracing::info!("Cookie Secure flag: enabled (ANYSERVER_COOKIE_SECURE=true)");
                true
            }
            "false" | "0" | "no" => {
                tracing::warn!(
                    "Cookie Secure flag: DISABLED via ANYSERVER_COOKIE_SECURE=false — \
                     refresh cookies will be sent over plain HTTP. \
                     Only use this for trusted LAN / development environments."
                );
                false
            }
            _ => {
                // "auto" or any unrecognised value
                #[cfg(feature = "bundle-frontend")]
                {
                    tracing::info!("Cookie Secure flag: enabled (auto — bundle-frontend build)");
                    true
                }
                #[cfg(not(feature = "bundle-frontend"))]
                {
                    tracing::warn!(
                        "Cookie Secure flag: DISABLED (auto — dev mode without bundle-frontend). \
                         Set ANYSERVER_COOKIE_SECURE=true for production HTTPS deployments."
                    );
                    false
                }
            }
        }
    })
}

struct IssuedTokens {
    access_token: String,
    refresh_token: String,
}

async fn create_and_store_tokens(
    state: &AppState,
    user: &User,
    family_id: Option<&str>,
) -> Result<IssuedTokens, AppError> {
    let access_token = create_access_token(user)?;
    let refresh_token = create_refresh_token(user)?;

    let owned_family = family_id
        .map(String::from)
        .unwrap_or_else(generate_family_id);
    let token_hash = hash_refresh_token(&refresh_token);
    state
        .db
        .insert_refresh_token(
            &Uuid::new_v4().to_string(),
            user.id,
            &owned_family,
            &token_hash,
            refresh_token_expiry(),
        )
        .await?;

    Ok(IssuedTokens {
        access_token,
        refresh_token,
    })
}

pub(crate) async fn issue_tokens_response(
    state: &AppState,
    user: &User,
    family_id: Option<&str>,
) -> Result<Response, AppError> {
    let tokens = create_and_store_tokens(state, user, family_id).await?;

    let response = AuthResponse {
        token: tokens.access_token,
        user: user.clone().into(),
    };

    Ok(create_response_with_refresh_cookie(
        response,
        &tokens.refresh_token,
    ))
}

/// POST /api/auth/setup
pub async fn setup(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SetupRequest>,
) -> Result<Response, AppError> {
    let settings = state.db.get_settings().await?;
    if settings.setup_complete {
        return Err(AppError::Conflict(
            "Setup has already been completed. Please log in.".into(),
        ));
    }

    if state.db.user_count().await? > 0 {
        return Err(AppError::Conflict(
            "An admin user already exists. Please log in.".into(),
        ));
    }

    let username = req.username.trim().to_lowercase();
    validate_username(&username)?;
    validate_password(&req.password)?;

    let password_hash = hash_password(&req.password)?;

    let user = User {
        id: Uuid::new_v4(),
        username: username.clone(),
        password_hash,
        role: Role::Admin,
        created_at: Utc::now(),
        token_generation: 0,
        global_capabilities: vec![],
    };

    state.db.insert_user(&user).await?;

    let new_settings = AppSettings {
        setup_complete: true,
        registration_enabled: false,
        allow_run_commands: false,
        run_command_sandbox: "auto".to_string(),
        run_command_default_timeout_secs: 300,
        run_command_use_namespaces: true,
    };
    state.db.save_settings(&new_settings).await?;

    tracing::info!(
        "Setup complete — admin user '{}' created (id={})",
        username,
        user.id
    );

    issue_tokens_response(&state, &user, None).await
}

/// POST /api/auth/login
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<Response, AppError> {
    let username = req.username.trim().to_lowercase();

    // Check rate limit BEFORE credential validation to prevent timing oracles.
    if let Err(retry_after_secs) = state.login_attempt_tracker.check_allowed(&username) {
        tracing::warn!(
            "Login attempt for '{}' rejected: in cooldown for {} more seconds",
            username,
            retry_after_secs
        );
        return Err(AppError::TooManyRequestsWithRetry {
            message: format!(
                "Too many failed login attempts. Please try again in {} seconds.",
                retry_after_secs
            ),
            retry_after_secs,
        });
    }

    let user = state
        .db
        .get_user_by_username(&username)
        .await?
        .ok_or_else(|| {
            state.login_attempt_tracker.record_failure(&username);
            AppError::Unauthorized("Invalid username or password".into())
        })?;

    let valid = verify_password(&req.password, &user.password_hash)?;
    if !valid {
        state.login_attempt_tracker.record_failure(&username);
        tracing::warn!("Failed login attempt for user '{}'", username);
        return Err(AppError::Unauthorized(
            "Invalid username or password".into(),
        ));
    }

    state.login_attempt_tracker.record_success(&username);

    tracing::info!("User '{}' logged in (id={})", user.username, user.id);

    issue_tokens_response(&state, &user, None).await
}

/// POST /api/auth/register
pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterRequest>,
) -> Result<Response, AppError> {
    let settings = state.db.get_settings().await?;

    if !settings.setup_complete {
        return Err(AppError::BadRequest(
            "Initial setup has not been completed yet. Please use the setup page.".into(),
        ));
    }

    if !settings.registration_enabled {
        return Err(AppError::Forbidden(
            "User registration is currently disabled. Ask an admin to create your account.".into(),
        ));
    }

    let username = req.username.trim().to_lowercase();
    validate_username(&username)?;
    validate_password(&req.password)?;

    if state.db.username_exists(&username).await? {
        return Err(AppError::Conflict(format!(
            "Username '{}' is already taken",
            username
        )));
    }

    let password_hash = hash_password(&req.password)?;

    let user = User {
        id: Uuid::new_v4(),
        username: username.clone(),
        password_hash,
        role: Role::User,
        created_at: Utc::now(),
        token_generation: 0,
        global_capabilities: vec![],
    };

    state.db.insert_user(&user).await?;

    tracing::info!("User '{}' registered (id={})", username, user.id);

    issue_tokens_response(&state, &user, None).await
}

/// Grace period (in seconds) for refresh-token reuse detection.
///
/// When a browser hard-refreshes, the in-memory access token is lost and the
/// frontend calls `/api/auth/refresh`.  The backend rotates the refresh token
/// (revokes the old one, issues a new one via `Set-Cookie`).  If the user
/// hard-refreshes *again* before the browser has stored the new cookie, the
/// old (now-revoked) token is re-sent.  Without a grace period this triggers
/// reuse detection which revokes the entire token family, logging the user
/// out.
///
/// To avoid this, we allow a short window after revocation during which the
/// old token is still accepted.  In that case we look up the current active
/// successor in the same family and perform a normal rotation from *that*
/// token instead of nuking the family.
const REFRESH_REUSE_GRACE_SECONDS: i64 = 30;

/// POST /api/auth/refresh
pub async fn refresh(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    cookie_jar: axum_extra::extract::CookieJar,
) -> Result<Response, AppError> {
    // CSRF protection: require a custom header that cannot be sent cross-origin
    // without a CORS preflight. Since our CORS config does not allowlist
    // arbitrary origins, this effectively blocks cross-site request forgery.
    if headers.get("x-requested-with").is_none() {
        return Err(AppError::Forbidden(
            "Missing X-Requested-With header (CSRF protection)".into(),
        ));
    }

    let refresh_token = cookie_jar
        .get(REFRESH_COOKIE_NAME)
        .ok_or_else(|| AppError::Unauthorized("No refresh token provided".into()))?
        .value();

    let claims = validate_token(refresh_token)?;

    if claims.typ != "refresh" {
        return Err(AppError::Unauthorized(
            "Invalid token type. Expected refresh token.".into(),
        ));
    }

    let user_id = Uuid::parse_str(&claims.sub)
        .map_err(|_| AppError::Unauthorized("Invalid user ID in token".into()))?;

    let token_hash = hash_refresh_token(refresh_token);
    let stored_token = state
        .db
        .get_refresh_token(&token_hash)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Refresh token not found or revoked".into()))?;

    // --- Reuse detection with grace period ---
    //
    // `revoked` is 0 when active, or a unix-timestamp (seconds) recording
    // when the token was revoked.  A non-zero value that falls within the
    // grace window is treated as a benign hard-refresh race rather than
    // token theft.
    let active_family_token = if stored_token.revoked != 0 {
        let revoked_at = stored_token.revoked; // unix seconds
        let seconds_since_revoked = Utc::now().timestamp() - revoked_at;

        if seconds_since_revoked <= REFRESH_REUSE_GRACE_SECONDS {
            // Within grace period — look for the successor token in the
            // same family so we can rotate from it instead of killing
            // the whole family.
            tracing::info!(
                "Revoked refresh token re-presented within grace period ({seconds_since_revoked}s) \
                 for user '{}' (family={}). Treating as hard-refresh race.",
                user_id,
                stored_token.family_id
            );

            let successor = state
                .db
                .get_latest_active_family_token(&stored_token.family_id)
                .await?;

            match successor {
                Some(tok) => tok,
                None => {
                    // No active successor — the whole family was already
                    // revoked through some other path (logout, etc.).
                    return Err(AppError::Unauthorized(
                        "Refresh token has been revoked".into(),
                    ));
                }
            }
        } else {
            // Outside the grace window — genuine reuse detection.
            tracing::warn!(
                "Refresh token reuse detected for user '{}' (family={}). \
                 Token was revoked {seconds_since_revoked}s ago (>{REFRESH_REUSE_GRACE_SECONDS}s grace). \
                 Revoking entire family.",
                user_id,
                stored_token.family_id
            );
            let revoked_count = state
                .db
                .revoke_token_family(&stored_token.family_id)
                .await?;
            tracing::warn!(
                "Revoked {} tokens in family '{}' due to reuse detection",
                revoked_count,
                stored_token.family_id
            );
            return Err(AppError::Unauthorized(
                "Refresh token has been revoked".into(),
            ));
        }
    } else {
        // Token is still active — normal path.
        stored_token
    };

    let expires_at = chrono::DateTime::parse_from_rfc3339(&active_family_token.expires_at)
        .map_err(|_| AppError::Internal("Invalid expiry timestamp".into()))?;
    if expires_at.with_timezone(&Utc) < Utc::now() {
        return Err(AppError::Unauthorized("Refresh token has expired".into()));
    }

    let user = state
        .db
        .get_user(user_id)
        .await?
        .ok_or_else(|| AppError::Unauthorized("User no longer exists".into()))?;

    if claims.gen != user.token_generation {
        return Err(AppError::Unauthorized(
            "Token has been revoked. Please log in again.".into(),
        ));
    }

    let new_access_token = create_access_token(&user)?;
    let new_refresh_token = create_refresh_token(&user)?;
    let new_token_hash = hash_refresh_token(&new_refresh_token);

    state
        .db
        .revoke_refresh_token(&active_family_token.token_hash)
        .await?;
    state
        .db
        .insert_refresh_token(
            &Uuid::new_v4().to_string(),
            user.id,
            &active_family_token.family_id,
            &new_token_hash,
            refresh_token_expiry(),
        )
        .await?;

    tracing::debug!(
        "Refreshed access token for user '{}' (id={})",
        user.username,
        user.id
    );

    let response = RefreshResponse {
        token: new_access_token,
    };

    Ok(create_response_with_refresh_cookie(
        response,
        &new_refresh_token,
    ))
}

/// POST /api/auth/logout
pub async fn logout(
    State(state): State<Arc<AppState>>,
    cookie_jar: axum_extra::extract::CookieJar,
) -> Result<Response, AppError> {
    if let Some(cookie) = cookie_jar.get(REFRESH_COOKIE_NAME) {
        let token_hash = hash_refresh_token(cookie.value());
        let _ = state.db.revoke_refresh_token(&token_hash).await;
    }

    let mut cookie = Cookie::new(REFRESH_COOKIE_NAME, "");
    cookie.set_http_only(true);
    cookie.set_secure(cookie_secure());
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/api/auth");
    cookie.set_max_age(time::Duration::seconds(-1)); // Expire immediately

    let response = (StatusCode::OK, Json(serde_json::json!({"success": true})));

    Ok(([(header::SET_COOKIE, cookie.to_string())], response).into_response())
}

/// POST /api/auth/logout-everywhere
pub async fn logout_everywhere(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Response, AppError> {
    let new_generation = state.db.increment_token_generation(auth.user_id).await?;
    let revoked_count = state.db.revoke_all_refresh_tokens(auth.user_id).await?;

    let user = state
        .db
        .get_user(auth.user_id)
        .await?
        .ok_or_else(|| AppError::Unauthorized("User no longer exists".into()))?;

    tracing::info!(
        "User '{}' logged out everywhere (revoked {} refresh tokens, generation now {})",
        auth.username,
        revoked_count,
        new_generation
    );

    let tokens = create_and_store_tokens(&state, &user, None).await?;

    let response = LogoutEverywhereResponse {
        revoked_count,
        token: tokens.access_token,
    };

    Ok(create_response_with_refresh_cookie(
        response,
        &tokens.refresh_token,
    ))
}

/// GET /api/auth/me
pub async fn me(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<MeResponse>, AppError> {
    let user = state
        .db
        .get_user(auth.user_id)
        .await?
        .ok_or_else(|| AppError::Unauthorized("User no longer exists".into()))?;

    let settings = state.db.get_settings().await?;

    Ok(Json(MeResponse {
        user: user.into(),
        settings,
    }))
}

/// GET /api/auth/status — no auth required.
pub async fn status(State(state): State<Arc<AppState>>) -> Result<Json<AppSettings>, AppError> {
    let settings = state.db.get_settings().await?;
    Ok(Json(settings))
}

/// POST /api/auth/change-password
pub async fn change_password(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<Response, AppError> {
    validate_password(&req.new_password)?;

    let mut user = state
        .db
        .get_user(auth.user_id)
        .await?
        .ok_or_else(|| AppError::Unauthorized("User no longer exists".into()))?;

    let valid = verify_password(&req.current_password, &user.password_hash)?;
    if !valid {
        return Err(AppError::Unauthorized(
            "Current password is incorrect".into(),
        ));
    }

    user.password_hash = hash_password(&req.new_password)?;
    user.token_generation = state.db.increment_token_generation(auth.user_id).await?;
    state.db.update_user(&user).await?;
    state.db.revoke_all_refresh_tokens(auth.user_id).await?;

    tracing::info!(
        "User '{}' changed their password (generation now {})",
        user.username,
        user.token_generation
    );

    let tokens = create_and_store_tokens(&state, &user, None).await?;

    let response = ChangePasswordResponse {
        changed: true,
        token: tokens.access_token,
    };

    Ok(create_response_with_refresh_cookie(
        response,
        &tokens.refresh_token,
    ))
}

/// PUT /api/auth/settings
pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<UpdateSettingsRequest>,
) -> Result<Json<AppSettings>, AppError> {
    auth.require_fresh_admin(&state).await?;

    let mut settings = state.db.get_settings().await?;
    settings.registration_enabled = req.registration_enabled;
    settings.allow_run_commands = req.allow_run_commands;
    settings.run_command_sandbox = req.run_command_sandbox.clone();
    settings.run_command_default_timeout_secs = req.run_command_default_timeout_secs;
    settings.run_command_use_namespaces = req.run_command_use_namespaces;
    state.db.save_settings(&settings).await?;

    tracing::info!(
        "Admin '{}' updated settings: registration_enabled={}, allow_run_commands={}, run_command_sandbox={}, run_command_default_timeout_secs={}, run_command_use_namespaces={}",
        auth.username,
        settings.registration_enabled,
        settings.allow_run_commands,
        settings.run_command_sandbox,
        settings.run_command_default_timeout_secs,
        settings.run_command_use_namespaces
    );

    Ok(Json(settings))
}

/// GET /api/auth/sessions
pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    cookie_jar: axum_extra::extract::CookieJar,
) -> Result<Json<SessionListResponse>, AppError> {
    let sessions = state.db.list_user_sessions(auth.user_id).await?;
    let current_token_hash = cookie_jar
        .get(REFRESH_COOKIE_NAME)
        .map(|cookie| hash_refresh_token(cookie.value()));

    let session_infos: Vec<SessionInfo> = sessions
        .into_iter()
        .map(|session| {
            let created_at = chrono::DateTime::parse_from_rfc3339(&session.created_at)
                .unwrap_or_else(|_| chrono::Utc::now().into())
                .with_timezone(&Utc);
            let expires_at = chrono::DateTime::parse_from_rfc3339(&session.expires_at)
                .unwrap_or_else(|_| chrono::Utc::now().into())
                .with_timezone(&Utc);

            let is_current = current_token_hash
                .as_ref()
                .map(|hash| hash == &session.token_hash)
                .unwrap_or(false);

            SessionInfo {
                id: session.id,
                family_id: session.family_id,
                created_at,
                expires_at,
                is_current,
            }
        })
        .collect();

    Ok(Json(SessionListResponse {
        sessions: session_infos,
    }))
}

/// POST /api/auth/sessions/revoke
pub async fn revoke_session(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<RevokeSessionRequest>,
) -> Result<Json<RevokeSessionResponse>, AppError> {
    let revoked_count = state
        .db
        .revoke_session_by_family(&req.family_id, auth.user_id)
        .await?;

    if revoked_count == 0 {
        return Err(AppError::NotFound(
            "Session not found or already revoked".into(),
        ));
    }

    tracing::info!(
        "User '{}' revoked session family '{}' ({} tokens)",
        auth.username,
        req.family_id,
        revoked_count
    );

    Ok(Json(RevokeSessionResponse { revoked_count }))
}

/// POST /api/auth/ws-ticket
pub async fn ws_ticket(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<Option<WsTicketRequest>>,
) -> Result<Json<WsTicketResponse>, AppError> {
    let scope = req.and_then(|r| r.scope);

    let ticket = state
        .ws_ticket_store
        .mint(auth.user_id, auth.role, scope.clone())
        .ok_or_else(|| {
            AppError::TooManyRequests(
                "Too many outstanding WebSocket tickets. Please try again shortly.".into(),
            )
        })?;

    tracing::debug!(
        "Minted WebSocket ticket for user '{}' (scope: {:?})",
        auth.username,
        scope
    );

    Ok(Json(WsTicketResponse { ticket }))
}

// ─── API Token Management ────────────────────────────────────

/// POST /api/auth/api-tokens — create a new long-lived API token.
pub async fn create_api_token(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<crate::types::CreateApiTokenRequest>,
) -> Result<Json<crate::types::CreateApiTokenResponse>, AppError> {
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::BadRequest("Token name is required".into()));
    }
    if name.len() > 128 {
        return Err(AppError::BadRequest(
            "Token name must be 128 characters or fewer".into(),
        ));
    }

    // Validate scope access value.
    if req.scope.access != "full" && req.scope.access != "read_only" {
        return Err(AppError::BadRequest(
            "scope.access must be \"full\" or \"read_only\"".into(),
        ));
    }

    let raw_token = crate::auth::generate_api_token();
    let token_hash = crate::auth::hash_api_token(&raw_token);

    let token = crate::types::ApiToken {
        id: Uuid::new_v4(),
        user_id: auth.user_id,
        name: name.clone(),
        token_hash,
        scope: req.scope.clone(),
        created_at: chrono::Utc::now(),
        expires_at: req.expires_at,
        last_used_at: None,
        revoked: false,
    };

    state.db.create_api_token(&token).await?;

    tracing::info!(
        "User '{}' created API token '{}' (id={}, scope={:?})",
        auth.username,
        name,
        token.id,
        req.scope.access,
    );

    Ok(Json(crate::types::CreateApiTokenResponse {
        id: token.id,
        name,
        token: raw_token,
        scope: req.scope,
        created_at: token.created_at,
        expires_at: token.expires_at,
    }))
}

/// GET /api/auth/api-tokens — list the current user's API tokens.
pub async fn list_api_tokens(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<crate::types::ListApiTokensResponse>, AppError> {
    let tokens = state.db.list_api_tokens_for_user(&auth.user_id).await?;
    let infos: Vec<crate::types::ApiTokenInfo> = tokens.iter().map(|t| t.into()).collect();
    Ok(Json(crate::types::ListApiTokensResponse { tokens: infos }))
}

/// DELETE /api/auth/api-tokens/:id — revoke an API token.
pub async fn revoke_api_token(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(token_id): Path<Uuid>,
) -> Result<Json<crate::types::RevokeApiTokenResponse>, AppError> {
    let revoked = state.db.revoke_api_token(&token_id, &auth.user_id).await?;

    if revoked {
        tracing::info!("User '{}' revoked API token id={}", auth.username, token_id,);
    }

    Ok(Json(crate::types::RevokeApiTokenResponse { revoked }))
}

fn create_response_with_refresh_cookie<T: serde::Serialize>(
    body: T,
    refresh_token: &str,
) -> Response {
    let mut cookie = Cookie::new(REFRESH_COOKIE_NAME, refresh_token);
    cookie.set_http_only(true);
    cookie.set_secure(cookie_secure());
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/api/auth");
    cookie.set_max_age(time::Duration::days(7));

    ([(header::SET_COOKIE, cookie.to_string())], Json(body)).into_response()
}
