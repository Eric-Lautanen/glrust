//! Procedural macro for defining GLR grammars via a Rust-native DSL.
//!
//! # Example
//!
//! ```ignore
//! glr_grammar! {
//!     language: "json",
//!     tokens: {
//!         "string" = /"[^"]*"/,
//!         "number" = /\d+(\.\d+)?/,
//!     },
//!     rules: {
//!         Document = { Value },
//!         Value    = { "string" } | { "number" } | { Object } | { Array },
//!     },
//! }
//! ```

use proc_macro::TokenStream;

#[proc_macro]
pub fn glr_grammar(_input: TokenStream) -> TokenStream {
    // TODO: Phase 2.1 — parse DSL and generate parse table + lexer DFA at compile time
    unimplemented!("glr_grammar! macro — see Phase 2.1 of the roadmap")
}
