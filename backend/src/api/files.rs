use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path as AxumPath, Query, State};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::AppError;
use crate::types::*;
use crate::utils::blocking;
use crate::AppState;

fn get_unix_mode(meta: &std::fs::Metadata) -> Option<u32> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        Some(meta.permissions().mode())
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        None
    }
}

#[cfg(unix)]
fn get_uid_gid(meta: &std::fs::Metadata) -> (u32, u32) {
    use std::os::unix::fs::MetadataExt;
    (meta.uid(), meta.gid())
}

#[cfg(unix)]
fn uid_to_name(uid: u32) -> Option<String> {
    unsafe {
        let pw = libc::getpwuid(uid);
        if pw.is_null() {
            None
        } else {
            let name = std::ffi::CStr::from_ptr((*pw).pw_name);
            Some(name.to_string_lossy().into_owned())
        }
    }
}

#[cfg(unix)]
fn gid_to_name(gid: u32) -> Option<String> {
    unsafe {
        let gr = libc::getgrgid(gid);
        if gr.is_null() {
            None
        } else {
            let name = std::ffi::CStr::from_ptr((*gr).gr_name);
            Some(name.to_string_lossy().into_owned())
        }
    }
}

#[derive(Deserialize)]
pub struct FileQuery {
    pub path: Option<String>,
}

fn resolve_safe_path(
    state: &AppState,
    server_id: &Uuid,
    rel_path: &str,
) -> Result<PathBuf, AppError> {
    let server_dir = state.server_dir(server_id);

    std::fs::create_dir_all(&server_dir)
        .map_err(|e| AppError::Internal(format!("Failed to create server directory: {}", e)))?;

    let cleaned = rel_path.trim_start_matches('/');
    let candidate = if cleaned.is_empty() {
        server_dir.clone()
    } else {
        server_dir.join(cleaned)
    };

    let canon_root = std::fs::canonicalize(&server_dir).unwrap_or_else(|_| server_dir.clone());
    let canon_candidate = std::fs::canonicalize(&candidate).unwrap_or_else(|_| {
        // Target might not exist yet — walk up to a real parent.
        let mut existing = candidate.clone();
        let mut remainder = Vec::new();
        while !existing.exists() {
            if let Some(file_name) = existing.file_name() {
                remainder.push(file_name.to_os_string());
            }
            if !existing.pop() {
                break;
            }
        }
        let mut base = std::fs::canonicalize(&existing).unwrap_or(existing);
        for segment in remainder.into_iter().rev() {
            base.push(segment);
        }
        base
    });

    if !canon_candidate.starts_with(&canon_root) {
        return Err(AppError::BadRequest(
            "Path traversal detected — access denied".into(),
        ));
    }

    Ok(canon_candidate)
}

/// GET /api/servers/:id/files
pub async fn list_files(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    AxumPath(id): AxumPath<Uuid>,
    Query(query): Query<FileQuery>,
) -> Result<Json<FileListResponse>, AppError> {
    let server = state.db.require_server(id).await?;

    auth.require_level(&state, &server, PermissionLevel::Viewer)
        .await?;

    let rel_path = query.path.unwrap_or_default();
    let state_clone = Arc::clone(&state);

    let (entries, rel_path): (Vec<FileEntry>, String) = blocking(move || {
        let dir_path = resolve_safe_path(&state_clone, &id, &rel_path)?;

        if !dir_path.is_dir() {
            return Err(AppError::BadRequest(format!(
                "Path '{}' is not a directory",
                rel_path
            )));
        }

        let server_dir = state_clone.server_dir(&id);
        let canon_server_dir =
            std::fs::canonicalize(&server_dir).unwrap_or_else(|_| server_dir.clone());

        let mut entries = Vec::new();

        for entry in std::fs::read_dir(&dir_path)? {
            let entry = entry?;
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue, // skip entries we can't stat
            };

            let name = entry.file_name().to_string_lossy().to_string();
            let full_path = entry.path();
            let relative = full_path
                .strip_prefix(&canon_server_dir)
                .unwrap_or(&full_path)
                .to_string_lossy()
                .to_string();

            let modified: Option<DateTime<Utc>> = meta.modified().ok().map(DateTime::<Utc>::from);

            let mode = get_unix_mode(&meta).map(mode_to_octal_string);

            entries.push(FileEntry {
                name,
                path: relative,
                kind: if meta.is_dir() {
                    FileEntryKind::Directory
                } else {
                    FileEntryKind::File
                },
                size: meta.len(),
                modified,
                mode,
            });
        }

        entries.sort_by(|a, b| match (&a.kind, &b.kind) {
            (FileEntryKind::Directory, FileEntryKind::File) => std::cmp::Ordering::Less,
            (FileEntryKind::File, FileEntryKind::Directory) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });

        Ok((entries, rel_path))
    })
    .await?;

    Ok(Json(FileListResponse {
        path: rel_path,
        entries,
    }))
}

/// GET /api/servers/:id/files/read
pub async fn read_file(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    AxumPath(id): AxumPath<Uuid>,
    Query(query): Query<FileQuery>,
) -> Result<Json<FileContentResponse>, AppError> {
    let server = state.db.require_server(id).await?;

    auth.require_level(&state, &server, PermissionLevel::Viewer)
        .await?;

    let rel_path = query
        .path
        .ok_or_else(|| AppError::BadRequest("Query parameter 'path' is required".into()))?;

    if rel_path.is_empty() {
        return Err(AppError::BadRequest("path must not be empty".into()));
    }

    let state_clone = Arc::clone(&state);

    blocking(move || {
        let file_path = resolve_safe_path(&state_clone, &id, &rel_path)?;

        if !file_path.exists() {
            return Err(AppError::NotFound(format!("File not found: {}", rel_path)));
        }

        if !file_path.is_file() {
            return Err(AppError::BadRequest(format!(
                "'{}' is not a regular file",
                rel_path
            )));
        }

        let meta = std::fs::metadata(&file_path)?;
        const MAX_READ_SIZE: u64 = 10 * 1024 * 1024; // 10 MB
        if meta.len() > MAX_READ_SIZE {
            return Err(AppError::BadRequest(format!(
                "File is too large to read via the API ({} bytes, max {} bytes)",
                meta.len(),
                MAX_READ_SIZE,
            )));
        }

        let content = std::fs::read_to_string(&file_path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::InvalidData {
                AppError::BadRequest("File is not valid UTF-8 text".into())
            } else {
                AppError::from(e)
            }
        })?;

        Ok(Json(FileContentResponse {
            path: rel_path,
            content,
            size: meta.len(),
        }))
    })
    .await
}

/// POST /api/servers/:id/files/write
pub async fn write_file(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    AxumPath(id): AxumPath<Uuid>,
    Json(req): Json<WriteFileRequest>,
) -> Result<Json<WriteFileResponse>, AppError> {
    let server = state.db.require_server(id).await?;

    auth.require_level(&state, &server, PermissionLevel::Manager)
        .await?;

    if req.path.is_empty() {
        return Err(AppError::BadRequest("path must not be empty".into()));
    }

    const MAX_WRITE_SIZE: usize = 10 * 1024 * 1024; // 10 MB
    if req.content.len() > MAX_WRITE_SIZE {
        return Err(AppError::BadRequest(format!(
            "File content is too large ({} bytes, max {} bytes). Use SFTP for large file transfers.",
            req.content.len(),
            MAX_WRITE_SIZE,
        )));
    }

    let state_clone = Arc::clone(&state);

    blocking(move || {
        let file_path = resolve_safe_path(&state_clone, &id, &req.path)?;

        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content_len = req.content.len();
        std::fs::write(&file_path, &req.content)?;

        tracing::debug!("Wrote {} bytes to {:?}", content_len, file_path);

        Ok(Json(WriteFileResponse {
            written: true,
            path: req.path,
            size: content_len,
        }))
    })
    .await
}

/// POST /api/servers/:id/files/mkdir
pub async fn create_dir(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    AxumPath(id): AxumPath<Uuid>,
    Json(req): Json<CreateDirRequest>,
) -> Result<Json<CreateDirResponse>, AppError> {
    let server = state.db.require_server(id).await?;

    auth.require_level(&state, &server, PermissionLevel::Manager)
        .await?;

    if req.path.is_empty() {
        return Err(AppError::BadRequest("path must not be empty".into()));
    }

    let state_clone = Arc::clone(&state);

    blocking(move || {
        let dir_path = resolve_safe_path(&state_clone, &id, &req.path)?;
        std::fs::create_dir_all(&dir_path)?;

        tracing::debug!("Created directory {:?}", dir_path);

        Ok(Json(CreateDirResponse {
            created: true,
            path: req.path,
        }))
    })
    .await
}

/// POST /api/servers/:id/files/delete
pub async fn delete_path(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    AxumPath(id): AxumPath<Uuid>,
    Json(req): Json<DeleteRequest>,
) -> Result<Json<DeletePathResponse>, AppError> {
    let server = state.db.require_server(id).await?;

    auth.require_level(&state, &server, PermissionLevel::Manager)
        .await?;

    if req.path.is_empty() {
        return Err(AppError::BadRequest(
            "path must not be empty (refusing to delete server root)".into(),
        ));
    }

    let state_clone = Arc::clone(&state);

    blocking(move || {
        let target = resolve_safe_path(&state_clone, &id, &req.path)?;

        let server_dir = state_clone.server_dir(&id);
        let canon_root = std::fs::canonicalize(&server_dir).unwrap_or(server_dir);
        if target == canon_root {
            return Err(AppError::BadRequest(
                "Cannot delete the server root directory".into(),
            ));
        }

        if !target.exists() {
            return Err(AppError::NotFound(format!("Path not found: {}", req.path)));
        }

        if target.is_dir() {
            std::fs::remove_dir_all(&target)?;
            tracing::debug!("Deleted directory {:?}", target);
        } else {
            std::fs::remove_file(&target)?;
            tracing::debug!("Deleted file {:?}", target);
        }

        Ok(Json(DeletePathResponse {
            deleted: true,
            path: req.path,
        }))
    })
    .await
}

/// GET /api/servers/:id/files/permissions
pub async fn get_permissions(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    AxumPath(id): AxumPath<Uuid>,
    Query(query): Query<FileQuery>,
) -> Result<Json<FilePermissionsResponse>, AppError> {
    let server = state.db.require_server(id).await?;

    auth.require_level(&state, &server, PermissionLevel::Viewer)
        .await?;

    let rel_path = query
        .path
        .ok_or_else(|| AppError::BadRequest("Query parameter 'path' is required".into()))?;

    if rel_path.is_empty() {
        return Err(AppError::BadRequest("path must not be empty".into()));
    }

    let state_clone = Arc::clone(&state);

    blocking(move || {
        let file_path = resolve_safe_path(&state_clone, &id, &rel_path)?;

        if !file_path.exists() {
            return Err(AppError::NotFound(format!("Path not found: {}", rel_path)));
        }

        let meta = std::fs::metadata(&file_path)?;

        #[cfg(unix)]
        {
            let raw_mode = get_unix_mode(&meta).unwrap_or(0);
            let (uid, gid) = get_uid_gid(&meta);
            let owner = uid_to_name(uid);
            let group = gid_to_name(gid);

            Ok(Json(FilePermissionsResponse {
                path: rel_path,
                mode: mode_to_octal_string(raw_mode),
                mode_display: mode_to_rwx_string(raw_mode),
                is_directory: meta.is_dir(),
                uid,
                gid,
                owner,
                group,
            }))
        }

        #[cfg(not(unix))]
        {
            Ok(Json(FilePermissionsResponse {
                path: rel_path,
                mode: "0".to_string(),
                mode_display: "---------".to_string(),
                is_directory: meta.is_dir(),
                uid: 0,
                gid: 0,
                owner: None,
                group: None,
            }))
        }
    })
    .await
}

/// POST /api/servers/:id/files/chmod
pub async fn chmod(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    AxumPath(id): AxumPath<Uuid>,
    Json(req): Json<ChmodRequest>,
) -> Result<Json<ChmodResponse>, AppError> {
    let server = state.db.require_server(id).await?;

    auth.require_level(&state, &server, PermissionLevel::Manager)
        .await?;

    if req.path.is_empty() {
        return Err(AppError::BadRequest("path must not be empty".into()));
    }

    let mode_val = parse_octal_mode(&req.mode).ok_or_else(|| {
        AppError::BadRequest(format!(
            "Invalid octal mode '{}'. Use 3 or 4 octal digits, e.g. '755', '0644'.",
            req.mode
        ))
    })?;

    let state_clone = Arc::clone(&state);

    blocking(move || {
        let file_path = resolve_safe_path(&state_clone, &id, &req.path)?;

        if !file_path.exists() {
            return Err(AppError::NotFound(format!("Path not found: {}", req.path)));
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(mode_val);
            std::fs::set_permissions(&file_path, perms).map_err(|e| {
                AppError::Internal(format!(
                    "Failed to set permissions on '{}': {}",
                    req.path, e
                ))
            })?;

            tracing::debug!(
                "Set permissions on {:?} to {:o} ({})",
                file_path,
                mode_val,
                mode_to_rwx_string(mode_val),
            );
        }

        #[cfg(not(unix))]
        {
            let _ = mode_val;
            return Err(AppError::BadRequest(
                "Setting file permissions is only supported on Unix systems".into(),
            ));
        }

        Ok(Json(ChmodResponse {
            path: req.path,
            mode: mode_to_octal_string(mode_val),
            mode_display: mode_to_rwx_string(mode_val),
        }))
    })
    .await
}
