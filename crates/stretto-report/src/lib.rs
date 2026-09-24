//! Phase 0 measurement: how compressible is an agent's behavior?
//!
//! [`phase0::run`] reads τ²-bench results files and measures, per domain and
//! per agent model, on the official train/test split:
//!
//! - **predictability**: how well a habit learned from successful training
//!   episodes predicts the agent's next action on held-out tasks;
//! - **coverage**: the share of held-out decisions the habit could take at a
//!   given confidence, and how often it would agree with the agent;
//! - **transfer**: the same, training on one model and testing on another;
//! - **macro-tool headroom**: LLM turns spent inside runs of tool calls;
//! - **argument provenance**: where tool-argument values came from;
//! - **confirmation**: how often writes follow the user's "yes".
//!
//! With [`phase0::Config::shadow`] set, [`shadow`] (Phase 0b) also asks a
//! System-One model at every held-out decision a flow would hand it, scores
//! the answers, and re-runs the projection with them. With the v2 questions,
//! [`arbitrate`] also combines the answers with the habit (RFC-001 §3.6).
//!
//! [`render::markdown`] turns the result into a report, and
//! [`phase0::compile_flow`] keeps what a live read-only [`flow`] needs.

pub mod arbitrate;
pub mod flow;
pub mod phase0;
pub mod render;
pub mod shadow;
