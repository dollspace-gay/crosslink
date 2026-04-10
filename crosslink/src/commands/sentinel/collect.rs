use anyhow::Result;
use std::path::Path;

use crate::db::Database;

/// Statistics from a result collection pass.
#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct CollectStats {
    pub collected: u32,
    pub orphaned: u32,
    pub still_running: u32,
}

/// Poll completed agents, read findings, post results to GitHub, update records.
///
/// Runs every sentinel cycle (after dispatch phase in oneshot, every cycle in watch).
#[allow(dead_code)]
pub fn collect_completed(_db: &Database, _crosslink_dir: &Path) -> Result<CollectStats> {
    // Will be implemented in #655
    Ok(CollectStats::default())
}
