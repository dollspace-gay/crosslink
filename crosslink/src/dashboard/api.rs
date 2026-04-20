//! HTTP API for the dashboard aggregator (GH #429 §7).
//!
//! Routes live under `/api/v1/dashboard/` so they don't collide with the
//! existing single-project API (`/api/v1/issues`, `/api/v1/agents`, etc.)
//! that `crosslink serve` / the deprecation path continues to expose.
//!
//! Each handler opens a fresh [`DashboardDb`] connection from the path
//! recorded in [`crate::server::state::AppState::dashboard_db_path`].
//! `SQLite` opens are cheap (~microseconds) so per-request opens are
//! fine for the polling-panel use case. If that ever becomes a hot
//! path we can pool connections; not worth the complexity now.
//!
//! This module exposes a pure-axum `Router` factory — the server's
//! `build_router` nests it under `/api/v1/dashboard`.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use super::actions;
use super::db::DashboardDb;
use super::projects::Project;
use super::reader;
use crate::server::state::AppState;

/// Build the `/api/v1/dashboard` router.
///
/// Note: the project-detail route uses `{*slug}` (wildcard capture)
/// because crosslink slugs have the shape `owner/repo` and therefore
/// contain a slash. A single-segment capture (`{slug}`) would fail to
/// match. Clients can hit `/projects/forecast-bio/crosslink` directly;
/// no URL-encoding required.
pub fn build_router() -> Router<AppState> {
    // Write endpoints split by slug *and* issue id. Axum picks the
    // most-specific route that matches, so `/issues/{id}/close` wins
    // over `/projects/{*slug}` as long as we nest the writes beneath
    // a distinct prefix. Using `/w/` (for "write") keeps the path
    // pattern unambiguous without making URLs ugly.
    Router::new()
        .route("/projects", get(list_projects))
        .route("/projects/{*slug}", get(get_project_detail))
        .route("/alerts", get(list_alerts))
        .route("/w/{owner}/{repo}/issues/{id}/close", post(close_issue))
        .route("/w/{owner}/{repo}/issues/{id}/reopen", post(reopen_issue))
        .route("/w/{owner}/{repo}/issues/{id}/comment", post(comment_issue))
}

/// Wire-format representation of a tracked project on the list endpoint.
/// Extends [`Project`] with the current `project_state` counters so the
/// frontend can render a tile without a second round-trip.
#[derive(Debug, Serialize)]
struct ProjectListItem {
    slug: String,
    status: String,
    pinned: bool,
    hub_sha: Option<String>,
    hub_fetched_at: Option<String>,
    last_activity_at: Option<String>,
    added_at: String,
    counters: ProjectCountersView,
}

#[derive(Debug, Default, Serialize)]
struct ProjectCountersView {
    open_issues: i64,
    overdue_issues: i64,
    due_soon_issues: i64,
    blocked_issues: i64,
    active_agents: i64,
    stale_locks: i64,
    ci_status: Option<String>,
    updated_at: Option<String>,
}

/// Full detail payload for `/projects/{slug}`. Reads a live
/// [`reader::HubSnapshot`] off the cached clone so the frontend
/// gets the complete issue/agent/lock set — not just the aggregate
/// counters.
#[derive(Debug, Serialize)]
struct ProjectDetail {
    slug: String,
    status: String,
    pinned: bool,
    hub_sha: Option<String>,
    hub_fetched_at: Option<String>,
    last_activity_at: Option<String>,
    added_at: String,
    counters: ProjectCountersView,
    /// Full issue list from the hub branch.
    issues: Vec<crate::issue_file::IssueFile>,
    /// Per-agent heartbeats.
    agents: Vec<crate::locks::Heartbeat>,
    /// Lock entries (`issue_id` keyed).
    locks: Vec<SerializableLock>,
    /// Hub layout version (1 or 2).
    layout_version: u32,
}

/// Flat lock representation for JSON output. The reader's `LockRecord`
/// holds a [`crate::locks::Lock`] inline; mirror its fields here so the
/// wire shape is a single-level object.
#[derive(Debug, Serialize)]
struct SerializableLock {
    issue_id: i64,
    agent_id: String,
    branch: Option<String>,
    claimed_at: chrono::DateTime<chrono::Utc>,
    signed_by: String,
}

impl From<reader::LockRecord> for SerializableLock {
    fn from(record: reader::LockRecord) -> Self {
        Self {
            issue_id: record.issue_id,
            agent_id: record.lock.agent_id,
            branch: record.lock.branch,
            claimed_at: record.lock.claimed_at,
            signed_by: record.lock.signed_by,
        }
    }
}

/// `GET /api/v1/dashboard/projects` — list tracked projects with
/// materialised tile state.
async fn list_projects(
    State(state): State<AppState>,
) -> Result<Json<Vec<ProjectListItem>>, ApiError> {
    let db_path = state
        .dashboard_db_path
        .as_ref()
        .ok_or_else(|| ApiError::bad_request("dashboard DB not configured for this server"))?
        .clone();

    let items = tokio::task::spawn_blocking(move || load_project_list(&db_path))
        .await
        .map_err(|e| ApiError::internal(format!("list task panicked: {e}")))??;

    Ok(Json(items))
}

fn load_project_list(db_path: &std::path::Path) -> Result<Vec<ProjectListItem>, ApiError> {
    let db = DashboardDb::open(db_path).map_err(|e| ApiError::internal(format!("open db: {e}")))?;
    let mut stmt = db
        .conn
        .prepare(
            "SELECT p.slug, p.status, p.pinned, p.hub_sha, p.hub_fetched_at,
                    p.last_activity_at, p.added_at,
                    COALESCE(s.open_issues, 0),
                    COALESCE(s.overdue_issues, 0),
                    COALESCE(s.due_soon_issues, 0),
                    COALESCE(s.blocked_issues, 0),
                    COALESCE(s.active_agents, 0),
                    COALESCE(s.stale_locks, 0),
                    s.ci_status,
                    s.updated_at
             FROM projects p
             LEFT JOIN project_state s ON s.project_id = p.id
             ORDER BY p.pinned DESC, p.slug ASC",
        )
        .map_err(|e| ApiError::internal(format!("prepare: {e}")))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(ProjectListItem {
                slug: row.get(0)?,
                status: row.get(1)?,
                pinned: row.get::<_, i64>(2)? != 0,
                hub_sha: row.get(3)?,
                hub_fetched_at: row.get(4)?,
                last_activity_at: row.get(5)?,
                added_at: row.get(6)?,
                counters: ProjectCountersView {
                    open_issues: row.get(7)?,
                    overdue_issues: row.get(8)?,
                    due_soon_issues: row.get(9)?,
                    blocked_issues: row.get(10)?,
                    active_agents: row.get(11)?,
                    stale_locks: row.get(12)?,
                    ci_status: row.get(13)?,
                    updated_at: row.get(14)?,
                },
            })
        })
        .map_err(|e| ApiError::internal(format!("query: {e}")))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| ApiError::internal(format!("collect: {e}")))?;
    Ok(rows)
}

/// `GET /api/v1/dashboard/projects/{slug}` — full detail including a
/// freshly-read hub snapshot (issues, agents, locks).
async fn get_project_detail(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<ProjectDetail>, ApiError> {
    let db_path = state
        .dashboard_db_path
        .as_ref()
        .ok_or_else(|| ApiError::bad_request("dashboard DB not configured for this server"))?
        .clone();

    let detail = tokio::task::spawn_blocking(move || load_project_detail(&db_path, &slug))
        .await
        .map_err(|e| ApiError::internal(format!("detail task panicked: {e}")))??;
    Ok(Json(detail))
}

fn load_project_detail(db_path: &std::path::Path, slug: &str) -> Result<ProjectDetail, ApiError> {
    let db = DashboardDb::open(db_path).map_err(|e| ApiError::internal(format!("open db: {e}")))?;

    // Fetch the row + state in one query. Returns None if the slug
    // isn't tracked — which becomes a 404.
    let row: Option<(Project, ProjectCountersView)> = db
        .conn
        .query_row(
            "SELECT p.id, p.slug, p.clone_path, p.default_branch, p.hub_sha,
                    p.hub_fetched_at, p.status, p.added_at, p.last_activity_at,
                    p.pinned,
                    COALESCE(s.open_issues, 0), COALESCE(s.overdue_issues, 0),
                    COALESCE(s.due_soon_issues, 0), COALESCE(s.blocked_issues, 0),
                    COALESCE(s.active_agents, 0), COALESCE(s.stale_locks, 0),
                    s.ci_status, s.updated_at
             FROM projects p
             LEFT JOIN project_state s ON s.project_id = p.id
             WHERE p.slug = ?1",
            [slug],
            |r| {
                let project = Project {
                    id: r.get(0)?,
                    slug: r.get(1)?,
                    clone_path: std::path::PathBuf::from(r.get::<_, String>(2)?),
                    default_branch: r.get(3)?,
                    hub_sha: r.get(4)?,
                    hub_fetched_at: r.get(5)?,
                    status: r.get(6)?,
                    added_at: r.get(7)?,
                    last_activity_at: r.get(8)?,
                    pinned: r.get::<_, i64>(9)? != 0,
                };
                let counters = ProjectCountersView {
                    open_issues: r.get(10)?,
                    overdue_issues: r.get(11)?,
                    due_soon_issues: r.get(12)?,
                    blocked_issues: r.get(13)?,
                    active_agents: r.get(14)?,
                    stale_locks: r.get(15)?,
                    ci_status: r.get(16)?,
                    updated_at: r.get(17)?,
                };
                Ok((project, counters))
            },
        )
        .ok();

    let Some((project, counters)) = row else {
        return Err(ApiError::not_found(format!("project '{slug}' not tracked")));
    };

    // Read a fresh snapshot off the cache clone. If the clone is
    // missing (e.g. disk was wiped since `track`), return an empty
    // snapshot rather than erroring — the tile should still render.
    let snapshot = if project.clone_path.is_dir() {
        reader::read_snapshot(&project.clone_path).unwrap_or_else(|_| reader::HubSnapshot {
            hub_sha: None,
            layout_version: 1,
            issues: vec![],
            agents: vec![],
            locks: vec![],
            last_commit_at: None,
        })
    } else {
        reader::HubSnapshot {
            hub_sha: None,
            layout_version: 1,
            issues: vec![],
            agents: vec![],
            locks: vec![],
            last_commit_at: None,
        }
    };

    Ok(ProjectDetail {
        slug: project.slug,
        status: project.status,
        pinned: project.pinned,
        hub_sha: project.hub_sha,
        hub_fetched_at: project.hub_fetched_at,
        last_activity_at: project.last_activity_at,
        added_at: project.added_at,
        counters,
        layout_version: snapshot.layout_version,
        issues: snapshot.issues,
        agents: snapshot.agents,
        locks: snapshot.locks.into_iter().map(Into::into).collect(),
    })
}

/// Wire-format alert row returned by `GET /api/v1/dashboard/alerts`.
/// Mirrors the `alerts` table plus the `slug` of the parent project
/// so the frontend can link-off without a second fetch.
#[derive(Debug, Serialize)]
struct AlertItem {
    id: i64,
    project_slug: String,
    kind: String,
    severity: String,
    subject_ref: Option<String>,
    detail: Option<String>,
    opened_at: String,
    resolved_at: Option<String>,
    acknowledged_at: Option<String>,
}

/// `GET /api/v1/dashboard/alerts` — list currently-open alerts across
/// all tracked projects, most recent first. No filtering for MVP;
/// add `?severity=...` / `?project=...` query params in P3 polish.
async fn list_alerts(State(state): State<AppState>) -> Result<Json<Vec<AlertItem>>, ApiError> {
    let db_path = state
        .dashboard_db_path
        .as_ref()
        .ok_or_else(|| ApiError::bad_request("dashboard DB not configured for this server"))?
        .clone();

    let items = tokio::task::spawn_blocking(move || load_open_alerts(&db_path))
        .await
        .map_err(|e| ApiError::internal(format!("alerts task panicked: {e}")))??;
    Ok(Json(items))
}

fn load_open_alerts(db_path: &std::path::Path) -> Result<Vec<AlertItem>, ApiError> {
    let db = DashboardDb::open(db_path).map_err(|e| ApiError::internal(format!("open db: {e}")))?;
    let mut stmt = db
        .conn
        .prepare(
            "SELECT a.id, p.slug, a.kind, a.severity, a.subject_ref,
                    a.detail, a.opened_at, a.resolved_at, a.acknowledged_at
             FROM alerts a
             JOIN projects p ON p.id = a.project_id
             WHERE a.resolved_at IS NULL
             ORDER BY a.opened_at DESC",
        )
        .map_err(|e| ApiError::internal(format!("prepare: {e}")))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(AlertItem {
                id: row.get(0)?,
                project_slug: row.get(1)?,
                kind: row.get(2)?,
                severity: row.get(3)?,
                subject_ref: row.get(4)?,
                detail: row.get(5)?,
                opened_at: row.get(6)?,
                resolved_at: row.get(7)?,
                acknowledged_at: row.get(8)?,
            })
        })
        .map_err(|e| ApiError::internal(format!("query: {e}")))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| ApiError::internal(format!("collect: {e}")))?;
    Ok(rows)
}

/// Wire-format response for a completed write action.
#[derive(Debug, Serialize)]
struct ActionResponse {
    stdout: String,
    stderr: String,
}

/// Body for `POST /w/{owner}/{repo}/issues/{id}/comment`.
#[derive(Debug, Deserialize)]
struct CommentBody {
    content: String,
}

/// Shared setup: find the project by `{owner}/{repo}`, return a 404
/// if it isn't tracked.
async fn resolve_project(
    state: &AppState,
    owner: &str,
    repo: &str,
) -> Result<(std::path::PathBuf, Project), ApiError> {
    let slug = format!("{owner}/{repo}");
    let db_path = state
        .dashboard_db_path
        .as_ref()
        .ok_or_else(|| ApiError::bad_request("dashboard DB not configured for this server"))?
        .clone();

    let lookup_slug = slug.clone();
    let db_path_clone = db_path.clone();
    let project = tokio::task::spawn_blocking(move || -> Result<Option<Project>, ApiError> {
        let db = DashboardDb::open(&db_path_clone)
            .map_err(|e| ApiError::internal(format!("open db: {e}")))?;
        actions::find_project_by_slug(&db, &lookup_slug)
            .map_err(|e| ApiError::internal(format!("lookup: {e}")))
    })
    .await
    .map_err(|e| ApiError::internal(format!("lookup task panicked: {e}")))??;

    let project =
        project.ok_or_else(|| ApiError::not_found(format!("project '{slug}' not tracked")))?;
    Ok((db_path, project))
}

/// `POST /api/v1/dashboard/w/{owner}/{repo}/issues/{id}/close`
async fn close_issue(
    State(state): State<AppState>,
    Path((owner, repo, id)): Path<(String, String, i64)>,
) -> Result<Json<ActionResponse>, ApiError> {
    let (db_path, project) = resolve_project(&state, &owner, &repo).await?;
    let id_str = id.to_string();
    let result = actions::run_cli(
        &db_path,
        &project,
        "close_issue",
        Some(&format!("issue:{id}")),
        &["issue", "close", &id_str],
    )
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(ActionResponse {
        stdout: result.stdout,
        stderr: result.stderr,
    }))
}

/// `POST /api/v1/dashboard/w/{owner}/{repo}/issues/{id}/reopen`
async fn reopen_issue(
    State(state): State<AppState>,
    Path((owner, repo, id)): Path<(String, String, i64)>,
) -> Result<Json<ActionResponse>, ApiError> {
    let (db_path, project) = resolve_project(&state, &owner, &repo).await?;
    let id_str = id.to_string();
    let result = actions::run_cli(
        &db_path,
        &project,
        "reopen_issue",
        Some(&format!("issue:{id}")),
        &["issue", "reopen", &id_str],
    )
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(ActionResponse {
        stdout: result.stdout,
        stderr: result.stderr,
    }))
}

/// `POST /api/v1/dashboard/w/{owner}/{repo}/issues/{id}/comment`
///
/// Body: `{ "content": "..." }`.
async fn comment_issue(
    State(state): State<AppState>,
    Path((owner, repo, id)): Path<(String, String, i64)>,
    Json(body): Json<CommentBody>,
) -> Result<Json<ActionResponse>, ApiError> {
    if body.content.trim().is_empty() {
        return Err(ApiError::bad_request("comment content cannot be empty"));
    }
    let (db_path, project) = resolve_project(&state, &owner, &repo).await?;
    let id_str = id.to_string();
    let result = actions::run_cli(
        &db_path,
        &project,
        "comment_issue",
        Some(&format!("issue:{id}")),
        &["issue", "comment", &id_str, &body.content],
    )
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(ActionResponse {
        stdout: result.stdout,
        stderr: result.stderr,
    }))
}

/// Minimal typed error with status-code mapping. Patterned after axum
/// idioms — manual `IntoResponse` implementation maps to the right
/// status without pulling in a full error-handling crate.
#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn not_found(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: msg.into(),
        }
    }
    fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: msg.into(),
        }
    }
    fn internal(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: msg.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let body = Json(serde_json::json!({
            "error": self.message,
            "status": self.status.as_u16(),
        }));
        (self.status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt; // .oneshot(...)

    /// Build a minimal `AppState` wired to a temp DB.
    fn test_state(dashboard_db: Option<std::path::PathBuf>) -> (AppState, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let main_db_path = dir.path().join("crosslink.db");
        let main_db = crate::db::Database::open(&main_db_path).unwrap();
        let mut state = AppState::new(main_db, dir.path().join(".crosslink"));
        if let Some(p) = dashboard_db {
            state = state.with_dashboard_db(p);
        }
        (state, dir)
    }

    fn seed_project(db_path: &std::path::Path, slug: &str, clone_path: &std::path::Path) -> i64 {
        let db = DashboardDb::open(db_path).unwrap();
        db.conn
            .execute(
                "INSERT INTO projects (slug, clone_path, default_branch, status, added_at)
                 VALUES (?1, ?2, 'main', 'active', '2026-04-20T00:00:00Z')",
                rusqlite::params![slug, clone_path.to_string_lossy().as_ref()],
            )
            .unwrap();
        db.conn.last_insert_rowid()
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn test_list_projects_without_dashboard_db_returns_400() {
        let (state, _tmp) = test_state(None);
        let app = Router::new()
            .nest("/api/v1/dashboard", build_router())
            .with_state(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/dashboard/projects")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_list_projects_empty_returns_empty_array() {
        let tmp = tempfile::tempdir().unwrap();
        let dashboard_db_path = tmp.path().join("dashboard.db");
        DashboardDb::open(&dashboard_db_path).unwrap();

        let (state, _tmp2) = test_state(Some(dashboard_db_path));
        let app = Router::new()
            .nest("/api/v1/dashboard", build_router())
            .with_state(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/dashboard/projects")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json, serde_json::json!([]));
    }

    #[tokio::test]
    async fn test_list_projects_returns_tracked() {
        let tmp = tempfile::tempdir().unwrap();
        let dashboard_db_path = tmp.path().join("dashboard.db");
        DashboardDb::open(&dashboard_db_path).unwrap();
        let clone_path = tmp.path().join("owner").join("repo");
        seed_project(&dashboard_db_path, "owner/repo", &clone_path);

        let (state, _tmp2) = test_state(Some(dashboard_db_path));
        let app = Router::new()
            .nest("/api/v1/dashboard", build_router())
            .with_state(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/dashboard/projects")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let arr = json.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["slug"], "owner/repo");
        assert_eq!(arr[0]["status"], "active");
        // counters default to zeros when project_state hasn't been populated.
        assert_eq!(arr[0]["counters"]["open_issues"], 0);
    }

    #[tokio::test]
    async fn test_project_detail_unknown_slug_returns_404() {
        let tmp = tempfile::tempdir().unwrap();
        let dashboard_db_path = tmp.path().join("dashboard.db");
        DashboardDb::open(&dashboard_db_path).unwrap();

        let (state, _tmp2) = test_state(Some(dashboard_db_path));
        let app = Router::new()
            .nest("/api/v1/dashboard", build_router())
            .with_state(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/dashboard/projects/does-not/exist")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_list_alerts_without_dashboard_db_returns_400() {
        let (state, _tmp) = test_state(None);
        let app = Router::new()
            .nest("/api/v1/dashboard", build_router())
            .with_state(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/dashboard/alerts")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_list_alerts_empty_returns_empty_array() {
        let tmp = tempfile::tempdir().unwrap();
        let dashboard_db_path = tmp.path().join("dashboard.db");
        DashboardDb::open(&dashboard_db_path).unwrap();

        let (state, _tmp2) = test_state(Some(dashboard_db_path));
        let app = Router::new()
            .nest("/api/v1/dashboard", build_router())
            .with_state(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/dashboard/alerts")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json, serde_json::json!([]));
    }

    #[tokio::test]
    async fn test_list_alerts_returns_open_rows_with_project_slug() {
        let tmp = tempfile::tempdir().unwrap();
        let dashboard_db_path = tmp.path().join("dashboard.db");
        let db = DashboardDb::open(&dashboard_db_path).unwrap();
        db.conn
            .execute(
                "INSERT INTO projects (slug, clone_path, default_branch, status, added_at)
                 VALUES ('owner/repo', '/tmp/x', 'main', 'active', '2026-04-20T00:00:00Z')",
                [],
            )
            .unwrap();
        let project_id = db.conn.last_insert_rowid();
        db.conn
            .execute(
                "INSERT INTO alerts
                   (project_id, kind, severity, subject_ref, detail, opened_at)
                 VALUES (?1, 'stale_lock', 'warning', 'lock:42', 'held too long', '2026-04-20T12:00:00Z')",
                rusqlite::params![project_id],
            )
            .unwrap();
        // A resolved alert should NOT show up.
        db.conn
            .execute(
                "INSERT INTO alerts
                   (project_id, kind, severity, subject_ref, detail, opened_at, resolved_at)
                 VALUES (?1, 'overdue_issue', 'warning', 'issue:1', 'was overdue', '2026-04-20T10:00:00Z', '2026-04-20T11:00:00Z')",
                rusqlite::params![project_id],
            )
            .unwrap();

        let (state, _tmp2) = test_state(Some(dashboard_db_path));
        let app = Router::new()
            .nest("/api/v1/dashboard", build_router())
            .with_state(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/dashboard/alerts")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let arr = json.as_array().unwrap();
        assert_eq!(arr.len(), 1, "only one open alert should be returned");
        assert_eq!(arr[0]["project_slug"], "owner/repo");
        assert_eq!(arr[0]["kind"], "stale_lock");
        assert_eq!(arr[0]["subject_ref"], "lock:42");
    }

    #[tokio::test]
    async fn test_close_issue_returns_404_for_untracked_slug() {
        let tmp = tempfile::tempdir().unwrap();
        let dashboard_db_path = tmp.path().join("dashboard.db");
        DashboardDb::open(&dashboard_db_path).unwrap();

        let (state, _tmp2) = test_state(Some(dashboard_db_path));
        let app = Router::new()
            .nest("/api/v1/dashboard", build_router())
            .with_state(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/dashboard/w/owner/repo/issues/42/close")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_comment_issue_rejects_empty_content() {
        let tmp = tempfile::tempdir().unwrap();
        let dashboard_db_path = tmp.path().join("dashboard.db");
        let db = DashboardDb::open(&dashboard_db_path).unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["init", "-q"])
            .status()
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO projects (slug, clone_path, default_branch, status, added_at)
                 VALUES ('owner/repo', ?1, 'main', 'active', '2026-04-20T00:00:00Z')",
                [repo.to_string_lossy().as_ref()],
            )
            .unwrap();

        let (state, _tmp2) = test_state(Some(dashboard_db_path));
        let app = Router::new()
            .nest("/api/v1/dashboard", build_router())
            .with_state(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/dashboard/w/owner/repo/issues/1/comment")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"content":"   "}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_project_detail_returns_empty_snapshot_when_clone_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let dashboard_db_path = tmp.path().join("dashboard.db");
        DashboardDb::open(&dashboard_db_path).unwrap();
        let clone_path = tmp.path().join("does-not-exist");
        seed_project(&dashboard_db_path, "owner/repo", &clone_path);

        let (state, _tmp2) = test_state(Some(dashboard_db_path));
        let app = Router::new()
            .nest("/api/v1/dashboard", build_router())
            .with_state(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/dashboard/projects/owner/repo")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["slug"], "owner/repo");
        assert_eq!(json["issues"], serde_json::json!([]));
        assert_eq!(json["agents"], serde_json::json!([]));
        assert_eq!(json["locks"], serde_json::json!([]));
    }
}
