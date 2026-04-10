pub mod collect;
pub mod config;
pub mod dispatch;
pub mod engine;
pub mod history;
pub mod sources;

use anyhow::Result;
use std::path::Path;

use crate::db::Database;
use crate::shared_writer::SharedWriter;
use crate::SentinelCommands;

use config::SentinelConfig;

pub fn dispatch_cmd(
    command: SentinelCommands,
    crosslink_dir: &Path,
    db: &Database,
    writer: Option<&SharedWriter>,
    quiet: bool,
    json: bool,
) -> Result<()> {
    match command {
        SentinelCommands::Run { dry_run, label } => {
            let config = SentinelConfig::load(crosslink_dir)?;
            engine::run_oneshot(
                crosslink_dir,
                db,
                writer,
                &config,
                dry_run,
                label.as_deref(),
                quiet,
            )?;
            Ok(())
        }
        SentinelCommands::Watch { interval: _ } => {
            // Will be implemented in #659
            println!("sentinel watch not yet implemented");
            Ok(())
        }
        SentinelCommands::Status => {
            // Will be implemented in #659
            println!("sentinel not running");
            Ok(())
        }
        SentinelCommands::History {
            limit,
            json: json_flag,
        } => {
            let use_json = json || json_flag;
            history::show_history(db, limit, use_json)
        }
        SentinelCommands::Stop => {
            // Will be implemented in #659
            println!("sentinel not running");
            Ok(())
        }
    }
}
