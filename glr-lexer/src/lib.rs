#![cfg_attr(not(feature = "std"), no_std)]
#![allow(missing_docs)]

extern crate alloc;

mod token;

pub use token::{Token, Lexer};
