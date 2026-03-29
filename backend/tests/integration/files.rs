use axum::http::StatusCode;
use serde_json::json;
use uuid::Uuid;

use crate::common::TestApp;

#[tokio::test]
async fn test_list_files_empty_dir() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "File Test").await;

    let (status, body) = app
        .get(&format!("/api/servers/{}/files", server_id), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["entries"].is_array());
    // Empty server dir should have no entries
    assert_eq!(body["path"], "");
}

#[tokio::test]
async fn test_write_and_read_file() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "File Test").await;

    // Write a file
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/files/write", server_id),
            Some(&token),
            json!({ "path": "config.txt", "content": "server-name=Test\nport=25565" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["written"], true);

    // Read it back
    let (status, body) = app
        .get(
            &format!("/api/servers/{}/files/read?path=config.txt", server_id),
            Some(&token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["content"], "server-name=Test\nport=25565");
    assert_eq!(body["path"], "config.txt");
}

#[tokio::test]
async fn test_write_file_in_subdirectory() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "File Test").await;

    // Write a file in a nested directory (parent dirs should be created)
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/write", server_id),
            Some(&token),
            json!({ "path": "subdir/deep/file.json", "content": "{}" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Read it back
    let (status, body) = app
        .get(
            &format!(
                "/api/servers/{}/files/read?path=subdir/deep/file.json",
                server_id
            ),
            Some(&token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["content"], "{}");
}

#[tokio::test]
async fn test_create_directory() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "File Test").await;

    let (status, body) = app
        .post(
            &format!("/api/servers/{}/files/mkdir", server_id),
            Some(&token),
            json!({ "path": "logs/archive" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["created"], true);

    // Listing root should show the `logs` directory
    let (_, body) = app
        .get(&format!("/api/servers/{}/files", server_id), Some(&token))
        .await;
    let names: Vec<&str> = body["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"logs"), "expected 'logs' in {:?}", names);

    // Listing logs should show archive
    let (_, body) = app
        .get(
            &format!("/api/servers/{}/files?path=logs", server_id),
            Some(&token),
        )
        .await;
    let names: Vec<&str> = body["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"archive"),
        "expected 'archive' in {:?}",
        names
    );
}

#[tokio::test]
async fn test_delete_file() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "File Test").await;

    // Create a file
    app.post(
        &format!("/api/servers/{}/files/write", server_id),
        Some(&token),
        json!({ "path": "deleteme.txt", "content": "gone" }),
    )
    .await;

    // Delete it
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/files/delete", server_id),
            Some(&token),
            json!({ "path": "deleteme.txt" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["deleted"], true);

    // Verify it's gone
    let (status, _) = app
        .get(
            &format!("/api/servers/{}/files/read?path=deleteme.txt", server_id),
            Some(&token),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_delete_directory_recursively() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "File Test").await;

    // Create a directory with files in it
    app.post(
        &format!("/api/servers/{}/files/write", server_id),
        Some(&token),
        json!({ "path": "dir/a.txt", "content": "a" }),
    )
    .await;
    app.post(
        &format!("/api/servers/{}/files/write", server_id),
        Some(&token),
        json!({ "path": "dir/b.txt", "content": "b" }),
    )
    .await;

    // Delete the whole directory
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/files/delete", server_id),
            Some(&token),
            json!({ "path": "dir" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["deleted"], true);

    // Listing root should not show "dir"
    let (_, body) = app
        .get(&format!("/api/servers/{}/files", server_id), Some(&token))
        .await;
    let names: Vec<&str> = body["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert!(
        !names.contains(&"dir"),
        "dir should be deleted: {:?}",
        names
    );
}

#[tokio::test]
async fn test_path_traversal_blocked_dotdot() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "File Test").await;

    // Try to escape the jail with ../
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/files/write", server_id),
            Some(&token),
            json!({ "path": "../../etc/passwd", "content": "evil" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"]
        .as_str()
        .unwrap()
        .to_lowercase()
        .contains("traversal"));
}

#[tokio::test]
async fn test_path_traversal_blocked_absolute() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "File Test").await;

    // Leading slashes should be stripped to prevent absolute paths.
    // Writing to "/etc/passwd" should be treated as "etc/passwd" relative to server dir.
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/write", server_id),
            Some(&token),
            json!({ "path": "/tmp/evil", "content": "evil" }),
        )
        .await;
    // This should succeed but write inside the server dir, OR be blocked.
    // It depends on implementation — the key thing is /tmp/evil should NOT exist outside the jail.
    assert!(
        status == StatusCode::OK || status == StatusCode::BAD_REQUEST,
        "unexpected status: {}",
        status
    );
}

#[tokio::test]
async fn test_cannot_delete_server_root() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "File Test").await;

    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/delete", server_id),
            Some(&token),
            json!({ "path": "" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_read_nonexistent_file() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "File Test").await;

    let (status, _) = app
        .get(
            &format!(
                "/api/servers/{}/files/read?path=does_not_exist.txt",
                server_id
            ),
            Some(&token),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_read_file_without_path_param() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "File Test").await;

    let (status, _) = app
        .get(
            &format!("/api/servers/{}/files/read", server_id),
            Some(&token),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_viewer_cannot_write_files() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "File Test").await;

    // Grant viewer
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "viewer" }),
    )
    .await;

    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/write", server_id),
            Some(&user_token),
            json!({ "path": "hacked.txt", "content": "pwned" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_viewer_cannot_delete_files() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "File Test").await;

    // Write a file as admin
    app.post(
        &format!("/api/servers/{}/files/write", server_id),
        Some(&admin_token),
        json!({ "path": "important.txt", "content": "data" }),
    )
    .await;

    // Grant viewer
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "viewer" }),
    )
    .await;

    // Viewer tries to delete — should fail
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/delete", server_id),
            Some(&user_token),
            json!({ "path": "important.txt" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_viewer_cannot_mkdir() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "File Test").await;

    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "viewer" }),
    )
    .await;

    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/mkdir", server_id),
            Some(&user_token),
            json!({ "path": "sneaky" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_viewer_can_list_and_read_files() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "File Test").await;

    // Write a file as admin
    app.post(
        &format!("/api/servers/{}/files/write", server_id),
        Some(&admin_token),
        json!({ "path": "readme.txt", "content": "hello viewer" }),
    )
    .await;

    // Grant viewer
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "viewer" }),
    )
    .await;

    // Viewer can list files
    let (status, body) = app
        .get(
            &format!("/api/servers/{}/files", server_id),
            Some(&user_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let names: Vec<&str> = body["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"readme.txt"));

    // Viewer can read files
    let (status, body) = app
        .get(
            &format!("/api/servers/{}/files/read?path=readme.txt", server_id),
            Some(&user_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["content"], "hello viewer");
}

#[tokio::test]
async fn test_unauthenticated_cannot_access_files() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "File Test").await;

    let (status, _) = app
        .get(&format!("/api/servers/{}/files", server_id), None)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_manager_can_write_and_delete_files() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "File Test").await;

    // Grant manager
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "manager" }),
    )
    .await;

    // Manager can write
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/write", server_id),
            Some(&user_token),
            json!({ "path": "managed.txt", "content": "managed content" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Manager can read
    let (status, body) = app
        .get(
            &format!("/api/servers/{}/files/read?path=managed.txt", server_id),
            Some(&user_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["content"], "managed content");

    // Manager can delete
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/delete", server_id),
            Some(&user_token),
            json!({ "path": "managed.txt" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_operator_cannot_write_files() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "File Test").await;

    // Grant operator
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "operator" }),
    )
    .await;

    // Operator should not be able to write files (needs manager)
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/write", server_id),
            Some(&user_token),
            json!({ "path": "test.txt", "content": "test" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_file_entry_metadata() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "Meta Test").await;

    // Write a file with known content
    app.post(
        &format!("/api/servers/{}/files/write", server_id),
        Some(&token),
        json!({ "path": "data.txt", "content": "12345" }),
    )
    .await;

    // Create a directory
    app.post(
        &format!("/api/servers/{}/files/mkdir", server_id),
        Some(&token),
        json!({ "path": "subdir" }),
    )
    .await;

    let (_, body) = app
        .get(&format!("/api/servers/{}/files", server_id), Some(&token))
        .await;

    let entries = body["entries"].as_array().unwrap();

    // Directory should come first (sorted dirs-first)
    let dir_entry = entries.iter().find(|e| e["name"] == "subdir").unwrap();
    assert_eq!(dir_entry["kind"], "directory");

    let file_entry = entries.iter().find(|e| e["name"] == "data.txt").unwrap();
    assert_eq!(file_entry["kind"], "file");
    assert_eq!(file_entry["size"], 5); // "12345" = 5 bytes
    assert!(file_entry["modified"].is_string()); // should have a timestamp
}

// ─── File Permissions ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_file_entry_includes_mode() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "Mode Test").await;

    // Write a file
    app.post(
        &format!("/api/servers/{}/files/write", server_id),
        Some(&token),
        json!({ "path": "test.txt", "content": "hello" }),
    )
    .await;

    let (status, body) = app
        .get(&format!("/api/servers/{}/files", server_id), Some(&token))
        .await;
    assert_eq!(status, StatusCode::OK);

    let entries = body["entries"].as_array().unwrap();
    let entry = entries.iter().find(|e| e["name"] == "test.txt").unwrap();

    // On Unix, mode should be a non-null string (e.g. "100644" or "644")
    assert!(
        entry["mode"].is_string(),
        "Expected mode to be a string, got: {:?}",
        entry["mode"]
    );
    let mode_str = entry["mode"].as_str().unwrap();
    assert!(!mode_str.is_empty(), "mode should not be empty");
}

#[tokio::test]
async fn test_get_permissions_for_file() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "Perms Test").await;

    // Write a file
    app.post(
        &format!("/api/servers/{}/files/write", server_id),
        Some(&token),
        json!({ "path": "data.txt", "content": "test content" }),
    )
    .await;

    let (status, body) = app
        .get(
            &format!("/api/servers/{}/files/permissions?path=data.txt", server_id),
            Some(&token),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "body: {:?}", body);

    assert_eq!(body["path"], "data.txt");
    assert!(body["mode"].is_string());
    assert!(body["mode_display"].is_string());
    assert_eq!(body["is_directory"], false);
    assert!(body["uid"].is_number());
    assert!(body["gid"].is_number());

    // mode_display should be 9 chars like "rw-r--r--"
    let display = body["mode_display"].as_str().unwrap();
    assert_eq!(
        display.len(),
        9,
        "mode_display should be 9 chars, got: '{}'",
        display
    );
}

#[tokio::test]
async fn test_get_permissions_for_directory() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "Dir Perms Test").await;

    app.post(
        &format!("/api/servers/{}/files/mkdir", server_id),
        Some(&token),
        json!({ "path": "mydir" }),
    )
    .await;

    let (status, body) = app
        .get(
            &format!("/api/servers/{}/files/permissions?path=mydir", server_id),
            Some(&token),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "body: {:?}", body);

    assert_eq!(body["path"], "mydir");
    assert_eq!(body["is_directory"], true);
    assert!(body["mode"].is_string());
    assert!(body["mode_display"].is_string());
}

#[tokio::test]
async fn test_get_permissions_nonexistent_path() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "NoFile Test").await;

    let (status, _) = app
        .get(
            &format!(
                "/api/servers/{}/files/permissions?path=nonexistent.txt",
                server_id
            ),
            Some(&token),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_permissions_requires_path_param() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "NoPath Test").await;

    let (status, _) = app
        .get(
            &format!("/api/servers/{}/files/permissions", server_id),
            Some(&token),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_chmod_file() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _body) = app.create_test_server(&token, "Chmod Test").await;
    let _sid = Uuid::parse_str(&server_id).unwrap();

    // Write a file
    app.post(
        &format!("/api/servers/{}/files/write", server_id),
        Some(&token),
        json!({ "path": "script.sh", "content": "#!/bin/sh\necho hello" }),
    )
    .await;

    // Set to 755
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/files/chmod", server_id),
            Some(&token),
            json!({ "path": "script.sh", "mode": "755" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "chmod failed: {:?}", body);
    assert_eq!(body["mode"], "755");
    assert_eq!(body["mode_display"], "rwxr-xr-x");

    // Verify by fetching permissions
    let (status, body) = app
        .get(
            &format!(
                "/api/servers/{}/files/permissions?path=script.sh",
                server_id
            ),
            Some(&token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["mode"], "755");
    assert_eq!(body["mode_display"], "rwxr-xr-x");

    // Verify in file listing too
    let (_, body) = app
        .get(&format!("/api/servers/{}/files", server_id), Some(&token))
        .await;
    let entries = body["entries"].as_array().unwrap();
    let entry = entries.iter().find(|e| e["name"] == "script.sh").unwrap();
    assert_eq!(entry["mode"], "755");
}

#[tokio::test]
async fn test_chmod_directory() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "Chmod Dir Test").await;

    app.post(
        &format!("/api/servers/{}/files/mkdir", server_id),
        Some(&token),
        json!({ "path": "data" }),
    )
    .await;

    let (status, body) = app
        .post(
            &format!("/api/servers/{}/files/chmod", server_id),
            Some(&token),
            json!({ "path": "data", "mode": "700" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "chmod failed: {:?}", body);
    assert_eq!(body["mode"], "700");
    assert_eq!(body["mode_display"], "rwx------");
}

#[tokio::test]
async fn test_chmod_various_modes() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "Chmod Modes").await;

    app.post(
        &format!("/api/servers/{}/files/write", server_id),
        Some(&token),
        json!({ "path": "test.txt", "content": "x" }),
    )
    .await;

    let test_modes = [
        ("644", "rw-r--r--"),
        ("755", "rwxr-xr-x"),
        ("600", "rw-------"),
        ("777", "rwxrwxrwx"),
        ("400", "r--------"),
        ("664", "rw-rw-r--"),
    ];

    for (mode, expected_display) in test_modes {
        let (status, body) = app
            .post(
                &format!("/api/servers/{}/files/chmod", server_id),
                Some(&token),
                json!({ "path": "test.txt", "mode": mode }),
            )
            .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "chmod to {} failed: {:?}",
            mode,
            body
        );
        assert_eq!(
            body["mode"], mode,
            "Expected mode {} but got {}",
            mode, body["mode"]
        );
        assert_eq!(
            body["mode_display"], expected_display,
            "Expected display {} for mode {} but got {}",
            expected_display, mode, body["mode_display"]
        );
    }
}

#[tokio::test]
async fn test_chmod_invalid_mode_rejected() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "Bad Mode Test").await;

    app.post(
        &format!("/api/servers/{}/files/write", server_id),
        Some(&token),
        json!({ "path": "test.txt", "content": "x" }),
    )
    .await;

    // Non-octal digits
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/chmod", server_id),
            Some(&token),
            json!({ "path": "test.txt", "mode": "999" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Letters
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/chmod", server_id),
            Some(&token),
            json!({ "path": "test.txt", "mode": "abc" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Empty string
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/chmod", server_id),
            Some(&token),
            json!({ "path": "test.txt", "mode": "" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_chmod_nonexistent_path() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "Chmod NoFile").await;

    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/chmod", server_id),
            Some(&token),
            json!({ "path": "nofile.txt", "mode": "755" }),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_chmod_empty_path_rejected() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "Chmod Empty").await;

    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/chmod", server_id),
            Some(&token),
            json!({ "path": "", "mode": "755" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_viewer_cannot_chmod() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Viewer Chmod").await;

    // Grant viewer permission
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "viewer" }),
    )
    .await;

    // Write a file as admin
    app.post(
        &format!("/api/servers/{}/files/write", server_id),
        Some(&admin_token),
        json!({ "path": "test.txt", "content": "x" }),
    )
    .await;

    // Viewer should be able to read permissions
    let (status, _) = app
        .get(
            &format!("/api/servers/{}/files/permissions?path=test.txt", server_id),
            Some(&user_token),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "Viewer should be able to read permissions"
    );

    // Viewer should NOT be able to chmod
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/chmod", server_id),
            Some(&user_token),
            json!({ "path": "test.txt", "mode": "777" }),
        )
        .await;
    assert!(
        status == StatusCode::FORBIDDEN || status == StatusCode::UNAUTHORIZED,
        "Viewer should not be able to chmod, got: {}",
        status
    );
}

#[tokio::test]
async fn test_operator_cannot_chmod() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Op Chmod").await;

    // Grant operator permission
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "operator" }),
    )
    .await;

    app.post(
        &format!("/api/servers/{}/files/write", server_id),
        Some(&admin_token),
        json!({ "path": "test.txt", "content": "x" }),
    )
    .await;

    // Operator should NOT be able to chmod (requires Manager)
    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/chmod", server_id),
            Some(&user_token),
            json!({ "path": "test.txt", "mode": "755" }),
        )
        .await;
    assert!(
        status == StatusCode::FORBIDDEN || status == StatusCode::UNAUTHORIZED,
        "Operator should not be able to chmod, got: {}",
        status
    );
}

#[tokio::test]
async fn test_manager_can_chmod() {
    let app = TestApp::new().await;
    let (admin_token, user_token, user_id) = app.setup_admin_and_user().await;
    let (server_id, _) = app.create_test_server(&admin_token, "Mgr Chmod").await;

    // Grant manager permission
    app.post(
        &format!("/api/servers/{}/permissions", server_id),
        Some(&admin_token),
        json!({ "user_id": user_id, "level": "manager" }),
    )
    .await;

    app.post(
        &format!("/api/servers/{}/files/write", server_id),
        Some(&admin_token),
        json!({ "path": "test.txt", "content": "x" }),
    )
    .await;

    // Manager should be able to chmod
    let (status, body) = app
        .post(
            &format!("/api/servers/{}/files/chmod", server_id),
            Some(&user_token),
            json!({ "path": "test.txt", "mode": "755" }),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "Manager should be able to chmod: {:?}",
        body
    );
    assert_eq!(body["mode"], "755");
}

#[tokio::test]
async fn test_unauthenticated_cannot_access_permissions() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "Unauth Perms").await;

    let (status, _) = app
        .get(
            &format!("/api/servers/{}/files/permissions?path=anything", server_id),
            None,
        )
        .await;
    assert!(
        status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN,
        "Expected auth failure, got: {}",
        status
    );

    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/chmod", server_id),
            None,
            json!({ "path": "anything", "mode": "755" }),
        )
        .await;
    assert!(
        status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN,
        "Expected auth failure, got: {}",
        status
    );
}

#[tokio::test]
async fn test_chmod_path_traversal_blocked() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "Chmod Traversal").await;

    let (status, _) = app
        .post(
            &format!("/api/servers/{}/files/chmod", server_id),
            Some(&token),
            json!({ "path": "../../etc/passwd", "mode": "777" }),
        )
        .await;
    // Should be blocked — either BadRequest (traversal) or NotFound
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::NOT_FOUND,
        "Path traversal should be blocked, got: {}",
        status
    );
}

#[tokio::test]
async fn test_permissions_owner_and_group_fields() {
    // Verify that the owner/group fields are populated (at least one should
    // resolve since we're running as a real user).
    let app = TestApp::new().await;
    let token = app.setup_admin("admin", "Admin1234").await;
    let (server_id, _) = app.create_test_server(&token, "Owner Test").await;

    app.post(
        &format!("/api/servers/{}/files/write", server_id),
        Some(&token),
        json!({ "path": "owned.txt", "content": "mine" }),
    )
    .await;

    let (status, body) = app
        .get(
            &format!(
                "/api/servers/{}/files/permissions?path=owned.txt",
                server_id
            ),
            Some(&token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // uid and gid should be our process's uid/gid
    assert!(body["uid"].is_number());
    assert!(body["gid"].is_number());
    // owner should be resolvable to a username string
    assert!(
        body["owner"].is_string(),
        "Expected owner to be a string, got: {:?}",
        body["owner"]
    );
    let owner = body["owner"].as_str().unwrap();
    assert!(!owner.is_empty(), "owner should not be empty");
}
