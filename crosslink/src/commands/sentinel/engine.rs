use anyhow::Result;
use std::path::Path;

use crate::db::Database;
use crate::shared_writer::SharedWriter;

use super::config::SentinelConfig;

/// Statistics from a single sentinel cycle.
#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct CycleStats {
    pub signals_found: u32,
    pub dispatched: u32,
    pub collected: u32,
    pub skipped: u32,
    pub deferred: u32,
}

/// Run a single sentinel cycle: poll sources, triage, dispatch, collect.
pub fn run_oneshot(
    _crosslink_dir: &Path,
    _db: &Database,
    _writer: Option<&SharedWriter>,
    config: &SentinelConfig,
    dry_run: bool,
    _label_filter: Option<&str>,
    quiet: bool,
) -> Result<CycleStats> {
    if !config.enabled {
        if !quiet {
            println!("sentinel is disabled");
        }
        return Ok(CycleStats::default());
    }

    if dry_run {
        if !quiet {
            println!("sentinel dry-run: would poll sources and dispatch agents");
            println!(
                "  sources: github-labels (labels: {:?})",
                config.sources.github_labels.labels
            );
            println!("  max concurrent agents: {}", config.max_concurrent_agents);
            println!("  default model: {}", config.default_agent.model);
            if config.escalation.enabled {
                println!(
                    "  escalation: {} after {}m cooldown",
                    config.escalation.model, config.escalation.cooldown_minutes
                );
            }
        }
        return Ok(CycleStats::default());
    }

    // Full implementation in #654
    if !quiet {
        println!("0 signals found");
    }
    Ok(CycleStats::default())
}
