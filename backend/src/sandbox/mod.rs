//! Process-level sandboxing for managed server processes.
//!
//! Layers: Landlock (5.13+), NO_NEW_PRIVS, FD cleanup, RLIMIT_NPROC.
//! Each layer is runtime-detected and gracefully skipped if unavailable.
//! On non-Linux platforms, `PreExecSandbox::apply` is a no-op.

#[cfg(target_os = "linux")]
mod hardening;
#[cfg(target_os = "linux")]
pub mod landlock;
#[cfg(target_os = "linux")]
pub mod namespaces;

use std::path::Path;

use crate::types::IsolationConfig;

/// Pre-computed sandbox config. All heap allocation happens in [`Self::new`];
/// [`Self::apply`] uses only async-signal-safe syscalls (safe for `pre_exec`).
pub struct PreExecSandbox {
    enabled: bool,

    #[cfg(target_os = "linux")]
    rw_paths: Vec<std::ffi::CString>,

    #[cfg(target_os = "linux")]
    ro_paths: Vec<std::ffi::CString>,

    #[cfg(target_os = "linux")]
    pids_max: Option<u64>,
}

#[cfg(target_os = "linux")]
const DEFAULT_RO_PATHS: &[&str] = &[
    "/usr",
    "/lib",
    "/lib64",
    "/lib32",
    "/etc",
    "/bin",
    "/sbin",
    "/dev",
    "/proc",
    "/sys",
    "/run",
    "/nix",
    "/var/lib/alternatives",
    "/var/lib/dpkg",
    "/opt",
    "/snap",
];

/// `/tmp` is intentionally omitted — add it to `extra_rw_paths` per-server if needed.
#[cfg(target_os = "linux")]
const DEFAULT_RW_PATHS: &[&str] = &[
    "/dev/null",
    "/dev/zero",
    "/dev/full",
    "/dev/random",
    "/dev/urandom",
    "/dev/tty",
    "/dev/shm",
];

impl PreExecSandbox {
    pub fn new(server_dir: &Path, config: &IsolationConfig) -> Self {
        #[cfg(target_os = "linux")]
        {
            if !config.enabled {
                return Self {
                    enabled: false,
                    rw_paths: Vec::new(),
                    ro_paths: Vec::new(),
                    pids_max: None,
                };
            }

            let to_cstring = |p: &str| -> Option<std::ffi::CString> {
                // Only include paths that actually exist on this host.
                if std::path::Path::new(p).exists() {
                    std::ffi::CString::new(p).ok()
                } else {
                    None
                }
            };

            // Read-write paths: server dir + defaults + user extras.
            let mut rw_paths: Vec<std::ffi::CString> = Vec::new();
            if let Ok(cs) = std::ffi::CString::new(server_dir.to_string_lossy().as_ref()) {
                rw_paths.push(cs);
            }
            for p in DEFAULT_RW_PATHS {
                if let Some(cs) = to_cstring(p) {
                    rw_paths.push(cs);
                }
            }
            for p in &config.extra_rw_paths {
                if let Some(cs) = to_cstring(p) {
                    rw_paths.push(cs);
                }
            }

            // Read-only paths: defaults + user extras.
            let mut ro_paths: Vec<std::ffi::CString> = Vec::new();
            for p in DEFAULT_RO_PATHS {
                if let Some(cs) = to_cstring(p) {
                    ro_paths.push(cs);
                }
            }
            for p in &config.extra_read_paths {
                if let Some(cs) = to_cstring(p) {
                    ro_paths.push(cs);
                }
            }

            Self {
                enabled: true,
                rw_paths,
                ro_paths,
                pids_max: config.pids_max,
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = (server_dir, config);
            Self {
                enabled: config.enabled,
            }
        }
    }

    pub fn apply(&self) -> std::io::Result<()> {
        if !self.enabled {
            return Ok(());
        }

        #[cfg(target_os = "linux")]
        {
            hardening::close_inherited_fds();

            match landlock::apply_landlock(&self.rw_paths, &self.ro_paths) {
                Ok(()) => {}
                Err(e) => {
                    let msg = format!("[anyserver sandbox] Landlock not applied: {}\n", e,);
                    unsafe {
                        libc::write(2, msg.as_ptr() as *const libc::c_void, msg.len());
                    }
                }
            }

            hardening::apply_no_new_privs();

            if let Some(limit) = self.pids_max {
                hardening::set_nproc_limit(limit);
            }

            hardening::set_non_dumpable();
        }

        Ok(())
    }
}

pub fn probe_capabilities() -> String {
    #[cfg(target_os = "linux")]
    {
        let ll_version = landlock::probe_abi_version();
        let ll_status = match ll_version {
            Some(v) => format!("✓ ABI v{}", v),
            None => "✗ not available".to_string(),
        };
        let ns_status = if namespaces::probe_namespace_support() {
            "✓ available".to_string()
        } else {
            "✗ not available".to_string()
        };
        format!(
            "Isolation capabilities:\n  \
             Landlock:       {}\n  \
             Namespaces:     {}\n  \
             NO_NEW_PRIVS:   ✓ always available\n  \
             FD cleanup:     ✓ always available\n  \
             RLIMIT_NPROC:   ✓ always available",
            ll_status, ns_status,
        )
    }

    #[cfg(not(target_os = "linux"))]
    {
        "Isolation capabilities: none (non-Linux platform)".to_string()
    }
}
