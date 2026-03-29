//! Dynamic-options fetch logic (Ticket 013, Tier 1).
//!
//! Proxies an outbound HTTP GET to a user-supplied URL, navigates the
//! JSON response using a dot-separated path, extracts option values
//! (and optional labels) from the resulting array, then applies sorting
//! and limiting before returning a canonical `Vec<FetchedOption>`.
//!
//! This module is deliberately free of `AppState` so all core logic can
//! be unit-tested in isolation.
//!
//! JSON path navigation is provided by [`crate::json_path::json_navigate`].

use std::collections::HashMap;
use std::time::Duration;

use serde_json::Value;

use crate::types::{FetchedOption, OptionsSortOrder};

// Re-export json_navigate for backward compatibility with tests
pub use crate::utils::json_path::json_navigate;

// ─── Constants ───────────────────────────────────────────────────────

/// Max response body size (2 MB).
const RESPONSE_MAX_BYTES: usize = 2 * 1024 * 1024;

/// Hard timeout on the outbound HTTP request.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

// ─── Variable substitution ───────────────────────────────────────────

/// Replace `{{key}}` placeholders in `input` with values from `vars`.
///
/// Uses the double-brace convention (`{{param}}`) specified in the ticket
/// rather than the `${param}` convention used by pipeline variables so
/// that template authors can distinguish between the two substitution
/// contexts.
pub fn substitute_template_vars(input: &str, vars: &HashMap<String, String>) -> String {
    let mut result = input.to_string();
    for (key, value) in vars {
        result = result.replace(&format!("{{{{{}}}}}", key), value);
    }
    result
}

// ─── Option extraction ───────────────────────────────────────────────

/// Extract a flat list of `FetchedOption` from a JSON value that should
/// be an array.
///
/// - If `value_key` is `None`, each array element must be a scalar
///   (string or number) which is used as both `value` and `label`.
/// - If `value_key` is `Some`, each element must be an object and the
///   key is read for the `value`.  `label_key` (if given) is read for
///   the display label; otherwise `value_key` is used for both.
pub fn extract_options(
    value: &Value,
    value_key: Option<&str>,
    label_key: Option<&str>,
) -> Result<Vec<FetchedOption>, String> {
    let arr = match value {
        Value::Array(a) => a,
        _ => {
            return Err("Expected a JSON array at the resolved path, got a non-array value".into());
        }
    };

    let mut options = Vec::with_capacity(arr.len());

    for (i, element) in arr.iter().enumerate() {
        match value_key {
            Some(vk) => {
                // Object mode — extract value_key and optional label_key.
                let obj = element.as_object().ok_or_else(|| {
                    format!(
                        "Array element at index {} is not an object (value_key '{}' was specified)",
                        i, vk
                    )
                })?;

                let val = obj
                    .get(vk)
                    .ok_or_else(|| {
                        format!("Array element at index {} does not have key '{}'", i, vk)
                    })
                    .and_then(|v| {
                        scalar_to_string(v).ok_or_else(|| {
                            format!(
                                "Value at key '{}' in element {} is not a string or number",
                                vk, i
                            )
                        })
                    })?;

                let lbl = match label_key {
                    Some(lk) => obj
                        .get(lk)
                        .and_then(scalar_to_string)
                        .unwrap_or_else(|| val.clone()),
                    None => val.clone(),
                };

                options.push(FetchedOption {
                    value: val,
                    label: lbl,
                });
            }
            None => {
                // Scalar mode — each element is a string/number.
                let s = scalar_to_string(element).ok_or_else(|| {
                    format!(
                        "Array element at index {} is not a string or number \
                         (set value_key if the array contains objects)",
                        i
                    )
                })?;
                options.push(FetchedOption {
                    value: s.clone(),
                    label: s,
                });
            }
        }
    }

    Ok(options)
}

/// Convert a scalar JSON value (string, number, bool) to a `String`.
fn scalar_to_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

// ─── Version-aware comparison ────────────────────────────────────────

/// Returns `true` when the string looks like a version number (starts
/// with an ASCII digit).  Used to partition version-like entries before
/// non-version entries so that, e.g., `"1.21.4"` always sorts before
/// `"snapshot-abc"` regardless of ascending/descending order.
fn is_version_like(s: &str) -> bool {
    s.bytes().next().is_some_and(|b| b.is_ascii_digit())
}

/// Compare two strings using version-aware logic:
///
/// 1. Split both strings on `'.'`.
/// 2. Walk segments pair-wise.  Each segment is decomposed into a
///    leading numeric prefix and an optional non-numeric remainder
///    (e.g. `"9-pre2"` → `(Some(9), "-pre2")`).  The numeric
///    prefixes are compared first; if equal, the remainders are
///    compared lexicographically.  A purely numeric segment (empty
///    remainder) sorts **after** one with a remainder so that
///    `"1.21.9" > "1.21.9-pre2"`.
/// 3. If all compared segments are equal, the string with more
///    segments is considered greater (e.g. `"1.21.1" > "1.21"`).
///
/// The decomposition guarantees a **total order** even when the
/// input mixes pure-numeric and suffixed segments (which would
/// break transitivity under the previous "parse-or-string-fallback"
/// strategy).
pub fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let a_parts: Vec<&str> = a.split('.').collect();
    let b_parts: Vec<&str> = b.split('.').collect();

    let max_len = a_parts.len().max(b_parts.len());
    for i in 0..max_len {
        match (a_parts.get(i), b_parts.get(i)) {
            (Some(ap), Some(bp)) => {
                let cmp = compare_segments(ap, bp);
                if cmp != std::cmp::Ordering::Equal {
                    return cmp;
                }
            }
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (None, None) => break,
        }
    }

    std::cmp::Ordering::Equal
}

/// Split a version segment into an optional leading integer and the
/// remaining (possibly empty) suffix.
///
/// Examples:
/// - `"21"`     → `(Some(21), "")`
/// - `"9-pre2"` → `(Some(9), "-pre2")`
/// - `"rc1"`    → `(None, "rc1")`
/// - `""`       → `(None, "")`
fn split_numeric_prefix(s: &str) -> (Option<u64>, &str) {
    let end = s
        .bytes()
        .position(|b| !b.is_ascii_digit())
        .unwrap_or(s.len());
    if end == 0 {
        (None, s)
    } else {
        // The prefix is guaranteed to be all-ASCII-digit, so parse
        // can only fail on overflow — treat that as non-numeric.
        match s[..end].parse::<u64>() {
            Ok(n) => (Some(n), &s[end..]),
            Err(_) => (None, s),
        }
    }
}

/// Compare two individual version segments with a total-order-safe
/// strategy:
///
/// 1. Extract the numeric prefix and remainder from each segment.
/// 2. Segments **with** a numeric prefix sort before those without.
/// 3. Among segments that both have a numeric prefix, compare the
///    numbers first.  If equal, a segment with an **empty** remainder
///    (pure number) sorts **after** one with a non-empty remainder,
///    so that release versions beat pre-release tags
///    (e.g. `"9" > "9-pre2"`).  Otherwise remainders are compared
///    lexicographically.
/// 4. Segments without a numeric prefix are compared as plain strings.
fn compare_segments(a: &str, b: &str) -> std::cmp::Ordering {
    let (a_num, a_rest) = split_numeric_prefix(a);
    let (b_num, b_rest) = split_numeric_prefix(b);

    match (a_num, b_num) {
        (Some(an), Some(bn)) => {
            let cmp = an.cmp(&bn);
            if cmp != std::cmp::Ordering::Equal {
                return cmp;
            }
            // Numeric prefixes equal — compare remainders.
            // Empty remainder (pure number) sorts AFTER non-empty
            // remainder (pre-release tag), so "9" > "9-pre2".
            match (a_rest.is_empty(), b_rest.is_empty()) {
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                _ => a_rest.cmp(b_rest),
            }
        }
        // Numeric prefix beats no numeric prefix.
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        // Neither has a numeric prefix — plain string compare.
        (None, None) => a.cmp(b),
    }
}

// ─── Sort + Limit ────────────────────────────────────────────────────

/// Apply optional sorting and limiting to a list of options.
///
/// Sorting is **version-aware**: segments separated by `'.'` are
/// compared numerically when both sides are integers, so `"1.21"` sorts
/// higher than `"1.9"`.  Entries that do not start with a digit are
/// placed **after** version-like entries and sorted alphabetically among
/// themselves.
pub fn sort_and_limit(
    mut options: Vec<FetchedOption>,
    sort: Option<OptionsSortOrder>,
    limit: Option<u32>,
) -> Vec<FetchedOption> {
    if let Some(order) = sort {
        options.sort_by(|a, b| {
            let a_ver = is_version_like(&a.value);
            let b_ver = is_version_like(&b.value);

            // Version-like entries always come before non-version-like.
            // Within each group, apply the requested order.
            match (a_ver, b_ver) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => match order {
                    OptionsSortOrder::Asc => version_cmp(&a.value, &b.value),
                    OptionsSortOrder::Desc => version_cmp(&b.value, &a.value),
                },
            }
        });
    }

    if let Some(max) = limit {
        options.truncate(max as usize);
    }

    options
}

// ─── HTTP fetch + full pipeline ──────────────────────────────────────

/// Perform the full fetch-options pipeline:
///
/// 1. Substitute `{{param}}` variables in the URL.
/// 2. GET the URL with timeout + size cap.
/// 3. Navigate the JSON response using `path`.
/// 4. Extract options via `value_key` / `label_key`.
/// 5. Sort and limit.
#[allow(clippy::too_many_arguments)]
pub async fn fetch_and_extract(
    http_client: &reqwest::Client,
    url: &str,
    path: Option<&str>,
    value_key: Option<&str>,
    label_key: Option<&str>,
    sort: Option<OptionsSortOrder>,
    limit: Option<u32>,
    vars: &HashMap<String, String>,
) -> Result<Vec<FetchedOption>, String> {
    let url = substitute_template_vars(url, vars);

    // Only allow http(s) schemes.
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err(format!(
            "URL scheme must be http or https, got: {}",
            url.split("://").next().unwrap_or("(none)")
        ));
    }

    let resp = http_client
        .get(&url)
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("HTTP request to {} failed: {}", url, e))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {} from {}", resp.status(), url));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response body from {}: {}", url, e))?;

    if bytes.len() > RESPONSE_MAX_BYTES {
        return Err(format!(
            "Response from {} exceeds 2 MB limit ({} bytes)",
            url,
            bytes.len()
        ));
    }

    let json: Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Response from {} is not valid JSON: {}", url, e))?;

    let navigated = json_navigate(&json, path).ok_or_else(|| {
        format!(
            "Path '{}' did not resolve in the JSON response from {}",
            path.unwrap_or(""),
            url
        )
    })?;

    let options = extract_options(navigated, value_key, label_key)?;

    Ok(sort_and_limit(options, sort, limit))
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Shorthand: build a `FetchedOption` where value == label.
    fn fo(v: &str) -> FetchedOption {
        FetchedOption {
            value: v.into(),
            label: v.into(),
        }
    }

    // ── substitute_template_vars ──

    #[test]
    fn substitute_replaces_double_braces() {
        let mut vars = HashMap::new();
        vars.insert("version".into(), "1.21.4".into());
        vars.insert("project".into(), "paper".into());

        let result = substitute_template_vars(
            "https://api.papermc.io/v2/projects/{{project}}/versions/{{version}}/builds",
            &vars,
        );
        assert_eq!(
            result,
            "https://api.papermc.io/v2/projects/paper/versions/1.21.4/builds"
        );
    }

    #[test]
    fn substitute_no_vars_passthrough() {
        let vars = HashMap::new();
        let result = substitute_template_vars("https://api.example.com/versions", &vars);
        assert_eq!(result, "https://api.example.com/versions");
    }

    #[test]
    fn substitute_missing_var_left_as_is() {
        let vars = HashMap::new();
        let result = substitute_template_vars("https://api.example.com/{{missing}}/data", &vars);
        assert_eq!(result, "https://api.example.com/{{missing}}/data");
    }

    // ── json_navigate ──

    #[test]
    fn navigate_none_path_returns_root() {
        let val = json!({"a": 1});
        assert_eq!(json_navigate(&val, None), Some(&val));
    }

    #[test]
    fn navigate_empty_path_returns_root() {
        let val = json!({"a": 1});
        assert_eq!(json_navigate(&val, Some("")), Some(&val));
    }

    #[test]
    fn navigate_single_key() {
        let val = json!({"versions": [1, 2, 3]});
        assert_eq!(
            json_navigate(&val, Some("versions")),
            Some(&json!([1, 2, 3]))
        );
    }

    #[test]
    fn navigate_nested_path() {
        let val = json!({"data": {"runtimes": ["a", "b"]}});
        assert_eq!(
            json_navigate(&val, Some("data.runtimes")),
            Some(&json!(["a", "b"]))
        );
    }

    #[test]
    fn navigate_missing_key_returns_none() {
        let val = json!({"a": 1});
        assert_eq!(json_navigate(&val, Some("b")), None);
    }

    #[test]
    fn navigate_through_non_object_returns_none() {
        let val = json!({"a": "hello"});
        assert_eq!(json_navigate(&val, Some("a.b")), None);
    }

    // ── extract_options — scalar mode ──

    #[test]
    fn extract_string_array() {
        let val = json!(["1.21.4", "1.21.3", "1.20.6"]);
        let opts = extract_options(&val, None, None).unwrap();
        assert_eq!(opts.len(), 3);
        assert_eq!(opts[0].value, "1.21.4");
        assert_eq!(opts[0].label, "1.21.4");
        assert_eq!(opts[2].value, "1.20.6");
    }

    #[test]
    fn extract_number_array() {
        let val = json!([100, 200, 300]);
        let opts = extract_options(&val, None, None).unwrap();
        assert_eq!(opts.len(), 3);
        assert_eq!(opts[0].value, "100");
        assert_eq!(opts[0].label, "100");
    }

    #[test]
    fn extract_empty_array() {
        let val = json!([]);
        let opts = extract_options(&val, None, None).unwrap();
        assert!(opts.is_empty());
    }

    #[test]
    fn extract_non_array_returns_error() {
        let val = json!("hello");
        let err = extract_options(&val, None, None).unwrap_err();
        assert!(err.contains("Expected a JSON array"));
    }

    #[test]
    fn extract_mixed_non_scalar_array_returns_error() {
        let val = json!([{"a": 1}, {"a": 2}]);
        let err = extract_options(&val, None, None).unwrap_err();
        assert!(err.contains("not a string or number"));
    }

    // ── extract_options — object mode ──

    #[test]
    fn extract_with_value_key() {
        let val = json!([
            {"id": "java17", "name": "Java 17"},
            {"id": "java21", "name": "Java 21"}
        ]);
        let opts = extract_options(&val, Some("id"), None).unwrap();
        assert_eq!(opts.len(), 2);
        assert_eq!(opts[0].value, "java17");
        assert_eq!(opts[0].label, "java17"); // no label_key → uses value
    }

    #[test]
    fn extract_with_value_key_and_label_key() {
        let val = json!([
            {"id": "java17", "display_name": "Java 17 (LTS)"},
            {"id": "java21", "display_name": "Java 21 (LTS)"}
        ]);
        let opts = extract_options(&val, Some("id"), Some("display_name")).unwrap();
        assert_eq!(opts.len(), 2);
        assert_eq!(opts[0].value, "java17");
        assert_eq!(opts[0].label, "Java 17 (LTS)");
        assert_eq!(opts[1].value, "java21");
        assert_eq!(opts[1].label, "Java 21 (LTS)");
    }

    #[test]
    fn extract_with_label_key_missing_falls_back_to_value() {
        let val = json!([
            {"id": "java17"},
            {"id": "java21", "display_name": "Java 21 (LTS)"}
        ]);
        let opts = extract_options(&val, Some("id"), Some("display_name")).unwrap();
        assert_eq!(opts[0].label, "java17"); // fallback
        assert_eq!(opts[1].label, "Java 21 (LTS)");
    }

    #[test]
    fn extract_with_value_key_missing_returns_error() {
        let val = json!([{"name": "Java 17"}]);
        let err = extract_options(&val, Some("id"), None).unwrap_err();
        assert!(err.contains("does not have key 'id'"));
    }

    #[test]
    fn extract_numeric_value_key() {
        let val = json!([
            {"build": 42, "version": "1.21.4"},
            {"build": 43, "version": "1.21.4"}
        ]);
        let opts = extract_options(&val, Some("build"), Some("version")).unwrap();
        assert_eq!(opts[0].value, "42");
        assert_eq!(opts[0].label, "1.21.4");
    }

    #[test]
    fn extract_non_object_elements_with_value_key_returns_error() {
        let val = json!(["a", "b"]);
        let err = extract_options(&val, Some("id"), None).unwrap_err();
        assert!(err.contains("not an object"));
    }

    // ── sort_and_limit ──

    #[test]
    fn sort_asc() {
        let opts = vec![fo("c"), fo("a"), fo("b")];
        let sorted = sort_and_limit(opts, Some(OptionsSortOrder::Asc), None);
        assert_eq!(sorted[0].value, "a");
        assert_eq!(sorted[1].value, "b");
        assert_eq!(sorted[2].value, "c");
    }

    #[test]
    fn sort_desc() {
        let opts = vec![fo("a"), fo("c"), fo("b")];
        let sorted = sort_and_limit(opts, Some(OptionsSortOrder::Desc), None);
        assert_eq!(sorted[0].value, "c");
        assert_eq!(sorted[1].value, "b");
        assert_eq!(sorted[2].value, "a");
    }

    #[test]
    fn limit_only() {
        let opts = vec![fo("a"), fo("b"), fo("c")];
        let limited = sort_and_limit(opts, None, Some(2));
        assert_eq!(limited.len(), 2);
        assert_eq!(limited[0].value, "a");
        assert_eq!(limited[1].value, "b");
    }

    #[test]
    fn sort_then_limit() {
        let opts = vec![fo("1.20"), fo("1.21"), fo("1.19"), fo("1.18")];
        let result = sort_and_limit(opts, Some(OptionsSortOrder::Desc), Some(2));
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].value, "1.21");
        assert_eq!(result[1].value, "1.20");
    }

    #[test]
    fn no_sort_no_limit_preserves_order() {
        let opts = vec![fo("c"), fo("a")];
        let result = sort_and_limit(opts.clone(), None, None);
        assert_eq!(result[0].value, "c");
        assert_eq!(result[1].value, "a");
    }

    #[test]
    fn limit_larger_than_vec() {
        let opts = vec![fo("a")];
        let result = sort_and_limit(opts, None, Some(100));
        assert_eq!(result.len(), 1);
    }

    // ── Full pipeline (mock-style with json_navigate + extract + sort) ──

    #[test]
    fn papermc_style_full_pipeline() {
        let response = json!({
            "project_id": "paper",
            "project_name": "Paper",
            "versions": [
                "1.8", "1.8.1", "1.8.2", "1.19", "1.19.1", "1.20",
                "1.20.1", "1.20.4", "1.20.6", "1.21", "1.21.1",
                "1.21.2", "1.21.3", "1.21.4"
            ]
        });

        let navigated = json_navigate(&response, Some("versions")).unwrap();
        let opts = extract_options(navigated, None, None).unwrap();
        let result = sort_and_limit(opts, Some(OptionsSortOrder::Desc), Some(5));

        assert_eq!(result.len(), 5);
        // Version-aware sort: 1.21.4 > 1.21.3 > … > 1.21 > 1.20.6 …
        assert_eq!(result[0].value, "1.21.4");
        assert_eq!(result[1].value, "1.21.3");
        assert_eq!(result[2].value, "1.21.2");
        assert_eq!(result[3].value, "1.21.1");
        assert_eq!(result[4].value, "1.21");
    }

    #[test]
    fn version_sort_prerelease_segments() {
        // Segments like "9-pre2" must not break the total order.
        // The numeric prefix (9) is extracted first; the suffix
        // ("-pre2") is compared only when prefixes are equal.
        // Pure-numeric segments sort AFTER their pre-release
        // counterparts: "1.21.9" > "1.21.9-rc1" > "1.21.9-pre4".
        let opts = vec![
            fo("1.21.9-pre2"),
            fo("1.21.10"),
            fo("1.21.9"),
            fo("1.21.9-rc1"),
            fo("1.21.9-pre4"),
            fo("1.21.8"),
        ];
        let sorted = sort_and_limit(opts, Some(OptionsSortOrder::Desc), None);
        let vals: Vec<&str> = sorted.iter().map(|o| o.value.as_str()).collect();
        assert_eq!(
            vals,
            vec![
                "1.21.10",
                "1.21.9",
                "1.21.9-rc1",
                "1.21.9-pre4",
                "1.21.9-pre2",
                "1.21.8",
            ]
        );
    }

    #[test]
    fn version_sort_real_papermc_data() {
        // Reproduce the exact data from the live PaperMC API that
        // triggered the "does not correctly implement a total order"
        // panic in Rust 1.81+.
        let versions = vec![
            "1.7.10",
            "1.8.8",
            "1.9.4",
            "1.10.2",
            "1.11.2",
            "1.12",
            "1.12.1",
            "1.12.2",
            "1.13-pre7",
            "1.13",
            "1.13.1",
            "1.13.2",
            "1.14",
            "1.14.1",
            "1.14.2",
            "1.14.3",
            "1.14.4",
            "1.15",
            "1.15.1",
            "1.15.2",
            "1.16.1",
            "1.16.2",
            "1.16.3",
            "1.16.4",
            "1.16.5",
            "1.17",
            "1.17.1",
            "1.18",
            "1.18.1",
            "1.18.2",
            "1.19",
            "1.19.1",
            "1.19.2",
            "1.19.3",
            "1.19.4",
            "1.20",
            "1.20.1",
            "1.20.2",
            "1.20.4",
            "1.20.5",
            "1.20.6",
            "1.21",
            "1.21.1",
            "1.21.3",
            "1.21.4",
            "1.21.5",
            "1.21.6",
            "1.21.7",
            "1.21.8",
            "1.21.9-pre2",
            "1.21.9-pre3",
            "1.21.9-pre4",
            "1.21.9-rc1",
            "1.21.9",
            "1.21.10",
            "1.21.11-pre3",
            "1.21.11-pre4",
            "1.21.11-pre5",
            "1.21.11-rc1",
            "1.21.11-rc2",
            "1.21.11-rc3",
            "1.21.11",
        ];
        let opts: Vec<FetchedOption> = versions.into_iter().map(fo).collect();

        // Must not panic.
        let result = sort_and_limit(opts, Some(OptionsSortOrder::Desc), Some(10));
        assert_eq!(result.len(), 10);
        // Top entries should be the highest versions.
        assert_eq!(result[0].value, "1.21.11");
        assert_eq!(result[1].value, "1.21.11-rc3");
        assert_eq!(result[2].value, "1.21.11-rc2");
        assert_eq!(result[3].value, "1.21.11-rc1");
        assert_eq!(result[4].value, "1.21.11-pre5");
    }

    #[test]
    fn object_array_full_pipeline() {
        let response = json!({
            "data": {
                "runtimes": [
                    {"id": "java17", "display_name": "Java 17 (LTS)"},
                    {"id": "java11", "display_name": "Java 11 (LTS)"},
                    {"id": "java21", "display_name": "Java 21 (LTS)"}
                ]
            }
        });

        let navigated = json_navigate(&response, Some("data.runtimes")).unwrap();
        let opts = extract_options(navigated, Some("id"), Some("display_name")).unwrap();
        let result = sort_and_limit(opts, Some(OptionsSortOrder::Asc), None);

        assert_eq!(result.len(), 3);
        // Non-version-like strings: all start with "java", sorted
        // lexicographically among themselves.
        assert_eq!(result[0].value, "java11");
        assert_eq!(result[0].label, "Java 11 (LTS)");
        assert_eq!(result[1].value, "java17");
        assert_eq!(result[1].label, "Java 17 (LTS)");
        assert_eq!(result[2].value, "java21");
        assert_eq!(result[2].label, "Java 21 (LTS)");
    }

    #[test]
    fn builds_with_numeric_keys() {
        let response = json!({
            "builds": [
                {"build": 1, "channel": "default"},
                {"build": 2, "channel": "default"},
                {"build": 3, "channel": "experimental"}
            ]
        });

        let navigated = json_navigate(&response, Some("builds")).unwrap();
        let opts = extract_options(navigated, Some("build"), Some("channel")).unwrap();
        let result = sort_and_limit(opts, Some(OptionsSortOrder::Desc), Some(2));

        assert_eq!(result.len(), 2);
        // Numeric values: version_cmp compares 3 > 2 > 1.
        assert_eq!(result[0].value, "3");
        assert_eq!(result[0].label, "experimental");
        assert_eq!(result[1].value, "2");
        assert_eq!(result[1].label, "default");
    }

    // ── Version-aware sort tests ──────────────────────────────────────

    #[test]
    fn version_sort_numeric_segments() {
        // Ensures "1.9" < "1.20" (numeric comparison, not lexicographic).
        let opts = vec![fo("1.20"), fo("1.9"), fo("1.21")];
        let sorted = sort_and_limit(opts, Some(OptionsSortOrder::Asc), None);
        assert_eq!(sorted[0].value, "1.9");
        assert_eq!(sorted[1].value, "1.20");
        assert_eq!(sorted[2].value, "1.21");
    }

    #[test]
    fn version_sort_desc_three_part() {
        let opts = vec![fo("1.8.1"), fo("1.21.4"), fo("1.20.6"), fo("1.21")];
        let sorted = sort_and_limit(opts, Some(OptionsSortOrder::Desc), None);
        assert_eq!(sorted[0].value, "1.21.4");
        assert_eq!(sorted[1].value, "1.21");
        assert_eq!(sorted[2].value, "1.20.6");
        assert_eq!(sorted[3].value, "1.8.1");
    }

    #[test]
    fn version_sort_mixed_version_and_non_version() {
        // Non-version-like entries (not starting with a digit) should
        // sort AFTER all version-like entries.
        let opts = vec![fo("snapshot-1"), fo("1.20"), fo("beta-2"), fo("1.9")];
        let sorted = sort_and_limit(opts, Some(OptionsSortOrder::Asc), None);
        // Version-like first (ascending), then non-version (ascending)
        assert_eq!(sorted[0].value, "1.9");
        assert_eq!(sorted[1].value, "1.20");
        assert_eq!(sorted[2].value, "beta-2");
        assert_eq!(sorted[3].value, "snapshot-1");
    }

    #[test]
    fn version_sort_mixed_desc() {
        let opts = vec![fo("alpha"), fo("2.0"), fo("1.10")];
        let sorted = sort_and_limit(opts, Some(OptionsSortOrder::Desc), None);
        // Version-like first (descending), then non-version (descending)
        assert_eq!(sorted[0].value, "2.0");
        assert_eq!(sorted[1].value, "1.10");
        assert_eq!(sorted[2].value, "alpha");
    }

    #[test]
    fn version_sort_equal_prefix_longer_wins() {
        // "1.21.1" should be greater than "1.21" because it has more
        // segments.
        let opts = vec![fo("1.21.1"), fo("1.21")];
        let sorted = sort_and_limit(opts, Some(OptionsSortOrder::Asc), None);
        assert_eq!(sorted[0].value, "1.21");
        assert_eq!(sorted[1].value, "1.21.1");
    }

    #[test]
    fn version_cmp_unit() {
        use super::version_cmp;
        use std::cmp::Ordering;

        assert_eq!(version_cmp("1.9", "1.20"), Ordering::Less);
        assert_eq!(version_cmp("1.21.4", "1.21.3"), Ordering::Greater);
        assert_eq!(version_cmp("1.21", "1.21"), Ordering::Equal);
        assert_eq!(version_cmp("1.21", "1.21.0"), Ordering::Less);
        assert_eq!(version_cmp("2.0", "1.99"), Ordering::Greater);
        assert_eq!(version_cmp("10", "9"), Ordering::Greater);
        // Non-numeric segment falls back to lexicographic
        assert_eq!(version_cmp("1.0a", "1.0b"), Ordering::Less);

        // Pre-release suffixes: pure number > suffixed
        assert_eq!(version_cmp("1.21.9", "1.21.9-pre2"), Ordering::Greater);
        assert_eq!(version_cmp("1.21.9-pre2", "1.21.9"), Ordering::Less);
        assert_eq!(version_cmp("1.21.9-pre2", "1.21.9-pre4"), Ordering::Less);
        assert_eq!(version_cmp("1.21.9-rc1", "1.21.9-pre4"), Ordering::Greater);

        // The critical transitivity case that caused the panic:
        // 10 > 9-pre2 (numeric prefix 10 > 9) AND 10 > 9 (numeric)
        assert_eq!(version_cmp("1.21.10", "1.21.9-pre2"), Ordering::Greater);
        assert_eq!(version_cmp("1.21.10", "1.21.9"), Ordering::Greater);
        assert_eq!(version_cmp("1.21.9-pre2", "1.21.9"), Ordering::Less);

        // "13-pre7" style segment
        assert_eq!(version_cmp("1.13-pre7", "1.13"), Ordering::Less);
        assert_eq!(version_cmp("1.13", "1.13-pre7"), Ordering::Greater);
        assert_eq!(version_cmp("1.14", "1.13-pre7"), Ordering::Greater);
    }

    #[test]
    fn split_numeric_prefix_unit() {
        use super::split_numeric_prefix;

        assert_eq!(split_numeric_prefix("21"), (Some(21), ""));
        assert_eq!(split_numeric_prefix("9-pre2"), (Some(9), "-pre2"));
        assert_eq!(split_numeric_prefix("rc1"), (None, "rc1"));
        assert_eq!(split_numeric_prefix(""), (None, ""));
        assert_eq!(split_numeric_prefix("0"), (Some(0), ""));
        assert_eq!(split_numeric_prefix("13-pre7"), (Some(13), "-pre7"));
        assert_eq!(split_numeric_prefix("11-rc3"), (Some(11), "-rc3"));
    }

    #[test]
    fn compare_segments_total_order_exhaustive() {
        // Verify that compare_segments satisfies the total order
        // property for a set of segments that mix numeric, suffixed,
        // and purely non-numeric values.
        use super::compare_segments;
        use std::cmp::Ordering;

        let segments = [
            "8", "9", "9-pre2", "9-pre4", "9-rc1", "9-rc2", "10", "11-pre3", "11-rc1", "11", "abc",
            "rc1",
        ];

        // Check antisymmetry and transitivity by sorting and
        // verifying the result is consistent.
        let mut sorted = segments.to_vec();
        sorted.sort_by(|a, b| compare_segments(a, b));

        // Verify every pair in the sorted order is actually ≤.
        for i in 0..sorted.len() {
            for j in i..sorted.len() {
                let cmp = compare_segments(sorted[i], sorted[j]);
                assert!(
                    cmp != Ordering::Greater,
                    "Total order violation: {:?} should be <= {:?} but got Greater",
                    sorted[i],
                    sorted[j],
                );
            }
        }

        // Also verify antisymmetry explicitly for every pair.
        for a in &segments {
            for b in &segments {
                let ab = compare_segments(a, b);
                let ba = compare_segments(b, a);
                assert_eq!(
                    ab,
                    ba.reverse(),
                    "Antisymmetry violation: cmp({:?},{:?})={:?} but cmp({:?},{:?})={:?}",
                    a,
                    b,
                    ab,
                    b,
                    a,
                    ba,
                );
            }
        }
    }

    #[test]
    fn version_sort_realistic_minecraft_versions() {
        // Simulate the real PaperMC API response order problem.
        let versions = vec![
            "1.8", "1.8.1", "1.8.2", "1.9", "1.9.1", "1.10", "1.11", "1.12", "1.13", "1.14",
            "1.15", "1.16", "1.17", "1.18", "1.19", "1.19.1", "1.20", "1.20.1", "1.20.4", "1.20.6",
            "1.21", "1.21.1", "1.21.2", "1.21.3", "1.21.4",
        ];
        let opts: Vec<FetchedOption> = versions.into_iter().map(fo).collect();

        let result = sort_and_limit(opts, Some(OptionsSortOrder::Desc), Some(10));
        let values: Vec<&str> = result.iter().map(|o| o.value.as_str()).collect();
        assert_eq!(
            values,
            vec![
                "1.21.4", "1.21.3", "1.21.2", "1.21.1", "1.21", "1.20.6", "1.20.4", "1.20.1",
                "1.20", "1.19.1",
            ]
        );
    }

    #[test]
    fn version_sort_mixed_prerelease_and_release() {
        // Versions with pre-release suffixes must sort correctly
        // relative to their release counterparts.
        let opts = vec![
            fo("1.13-pre7"),
            fo("1.13"),
            fo("1.13.1"),
            fo("1.12.2"),
            fo("1.14"),
        ];
        let sorted = sort_and_limit(opts, Some(OptionsSortOrder::Desc), None);
        let vals: Vec<&str> = sorted.iter().map(|o| o.value.as_str()).collect();
        assert_eq!(vals, vec!["1.14", "1.13.1", "1.13", "1.13-pre7", "1.12.2"]);
    }

    #[test]
    fn root_array_no_path() {
        let response = json!(["alpha", "beta", "gamma"]);
        let navigated = json_navigate(&response, None).unwrap();
        let opts = extract_options(navigated, None, None).unwrap();
        assert_eq!(opts.len(), 3);
        assert_eq!(opts[0].value, "alpha");
    }

    // ── URL scheme validation (tested indirectly via fetch_and_extract) ──

    #[tokio::test]
    async fn rejects_non_http_scheme() {
        let client = reqwest::Client::new();
        let vars = HashMap::new();
        let err = fetch_and_extract(
            &client,
            "ftp://evil.com/data",
            None,
            None,
            None,
            None,
            None,
            &vars,
        )
        .await
        .unwrap_err();
        assert!(err.contains("URL scheme must be http or https"));
    }

    #[tokio::test]
    async fn rejects_no_scheme() {
        let client = reqwest::Client::new();
        let vars = HashMap::new();
        let err = fetch_and_extract(&client, "not-a-url", None, None, None, None, None, &vars)
            .await
            .unwrap_err();
        assert!(err.contains("URL scheme must be http or https"));
    }
}
