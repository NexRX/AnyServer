//! Java runtime detection — scans the host for installed JDKs/JREs.
//!
//! The detection strategy mirrors what a developer would do manually:
//!
//! 1. Resolve the default `java` on `$PATH` via `which java` + readlink.
//! 2. Check `$JAVA_HOME/bin/java` if the variable is set.
//! 3. Scan well-known directories where package managers install JDKs:
//!    - `/usr/lib/jvm/*/bin/java`          (Debian, Ubuntu, Fedora, Arch)
//!    - `/usr/java/*/bin/java`             (Oracle RPM, legacy RHEL)
//!    - `/usr/local/lib/jvm/*/bin/java`    (manual installs)
//!    - `/opt/java/*/bin/java`             (manual / SDKMAN-style)
//!    - `/opt/jdk*/bin/java`               (manual tarballs)
//!    - `~/.sdkman/candidates/java/*/bin/java`
//!    - `/run/current-system/sw/bin/java`  (NixOS system profile)
//!    - `/nix/var/nix/profiles/default/bin/java`
//!    - macOS: `/Library/Java/JavaVirtualMachines/*/Contents/Home/bin/java`
//!
//! For each candidate binary we run `<path> -version` (stderr), parse the
//! version string and runtime name, then deduplicate by canonical path.

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::types::JavaRuntime;

/// Discover all Java runtimes installed on the host.
///
/// Results are deduplicated by canonical (symlink-resolved) path and
/// sorted by major version descending so the newest JDK appears first.
pub fn detect_java_runtimes() -> Vec<JavaRuntime> {
    let mut seen_paths: HashSet<PathBuf> = HashSet::new();
    let mut runtimes: Vec<JavaRuntime> = Vec::new();

    // ── Determine the default `java` on PATH ──
    let default_canonical = resolve_default_java();

    // ── Collect candidate paths ──
    let candidates = collect_candidates();

    for candidate in candidates {
        let canonical = match std::fs::canonicalize(&candidate) {
            Ok(p) => p,
            Err(_) => continue,
        };

        if !seen_paths.insert(canonical.clone()) {
            continue; // already processed this physical binary
        }

        if let Some(mut rt) = probe_java(&candidate) {
            rt.is_default = default_canonical
                .as_ref()
                .map(|d| d == &canonical)
                .unwrap_or(false);
            runtimes.push(rt);
        }
    }

    // Sort: default first, then by major_version descending, then by path.
    runtimes.sort_by(|a, b| {
        b.is_default
            .cmp(&a.is_default)
            .then(b.major_version.cmp(&a.major_version))
            .then(a.path.cmp(&b.path))
    });

    runtimes
}

/// Generate recommended environment variables for a specific Java runtime.
///
/// Given a `java_home` directory (e.g. `/usr/lib/jvm/java-21`), returns a
/// `HashMap` with:
/// - `JAVA_HOME` — the JDK/JRE root directory
///
/// The caller should merge these with the server's existing env config.
/// At spawn time the backend automatically prepends `$JAVA_HOME/bin` to
/// `PATH`, so shell scripts that invoke bare `java` will pick up the
/// selected runtime without manual PATH edits.
pub fn generate_java_env_vars(java_home: &str) -> HashMap<String, String> {
    let mut env = HashMap::new();
    env.insert("JAVA_HOME".to_string(), java_home.to_string());
    env
}

// ─── Internal helpers ────────────────────────────────────────────────────

/// Resolve the canonical path of whichever `java` the system PATH points to.
fn resolve_default_java() -> Option<PathBuf> {
    // Try `which java` first.
    let output = Command::new("which").arg("java").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if raw.is_empty() {
        return None;
    }
    std::fs::canonicalize(raw).ok()
}

/// Gather all candidate `java` binary paths from known locations.
fn collect_candidates() -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    // 1. Default on PATH
    if let Some(p) = which_java() {
        candidates.push(p);
    }

    // 2. $JAVA_HOME/bin/java
    if let Ok(home) = std::env::var("JAVA_HOME") {
        let p = PathBuf::from(&home).join("bin/java");
        if p.exists() {
            candidates.push(p);
        }
    }

    // 3. Glob well-known directories
    let glob_patterns: &[&str] = &[
        "/usr/lib/jvm/*/bin/java",
        "/usr/lib64/jvm/*/bin/java",
        "/usr/java/*/bin/java",
        "/usr/local/lib/jvm/*/bin/java",
        "/opt/java/*/bin/java",
        "/opt/jdk*/bin/java",
        "/opt/openjdk*/bin/java",
        // NixOS system profiles
        "/run/current-system/sw/bin/java",
        "/nix/var/nix/profiles/default/bin/java",
        // NixOS store — JDK/JRE packages use naming conventions like:
        //   /nix/store/<hash>-zulu-ca-jdk-21.0.8/bin/java
        //   /nix/store/<hash>-openjdk-21.0.2/bin/java
        //   /nix/store/<hash>-graalvm-ce-21.0.2/bin/java
        // We glob for common patterns.  The .drv files won't have bin/java
        // so they're naturally filtered by the exists() check.
        "/nix/store/*-jdk-*/bin/java",
        "/nix/store/*-jdk*/bin/java",
        "/nix/store/*-openjdk-*/bin/java",
        "/nix/store/*-openjdk*/bin/java",
        "/nix/store/*-zulu-*/bin/java",
        "/nix/store/*-graalvm-*/bin/java",
        "/nix/store/*-graalvm*/bin/java",
        "/nix/store/*-temurin-*/bin/java",
        "/nix/store/*-corretto-*/bin/java",
        // NixOS per-user profiles
        "/nix/var/nix/profiles/per-user/*/bin/java",
        "/home/*/.nix-profile/bin/java",
        // macOS
        "/Library/Java/JavaVirtualMachines/*/Contents/Home/bin/java",
    ];

    for pattern in glob_patterns {
        if let Ok(entries) = glob::glob(pattern) {
            for entry in entries.flatten() {
                if entry.exists() {
                    candidates.push(entry);
                }
            }
        }
    }

    // 4. SDKMAN (user-local)
    if let Ok(home) = std::env::var("HOME") {
        let sdkman_pattern = format!("{}/.sdkman/candidates/java/*/bin/java", home);
        if let Ok(entries) = glob::glob(&sdkman_pattern) {
            for entry in entries.flatten() {
                if entry.exists() {
                    candidates.push(entry);
                }
            }
        }
    }

    // 5. Direct well-known absolute paths (non-glob)
    let direct_paths: &[&str] = &["/usr/bin/java", "/usr/local/bin/java", "/bin/java"];
    for p in direct_paths {
        let path = PathBuf::from(p);
        if path.exists() {
            candidates.push(path);
        }
    }

    candidates
}

/// Locate `java` on the system PATH, returning the (possibly symlinked) path.
fn which_java() -> Option<PathBuf> {
    let output = Command::new("which").arg("java").output().ok()?;
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

/// Run `<java_path> -version` and parse the output into a `JavaRuntime`.
///
/// `java -version` writes to **stderr** (not stdout). Typical output:
///
/// ```text
/// openjdk version "21.0.2" 2024-01-16
/// OpenJDK Runtime Environment (build 21.0.2+13-58)
/// OpenJDK 64-Bit Server VM (build 21.0.2+13-58, mixed mode, sharing)
/// ```
fn probe_java(java_path: &Path) -> Option<JavaRuntime> {
    let output = Command::new(java_path)
        .arg("-version")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .ok()?;

    // java -version outputs to stderr
    let text = if !output.stderr.is_empty() {
        String::from_utf8_lossy(&output.stderr).to_string()
    } else {
        String::from_utf8_lossy(&output.stdout).to_string()
    };

    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return None;
    }

    // ── Parse version from first line ──
    // Formats:
    //   openjdk version "21.0.2" 2024-01-16
    //   java version "1.8.0_392"
    //   openjdk version "17.0.9" 2023-10-17
    let version = parse_version_string(lines[0])?;
    let major = parse_major_version(&version);

    // ── Parse runtime name from second line ──
    let runtime_name = if lines.len() > 1 {
        parse_runtime_name(lines[1])
    } else {
        "Unknown Java Runtime".to_string()
    };

    // Use the provided path (not canonicalized) for display, so the user
    // sees the path they'd actually type.  We still need the canonical
    // path, but the caller handles dedup with that separately.
    let display_path = java_path.to_string_lossy().to_string();

    // Derive JAVA_HOME by stripping the trailing `/bin/java` (or `\bin\java`
    // on Windows) from the binary path.  If the path doesn't end with that
    // suffix (unusual, but possible), fall back to the parent of the parent.
    let java_home = java_path
        .parent() // .../bin
        .and_then(|bin_dir| {
            if bin_dir.file_name().map(|n| n == "bin").unwrap_or(false) {
                bin_dir.parent() // .../jdk-21
            } else {
                None
            }
        })
        .unwrap_or(java_path)
        .to_string_lossy()
        .to_string();

    Some(JavaRuntime {
        path: display_path,
        java_home,
        version,
        major_version: major,
        runtime_name,
        is_default: false, // caller sets this
    })
}

/// Extract the quoted version string from a `java -version` first line.
///
/// Examples:
/// - `openjdk version "21.0.2" 2024-01-16` → `"21.0.2"`
/// - `java version "1.8.0_392"` → `"1.8.0_392"`
fn parse_version_string(line: &str) -> Option<String> {
    // Try to find a quoted version string first
    if let Some(start) = line.find('"') {
        if let Some(end) = line[start + 1..].find('"') {
            return Some(line[start + 1..start + 1 + end].to_string());
        }
    }

    // Fallback: look for something that looks like a version number
    for word in line.split_whitespace() {
        let trimmed = word.trim_matches('"');
        if trimmed.contains('.')
            && trimmed
                .chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
        {
            return Some(trimmed.to_string());
        }
    }

    None
}

/// Extract the major version number from a Java version string.
///
/// - `"21.0.2"` → `21`
/// - `"1.8.0_392"` → `8`  (the old `1.x` scheme maps to major `x`)
/// - `"17.0.9"` → `17`
fn parse_major_version(version: &str) -> u32 {
    let first_part = version.split('.').next().unwrap_or("0");
    let first: u32 = first_part.parse().unwrap_or(0);

    if first == 1 {
        // Old-style: 1.8.0_xxx → major is 8
        version
            .split('.')
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(first)
    } else {
        first
    }
}

/// Extract a human-friendly runtime name from the second line of `java -version`.
///
/// Example input: `OpenJDK Runtime Environment (build 21.0.2+13-58)`
/// Output: `"OpenJDK Runtime Environment"`
fn parse_runtime_name(line: &str) -> String {
    let trimmed = line.trim();

    // Strip the "(build ...)" suffix if present.
    if let Some(paren_pos) = trimmed.find('(') {
        let name = trimmed[..paren_pos].trim();
        if !name.is_empty() {
            return name.to_string();
        }
    }

    // Fallback: return the whole line trimmed.
    if trimmed.is_empty() {
        "Unknown Java Runtime".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version_openjdk_21() {
        let v = parse_version_string(r#"openjdk version "21.0.2" 2024-01-16"#);
        assert_eq!(v, Some("21.0.2".into()));
    }

    #[test]
    fn test_parse_version_java_8() {
        let v = parse_version_string(r#"java version "1.8.0_392""#);
        assert_eq!(v, Some("1.8.0_392".into()));
    }

    #[test]
    fn test_parse_version_openjdk_17() {
        let v = parse_version_string(r#"openjdk version "17.0.9" 2023-10-17"#);
        assert_eq!(v, Some("17.0.9".into()));
    }

    #[test]
    fn test_parse_major_version_21() {
        assert_eq!(parse_major_version("21.0.2"), 21);
    }

    #[test]
    fn test_parse_major_version_8_old_scheme() {
        assert_eq!(parse_major_version("1.8.0_392"), 8);
    }

    #[test]
    fn test_parse_major_version_17() {
        assert_eq!(parse_major_version("17.0.9"), 17);
    }

    #[test]
    fn test_parse_major_version_11() {
        assert_eq!(parse_major_version("11.0.21"), 11);
    }

    #[test]
    fn test_parse_runtime_name_openjdk() {
        let name = parse_runtime_name("OpenJDK Runtime Environment (build 21.0.2+13-58)");
        assert_eq!(name, "OpenJDK Runtime Environment");
    }

    #[test]
    fn test_parse_runtime_name_graalvm() {
        let name = parse_runtime_name("GraalVM CE 21.0.2+13.1 (build 21.0.2+13-jvmci-23.1-b30)");
        assert_eq!(name, "GraalVM CE 21.0.2+13.1");
    }

    #[test]
    fn test_parse_runtime_name_empty() {
        assert_eq!(parse_runtime_name(""), "Unknown Java Runtime");
    }

    #[test]
    fn test_parse_runtime_name_no_parens() {
        assert_eq!(
            parse_runtime_name("Some Custom Runtime"),
            "Some Custom Runtime"
        );
    }

    /// Live integration test — actually runs detection on the host and
    /// prints every runtime found.  Useful for verifying NixOS / SDKMAN /
    /// multi-JDK setups.  Run with:
    ///
    ///   cargo test test_detect_live -- --nocapture
    #[test]
    fn test_detect_live() {
        let runtimes = detect_java_runtimes();
        println!("\n=== Detected {} Java runtime(s) ===", runtimes.len());
        for rt in &runtimes {
            println!(
                "  Java {} ({}){} — {} — {}",
                rt.major_version,
                rt.version,
                if rt.is_default { " ★ DEFAULT" } else { "" },
                rt.runtime_name,
                rt.path,
            );
        }
        println!("=== end ===\n");

        // On a system with java on PATH we should find at least one.
        // If this fails, the host genuinely has no Java — that's fine,
        // just skip with:  cargo test test_detect_live -- --ignored
        if which_java().is_some() {
            assert!(
                !runtimes.is_empty(),
                "java is on PATH but detect_java_runtimes() found nothing"
            );
            // Exactly one runtime should be marked as default
            let defaults: Vec<_> = runtimes.iter().filter(|r| r.is_default).collect();
            assert_eq!(
                defaults.len(),
                1,
                "expected exactly 1 default runtime, got {}: {:?}",
                defaults.len(),
                defaults.iter().map(|r| &r.path).collect::<Vec<_>>(),
            );
        }
    }

    #[test]
    fn test_generate_java_env_vars() {
        let env = generate_java_env_vars("/usr/lib/jvm/java-21");
        assert_eq!(env.get("JAVA_HOME").unwrap(), "/usr/lib/jvm/java-21");
        assert_eq!(env.len(), 1, "should only contain JAVA_HOME");
    }

    #[test]
    fn test_generate_java_env_vars_nix_store() {
        let env = generate_java_env_vars("/nix/store/abc123-openjdk-21.0.2");
        assert_eq!(
            env.get("JAVA_HOME").unwrap(),
            "/nix/store/abc123-openjdk-21.0.2"
        );
    }

    #[test]
    fn test_java_home_derived_from_bin_java() {
        // Simulate what probe_java does: given a path ending in /bin/java,
        // java_home should strip the trailing /bin/java.
        let java_path = Path::new("/usr/lib/jvm/java-21/bin/java");
        let java_home = java_path
            .parent()
            .and_then(|bin_dir| {
                if bin_dir.file_name().map(|n| n == "bin").unwrap_or(false) {
                    bin_dir.parent()
                } else {
                    None
                }
            })
            .unwrap_or(java_path)
            .to_string_lossy()
            .to_string();
        assert_eq!(java_home, "/usr/lib/jvm/java-21");
    }

    #[test]
    fn test_java_home_derived_nix_store_path() {
        let java_path = Path::new("/nix/store/abc123-zulu-ca-jdk-21.0.8/bin/java");
        let java_home = java_path
            .parent()
            .and_then(|bin_dir| {
                if bin_dir.file_name().map(|n| n == "bin").unwrap_or(false) {
                    bin_dir.parent()
                } else {
                    None
                }
            })
            .unwrap_or(java_path)
            .to_string_lossy()
            .to_string();
        assert_eq!(java_home, "/nix/store/abc123-zulu-ca-jdk-21.0.8");
    }

    #[test]
    fn test_java_home_fallback_when_not_in_bin() {
        // If the java binary isn't inside a `bin/` directory, fall back
        // to the path itself (unusual but shouldn't panic).
        let java_path = Path::new("/opt/custom-java/java");
        let java_home = java_path
            .parent()
            .and_then(|bin_dir| {
                if bin_dir.file_name().map(|n| n == "bin").unwrap_or(false) {
                    bin_dir.parent()
                } else {
                    None
                }
            })
            .unwrap_or(java_path)
            .to_string_lossy()
            .to_string();
        // Falls back to the original path since parent is not "bin"
        assert_eq!(java_home, "/opt/custom-java/java");
    }

    #[test]
    fn test_detect_live_includes_java_home() {
        let runtimes = detect_java_runtimes();
        for rt in &runtimes {
            // Every runtime should have a non-empty java_home
            assert!(
                !rt.java_home.is_empty(),
                "java_home should not be empty for runtime at {}",
                rt.path,
            );
            // java_home should NOT end with /bin/java
            assert!(
                !rt.java_home.ends_with("/bin/java"),
                "java_home should not end with /bin/java, got: {}",
                rt.java_home,
            );
        }
    }
}
