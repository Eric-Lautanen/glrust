#![cfg_attr(not(feature = "std"), no_std)]
//! Lexer trait and [`Token`] type consumed by the GLR parser engine.

extern crate alloc;

mod token;

pub use token::{Lexer, Token};
