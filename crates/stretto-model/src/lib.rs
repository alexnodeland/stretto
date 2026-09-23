//! Action abstraction and Bayesian world models over agent traces.
//!
//! - [`abstraction`] turns an episode into a sequence of [`Step`]s: an abstract
//!   action (a tool, or a reply to the user) and what came back.
//! - [`world`] is a hierarchical Dirichlet back-off model of the next action
//!   given the last `k` steps, with the evaluation metrics Phase 0 reports.
//! - [`alpha`] infers the model's concentration from the data's marginal
//!   likelihood, with fugue.
//! - [`provenance`] classifies where each tool argument's value came from.
//! - [`bursts`] finds runs of tool calls a macro-tool could replace.
//! - [`policy`] reads policy compliance (confirmation before writes) off a trace.

pub mod abstraction;
pub mod alpha;
pub mod bursts;
pub mod policy;
pub mod provenance;
pub mod world;

pub use abstraction::{steps, Action, Outcome, Step, Vocab};
pub use world::{BackoffModel, CoveragePoint, EncodedEpisode, EvalStats};
