#![cfg_attr(not(feature = "std"), no_std)]
#![allow(missing_docs)]

//! Core data structures for GLR parsing: `Grammar`, `ParseTable`, `Symbol`,
//! `Tree`, and associated types. All `#[no_std]` compatible.
//!
//! This crate provides the foundational types used by `glr-engine`, `glr-lexer`,
//! and downstream consumers. No parsing logic lives here — only data.

extern crate alloc;

mod grammar;
mod parse_table;
mod symbol;
mod tree;

pub use grammar::Grammar;
pub use parse_table::{ParseTable, ParseTableEntry};
pub use symbol::Symbol;
pub use tree::{InternalNode, MutableTree, Node, Tree};

/// Opaque identifier for a parser state in the LR parse table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StateId(pub u32);

/// Opaque identifier for a grammar symbol (terminal or nonterminal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymbolId(pub u32);

/// Opaque identifier for a grammar production.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProductionId(pub u16);

/// Describes a single text edit for incremental re-parse.
#[derive(Debug, Clone)]
pub struct InputEdit {
    pub start_byte: u32,
    pub old_end_byte: u32,
    pub new_end_byte: u32,
    pub start_point: Point,
    pub old_end_point: Point,
    pub new_end_point: Point,
}

/// A row/column position in source text (0-indexed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub row: u32,
    pub column: u32,
}
