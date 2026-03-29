-- Invite codes: one-time redeemable codes created by admins
CREATE TABLE IF NOT EXISTS invite_codes (
    id TEXT PRIMARY KEY NOT NULL,
    -- 6-digit numeric code, unique and indexed for fast lookup
    code TEXT NOT NULL UNIQUE,
    -- Admin who created this invite
    created_by TEXT NOT NULL,
    -- Role to assign when redeemed (e.g. "admin", "user")
    assigned_role TEXT NOT NULL DEFAULT 'user',
    -- JSON array of {server_id, level} permission grants
    assigned_permissions TEXT NOT NULL DEFAULT '[]',
    -- When this code expires (RFC3339)
    expires_at TEXT NOT NULL,
    -- NULL until redeemed
    redeemed_by TEXT,
    redeemed_at TEXT,
    created_at TEXT NOT NULL,
    -- Optional label/note for admin reference
    label TEXT,
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY (redeemed_by) REFERENCES users(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_invite_codes_code ON invite_codes(code);
CREATE INDEX IF NOT EXISTS idx_invite_codes_created_by ON invite_codes(created_by);
CREATE INDEX IF NOT EXISTS idx_invite_codes_expires_at ON invite_codes(expires_at);

-- Sandbox profiles: per-server granular security configuration
-- Feature-flagged site-wide by the owner via app settings
CREATE TABLE IF NOT EXISTS sandbox_profiles (
    -- One profile per server (1:1 relationship)
    server_id TEXT PRIMARY KEY NOT NULL,
    -- Master switch for all isolation (overrides individual toggles)
    enabled INTEGER NOT NULL DEFAULT 1,
    -- Landlock filesystem sandboxing (Linux 5.13+)
    landlock_enabled INTEGER NOT NULL DEFAULT 1,
    -- PR_SET_NO_NEW_PRIVS — prevents suid/sgid privilege escalation
    no_new_privs INTEGER NOT NULL DEFAULT 1,
    -- Close inherited file descriptors beyond stdin/stdout/stderr
    fd_cleanup INTEGER NOT NULL DEFAULT 1,
    -- PR_SET_DUMPABLE=0 — prevents ptrace and /proc/pid/mem reads
    non_dumpable INTEGER NOT NULL DEFAULT 1,
    -- PID + mount namespace isolation
    namespace_isolation Integer NOT NULL DEFAULT 1,
    -- RLIMIT_NPROC fork-bomb protection (0 = no limit)
    pids_max INTEGER NOT NULL DEFAULT 0,
    -- JSON array of extra read-only paths
    extra_read_paths TEXT NOT NULL DEFAULT '[]',
    -- JSON array of extra read-write paths
    extra_rw_paths TEXT NOT NULL DEFAULT '[]',
    -- Enable network namespace isolation (currently not used but reserved)
    network_isolation INTEGER NOT NULL DEFAULT 0,
    -- Seccomp BPF filter mode: "off", "basic", "strict" (reserved for future)
    seccomp_mode TEXT NOT NULL DEFAULT 'off',
    updated_at TEXT NOT NULL,
    FOREIGN KEY (server_id) REFERENCES servers(id) ON DELETE CASCADE
);

-- Add sandbox_management_enabled to settings if not present
-- (handled via the key-value settings table, no schema change needed)

-- Add token_generation column if missing (idempotent for older DBs)
-- This column already exists from initial schema but we guard it anyway
-- ALTER TABLE users ADD COLUMN IF NOT EXISTS token_generation INTEGER NOT NULL DEFAULT 0;
-- SQLite doesn't support IF NOT EXISTS on ALTER TABLE, so we skip this.
