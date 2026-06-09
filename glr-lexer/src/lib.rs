#![cfg_attr(not(feature = "std"), no_std)]
#![allow(missing_docs)]

//! DFA-based lexer and `ExternalScanner` trait for GLR parsing.
//!
//! Provides a generic, table-driven lexer generated from grammar definitions,
//! and a trait for implementing hand-written external scanners (indentation,
//! heredocs, template strings, etc.).

extern crate alloc;

mod builtin_lexer;
mod external_scanner;

pub use builtin_lexer::BuiltinLexer;
pub use external_scanner::ExternalScanner;
