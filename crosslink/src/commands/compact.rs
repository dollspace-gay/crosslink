use anyhow::{Context, Result};
use std::path::Path;

use crate::db::Database;
use crate::hydration::hydrate_to_sqlite;
use crate::identity::AgentConfig;
use crate::sync::SyncManager;

/// `crosslink compact` — run event compaction manually.
pub fn run(crosslink_dir: &Path, db: &Database, force: bool) -> Result<()> {
    let agent = AgentConfig::load(crosslink_dir)?.ok_or_else(|| {
        anyhow::anyhow!("No agent configured. Run 'crosslink agent init <id>' first.")
    })?;

    let sync = SyncManager::new(crosslink_dir)?;
    sync.init_cache()?;
    sync.fetch()?;

    let cache_dir = sync.cache_path();

    let result = crate::compaction::compact(cache_dir, &agent.agent_id, force)?;

    match result {
        Some(stats) => {
            println!("Compaction complete:");
            println!("  Events processed: {}", stats.events_processed);
            println!("  Issues materialized: {}", stats.issues_materialized);
            println!("  Locks materialized: {}", stats.locks_materialized);
            if stats.skew_warnings > 0 {
                println!("  Clock skew warnings: {}", stats.skew_warnings);
            }
            if stats.unsigned_warnings > 0 {
                println!("  Unsigned event warnings: {}", stats.unsigned_warnings);
            }

            // Prune own agent's compacted events
            let pruned = crate::compaction::prune_events(cache_dir, &agent.agent_id)?;
            if pruned > 0 {
                println!("  Pruned {} compacted event(s)", pruned);
            }

            // Commit and push if there were changes
            if stats.events_processed > 0 {
                let commit_result = sync.git_in_cache_pub(&["add", "-A"]);
                if commit_result.is_ok() {
                    let msg = format!(
                        "compact: {} events, {} issues",
                        stats.events_processed, stats.issues_materialized
                    );
                    let commit = sync.git_in_cache_pub(&["commit", "-m", &msg]);
                    if commit.is_ok() {
                        let push =
                            sync.git_in_cache_pub(&["push", "origin", crate::sync::HUB_BRANCH]);
                        if let Err(e) = push {
                            eprintln!("Warning: push failed (changes saved locally): {}", e);
                        }
                    }
                }
            }

            // Hydrate local SQLite
            hydrate_to_sqlite(cache_dir, db).context("Failed to hydrate after compaction")?;
        }
        None => {
            println!("Compaction skipped (lease held by another agent). Use --force to override.");
        }
    }

    Ok(())
}
