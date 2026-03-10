//! LLM-assisted document decomposition orchestrator.
//!
//! This module decomposes design documents into phased execution plans using
//! an LLM (Claude) as the analysis backend. The resulting plan structures can
//! be stored on disk and surfaced through the REST API for review and
//! execution.
//!
//! # Modules
//!
//! - [`models`] — domain types for LLM interaction and plan storage
//! - [`decompose`] — core decomposition logic that shells out to `claude`

pub mod decompose;
pub mod models;
