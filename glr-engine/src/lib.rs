#![cfg_attr(not(feature = "std"), no_std)]
#![deny(unsafe_code)]
//! RNGLR parser engine — Graph-Structured Stack (GSS) + shift/reduce driver.

extern crate alloc;

mod gss;
mod parser;

pub use parser::Parser;
