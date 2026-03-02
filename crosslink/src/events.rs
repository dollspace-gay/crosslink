//! Core event types, serialization, and log I/O for the event-sourced CRDT system.
//!
//! Events are append-only NDJSON records stored in per-agent log files at
//! `agents/{agent_id}/events.log` on the coordination branch. Each event
//! carries an `EventEnvelope` with ordering metadata and an optional SSH
//! signature.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;
use uuid::Uuid;

use crate::signing;

/// Total ordering key for events: (timestamp, agent_id, agent_seq).
///
/// Events are sorted by this key during compaction to produce a deterministic
/// materialized state regardless of which agent reads them.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct OrderingKey {
    pub timestamp: DateTime<Utc>,
    pub agent_id: String,
    pub agent_seq: u64,
}

impl OrderingKey {
    pub fn from_envelope(env: &EventEnvelope) -> Self {
        Self {
            timestamp: env.timestamp,
            agent_id: env.agent_id.clone(),
            agent_seq: env.agent_seq,
        }
    }
}

/// Event envelope — every event carries this metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub agent_id: String,
    pub agent_seq: u64,
    pub timestamp: DateTime<Utc>,
    pub event: Event,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signed_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// The 13 event variants across T1 and T2 tiers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Event {
    // T1: Exclusive
    IssueCreated {
        uuid: Uuid,
        title: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        priority: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        labels: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_uuid: Option<Uuid>,
        created_by: String,
    },
    LockClaimed {
        issue_display_id: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
    },
    LockReleased {
        issue_display_id: i64,
    },

    // T2: Causal
    IssueUpdated {
        uuid: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        priority: Option<String>,
    },
    StatusChanged {
        uuid: Uuid,
        new_status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        closed_at: Option<DateTime<Utc>>,
    },
    DependencyAdded {
        blocked_uuid: Uuid,
        blocker_uuid: Uuid,
    },
    DependencyRemoved {
        blocked_uuid: Uuid,
        blocker_uuid: Uuid,
    },
    RelationAdded {
        uuid_a: Uuid,
        uuid_b: Uuid,
    },
    RelationRemoved {
        uuid_a: Uuid,
        uuid_b: Uuid,
    },
    MilestoneAssigned {
        issue_uuid: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        milestone_uuid: Option<Uuid>,
    },
    LabelAdded {
        issue_uuid: Uuid,
        label: String,
    },
    LabelRemoved {
        issue_uuid: Uuid,
        label: String,
    },
    ParentChanged {
        issue_uuid: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        new_parent_uuid: Option<Uuid>,
    },
}

// ── Codec ────────────────────────────────────────────────────────────

/// Trait for encoding/decoding event envelopes.
pub trait EventCodec {
    fn encode(&self, event: &EventEnvelope) -> Result<Vec<u8>>;
    fn encode_batch(&self, events: &[EventEnvelope]) -> Result<Vec<u8>>;
    fn decode_all(&self, bytes: &[u8]) -> Result<Vec<EventEnvelope>>;
}

/// NDJSON (newline-delimited JSON) codec for event envelopes.
pub struct NdjsonCodec;

impl EventCodec for NdjsonCodec {
    fn encode(&self, event: &EventEnvelope) -> Result<Vec<u8>> {
        let mut line = serde_json::to_vec(event).context("Failed to encode event as JSON")?;
        line.push(b'\n');
        Ok(line)
    }

    fn encode_batch(&self, events: &[EventEnvelope]) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        for event in events {
            serde_json::to_writer(&mut buf, event).context("Failed to encode event")?;
            buf.push(b'\n');
        }
        Ok(buf)
    }

    fn decode_all(&self, bytes: &[u8]) -> Result<Vec<EventEnvelope>> {
        let mut events = Vec::new();
        for line in bytes.split(|&b| b == b'\n') {
            if line.is_empty() {
                continue;
            }
            let envelope: EventEnvelope =
                serde_json::from_slice(line).context("Failed to decode event line")?;
            events.push(envelope);
        }
        Ok(events)
    }
}

// ── Log I/O ──────────────────────────────────────────────────────────

/// Append an event to an agent's log file (creates file if needed).
pub fn append_event(log_path: &Path, envelope: &EventEnvelope) -> Result<()> {
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create dir for {}", log_path.display()))?;
    }
    let codec = NdjsonCodec;
    let bytes = codec.encode(envelope)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .with_context(|| format!("Failed to open event log: {}", log_path.display()))?;
    file.write_all(&bytes)
        .with_context(|| format!("Failed to append to event log: {}", log_path.display()))?;
    Ok(())
}

/// Read all events from a log file.
pub fn read_events(log_path: &Path) -> Result<Vec<EventEnvelope>> {
    if !log_path.exists() {
        return Ok(Vec::new());
    }
    let bytes = std::fs::read(log_path)
        .with_context(|| format!("Failed to read event log: {}", log_path.display()))?;
    let codec = NdjsonCodec;
    codec
        .decode_all(&bytes)
        .with_context(|| format!("Failed to parse event log: {}", log_path.display()))
}

/// Read only events with ordering key > watermark.
pub fn read_events_after(log_path: &Path, watermark: &OrderingKey) -> Result<Vec<EventEnvelope>> {
    let all = read_events(log_path)?;
    Ok(all
        .into_iter()
        .filter(|e| OrderingKey::from_envelope(e) > *watermark)
        .collect())
}

// ── Event Signing ────────────────────────────────────────────────────

/// Canonicalize an event envelope for signing.
///
/// Uses the event's JSON representation (without signature fields) as the
/// content to sign, ensuring deterministic canonical form.
fn canonicalize_event(envelope: &EventEnvelope) -> Vec<u8> {
    let event_json = serde_json::to_string(&envelope.event).unwrap_or_default();
    signing::canonicalize_for_signing(&[
        ("agent_id", &envelope.agent_id),
        ("agent_seq", &envelope.agent_seq.to_string()),
        ("timestamp", &envelope.timestamp.to_rfc3339()),
        ("event", &event_json),
    ])
}

/// Sign an event envelope using the agent's SSH key.
pub fn sign_event(
    envelope: &mut EventEnvelope,
    private_key_path: &Path,
    fingerprint: &str,
) -> Result<()> {
    let content = canonicalize_event(envelope);
    let sig = signing::sign_content(private_key_path, &content, "crosslink-event")?;
    envelope.signed_by = Some(fingerprint.to_string());
    envelope.signature = Some(sig);
    Ok(())
}

/// Verify an event's signature against the allowed signers store.
pub fn verify_event_signature(
    envelope: &EventEnvelope,
    allowed_signers_path: &Path,
) -> Result<bool> {
    let (signed_by, signature) = match (&envelope.signed_by, &envelope.signature) {
        (Some(s), Some(sig)) => (s, sig),
        _ => return Ok(false),
    };
    let content = canonicalize_event(envelope);
    let principal = format!("{}@crosslink", envelope.agent_id);
    signing::verify_content(
        allowed_signers_path,
        &principal,
        "crosslink-event",
        &content,
        signature,
    )
    .with_context(|| format!("Failed to verify event signature for {}", signed_by))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_envelope(agent_id: &str, seq: u64) -> EventEnvelope {
        EventEnvelope {
            agent_id: agent_id.to_string(),
            agent_seq: seq,
            timestamp: Utc::now(),
            event: Event::IssueCreated {
                uuid: Uuid::new_v4(),
                title: "Test issue".to_string(),
                description: None,
                priority: "medium".to_string(),
                labels: vec![],
                parent_uuid: None,
                created_by: agent_id.to_string(),
            },
            signed_by: None,
            signature: None,
        }
    }

    #[test]
    fn test_ndjson_codec_roundtrip() {
        let codec = NdjsonCodec;
        let envelope = make_envelope("agent-1", 1);
        let bytes = codec.encode(&envelope).unwrap();
        let decoded: EventEnvelope = serde_json::from_slice(bytes.trim_ascii()).unwrap();
        assert_eq!(decoded.agent_id, "agent-1");
        assert_eq!(decoded.agent_seq, 1);
    }

    #[test]
    fn test_ndjson_codec_batch_roundtrip() {
        let codec = NdjsonCodec;
        let envelopes = vec![make_envelope("agent-1", 1), make_envelope("agent-2", 2)];
        let bytes = codec.encode_batch(&envelopes).unwrap();
        let decoded = codec.decode_all(&bytes).unwrap();
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].agent_id, "agent-1");
        assert_eq!(decoded[1].agent_id, "agent-2");
    }

    #[test]
    fn test_ordering_key_comparison() {
        use chrono::Duration;
        let now = Utc::now();
        let k1 = OrderingKey {
            timestamp: now,
            agent_id: "a".to_string(),
            agent_seq: 1,
        };
        let k2 = OrderingKey {
            timestamp: now + Duration::seconds(1),
            agent_id: "a".to_string(),
            agent_seq: 1,
        };
        let k3 = OrderingKey {
            timestamp: now,
            agent_id: "b".to_string(),
            agent_seq: 1,
        };
        let k4 = OrderingKey {
            timestamp: now,
            agent_id: "a".to_string(),
            agent_seq: 2,
        };

        assert!(k1 < k2, "later timestamp should be greater");
        assert!(k1 < k3, "agent_id 'a' < 'b'");
        assert!(k1 < k4, "agent_seq 1 < 2");
    }

    #[test]
    fn test_append_and_read_events() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("agents/agent-1/events.log");

        let e1 = make_envelope("agent-1", 1);
        let e2 = make_envelope("agent-1", 2);

        append_event(&log_path, &e1).unwrap();
        append_event(&log_path, &e2).unwrap();

        let events = read_events(&log_path).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].agent_seq, 1);
        assert_eq!(events[1].agent_seq, 2);
    }

    #[test]
    fn test_read_events_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("nonexistent/events.log");
        let events = read_events(&log_path).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_read_events_after_watermark() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("events.log");

        let now = Utc::now();
        let mut e1 = make_envelope("agent-1", 1);
        e1.timestamp = now - chrono::Duration::seconds(10);
        let mut e2 = make_envelope("agent-1", 2);
        e2.timestamp = now;
        let mut e3 = make_envelope("agent-1", 3);
        e3.timestamp = now + chrono::Duration::seconds(10);

        append_event(&log_path, &e1).unwrap();
        append_event(&log_path, &e2).unwrap();
        append_event(&log_path, &e3).unwrap();

        let watermark = OrderingKey::from_envelope(&e1);
        let filtered = read_events_after(&log_path, &watermark).unwrap();
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].agent_seq, 2);
        assert_eq!(filtered[1].agent_seq, 3);
    }

    #[test]
    fn test_event_serde_all_variants() {
        let variants = vec![
            Event::IssueCreated {
                uuid: Uuid::new_v4(),
                title: "test".to_string(),
                description: Some("desc".to_string()),
                priority: "high".to_string(),
                labels: vec!["bug".to_string()],
                parent_uuid: None,
                created_by: "agent-1".to_string(),
            },
            Event::LockClaimed {
                issue_display_id: 1,
                branch: Some("feature/x".to_string()),
            },
            Event::LockReleased {
                issue_display_id: 1,
            },
            Event::IssueUpdated {
                uuid: Uuid::new_v4(),
                title: Some("new title".to_string()),
                description: None,
                priority: None,
            },
            Event::StatusChanged {
                uuid: Uuid::new_v4(),
                new_status: "closed".to_string(),
                closed_at: Some(Utc::now()),
            },
            Event::DependencyAdded {
                blocked_uuid: Uuid::new_v4(),
                blocker_uuid: Uuid::new_v4(),
            },
            Event::DependencyRemoved {
                blocked_uuid: Uuid::new_v4(),
                blocker_uuid: Uuid::new_v4(),
            },
            Event::RelationAdded {
                uuid_a: Uuid::new_v4(),
                uuid_b: Uuid::new_v4(),
            },
            Event::RelationRemoved {
                uuid_a: Uuid::new_v4(),
                uuid_b: Uuid::new_v4(),
            },
            Event::MilestoneAssigned {
                issue_uuid: Uuid::new_v4(),
                milestone_uuid: Some(Uuid::new_v4()),
            },
            Event::LabelAdded {
                issue_uuid: Uuid::new_v4(),
                label: "bug".to_string(),
            },
            Event::LabelRemoved {
                issue_uuid: Uuid::new_v4(),
                label: "bug".to_string(),
            },
            Event::ParentChanged {
                issue_uuid: Uuid::new_v4(),
                new_parent_uuid: None,
            },
        ];

        for event in variants {
            let json = serde_json::to_string(&event).unwrap();
            let parsed: Event = serde_json::from_str(&json).unwrap();
            // Verify the tag roundtrips
            let json2 = serde_json::to_string(&parsed).unwrap();
            let reparsed: Event = serde_json::from_str(&json2).unwrap();
            assert_eq!(
                serde_json::to_string(&parsed).unwrap(),
                serde_json::to_string(&reparsed).unwrap()
            );
        }
    }

    #[test]
    fn test_event_envelope_serde_roundtrip() {
        let envelope = EventEnvelope {
            agent_id: "agent-1".to_string(),
            agent_seq: 42,
            timestamp: Utc::now(),
            event: Event::LabelAdded {
                issue_uuid: Uuid::new_v4(),
                label: "feature".to_string(),
            },
            signed_by: Some("SHA256:abc".to_string()),
            signature: Some("sig123".to_string()),
        };
        let json = serde_json::to_string(&envelope).unwrap();
        let parsed: EventEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.agent_id, "agent-1");
        assert_eq!(parsed.agent_seq, 42);
        assert_eq!(parsed.signed_by, Some("SHA256:abc".to_string()));
    }

    #[test]
    fn test_event_envelope_optional_fields_omitted() {
        let envelope = make_envelope("agent-1", 1);
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(!json.contains("signed_by"));
        assert!(!json.contains("signature"));
    }

    #[test]
    fn test_canonicalize_event_deterministic() {
        let envelope = make_envelope("agent-1", 1);
        let c1 = canonicalize_event(&envelope);
        let c2 = canonicalize_event(&envelope);
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_ordering_key_from_envelope() {
        let envelope = make_envelope("agent-1", 5);
        let key = OrderingKey::from_envelope(&envelope);
        assert_eq!(key.agent_id, "agent-1");
        assert_eq!(key.agent_seq, 5);
        assert_eq!(key.timestamp, envelope.timestamp);
    }
}
