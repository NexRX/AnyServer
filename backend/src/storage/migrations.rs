//! Data migrations for AnyServer.
//!
//! This module contains migration functions that run on startup to transform
//! legacy data formats into current schema requirements.

use argon2::password_hash::PasswordHash;

use crate::auth::hash_password;
use crate::error::AppError;
use crate::storage::db::Database;

/// Returns `true` if `value` is a valid PHC-format argon2 hash.
///
/// This is more robust than a simple `starts_with("$argon2")` prefix check
/// because it actually parses the hash structure, rejecting strings that
/// merely begin with the prefix but aren't valid hashes.
fn is_valid_argon2_hash(value: &str) -> bool {
    PasswordHash::new(value).is_ok()
}

/// Migrate plaintext SFTP passwords to argon2id hashes.
///
/// This migration runs on startup and is idempotent:
/// - Plaintext passwords are detected by attempting to parse them as valid
///   argon2 hashes — only structurally valid hashes are considered
///   already-migrated.
/// - Already-hashed passwords are left untouched
/// - Running this migration multiple times is safe
///
/// Returns the number of passwords that were migrated.
pub async fn migrate_sftp_passwords(db: &Database) -> Result<usize, AppError> {
    let servers = db.list_servers().await?;
    let mut migrated_count = 0;

    for mut server in servers {
        // Check if the server has an SFTP password that needs migration.
        // We parse the stored value as an argon2 hash; if parsing fails it
        // is plaintext and must be hashed.
        let needs_migration = matches!(
            &server.config.sftp_password,
            Some(password) if !password.is_empty() && !is_valid_argon2_hash(password)
        );

        if needs_migration {
            // Hash the plaintext password
            let plaintext = server.config.sftp_password.as_ref().unwrap();
            let hashed = hash_password(plaintext)?;

            tracing::debug!(
                "Migrating SFTP password for server '{}' ({})",
                server.config.name,
                server.id
            );

            server.config.sftp_password = Some(hashed);

            // Update the server in the database
            db.update_server(&server).await?;
            migrated_count += 1;
        }
    }

    if migrated_count > 0 {
        tracing::info!(
            "Migrated {} SFTP password(s) from plaintext to argon2id hashes",
            migrated_count
        );
    } else {
        tracing::debug!("No SFTP passwords needed migration");
    }

    Ok(migrated_count)
}
