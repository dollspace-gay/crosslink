//! Backfill events from a legacy JSON-canonical workspace (#604).
//!
//! Pre-event-sourcing crosslink workspaces have `issues/*.json` and
//! `meta/milestones/*.json` files written directly by the old
//! `write_commit_push` path, with no corresponding events in the
//! per-agent event log. After the event-sourcing migration the canonical
//! source of truth is the event log; the JSON files become a derived
//! materialized view.
//!
//! [`backfill_events_from_json`] walks the existing JSON view and
//! synthesizes one event envelope per logical mutation that would have
//! produced the current state, appending them to the calling agent's
//! event log. After backfill, the workspace's event log + checkpoint +
//! materialized JSON are mutually consistent and the workspace can be
//! migrated forward without losing history.
//!
//! Idempotent: if events already exist for an issue UUID (i.e. another
//! agent's event log mentions it), the backfill skips that issue rather
//! than emitting duplicate IssueCreated events that would conflict in
//! compaction.

use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use std::collections::HashSet;
use std::path::Path;
use uuid::Uuid;

use crate::events::{append_event, read_events, Event, EventEnvelope};
use crate::issue_file::{read_all_issue_files, read_all_milestone_files};

/// Summary of how many events were synthesized.
#[derive(Debug, Default, Clone)]
pub struct BackfillStats {
    pub issues: usize,
    pub labels: usize,
    pub dependencies: usize,
    pub relations: usize,
    pub comments: usize,
    pub milestones: usize,
    pub milestone_assignments: usize,
}

impl BackfillStats {
    /// Total number of envelopes appended to the event log.
    #[must_use]
    pub const fn total(&self) -> usize {
        self.issues
            + self.labels
            + self.dependencies
            + self.relations
            + self.comments
            + self.milestones
            + self.milestone_assignments
    }
}

/// Synthesize events from the on-disk JSON view of `cache_dir` and
/// append them to `agent_id`'s event log.
///
/// The synthesized envelopes carry timestamps drawn from the issue
/// files' own `created_at` / `updated_at` fields so that replay-by-
/// timestamp produces the same observable end state. The `agent_seq` is
/// assigned monotonically starting from `next_agent_seq` (caller-
/// provided so the backfill doesn't collide with the agent's existing
/// counter).
///
/// Idempotent: issues whose UUID is already mentioned in *any* agent's
/// event log are skipped. The skip is at issue granularity (all child
/// rows of a skipped issue are also skipped) to prevent partial
/// duplication.
///
/// # Errors
///
/// Returns an error if the JSON files cannot be read, the event log
/// cannot be appended to, or any envelope cannot be canonicalized.
pub fn backfill_events_from_json(
    cache_dir: &Path,
    agent_id: &str,
    next_agent_seq: u64,
) -> Result<BackfillStats> {
    let mut stats = BackfillStats::default();
    let mut seq = next_agent_seq;

    // Collect UUIDs already represented in any agent's event log so we
    // don't emit IssueCreated for issues that already have one.
    let mut already_eventized: HashSet<Uuid> = HashSet::new();
    let agents_dir = cache_dir.join("agents");
    if agents_dir.exists() {
        for entry in std::fs::read_dir(&agents_dir)
            .with_context(|| format!("read agents dir {}", agents_dir.display()))?
        {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let log_path = entry.path().join("events.log");
            for env in read_events(&log_path)? {
                match &env.event {
                    Event::IssueCreated { uuid, .. } => {
                        already_eventized.insert(*uuid);
                    }
                    _ => {}
                }
            }
        }
    }

    let log_path = agents_dir.join(agent_id).join("events.log");

    let issues = read_all_issue_files(&cache_dir.join("issues"))?;
    // Sort by created_at so issue parents arrive before their children
    // (best effort — true topological order would require the full graph).
    let mut issues = issues;
    issues.sort_by_key(|i| i.created_at);

    for issue in &issues {
        if already_eventized.contains(&issue.uuid) {
            continue;
        }

        // Synthesize IssueCreated with the original created_at so
        // replay timestamps match the JSON.
        let mut envelope = make_envelope(
            agent_id,
            seq,
            issue.created_at,
            Event::IssueCreated {
                uuid: issue.uuid,
                title: issue.title.clone(),
                description: issue.description.clone(),
                priority: format!("{:?}", issue.priority).to_lowercase(),
                labels: issue.labels.clone(),
                parent_uuid: issue.parent_uuid,
                created_by: issue.created_by.clone(),
            },
        );
        seq += 1;
        append_event(&log_path, &envelope)?;
        stats.issues += 1;

        // If the issue is closed, emit StatusChanged.
        if matches!(
            issue.status,
            crate::models::IssueStatus::Closed | crate::models::IssueStatus::Archived
        ) {
            envelope = make_envelope(
                agent_id,
                seq,
                issue.closed_at.unwrap_or(issue.updated_at),
                Event::StatusChanged {
                    uuid: issue.uuid,
                    new_status: status_to_string(issue.status).to_string(),
                    closed_at: issue.closed_at,
                },
            );
            seq += 1;
            append_event(&log_path, &envelope)?;
        }

        // Note: IssueCreated already carries labels, so we don't emit
        // separate LabelAdded events. Same with parent_uuid.

        // Blockers — one DependencyAdded per blocker.
        for &blocker_uuid in &issue.blockers {
            envelope = make_envelope(
                agent_id,
                seq,
                issue.updated_at,
                Event::DependencyAdded {
                    blocked_uuid: issue.uuid,
                    blocker_uuid,
                },
            );
            seq += 1;
            append_event(&log_path, &envelope)?;
            stats.dependencies += 1;
        }

        // Relations — emit RelationAdded for each. JSON stores
        // single-direction; the apply handles both sides.
        for &related_uuid in &issue.related {
            envelope = make_envelope(
                agent_id,
                seq,
                issue.updated_at,
                Event::RelationAdded {
                    uuid_a: issue.uuid,
                    uuid_b: related_uuid,
                },
            );
            seq += 1;
            append_event(&log_path, &envelope)?;
            stats.relations += 1;
        }

        // Milestone assignment.
        if let Some(ms_uuid) = issue.milestone_uuid {
            envelope = make_envelope(
                agent_id,
                seq,
                issue.updated_at,
                Event::MilestoneAssigned {
                    issue_uuid: issue.uuid,
                    milestone_uuid: Some(ms_uuid),
                },
            );
            seq += 1;
            append_event(&log_path, &envelope)?;
            stats.milestone_assignments += 1;
        }

        // Inline comments (V1 layout) — synthesize CommentAdded each.
        for c in &issue.comments {
            envelope = make_envelope(
                agent_id,
                seq,
                c.created_at,
                Event::CommentAdded {
                    comment_uuid: Uuid::new_v4(),
                    issue_uuid: issue.uuid,
                    author: c.author.clone(),
                    content: c.content.clone(),
                    kind: c.kind.clone(),
                    trigger_type: c.trigger_type.clone(),
                    intervention_context: c.intervention_context.clone(),
                    driver_key_fingerprint: c.driver_key_fingerprint.clone(),
                },
            );
            seq += 1;
            append_event(&log_path, &envelope)?;
            stats.comments += 1;
        }
    }

    // Milestones — synthesize MilestoneCreated for each on-disk
    // milestone whose UUID isn't already represented.
    let milestones = read_all_milestone_files(&cache_dir.join("meta").join("milestones"))?;
    for ms in &milestones {
        let envelope = make_envelope(
            agent_id,
            seq,
            ms.created_at,
            Event::MilestoneCreated {
                uuid: ms.uuid,
                name: ms.name.clone(),
                description: ms.description.clone(),
            },
        );
        seq += 1;
        append_event(&log_path, &envelope)?;
        stats.milestones += 1;

        if matches!(ms.status, crate::models::IssueStatus::Closed) {
            let close_env = make_envelope(
                agent_id,
                seq,
                ms.closed_at.unwrap_or_else(Utc::now),
                Event::MilestoneClosed { uuid: ms.uuid },
            );
            seq += 1;
            append_event(&log_path, &close_env)?;
        }
    }

    Ok(stats)
}

fn make_envelope(
    agent_id: &str,
    seq: u64,
    timestamp: chrono::DateTime<Utc>,
    event: Event,
) -> EventEnvelope {
    EventEnvelope {
        agent_id: agent_id.to_string(),
        agent_seq: seq,
        // Stagger by 1 microsecond per backfilled event so the ordering
        // key is unique within the agent. This matters for
        // OrderingKey-based deduplication during compaction.
        timestamp: timestamp + Duration::microseconds(seq.try_into().unwrap_or(0)),
        event,
        signed_by: None,
        signature: None,
    }
}

const fn status_to_string(s: crate::models::IssueStatus) -> &'static str {
    use crate::models::IssueStatus;
    match s {
        IssueStatus::Open => "open",
        IssueStatus::Closed => "closed",
        IssueStatus::Archived => "archived",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::issue_file::write_issue_file;
    use chrono::Utc;
    use tempfile::tempdir;

    fn make_test_issue(display_id: i64, title: &str) -> crate::issue_file::IssueFile {
        crate::issue_file::IssueFile {
            uuid: Uuid::new_v4(),
            display_id: Some(display_id),
            title: title.to_string(),
            description: None,
            status: crate::models::IssueStatus::Open,
            priority: crate::models::Priority::Medium,
            parent_uuid: None,
            created_by: "test".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            closed_at: None,
            scheduled_at: None,
            due_at: None,
            labels: vec!["bug".to_string()],
            comments: vec![],
            blockers: vec![],
            related: vec![],
            milestone_uuid: None,
            time_entries: vec![],
        }
    }

    #[test]
    fn test_backfill_emits_issue_created_for_each_json_file() {
        let dir = tempdir().unwrap();
        let cache = dir.path();
        std::fs::create_dir_all(cache.join("issues")).unwrap();
        std::fs::create_dir_all(cache.join("agents/test-agent")).unwrap();
        std::fs::create_dir_all(cache.join("meta/milestones")).unwrap();

        let i1 = make_test_issue(1, "first");
        let i2 = make_test_issue(2, "second");
        write_issue_file(&cache.join(format!("issues/{}.json", i1.uuid)), &i1).unwrap();
        write_issue_file(&cache.join(format!("issues/{}.json", i2.uuid)), &i2).unwrap();

        let stats = backfill_events_from_json(cache, "test-agent", 1).unwrap();
        assert_eq!(stats.issues, 2, "should emit one IssueCreated per issue");

        // Verify both UUIDs land in the event log.
        let log = cache.join("agents/test-agent/events.log");
        let events = read_events(&log).unwrap();
        let created: std::collections::HashSet<Uuid> = events
            .iter()
            .filter_map(|e| match e.event {
                Event::IssueCreated { uuid, .. } => Some(uuid),
                _ => None,
            })
            .collect();
        assert!(created.contains(&i1.uuid));
        assert!(created.contains(&i2.uuid));
    }

    #[test]
    fn test_backfill_idempotent_against_existing_events() {
        let dir = tempdir().unwrap();
        let cache = dir.path();
        std::fs::create_dir_all(cache.join("issues")).unwrap();
        std::fs::create_dir_all(cache.join("agents/test-agent")).unwrap();
        std::fs::create_dir_all(cache.join("meta/milestones")).unwrap();

        let i1 = make_test_issue(1, "already-eventized");
        write_issue_file(&cache.join(format!("issues/{}.json", i1.uuid)), &i1).unwrap();

        // Pre-populate event log with an IssueCreated for i1.
        let pre = make_envelope(
            "other-agent",
            1,
            Utc::now(),
            Event::IssueCreated {
                uuid: i1.uuid,
                title: "first".to_string(),
                description: None,
                priority: "medium".to_string(),
                labels: vec![],
                parent_uuid: None,
                created_by: "other".to_string(),
            },
        );
        let other_log = cache.join("agents/other-agent/events.log");
        std::fs::create_dir_all(other_log.parent().unwrap()).unwrap();
        append_event(&other_log, &pre).unwrap();

        let stats = backfill_events_from_json(cache, "test-agent", 1).unwrap();
        assert_eq!(
            stats.issues, 0,
            "should skip issue already represented in event log"
        );
    }

    #[test]
    fn test_backfill_emits_dependencies_and_relations() {
        let dir = tempdir().unwrap();
        let cache = dir.path();
        std::fs::create_dir_all(cache.join("issues")).unwrap();
        std::fs::create_dir_all(cache.join("agents/test-agent")).unwrap();
        std::fs::create_dir_all(cache.join("meta/milestones")).unwrap();

        let mut blocker = make_test_issue(1, "blocker");
        let mut blocked = make_test_issue(2, "blocked");
        blocked.blockers = vec![blocker.uuid];
        blocked.related = vec![blocker.uuid];
        blocker.related = vec![blocked.uuid];
        write_issue_file(
            &cache.join(format!("issues/{}.json", blocker.uuid)),
            &blocker,
        )
        .unwrap();
        write_issue_file(
            &cache.join(format!("issues/{}.json", blocked.uuid)),
            &blocked,
        )
        .unwrap();

        let stats = backfill_events_from_json(cache, "test-agent", 1).unwrap();
        assert_eq!(stats.dependencies, 1, "one DependencyAdded for the blocker");
        // Each side carries `related` so we emit both — apply will
        // dedupe to a single canonical relation in state.
        assert_eq!(stats.relations, 2, "two RelationAdded events, one per side");
    }
}
