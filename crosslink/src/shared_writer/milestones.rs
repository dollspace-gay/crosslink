//! Milestone operations: create, close, delete, assign, unassign.
//!
//! Migrated to the event-sourcing path (#604). Each mutation emits an
//! event via `emit_compact_push`; compaction's apply arms update
//! `CheckpointState.milestones` (for milestone lifecycle) and
//! `CheckpointState.issues[uuid].milestone_uuid` (for assignment).
//! Materialize writes the per-milestone files and per-issue JSON.

use anyhow::{Context, Result};

use crate::db::Database;

use super::core::SharedWriter;

impl SharedWriter {
    /// Create a milestone on the coordination branch.
    ///
    /// Returns the assigned milestone display ID.
    ///
    /// # Errors
    /// Returns an error if writing or pushing to the coordination branch fails.
    pub fn create_milestone(
        &self,
        db: &Database,
        name: &str,
        description: Option<&str>,
    ) -> Result<i64> {
        let uuid = uuid::Uuid::new_v4();
        let event = crate::events::Event::MilestoneCreated {
            uuid,
            name: name.to_string(),
            description: description.map(std::string::ToString::to_string),
        };
        self.emit_compact_push(event, &format!("create milestone: {name}"))?;
        self.hydrate_with_retry(db);

        // Read back the assigned display_id from the materialized
        // milestone file at meta/milestones/<uuid>.json.
        let path = self
            .cache_dir
            .join("meta")
            .join("milestones")
            .join(format!("{uuid}.json"));
        let entry = crate::issue_file::read_milestone_file(&path).with_context(|| {
            format!(
                "create_milestone: failed to read back materialized milestone {}",
                path.display()
            )
        })?;
        Ok(entry.display_id)
    }

    /// Close a milestone on the coordination branch.
    ///
    /// # Errors
    /// Returns an error if the milestone cannot be loaded or the write fails.
    pub fn close_milestone(&self, db: &Database, milestone_id: i64) -> Result<()> {
        let entry = self.load_milestone_by_id(milestone_id)?;
        let event = crate::events::Event::MilestoneClosed { uuid: entry.uuid };
        self.emit_compact_push(event, &format!("close milestone #{milestone_id}"))?;
        self.hydrate_with_retry(db);
        Ok(())
    }

    /// Delete a milestone file from the coordination branch.
    ///
    /// # Errors
    /// Returns an error if the milestone cannot be loaded or the write fails.
    pub fn delete_milestone(&self, db: &Database, milestone_id: i64) -> Result<()> {
        let entry = self.load_milestone_by_id(milestone_id)?;
        let event = crate::events::Event::MilestoneDeleted { uuid: entry.uuid };
        self.emit_compact_push(event, &format!("delete milestone #{milestone_id}"))?;
        self.hydrate_with_retry(db);
        Ok(())
    }

    /// Set `milestone_uuid` on issue JSON files for the given issue IDs.
    ///
    /// Emits one `MilestoneAssigned` event per issue (#604). The events
    /// land in a single compaction round, so the operation is atomic at
    /// the event-log level.
    ///
    /// # Errors
    /// Returns an error if the milestone or any issue cannot be loaded, or the write fails.
    pub fn set_milestone_on_issues(
        &self,
        db: &Database,
        milestone_id: i64,
        issue_ids: &[i64],
    ) -> Result<()> {
        let milestone = self.load_milestone_by_id(milestone_id)?;
        let ms_uuid = milestone.uuid;

        for &issue_id in issue_ids {
            let issue = self.load_issue_by_id(issue_id, db)?;
            let event = crate::events::Event::MilestoneAssigned {
                issue_uuid: issue.uuid,
                milestone_uuid: Some(ms_uuid),
            };
            self.emit_compact_push(
                event,
                &format!("assign issue #{issue_id} to milestone #{milestone_id}"),
            )?;
        }
        self.hydrate_with_retry(db);
        Ok(())
    }

    /// Clear `milestone_uuid` on an issue JSON file.
    ///
    /// # Errors
    /// Returns an error if the issue cannot be loaded or the write fails.
    pub fn clear_milestone_on_issue(&self, db: &Database, issue_id: i64) -> Result<()> {
        let issue = self.load_issue_by_id(issue_id, db)?;
        let event = crate::events::Event::MilestoneAssigned {
            issue_uuid: issue.uuid,
            milestone_uuid: None,
        };
        self.emit_compact_push(event, &format!("clear milestone on issue #{issue_id}"))?;
        self.hydrate_with_retry(db);
        Ok(())
    }
}
