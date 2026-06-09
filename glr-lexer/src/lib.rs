#![cfg_attr(not(feature = "std"), no_std)]
#![deny(unsafe_code)]
//! Lexer trait and [`Token`] type consumed by the GLR parser engine.

extern crate alloc;

mod dfa;
mod scanner;
mod token;

pub use dfa::{BuiltinLexer, DfaTableExt};
pub use glr_core::dfa::{DfaState, DfaTable, DfaTransition};
pub use scanner::CompositeLexer;
pub use token::{ExternalScanner, LexError, Lexer, Token};
