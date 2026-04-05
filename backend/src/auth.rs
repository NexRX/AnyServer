use std::sync::{Arc, OnceLock};
use tracing::warn;

use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use base64::Engine;
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::AppError;
use crate::types::{EffectivePermission, GlobalCapability, PermissionLevel, Role, User};
use crate::AppState;

/// Prefix for API tokens — makes them trivially identifiable in leaked
/// credential scanners (GitHub secret scanning, trufflehog, etc.).
pub const API_TOKEN_PREFIX: &str = "as_";

pub fn hash_password(password: &str) -> Result<String, AppError> {
    use argon2::{
        password_hash::{rand_core::OsRng, SaltString},
        Argon2, PasswordHasher,
    };

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();

    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(format!("Password hashing failed: {}", e)))
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, AppError> {
    use argon2::{password_hash::PasswordHash, Argon2, PasswordVerifier};

    let parsed = PasswordHash::new(hash)
        .map_err(|e| AppError::Internal(format!("Invalid password hash: {}", e)))?;

    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

/// Resolution order:
///   1. `ANYSERVER_JWT_SECRET` env var (recommended for production).
///   2. A randomly generated 64-byte secret persisted to `<data_dir>/jwt_secret`.
///
/// Never falls back to a hard-coded string.
static JWT_SECRET: OnceLock<Vec<u8>> = OnceLock::new();

pub fn init_jwt_secret(data_dir: &std::path::Path) {
    let _ = JWT_SECRET.get_or_init(|| resolve_jwt_secret(data_dir));
}

fn resolve_jwt_secret(data_dir: &std::path::Path) -> Vec<u8> {
    if let Ok(val) = std::env::var("ANYSERVER_JWT_SECRET") {
        if val.len() < 32 {
            tracing::warn!(
                "ANYSERVER_JWT_SECRET is set but very short ({} bytes). \
                 Use at least 32 characters for adequate security.",
                val.len(),
            );
        }
        return val.into_bytes();
    }

    // Check if the data directory is world-writable (severe security risk).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(data_dir) {
            let mode = meta.permissions().mode() & 0o777;
            if mode & 0o002 != 0 {
                tracing::error!(
                    "DATA DIRECTORY {} is world-writable (mode {:04o})! \
                     This is a severe security risk. Fix with: chmod 700 {}",
                    data_dir.display(),
                    mode,
                    data_dir.display(),
                );
            }
        }
    }

    let secret_path = data_dir.join("jwt_secret");
    if let Ok(existing) = std::fs::read(&secret_path) {
        if existing.len() >= 32 {
            // Verify and repair file permissions on Unix.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = std::fs::metadata(&secret_path) {
                    let mode = meta.permissions().mode() & 0o777;
                    if mode != 0o600 {
                        tracing::warn!(
                            "JWT secret file {} has overly permissive mode {:04o} — \
                             tightening to 0600. Audit who may have read this file.",
                            secret_path.display(),
                            mode,
                        );
                        let _ = std::fs::set_permissions(
                            &secret_path,
                            std::fs::Permissions::from_mode(0o600),
                        );
                    }
                }
            }
            tracing::info!("Using persisted JWT secret from {}", secret_path.display());
            return existing;
        }
        tracing::warn!(
            "Persisted JWT secret at {} is too short ({} bytes), regenerating.",
            secret_path.display(),
            existing.len(),
        );
    }

    use rand::RngCore;
    let mut secret = vec![0u8; 64];
    rand::thread_rng().fill_bytes(&mut secret);

    if let Err(e) = std::fs::write(&secret_path, &secret) {
        tracing::error!(
            "Failed to persist JWT secret to {}: {}. \
             Tokens will NOT survive a restart. Set ANYSERVER_JWT_SECRET \
             to avoid this problem.",
            secret_path.display(),
            e,
        );
    } else {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&secret_path, std::fs::Permissions::from_mode(0o600));
        }
        tracing::warn!(
            "No ANYSERVER_JWT_SECRET set — generated and persisted a random secret to {}. \
             For production, set ANYSERVER_JWT_SECRET in your environment.",
            secret_path.display(),
        );
    }

    secret
}

pub(crate) fn jwt_secret() -> &'static [u8] {
    JWT_SECRET
        .get()
        .expect("JWT secret not initialised. Call auth::init_jwt_secret() at startup.")
}

const ACCESS_TOKEN_EXPIRY_MINUTES: i64 = 15;
const REFRESH_TOKEN_EXPIRY_DAYS: i64 = 7;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub username: String,
    pub role: Role,
    pub gen: i64,
    pub iat: i64,
    pub exp: i64,
    #[serde(default = "default_token_type")]
    pub typ: String,
}

fn default_token_type() -> String {
    "access".to_string()
}

fn create_jwt(user: &User, typ: &str, expiry: Duration) -> Result<String, AppError> {
    let now = Utc::now();
    let claims = Claims {
        sub: user.id.to_string(),
        username: user.username.clone(),
        role: user.role,
        gen: user.token_generation,
        iat: now.timestamp(),
        exp: (now + expiry).timestamp(),
        typ: typ.to_string(),
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret()),
    )
    .map_err(|e| AppError::Internal(format!("JWT creation failed: {}", e)))
}

pub fn create_access_token(user: &User) -> Result<String, AppError> {
    create_jwt(
        user,
        "access",
        Duration::minutes(ACCESS_TOKEN_EXPIRY_MINUTES),
    )
}

pub fn create_refresh_token(user: &User) -> Result<String, AppError> {
    create_jwt(user, "refresh", Duration::days(REFRESH_TOKEN_EXPIRY_DAYS))
}

pub fn validate_token(token: &str) -> Result<Claims, AppError> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(jwt_secret()),
        &Validation::default(),
    )
    .map_err(|e| AppError::Unauthorized(format!("Invalid token: {}", e)))?;

    Ok(data.claims)
}

pub async fn validate_token_with_generation(
    token: &str,
    state: &crate::AppState,
) -> Result<Claims, AppError> {
    let claims = validate_token(token)?;

    let user_id = uuid::Uuid::parse_str(&claims.sub)
        .map_err(|_| AppError::Unauthorized("Invalid user ID in token".into()))?;

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

    Ok(claims)
}

pub fn hash_refresh_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn generate_family_id() -> String {
    Uuid::new_v4().to_string()
}

pub fn refresh_token_expiry() -> chrono::DateTime<Utc> {
    Utc::now() + Duration::days(REFRESH_TOKEN_EXPIRY_DAYS)
}

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub username: String,
    pub role: Role,
}

impl AuthUser {
    /// Returns `true` if the JWT claims admin role. This is a snapshot from
    /// token-issuance time and may be stale — use [`Self::require_fresh_admin`]
    /// for security-critical operations.
    pub fn is_admin(&self) -> bool {
        self.role == Role::Admin
    }

    /// Check whether the user has the given global capability.
    ///
    /// Admins implicitly have all capabilities. For non-admin users this
    /// fetches the current `global_capabilities` list from the database
    /// (not the JWT) so revocations take effect immediately.
    ///
    /// Returns `Ok(())` on success or `Err(403)` with a descriptive message.
    pub async fn require_capability(
        &self,
        state: &AppState,
        cap: GlobalCapability,
    ) -> Result<(), AppError> {
        // Admins bypass all capability checks.
        if self.is_admin() {
            return Ok(());
        }

        let user = state
            .db
            .get_user(self.user_id)
            .await?
            .ok_or_else(|| AppError::Unauthorized("User no longer exists".into()))?;

        // Double-check: the DB role may have been promoted since the JWT was
        // issued — honour the latest role.
        if user.role == Role::Admin {
            return Ok(());
        }

        if user.has_capability(cap) {
            return Ok(());
        }

        Err(AppError::Forbidden(format!(
            "You do not have the {:?} capability. Contact an admin to request access.",
            cap,
        )))
    }

    /// Re-validate admin role against the database (not the JWT).
    /// Use for destructive or privilege-sensitive operations.
    pub async fn require_fresh_admin(&self, state: &AppState) -> Result<Role, AppError> {
        let user = state
            .db
            .get_user(self.user_id)
            .await?
            .ok_or_else(|| AppError::Unauthorized("User no longer exists".into()))?;

        if user.role != Role::Admin {
            warn!(
                "User '{}' (id={}) has admin role in JWT but is {:?} in database — rejecting",
                self.username, self.user_id, user.role,
            );
            return Err(AppError::Forbidden(
                "Your admin privileges have been revoked. Please log in again.".into(),
            ));
        }

        Ok(user.role)
    }

    pub async fn effective_permission(
        &self,
        state: &AppState,
        server: &crate::types::Server,
    ) -> Result<Option<EffectivePermission>, AppError> {
        if self.is_admin() {
            return Ok(Some(EffectivePermission {
                level: PermissionLevel::Owner,
                is_global_admin: true,
            }));
        }

        if server.owner_id == self.user_id {
            return Ok(Some(EffectivePermission {
                level: PermissionLevel::Owner,
                is_global_admin: false,
            }));
        }

        match state
            .db
            .get_effective_permission(&self.user_id, &server.id)
            .await?
        {
            Some(level) => Ok(Some(EffectivePermission {
                level,
                is_global_admin: false,
            })),
            None => Ok(None),
        }
    }

    pub async fn require_permission(
        &self,
        state: &AppState,
        server: &crate::types::Server,
    ) -> Result<EffectivePermission, AppError> {
        self.effective_permission(state, server)
            .await?
            .ok_or_else(|| {
                AppError::Forbidden(format!(
                    "You do not have access to server '{}'",
                    server.config.name
                ))
            })
    }

    pub async fn require_level(
        &self,
        state: &AppState,
        server: &crate::types::Server,
        required: PermissionLevel,
    ) -> Result<EffectivePermission, AppError> {
        let perm = self.require_permission(state, server).await?;
        if perm.level >= required {
            Ok(perm)
        } else {
            Err(AppError::Forbidden(format!(
                "Insufficient permission on server '{}': you have {:?}, need {:?}",
                server.config.name, perm.level, required,
            )))
        }
    }

    /// Like [`Self::require_level`], but re-validates admin role against the DB
    /// when the permission came from the global admin shortcut. Use for
    /// destructive or irreversible operations.
    pub async fn require_level_verified(
        &self,
        state: &AppState,
        server: &crate::types::Server,
        required: PermissionLevel,
    ) -> Result<EffectivePermission, AppError> {
        let perm = self.require_level(state, server, required).await?;

        if perm.is_global_admin {
            self.require_fresh_admin(state).await?;
        }

        Ok(perm)
    }
}

#[async_trait]
impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        extract_auth_user(parts, state).await
    }
}

// ─── API Token Helpers ───────────────────────────────────────

/// Generate a new random API token with the `as_` prefix.
/// Returns the raw token string (to be shown to the user once).
pub fn generate_api_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!(
        "{}{}",
        API_TOKEN_PREFIX,
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    )
}

/// Hash an API token for storage.  Only the hash is persisted; the raw
/// token is shown to the user exactly once at creation time.
pub fn hash_api_token(raw_token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw_token.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Authenticate a request using an `as_`-prefixed API token.
/// Looks up the token hash in the database, validates expiry and
/// revocation, and returns the associated `AuthUser`.
async fn authenticate_api_token(
    token: &str,
    state: &Arc<crate::AppState>,
) -> Result<AuthUser, AppError> {
    let token_hash = hash_api_token(token);

    let api_token = state
        .db
        .find_api_token_by_hash(&token_hash)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Invalid API token".into()))?;

    if api_token.revoked {
        return Err(AppError::Unauthorized("API token has been revoked".into()));
    }
    if api_token.is_expired() {
        return Err(AppError::Unauthorized("API token has expired".into()));
    }

    // Look up the owning user to get current role.
    let user = state
        .db
        .get_user(api_token.user_id)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Token owner no longer exists".into()))?;

    // Update last_used_at (fire-and-forget — don't fail the request).
    let db = state.db.clone();
    let tid = api_token.id;
    tokio::spawn(async move {
        let _ = db.update_api_token_last_used(&tid).await;
    });

    Ok(AuthUser {
        user_id: user.id,
        username: user.username,
        role: user.role,
    })
}

async fn extract_auth_user(
    parts: &Parts,
    state: &Arc<crate::AppState>,
) -> Result<AuthUser, AppError> {
    let header = parts
        .headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("Missing Authorization header".into()))?;

    let token = header.strip_prefix("Bearer ").ok_or_else(|| {
        AppError::Unauthorized("Authorization header must use Bearer scheme".into())
    })?;

    // API token path: tokens with the `as_` prefix are long-lived API tokens
    // looked up by hash in the database, not JWTs.
    if token.starts_with(API_TOKEN_PREFIX) {
        return authenticate_api_token(token, state).await;
    }

    // JWT path: existing validate_token_with_generation logic.
    let claims = validate_token_with_generation(token, state).await?;

    if claims.typ != "access" {
        return Err(AppError::Unauthorized(
            "Invalid token type. Use an access token for API requests.".into(),
        ));
    }

    let user_id = Uuid::parse_str(&claims.sub)
        .map_err(|_| AppError::Unauthorized("Invalid user ID in token".into()))?;

    Ok(AuthUser {
        user_id,
        username: claims.username,
        role: claims.role,
    })
}

/// Does NOT reject unauthenticated requests — resolves to `None` instead.
#[derive(Debug, Clone)]
pub struct MaybeAuthUser(pub Option<AuthUser>);

#[async_trait]
impl FromRequestParts<Arc<AppState>> for MaybeAuthUser {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        Ok(match extract_auth_user(parts, state).await {
            Ok(user) => MaybeAuthUser(Some(user)),
            Err(_) => MaybeAuthUser(None),
        })
    }
}

pub fn validate_username(username: &str) -> Result<(), AppError> {
    let username = username.trim();
    if username.len() < 3 {
        return Err(AppError::BadRequest(
            "Username must be at least 3 characters".into(),
        ));
    }
    if username.len() > 32 {
        return Err(AppError::BadRequest(
            "Username must be at most 32 characters".into(),
        ));
    }
    if !username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(AppError::BadRequest(
            "Username may only contain letters, digits, underscores, and hyphens".into(),
        ));
    }
    Ok(())
}

pub fn validate_password(password: &str) -> Result<(), AppError> {
    if password.len() < 8 {
        return Err(AppError::BadRequest(
            "Password must be at least 8 characters".into(),
        ));
    }
    if password.len() > 256 {
        return Err(AppError::BadRequest(
            "Password must be at most 256 characters".into(),
        ));
    }

    let has_lower = password.chars().any(|c| c.is_ascii_lowercase());
    let has_upper = password.chars().any(|c| c.is_ascii_uppercase());
    let has_digit = password.chars().any(|c| c.is_ascii_digit());

    if !has_lower || !has_upper || !has_digit {
        return Err(AppError::BadRequest(
            "Password must contain at least one lowercase letter, one uppercase letter, and one digit".into(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[cfg(unix)]
    #[test]
    fn test_jwt_secret_file_created_with_0600_permissions() {
        let tmp = TempDir::new().unwrap();
        let data_dir = tmp.path();

        // No existing secret, no env var — should generate one
        let secret = resolve_jwt_secret(data_dir);
        assert!(secret.len() >= 32);

        let secret_path = data_dir.join("jwt_secret");
        assert!(secret_path.exists());

        let meta = std::fs::metadata(&secret_path).unwrap();
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "Generated secret file should have mode 0600, got {:04o}",
            mode
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_jwt_secret_tightens_overly_permissive_file() {
        let tmp = TempDir::new().unwrap();
        let data_dir = tmp.path();
        let secret_path = data_dir.join("jwt_secret");

        // Write a 64-byte secret with world-readable permissions
        let mut f = std::fs::File::create(&secret_path).unwrap();
        f.write_all(&[0xABu8; 64]).unwrap();
        drop(f);
        std::fs::set_permissions(&secret_path, std::fs::Permissions::from_mode(0o644)).unwrap();

        // Verify it starts at 0644
        let meta = std::fs::metadata(&secret_path).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o644);

        // resolve_jwt_secret should read it and tighten permissions
        let secret = resolve_jwt_secret(data_dir);
        assert_eq!(secret.len(), 64);

        let meta_after = std::fs::metadata(&secret_path).unwrap();
        let mode_after = meta_after.permissions().mode() & 0o777;
        assert_eq!(
            mode_after, 0o600,
            "Should have tightened to 0600, got {:04o}",
            mode_after
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_jwt_secret_correct_permissions_not_changed() {
        let tmp = TempDir::new().unwrap();
        let data_dir = tmp.path();
        let secret_path = data_dir.join("jwt_secret");

        // Write a 64-byte secret with correct permissions
        let mut f = std::fs::File::create(&secret_path).unwrap();
        f.write_all(&[0xCDu8; 64]).unwrap();
        drop(f);
        std::fs::set_permissions(&secret_path, std::fs::Permissions::from_mode(0o600)).unwrap();

        let secret = resolve_jwt_secret(data_dir);
        assert_eq!(secret.len(), 64);

        // Permissions should still be 0600
        let meta = std::fs::metadata(&secret_path).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn test_jwt_secret_short_file_triggers_regeneration() {
        let tmp = TempDir::new().unwrap();
        let data_dir = tmp.path();
        let secret_path = data_dir.join("jwt_secret");

        // Write a too-short secret (< 32 bytes)
        std::fs::write(&secret_path, b"too-short").unwrap();

        let secret = resolve_jwt_secret(data_dir);
        // Should have regenerated a proper-length secret
        assert!(
            secret.len() >= 32,
            "Regenerated secret should be >= 32 bytes, got {}",
            secret.len()
        );

        // The file should have been overwritten with the new secret
        let on_disk = std::fs::read(&secret_path).unwrap();
        assert!(on_disk.len() >= 32);
    }

    #[test]
    fn test_jwt_secret_uses_existing_valid_file() {
        let tmp = TempDir::new().unwrap();
        let data_dir = tmp.path();
        let secret_path = data_dir.join("jwt_secret");

        let expected: Vec<u8> = (0..64).collect();
        std::fs::write(&secret_path, &expected).unwrap();

        let secret = resolve_jwt_secret(data_dir);
        assert_eq!(
            secret, expected,
            "Should return the existing secret unchanged"
        );
    }
}
