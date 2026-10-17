//! Application use cases and orchestration.
//!
//! This layer coordinates domain rules. Transport and persistence concerns are
//! supplied at the composition root instead of being selected by the use case.

pub mod engine;
pub mod runner;
