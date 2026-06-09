//! Syntax highlighting pipeline.
//!
//! Consumes a compiled `Grammar`, a set of compiled `Query` objects from `.scm`
//! highlight files, and source bytes, and returns an iterator of
//! `(byte_range, highlight_name)` pairs.

mod highlight;

pub use highlight::highlight;
