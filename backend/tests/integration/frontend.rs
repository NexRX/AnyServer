// ─── Tests (only compiled with bundle-frontend) ──────────────────────────────

#[cfg(feature = "bundle-frontend")]
mod bundled {
    use axum::body::Body;
    use axum::http::{header, Method, Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use crate::common::TestApp;

    /// Helper: send a raw request and return (status, headers, body bytes).
    /// We need this instead of TestApp::request because the frontend serves HTML,
    /// not JSON, and we want to inspect response headers (Content-Type, Cache-Control).
    async fn raw_request(
        app: &TestApp,
        method: Method,
        uri: &str,
    ) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .expect("failed to build request");

        let resp = app
            .router
            .clone()
            .oneshot(req)
            .await
            .expect("request failed");

        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = resp
            .into_body()
            .collect()
            .await
            .expect("failed to collect body")
            .to_bytes()
            .to_vec();

        (status, headers, bytes)
    }

    fn header_value(headers: &axum::http::HeaderMap, name: header::HeaderName) -> String {
        headers
            .get(name)
            .map(|v| v.to_str().unwrap_or("").to_string())
            .unwrap_or_default()
    }

    #[tokio::test]
    async fn test_root_serves_index_html() {
        let app = TestApp::new().await;

        let (status, headers, body) = raw_request(&app, Method::GET, "/").await;
        let body_str = String::from_utf8_lossy(&body);

        assert_eq!(status, StatusCode::OK);
        assert!(
            header_value(&headers, header::CONTENT_TYPE).contains("text/html"),
            "Expected text/html, got: {}",
            header_value(&headers, header::CONTENT_TYPE)
        );
        assert!(
            body_str.contains("<div id=\"root\">"),
            "Expected index.html to contain <div id=\"root\">, got: {}",
            &body_str[..body_str.len().min(500)]
        );
    }

    #[tokio::test]
    async fn test_index_html_has_no_cache_header() {
        let app = TestApp::new().await;

        let (status, headers, _) = raw_request(&app, Method::GET, "/").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            header_value(&headers, header::CACHE_CONTROL),
            "no-cache",
            "index.html should not be aggressively cached"
        );
    }

    #[tokio::test]
    async fn test_unknown_route_falls_back_to_index_html_for_spa() {
        let app = TestApp::new().await;

        // A route that doesn't match any API or static file should return index.html
        // so the SPA client-side router can handle it.
        let (status, headers, body) = raw_request(&app, Method::GET, "/servers/some-id").await;
        let body_str = String::from_utf8_lossy(&body);

        assert_eq!(status, StatusCode::OK);
        assert!(
            header_value(&headers, header::CONTENT_TYPE).contains("text/html"),
            "SPA fallback should serve text/html, got: {}",
            header_value(&headers, header::CONTENT_TYPE)
        );
        assert!(
            body_str.contains("<div id=\"root\">"),
            "SPA fallback should serve index.html content"
        );
    }

    #[tokio::test]
    async fn test_nested_unknown_route_falls_back_to_index_html() {
        let app = TestApp::new().await;

        let (status, headers, body) =
            raw_request(&app, Method::GET, "/some/deeply/nested/route").await;
        let body_str = String::from_utf8_lossy(&body);

        assert_eq!(status, StatusCode::OK);
        assert!(header_value(&headers, header::CONTENT_TYPE).contains("text/html"));
        assert!(body_str.contains("<div id=\"root\">"));
    }

    #[tokio::test]
    async fn test_api_routes_still_work_with_frontend_enabled() {
        let app = TestApp::new().await;
        let token = app.setup_admin("admin", "Admin1234").await;

        // API routes should be handled by the API, not the frontend fallback.
        let (status, body) = app.get("/api/servers", Some(&token)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            body["servers"].is_array(),
            "API should return JSON, not HTML: {:?}",
            body
        );
    }

    #[tokio::test]
    async fn test_api_routes_not_intercepted_by_frontend() {
        let app = TestApp::new().await;

        // Unauthenticated API request should get 401 from the API layer,
        // NOT a 200 with index.html from the frontend fallback.
        let (status, _, _) = raw_request(&app, Method::GET, "/api/servers").await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "API routes must be handled by API handlers, not frontend fallback"
        );
    }

    #[tokio::test]
    async fn test_static_asset_js_served_with_correct_content_type() {
        let app = TestApp::new().await;

        // Find a JS asset from the embedded files by requesting index.html and
        // extracting the script src. This avoids hardcoding hashed filenames.
        let (_, _, index_bytes) = raw_request(&app, Method::GET, "/").await;
        let index_str = String::from_utf8_lossy(&index_bytes);

        // Extract the JS asset path from the script tag, e.g. /assets/index-BTFtMjnE.js
        let js_path = extract_attribute(&index_str, "src=\"", "\"")
            .expect("Could not find a <script src=\"/...\"> in index.html");

        let (status, headers, body) = raw_request(&app, Method::GET, &js_path).await;

        assert_eq!(status, StatusCode::OK);
        assert!(
            header_value(&headers, header::CONTENT_TYPE).contains("application/javascript"),
            "JS asset should have application/javascript content type, got: {}",
            header_value(&headers, header::CONTENT_TYPE)
        );
        assert!(!body.is_empty(), "JS asset should not be empty");
    }

    #[tokio::test]
    async fn test_static_asset_css_served_with_correct_content_type() {
        let app = TestApp::new().await;

        let (_, _, index_bytes) = raw_request(&app, Method::GET, "/").await;
        let index_str = String::from_utf8_lossy(&index_bytes);

        // Extract the CSS asset path from the link tag, e.g. /assets/index-DiUOoxez.css
        let css_path = extract_attribute(&index_str, "href=\"/assets/", "\"")
            .map(|p| format!("/assets/{p}"))
            .expect("Could not find a <link href=\"/assets/...\"> in index.html");

        let (status, headers, body) = raw_request(&app, Method::GET, &css_path).await;

        assert_eq!(status, StatusCode::OK);
        assert!(
            header_value(&headers, header::CONTENT_TYPE).contains("text/css"),
            "CSS asset should have text/css content type, got: {}",
            header_value(&headers, header::CONTENT_TYPE)
        );
        assert!(!body.is_empty(), "CSS asset should not be empty");
    }

    #[tokio::test]
    async fn test_hashed_assets_have_immutable_cache_headers() {
        let app = TestApp::new().await;

        let (_, _, index_bytes) = raw_request(&app, Method::GET, "/").await;
        let index_str = String::from_utf8_lossy(&index_bytes);

        let js_path = extract_attribute(&index_str, "src=\"", "\"")
            .expect("Could not find a <script src=\"/...\"> in index.html");

        let (_, headers, _) = raw_request(&app, Method::GET, &js_path).await;

        let cache_control = header_value(&headers, header::CACHE_CONTROL);
        assert!(
            cache_control.contains("immutable"),
            "Hashed assets should have immutable cache-control, got: {}",
            cache_control
        );
        assert!(
            cache_control.contains("max-age=31536000"),
            "Hashed assets should have max-age=31536000, got: {}",
            cache_control
        );
    }

    #[tokio::test]
    async fn test_nonexistent_static_file_falls_back_to_index() {
        let app = TestApp::new().await;

        // A request for a file that looks like a static asset but doesn't exist
        // should still get index.html (SPA fallback).
        let (status, headers, body) =
            raw_request(&app, Method::GET, "/assets/nonexistent-file.xyz").await;
        let body_str = String::from_utf8_lossy(&body);

        assert_eq!(status, StatusCode::OK);
        assert!(header_value(&headers, header::CONTENT_TYPE).contains("text/html"));
        assert!(body_str.contains("<div id=\"root\">"));
    }

    // ─── Helpers ──────────────────────────────────────────────────────────

    /// Extract a value from HTML by finding text between a prefix and a suffix.
    /// For example, to extract `foo.js` from `src="/foo.js"`, use
    /// `extract_attribute(html, "src=\"/", "\"")` which returns `Some("/foo.js")`.
    fn extract_attribute(html: &str, after: &str, until: &str) -> Option<String> {
        let start = html.find(after)? + after.len();
        let rest = &html[start..];
        let end = rest.find(until)?;
        Some(rest[..end].to_string())
    }
}
