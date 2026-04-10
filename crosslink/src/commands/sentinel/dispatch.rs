use std::time::Duration;

use crate::commands::kickoff::VerifyLevel;

/// What the triage engine decides to do with a signal.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Disposition {
    /// Spawn a kickoff agent with this scope.
    Dispatch {
        description: String,
        scope: AgentScope,
        attempt: u32,
    },
    /// Create a crosslink issue for human review.
    Triage {
        priority: String,
        labels: Vec<String>,
    },
    /// Already handled or no matching rule — skip.
    Skip { reason: String },
    /// Eligible but cannot dispatch right now.
    Defer { reason: String },
}

/// Constrains what a dispatched agent can do.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AgentScope {
    pub allowed_paths: Vec<String>,
    pub verify: VerifyLevel,
    pub timeout: Duration,
    pub model: String,
}
