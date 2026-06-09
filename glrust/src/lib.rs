#![deny(unsafe_code)]
//! `glrust` — facade crate for the GLR parser ecosystem.
//!
//! Re-exports all `glr-*` sub-crates so consumers can depend on a single
//! `glrust` crate instead of wiring up each sub-crate individually.

pub use glr_bench as bench;
pub use glr_conformance as conformance;
pub use glr_core as core;
pub use glr_engine as engine;
pub use glr_fuzz as fuzz;
pub use glr_grammar as grammar;
pub use glr_lexer as lexer;
pub use glr_macro as macros;
pub use glr_query as query;
pub use glr_syntax as syntax;
