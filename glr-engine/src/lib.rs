#![cfg_attr(not(feature = "std"), no_std)]
#![allow(missing_docs)]

extern crate alloc;

mod gss;
mod parser;

pub use parser::Parser;
