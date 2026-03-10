//! Handlers for the design document orchestration endpoints.
//!
//! Implements:
//! - `POST /api/v1/orchestrator/decompose` — LLM-assisted doc → plan breakdown

use axum::{extract::State, http::StatusCode, response::Json};

use crate::orchestrator::decompose;
use crate::server::{
    state::AppState,
    types::{ApiError, DecomposeRequest, OrchestratorPlan},
};

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

fn internal_error(context: &str, e: impl std::fmt::Display) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: context.to_string(),
            detail: Some(e.to_string()),
        }),
    )
}

fn bad_request(msg: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            error: "bad request".to_string(),
            detail: Some(msg.into()),
        }),
    )
}

// ---------------------------------------------------------------------------
// POST /api/v1/orchestrator/decompose
// ---------------------------------------------------------------------------

/// `POST /api/v1/orchestrator/decompose` — decompose a design document.
///
/// Accepts a JSON body with `document` (markdown string) and optional `slug`.
/// Calls the Claude CLI to produce a structured phase/stage/task breakdown,
/// stores the resulting plan on disk, and returns it.
///
/// # Errors
///
/// - `400 Bad Request` if the document is empty
/// - `500 Internal Server Error` if the Claude CLI fails or returns invalid JSON
pub async fn decompose(
    State(state): State<AppState>,
    Json(body): Json<DecomposeRequest>,
) -> Result<Json<OrchestratorPlan>, (StatusCode, Json<ApiError>)> {
    if body.document.trim().is_empty() {
        return Err(bad_request(
            "document field is required and must not be empty",
        ));
    }

    let slug = body.slug.as_deref();

    let plan = decompose::decompose_document(&state.crosslink_dir, &body.document, slug)
        .await
        .map_err(|e| internal_error("decomposition failed", e))?;

    Ok(Json(plan))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::server::state::AppState;

    fn test_state() -> (AppState, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");
        let db = Database::open(&db_path).expect("test db");
        let state = AppState::new(db, dir.path().join(".crosslink"));
        (state, dir)
    }

    #[tokio::test]
    async fn test_decompose_empty_document() {
        let (state, _dir) = test_state();
        let body = DecomposeRequest {
            document: "".to_string(),
            slug: None,
        };
        let result = decompose(State(state), Json(body)).await;
        assert!(result.is_err());
        let (status, json) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(json.0.detail.unwrap().contains("empty"));
    }

    #[tokio::test]
    async fn test_decompose_whitespace_only() {
        let (state, _dir) = test_state();
        let body = DecomposeRequest {
            document: "   \n\t  ".to_string(),
            slug: Some("test".to_string()),
        };
        let result = decompose(State(state), Json(body)).await;
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
