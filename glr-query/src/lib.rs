#![deny(unsafe_code)]
//! Tree query engine — pattern matching over [`Tree`](glr_core::Tree) nodes.

pub mod compile;
pub mod execute;
pub mod query;

pub use execute::{QueryMatches, Queryable};
pub use query::{Capture, NodeRef, Query, QueryMatch};
