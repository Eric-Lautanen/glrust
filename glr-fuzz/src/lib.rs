#![deny(unsafe_code)]
//! Fuzz testing targets (Tier 3 validation).
//!
//! Targets for `cargo fuzz`:
//! - GLR engine: random byte strings → no crash
//! - Incremental re-parse: random edits on real source → identity with full parse
//! - Lexer: arbitrary bytes → no panic, valid spans
//! - Query engine: random query strings + random trees → no panic

// Fuzz targets are defined in `fuzz/` directory using cargo-fuzz conventions.
