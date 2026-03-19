// Extended server API tests covering: comments, labels, blockers, subissues,
// usage tracking, session work-on, milestone assignment, agent scoping,
// and various filter combinations.

use super::harness::SmokeHarness;
use super::server_api::{http_request, parse_json};

// ===========================================================================
// Comments API
// ===========================================================================

#[test]
fn test_api_add_comment() {
    let mut h = SmokeHarness::new();
    h.run_ok(&["issue", "create", "Comment API test"]);

    let port = h.start_server();

    let payload = r#"{"content": "First comment via API", "kind": "note"}"#;
    let (status, body) = http_request(port, "POST", "/api/v1/issues/1/comments", Some(payload));
    assert!(
        status == 200 || status == 201,
        "Add comment should return 200/201, got {}",
        status
    );

    let json = parse_json(&body);
    assert_eq!(json["content"], "First comment via API");
}

#[test]
fn test_api_list_comments() {
    let mut h = SmokeHarness::new();
    h.run_ok(&["issue", "create", "Comments list test"]);
    h.run_ok(&["issue", "comment", "1", "CLI comment one", "--kind", "note"]);
    h.run_ok(&["issue", "comment", "1", "CLI comment two", "--kind", "plan"]);

    let port = h.start_server();

    let (status, body) = http_request(port, "GET", "/api/v1/issues/1/comments", None);
    assert_eq!(status, 200);

    let json = parse_json(&body);
    let items = json["items"]
        .as_array()
        .expect("comments should have items array");
    assert!(
        items.len() >= 2,
        "Should have at least 2 comments, got {}",
        items.len()
    );
}

#[test]
fn test_api_comment_on_nonexistent_issue() {
    let mut h = SmokeHarness::new();
    let port = h.start_server();

    let payload = r#"{"content": "Ghost comment", "kind": "note"}"#;
    let (status, _) = http_request(port, "POST", "/api/v1/issues/99999/comments", Some(payload));
    assert_eq!(
        status, 404,
        "Adding comment to nonexistent issue should return 404"
    );
}

// ===========================================================================
// Labels API
// ===========================================================================

#[test]
fn test_api_add_label() {
    let mut h = SmokeHarness::new();
    h.run_ok(&["issue", "create", "Label API test"]);

    let port = h.start_server();

    let payload = r#"{"label": "api-label"}"#;
    let (status, _) = http_request(port, "POST", "/api/v1/issues/1/labels", Some(payload));
    assert_eq!(status, 200, "Add label should return 200, got {}", status);

    // Verify via GET
    let (_, body) = http_request(port, "GET", "/api/v1/issues/1", None);
    let json = parse_json(&body);
    let labels = json["labels"].as_array().expect("Should have labels array");
    assert!(
        labels
            .iter()
            .any(|l: &serde_json::Value| l.as_str() == Some("api-label")),
        "Issue should have api-label, got: {:?}",
        labels
    );
}

#[test]
fn test_api_remove_label() {
    let mut h = SmokeHarness::new();
    h.run_ok(&["issue", "create", "Remove label API test"]);
    h.run_ok(&["issue", "label", "1", "remove-me"]);
    h.run_ok(&["issue", "label", "1", "keep-me"]);

    let port = h.start_server();

    let (status, _) = http_request(port, "DELETE", "/api/v1/issues/1/labels/remove-me", None);
    assert_eq!(
        status, 200,
        "Remove label should return 200, got {}",
        status
    );

    // Verify label was removed
    let (_, body) = http_request(port, "GET", "/api/v1/issues/1", None);
    let json = parse_json(&body);
    let labels = json["labels"].as_array().expect("Should have labels array");
    assert!(
        !labels
            .iter()
            .any(|l: &serde_json::Value| l.as_str() == Some("remove-me")),
        "Label 'remove-me' should be gone, got: {:?}",
        labels
    );
    assert!(
        labels
            .iter()
            .any(|l: &serde_json::Value| l.as_str() == Some("keep-me")),
        "Label 'keep-me' should remain, got: {:?}",
        labels
    );
}

// ===========================================================================
// Blockers API
// ===========================================================================

#[test]
fn test_api_add_blocker() {
    let mut h = SmokeHarness::new();
    h.run_ok(&["issue", "create", "Blocked via API"]);
    h.run_ok(&["issue", "create", "Blocker via API"]);

    let port = h.start_server();

    let payload = r#"{"blocker_id": 2}"#;
    let (status, _) = http_request(port, "POST", "/api/v1/issues/1/block", Some(payload));
    assert_eq!(status, 200, "Add blocker should return 200, got {}", status);

    // Verify via GET
    let (_, body) = http_request(port, "GET", "/api/v1/issues/1", None);
    let json = parse_json(&body);
    let blockers = json["blockers"]
        .as_array()
        .expect("Should have blockers array");
    assert!(
        !blockers.is_empty(),
        "Issue should have at least one blocker"
    );
}

#[test]
fn test_api_remove_blocker() {
    let mut h = SmokeHarness::new();
    h.run_ok(&["issue", "create", "Unblock via API"]);
    h.run_ok(&["issue", "create", "Blocker to remove"]);
    h.run_ok(&["issue", "block", "1", "2"]);

    let port = h.start_server();

    let (status, _) = http_request(port, "DELETE", "/api/v1/issues/1/block/2", None);
    assert_eq!(
        status, 200,
        "Remove blocker should return 200, got {}",
        status
    );

    // Verify blocker is removed
    let (_, body) = http_request(port, "GET", "/api/v1/issues/1", None);
    let json = parse_json(&body);
    let blockers = json["blockers"]
        .as_array()
        .expect("Should have blockers array");
    assert!(
        blockers.is_empty(),
        "Blockers should be empty after removal, got: {:?}",
        blockers
    );
}

// ===========================================================================
// Subissues API
// ===========================================================================

#[test]
fn test_api_create_subissue() {
    let mut h = SmokeHarness::new();
    h.run_ok(&["issue", "create", "Parent via API"]);

    let port = h.start_server();

    let payload = r#"{"title": "Child via API", "priority": "medium"}"#;
    let (status, body) = http_request(port, "POST", "/api/v1/issues/1/subissue", Some(payload));
    assert!(
        status == 200 || status == 201,
        "Create subissue should return 200/201, got {}",
        status
    );

    let json = parse_json(&body);
    assert_eq!(json["title"], "Child via API");
    assert_eq!(json["parent_id"], 1);
}

// ===========================================================================
// Session Work-on API
// ===========================================================================

#[test]
fn test_api_session_work_on() {
    let mut h = SmokeHarness::new();
    h.run_ok(&["issue", "create", "Work target issue"]);

    let port = h.start_server();

    // Start session first
    http_request(port, "POST", "/api/v1/sessions/start", Some("{}"));

    // Work on issue 1
    let (status, body) = http_request(port, "POST", "/api/v1/sessions/work/1", None);
    assert_eq!(
        status, 200,
        "Work on issue should return 200, got {}",
        status
    );
    let json = parse_json(&body);
    assert_eq!(json["ok"], true);

    // Get current session — should show active_issue_id
    let (_, body) = http_request(port, "GET", "/api/v1/sessions/current", None);
    let json = parse_json(&body);
    assert_eq!(json["active_issue_id"], 1);
}

// ===========================================================================
// Milestone Assignment API
// ===========================================================================

#[test]
fn test_api_milestone_assign() {
    let mut h = SmokeHarness::new();
    h.run_ok(&["issue", "create", "Milestone target"]);

    let port = h.start_server();

    // Create milestone
    let payload = r#"{"name": "api-ms"}"#;
    let (status, body) = http_request(port, "POST", "/api/v1/milestones", Some(payload));
    assert_eq!(status, 200);
    let ms = parse_json(&body);
    let ms_id = ms["id"].as_i64().unwrap();

    // Assign issue to milestone (body: {"issue_id": N})
    let assign_payload = format!(r#"{{"issue_id": 1}}"#);
    let (status, _) = http_request(
        port,
        "POST",
        &format!("/api/v1/milestones/{}/assign", ms_id),
        Some(&assign_payload),
    );
    assert_eq!(
        status, 200,
        "Assign to milestone should return 200, got {}",
        status
    );

    // Verify milestone progress
    let (_, body) = http_request(port, "GET", &format!("/api/v1/milestones/{}", ms_id), None);
    let json = parse_json(&body);
    assert_eq!(json["issue_count"], 1);
}

#[test]
fn test_api_milestone_close() {
    let mut h = SmokeHarness::new();
    let port = h.start_server();

    // Create milestone
    let payload = r#"{"name": "close-me-ms"}"#;
    let (status, body) = http_request(port, "POST", "/api/v1/milestones", Some(payload));
    assert_eq!(status, 200);
    let ms = parse_json(&body);
    let ms_id = ms["id"].as_i64().unwrap();

    // Close it — returns {"ok": true}
    let (status, body) = http_request(
        port,
        "POST",
        &format!("/api/v1/milestones/{}/close", ms_id),
        None,
    );
    assert_eq!(status, 200, "Close milestone should return 200");
    let json = parse_json(&body);
    assert_eq!(json["ok"], true);
}

// ===========================================================================
// Usage Tracking API
// ===========================================================================

#[test]
fn test_api_usage_create_and_list() {
    let mut h = SmokeHarness::new();
    let port = h.start_server();

    let payload = r#"{
        "agent_id": "test-agent",
        "input_tokens": 1000,
        "output_tokens": 500,
        "model": "claude-3-opus"
    }"#;
    let (status, _) = http_request(port, "POST", "/api/v1/usage", Some(payload));
    assert!(
        status == 200 || status == 201,
        "Create usage should return 200/201, got {}",
        status
    );

    // List usage
    let (status, body) = http_request(port, "GET", "/api/v1/usage", None);
    assert_eq!(status, 200);
    let json = parse_json(&body);
    let total = json["total"].as_u64().unwrap_or(0);
    assert!(total >= 1, "Should have at least 1 usage entry");
}

#[test]
fn test_api_usage_summary() {
    let mut h = SmokeHarness::new();
    let port = h.start_server();

    // Create multiple usage entries
    for i in 0..3 {
        let payload = format!(
            r#"{{
                "agent_id": "summary-agent",
                "input_tokens": {},
                "output_tokens": {},
                "model": "claude-3-opus"
            }}"#,
            1000 * (i + 1),
            500 * (i + 1)
        );
        http_request(port, "POST", "/api/v1/usage", Some(&payload));
    }

    // Get summary
    let (status, body) = http_request(port, "GET", "/api/v1/usage/summary", None);
    assert_eq!(status, 200);
    let json = parse_json(&body);
    assert!(json["items"].is_array(), "Summary should have items array");
}

// ===========================================================================
// Filter Combinations
// ===========================================================================

#[test]
fn test_api_list_issues_status_filter() {
    let mut h = SmokeHarness::new();
    h.run_ok(&["issue", "create", "Open one"]);
    h.run_ok(&["issue", "create", "To close"]);
    h.run_ok(&["issue", "close", "2"]);

    let port = h.start_server();

    // Filter by open
    let (status, body) = http_request(port, "GET", "/api/v1/issues?status=open", None);
    assert_eq!(status, 200);
    let json = parse_json(&body);
    assert_eq!(json["total"], 1, "Should have 1 open issue");

    // Filter by closed
    let (status, body) = http_request(port, "GET", "/api/v1/issues?status=closed", None);
    assert_eq!(status, 200);
    let json = parse_json(&body);
    assert_eq!(json["total"], 1, "Should have 1 closed issue");

    // All
    let (status, body) = http_request(port, "GET", "/api/v1/issues?status=all", None);
    assert_eq!(status, 200);
    let json = parse_json(&body);
    assert_eq!(json["total"], 2, "Should have 2 total issues");
}

#[test]
fn test_api_list_issues_label_filter() {
    let mut h = SmokeHarness::new();
    h.run_ok(&["issue", "create", "Bug issue"]);
    h.run_ok(&["issue", "create", "Feature issue"]);
    h.run_ok(&["issue", "label", "1", "bug"]);
    h.run_ok(&["issue", "label", "2", "feature"]);

    let port = h.start_server();

    let (status, body) = http_request(port, "GET", "/api/v1/issues?label=bug", None);
    assert_eq!(status, 200);
    let json = parse_json(&body);
    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "Should have 1 bug issue");
    assert_eq!(items[0]["title"], "Bug issue");
}

#[test]
fn test_api_list_issues_priority_filter() {
    let mut h = SmokeHarness::new();
    h.run_ok(&["issue", "create", "Critical task", "-p", "critical"]);
    h.run_ok(&["issue", "create", "Low task", "-p", "low"]);

    let port = h.start_server();

    let (status, body) = http_request(port, "GET", "/api/v1/issues?priority=critical", None);
    assert_eq!(status, 200);
    let json = parse_json(&body);
    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "Should have 1 critical issue");
    assert_eq!(items[0]["title"], "Critical task");
}

#[test]
fn test_api_list_issues_search_filter() {
    let mut h = SmokeHarness::new();
    h.run_ok(&["issue", "create", "Fix authentication bug"]);
    h.run_ok(&["issue", "create", "Add dashboard widget"]);

    let port = h.start_server();

    let (status, body) = http_request(port, "GET", "/api/v1/issues?search=authentication", None);
    assert_eq!(status, 200);
    let json = parse_json(&body);
    let items = json["items"].as_array().unwrap();
    assert!(
        items.len() >= 1,
        "Should find at least 1 issue matching 'authentication'"
    );
}

// ===========================================================================
// Agent Scoping
// ===========================================================================

#[test]
fn test_api_session_agent_scoping() {
    let mut h = SmokeHarness::new();
    let port = h.start_server();

    // Start session with agent_id in body
    let (status, _) = http_request(
        port,
        "POST",
        "/api/v1/sessions/start",
        Some(r#"{"agent_id": "test-agent"}"#),
    );
    assert_eq!(status, 200);

    // Get current session with agent scoping (query param)
    let (status, body) = http_request(
        port,
        "GET",
        "/api/v1/sessions/current?agent_id=test-agent",
        None,
    );
    assert_eq!(status, 200);
    let json = parse_json(&body);
    assert!(json["id"].as_i64().is_some());
    assert_eq!(json["agent_id"], "test-agent");

    // Get current session for a different agent — should be 404
    let (status, _) = http_request(
        port,
        "GET",
        "/api/v1/sessions/current?agent_id=other-agent",
        None,
    );
    assert_eq!(status, 404, "Different agent should not see this session");
}

// ===========================================================================
// Validation Errors
// ===========================================================================

#[test]
fn test_api_create_issue_invalid_priority() {
    let mut h = SmokeHarness::new();
    let port = h.start_server();

    let payload = r#"{"title": "Bad priority", "priority": "ULTRA"}"#;
    let (status, _) = http_request(port, "POST", "/api/v1/issues", Some(payload));
    assert!(
        status == 400 || status == 422,
        "Invalid priority should return 400 or 422, got {}",
        status
    );
}

#[test]
fn test_api_create_issue_empty_title() {
    let mut h = SmokeHarness::new();
    let port = h.start_server();

    let payload = r#"{"title": ""}"#;
    let (status, _) = http_request(port, "POST", "/api/v1/issues", Some(payload));
    // Empty title may be rejected or accepted; either is fine as long as no crash
    assert!(
        status == 200 || status == 201 || status == 400 || status == 422,
        "Empty title should be handled gracefully, got {}",
        status
    );
}

#[test]
fn test_api_create_issue_title_too_long() {
    let mut h = SmokeHarness::new();
    let port = h.start_server();

    let long_title = "a".repeat(600);
    let payload = format!(r#"{{"title": "{}"}}"#, long_title);
    let (status, _) = http_request(port, "POST", "/api/v1/issues", Some(&payload));
    assert!(
        status == 400 || status == 422 || status == 200 || status == 201,
        "Long title should be handled, got {}",
        status
    );
}

// ===========================================================================
// Usage Filtering
// ===========================================================================

#[test]
fn test_api_usage_agent_filter() {
    let mut h = SmokeHarness::new();
    let port = h.start_server();

    // Create usage for two agents
    let payload_a =
        r#"{"agent_id": "agent-a", "input_tokens": 100, "output_tokens": 50, "model": "opus"}"#;
    let payload_b =
        r#"{"agent_id": "agent-b", "input_tokens": 200, "output_tokens": 100, "model": "opus"}"#;
    http_request(port, "POST", "/api/v1/usage", Some(payload_a));
    http_request(port, "POST", "/api/v1/usage", Some(payload_b));

    // Filter by agent-a
    let (status, body) = http_request(port, "GET", "/api/v1/usage?agent_id=agent-a", None);
    assert_eq!(status, 200);
    let json = parse_json(&body);
    let items = json["items"].as_array().unwrap();
    for item in items {
        assert_eq!(item["agent_id"], "agent-a");
    }
}

// ===========================================================================
// Milestones Filter
// ===========================================================================

#[test]
fn test_api_milestones_status_filter() {
    let mut h = SmokeHarness::new();
    let port = h.start_server();

    // Create two milestones, close one
    http_request(
        port,
        "POST",
        "/api/v1/milestones",
        Some(r#"{"name": "open-ms"}"#),
    );
    let (_, body) = http_request(
        port,
        "POST",
        "/api/v1/milestones",
        Some(r#"{"name": "close-ms"}"#),
    );
    let ms = parse_json(&body);
    let ms_id = ms["id"].as_i64().unwrap();
    http_request(
        port,
        "POST",
        &format!("/api/v1/milestones/{}/close", ms_id),
        None,
    );

    // Filter by open
    let (_, body) = http_request(port, "GET", "/api/v1/milestones?status=open", None);
    let json = parse_json(&body);
    assert_eq!(json["total"], 1, "Should have 1 open milestone");

    // Filter by closed
    let (_, body) = http_request(port, "GET", "/api/v1/milestones?status=closed", None);
    let json = parse_json(&body);
    assert_eq!(json["total"], 1, "Should have 1 closed milestone");

    // All
    let (_, body) = http_request(port, "GET", "/api/v1/milestones?status=all", None);
    let json = parse_json(&body);
    assert_eq!(json["total"], 2, "Should have 2 total milestones");
}
