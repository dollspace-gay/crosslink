use anyhow::Result;

use crate::db::Database;

/// Display past sentinel runs and their dispatch outcomes.
pub fn show_history(db: &Database, limit: usize, json: bool) -> Result<()> {
    let runs = db.list_sentinel_runs(limit)?;

    if runs.is_empty() {
        if json {
            println!("[]");
        } else {
            println!("No sentinel runs recorded yet.");
        }
        return Ok(());
    }

    if json {
        let json_str = serde_json::to_string_pretty(&runs)?;
        println!("{json_str}");
        return Ok(());
    }

    // Table header
    println!(
        "{:<36}  {:<20}  {:>7}  {:>10}  {:>9}  {:>7}  {:>7}",
        "Run", "Started", "Signals", "Dispatched", "Collected", "Skipped", "Deferred"
    );
    println!("{}", "-".repeat(105));

    for run in &runs {
        let started = run
            .started_at
            .get(..19)
            .unwrap_or(&run.started_at)
            .replace('T', " ");
        let run_id_short = run.run_id.get(..12).unwrap_or(&run.run_id);
        println!(
            "{:<36}  {:<20}  {:>7}  {:>10}  {:>9}  {:>7}  {:>7}",
            run_id_short,
            started,
            run.signals_found,
            run.dispatched,
            run.collected,
            run.skipped,
            run.deferred,
        );
    }

    Ok(())
}
