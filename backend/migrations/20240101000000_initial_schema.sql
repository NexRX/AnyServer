-- Initial schema for AnyServer SQLite database

-- Users table
CREATE TABLE users (
    id TEXT PRIMARY KEY NOT NULL,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'user',
    created_at TEXT NOT NULL
);

CREATE INDEX idx_users_username ON users(username);

-- Servers table
CREATE TABLE servers (
    id TEXT PRIMARY KEY NOT NULL,
    owner_id TEXT NOT NULL,
    config TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    parameter_values TEXT NOT NULL DEFAULT '{}',
    installed INTEGER NOT NULL DEFAULT 0,
    installed_at TEXT,
    updated_via_pipeline_at TEXT,
    installed_version TEXT,
    source_template_id TEXT,
    FOREIGN KEY (owner_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX idx_servers_owner ON servers(owner_id);
CREATE INDEX idx_servers_source_template ON servers(source_template_id);

-- Index for SFTP username lookups (avoids O(n) scan)
CREATE INDEX idx_servers_sftp_username ON servers(
    json_extract(config, '$.sftp_username')
) WHERE json_extract(config, '$.sftp_username') IS NOT NULL;

-- Templates table
CREATE TABLE templates (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    config TEXT NOT NULL,
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    is_builtin INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX idx_templates_created_by ON templates(created_by);
CREATE INDEX idx_templates_is_builtin ON templates(is_builtin);

-- Permissions table (user <-> server mapping)
CREATE TABLE permissions (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    server_id TEXT NOT NULL,
    level TEXT NOT NULL,
    UNIQUE(user_id, server_id),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY (server_id) REFERENCES servers(id) ON DELETE CASCADE
);

CREATE INDEX idx_permissions_user ON permissions(user_id);
CREATE INDEX idx_permissions_server ON permissions(server_id);

-- Settings table (key-value store for app-level config)
CREATE TABLE settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);

-- Server alerts table (per-server alert configuration)
CREATE TABLE server_alerts (
    server_id TEXT PRIMARY KEY NOT NULL,
    config TEXT NOT NULL,
    FOREIGN KEY (server_id) REFERENCES servers(id) ON DELETE CASCADE
);
