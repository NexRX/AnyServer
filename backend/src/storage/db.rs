use std::collections::HashMap;
use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{FromRow, Pool, Sqlite};
use uuid::Uuid;

use crate::error::AppError;
use crate::types::{
    AlertConfig, ApiToken, AppSettings, InviteCode, InvitePermissionGrant, PermissionLevel,
    SandboxProfile, Server, ServerAlertConfig, ServerConfig, ServerPermission, ServerTemplate,
    SmtpConfig, User,
};

// ──────────────────────────────────────────────
//  Helper Functions
// ──────────────────────────────────────────────

/// Escape SQL LIKE wildcard characters so they match literally.
///
/// Replaces `\` → `\\`, `%` → `\%`, `_` → `\_`.  The resulting string
/// must be used with `LIKE ? ESCAPE '\'` in the SQL query.
fn escape_like(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// Build a `%…%` search pattern with LIKE wildcards properly escaped.
fn make_search_pattern(search: Option<&str>) -> Option<String> {
    search.map(|s| format!("%{}%", escape_like(s)))
}

/// Build an `ORDER BY` clause for server list queries.
fn server_order_clause(sort: &str, order: &str) -> &'static str {
    match sort {
        "created_at" => match order {
            "desc" => "s.created_at DESC",
            _ => "s.created_at ASC",
        },
        _ => match order {
            "desc" => "json_extract(s.config, '$.name') DESC",
            _ => "json_extract(s.config, '$.name') ASC",
        },
    }
}

/// Trait for converting database rows to domain types
trait IntoDomain {
    type Output;
    fn into_domain(self) -> Result<Self::Output, AppError>;
}

/// Parse UUID from string
fn parse_uuid(s: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(s).map_err(|e| AppError::Internal(format!("Invalid UUID: {}", e)))
}

/// Parse RFC3339 timestamp to UTC DateTime
fn parse_timestamp(s: &str) -> Result<chrono::DateTime<chrono::Utc>, AppError> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .map_err(|e| AppError::Internal(format!("Invalid timestamp: {}", e)))
}

/// Parse optional timestamp
fn parse_optional_timestamp(
    s: &Option<String>,
) -> Result<Option<chrono::DateTime<chrono::Utc>>, AppError> {
    s.as_ref().map(|s| parse_timestamp(s)).transpose()
}

/// Deserialize JSON string
fn deserialize_json<T: serde::de::DeserializeOwned>(s: &str) -> Result<T, AppError> {
    serde_json::from_str(s).map_err(|e| AppError::Internal(format!("Deserialization error: {}", e)))
}

/// Serialize to JSON string
fn serialize_json<T: serde::Serialize>(value: &T) -> Result<String, AppError> {
    serde_json::to_string(value)
        .map_err(|e| AppError::Internal(format!("Serialization error: {}", e)))
}

/// Helper to format boolean as SQLite integer (1 or 0)
fn bool_to_int(b: bool) -> i64 {
    i64::from(b)
}

// ──────────────────────────────────────────────
//  Row Structs (map directly to database tables)
// ──────────────────────────────────────────────

#[derive(FromRow)]
struct ServerRow {
    id: String,
    owner_id: String,
    config: String,
    created_at: String,
    updated_at: String,
    parameter_values: String,
    installed: i64,
    installed_at: Option<String>,
    updated_via_pipeline_at: Option<String>,
    installed_version: Option<String>,
    source_template_id: Option<String>,
}

impl IntoDomain for ServerRow {
    type Output = Server;

    fn into_domain(self) -> Result<Server, AppError> {
        Ok(Server {
            id: parse_uuid(&self.id)?,
            owner_id: parse_uuid(&self.owner_id)?,
            config: deserialize_json(&self.config)?,
            created_at: parse_timestamp(&self.created_at)?,
            updated_at: parse_timestamp(&self.updated_at)?,
            parameter_values: deserialize_json(&self.parameter_values)?,
            installed: self.installed != 0,
            installed_at: parse_optional_timestamp(&self.installed_at)?,
            updated_via_pipeline_at: parse_optional_timestamp(&self.updated_via_pipeline_at)?,
            installed_version: self.installed_version,
            source_template_id: self
                .source_template_id
                .as_ref()
                .map(|s| parse_uuid(s))
                .transpose()?,
        })
    }
}

#[derive(FromRow)]
struct UserRow {
    id: String,
    username: String,
    password_hash: String,
    role: String,
    created_at: String,
    token_generation: i64,
    global_capabilities: String,
}

impl IntoDomain for UserRow {
    type Output = User;

    fn into_domain(self) -> Result<User, AppError> {
        Ok(User {
            id: parse_uuid(&self.id)?,
            username: self.username,
            password_hash: self.password_hash,
            role: parse_role(&self.role)?,
            created_at: parse_timestamp(&self.created_at)?,
            token_generation: self.token_generation,
            global_capabilities: deserialize_json(&self.global_capabilities)?,
        })
    }
}

fn parse_role(s: &str) -> Result<crate::types::Role, AppError> {
    match s {
        "admin" => Ok(crate::types::Role::Admin),
        "user" => Ok(crate::types::Role::User),
        _ => Err(AppError::Internal(format!("Invalid role: {}", s))),
    }
}

#[derive(FromRow)]
struct TemplateRow {
    id: String,
    name: String,
    description: Option<String>,
    config: String,
    created_by: String,
    created_at: String,
    updated_at: String,
    is_builtin: i64,
}

impl IntoDomain for TemplateRow {
    type Output = ServerTemplate;

    fn into_domain(self) -> Result<ServerTemplate, AppError> {
        let config: ServerConfig = deserialize_json(&self.config)?;
        let requires_steamcmd = crate::utils::steamcmd::config_requires_steamcmd(&config);
        let requires_curseforge = crate::types::template::config_requires_curseforge(&config);
        let requires_github = crate::types::template::config_requires_github(&config);
        Ok(ServerTemplate {
            id: parse_uuid(&self.id)?,
            name: self.name,
            description: self.description,
            config,
            created_by: parse_uuid(&self.created_by)?,
            created_at: parse_timestamp(&self.created_at)?,
            updated_at: parse_timestamp(&self.updated_at)?,
            is_builtin: self.is_builtin != 0,
            requires_steamcmd,
            requires_curseforge,
            requires_github,
        })
    }
}

#[derive(FromRow)]
struct PermissionRow {
    user_id: String,
    server_id: String,
    level: String,
}

impl IntoDomain for PermissionRow {
    type Output = ServerPermission;

    fn into_domain(self) -> Result<ServerPermission, AppError> {
        Ok(ServerPermission {
            user_id: parse_uuid(&self.user_id)?,
            server_id: parse_uuid(&self.server_id)?,
            level: parse_permission_level(&self.level)?,
        })
    }
}

fn parse_permission_level(s: &str) -> Result<PermissionLevel, AppError> {
    match s {
        "viewer" => Ok(PermissionLevel::Viewer),
        "operator" => Ok(PermissionLevel::Operator),
        "manager" => Ok(PermissionLevel::Manager),
        "admin" => Ok(PermissionLevel::Admin),
        "owner" => Ok(PermissionLevel::Owner),
        _ => Err(AppError::Internal(format!(
            "Invalid permission level: {}",
            s
        ))),
    }
}

fn format_permission_level(level: PermissionLevel) -> String {
    match level {
        PermissionLevel::Viewer => "viewer",
        PermissionLevel::Operator => "operator",
        PermissionLevel::Manager => "manager",
        PermissionLevel::Admin => "admin",
        PermissionLevel::Owner => "owner",
    }
    .to_string()
}

/// Async SQLite database wrapper for all persistence operations.
///
/// Tables:
///   - `users`         : User accounts with auth credentials
///   - `servers`       : Server configurations and metadata
///   - `templates`     : Server templates
///   - `permissions`   : User-server permission mappings
///   - `settings`      : Key-value store for app-level config (SMTP, alerts, etc.)
///   - `server_alerts` : Per-server alert preferences
#[derive(Clone)]
pub struct Database {
    pool: Pool<Sqlite>,
}

impl Database {
    /// Expose the connection pool for operations that need direct access
    /// (e.g. `VACUUM INTO` for backups).
    pub fn pool(&self) -> &Pool<Sqlite> {
        &self.pool
    }

    /// Create a consistent, standalone backup of the database at `dest`.
    ///
    /// Uses SQLite's `VACUUM INTO` which produces a clean, defragmented
    /// copy that does not require WAL or SHM files.  The operation runs
    /// concurrently with normal reads/writes (WAL mode).
    pub async fn vacuum_into(&self, dest: &std::path::Path) -> Result<(), AppError> {
        let dest_str = dest.to_string_lossy().replace('\'', "''");
        sqlx::query(&format!("VACUUM INTO '{}'", dest_str))
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Internal(format!("Database backup failed: {}", e)))?;
        Ok(())
    }

    /// Open or create a SQLite database at the given path.
    /// Runs migrations automatically and configures WAL mode + foreign keys.
    pub async fn open(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let db_path = path.as_ref();

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let options = SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true);

        let max_conns: u32 = std::env::var("ANYSERVER_DB_MAX_CONNECTIONS")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|&n| n > 0)
            .unwrap_or(16);

        let pool = SqlitePoolOptions::new()
            .max_connections(max_conns)
            .min_connections(2)
            .connect_with(options)
            .await?;

        // Run migrations
        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }

    // ──────────────────────────────────────────────
    //  Generic Helper Methods (internal)
    // ──────────────────────────────────────────────

    /// Generic method to get a JSON setting
    async fn get_json_setting<T: serde::de::DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<Option<T>, AppError> {
        let row = sqlx::query!("SELECT value FROM settings WHERE key = ?", key)
            .fetch_optional(&self.pool)
            .await?;

        row.map(|r| deserialize_json::<T>(&r.value)).transpose()
    }

    /// Generic method to save a JSON setting
    async fn save_json_setting<T: serde::Serialize>(
        &self,
        key: &str,
        value: &T,
    ) -> Result<(), AppError> {
        let json = serde_json::to_string(value)
            .map_err(|e| AppError::Internal(format!("Serialization error: {}", e)))?;

        sqlx::query!(
            "INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            key,
            json
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Generic method to delete a setting
    async fn delete_setting(&self, key: &str) -> Result<bool, AppError> {
        let result = sqlx::query!("DELETE FROM settings WHERE key = ?", key)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    // ──────────────────────────────────────────────
    //  Servers
    // ──────────────────────────────────────────────

    pub async fn insert_server(&self, server: &Server) -> Result<(), AppError> {
        let id = server.id.to_string();
        let owner_id = server.owner_id.to_string();
        let config = serialize_json(&server.config)?;
        let created_at = server.created_at.to_rfc3339();
        let updated_at = server.updated_at.to_rfc3339();
        let parameter_values = serialize_json(&server.parameter_values)?;
        let installed = bool_to_int(server.installed);
        let installed_at = server.installed_at.as_ref().map(|t| t.to_rfc3339());
        let updated_via_pipeline_at = server
            .updated_via_pipeline_at
            .as_ref()
            .map(|t| t.to_rfc3339());
        let installed_version = &server.installed_version;
        let source_template_id = server.source_template_id.as_ref().map(|id| id.to_string());

        sqlx::query!(
            r#"INSERT INTO servers (id, owner_id, config, created_at, updated_at, parameter_values,
                installed, installed_at, updated_via_pipeline_at, installed_version, source_template_id)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            id,
            owner_id,
            config,
            created_at,
            updated_at,
            parameter_values,
            installed,
            installed_at,
            updated_via_pipeline_at,
            installed_version,
            source_template_id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Get a server by ID, returning `AppError::NotFound` if it doesn't exist.
    pub async fn require_server(&self, id: Uuid) -> Result<Server, AppError> {
        self.get_server(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Server {} not found", id)))
    }

    pub async fn get_server(&self, id: Uuid) -> Result<Option<Server>, AppError> {
        let id_str = id.to_string();

        let row = sqlx::query_as!(
            ServerRow,
            r#"
            SELECT id, owner_id, config, created_at, updated_at,
                   parameter_values, installed, installed_at,
                   updated_via_pipeline_at, installed_version, source_template_id
            FROM servers WHERE id = ?
            "#,
            id_str
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| r.into_domain()).transpose()
    }

    pub async fn list_servers(&self) -> Result<Vec<Server>, AppError> {
        let rows = sqlx::query_as!(
            ServerRow,
            r#"
            SELECT id, owner_id, config, created_at, updated_at,
                   parameter_values, installed, installed_at,
                   updated_via_pipeline_at, installed_version, source_template_id
            FROM servers
            ORDER BY created_at DESC
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(|r| r.into_domain()).collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn list_servers_paginated(
        &self,
        page: u32,
        per_page: u32,
        search: Option<&str>,
        _status: Option<&str>,
        sort: &str,
        order: &str,
        user_id: Option<&Uuid>,
    ) -> Result<(Vec<Server>, u64), AppError> {
        // Clamp per_page to 1–100
        let per_page = per_page.clamp(1, 100);
        let page = page.max(1);
        let offset = (page - 1) * per_page;

        let order_clause = server_order_clause(sort, order);

        // Note: status filtering is done in application code since runtime status
        // is not stored in the database

        if let Some(uid) = user_id {
            // Non-admin: join with permissions table
            let uid_str = uid.to_string();
            let search_pattern = make_search_pattern(search);

            // Get total count first
            let count_query = r#"
                SELECT COUNT(DISTINCT s.id) as total
                FROM servers s
                LEFT JOIN permissions p ON p.server_id = s.id
                WHERE (s.owner_id = ? OR p.user_id = ?)
                  AND (? IS NULL OR json_extract(s.config, '$.name') LIKE ? ESCAPE '\')
            "#;

            let total: i64 = sqlx::query_scalar(count_query)
                .bind(&uid_str)
                .bind(&uid_str)
                .bind(&search_pattern)
                .bind(&search_pattern)
                .fetch_one(&self.pool)
                .await?;

            // Get paginated data
            let data_query = format!(
                r#"
                SELECT s.id, s.owner_id, s.config, s.created_at, s.updated_at,
                       s.parameter_values, s.installed, s.installed_at,
                       s.updated_via_pipeline_at, s.installed_version, s.source_template_id
                FROM servers s
                LEFT JOIN permissions p ON p.server_id = s.id
                WHERE (s.owner_id = ? OR p.user_id = ?)
                  AND (? IS NULL OR json_extract(s.config, '$.name') LIKE ? ESCAPE '\')
                GROUP BY s.id
                ORDER BY {}
                LIMIT ? OFFSET ?
                "#,
                order_clause
            );

            let rows: Vec<ServerRow> = sqlx::query_as(&data_query)
                .bind(&uid_str)
                .bind(&uid_str)
                .bind(&search_pattern)
                .bind(&search_pattern)
                .bind(per_page as i64)
                .bind(offset as i64)
                .fetch_all(&self.pool)
                .await?;

            let servers: Result<Vec<Server>, AppError> =
                rows.into_iter().map(|r| r.into_domain()).collect();

            Ok((servers?, total as u64))
        } else {
            // Admin: no permission filtering
            let search_pattern = make_search_pattern(search);

            // Get total count first
            let count_query = r#"
                SELECT COUNT(*) as total
                FROM servers s
                WHERE (? IS NULL OR json_extract(s.config, '$.name') LIKE ? ESCAPE '\')
            "#;

            let total: i64 = sqlx::query_scalar(count_query)
                .bind(&search_pattern)
                .bind(&search_pattern)
                .fetch_one(&self.pool)
                .await?;

            // Get paginated data
            let data_query = format!(
                r#"
                SELECT id, owner_id, config, created_at, updated_at,
                       parameter_values, installed, installed_at,
                       updated_via_pipeline_at, installed_version, source_template_id
                FROM servers s
                WHERE (? IS NULL OR json_extract(s.config, '$.name') LIKE ? ESCAPE '\')
                ORDER BY {}
                LIMIT ? OFFSET ?
                "#,
                order_clause
            );

            let rows: Vec<ServerRow> = sqlx::query_as(&data_query)
                .bind(&search_pattern)
                .bind(&search_pattern)
                .bind(per_page as i64)
                .bind(offset as i64)
                .fetch_all(&self.pool)
                .await?;

            let servers: Result<Vec<Server>, AppError> =
                rows.into_iter().map(|r| r.into_domain()).collect();

            Ok((servers?, total as u64))
        }
    }

    /// Fetch all servers accessible to the given user, with optional search
    /// filtering and sorting but **no pagination**.  Used when the caller
    /// needs to apply an application-level filter (e.g. runtime status) and
    /// then paginate the result itself.
    pub async fn list_servers_all_filtered(
        &self,
        search: Option<&str>,
        sort: &str,
        order: &str,
        user_id: Option<&Uuid>,
    ) -> Result<Vec<Server>, AppError> {
        let order_clause = server_order_clause(sort, order);

        if let Some(uid) = user_id {
            let uid_str = uid.to_string();
            let search_pattern = make_search_pattern(search);

            let data_query = format!(
                r#"
                SELECT s.id, s.owner_id, s.config, s.created_at, s.updated_at,
                       s.parameter_values, s.installed, s.installed_at,
                       s.updated_via_pipeline_at, s.installed_version, s.source_template_id
                FROM servers s
                LEFT JOIN permissions p ON p.server_id = s.id
                WHERE (s.owner_id = ? OR p.user_id = ?)
                  AND (? IS NULL OR json_extract(s.config, '$.name') LIKE ? ESCAPE '\')
                GROUP BY s.id
                ORDER BY {}
                "#,
                order_clause
            );

            let rows: Vec<ServerRow> = sqlx::query_as(&data_query)
                .bind(&uid_str)
                .bind(&uid_str)
                .bind(&search_pattern)
                .bind(&search_pattern)
                .fetch_all(&self.pool)
                .await?;

            rows.into_iter().map(|r| r.into_domain()).collect()
        } else {
            let search_pattern = make_search_pattern(search);

            let data_query = format!(
                r#"
                SELECT id, owner_id, config, created_at, updated_at,
                       parameter_values, installed, installed_at,
                       updated_via_pipeline_at, installed_version, source_template_id
                FROM servers s
                WHERE (? IS NULL OR json_extract(s.config, '$.name') LIKE ? ESCAPE '\')
                ORDER BY {}
                "#,
                order_clause
            );

            let rows: Vec<ServerRow> = sqlx::query_as(&data_query)
                .bind(&search_pattern)
                .bind(&search_pattern)
                .fetch_all(&self.pool)
                .await?;

            rows.into_iter().map(|r| r.into_domain()).collect()
        }
    }

    pub async fn update_server(&self, server: &Server) -> Result<(), AppError> {
        let config = serialize_json(&server.config)?;
        let updated_at = server.updated_at.to_rfc3339();
        let parameter_values = serialize_json(&server.parameter_values)?;
        let installed = bool_to_int(server.installed);
        let installed_at = server.installed_at.as_ref().map(|t| t.to_rfc3339());
        let updated_via_pipeline_at = server
            .updated_via_pipeline_at
            .as_ref()
            .map(|t| t.to_rfc3339());
        let installed_version = &server.installed_version;
        let source_template_id = server.source_template_id.as_ref().map(|id| id.to_string());
        let id = server.id.to_string();

        sqlx::query!(
            r#"UPDATE servers SET config = ?, updated_at = ?, parameter_values = ?, installed = ?,
               installed_at = ?, updated_via_pipeline_at = ?, installed_version = ?, source_template_id = ?
               WHERE id = ?"#,
            config,
            updated_at,
            parameter_values,
            installed,
            installed_at,
            updated_via_pipeline_at,
            installed_version,
            source_template_id,
            id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_server(&self, id: Uuid) -> Result<bool, AppError> {
        let id_str = id.to_string();
        let result = sqlx::query!("DELETE FROM servers WHERE id = ?", id_str)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // ──────────────────────────────────────────────
    //  Templates
    // ──────────────────────────────────────────────

    pub async fn insert_template(&self, template: &ServerTemplate) -> Result<(), AppError> {
        let id = template.id.to_string();
        let name = &template.name;
        let description = &template.description;
        let config = serialize_json(&template.config)?;
        let created_by = template.created_by.to_string();
        let created_at = template.created_at.to_rfc3339();
        let updated_at = template.updated_at.to_rfc3339();
        let is_builtin = bool_to_int(template.is_builtin);

        sqlx::query!(
            r#"INSERT INTO templates (id, name, description, config, created_by, created_at, updated_at, is_builtin)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
            id,
            name,
            description,
            config,
            created_by,
            created_at,
            updated_at,
            is_builtin
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Get a template by ID, returning `AppError::NotFound` if it doesn't exist.
    pub async fn require_template(&self, id: Uuid) -> Result<ServerTemplate, AppError> {
        self.get_template(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Template {} not found", id)))
    }

    pub async fn get_template(&self, id: Uuid) -> Result<Option<ServerTemplate>, AppError> {
        let id_str = id.to_string();

        let row = sqlx::query_as!(
            TemplateRow,
            r#"
            SELECT id, name, description, config, created_by, created_at, updated_at, is_builtin
            FROM templates WHERE id = ?
            "#,
            id_str
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| r.into_domain()).transpose()
    }

    pub async fn list_templates(&self) -> Result<Vec<ServerTemplate>, AppError> {
        let rows = sqlx::query_as!(
            TemplateRow,
            r#"
            SELECT id, name, description, config, created_by, created_at, updated_at, is_builtin
            FROM templates
            ORDER BY is_builtin DESC, name ASC
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(|r| r.into_domain()).collect()
    }

    pub async fn update_template(&self, template: &ServerTemplate) -> Result<(), AppError> {
        let name = &template.name;
        let description = &template.description;
        let config = serialize_json(&template.config)?;
        let updated_at = template.updated_at.to_rfc3339();
        let id = template.id.to_string();

        sqlx::query!(
            "UPDATE templates SET name = ?, description = ?, config = ?, updated_at = ? WHERE id = ?",
            name,
            description,
            config,
            updated_at,
            id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_template(&self, id: Uuid) -> Result<bool, AppError> {
        let id_str = id.to_string();
        let result = sqlx::query!("DELETE FROM templates WHERE id = ?", id_str)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // ──────────────────────────────────────────────
    //  Users
    // ──────────────────────────────────────────────

    pub async fn insert_user(&self, user: &User) -> Result<(), AppError> {
        let id = user.id.to_string();
        let username = &user.username;
        let password_hash = &user.password_hash;
        let role = format!("{:?}", user.role).to_lowercase();
        let created_at = user.created_at.to_rfc3339();
        let token_generation = user.token_generation;
        let global_capabilities = serialize_json(&user.global_capabilities)?;

        sqlx::query!(
            "INSERT INTO users (id, username, password_hash, role, created_at, token_generation, global_capabilities) VALUES (?, ?, ?, ?, ?, ?, ?)",
            id,
            username,
            password_hash,
            role,
            created_at,
            token_generation,
            global_capabilities
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Get a user by ID, returning `AppError::NotFound` if they don't exist.
    pub async fn require_user(&self, id: Uuid) -> Result<User, AppError> {
        self.get_user(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("User {} not found", id)))
    }

    pub async fn get_user(&self, id: Uuid) -> Result<Option<User>, AppError> {
        let id_str = id.to_string();
        sqlx::query_as!(
            UserRow,
            "SELECT id, username, password_hash, role, created_at, token_generation, global_capabilities FROM users WHERE id = ?",
            id_str
        )
        .fetch_optional(&self.pool)
        .await?
        .map(|r| r.into_domain())
        .transpose()
    }

    pub async fn get_user_by_username(&self, username: &str) -> Result<Option<User>, AppError> {
        sqlx::query_as!(
            UserRow,
            "SELECT id, username, password_hash, role, created_at, token_generation, global_capabilities FROM users WHERE username = ?",
            username
        )
        .fetch_optional(&self.pool)
        .await?
        .map(|r| r.into_domain())
        .transpose()
    }

    pub async fn list_users(&self) -> Result<Vec<User>, AppError> {
        sqlx::query_as!(
            UserRow,
            "SELECT id, username, password_hash, role, created_at, token_generation, global_capabilities FROM users ORDER BY created_at ASC"
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|r| r.into_domain())
        .collect()
    }

    pub async fn user_count(&self) -> Result<usize, AppError> {
        let row = sqlx::query!("SELECT COUNT(*) as count FROM users")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.count as usize)
    }

    pub async fn update_user(&self, user: &User) -> Result<(), AppError> {
        let username = &user.username;
        let password_hash = &user.password_hash;
        let role = format!("{:?}", user.role).to_lowercase();
        let token_generation = user.token_generation;
        let global_capabilities = serialize_json(&user.global_capabilities)?;
        let id = user.id.to_string();

        sqlx::query!(
            "UPDATE users SET username = ?, password_hash = ?, role = ?, token_generation = ?, global_capabilities = ? WHERE id = ?",
            username,
            password_hash,
            role,
            token_generation,
            global_capabilities,
            id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_user(&self, id: Uuid) -> Result<bool, AppError> {
        let id_str = id.to_string();
        let result = sqlx::query!("DELETE FROM users WHERE id = ?", id_str)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn username_exists(&self, username: &str) -> Result<bool, AppError> {
        Ok(sqlx::query_scalar!(
            "SELECT COUNT(*) > 0 FROM users WHERE username = ?",
            username
        )
        .fetch_one(&self.pool)
        .await?
            != 0)
    }

    // ──────────────────────────────────────────────
    //  Permissions
    // ──────────────────────────────────────────────

    pub async fn set_permission(&self, perm: &ServerPermission) -> Result<(), AppError> {
        let id = Uuid::new_v4().to_string();
        let user_id = perm.user_id.to_string();
        let server_id = perm.server_id.to_string();
        let level = format_permission_level(perm.level);

        sqlx::query!(
            r#"INSERT INTO permissions (id, user_id, server_id, level) VALUES (?, ?, ?, ?)
               ON CONFLICT(user_id, server_id) DO UPDATE SET level = excluded.level"#,
            id,
            user_id,
            server_id,
            level
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_permission(
        &self,
        user_id: &Uuid,
        server_id: &Uuid,
    ) -> Result<Option<ServerPermission>, AppError> {
        let user_id_str = user_id.to_string();
        let server_id_str = server_id.to_string();

        sqlx::query_as!(
            PermissionRow,
            "SELECT user_id, server_id, level FROM permissions WHERE user_id = ? AND server_id = ?",
            user_id_str,
            server_id_str
        )
        .fetch_optional(&self.pool)
        .await?
        .map(|r| r.into_domain())
        .transpose()
    }

    pub async fn remove_permission(
        &self,
        user_id: &Uuid,
        server_id: &Uuid,
    ) -> Result<bool, AppError> {
        let user_id_str = user_id.to_string();
        let server_id_str = server_id.to_string();

        let result = sqlx::query!(
            "DELETE FROM permissions WHERE user_id = ? AND server_id = ?",
            user_id_str,
            server_id_str
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn list_permissions_for_server(
        &self,
        server_id: &Uuid,
    ) -> Result<Vec<ServerPermission>, AppError> {
        let server_id_str = server_id.to_string();

        sqlx::query_as!(
            PermissionRow,
            "SELECT user_id, server_id, level FROM permissions WHERE server_id = ?",
            server_id_str
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|r| r.into_domain())
        .collect()
    }

    pub async fn list_permissions_for_user(
        &self,
        user_id: &Uuid,
    ) -> Result<Vec<ServerPermission>, AppError> {
        let user_id_str = user_id.to_string();

        sqlx::query_as!(
            PermissionRow,
            "SELECT user_id, server_id, level FROM permissions WHERE user_id = ?",
            user_id_str
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|r| r.into_domain())
        .collect()
    }

    /// Fetch all permissions for a user in a single query, returning a map
    /// from server ID → permission level.  Used by the server-list endpoint
    /// to avoid N+1 queries.
    pub async fn list_permissions_for_user_batch(
        &self,
        user_id: &Uuid,
    ) -> Result<HashMap<Uuid, PermissionLevel>, AppError> {
        let perms = self.list_permissions_for_user(user_id).await?;
        let mut map = HashMap::with_capacity(perms.len());
        for p in perms {
            map.insert(p.server_id, p.level);
        }
        Ok(map)
    }

    pub async fn get_effective_permission(
        &self,
        user_id: &Uuid,
        server_id: &Uuid,
    ) -> Result<Option<PermissionLevel>, AppError> {
        Ok(self
            .get_permission(user_id, server_id)
            .await?
            .map(|p| p.level))
    }

    // ──────────────────────────────────────────────
    //  Settings (key-value store)
    // ──────────────────────────────────────────────

    pub async fn get_settings(&self) -> Result<AppSettings, AppError> {
        self.get_json_setting("app")
            .await
            .map(|opt| opt.unwrap_or_default())
    }

    pub async fn save_settings(&self, settings: &AppSettings) -> Result<(), AppError> {
        self.save_json_setting("app", settings).await
    }

    pub async fn is_setup_complete(&self) -> bool {
        self.get_settings()
            .await
            .map(|s| s.setup_complete)
            .unwrap_or(false)
    }

    // ──────────────────────────────────────────────
    //  SMTP Config
    // ──────────────────────────────────────────────

    pub async fn get_smtp_config(&self) -> Result<Option<SmtpConfig>, AppError> {
        let config: Option<SmtpConfig> = self.get_json_setting("smtp").await?;
        match config {
            Some(mut c) if !c.password.is_empty() => {
                if crate::security::encryption::is_encrypted(&c.password) {
                    c.password =
                        crate::security::encryption::decrypt(&c.password).map_err(|e| {
                            AppError::Internal(format!(
                                "Failed to decrypt SMTP password: {}. \
                             If the JWT secret changed, re-save the SMTP configuration.",
                                e
                            ))
                        })?;
                }
                Ok(Some(c))
            }
            other => Ok(other),
        }
    }

    pub async fn save_smtp_config(&self, config: &SmtpConfig) -> Result<(), AppError> {
        let mut to_store = config.clone();
        if !to_store.password.is_empty()
            && !crate::security::encryption::is_encrypted(&to_store.password)
        {
            to_store.password =
                crate::security::encryption::encrypt(&to_store.password).map_err(|e| {
                    AppError::Internal(format!("Failed to encrypt SMTP password: {}", e))
                })?;
        }
        self.save_json_setting("smtp", &to_store).await
    }

    /// Migrate a plaintext or unencrypted SMTP password to encrypted form.
    /// This is idempotent — already-encrypted passwords are left untouched.
    pub async fn migrate_smtp_password(&self) -> Result<bool, AppError> {
        // Read the raw config WITHOUT decryption so we can inspect the
        // stored representation.
        let raw: Option<SmtpConfig> = self.get_json_setting("smtp").await?;
        match raw {
            Some(ref c)
                if !c.password.is_empty()
                    && !crate::security::encryption::is_encrypted(&c.password) =>
            {
                tracing::info!("Migrating plaintext SMTP password to encrypted form");
                // `save_smtp_config` will encrypt it for us.
                // The password is currently plaintext, so we can pass the raw
                // config directly.
                self.save_smtp_config(c).await?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub async fn delete_smtp_config(&self) -> Result<bool, AppError> {
        self.delete_setting("smtp").await
    }

    // ──────────────────────────────────────────────
    //  GitHub Settings
    // ──────────────────────────────────────────────

    pub async fn get_github_settings(
        &self,
    ) -> Result<Option<crate::types::system::GithubSettings>, AppError> {
        self.get_json_setting("github").await
    }

    pub async fn save_github_settings(
        &self,
        settings: &crate::types::system::GithubSettings,
    ) -> Result<(), AppError> {
        self.save_json_setting("github", settings).await
    }

    pub async fn delete_github_settings(&self) -> Result<bool, AppError> {
        self.delete_setting("github").await
    }

    // ──────────────────────────────────────────────
    //  CurseForge Settings
    // ──────────────────────────────────────────────

    pub async fn get_curseforge_settings(
        &self,
    ) -> Result<Option<crate::types::system::CurseForgeSettings>, AppError> {
        self.get_json_setting("curseforge").await
    }

    pub async fn save_curseforge_settings(
        &self,
        settings: &crate::types::system::CurseForgeSettings,
    ) -> Result<(), AppError> {
        self.save_json_setting("curseforge", settings).await
    }

    pub async fn delete_curseforge_settings(&self) -> Result<bool, AppError> {
        self.delete_setting("curseforge").await
    }

    // ──────────────────────────────────────────────
    //  Alert Config
    // ──────────────────────────────────────────────

    pub async fn get_alert_config(&self) -> Result<AlertConfig, AppError> {
        self.get_json_setting("alerts")
            .await
            .map(|opt| opt.unwrap_or_default())
    }

    pub async fn save_alert_config(&self, config: &AlertConfig) -> Result<(), AppError> {
        self.save_json_setting("alerts", config).await
    }

    // ──────────────────────────────────────────────
    //  Per-Server Alert Config
    // ──────────────────────────────────────────────

    pub async fn get_server_alert_config(
        &self,
        server_id: &Uuid,
    ) -> Result<Option<ServerAlertConfig>, AppError> {
        let server_id_str = server_id.to_string();

        sqlx::query!(
            "SELECT config FROM server_alerts WHERE server_id = ?",
            server_id_str
        )
        .fetch_optional(&self.pool)
        .await?
        .map(|r| deserialize_json::<ServerAlertConfig>(&r.config))
        .transpose()
    }

    pub async fn save_server_alert_config(
        &self,
        config: &ServerAlertConfig,
    ) -> Result<(), AppError> {
        let server_id = config.server_id.to_string();
        let config_json = serialize_json(config)?;

        sqlx::query!(
            "INSERT INTO server_alerts (server_id, config) VALUES (?, ?) ON CONFLICT(server_id) DO UPDATE SET config = excluded.config",
            server_id,
            config_json
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_server_alert_config(&self, server_id: &Uuid) -> Result<bool, AppError> {
        let server_id_str = server_id.to_string();
        let result = sqlx::query!(
            "DELETE FROM server_alerts WHERE server_id = ?",
            server_id_str
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    // ──────────────────────────────────────────────
    //  SFTP Lookups (indexed)
    // ──────────────────────────────────────────────

    /// Find a server by SFTP username (uses indexed lookup, not O(n) scan).
    /// Look up a server by its SFTP login username.
    /// The username can be either:
    /// - The server's UUID (as a string)
    /// - The configured `sftp_username` in the server's config
    pub async fn find_server_by_sftp_username(
        &self,
        username: &str,
    ) -> Result<Option<Server>, AppError> {
        sqlx::query_as!(
            ServerRow,
            r#"SELECT id, owner_id, config, created_at, updated_at, parameter_values, installed,
               installed_at, updated_via_pipeline_at, installed_version, source_template_id
               FROM servers
               WHERE id = ? OR (json_extract(config, '$.sftp_username') = ? AND ? != '')
               LIMIT 1"#,
            username,
            username,
            username
        )
        .fetch_optional(&self.pool)
        .await?
        .map(|r| r.into_domain())
        .transpose()
    }

    // ──────────────────────────────────────────────
    //  Token Lifecycle Management
    // ──────────────────────────────────────────────

    /// Increment a user's token_generation, invalidating all their outstanding tokens.
    pub async fn increment_token_generation(&self, user_id: Uuid) -> Result<i64, AppError> {
        let user_id_str = user_id.to_string();

        sqlx::query!(
            "UPDATE users SET token_generation = token_generation + 1 WHERE id = ?",
            user_id_str
        )
        .execute(&self.pool)
        .await?;

        // Return the new generation value
        let row = sqlx::query!(
            "SELECT token_generation FROM users WHERE id = ?",
            user_id_str
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(row.token_generation)
    }

    /// Store a refresh token in the database.
    pub async fn insert_refresh_token(
        &self,
        id: &str,
        user_id: Uuid,
        family_id: &str,
        token_hash: &str,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), AppError> {
        let user_id_str = user_id.to_string();
        let expires_at_str = expires_at.to_rfc3339();
        let created_at = chrono::Utc::now().to_rfc3339();

        sqlx::query!(
            "INSERT INTO refresh_tokens (id, user_id, family_id, token_hash, expires_at, created_at, revoked) VALUES (?, ?, ?, ?, ?, ?, 0)",
            id,
            user_id_str,
            family_id,
            token_hash,
            expires_at_str,
            created_at
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get a refresh token by its hash.
    pub async fn get_refresh_token(
        &self,
        token_hash: &str,
    ) -> Result<Option<RefreshTokenRow>, AppError> {
        sqlx::query_as!(
            RefreshTokenRow,
            "SELECT id, user_id, family_id, token_hash, expires_at, created_at, revoked FROM refresh_tokens WHERE token_hash = ?",
            token_hash
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Revoke a specific refresh token.
    ///
    /// Stores the current unix timestamp (seconds) in `revoked` instead of a
    /// bare `1`.  Every non-zero value is still treated as "revoked" by all
    /// existing checks (`revoked != 0`), but the timestamp lets the refresh
    /// handler distinguish a *very recently* rotated token from a genuinely
    /// stale one (grace-period logic for hard-refresh races).
    pub async fn revoke_refresh_token(&self, token_hash: &str) -> Result<(), AppError> {
        let now_ts = chrono::Utc::now().timestamp();
        sqlx::query!(
            "UPDATE refresh_tokens SET revoked = ? WHERE token_hash = ?",
            now_ts,
            token_hash
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Return the most-recently created **active** (non-revoked, non-expired)
    /// refresh token in the given family, if any.  Used by the refresh handler
    /// to recover gracefully when a just-rotated token is re-presented during
    /// a hard-reload race.
    pub async fn get_latest_active_family_token(
        &self,
        family_id: &str,
    ) -> Result<Option<RefreshTokenRow>, AppError> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query_as!(
            RefreshTokenRow,
            "SELECT id, user_id, family_id, token_hash, expires_at, created_at, revoked
             FROM refresh_tokens
             WHERE family_id = ? AND revoked = 0 AND expires_at > ?
             ORDER BY created_at DESC
             LIMIT 1",
            family_id,
            now
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Revoke all refresh tokens for a user.
    pub async fn revoke_all_refresh_tokens(&self, user_id: Uuid) -> Result<i64, AppError> {
        let user_id_str = user_id.to_string();
        let result = sqlx::query!(
            "UPDATE refresh_tokens SET revoked = 1 WHERE user_id = ? AND revoked = 0",
            user_id_str
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() as i64)
    }

    /// Delete expired refresh tokens (cleanup).
    pub async fn delete_expired_refresh_tokens(&self) -> Result<u64, AppError> {
        let now = chrono::Utc::now().to_rfc3339();
        let result = sqlx::query!("DELETE FROM refresh_tokens WHERE expires_at < ?", now)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// Revoke all refresh tokens in a specific family (used for reuse detection).
    pub async fn revoke_token_family(&self, family_id: &str) -> Result<i64, AppError> {
        let result = sqlx::query!(
            "UPDATE refresh_tokens SET revoked = 1 WHERE family_id = ? AND revoked = 0",
            family_id
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() as i64)
    }

    /// List active refresh tokens for a user (for session management).
    pub async fn list_user_sessions(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<RefreshTokenRow>, AppError> {
        let user_id_str = user_id.to_string();
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query_as!(
            RefreshTokenRow,
            "SELECT id, user_id, family_id, token_hash, expires_at, created_at, revoked
             FROM refresh_tokens
             WHERE user_id = ? AND revoked = 0 AND expires_at > ?
             ORDER BY created_at DESC",
            user_id_str,
            now
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Revoke a refresh token by its ID (for individual session revocation).
    pub async fn revoke_refresh_token_by_id(
        &self,
        token_id: &str,
        user_id: Uuid,
    ) -> Result<bool, AppError> {
        let user_id_str = user_id.to_string();
        let result = sqlx::query!(
            "UPDATE refresh_tokens SET revoked = 1 WHERE id = ? AND user_id = ? AND revoked = 0",
            token_id,
            user_id_str
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Revoke a token family by family_id (for individual session revocation by family).
    pub async fn revoke_session_by_family(
        &self,
        family_id: &str,
        user_id: Uuid,
    ) -> Result<i64, AppError> {
        let user_id_str = user_id.to_string();
        let result = sqlx::query!(
            "UPDATE refresh_tokens SET revoked = 1 WHERE family_id = ? AND user_id = ? AND revoked = 0",
            family_id,
            user_id_str
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() as i64)
    }

    // ──────────────────────────────────────────────
    //  API Tokens
    // ──────────────────────────────────────────────

    pub async fn create_api_token(&self, token: &ApiToken) -> Result<(), AppError> {
        let id = token.id.to_string();
        let user_id = token.user_id.to_string();
        let scope = serde_json::to_string(&token.scope)
            .map_err(|e| AppError::Internal(format!("Failed to serialize token scope: {}", e)))?;
        let name = &token.name;
        let token_hash = &token.token_hash;
        let created_at = token.created_at.to_rfc3339();
        let expires_at = token.expires_at.as_ref().map(|t| t.to_rfc3339());
        let revoked = if token.revoked { 1i32 } else { 0i32 };

        sqlx::query!(
            r#"INSERT INTO api_tokens (id, user_id, name, token_hash, scope, created_at, expires_at, revoked)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
            id,
            user_id,
            name,
            token_hash,
            scope,
            created_at,
            expires_at,
            revoked
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn list_api_tokens_for_user(
        &self,
        user_id: &Uuid,
    ) -> Result<Vec<ApiToken>, AppError> {
        let uid = user_id.to_string();

        let rows = sqlx::query_as!(
            ApiTokenRow,
            r#"SELECT id, user_id, name, token_hash, scope, created_at, expires_at, last_used_at, revoked
               FROM api_tokens
               WHERE user_id = ?
               ORDER BY created_at DESC"#,
            uid
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(|r| r.into_domain()).collect()
    }

    pub async fn find_api_token_by_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<ApiToken>, AppError> {
        let row = sqlx::query_as!(
            ApiTokenRow,
            r#"SELECT id, user_id, name, token_hash, scope, created_at, expires_at, last_used_at, revoked
               FROM api_tokens
               WHERE token_hash = ?"#,
            token_hash
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| r.into_domain()).transpose()
    }

    pub async fn revoke_api_token(
        &self,
        token_id: &Uuid,
        user_id: &Uuid,
    ) -> Result<bool, AppError> {
        let tid = token_id.to_string();
        let uid = user_id.to_string();
        let result = sqlx::query!(
            "UPDATE api_tokens SET revoked = 1 WHERE id = ? AND user_id = ? AND revoked = 0",
            tid,
            uid
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn update_api_token_last_used(&self, token_id: &Uuid) -> Result<(), AppError> {
        let tid = token_id.to_string();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query!(
            "UPDATE api_tokens SET last_used_at = ? WHERE id = ?",
            now,
            tid
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ──────────────────────────────────────────────
    //  Invite Codes
    // ──────────────────────────────────────────────

    pub async fn insert_invite_code(&self, invite: &InviteCode) -> Result<(), AppError> {
        let id = invite.id.to_string();
        let code = &invite.code;
        let created_by = invite.created_by.to_string();
        let assigned_role = format!("{:?}", invite.assigned_role).to_lowercase();
        let assigned_permissions = serialize_json(&invite.assigned_permissions)?;
        let assigned_capabilities = serialize_json(&invite.assigned_capabilities)?;
        let expires_at = invite.expires_at.to_rfc3339();
        let created_at = invite.created_at.to_rfc3339();
        let label = invite.label.as_deref();

        sqlx::query!(
            r#"INSERT INTO invite_codes
               (id, code, created_by, assigned_role, assigned_permissions, assigned_capabilities, expires_at, created_at, label)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            id,
            code,
            created_by,
            assigned_role,
            assigned_permissions,
            assigned_capabilities,
            expires_at,
            created_at,
            label
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_invite_code_by_code(
        &self,
        code: &str,
    ) -> Result<Option<InviteCode>, AppError> {
        let row = sqlx::query_as!(
            InviteCodeRow,
            r#"SELECT id, code, created_by, assigned_role, assigned_permissions, assigned_capabilities,
                      expires_at, redeemed_by, redeemed_at, created_at, label
               FROM invite_codes WHERE code = ?"#,
            code
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| r.into_domain()).transpose()
    }

    pub async fn get_invite_code(&self, id: Uuid) -> Result<Option<InviteCode>, AppError> {
        let id_str = id.to_string();
        let row = sqlx::query_as!(
            InviteCodeRow,
            r#"SELECT id, code, created_by, assigned_role, assigned_permissions, assigned_capabilities,
                      expires_at, redeemed_by, redeemed_at, created_at, label
               FROM invite_codes WHERE id = ?"#,
            id_str
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| r.into_domain()).transpose()
    }

    pub async fn list_invite_codes(&self) -> Result<Vec<InviteCode>, AppError> {
        sqlx::query_as!(
            InviteCodeRow,
            r#"SELECT id, code, created_by, assigned_role, assigned_permissions, assigned_capabilities,
                      expires_at, redeemed_by, redeemed_at, created_at, label
               FROM invite_codes ORDER BY created_at DESC"#
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|r| r.into_domain())
        .collect()
    }

    pub async fn redeem_invite_code(&self, code: &str, user_id: &Uuid) -> Result<bool, AppError> {
        let user_id_str = user_id.to_string();
        let now = chrono::Utc::now().to_rfc3339();

        let result = sqlx::query!(
            r#"UPDATE invite_codes
               SET redeemed_by = ?, redeemed_at = ?
               WHERE code = ? AND redeemed_at IS NULL"#,
            user_id_str,
            now,
            code
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn update_invite_permissions(
        &self,
        id: &Uuid,
        assigned_role: &str,
        assigned_permissions: &[InvitePermissionGrant],
    ) -> Result<bool, AppError> {
        let id_str = id.to_string();
        let perms_json = serialize_json(&assigned_permissions.to_vec())?;

        let result = sqlx::query!(
            r#"UPDATE invite_codes
               SET assigned_role = ?, assigned_permissions = ?
               WHERE id = ? AND redeemed_by IS NULL"#,
            assigned_role,
            perms_json,
            id_str
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_invite_code(&self, id: Uuid) -> Result<bool, AppError> {
        let id_str = id.to_string();
        let result = sqlx::query!("DELETE FROM invite_codes WHERE id = ?", id_str)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_expired_invite_codes(&self) -> Result<u64, AppError> {
        let now = chrono::Utc::now().to_rfc3339();
        let result = sqlx::query!(
            "DELETE FROM invite_codes WHERE expires_at < ? AND redeemed_by IS NULL",
            now
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn code_exists(&self, code: &str) -> Result<bool, AppError> {
        Ok(
            sqlx::query_scalar!("SELECT COUNT(*) > 0 FROM invite_codes WHERE code = ?", code)
                .fetch_one(&self.pool)
                .await?
                != 0,
        )
    }

    // ──────────────────────────────────────────────
    //  Sandbox Profiles
    // ──────────────────────────────────────────────

    pub async fn get_sandbox_profile(
        &self,
        server_id: &Uuid,
    ) -> Result<Option<SandboxProfile>, AppError> {
        let sid = server_id.to_string();
        let row = sqlx::query_as!(
            SandboxProfileRow,
            r#"SELECT server_id, enabled, landlock_enabled, no_new_privs, fd_cleanup,
                      non_dumpable, namespace_isolation, pids_max, extra_read_paths,
                      extra_rw_paths, network_isolation, seccomp_mode, updated_at
               FROM sandbox_profiles WHERE server_id = ?"#,
            sid
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| r.into_domain()).transpose()
    }

    pub async fn upsert_sandbox_profile(&self, profile: &SandboxProfile) -> Result<(), AppError> {
        let server_id = profile.server_id.to_string();
        let enabled = bool_to_int(profile.enabled);
        let landlock_enabled = bool_to_int(profile.landlock_enabled);
        let no_new_privs = bool_to_int(profile.no_new_privs);
        let fd_cleanup = bool_to_int(profile.fd_cleanup);
        let non_dumpable = bool_to_int(profile.non_dumpable);
        let namespace_isolation = bool_to_int(profile.namespace_isolation);
        let pids_max = profile.pids_max as i64;
        let extra_read_paths = serialize_json(&profile.extra_read_paths)?;
        let extra_rw_paths = serialize_json(&profile.extra_rw_paths)?;
        let network_isolation = bool_to_int(profile.network_isolation);
        let seccomp_mode = &profile.seccomp_mode;
        let updated_at = profile.updated_at.to_rfc3339();

        sqlx::query!(
            r#"INSERT INTO sandbox_profiles
               (server_id, enabled, landlock_enabled, no_new_privs, fd_cleanup,
                non_dumpable, namespace_isolation, pids_max, extra_read_paths,
                extra_rw_paths, network_isolation, seccomp_mode, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(server_id) DO UPDATE SET
                enabled = excluded.enabled,
                landlock_enabled = excluded.landlock_enabled,
                no_new_privs = excluded.no_new_privs,
                fd_cleanup = excluded.fd_cleanup,
                non_dumpable = excluded.non_dumpable,
                namespace_isolation = excluded.namespace_isolation,
                pids_max = excluded.pids_max,
                extra_read_paths = excluded.extra_read_paths,
                extra_rw_paths = excluded.extra_rw_paths,
                network_isolation = excluded.network_isolation,
                seccomp_mode = excluded.seccomp_mode,
                updated_at = excluded.updated_at"#,
            server_id,
            enabled,
            landlock_enabled,
            no_new_privs,
            fd_cleanup,
            non_dumpable,
            namespace_isolation,
            pids_max,
            extra_read_paths,
            extra_rw_paths,
            network_isolation,
            seccomp_mode,
            updated_at
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_sandbox_profile(&self, server_id: &Uuid) -> Result<bool, AppError> {
        let sid = server_id.to_string();
        let result = sqlx::query!("DELETE FROM sandbox_profiles WHERE server_id = ?", sid)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Check whether sandbox management feature is enabled site-wide.
    pub async fn is_sandbox_management_enabled(&self) -> Result<bool, AppError> {
        self.get_json_setting::<bool>("sandbox_management_enabled")
            .await
            .map(|opt| opt.unwrap_or(false))
    }

    /// Toggle sandbox management feature site-wide (owner only).
    pub async fn set_sandbox_management_enabled(&self, enabled: bool) -> Result<(), AppError> {
        self.save_json_setting("sandbox_management_enabled", &enabled)
            .await
    }

    // ──────────────────────────────────────────────
    //  User Permission Summaries (admin)
    // ──────────────────────────────────────────────

    /// List all users with their server permissions, for the admin permission management view.
    pub async fn list_user_permission_summaries(
        &self,
    ) -> Result<Vec<(User, Vec<(ServerPermission, String)>)>, AppError> {
        let users = self.list_users().await?;
        let mut result = Vec::new();

        for user in users {
            let perms = self.list_permissions_for_user(&user.id).await?;
            let mut perm_with_names = Vec::new();
            for perm in perms {
                let server_name = match self.get_server(perm.server_id).await? {
                    Some(s) => s.config.name,
                    None => "(deleted server)".to_string(),
                };
                perm_with_names.push((perm, server_name));
            }
            result.push((user, perm_with_names));
        }

        Ok(result)
    }
}

// ─── Refresh Token Row ───

#[derive(Debug)]
pub struct RefreshTokenRow {
    pub id: String,
    pub user_id: String,
    pub family_id: String,
    pub token_hash: String,
    pub expires_at: String,
    pub created_at: String,
    pub revoked: i64,
}

// ─── API Token Row ───

#[derive(Debug, sqlx::FromRow)]
pub struct ApiTokenRow {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub token_hash: String,
    pub scope: String,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub revoked: i64,
}

impl IntoDomain for ApiTokenRow {
    type Output = ApiToken;

    fn into_domain(self) -> Result<ApiToken, AppError> {
        Ok(ApiToken {
            id: parse_uuid(&self.id)?,
            user_id: parse_uuid(&self.user_id)?,
            name: self.name,
            token_hash: self.token_hash,
            scope: serde_json::from_str(&self.scope).unwrap_or_default(),
            created_at: parse_timestamp(&self.created_at)?,
            expires_at: parse_optional_timestamp(&self.expires_at)?,
            last_used_at: parse_optional_timestamp(&self.last_used_at)?,
            revoked: self.revoked != 0,
        })
    }
}

// ─── Invite Code Row ───

#[derive(Debug)]
struct InviteCodeRow {
    id: String,
    code: String,
    created_by: String,
    assigned_role: String,
    assigned_permissions: String,
    assigned_capabilities: String,
    expires_at: String,
    redeemed_by: Option<String>,
    redeemed_at: Option<String>,
    created_at: String,
    label: Option<String>,
}

impl IntoDomain for InviteCodeRow {
    type Output = InviteCode;

    fn into_domain(self) -> Result<Self::Output, AppError> {
        Ok(InviteCode {
            id: parse_uuid(&self.id)?,
            code: self.code,
            created_by: parse_uuid(&self.created_by)?,
            assigned_role: parse_role(&self.assigned_role)?,
            assigned_permissions: deserialize_json::<Vec<InvitePermissionGrant>>(
                &self.assigned_permissions,
            )?,
            assigned_capabilities: deserialize_json(&self.assigned_capabilities)?,
            expires_at: parse_timestamp(&self.expires_at)?,
            redeemed_by: self.redeemed_by.as_deref().map(parse_uuid).transpose()?,
            redeemed_at: parse_optional_timestamp(&self.redeemed_at)?,
            created_at: parse_timestamp(&self.created_at)?,
            label: self.label,
        })
    }
}

// ─── Sandbox Profile Row ───

#[derive(Debug)]
struct SandboxProfileRow {
    server_id: String,
    enabled: i64,
    landlock_enabled: i64,
    no_new_privs: i64,
    fd_cleanup: i64,
    non_dumpable: i64,
    namespace_isolation: i64,
    pids_max: i64,
    extra_read_paths: String,
    extra_rw_paths: String,
    network_isolation: i64,
    seccomp_mode: String,
    updated_at: String,
}

impl IntoDomain for SandboxProfileRow {
    type Output = SandboxProfile;

    fn into_domain(self) -> Result<Self::Output, AppError> {
        Ok(SandboxProfile {
            server_id: parse_uuid(&self.server_id)?,
            enabled: self.enabled != 0,
            landlock_enabled: self.landlock_enabled != 0,
            no_new_privs: self.no_new_privs != 0,
            fd_cleanup: self.fd_cleanup != 0,
            non_dumpable: self.non_dumpable != 0,
            namespace_isolation: self.namespace_isolation != 0,
            pids_max: self.pids_max as u64,
            extra_read_paths: deserialize_json::<Vec<String>>(&self.extra_read_paths)?,
            extra_rw_paths: deserialize_json::<Vec<String>>(&self.extra_rw_paths)?,
            network_isolation: self.network_isolation != 0,
            seccomp_mode: self.seccomp_mode,
            updated_at: parse_timestamp(&self.updated_at)?,
        })
    }
}
