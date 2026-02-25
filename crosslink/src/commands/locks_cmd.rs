use anyhow::Result;
use std::path::Path;

use crate::db::Database;
use crate::sync::{GpgVerification, SyncManager};
use crate::utils::truncate;

/// `chainlink locks list` — show current lock state
pub fn list(chainlink_dir: &Path, db: &Database, json_output: bool) -> Result<()> {
    let sync = SyncManager::new(chainlink_dir)?;
    sync.init_cache()?;
    sync.fetch()?;

    let locks_file = sync.read_locks()?;

    if json_output {
        let json = serde_json::to_string_pretty(&locks_file)?;
        println!("{}", json);
        return Ok(());
    }

    if locks_file.locks.is_empty() {
        println!("No active locks.");
        return Ok(());
    }

    let stale = sync.find_stale_locks()?;
    let stale_ids: Vec<i64> = stale.iter().map(|(id, _)| *id).collect();

    println!("Active locks:");
    for (issue_id_str, lock) in &locks_file.locks {
        let issue_id: i64 = issue_id_str.parse().unwrap_or(0);
        let title = db
            .get_issue(issue_id)?
            .map(|i| truncate(&i.title, 40))
            .unwrap_or_else(|| "(unknown issue)".to_string());

        let stale_marker = if stale_ids.contains(&issue_id) {
            " [STALE]"
        } else {
            ""
        };

        println!(
            "  #{:<4} {} -- claimed by {} on {}{}",
            issue_id,
            title,
            lock.agent_id,
            lock.claimed_at.format("%Y-%m-%d %H:%M"),
            stale_marker
        );
        if let Some(branch) = &lock.branch {
            println!("         branch: {}", branch);
        }
    }
    Ok(())
}

/// `chainlink locks check <id>` — check if an issue is available
pub fn check(chainlink_dir: &Path, issue_id: i64) -> Result<()> {
    let sync = SyncManager::new(chainlink_dir)?;
    sync.init_cache()?;
    sync.fetch()?;

    let locks_file = sync.read_locks()?;

    match locks_file.get_lock(issue_id) {
        Some(lock) => {
            println!(
                "Issue #{} is locked by '{}' (claimed {})",
                issue_id,
                lock.agent_id,
                lock.claimed_at.format("%Y-%m-%d %H:%M")
            );
            if let Some(branch) = &lock.branch {
                println!("  Branch: {}", branch);
            }
            // Check if stale
            let stale = sync.find_stale_locks()?;
            if stale.iter().any(|(id, _)| *id == issue_id) {
                println!("  Warning: this lock appears STALE (no recent heartbeat)");
            }
        }
        None => {
            println!(
                "Issue #{} is not locked. Available for claiming.",
                issue_id
            );
        }
    }
    Ok(())
}

/// `chainlink sync` — fetch latest locks and verify signatures
pub fn sync_cmd(chainlink_dir: &Path) -> Result<()> {
    let sync = SyncManager::new(chainlink_dir)?;
    sync.init_cache()?;
    sync.fetch()?;

    println!("Cache: {}", sync.cache_path().display());

    // Verify GPG signature
    let verification = sync.verify_locks_signature()?;
    match &verification {
        GpgVerification::Valid {
            commit,
            fingerprint,
        } => {
            println!(
                "Locks synced. Signature valid (commit {}).",
                &commit[..7.min(commit.len())]
            );
            if let Some(fp) = fingerprint {
                println!("  Signed by: {}", fp);
                // Check against keyring if available
                if let Ok(Some(keyring)) = sync.read_keyring() {
                    if keyring.is_trusted(fp) {
                        println!("  Key is in trusted keyring.");
                    } else {
                        println!("  WARNING: Signer not in trusted keyring!");
                    }
                }
            }
        }
        GpgVerification::Unsigned { commit } => {
            println!(
                "Locks synced. WARNING: Latest commit ({}) is NOT signed.",
                &commit[..7.min(commit.len())]
            );
        }
        GpgVerification::Invalid { commit, reason } => {
            println!(
                "Locks synced. WARNING: Signature verification failed on {}: {}",
                &commit[..7.min(commit.len())],
                reason
            );
        }
        GpgVerification::NoCommits => {
            println!("Locks branch has no commits yet.");
        }
    }

    let locks_file = sync.read_locks()?;
    println!("{} active lock(s).", locks_file.locks.len());

    let stale = sync.find_stale_locks()?;
    if !stale.is_empty() {
        println!("{} stale lock(s) detected:", stale.len());
        for (id, agent) in &stale {
            println!("  #{} (held by {})", id, agent);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    // Integration tests for locks_cmd require a git repo with a remote,
    // so they are covered in the CLI integration test suite rather than
    // unit tests here. The underlying sync.rs and locks.rs have their
    // own comprehensive unit tests.
}
