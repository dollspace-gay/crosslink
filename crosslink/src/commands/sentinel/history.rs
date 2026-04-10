use anyhow::Result;

use crate::db::Database;

/// Display past sentinel runs and their dispatch outcomes.
pub fn show_history(_db: &Database, _limit: usize, _json: bool) -> Result<()> {
    // Will be implemented in #656 (depends on schema from #651)
    println!("No sentinel runs recorded yet.");
    Ok(())
}
