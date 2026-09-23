//! Action abstraction and Bayesian world models over agent traces.
//!
//! - [`abstraction`] turns an episode into a sequence of [`Step`]s: an abstract
//!   action (a tool, or a reply to the user) and what came back.
//! - [`world`] is a hierarchical Dirichlet back-off model of the next action
//!   given the last `k` steps, with the evaluation metrics Phase 0 reports.
//! - [`alpha`] infers the model's concentration from the data's marginal
//!   likelihood, with fugue.
//! - [`features`] reads enum-like fields from tool outputs and keeps those that
//!   raise the marginal likelihood of the data.
//! - [`provenance`] classifies where each tool argument's value came from.
//! - [`bursts`] finds runs of tool calls a macro-tool could replace.
//! - [`policy`] reads policy compliance (confirmation before writes) off a trace.
//! - [`projection`] replays held-out episodes through macro-tool flows and
//!   counts the LLM turns they would save.

pub mod abstraction;
pub mod alpha;
pub mod bursts;
pub mod features;
pub mod policy;
pub mod projection;
pub mod provenance;
pub mod world;

pub use abstraction::{step_turns, steps, Action, Outcome, Step, Vocab};
pub use world::{BackoffModel, CoveragePoint, EncodedEpisode, EvalStats, GroupedModel, Predictor};
