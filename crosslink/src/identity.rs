use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Machine-local agent identity. Lives at `.chainlink/agent.json`.
/// This file is gitignored — each machine has its own.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentConfig {
    pub agent_id: String,
    pub machine_id: String,
    #[serde(default)]
    pub description: Option<String>,
}

impl AgentConfig {
    /// Load from the .chainlink directory. Returns None if agent.json doesn't exist.
    pub fn load(chainlink_dir: &Path) -> Result<Option<Self>> {
        let path = chainlink_dir.join("agent.json");
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let config: AgentConfig = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse {}", path.display()))?;
        config.validate()?;
        Ok(Some(config))
    }

    /// Create and write a new agent config.
    pub fn init(chainlink_dir: &Path, agent_id: &str, description: Option<&str>) -> Result<Self> {
        let machine_id = detect_hostname();
        let config = AgentConfig {
            agent_id: agent_id.to_string(),
            machine_id,
            description: description.map(|s| s.to_string()),
        };
        config.validate()?;
        let path = chainlink_dir.join("agent.json");
        let json = serde_json::to_string_pretty(&config)?;
        std::fs::write(&path, json)
            .with_context(|| format!("Failed to write {}", path.display()))?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        anyhow::ensure!(!self.agent_id.is_empty(), "agent_id cannot be empty");
        anyhow::ensure!(
            self.agent_id
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_'),
            "agent_id must be alphanumeric with hyphens/underscores only"
        );
        anyhow::ensure!(
            self.agent_id.len() <= 64,
            "agent_id must be <= 64 characters"
        );
        Ok(())
    }
}

/// Detect the hostname of the current machine.
fn detect_hostname() -> String {
    // Windows: COMPUTERNAME env var
    if let Ok(name) = std::env::var("COMPUTERNAME") {
        return name;
    }
    // Unix: HOSTNAME env var
    if let Ok(name) = std::env::var("HOSTNAME") {
        return name;
    }
    // Fallback: run hostname command
    if let Ok(output) = std::process::Command::new("hostname").output() {
        if output.status.success() {
            let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !name.is_empty() {
                return name;
            }
        }
    }
    "unknown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use tempfile::tempdir;

    #[test]
    fn test_load_missing_file() {
        let dir = tempdir().unwrap();
        let result = AgentConfig::load(dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_init_and_load_roundtrip() {
        let dir = tempdir().unwrap();
        let config = AgentConfig::init(dir.path(), "worker-1", Some("Test agent")).unwrap();
        assert_eq!(config.agent_id, "worker-1");
        assert_eq!(config.description, Some("Test agent".to_string()));
        assert!(!config.machine_id.is_empty());

        let loaded = AgentConfig::load(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.agent_id, config.agent_id);
        assert_eq!(loaded.machine_id, config.machine_id);
        assert_eq!(loaded.description, config.description);
    }

    #[test]
    fn test_init_no_description() {
        let dir = tempdir().unwrap();
        let config = AgentConfig::init(dir.path(), "worker-2", None).unwrap();
        assert_eq!(config.agent_id, "worker-2");
        assert!(config.description.is_none());
    }

    #[test]
    fn test_validate_empty_id() {
        let config = AgentConfig {
            agent_id: "".to_string(),
            machine_id: "test".to_string(),
            description: None,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_chars() {
        let config = AgentConfig {
            agent_id: "worker 1".to_string(),
            machine_id: "test".to_string(),
            description: None,
        };
        assert!(config.validate().is_err());

        let config2 = AgentConfig {
            agent_id: "worker@1".to_string(),
            machine_id: "test".to_string(),
            description: None,
        };
        assert!(config2.validate().is_err());
    }

    #[test]
    fn test_validate_too_long() {
        let config = AgentConfig {
            agent_id: "a".repeat(65),
            machine_id: "test".to_string(),
            description: None,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_valid_ids() {
        for id in &["worker-1", "agent_2", "MyAgent", "a", "test-agent-42"] {
            let config = AgentConfig {
                agent_id: id.to_string(),
                machine_id: "test".to_string(),
                description: None,
            };
            assert!(config.validate().is_ok(), "Failed for id: {}", id);
        }
    }

    #[test]
    fn test_json_roundtrip() {
        let config = AgentConfig {
            agent_id: "worker-1".to_string(),
            machine_id: "my-host".to_string(),
            description: Some("Test agent".to_string()),
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: AgentConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, parsed);
    }

    #[test]
    fn test_json_missing_description_defaults_none() {
        let json = r#"{"agent_id": "worker-1", "machine_id": "host"}"#;
        let config: AgentConfig = serde_json::from_str(json).unwrap();
        assert!(config.description.is_none());
    }

    #[test]
    fn test_detect_hostname_returns_something() {
        let hostname = detect_hostname();
        assert!(!hostname.is_empty());
    }

    proptest! {
        #[test]
        fn prop_valid_ids_roundtrip(id in "[a-zA-Z0-9_-]{1,64}") {
            let config = AgentConfig {
                agent_id: id.clone(),
                machine_id: "test".to_string(),
                description: None,
            };
            prop_assert!(config.validate().is_ok());
            let json = serde_json::to_string(&config).unwrap();
            let parsed: AgentConfig = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(parsed.agent_id, id);
        }
    }
}
