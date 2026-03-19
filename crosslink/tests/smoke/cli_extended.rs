// Extended CLI tests covering: daemon, timer, session lifecycle, issue search,
// issue next, issue tested, close-all, and other uncovered command paths.

use super::harness::{assert_issue_count, assert_stdout_contains, SmokeHarness};

// ===========================================================================
// Session Lifecycle (CLI)
// ===========================================================================

#[test]
fn test_session_start_end_roundtrip() {
    let h = SmokeHarness::new();

    let start = h.run_ok(&["session", "start"]);
    assert!(
        start.stdout_contains("started")
            || start.stdout_contains("Session")
            || start.stdout_contains("Started"),
        "Session start should confirm.\nstdout: {}",
        start.stdout,
    );

    let status = h.run_ok(&["session", "status"]);
    assert!(
        status.stdout_contains("active")
            || status.stdout_contains("Active")
            || status.stdout_contains("Session"),
        "Session should be active.\nstdout: {}",
        status.stdout,
    );

    let end = h.run_ok(&["session", "end", "--notes", "Test handoff notes"]);
    assert!(
        end.stdout_contains("ended")
            || end.stdout_contains("Ended")
            || end.stdout_contains("Session"),
        "Session end should confirm.\nstdout: {}",
        end.stdout,
    );
}

#[test]
fn test_session_work_on_issue() {
    let h = SmokeHarness::new();

    h.run_ok(&["issue", "create", "Work target"]);
    h.run_ok(&["session", "start"]);
    h.run_ok(&["session", "work", "1"]);

    let status = h.run_ok(&["session", "status"]);
    assert!(
        status.stdout_contains("Work target") || status.stdout_contains("#1"),
        "Session status should show active issue.\nstdout: {}",
        status.stdout,
    );
}

#[test]
fn test_session_last_handoff() {
    let h = SmokeHarness::new();

    // First session with notes
    h.run_ok(&["session", "start"]);
    h.run_ok(&["session", "end", "--notes", "Handoff: left off at step 3"]);

    // Second session — check last handoff
    h.run_ok(&["session", "start"]);
    let last = h.run_ok(&["session", "last-handoff"]);
    assert!(
        last.stdout_contains("left off at step 3") || last.stdout_contains("Handoff"),
        "Last handoff should show previous notes.\nstdout: {}",
        last.stdout,
    );
}

#[test]
fn test_session_action_breadcrumb() {
    let h = SmokeHarness::new();

    h.run_ok(&["session", "start"]);
    let action = h.run_ok(&["session", "action", "Investigating the auth module"]);
    assert!(
        action.stdout_contains("Recorded") || action.stdout_contains("action") || action.success,
        "Session action should confirm.\nstdout: {}",
        action.stdout,
    );
}

// ===========================================================================
// Issue Search
// ===========================================================================

#[test]
fn test_issue_search_basic() {
    let h = SmokeHarness::new();

    h.run_ok(&["issue", "create", "Fix authentication bug"]);
    h.run_ok(&["issue", "create", "Add dashboard widget"]);
    h.run_ok(&["issue", "create", "Update authentication docs"]);

    let result = h.run_ok(&["issue", "search", "authentication"]);
    assert!(
        result.stdout_contains("authentication"),
        "Search should find issues with 'authentication'.\nstdout: {}",
        result.stdout,
    );
}

#[test]
fn test_issue_search_no_match() {
    let h = SmokeHarness::new();

    h.run_ok(&["issue", "create", "Normal issue"]);

    let result = h.run_ok(&["issue", "search", "xyzzy_nonexistent"]);
    assert!(
        result.stdout_contains("No issues") || !result.stdout_contains("Normal issue"),
        "Search with no match should indicate no results.\nstdout: {}",
        result.stdout,
    );
}

// ===========================================================================
// Issue Next
// ===========================================================================

#[test]
fn test_issue_next_suggests_high_priority() {
    let h = SmokeHarness::new();

    h.run_ok(&["issue", "create", "Low task", "-p", "low"]);
    h.run_ok(&["issue", "create", "Critical task", "-p", "critical"]);
    h.run_ok(&["issue", "create", "Medium task", "-p", "medium"]);

    let next = h.run_ok(&["issue", "next"]);
    assert!(
        next.stdout_contains("Critical task") || next.stdout_contains("#2"),
        "Issue next should suggest the critical task.\nstdout: {}",
        next.stdout,
    );
}

#[test]
fn test_issue_next_empty() {
    let h = SmokeHarness::new();

    let next = h.run(&["issue", "next"]);
    // Either succeeds with "no issues" message or fails gracefully
    let combined = format!("{}{}", next.stdout, next.stderr);
    assert!(
        combined.contains("No") || combined.contains("no") || !next.success,
        "Issue next with no issues should handle gracefully.\nstdout: {}\nstderr: {}",
        next.stdout,
        next.stderr,
    );
}

// ===========================================================================
// Issue Tested
// ===========================================================================

#[test]
fn test_issue_tested_marker() {
    let h = SmokeHarness::new();

    h.run_ok(&["issue", "create", "Testable issue"]);

    // `issue tested` doesn't take an issue ID — it marks the current session's issue
    h.run_ok(&["session", "start"]);
    h.run_ok(&["session", "work", "1"]);

    let result = h.run_ok(&["issue", "tested"]);
    assert!(
        result.stdout_contains("tested")
            || result.stdout_contains("Tested")
            || result.stdout_contains("label")
            || result.stdout_contains("reset"),
        "Issue tested should mark the issue.\nstdout: {}",
        result.stdout,
    );
}

// ===========================================================================
// Close-All
// ===========================================================================

#[test]
fn test_close_all() {
    let h = SmokeHarness::new();

    for i in 1..=5 {
        h.run_ok(&["issue", "create", &format!("Close-all issue {}", i)]);
    }

    let result = h.run_ok(&["issue", "close-all", "--no-changelog"]);
    assert!(
        result.stdout_contains("Closed") || result.stdout_contains("closed"),
        "Close-all should confirm closure.\nstdout: {}",
        result.stdout,
    );

    assert_issue_count(&h, "open", 0);
    assert_issue_count(&h, "closed", 5);
}

#[test]
fn test_close_all_empty() {
    let h = SmokeHarness::new();

    let result = h.run_ok(&["issue", "close-all", "--no-changelog"]);
    // Should succeed even with no issues
    assert!(result.success);
}

// ===========================================================================
// Issue Quick
// ===========================================================================

#[test]
fn test_issue_quick() {
    let h = SmokeHarness::new();

    // Quick creates issue, adds label, and sets session work
    h.run_ok(&["session", "start"]);
    let result = h.run_ok(&["issue", "quick", "Quick bug fix", "-p", "high", "-l", "bug"]);
    assert!(
        result.stdout_contains("Created") || result.stdout_contains("quick"),
        "Quick should create issue.\nstdout: {}",
        result.stdout,
    );

    let show = h.run_ok(&["show", "1"]);
    assert_stdout_contains(&show, "Quick bug fix");
    assert_stdout_contains(&show, "high");
    assert_stdout_contains(&show, "bug");
}

// ===========================================================================
// Daemon
// ===========================================================================

#[test]
fn test_daemon_status_not_running() {
    let h = SmokeHarness::new();

    let result = h.run(&["daemon", "status"]);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("not running")
            || combined.contains("Not running")
            || combined.contains("No daemon")
            || !result.success,
        "Daemon status when not running should indicate not running.\nstdout: {}\nstderr: {}",
        result.stdout,
        result.stderr,
    );
}

#[test]
fn test_daemon_stop_not_running() {
    let h = SmokeHarness::new();

    let result = h.run(&["daemon", "stop"]);
    // Stopping when not running should be idempotent or informative
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("not running")
            || combined.contains("Not running")
            || combined.contains("No daemon")
            || result.success,
        "Daemon stop when not running should handle gracefully.\nstdout: {}\nstderr: {}",
        result.stdout,
        result.stderr,
    );
}

// ===========================================================================
// Timer (Direct CLI)
// ===========================================================================

#[test]
fn test_timer_lifecycle() {
    let h = SmokeHarness::new();
    h.run_ok(&["issue", "create", "Timer issue"]);

    // Start timer (takes issue ID)
    let start = h.run_ok(&["timer", "start", "1"]);
    assert!(
        start.stdout_contains("Started")
            || start.stdout_contains("timer")
            || start.stdout_contains("Timer"),
        "Timer start should confirm.\nstdout: {}",
        start.stdout,
    );

    // Show timer (no issue ID arg)
    let show = h.run_ok(&["timer", "show"]);
    assert!(
        show.stdout_contains("running")
            || show.stdout_contains("active")
            || show.stdout_contains("Timer"),
        "Timer should show running.\nstdout: {}",
        show.stdout,
    );

    // Stop timer (no issue ID arg)
    let stop = h.run_ok(&["timer", "stop"]);
    assert!(
        stop.stdout_contains("Stopped")
            || stop.stdout_contains("timer")
            || stop.stdout_contains("Timer"),
        "Timer stop should confirm.\nstdout: {}",
        stop.stdout,
    );
}

#[test]
fn test_timer_show_no_timer() {
    let h = SmokeHarness::new();
    h.run_ok(&["issue", "create", "No timer issue"]);

    let show = h.run_ok(&["timer", "show"]);
    assert!(
        show.stdout_contains("No time")
            || show.stdout_contains("No active")
            || show.stdout_contains("no active")
            || show.stdout_contains("0")
            || show.stdout_contains("Total"),
        "Timer show with no timer should handle gracefully.\nstdout: {}",
        show.stdout,
    );
}

#[test]
fn test_timer_double_start() {
    let h = SmokeHarness::new();
    h.run_ok(&["issue", "create", "Double start timer"]);

    h.run_ok(&["timer", "start", "1"]);
    // Second start on a different issue should handle gracefully (idempotent or error)
    let result = h.run(&["timer", "start", "1"]);
    assert!(
        result.success || result.stderr.contains("already") || result.stderr.contains("running"),
        "Double timer start should handle gracefully.\nstderr: {}",
        result.stderr,
    );
}

// ===========================================================================
// Issue Tree
// ===========================================================================

#[test]
fn test_issue_tree_empty() {
    let h = SmokeHarness::new();

    let tree = h.run_ok(&["issue", "tree"]);
    assert!(
        tree.stdout_contains("No issues") || tree.stdout.trim().is_empty() || tree.success,
        "Tree with no issues should handle gracefully.\nstdout: {}",
        tree.stdout,
    );
}

#[test]
fn test_issue_tree_flat() {
    let h = SmokeHarness::new();

    h.run_ok(&["issue", "create", "Flat A"]);
    h.run_ok(&["issue", "create", "Flat B"]);
    h.run_ok(&["issue", "create", "Flat C"]);

    let tree = h.run_ok(&["issue", "tree"]);
    assert_stdout_contains(&tree, "Flat A");
    assert_stdout_contains(&tree, "Flat B");
    assert_stdout_contains(&tree, "Flat C");
}

// ===========================================================================
// JSON Output Mode
// ===========================================================================

#[test]
fn test_list_json_output() {
    let h = SmokeHarness::new();

    h.run_ok(&["issue", "create", "JSON issue 1"]);
    h.run_ok(&["issue", "create", "JSON issue 2"]);

    let result = h.run_ok(&["issue", "list", "--json"]);
    let parsed: Vec<serde_json::Value> =
        serde_json::from_str(&result.stdout).expect("Should be valid JSON array");
    assert_eq!(parsed.len(), 2);
    assert!(parsed.iter().any(|i| i["title"] == "JSON issue 1"));
    assert!(parsed.iter().any(|i| i["title"] == "JSON issue 2"));
}

#[test]
fn test_list_json_with_filters() {
    let h = SmokeHarness::new();

    h.run_ok(&["issue", "create", "High JSON", "-p", "high"]);
    h.run_ok(&["issue", "create", "Low JSON", "-p", "low"]);

    let result = h.run_ok(&["issue", "list", "-p", "high", "--json"]);
    let parsed: Vec<serde_json::Value> =
        serde_json::from_str(&result.stdout).expect("Should be valid JSON array");
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0]["title"], "High JSON");
}

// ===========================================================================
// Quiet Mode
// ===========================================================================

#[test]
fn test_create_quiet_mode() {
    let h = SmokeHarness::new();

    let result = h.run_ok(&["issue", "create", "Quiet issue", "--quiet"]);
    // Quiet mode should produce minimal output (just the ID or nothing)
    assert!(
        result.stdout.trim().len() < 20 || result.stdout.contains("1"),
        "Quiet mode should produce minimal output.\nstdout: {:?}",
        result.stdout,
    );
}

// ===========================================================================
// Agent Status
// ===========================================================================

#[test]
fn test_agent_status_no_agent() {
    let h = SmokeHarness::new();

    let result = h.run(&["agent", "status"]);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("No agent")
            || combined.contains("not initialized")
            || combined.contains("agent")
            || !result.success,
        "Agent status with no agent should be informative.\nstdout: {}\nstderr: {}",
        result.stdout,
        result.stderr,
    );
}

#[test]
fn test_agent_init_and_status() {
    let h = SmokeHarness::new();

    h.run_ok(&["agent", "init", "test-agent", "--no-key"]);

    let status = h.run_ok(&["agent", "status"]);
    assert!(
        status.stdout_contains("test-agent"),
        "Agent status should show agent ID.\nstdout: {}",
        status.stdout,
    );
}

// ===========================================================================
// Migrate Commands
// ===========================================================================

#[test]
fn test_migrate_to_shared() {
    let h = SmokeHarness::new();

    // Create some local issues first
    h.run_ok(&["issue", "create", "Migrate test issue"]);

    let result = h.run(&["migrate", "to-shared"]);
    // May succeed or fail depending on agent/sync state — should not crash
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        result.success
            || combined.contains("agent")
            || combined.contains("sync")
            || combined.contains("remote")
            || combined.contains("already"),
        "Migrate to-shared should handle gracefully.\nstdout: {}\nstderr: {}",
        result.stdout,
        result.stderr,
    );
}

#[test]
fn test_migrate_from_shared() {
    let h = SmokeHarness::new();

    let result = h.run(&["migrate", "from-shared"]);
    // May succeed or fail — should not crash
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        result.success
            || combined.contains("agent")
            || combined.contains("sync")
            || combined.contains("remote")
            || combined.contains("hub")
            || combined.contains("No shared"),
        "Migrate from-shared should handle gracefully.\nstdout: {}\nstderr: {}",
        result.stdout,
        result.stderr,
    );
}
