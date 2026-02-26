use anyhow::{bail, Result};
use std::path::Path;

use crate::identity::AgentConfig;
use crate::utils::format_issue_id;

/// `crosslink agent init <agent-id> [-d "description"]`
pub fn init(crosslink_dir: &Path, agent_id: &str, description: Option<&str>) -> Result<()> {
    if AgentConfig::load(crosslink_dir)?.is_some() {
        bail!("Agent already configured. Delete .crosslink/agent.json to reconfigure.");
    }
    let config = AgentConfig::init(crosslink_dir, agent_id, description)?;
    println!("Agent initialized:");
    println!("  ID:      {}", config.agent_id);
    println!("  Machine: {}", config.machine_id);
    if let Some(desc) = &config.description {
        println!("  Description: {}", desc);
    }
    Ok(())
}

/// `crosslink agent status`
pub fn status(crosslink_dir: &Path) -> Result<()> {
    match AgentConfig::load(crosslink_dir)? {
        Some(config) => {
            println!("Agent: {}", config.agent_id);
            println!("Machine: {}", config.machine_id);
            if let Some(desc) = &config.description {
                println!("Description: {}", desc);
            }

            // Show locked issues (best-effort)
            if let Ok(sync) = crate::sync::SyncManager::new(crosslink_dir) {
                let _ = sync.init_cache();
                let _ = sync.fetch();
                if let Ok(locks) = sync.read_locks() {
                    let my_locks = locks.agent_locks(&config.agent_id);
                    if my_locks.is_empty() {
                        println!("Locks: none");
                    } else {
                        println!(
                            "Locks: {}",
                            my_locks
                                .iter()
                                .map(|id| format_issue_id(*id))
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                    }
                }
            }
        }
        None => {
            println!("No agent configured. Run 'crosslink agent init <id>' first.");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_init_creates_config() {
        let dir = tempdir().unwrap();
        let crosslink_dir = dir.path().join(".crosslink");
        std::fs::create_dir_all(&crosslink_dir).unwrap();

        init(&crosslink_dir, "worker-1", Some("Test agent")).unwrap();

        let config = AgentConfig::load(&crosslink_dir).unwrap().unwrap();
        assert_eq!(config.agent_id, "worker-1");
        assert_eq!(config.description, Some("Test agent".to_string()));
    }

    #[test]
    fn test_init_rejects_duplicate() {
        let dir = tempdir().unwrap();
        let crosslink_dir = dir.path().join(".crosslink");
        std::fs::create_dir_all(&crosslink_dir).unwrap();

        init(&crosslink_dir, "worker-1", None).unwrap();
        let result = init(&crosslink_dir, "worker-2", None);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("already configured"));
    }

    #[test]
    fn test_status_no_config() {
        let dir = tempdir().unwrap();
        let crosslink_dir = dir.path().join(".crosslink");
        std::fs::create_dir_all(&crosslink_dir).unwrap();

        // Should not error, just print message
        status(&crosslink_dir).unwrap();
    }

    #[test]
    fn test_status_with_config() {
        let dir = tempdir().unwrap();
        let crosslink_dir = dir.path().join(".crosslink");
        std::fs::create_dir_all(&crosslink_dir).unwrap();

        init(&crosslink_dir, "my-agent", Some("My agent")).unwrap();
        status(&crosslink_dir).unwrap();
    }
}
