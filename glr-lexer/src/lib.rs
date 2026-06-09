#![cfg_attr(not(feature = "std"), no_std)]
//! Lexer trait and [`Token`] type consumed by the GLR parser engine.

mod token;

pub use token::{LexError, Lexer, Token};
