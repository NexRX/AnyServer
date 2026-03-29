-- Global capabilities: per-user feature flags for non-admin users.
-- Stored as a JSON array of capability strings, e.g. '["create_servers","manage_templates"]'.
-- Admin users implicitly have all capabilities regardless of this column.
ALTER TABLE users ADD COLUMN global_capabilities TEXT NOT NULL DEFAULT '[]';

-- Invite codes can pre-assign global capabilities to the new user on redemption.
ALTER TABLE invite_codes ADD COLUMN assigned_capabilities TEXT NOT NULL DEFAULT '[]';
