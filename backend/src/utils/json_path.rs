//! Shared JSON path navigation utility.
//!
//! Extracted from `update_check.rs` and `fetch_options.rs` (ticket 023)
//! to eliminate duplication.  Both modules now import from here.

use serde_json::Value;

/// Walk a serde_json `Value` using a dot-separated path string.
///
/// Examples:
///   - `"versions"`        → `root["versions"]`
///   - `"data.builds"`     → `root["data"]["builds"]`
///   - `None` / `""`       → returns root as-is
pub fn json_navigate<'a>(root: &'a Value, path: Option<&str>) -> Option<&'a Value> {
    let path = match path {
        Some(p) if !p.is_empty() => p,
        _ => return Some(root),
    };

    let mut current = root;
    for segment in path.split('.') {
        match current {
            Value::Object(map) => {
                current = map.get(segment)?;
            }
            _ => return None,
        }
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn navigate_none_path_returns_root() {
        let v = json!({"a": 1});
        assert_eq!(json_navigate(&v, None), Some(&v));
    }

    #[test]
    fn navigate_empty_path_returns_root() {
        let v = json!({"a": 1});
        assert_eq!(json_navigate(&v, Some("")), Some(&v));
    }

    #[test]
    fn navigate_single_key() {
        let v = json!({"versions": [1, 2, 3]});
        assert_eq!(json_navigate(&v, Some("versions")), Some(&json!([1, 2, 3])));
    }

    #[test]
    fn navigate_nested_path() {
        let v = json!({"data": {"builds": [10, 20]}});
        assert_eq!(
            json_navigate(&v, Some("data.builds")),
            Some(&json!([10, 20]))
        );
    }

    #[test]
    fn navigate_missing_key_returns_none() {
        let v = json!({"a": 1});
        assert_eq!(json_navigate(&v, Some("missing")), None);
    }

    #[test]
    fn navigate_through_non_object_returns_none() {
        let v = json!({"a": 42});
        assert_eq!(json_navigate(&v, Some("a.b")), None);
    }

    #[test]
    fn navigate_deeply_nested() {
        let v = json!({"a": {"b": {"c": {"d": "deep"}}}});
        assert_eq!(json_navigate(&v, Some("a.b.c.d")), Some(&json!("deep")));
    }

    #[test]
    fn navigate_partial_path_missing() {
        let v = json!({"a": {"b": 1}});
        assert_eq!(json_navigate(&v, Some("a.b.c")), None);
    }

    #[test]
    fn navigate_root_is_array() {
        let v = json!([1, 2, 3]);
        // No path → returns root
        assert_eq!(json_navigate(&v, None), Some(&v));
        // Path on an array → None (arrays are not objects)
        assert_eq!(json_navigate(&v, Some("foo")), None);
    }

    #[test]
    fn navigate_root_is_scalar() {
        let v = json!(42);
        assert_eq!(json_navigate(&v, None), Some(&v));
        assert_eq!(json_navigate(&v, Some("x")), None);
    }
}
