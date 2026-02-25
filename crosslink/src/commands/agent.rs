use anyhow::{bail, Result};
use std::path::Path;

use crate::identity::AgentConfig;

/// `chainlink agent init <agent-id> [-d "description"]`
pub fn init(chainlink_dir: &Path, agent_id: &str, description: Option<&str>) -> Result<()> {
    if AgentConfig::load(chainlink_dir)?.is_some() {
        bail!("Agent already configured. Delete .chainlink/agent.json to reconfigure.");
    }
    let config = AgentConfig::init(chainlink_dir, agent_id, description)?;
    println!("Agent initialized:");
    println!("  ID:      {}", config.agent_id);
    println!("  Machine: {}", config.machine_id);
    if let Some(desc) = &config.description {
        println!("  Description: {}", desc);
    }
    Ok(())
}

/// `chainlink agent status`
pub fn status(chainlink_dir: &Path) -> Result<()> {
    match AgentConfig::load(chainlink_dir)? {
        Some(config) => {
            println!("Agent: {}", config.agent_id);
            println!("Machine: {}", config.machine_id);
            if let Some(desc) = &config.description {
                println!("Description: {}", desc);
            }

            // Show locked issues (best-effort)
            if let Ok(sync) = crate::sync::SyncManager::new(chainlink_dir) {
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
                                .map(|id| format!("#{}", id))
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                    }
                }
            }
        }
        None => {
            println!("No agent configured. Run 'chainlink agent init <id>' first.");
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
        let chainlink_dir = dir.path().join(".chainlink");
        std::fs::create_dir_all(&chainlink_dir).unwrap();

        init(&chainlink_dir, "worker-1", Some("Test agent")).unwrap();

        let config = AgentConfig::load(&chainlink_dir).unwrap().unwrap();
        assert_eq!(config.agent_id, "worker-1");
        assert_eq!(config.description, Some("Test agent".to_string()));
    }

    #[test]
    fn test_init_rejects_duplicate() {
        let dir = tempdir().unwrap();
        let chainlink_dir = dir.path().join(".chainlink");
        std::fs::create_dir_all(&chainlink_dir).unwrap();

        init(&chainlink_dir, "worker-1", None).unwrap();
        let result = init(&chainlink_dir, "worker-2", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already configured"));
    }

    #[test]
    fn test_status_no_config() {
        let dir = tempdir().unwrap();
        let chainlink_dir = dir.path().join(".chainlink");
        std::fs::create_dir_all(&chainlink_dir).unwrap();

        // Should not error, just print message
        status(&chainlink_dir).unwrap();
    }

    #[test]
    fn test_status_with_config() {
        let dir = tempdir().unwrap();
        let chainlink_dir = dir.path().join(".chainlink");
        std::fs::create_dir_all(&chainlink_dir).unwrap();

        init(&chainlink_dir, "my-agent", Some("My agent")).unwrap();
        status(&chainlink_dir).unwrap();
    }
}
