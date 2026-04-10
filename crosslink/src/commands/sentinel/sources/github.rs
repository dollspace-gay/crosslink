use anyhow::Result;

use super::{Signal, Source};
use crate::commands::sentinel::config::SentinelConfig;

/// Polls GitHub for issues with `agent-todo:*` labels via the `gh` CLI.
#[allow(dead_code)]
pub struct GitHubLabelSource {
    labels: Vec<String>,
    repo: Option<String>,
}

#[allow(dead_code)]
impl GitHubLabelSource {
    pub fn new(config: &SentinelConfig) -> Result<Self> {
        Ok(Self {
            labels: config.sources.github_labels.labels.clone(),
            repo: None,
        })
    }
}

impl Source for GitHubLabelSource {
    fn name(&self) -> &str {
        "github-labels"
    }

    fn poll(&mut self) -> Result<Vec<Signal>> {
        // Will be implemented in #652
        Ok(Vec::new())
    }
}
