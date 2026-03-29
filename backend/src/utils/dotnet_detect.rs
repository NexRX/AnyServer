//! .NET runtime detection — scans the host for installed .NET runtimes and SDKs.
//!
//! The detection strategy:
//!
//! 1. Find all `dotnet` binaries on the system via well-known paths and `which`.
//! 2. For each unique `dotnet` binary, run `dotnet --list-runtimes` to enumerate
//!    available runtimes (Microsoft.NETCore.App, Microsoft.AspNetCore.App, etc.).
//! 3. Parse the output to extract runtime name, version, and installation path.
//! 4. Deduplicate by canonical path and return a structured list.
//!
//! Unlike Java (where each JDK is self-contained), .NET uses a single `dotnet`
//! host that can load multiple runtime versions installed side-by-side in a
//! shared location.
//!
//! ## Well-known .NET installation paths:
//!
//! - `/usr/bin/dotnet`, `/usr/local/bin/dotnet`  (system package managers)
//! - `/usr/share/dotnet/dotnet`                   (Microsoft's .deb/.rpm)
//! - `/opt/dotnet/dotnet`                         (manual installs)
//! - `/run/current-system/sw/bin/dotnet`          (NixOS system profile)
//! - `/nix/store/*-dotnet-*/bin/dotnet`           (NixOS store - combined packages)
//! - `~/.dotnet/dotnet`                           (user-local install)
//! - macOS: `/usr/local/share/dotnet/dotnet`
//!
//! ## Environment variable support:
//!
//! For servers that need specific .NET versions, we provide helpers to generate
//! the appropriate environment variables:
//!
//! - `DOTNET_ROOT` — points to the directory containing the `dotnet` host
//! - `DOTNET_BUNDLE_EXTRACT_BASE_DIR` — where single-file apps extract bundled files
//!
//! This is especially important on NixOS where .NET runtimes may be scattered
//! across the Nix store and need explicit paths to function properly.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::types::DotnetRuntime;

/// Discover all .NET runtimes installed on the host.
///
/// This scans for `dotnet` binaries, queries each for installed runtimes,
/// deduplicates by canonical path, and returns a structured list sorted by
/// runtime name and version.
pub fn detect_dotnet_runtimes() -> Vec<DotnetRuntime> {
    let mut seen_installations: HashSet<PathBuf> = HashSet::new();
    let mut runtimes: Vec<DotnetRuntime> = Vec::new();

    // Determine the default `dotnet` on PATH
    let default_canonical = resolve_default_dotnet();

    // Collect all candidate `dotnet` binary paths
    let candidates = collect_candidates();

    for candidate in candidates {
        // Canonicalize to deduplicate symlinks
        let canonical = match std::fs::canonicalize(&candidate) {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Get the installation root (parent directory of the dotnet binary)
        // On NixOS: /nix/store/.../share/dotnet/dotnet -> /nix/store/.../share/dotnet
        // On standard Linux: /usr/share/dotnet/dotnet -> /usr/share/dotnet
        let installation_root = canonical.parent();
        if installation_root.is_none() {
            continue;
        }
        let installation_root = installation_root.unwrap().to_path_buf();

        // Skip if we've already scanned this installation
        if !seen_installations.insert(installation_root.clone()) {
            continue;
        }

        // Query this dotnet installation for all its runtimes
        let is_default = default_canonical
            .as_ref()
            .map(|d| d == &canonical)
            .unwrap_or(false);

        if let Some(detected) = probe_dotnet(&candidate, &installation_root, is_default) {
            runtimes.extend(detected);
        }
    }

    // Sort: default first, then by runtime name, then by version descending
    runtimes.sort_by(|a, b| {
        b.is_default
            .cmp(&a.is_default)
            .then(a.runtime_name.cmp(&b.runtime_name))
            .then(b.version.cmp(&a.version))
    });

    runtimes
}

/// Generate recommended environment variables for a specific .NET runtime.
///
/// Returns a HashMap with:
/// - `DOTNET_ROOT`: Path to the .NET installation root
/// - `DOTNET_BUNDLE_EXTRACT_BASE_DIR`: Suggested extraction path (relative to server dir)
/// - `LD_LIBRARY_PATH`: Library paths including gcc-lib for NixOS compatibility (when needed)
///
/// The caller should merge these with the server's existing env config.
pub fn generate_dotnet_env_vars(
    dotnet_root: &str,
    server_dir: Option<&str>,
) -> HashMap<String, String> {
    let mut env = HashMap::new();

    // Set DOTNET_ROOT to the installation directory
    env.insert("DOTNET_ROOT".to_string(), dotnet_root.to_string());

    // Set a bundle extraction directory - prefer server-local to avoid permission issues
    let extract_dir = if let Some(dir) = server_dir {
        format!("{}/.dotnet_bundle_cache", dir)
    } else {
        "./.dotnet_bundle_cache".to_string()
    };
    env.insert("DOTNET_BUNDLE_EXTRACT_BASE_DIR".to_string(), extract_dir);

    // On NixOS, .NET applications with native dependencies (like TShock) need
    // LD_LIBRARY_PATH to find libstdc++.so.6 and other system libraries.
    // We detect NixOS by checking if the dotnet_root is in /nix/store.
    if dotnet_root.starts_with("/nix/store") {
        // Find gcc-lib in the Nix store to provide libstdc++.so.6
        let mut lib_paths = Vec::new();

        // Add the .NET runtime's own lib directory
        lib_paths.push(dotnet_root.to_string());

        // Try to find gcc-lib paths in the Nix store
        if let Ok(entries) = glob::glob("/nix/store/*-gcc-*-lib/lib") {
            for entry in entries.flatten().take(5) {
                if entry.exists() {
                    lib_paths.push(entry.to_string_lossy().to_string());
                }
            }
        }

        // Also try xgcc (cross-compiler gcc)
        if let Ok(entries) = glob::glob("/nix/store/*-xgcc-*-libgcc/lib") {
            for entry in entries.flatten().take(5) {
                if entry.exists() {
                    lib_paths.push(entry.to_string_lossy().to_string());
                }
            }
        }

        // Add NixOS system library paths
        lib_paths.push("/run/current-system/sw/lib".to_string());

        if !lib_paths.is_empty() {
            env.insert("LD_LIBRARY_PATH".to_string(), lib_paths.join(":"));
        }
    }

    env
}

// ─── Internal helpers ────────────────────────────────────────────────────

/// Resolve the canonical path of the default `dotnet` on PATH.
fn resolve_default_dotnet() -> Option<PathBuf> {
    let output = Command::new("which").arg("dotnet").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if raw.is_empty() {
        return None;
    }
    std::fs::canonicalize(raw).ok()
}

/// Gather all candidate `dotnet` binary paths from known locations.
fn collect_candidates() -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    // 1. Default on PATH
    if let Some(p) = which_dotnet() {
        candidates.push(p);
    }

    // 2. $DOTNET_ROOT/dotnet
    if let Ok(root) = std::env::var("DOTNET_ROOT") {
        let p = PathBuf::from(&root).join("dotnet");
        if p.exists() {
            candidates.push(p);
        }
    }

    // 3. Glob well-known directories
    let glob_patterns: &[&str] = &[
        "/usr/bin/dotnet",
        "/usr/local/bin/dotnet",
        "/usr/share/dotnet/dotnet",
        "/usr/local/share/dotnet/dotnet",
        "/opt/dotnet/dotnet",
        "/opt/dotnet*/dotnet",
        // NixOS system profile
        "/run/current-system/sw/bin/dotnet",
        "/nix/var/nix/profiles/default/bin/dotnet",
        // NixOS store - various .NET packages
        "/nix/store/*-dotnet-sdk-*/bin/dotnet",
        "/nix/store/*-dotnet-runtime-*/bin/dotnet",
        "/nix/store/*-dotnet-runtime-wrapped-*/bin/dotnet",
        "/nix/store/*-dotnet-sdk-wrapped-*/bin/dotnet",
        "/nix/store/*-dotnet-combined*/bin/dotnet",
        "/nix/store/*dotnet*/bin/dotnet",
        // NixOS per-user profiles
        "/nix/var/nix/profiles/per-user/*/bin/dotnet",
        "/home/*/.nix-profile/bin/dotnet",
        // macOS
        "/usr/local/share/dotnet/dotnet",
    ];

    for pattern in glob_patterns {
        // Check if it's a direct path (no glob chars)
        if !pattern.contains('*') {
            let p = PathBuf::from(pattern);
            if p.exists() {
                candidates.push(p);
            }
        } else {
            // Use glob for patterns with wildcards
            if let Ok(entries) = glob::glob(pattern) {
                for entry in entries.flatten() {
                    if entry.exists() {
                        candidates.push(entry);
                    }
                }
            }
        }
    }

    // 4. User-local install (~/.dotnet)
    if let Ok(home) = std::env::var("HOME") {
        let p = PathBuf::from(&home).join(".dotnet/dotnet");
        if p.exists() {
            candidates.push(p);
        }
    }

    candidates
}

/// Locate `dotnet` on the system PATH.
fn which_dotnet() -> Option<PathBuf> {
    let output = Command::new("which").arg("dotnet").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if raw.is_empty() {
        return None;
    }
    let p = PathBuf::from(&raw);
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

/// Run `dotnet --list-runtimes` and parse the output into `DotnetRuntime` entries.
///
/// Example output:
/// ```text
/// Microsoft.NETCore.App 6.0.36 [/usr/share/dotnet/shared/Microsoft.NETCore.App]
/// Microsoft.NETCore.App 8.0.23 [/usr/share/dotnet/shared/Microsoft.NETCore.App]
/// Microsoft.AspNetCore.App 8.0.23 [/usr/share/dotnet/shared/Microsoft.AspNetCore.App]
/// ```
///
/// Each line: `<RuntimeName> <Version> [<Path>]`
fn probe_dotnet(
    dotnet_path: &Path,
    installation_root: &Path,
    is_default: bool,
) -> Option<Vec<DotnetRuntime>> {
    let output = Command::new(dotnet_path)
        .arg("--list-runtimes")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut runtimes = Vec::new();

    for line in text.lines() {
        if let Some(rt) = parse_runtime_line(line, installation_root, is_default) {
            runtimes.push(rt);
        }
    }

    if runtimes.is_empty() {
        None
    } else {
        Some(runtimes)
    }
}

/// Parse a single line from `dotnet --list-runtimes`.
///
/// Format: `Microsoft.NETCore.App 6.0.36 [/path/to/shared/Microsoft.NETCore.App]`
fn parse_runtime_line(
    line: &str,
    installation_root: &Path,
    is_default: bool,
) -> Option<DotnetRuntime> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    // Split into parts: name, version, [path]
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 3 {
        return None;
    }

    let runtime_name = parts[0].to_string();
    let version = parts[1].to_string();

    // Extract the bracketed path
    let path_part = parts[2..].join(" ");
    let runtime_path = if path_part.starts_with('[') && path_part.ends_with(']') {
        path_part[1..path_part.len() - 1].to_string()
    } else {
        // Fallback if format is unexpected
        path_part
    };

    // Parse major version from version string (e.g., "6.0.36" -> 6)
    let major_version = parse_major_version(&version);

    Some(DotnetRuntime {
        runtime_name,
        version,
        major_version,
        runtime_path,
        installation_root: installation_root.to_string_lossy().to_string(),
        is_default,
    })
}

/// Extract the major version number from a .NET version string.
///
/// Examples:
/// - `"6.0.36"` → `6`
/// - `"8.0.23"` → `8`
/// - `"9.0.0-preview.1"` → `9`
fn parse_major_version(version: &str) -> u32 {
    version
        .split('.')
        .next()
        .and_then(|s| {
            // Handle preview versions like "9.0.0-preview.1"
            s.split('-').next()?.parse().ok()
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_runtime_line_netcore() {
        let line = "Microsoft.NETCore.App 6.0.36 [/usr/share/dotnet/shared/Microsoft.NETCore.App]";
        let rt = parse_runtime_line(line, Path::new("/usr/share/dotnet"), false).unwrap();

        assert_eq!(rt.runtime_name, "Microsoft.NETCore.App");
        assert_eq!(rt.version, "6.0.36");
        assert_eq!(rt.major_version, 6);
        assert_eq!(
            rt.runtime_path,
            "/usr/share/dotnet/shared/Microsoft.NETCore.App"
        );
        assert!(!rt.is_default);
    }

    #[test]
    fn test_parse_runtime_line_aspnetcore() {
        let line =
            "Microsoft.AspNetCore.App 8.0.23 [/usr/share/dotnet/shared/Microsoft.AspNetCore.App]";
        let rt = parse_runtime_line(line, Path::new("/usr/share/dotnet"), true).unwrap();

        assert_eq!(rt.runtime_name, "Microsoft.AspNetCore.App");
        assert_eq!(rt.version, "8.0.23");
        assert_eq!(rt.major_version, 8);
        assert!(rt.is_default);
    }

    #[test]
    fn test_parse_major_version() {
        assert_eq!(parse_major_version("6.0.36"), 6);
        assert_eq!(parse_major_version("8.0.23"), 8);
        assert_eq!(parse_major_version("9.0.0"), 9);
        assert_eq!(parse_major_version("9.0.0-preview.1"), 9);
        assert_eq!(parse_major_version("7.0.0-rc.2.22472.3"), 7);
    }

    #[test]
    fn test_generate_dotnet_env_vars() {
        let env = generate_dotnet_env_vars("/usr/share/dotnet", Some("/app/server123"));

        assert_eq!(env.get("DOTNET_ROOT").unwrap(), "/usr/share/dotnet");
        assert_eq!(
            env.get("DOTNET_BUNDLE_EXTRACT_BASE_DIR").unwrap(),
            "/app/server123/.dotnet_bundle_cache"
        );
    }

    #[test]
    fn test_generate_dotnet_env_vars_no_server_dir() {
        let env = generate_dotnet_env_vars("/opt/dotnet", None);

        assert_eq!(env.get("DOTNET_ROOT").unwrap(), "/opt/dotnet");
        assert_eq!(
            env.get("DOTNET_BUNDLE_EXTRACT_BASE_DIR").unwrap(),
            "./.dotnet_bundle_cache"
        );
    }

    /// Live integration test — actually runs detection on the host.
    ///
    /// Run with:
    ///   cargo test test_detect_live -- --nocapture
    #[test]
    fn test_detect_live() {
        let runtimes = detect_dotnet_runtimes();
        println!("\n=== Detected {} .NET runtime(s) ===", runtimes.len());
        for rt in &runtimes {
            println!(
                "  .NET {} — {} v{}{} — {}",
                rt.major_version,
                rt.runtime_name,
                rt.version,
                if rt.is_default { " ★ DEFAULT" } else { "" },
                rt.runtime_path,
            );
            println!("    Installation: {}", rt.installation_root);
        }
        println!("=== end ===\n");

        // If dotnet is on PATH, we should find at least one runtime
        if which_dotnet().is_some() {
            assert!(
                !runtimes.is_empty(),
                "dotnet is on PATH but detect_dotnet_runtimes() found nothing"
            );
        }
    }
}
