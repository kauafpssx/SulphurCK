//! Pure business concepts and rules.
//!
//! This layer must not depend on terminal UI, networking, filesystem access, or
//! any concrete external service. It defines the language and settings the
//! application understands.

pub mod config;
pub mod ls;
