-- API tokens for long-lived, non-interactive authentication.
-- Each token is identified by a SHA-256 hash; the raw secret is shown
-- to the user exactly once at creation time and never stored.

CREATE TABLE IF NOT EXISTS api_tokens (
    id          TEXT    PRIMARY KEY NOT NULL,
    user_id     TEXT    NOT NULL,
    name        TEXT    NOT NULL,
    token_hash  TEXT    NOT NULL,
    scope       TEXT    NOT NULL DEFAULT '{}',
    created_at  TEXT    NOT NULL,
    expires_at  TEXT,
    last_used_at TEXT,
    revoked     INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_api_tokens_user   ON api_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_api_tokens_hash   ON api_tokens(token_hash);
CREATE INDEX IF NOT EXISTS idx_api_tokens_revoked ON api_tokens(revoked);
