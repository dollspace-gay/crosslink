//! Orchestrator module — DAG-based execution engine for design document plans.
//!
//! This module provides:
//! - [`dag`] — directed acyclic graph with topological sort and ready-node detection
//! - [`executor`] — execution lifecycle management with kickoff integration

pub mod dag;
pub mod executor;
