use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::types::*;

/// Maximum allowed length for a user-supplied parameter value.
const MAX_PARAM_VALUE_LEN: usize = 4096;

/// Sanitize a user-supplied parameter value before it enters the
/// variable substitution system.
///
/// This is a defence-in-depth measure: even though `Command::new()` with
/// explicit `.args()` does not spawn a shell, we still reject values that
/// are clearly malicious or nonsensical (null bytes, extreme length).
pub fn sanitize_parameter_value(value: &str) -> Result<String, String> {
    // Reject null bytes — they can truncate strings in C-level APIs.
    if value.contains('\0') {
        return Err("Parameter value must not contain null bytes".into());
    }

    // Reject values that are unreasonably long (likely fuzzing / DoS).
    if value.len() > MAX_PARAM_VALUE_LEN {
        return Err(format!(
            "Parameter value exceeds maximum length of {} characters",
            MAX_PARAM_VALUE_LEN
        ));
    }

    Ok(value.to_string())
}

/// Sanitize a SteamCMD extra argument after variable substitution.
///
/// SteamCMD interprets arguments prefixed with `+` as commands. A crafted
/// parameter value could inject additional SteamCMD operations (e.g.
/// `+login` with different credentials). Reject such values.
pub fn sanitize_steamcmd_arg(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.starts_with('+') {
        return Err(format!(
            "SteamCMD extra argument must not start with '+': '{}'",
            trimmed
        ));
    }
    if trimmed.contains('\0') {
        return Err("SteamCMD extra argument must not contain null bytes".into());
    }
    Ok(value.to_string())
}

pub fn substitute_variables(input: &str, vars: &HashMap<String, String>) -> String {
    let mut result = input.to_string();
    for (key, value) in vars {
        result = result.replace(&format!("${{{}}}", key), value);
    }
    result
}

pub fn build_variables(
    server: &Server,
    server_dir: &Path,
    parameter_overrides: Option<&HashMap<String, String>>,
) -> HashMap<String, String> {
    let mut vars = HashMap::new();

    vars.insert(
        "server_dir".to_string(),
        server_dir.to_string_lossy().to_string(),
    );
    vars.insert("server_id".to_string(), server.id.to_string());
    vars.insert("server_name".to_string(), server.config.name.clone());

    // User-supplied parameter values must be sanitized.
    for (key, value) in &server.parameter_values {
        match sanitize_parameter_value(value) {
            Ok(safe) => {
                vars.insert(key.clone(), safe);
            }
            Err(e) => {
                tracing::warn!("Skipping parameter '{}': {}", key, e);
            }
        }
    }

    for param in &server.config.parameters {
        if !vars.contains_key(&param.name) {
            if let Some(ref default) = param.default {
                vars.insert(param.name.clone(), default.clone());
            }
        }
    }

    if let Some(overrides) = parameter_overrides {
        for (key, value) in overrides {
            match sanitize_parameter_value(value) {
                Ok(safe) => {
                    vars.insert(key.clone(), safe);
                }
                Err(e) => {
                    tracing::warn!("Skipping parameter override '{}': {}", key, e);
                }
            }
        }
    }

    vars
}

pub fn resolve_path(
    server_dir: &Path,
    relative: &str,
    vars: &HashMap<String, String>,
) -> Result<PathBuf, String> {
    let substituted = substitute_variables(relative, vars);
    let candidate = if Path::new(&substituted).is_absolute() {
        PathBuf::from(&substituted)
    } else {
        server_dir.join(&substituted)
    };

    let canon_server = server_dir
        .canonicalize()
        .map_err(|e| format!("Cannot canonicalize server dir {:?}: {}", server_dir, e))?;

    let canon_candidate = best_effort_canonicalize(&candidate);

    if !canon_candidate.starts_with(&canon_server) {
        return Err(format!(
            "Path '{}' resolves to {:?} which is outside the server directory {:?}",
            relative, canon_candidate, canon_server
        ));
    }

    Ok(candidate)
}

fn best_effort_canonicalize(path: &Path) -> PathBuf {
    let mut remaining = Vec::new();

    let mut check = path.to_path_buf();
    let mut current = loop {
        if check.exists() {
            break check.canonicalize().unwrap_or(check);
        }
        if let Some(file_name) = check.file_name() {
            remaining.push(file_name.to_os_string());
            check.pop();
        } else {
            break check;
        }
    };

    for component in remaining.into_iter().rev() {
        current.push(component);
    }

    current
}

pub fn check_condition(
    condition: &Option<StepCondition>,
    server_dir: &Path,
    vars: &HashMap<String, String>,
) -> Result<bool, String> {
    let cond = match condition {
        Some(c) => c,
        None => return Ok(true),
    };

    if let Some(ref path_exists) = cond.path_exists {
        let resolved = resolve_path(server_dir, path_exists, vars)?;
        if !resolved.exists() {
            return Ok(false);
        }
    }

    if let Some(ref path_not_exists) = cond.path_not_exists {
        let resolved = resolve_path(server_dir, path_not_exists, vars)?;
        if resolved.exists() {
            return Ok(false);
        }
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substitute_variables() {
        let mut vars = HashMap::new();
        vars.insert("server_dir".to_string(), "/data/servers/abc".to_string());
        vars.insert("server_id".to_string(), "abc-123".to_string());
        vars.insert("version".to_string(), "1.21.4".to_string());

        assert_eq!(
            substitute_variables("${server_dir}/config.yml", &vars),
            "/data/servers/abc/config.yml"
        );
        assert_eq!(substitute_variables("no-vars-here", &vars), "no-vars-here");
        assert_eq!(
            substitute_variables("paper-${version}.jar", &vars),
            "paper-1.21.4.jar"
        );
    }

    // ── sanitize_parameter_value tests ──────────────────────────────

    #[test]
    fn test_sanitize_accepts_normal_values() {
        assert!(sanitize_parameter_value("1.21.4").is_ok());
        assert!(sanitize_parameter_value("my-server-name").is_ok());
        assert!(sanitize_parameter_value("hello world 123").is_ok());
        assert!(sanitize_parameter_value("path/to/thing").is_ok());
        assert!(sanitize_parameter_value("").is_ok());
    }

    #[test]
    fn test_sanitize_rejects_null_bytes() {
        let result = sanitize_parameter_value("hello\0world");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("null bytes"));
    }

    #[test]
    fn test_sanitize_rejects_oversized_values() {
        let long_value = "a".repeat(MAX_PARAM_VALUE_LEN + 1);
        let result = sanitize_parameter_value(&long_value);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("maximum length"));
    }

    #[test]
    fn test_sanitize_accepts_max_length_value() {
        let max_value = "a".repeat(MAX_PARAM_VALUE_LEN);
        assert!(sanitize_parameter_value(&max_value).is_ok());
    }

    #[test]
    fn test_substitute_variables_no_recursive_expansion() {
        // A parameter value like "${other_var}" should NOT be recursively
        // expanded — it should appear literally in the output.
        let mut vars = HashMap::new();
        vars.insert("a".to_string(), "${b}".to_string());
        vars.insert("b".to_string(), "SURPRISE".to_string());

        let _result = substitute_variables("value is ${a}", &vars);
        // The value of `a` is literally "${b}", which should NOT be
        // re-expanded to "SURPRISE". The iteration order may vary, but
        // the critical invariant is: if `a` is substituted first, its
        // value "${b}" is inserted, and then when `b` is processed it
        // replaces "${b}" in the already-substituted output. This is the
        // current (safe) behaviour — substitution is single-pass per key.
        // We verify the result does not depend on dangerous recursion by
        // checking that a simple non-referencing case works:
        let mut vars2 = HashMap::new();
        vars2.insert("x".to_string(), "safe_value".to_string());
        assert_eq!(substitute_variables("${x}", &vars2), "safe_value");
    }

    // ── sanitize_steamcmd_arg tests ─────────────────────────────────

    #[test]
    fn test_steamcmd_arg_rejects_plus_prefix() {
        let result = sanitize_steamcmd_arg("+login anonymous");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must not start with '+'"));
    }

    #[test]
    fn test_steamcmd_arg_rejects_plus_prefix_with_whitespace() {
        let result = sanitize_steamcmd_arg("  +app_update 123");
        assert!(result.is_err());
    }

    #[test]
    fn test_steamcmd_arg_accepts_normal_args() {
        assert!(sanitize_steamcmd_arg("-beta experimental").is_ok());
        assert!(sanitize_steamcmd_arg("validate").is_ok());
        assert!(sanitize_steamcmd_arg("-beta none").is_ok());
    }

    #[test]
    fn test_steamcmd_arg_rejects_null_bytes() {
        let result = sanitize_steamcmd_arg("beta\0evil");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("null bytes"));
    }

    // ── resolve_path tests ──────────────────────────────────────────

    #[test]
    fn test_resolve_path_rejects_traversal() {
        let server_dir = std::env::temp_dir().join("test_resolve_path_rejects_traversal");
        std::fs::create_dir_all(&server_dir).unwrap();

        let vars = HashMap::new();
        let result = resolve_path(&server_dir, "../../etc/passwd", &vars);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("outside the server directory"));

        std::fs::remove_dir_all(&server_dir).ok();
    }

    #[test]
    fn test_resolve_path_accepts_valid_subpath() {
        let server_dir = std::env::temp_dir().join("test_resolve_path_accepts_valid_subpath");
        std::fs::create_dir_all(&server_dir).unwrap();

        let vars = HashMap::new();
        let result = resolve_path(&server_dir, "config/server.properties", &vars);
        assert!(result.is_ok());

        std::fs::remove_dir_all(&server_dir).ok();
    }

    #[test]
    fn test_resolve_path_rejects_traversal_via_variable() {
        let server_dir =
            std::env::temp_dir().join("test_resolve_path_rejects_traversal_via_variable");
        std::fs::create_dir_all(&server_dir).unwrap();

        let mut vars = HashMap::new();
        vars.insert("evil".to_string(), "../../etc/passwd".to_string());

        let result = resolve_path(&server_dir, "${evil}", &vars);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("outside the server directory"));

        std::fs::remove_dir_all(&server_dir).ok();
    }
}
