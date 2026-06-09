#![cfg_attr(not(feature = "std"), no_std)]
#![allow(missing_docs)]

//! GLR parser engine: parse loop, Graph-Structured Stack (GSS), error recovery.
//!
//! Implements the RNGLR algorithm (Right-Nulled GLR, Scott & Johnstone 2006)
//! for correct handling of ε-rules. Produces a `Tree` via `glr-core`.

extern crate alloc;

mod gss;
mod parser;

pub use parser::Parser;
