//! Issue mutation operations: create, update, close, reopen, delete,
//! comments, labels, blockers, and relations.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::cell::Cell;
use uuid::Uuid;

use crate::db::Database;
use crate::issue_file::{read_issue_file, IssueFile};

use super::core::{PushOutcome, SharedWriter, WriteSet};

/// Represents an update to a description field with three possible states:
/// unchanged, cleared, or set to a new value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DescriptionUpdate<'a> {
    /// Do not modify the description.
    #[default]
    Unchanged,
    /// Clear the description (set to `None`).
    Clear,
    /// Set the description to the given value.
    Set(&'a str),
}

impl<'a> From<Option<Option<&'a str>>> for DescriptionUpdate<'a> {
    fn from(opt: Option<Option<&'a str>>) -> Self {
        match opt {
            None => Self::Unchanged,
            Some(None) => Self::Clear,
            Some(Some(s)) => Self::Set(s),
        }
    }
}

/// Generic three-valued update for optional fields (GH #361). Use for any
/// setter that needs to distinguish "leave alone" from "set to `None`" from
/// "set to `Some(value)`".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FieldUpdate<T> {
    /// Do not modify the field.
    #[default]
    Unchanged,
    /// Clear the field (set to `None`).
    Clear,
    /// Set the field to the given value.
    Set(T),
}

impl<T> From<Option<Option<T>>> for FieldUpdate<T> {
    fn from(opt: Option<Option<T>>) -> Self {
        match opt {
            None => Self::Unchanged,
            Some(None) => Self::Clear,
            Some(Some(v)) => Self::Set(v),
        }
    }
}

/// Field-level update for an existing issue. Every field defaults to
/// "leave unchanged," so callers touch only what they want to change:
///
/// ```ignore
/// writer.update_issue(&db, id, IssueUpdate {
///     title: Some("renamed"),
///     scheduled_at: FieldUpdate::Clear,
///     ..Default::default()
/// })?;
/// ```
///
/// Replaces the previous 8-argument positional signature that was
/// trivial to misuse at the call site (two adjacent `Option<&str>`
/// parameters for status and priority were indistinguishable).
#[derive(Debug, Clone, Copy, Default)]
pub struct IssueUpdate<'a> {
    pub title: Option<&'a str>,
    pub description: DescriptionUpdate<'a>,
    pub status: Option<&'a str>,
    pub priority: Option<&'a str>,
    pub scheduled_at: FieldUpdate<chrono::DateTime<chrono::Utc>>,
    pub due_at: FieldUpdate<chrono::DateTime<chrono::Utc>>,
}

/// Internal shape of a new-issue creation request, used to keep
/// `create_issue_inner`'s signature narrow. The public `create_issue` /
/// `create_subissue` entry points keep their positional-argument shape
/// for backward compatibility with callers throughout the crate; this
/// struct exists purely so the shared inner helper doesn't have to
/// carry 8 positional parameters.
#[derive(Debug, Clone, Copy)]
struct IssueCreate<'a> {
    title: &'a str,
    description: Option<&'a str>,
    priority: &'a str,
    parent_uuid: Option<Uuid>,
    scheduled_at: Option<chrono::DateTime<chrono::Utc>>,
    due_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Internal parameters for creating a comment (shared by `add_comment`
/// and `add_intervention_comment` to avoid duplicating V1/V2 dispatch).
#[derive(Clone)]
struct CommentParams {
    content: String,
    kind: String,
    trigger_type: Option<String>,
    intervention_context: Option<String>,
    driver_key_fingerprint: Option<String>,
}

impl SharedWriter {
    /// Internal helper: create an issue (optionally as a subissue).
    ///
    /// Shared by `create_issue` and `create_subissue` to avoid duplicating
    /// the UUID generation, ID claiming, V2 directory setup, offline
    /// rewrite, and hydration logic.
    fn create_issue_inner(
        &self,
        db: &Database,
        create: IssueCreate<'_>,
        commit_msg: &str,
    ) -> Result<i64> {
        let uuid = Uuid::new_v4();
        let now = Utc::now();
        let title_owned = create.title.to_string();
        let desc_owned = create.description.map(std::string::ToString::to_string);
        let priority_parsed: crate::models::Priority = create.priority.parse()?;
        let agent_id = self.agent.agent_id.clone();
        let display_id = Cell::new(0i64);
        let parent_uuid = create.parent_uuid;
        let scheduled_at = create.scheduled_at;
        let due_at = create.due_at;

        let outcome = self.write_commit_push(
            |writer| {
                let (id, counters) = writer.claim_display_id(1)?;
                display_id.set(id);
                let is_v2 = writer.layout_version() >= 2;
                let issue = IssueFile {
                    uuid,
                    display_id: Some(id),
                    title: title_owned.clone(),
                    description: desc_owned.clone(),
                    status: crate::models::IssueStatus::Open,
                    priority: priority_parsed,
                    parent_uuid,
                    created_by: agent_id.clone(),
                    created_at: now,
                    updated_at: now,
                    closed_at: None,
                    scheduled_at,
                    due_at,
                    labels: vec![],
                    comments: vec![],
                    blockers: vec![],
                    related: vec![],
                    milestone_uuid: None,
                    time_entries: vec![],
                };
                let json = serde_json::to_vec_pretty(&issue)?;
                let rel_path = writer.issue_rel_path(&uuid);
                if is_v2 {
                    let comments_dir = writer
                        .cache_dir
                        .join("issues")
                        .join(uuid.to_string())
                        .join("comments");
                    std::fs::create_dir_all(&comments_dir)
                        .context("Failed to create v2 comments directory")?;
                }
                Ok(WriteSet {
                    files: vec![(rel_path, json)],
                    counters: Some(counters),
                    use_git_rm: false,
                })
            },
            commit_msg,
        )?;

        if outcome == PushOutcome::LocalOnly {
            self.rewrite_as_offline(uuid)?;
            self.hydrate_with_retry(db);
            return db.get_issue_id_by_uuid(&uuid.to_string());
        }

        self.hydrate_with_retry(db);
        Ok(display_id.get())
    }

    /// Create a new issue: generate UUID, claim display ID, write JSON, push, hydrate.
    ///
    /// Returns the assigned display ID. `scheduled_at` / `due_at` are
    /// optional scheduling dates (GH #361); pass `None` for neither to
    /// create a dateless issue.
    ///
    /// # Errors
    ///
    /// Returns an error if UUID generation, counter claiming, JSON serialization,
    /// git operations, or hydration fail.
    pub fn create_issue(
        &self,
        db: &Database,
        title: &str,
        description: Option<&str>,
        priority: &str,
        scheduled_at: Option<DateTime<Utc>>,
        due_at: Option<DateTime<Utc>>,
    ) -> Result<i64> {
        self.create_issue_inner(
            db,
            IssueCreate {
                title,
                description,
                priority,
                parent_uuid: None,
                scheduled_at,
                due_at,
            },
            &format!("create issue: {title}"),
        )
    }

    /// Create a subissue under a parent.
    ///
    /// Returns the assigned display ID for the child. Subissues never carry
    /// scheduling dates — those are a property of the parent deliverable
    /// (GH #361, REQ-12). The CLI layer rejects `--scheduled`/`--due`
    /// when `--parent` is present; this function does not accept them.
    ///
    /// # Errors
    ///
    /// Returns an error if the parent issue cannot be resolved, or if creation fails.
    pub fn create_subissue(
        &self,
        db: &Database,
        parent_id: i64,
        title: &str,
        description: Option<&str>,
        priority: &str,
    ) -> Result<i64> {
        let parent_uuid = self.resolve_uuid(parent_id, db)?;
        self.create_issue_inner(
            db,
            IssueCreate {
                title,
                description,
                priority,
                parent_uuid: Some(parent_uuid),
                scheduled_at: None,
                due_at: None,
            },
            &format!("create subissue under #{parent_id}: {title}"),
        )
    }

    /// Update an issue's title, description, status, priority, or scheduling.
    ///
    /// Unspecified fields of `update` are left unchanged. See [`IssueUpdate`]
    /// for the field-level semantics (Unchanged / Clear / Set).
    ///
    /// # Errors
    ///
    /// Returns an error if the issue cannot be loaded, status/priority parsing
    /// fails, or git operations fail.
    pub fn update_issue(
        &self,
        db: &Database,
        display_id: i64,
        update: IssueUpdate<'_>,
    ) -> Result<()> {
        // Event-sourced (#604). Status changes get a separate
        // StatusChanged event for audit clarity; other field changes go
        // through IssueUpdated. Both events are appended in one
        // emit_compact_push round if both are present.
        let issue = self.load_issue_by_id(display_id, db)?;
        let uuid = issue.uuid;

        // StatusChanged first (audit ordering matches user intent).
        if let Some(s) = update.status {
            let parsed: crate::models::IssueStatus = s.parse()?;
            let event = crate::events::Event::StatusChanged {
                uuid,
                new_status: s.to_string(),
                closed_at: if matches!(parsed, crate::models::IssueStatus::Closed) {
                    Some(Utc::now())
                } else {
                    None
                },
            };
            self.emit_compact_push(event, &format!("update status of #{display_id}"))?;
        }

        // IssueUpdated for any of {title, description, priority,
        // scheduled_at, due_at}.
        let has_field_update = update.title.is_some()
            || !matches!(update.description, DescriptionUpdate::Unchanged)
            || update.priority.is_some()
            || !matches!(update.scheduled_at, FieldUpdate::Unchanged)
            || !matches!(update.due_at, FieldUpdate::Unchanged);

        if has_field_update {
            let (description, clear_description) = match &update.description {
                DescriptionUpdate::Unchanged => (None, None),
                DescriptionUpdate::Clear => (None, Some(true)),
                DescriptionUpdate::Set(s) => (Some((*s).to_string()), None),
            };
            let scheduled_at = match update.scheduled_at {
                FieldUpdate::Unchanged => None,
                FieldUpdate::Clear => Some(None),
                FieldUpdate::Set(dt) => Some(Some(dt)),
            };
            let due_at = match update.due_at {
                FieldUpdate::Unchanged => None,
                FieldUpdate::Clear => Some(None),
                FieldUpdate::Set(dt) => Some(Some(dt)),
            };
            let event = crate::events::Event::IssueUpdated {
                uuid,
                title: update.title.map(std::string::ToString::to_string),
                description,
                clear_description,
                priority: update.priority.map(std::string::ToString::to_string),
                scheduled_at,
                due_at,
            };
            self.emit_compact_push(event, &format!("update issue #{display_id}"))?;
        }

        self.hydrate_with_retry(db);
        Ok(())
    }

    /// Close an issue (set status to "closed" and record `closed_at`).
    ///
    /// # Errors
    ///
    /// Returns an error if the issue cannot be loaded or git operations fail.
    pub fn close_issue(&self, db: &Database, display_id: i64) -> Result<()> {
        // Event-sourced (#604): emit StatusChanged with new_status="closed"
        // and a fresh closed_at timestamp. The envelope timestamp drives
        // the issue's `updated_at` during apply.
        let issue = self.load_issue_by_id(display_id, db)?;
        let event = crate::events::Event::StatusChanged {
            uuid: issue.uuid,
            new_status: "closed".to_string(),
            closed_at: Some(Utc::now()),
        };
        self.emit_compact_push(event, &format!("close issue #{display_id}"))?;
        self.hydrate_with_retry(db);
        Ok(())
    }

    /// Reopen an issue (set status to "open", clear `closed_at`).
    ///
    /// # Errors
    ///
    /// Returns an error if the issue cannot be loaded or git operations fail.
    pub fn reopen_issue(&self, db: &Database, display_id: i64) -> Result<()> {
        // Event-sourced (#604): StatusChanged with new_status="open" and
        // closed_at=None to clear the prior closure timestamp.
        let issue = self.load_issue_by_id(display_id, db)?;
        let event = crate::events::Event::StatusChanged {
            uuid: issue.uuid,
            new_status: "open".to_string(),
            closed_at: None,
        };
        self.emit_compact_push(event, &format!("reopen issue #{display_id}"))?;
        self.hydrate_with_retry(db);
        Ok(())
    }

    /// Delete an issue JSON file from the coordination branch.
    ///
    /// # Errors
    ///
    /// Returns an error if the issue cannot be found or git operations fail.
    pub fn delete_issue(&self, db: &Database, display_id: i64) -> Result<()> {
        // Event-sourced (#604): emit IssueDeleted; compaction removes
        // the issue from state and materialize deletes the JSON file
        // (whole V2 directory or flat V1 file).
        let issue = self.load_issue_by_id(display_id, db)?;
        let event = crate::events::Event::IssueDeleted { uuid: issue.uuid };
        self.emit_compact_push(event, &format!("delete issue #{display_id}"))?;
        self.hydrate_with_retry(db);
        Ok(())
    }

    /// Internal helper: add a comment to an issue with the given parameters.
    ///
    /// Handles counter claiming, signing, and V1/V2 layout dispatch.
    fn add_comment_inner(
        &self,
        db: &Database,
        display_id: i64,
        params: &CommentParams,
        commit_msg: &str,
    ) -> Result<i64> {
        // Event-sourced (#604): emit CommentAdded. The comment id is
        // assigned during apply from CheckpointState.next_comment_id, so
        // the caller reads it back from the materialized JSON after
        // emit_compact_push returns.
        let issue = self.load_issue_by_id(display_id, db)?;
        let issue_uuid = issue.uuid;
        let comment_uuid = Uuid::new_v4();
        let agent_id = self.agent.agent_id.clone();

        let event = crate::events::Event::CommentAdded {
            comment_uuid,
            issue_uuid,
            author: agent_id,
            content: params.content.clone(),
            kind: params.kind.clone(),
            trigger_type: params.trigger_type.clone(),
            intervention_context: params.intervention_context.clone(),
            driver_key_fingerprint: params.driver_key_fingerprint.clone(),
        };
        self.emit_compact_push(event, commit_msg)?;

        self.hydrate_with_retry(db);

        // Read back the assigned comment id. The CommentAdded apply
        // pushed the comment onto state.issues[uuid].comments with an
        // id claimed from next_comment_id; materialize wrote it as a
        // per-comment file (V2) or inlined into the issue JSON (V1).
        // We look up by `comment_uuid` (stable across layouts) rather
        // than by content (which can collide between comments).
        let id = self.lookup_comment_id_by_uuid(issue_uuid, comment_uuid)?;
        Ok(id)
    }

    /// Look up the materialized comment id for a comment with a known
    /// UUID. Handles both V1 (inline) and V2 (per-comment file) layouts.
    fn lookup_comment_id_by_uuid(&self, issue_uuid: Uuid, comment_uuid: Uuid) -> Result<i64> {
        if self.layout_version() >= 2 {
            let path = self
                .cache_dir
                .join(Self::comment_rel_path(&issue_uuid, &comment_uuid));
            // V2 comment files don't carry the SQLite id directly — the
            // id lives in CheckpointState.issues[issue_uuid].comments.
            // Read the checkpoint to find the assigned id.
            let _ = path; // path exists as evidence the file was written
            let checkpoint = crate::checkpoint::read_checkpoint(&self.cache_dir)?;
            let id = checkpoint
                .issues
                .get(&issue_uuid)
                .and_then(|i| i.comments.iter().find(|c| c.uuid == comment_uuid))
                .map(|c| c.id)
                .context("comment not found in checkpoint state after emit")?;
            Ok(id)
        } else {
            let issue = read_issue_file(&self.issue_path(&issue_uuid))?;
            // V1 inline comments don't carry uuid in the file, fall back
            // to the checkpoint state lookup.
            let _ = issue;
            let checkpoint = crate::checkpoint::read_checkpoint(&self.cache_dir)?;
            let id = checkpoint
                .issues
                .get(&issue_uuid)
                .and_then(|i| i.comments.iter().find(|c| c.uuid == comment_uuid))
                .map(|c| c.id)
                .context("comment not found in checkpoint state after emit")?;
            Ok(id)
        }
    }

    /// Add a comment to an issue.
    ///
    /// Returns the comment ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the issue cannot be loaded or git operations fail.
    pub fn add_comment(
        &self,
        db: &Database,
        display_id: i64,
        content: &str,
        kind: &str,
    ) -> Result<i64> {
        self.add_comment_inner(
            db,
            display_id,
            &CommentParams {
                content: content.to_string(),
                kind: kind.to_string(),
                trigger_type: None,
                intervention_context: None,
                driver_key_fingerprint: None,
            },
            &format!("comment on issue #{display_id}"),
        )
    }

    /// Add a driver intervention comment to an issue (kind = "intervention").
    ///
    /// # Errors
    ///
    /// Returns an error if the issue cannot be loaded or git operations fail.
    pub fn add_intervention_comment(
        &self,
        db: &Database,
        display_id: i64,
        content: &str,
        trigger_type: &str,
        intervention_context: Option<&str>,
        driver_key_fingerprint: Option<&str>,
    ) -> Result<i64> {
        self.add_comment_inner(
            db,
            display_id,
            &CommentParams {
                content: content.to_string(),
                kind: super::core::KIND_INTERVENTION.to_string(),
                trigger_type: Some(trigger_type.to_string()),
                intervention_context: intervention_context.map(std::string::ToString::to_string),
                driver_key_fingerprint: driver_key_fingerprint
                    .map(std::string::ToString::to_string),
            },
            &format!("intervention on issue #{display_id}"),
        )
    }

    /// Add a label to an issue.
    ///
    /// Returns `Ok(true)` if the label was newly added, `Ok(false)` if the
    /// issue already carried the label (no-op short-circuit).
    ///
    /// # Errors
    ///
    /// Returns an error if the issue cannot be loaded or git operations fail.
    pub fn add_label(&self, db: &Database, display_id: i64, label: &str) -> Result<bool> {
        let label_owned = label.to_string();

        let current = self.load_issue_by_id(display_id, db)?;
        if current.labels.contains(&label_owned) {
            return Ok(false);
        }
        let issue_uuid = current.uuid;

        // Event-sourced (#604): emit `LabelAdded`; compaction's
        // apply_graph_event inserts into the BTreeSet, materialize
        // writes the updated issue JSON.
        let event = crate::events::Event::LabelAdded {
            issue_uuid,
            label: label_owned,
        };
        self.emit_compact_push(event, &format!("label issue #{display_id} with {label}"))?;

        self.hydrate_with_retry(db);
        Ok(true)
    }

    /// Remove a label from an issue.
    ///
    /// Returns `Ok(true)` if the label was removed, `Ok(false)` if the issue
    /// did not carry the label (no-op short-circuit).
    ///
    /// # Errors
    ///
    /// Returns an error if the issue cannot be loaded or git operations fail.
    pub fn remove_label(&self, db: &Database, display_id: i64, label: &str) -> Result<bool> {
        let label_owned = label.to_string();

        let current = self.load_issue_by_id(display_id, db)?;
        if !current.labels.contains(&label_owned) {
            return Ok(false);
        }
        let issue_uuid = current.uuid;

        // Event-sourced (#604): emit `LabelRemoved`; compaction removes
        // it from the BTreeSet and materialize rewrites the JSON.
        let event = crate::events::Event::LabelRemoved {
            issue_uuid,
            label: label_owned,
        };
        self.emit_compact_push(event, &format!("unlabel {label} from issue #{display_id}"))?;

        self.hydrate_with_retry(db);
        Ok(true)
    }

    /// Add a blocker dependency: `issue_id` is blocked by `blocking_issue_id`.
    ///
    /// Only modifies the blocked issue's file (single-direction storage).
    ///
    /// Returns `Ok(true)` if the blocker was newly added, `Ok(false)` if it
    /// was already recorded (no-op short-circuit).
    ///
    /// # Errors
    ///
    /// Returns an error if either issue cannot be resolved or git operations fail.
    pub fn add_blocker(
        &self,
        db: &Database,
        issue_id: i64,
        blocking_issue_id: i64,
    ) -> Result<bool> {
        let blocker_uuid = self.resolve_uuid(blocking_issue_id, db)?;

        // Idempotency short-circuit (#600): if the blocker is already
        // present in the materialized JSON view, no event is needed.
        let current = self.load_issue_by_id(issue_id, db)?;
        if current.blockers.contains(&blocker_uuid) {
            return Ok(false);
        }
        let blocked_uuid = current.uuid;

        // Event-sourced path (#604): emit a `DependencyAdded` event and
        // let compaction materialize the updated issue JSON. The event
        // log is the source of truth; the JSON file is a derived view
        // rebuilt by the materialize step. This eliminates the
        // JSON-write/git-commit/SQLite-hydrate transactionality gap
        // that the previous `write_commit_push` path had — see #604 for
        // the failure-mode table that path's gaps produced.
        let event = crate::events::Event::DependencyAdded {
            blocked_uuid,
            blocker_uuid,
        };
        self.emit_compact_push(
            event,
            &format!("block issue #{issue_id} on #{blocking_issue_id}"),
        )?;

        self.hydrate_with_retry(db);
        Ok(true)
    }

    /// Remove a blocker dependency.
    ///
    /// Returns `Ok(true)` if the blocker was removed, `Ok(false)` if the
    /// blocker was not present (no-op short-circuit).
    ///
    /// # Errors
    ///
    /// Returns an error if either issue cannot be resolved or git operations fail.
    pub fn remove_blocker(
        &self,
        db: &Database,
        issue_id: i64,
        blocking_issue_id: i64,
    ) -> Result<bool> {
        let blocker_uuid = self.resolve_uuid(blocking_issue_id, db)?;

        let current = self.load_issue_by_id(issue_id, db)?;
        if !current.blockers.contains(&blocker_uuid) {
            return Ok(false);
        }
        let blocked_uuid = current.uuid;

        // Event-sourced (#604): emit `DependencyRemoved`; compaction
        // rewrites the issue JSON without the blocker.
        let event = crate::events::Event::DependencyRemoved {
            blocked_uuid,
            blocker_uuid,
        };
        self.emit_compact_push(
            event,
            &format!("unblock issue #{issue_id} from #{blocking_issue_id}"),
        )?;

        self.hydrate_with_retry(db);
        Ok(true)
    }

    /// Add a relation between two issues (single-direction storage).
    ///
    /// Returns `Ok(true)` if the relation was newly added, `Ok(false)` if
    /// it was already recorded (no-op short-circuit).
    ///
    /// # Errors
    ///
    /// Returns an error if either issue cannot be resolved or git operations fail.
    pub fn add_relation(&self, db: &Database, issue_id: i64, related_id: i64) -> Result<bool> {
        let related_uuid = self.resolve_uuid(related_id, db)?;

        let current = self.load_issue_by_id(issue_id, db)?;
        if current.related.contains(&related_uuid) {
            return Ok(false);
        }
        let uuid_a = current.uuid;

        // Event-sourced (#604): emit `RelationAdded`; compaction's
        // `apply_graph_event` arm adds the relation to both sides.
        let event = crate::events::Event::RelationAdded {
            uuid_a,
            uuid_b: related_uuid,
        };
        self.emit_compact_push(event, &format!("relate issue #{issue_id} to #{related_id}"))?;

        self.hydrate_with_retry(db);
        Ok(true)
    }

    /// Remove a relation between two issues.
    ///
    /// Returns `Ok(true)` if the relation was removed, `Ok(false)` if no
    /// such relation existed (no-op short-circuit).
    ///
    /// # Errors
    ///
    /// Returns an error if either issue cannot be resolved or git operations fail.
    pub fn remove_relation(&self, db: &Database, issue_id: i64, related_id: i64) -> Result<bool> {
        let related_uuid = self.resolve_uuid(related_id, db)?;

        let current = self.load_issue_by_id(issue_id, db)?;
        if !current.related.contains(&related_uuid) {
            return Ok(false);
        }
        let uuid_a = current.uuid;

        // Event-sourced (#604): compaction's `apply_graph_event` removes
        // both directions when it sees `RelationRemoved`.
        let event = crate::events::Event::RelationRemoved {
            uuid_a,
            uuid_b: related_uuid,
        };
        self.emit_compact_push(
            event,
            &format!("unrelate issue #{issue_id} from #{related_id}"),
        )?;

        self.hydrate_with_retry(db);
        Ok(true)
    }

    /// Rewrite a just-committed issue to set `display_id: null` and revert the
    /// counter bump. Used when a push failed (offline/exhausted retries) so the
    /// locally-claimed display ID doesn't collide with remote state.
    pub(super) fn rewrite_as_offline(&self, uuid: Uuid) -> Result<()> {
        // Serialize access to the hub cache (#373)
        let _lock_guard = self.sync.acquire_lock()?;

        let path = self.issue_path(&uuid);
        let mut issue = crate::issue_file::read_issue_file(&path)?;
        issue.display_id = None;
        let json = serde_json::to_string_pretty(&issue)?;
        std::fs::write(&path, json)?;

        // Revert the counter bump (the remote never saw it)
        let mut counters = self.read_counters()?;
        if counters.next_display_id > 1 {
            counters.next_display_id -= 1;
        }
        self.write_counters_to_cache(&counters)?;

        // Amend the local commit with the reverted files
        let rel_path = self.issue_rel_path(&uuid);
        self.git_in_cache(&["add", &rel_path])?;
        self.git_in_cache(&["add", "meta/counters.json"])?;
        self.git_commit_in_cache_with_args(&["--amend", "--no-edit"])?;
        Ok(())
    }
}
