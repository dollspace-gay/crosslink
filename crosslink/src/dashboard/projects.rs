//! Tracked-project CRUD for `crosslink dashboard track / untrack / list`.
//!
//! Each tracked project has a row in the `projects` table and a local
//! clone under `~/.crosslink/dashboard-cache/<owner>/<repo>/`. This
//! module provides the CLI-side handlers; the poll loop (P1.2.C) reads
//! the rows this module writes and fetches + diffs the clones on a
//! 5-second interval.

use anyhow::{bail, Context, Result};
use rusqlite::params;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::db::DashboardDb;

/// A tracked repository, hydrated from the `projects` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub id: i64,
    pub slug: String,
    pub clone_path: PathBuf,
    pub default_branch: String,
    pub hub_sha: Option<String>,
    pub hub_fetched_at: Option<String>,
    pub status: String,
    pub added_at: String,
    pub last_activity_at: Option<String>,
    pub pinned: bool,
}

/// Default cache-root for local clones: `~/.crosslink/dashboard-cache/`.
///
/// # Errors
/// Returns an error if the user's home directory can't be resolved.
pub fn default_cache_root() -> Result<PathBuf> {
    let db_path = DashboardDb::default_path()?;
    // dashboard.db lives at `~/.crosslink/dashboard.db` — strip the
    // filename and sibling-join the cache dir so both live under the
    // same parent.
    let parent = db_path
        .parent()
        .context("dashboard DB path has no parent")?;
    Ok(parent.join("dashboard-cache"))
}

/// Validate an `owner/repo` slug. Returns `(owner, repo)` on success.
fn parse_slug(slug: &str) -> Result<(&str, &str)> {
    let mut parts = slug.splitn(2, '/');
    let owner = parts.next().unwrap_or_default();
    let repo = parts.next().unwrap_or_default();
    if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
        bail!("slug must be in the form `owner/repo`, got: {slug}");
    }
    // Reject any component that contains a path separator to prevent
    // tracking a repo named e.g. `foo/../../etc`. `parse_slug` is the
    // only entry point for slugs before they hit the filesystem.
    if owner.contains(std::path::is_separator)
        || repo.contains(std::path::is_separator)
        || owner.contains('\\')
        || repo.contains('\\')
    {
        bail!("slug must not contain path separators: {slug}");
    }
    Ok((owner, repo))
}

/// Resolve the on-disk clone path for a slug.
fn clone_path_for(cache_root: &Path, slug: &str) -> Result<PathBuf> {
    let (owner, repo) = parse_slug(slug)?;
    Ok(cache_root.join(owner).join(repo))
}

/// CLI-level wrapper: resolves default paths from the user's home dir,
/// then delegates to [`track_with_paths`].
///
/// # Errors
/// As [`track_with_paths`], plus home-dir resolution.
pub fn track(slug: &str, clone_url: Option<&str>) -> Result<()> {
    let db_path = DashboardDb::default_path()?;
    let cache_root = default_cache_root()?;
    track_with_paths(slug, clone_url, &db_path, &cache_root)
}

/// Core logic for tracking a repository, parameterised on paths so tests
/// can point at a tempdir without mutating `$HOME`.
///
/// - Validates the slug format.
/// - Resolves the clone URL (defaults to `https://github.com/<slug>.git`).
/// - Clones the repo under `cache_root/<owner>/<repo>/`, restricted to
///   the `crosslink/hub` branch so the initial fetch stays small.
/// - Inserts a row in the `projects` table in the DB at `db_path`.
/// - Errors loudly if the slug is already tracked.
///
/// # Errors
/// Returns an error if the slug is invalid, the clone directory already
/// exists, the git clone fails, or the DB insert fails.
pub fn track_with_paths(
    slug: &str,
    clone_url: Option<&str>,
    db_path: &Path,
    cache_root: &Path,
) -> Result<()> {
    let (owner, repo) = parse_slug(slug)?;
    let url = clone_url.map_or_else(
        || format!("https://github.com/{owner}/{repo}.git"),
        ToString::to_string,
    );

    let clone_path = clone_path_for(cache_root, slug)?;

    if clone_path.exists() {
        bail!(
            "clone directory {} already exists — untrack first if you want to re-add",
            clone_path.display()
        );
    }

    // Open the dashboard DB first so we catch "already tracked" errors
    // before making any filesystem changes we'd need to roll back.
    let db = DashboardDb::open(db_path)?;
    let existing: Option<i64> = db
        .conn
        .query_row("SELECT id FROM projects WHERE slug = ?1", [slug], |row| {
            row.get(0)
        })
        .ok();
    if existing.is_some() {
        bail!("{slug} is already tracked");
    }

    if let Some(parent) = clone_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create cache parent {}", parent.display()))?;
    }

    // Shallow clone of just the hub branch. Cheaper initial pull; the
    // poll loop fetches deltas from there.
    let out = Command::new("git")
        .args([
            "clone",
            "--single-branch",
            "--branch",
            "crosslink/hub",
            "--depth",
            "50",
            &url,
            clone_path.to_string_lossy().as_ref(),
        ])
        .output()
        .context("Failed to invoke git clone")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!("git clone of {url} failed: {}", stderr.trim());
    }

    let now = chrono::Utc::now().to_rfc3339();
    db.conn.execute(
        "INSERT INTO projects (slug, clone_path, default_branch, status, added_at)
         VALUES (?1, ?2, ?3, 'active', ?4)",
        params![
            slug,
            clone_path.to_string_lossy().as_ref(),
            "main", // TODO (P1.2.B+): detect actual default branch
            now
        ],
    )?;

    println!("Tracking {slug} (cloned to {})", clone_path.display());
    Ok(())
}

/// CLI-level wrapper: resolves the default DB path and delegates.
///
/// # Errors
/// As [`untrack_with_path`], plus home-dir resolution.
pub fn untrack(slug: &str, keep_clone: bool) -> Result<()> {
    let db_path = DashboardDb::default_path()?;
    untrack_with_path(slug, keep_clone, &db_path)
}

/// Core logic for untracking, parameterised on the DB path so tests can
/// use a tempdir without mutating `$HOME`.
///
/// Deletes the `projects` row (CASCADE handles `project_state`,
/// `alerts`, `activity`). Unless `keep_clone`, also `rm -rf` the
/// clone directory that was recorded on the row.
///
/// # Errors
/// Returns an error if the slug isn't tracked, the DB delete fails, or
/// (when not `keep_clone`) the clone directory can't be removed.
pub fn untrack_with_path(slug: &str, keep_clone: bool, db_path: &Path) -> Result<()> {
    parse_slug(slug)?;

    let db = DashboardDb::open(db_path)?;

    let clone_path_str: Option<String> = db
        .conn
        .query_row(
            "SELECT clone_path FROM projects WHERE slug = ?1",
            [slug],
            |row| row.get(0),
        )
        .ok();
    let Some(clone_path_str) = clone_path_str else {
        bail!("{slug} is not currently tracked");
    };

    let rows = db
        .conn
        .execute("DELETE FROM projects WHERE slug = ?1", [slug])?;
    if rows == 0 {
        bail!("{slug} is not currently tracked");
    }

    let clone_path = PathBuf::from(clone_path_str);
    if !keep_clone && clone_path.exists() {
        std::fs::remove_dir_all(&clone_path).with_context(|| {
            format!("Failed to remove clone directory {}", clone_path.display())
        })?;
        println!("Untracked {slug}; removed {}", clone_path.display());
    } else if keep_clone {
        println!(
            "Untracked {slug}; kept local clone at {}",
            clone_path.display()
        );
    } else {
        println!("Untracked {slug} (clone directory was already gone)");
    }
    Ok(())
}

/// CLI-level wrapper: resolves the default DB path and delegates.
///
/// # Errors
/// As [`list_with_path`], plus home-dir resolution.
pub fn list() -> Result<()> {
    let db_path = DashboardDb::default_path()?;
    list_with_path(&db_path)
}

/// Core list implementation, parameterised on DB path.
///
/// # Errors
/// Returns an error if the DB can't be opened or the query fails.
pub fn list_with_path(db_path: &Path) -> Result<()> {
    let db = DashboardDb::open(db_path)?;

    let mut stmt = db.conn.prepare(
        "SELECT id, slug, clone_path, default_branch, hub_sha, hub_fetched_at,
                status, added_at, last_activity_at, pinned
         FROM projects
         ORDER BY pinned DESC, slug ASC",
    )?;
    let projects: Vec<Project> = stmt
        .query_map([], |row| {
            Ok(Project {
                id: row.get(0)?,
                slug: row.get(1)?,
                clone_path: PathBuf::from(row.get::<_, String>(2)?),
                default_branch: row.get(3)?,
                hub_sha: row.get(4)?,
                hub_fetched_at: row.get(5)?,
                status: row.get(6)?,
                added_at: row.get(7)?,
                last_activity_at: row.get(8)?,
                pinned: row.get::<_, i64>(9)? != 0,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    if projects.is_empty() {
        println!("No tracked projects. Add one with `crosslink dashboard track <owner/repo>`.");
        return Ok(());
    }

    println!(
        "{:<5} {:<40} {:<10} {:<25} Clone",
        "PIN", "SLUG", "STATUS", "LAST FETCH"
    );
    for p in &projects {
        let pin = if p.pinned { "●" } else { " " };
        let last_fetch = p.hub_fetched_at.as_deref().unwrap_or("—");
        println!(
            "{pin:<5} {:<40} {:<10} {:<25} {}",
            p.slug,
            p.status,
            last_fetch,
            p.clone_path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_parse_slug_valid() {
        assert_eq!(
            parse_slug("forecast-bio/crosslink").unwrap(),
            ("forecast-bio", "crosslink")
        );
    }

    #[test]
    fn test_parse_slug_rejects_single_segment() {
        assert!(parse_slug("crosslink").is_err());
    }

    #[test]
    fn test_parse_slug_rejects_three_segments() {
        assert!(parse_slug("forecast/bio/crosslink").is_err());
    }

    #[test]
    fn test_parse_slug_rejects_empty_owner() {
        assert!(parse_slug("/crosslink").is_err());
    }

    #[test]
    fn test_parse_slug_rejects_empty_repo() {
        assert!(parse_slug("forecast-bio/").is_err());
    }

    #[test]
    fn test_parse_slug_rejects_path_traversal() {
        assert!(parse_slug("../etc/passwd").is_err());
        assert!(parse_slug("foo\\bar").is_err());
    }

    #[test]
    fn test_clone_path_for_composes_under_cache_root() {
        let root = PathBuf::from("/tmp/cache");
        let path = clone_path_for(&root, "forecast-bio/crosslink").unwrap();
        assert_eq!(path, PathBuf::from("/tmp/cache/forecast-bio/crosslink"));
    }

    /// Helper: open a fresh DB in a tempdir and return (tempdir, db_path).
    /// Keeping the tempdir alive is the caller's responsibility.
    fn temp_db() -> (tempfile::TempDir, PathBuf) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("dashboard.db");
        DashboardDb::open(&db_path).unwrap();
        (dir, db_path)
    }

    #[test]
    fn test_untrack_rejects_unknown_slug() {
        let (_home, db_path) = temp_db();
        let err = untrack_with_path("forecast-bio/crosslink", false, &db_path).unwrap_err();
        assert!(err.to_string().contains("not currently tracked"));
    }

    #[test]
    fn test_list_on_empty_db_prints_help() {
        let (_home, db_path) = temp_db();
        // Doesn't capture stdout; just verifies Ok on an empty DB.
        list_with_path(&db_path).unwrap();
    }

    #[test]
    fn test_track_rejects_invalid_slug() {
        let (_home, db_path) = temp_db();
        let cache = tempdir().unwrap();
        let err = track_with_paths("not-a-slug", None, &db_path, cache.path()).unwrap_err();
        assert!(err.to_string().contains("owner/repo"));
    }

    #[test]
    fn test_track_rejects_duplicate_slug() {
        let (_home, db_path) = temp_db();
        let cache = tempdir().unwrap();
        // Seed a row directly so we exercise the "already tracked"
        // branch without needing a network-reachable git clone.
        let db = DashboardDb::open(&db_path).unwrap();
        db.conn
            .execute(
                "INSERT INTO projects (slug, clone_path, default_branch, status, added_at)
                 VALUES ('owner/repo', '/nonexistent', 'main', 'active', '2026-04-20T00:00:00Z')",
                [],
            )
            .unwrap();
        let err = track_with_paths("owner/repo", None, &db_path, cache.path()).unwrap_err();
        assert!(err.to_string().contains("already tracked"));
    }

    #[test]
    fn test_track_rejects_when_clone_dir_exists() {
        let (_home, db_path) = temp_db();
        let cache = tempdir().unwrap();
        // Pre-create the target clone path.
        let pre = cache.path().join("owner").join("repo");
        std::fs::create_dir_all(&pre).unwrap();
        let err = track_with_paths("owner/repo", None, &db_path, cache.path()).unwrap_err();
        assert!(
            err.to_string().contains("already exists"),
            "error should mention the existing clone dir: {err}"
        );
    }

    #[test]
    fn test_untrack_removes_row_and_directory() {
        let (_home, db_path) = temp_db();
        let cache = tempdir().unwrap();
        let clone = cache.path().join("owner").join("repo");
        std::fs::create_dir_all(&clone).unwrap();

        let db = DashboardDb::open(&db_path).unwrap();
        db.conn
            .execute(
                "INSERT INTO projects (slug, clone_path, default_branch, status, added_at)
                 VALUES ('owner/repo', ?1, 'main', 'active', '2026-04-20T00:00:00Z')",
                [clone.to_string_lossy().as_ref()],
            )
            .unwrap();
        drop(db);

        untrack_with_path("owner/repo", false, &db_path).unwrap();

        assert!(!clone.exists(), "clone dir should be removed by untrack");
        let db = DashboardDb::open(&db_path).unwrap();
        let count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_untrack_keep_clone_preserves_directory() {
        let (_home, db_path) = temp_db();
        let cache = tempdir().unwrap();
        let clone = cache.path().join("owner").join("repo");
        std::fs::create_dir_all(&clone).unwrap();

        let db = DashboardDb::open(&db_path).unwrap();
        db.conn
            .execute(
                "INSERT INTO projects (slug, clone_path, default_branch, status, added_at)
                 VALUES ('owner/repo', ?1, 'main', 'active', '2026-04-20T00:00:00Z')",
                [clone.to_string_lossy().as_ref()],
            )
            .unwrap();
        drop(db);

        untrack_with_path("owner/repo", true, &db_path).unwrap();

        assert!(
            clone.exists(),
            "--keep-clone should leave the clone dir in place"
        );
    }
}
