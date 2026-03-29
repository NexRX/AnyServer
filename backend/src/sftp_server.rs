use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use russh::server::{Auth, Handler, Msg, Server, Session};
use russh::{Channel, ChannelId};
use russh_keys::key::KeyPair;
use uuid::Uuid;

use crate::auth::verify_password;
use crate::AppState;

/// Pre-computed argon2 hash used for constant-time rejection when no
/// server matches, preventing timing side-channel username enumeration.
const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQxMjM0NTY3OA$XWJvwiTqHVwx4WF+dMxgxKTgFLO+x+lLvzPyJvqZD7I";

struct SftpSession {
    state: Arc<AppState>,
    authorized_server_id: Option<Uuid>,
    root_dir: Option<PathBuf>,
    channels: HashMap<ChannelId, Channel<Msg>>,
}

impl SftpSession {
    #[allow(dead_code)]
    fn resolve_path(&self, requested: &str) -> Option<PathBuf> {
        let root = self.root_dir.as_ref()?;

        let cleaned = requested.trim_start_matches('/');
        let candidate = if cleaned.is_empty() {
            root.clone()
        } else {
            root.join(cleaned)
        };

        let canon_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.clone());
        let canon_candidate = std::fs::canonicalize(&candidate).unwrap_or(candidate);

        if canon_candidate.starts_with(&canon_root) {
            Some(canon_candidate)
        } else {
            tracing::warn!(
                "SFTP path traversal blocked: {:?} is outside {:?}",
                canon_candidate,
                canon_root
            );
            None
        }
    }
}

struct SftpServerImpl {
    state: Arc<AppState>,
}

impl Server for SftpServerImpl {
    type Handler = SftpSession;

    fn new_client(&mut self, peer_addr: Option<SocketAddr>) -> Self::Handler {
        tracing::debug!("New SFTP connection from {:?}", peer_addr);
        SftpSession {
            state: Arc::clone(&self.state),
            authorized_server_id: None,
            root_dir: None,
            channels: HashMap::new(),
        }
    }
}

#[async_trait]
impl Handler for SftpSession {
    type Error = anyhow::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        let server = self.state.db.find_server_by_sftp_username(user).await?;

        // Always verify against a real or dummy hash for constant-time rejection.
        let (hash, is_real) = match server {
            Some(ref s) => match &s.config.sftp_password {
                Some(pw) if !pw.is_empty() => (pw.as_str(), true),
                _ => (DUMMY_HASH, false),
            },
            None => (DUMMY_HASH, false),
        };

        let password_ok = match verify_password(password, hash) {
            Ok(valid) => valid,
            Err(e) => {
                if is_real {
                    tracing::warn!(
                        "SFTP auth password verification error for server {}: {}",
                        server.as_ref().unwrap().id,
                        e
                    );
                }
                false
            }
        };

        if is_real && password_ok {
            let server = server.unwrap();
            let server_dir = self.state.server_dir(&server.id);
            if let Err(e) = std::fs::create_dir_all(&server_dir) {
                tracing::error!(
                    "Failed to create server dir {:?} for SFTP: {}",
                    server_dir,
                    e
                );
                return Ok(Auth::Reject {
                    proceed_with_methods: None,
                });
            }

            self.authorized_server_id = Some(server.id);
            self.root_dir = Some(server_dir.clone());

            tracing::info!(
                "SFTP auth success: user='{}' -> server {} ({}) at {:?}",
                user,
                server.config.name,
                server.id,
                server_dir,
            );
            return Ok(Auth::Accept);
        }

        tracing::warn!("SFTP auth failed for user '{}'", user);
        Ok(Auth::Reject {
            proceed_with_methods: None,
        })
    }

    async fn auth_publickey(
        &mut self,
        _user: &str,
        _public_key: &russh_keys::key::PublicKey,
    ) -> Result<Auth, Self::Error> {
        Ok(Auth::Reject {
            proceed_with_methods: None,
        })
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        let id = channel.id();
        self.channels.insert(id, channel);
        Ok(true)
    }

    async fn subsystem_request(
        &mut self,
        channel_id: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if name != "sftp" {
            tracing::debug!("Rejected unknown subsystem request: {}", name);
            session.channel_failure(channel_id);
            return Ok(());
        }

        let server_id = match self.authorized_server_id {
            Some(id) => id,
            None => {
                tracing::warn!("SFTP subsystem requested but session is not authorized");
                session.channel_failure(channel_id);
                return Ok(());
            }
        };

        let root_dir = match &self.root_dir {
            Some(d) => d.clone(),
            None => {
                session.channel_failure(channel_id);
                return Ok(());
            }
        };

        tracing::warn!(
            "SFTP file transfer is not yet implemented. \
             Connection for server {} (channel {:?}, root={:?}) authenticated \
             but subsystem rejected. The client will see a 'subsystem request failed' error.",
            server_id,
            channel_id,
            root_dir,
        );

        session.channel_failure(channel_id);

        Ok(())
    }

    async fn data(
        &mut self,
        channel_id: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::trace!(
            "SFTP data on channel {:?}: {} bytes",
            channel_id,
            data.len()
        );
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        channel_id: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::debug!("Channel {:?} EOF", channel_id);
        self.channels.remove(&channel_id);
        Ok(())
    }

    async fn channel_close(
        &mut self,
        channel_id: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::debug!("Channel {:?} closed", channel_id);
        self.channels.remove(&channel_id);
        Ok(())
    }
}

/// Persisted at `<data_dir>/sftp_host_key` so clients don't see a
/// host-key-changed warning on every restart.
fn load_or_generate_host_key(data_dir: &Path) -> anyhow::Result<KeyPair> {
    let key_path = data_dir.join("sftp_host_key");

    if key_path.exists() {
        match std::fs::read(&key_path) {
            Ok(bytes) => {
                match russh_keys::decode_secret_key(&String::from_utf8_lossy(&bytes), None) {
                    Ok(key) => {
                        tracing::info!("Loaded existing SFTP host key from {}", key_path.display());
                        return Ok(key);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to decode SFTP host key at {} ({}), generating a new one",
                            key_path.display(),
                            e
                        );
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to read SFTP host key at {} ({}), generating a new one",
                    key_path.display(),
                    e
                );
            }
        }
    }

    let key = KeyPair::generate_ed25519();
    match persist_host_key(&key_path, &key) {
        Ok(()) => {
            tracing::info!(
                "Generated and saved new SFTP host key to {}",
                key_path.display()
            );
        }
        Err(e) => {
            tracing::error!(
                "Generated SFTP host key but failed to persist to {}: {}. \
                 Clients will see a host-key-changed warning on next restart.",
                key_path.display(),
                e
            );
        }
    }

    Ok(key)
}

fn persist_host_key(path: &Path, key: &KeyPair) -> anyhow::Result<()> {
    let tmp_path = path.with_extension("tmp");
    {
        let mut f = std::fs::File::create(&tmp_path)?;
        russh_keys::encode_pkcs8_pem(key, &mut f)
            .map_err(|e| anyhow::anyhow!("Failed to encode host key: {}", e))?;
        f.sync_all()?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600));
    }

    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

pub async fn run_sftp_server(state: Arc<AppState>, port: u16) -> anyhow::Result<()> {
    let host_key = load_or_generate_host_key(&state.data_dir)?;

    let config = Arc::new(russh::server::Config {
        auth_rejection_time: std::time::Duration::from_secs(2),
        auth_rejection_time_initial: Some(std::time::Duration::from_secs(0)),
        keys: vec![host_key],
        ..Default::default()
    });

    let addr: SocketAddr = format!("0.0.0.0:{}", port).parse()?;
    tracing::info!("SFTP server binding to {}", addr);

    let mut server = SftpServerImpl {
        state: Arc::clone(&state),
    };

    server.run_on_address(config, addr).await?;

    Ok(())
}
