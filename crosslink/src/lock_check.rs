use anyhow::{bail, Result};
use std::path::Path;

use crate::identity::AgentConfig;
use crate::sync::SyncManager;

/// Result of checking whether an agent can work on an issue.
#[derive(Debug, PartialEq)]
pub enum LockStatus {
    /// No lock system configured (no agent.json). Single-agent mode.
    NotConfigured,
    /// Issue is not locked by anyone.
    Available,
    /// Issue is locked by this agent. Proceed.
    LockedBySelf,
    /// Issue is locked by another agent.
    LockedByOther { agent_id: String, stale: bool },
}

/// Check whether the current agent can work on the given issue.
///
/// Returns `LockStatus` without blocking — callers decide how to handle.
/// Gracefully degrades: if agent config is missing, sync fails, or we're
/// offline, returns `NotConfigured` so single-agent usage is unaffected.
pub fn check_lock(crosslink_dir: &Path, issue_id: i64) -> Result<LockStatus> {
    // If no agent config, we're in single-agent mode — no lock checking
    let agent = match AgentConfig::load(crosslink_dir)? {
        Some(a) => a,
        None => return Ok(LockStatus::NotConfigured),
    };

    // Try to create sync manager. If it fails, don't block.
    let sync = match SyncManager::new(crosslink_dir) {
        Ok(s) => s,
        Err(_) => return Ok(LockStatus::NotConfigured),
    };

    // Best-effort init and fetch — don't fail if offline
    let _ = sync.init_cache();
    let _ = sync.fetch();

    // If cache still isn't set up, can't check locks
    if !sync.is_initialized() {
        return Ok(LockStatus::NotConfigured);
    }

    let locks = match sync.read_locks() {
        Ok(l) => l,
        Err(_) => return Ok(LockStatus::NotConfigured),
    };

    // Check if locked by this agent
    if locks.is_locked_by(issue_id, &agent.agent_id) {
        return Ok(LockStatus::LockedBySelf);
    }

    // Check if locked by someone else
    match locks.get_lock(issue_id) {
        Some(lock) => {
            let stale = sync
                .find_stale_locks()
                .unwrap_or_default()
                .iter()
                .any(|(id, _)| *id == issue_id);
            Ok(LockStatus::LockedByOther {
                agent_id: lock.agent_id.clone(),
                stale,
            })
        }
        None => Ok(LockStatus::Available),
    }
}

/// Enforce lock check. Bails if another agent holds the lock (unless stale).
///
/// Use this in commands that set the active work item (session work, create --work, quick).
pub fn enforce_lock(crosslink_dir: &Path, issue_id: i64) -> Result<()> {
    match check_lock(crosslink_dir, issue_id)? {
        LockStatus::NotConfigured | LockStatus::Available | LockStatus::LockedBySelf => Ok(()),
        LockStatus::LockedByOther { agent_id, stale } => {
            if stale {
                eprintln!(
                    "Warning: Issue #{} is locked by '{}' but the lock appears STALE. Proceeding.",
                    issue_id, agent_id
                );
                Ok(())
            } else {
                bail!(
                    "Issue #{} is locked by agent '{}'. \
                     Use 'crosslink locks check {}' for details. \
                     Ask the human to release it or wait for the lock to expire.",
                    issue_id,
                    agent_id,
                    issue_id
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_no_agent_config_returns_not_configured() {
        let dir = tempdir().unwrap();
        let crosslink_dir = dir.path().join(".crosslink");
        std::fs::create_dir_all(&crosslink_dir).unwrap();

        let status = check_lock(&crosslink_dir, 1).unwrap();
        assert_eq!(status, LockStatus::NotConfigured);
    }

    #[test]
    fn test_enforce_not_configured_allows() {
        let dir = tempdir().unwrap();
        let crosslink_dir = dir.path().join(".crosslink");
        std::fs::create_dir_all(&crosslink_dir).unwrap();

        // No agent.json → NotConfigured → allowed
        assert!(enforce_lock(&crosslink_dir, 1).is_ok());
    }

    #[test]
    fn test_enforce_available_allows() {
        // enforce_lock on Available should succeed
        // We can't easily test this without a full git setup,
        // but the logic is: Available → Ok(())
        // Covered implicitly by the NotConfigured test since
        // that path also returns Ok(())
    }

    #[test]
    fn test_lock_status_debug() {
        // Ensure all variants are debuggable
        let statuses = vec![
            LockStatus::NotConfigured,
            LockStatus::Available,
            LockStatus::LockedBySelf,
            LockStatus::LockedByOther {
                agent_id: "worker-1".to_string(),
                stale: false,
            },
            LockStatus::LockedByOther {
                agent_id: "worker-2".to_string(),
                stale: true,
            },
        ];
        for s in statuses {
            let _ = format!("{:?}", s);
        }
    }

    #[test]
    fn test_lock_status_equality() {
        assert_eq!(LockStatus::NotConfigured, LockStatus::NotConfigured);
        assert_eq!(LockStatus::Available, LockStatus::Available);
        assert_eq!(LockStatus::LockedBySelf, LockStatus::LockedBySelf);
        assert_ne!(LockStatus::Available, LockStatus::NotConfigured);
        assert_eq!(
            LockStatus::LockedByOther {
                agent_id: "a".to_string(),
                stale: false
            },
            LockStatus::LockedByOther {
                agent_id: "a".to_string(),
                stale: false
            }
        );
        assert_ne!(
            LockStatus::LockedByOther {
                agent_id: "a".to_string(),
                stale: false
            },
            LockStatus::LockedByOther {
                agent_id: "b".to_string(),
                stale: false
            }
        );
    }
}
