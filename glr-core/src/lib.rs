#![cfg_attr(not(feature = "std"), no_std)]
//! Core data structures for GLR parsing: [`Grammar`], [`ParseTable`], [`Symbol`],
//! [`Tree`], and newtype ids ([`StateId`], [`SymbolId`], [`ProductionId`]).

#[cfg(feature = "serde")]
extern crate serde;

extern crate alloc;

pub mod grammar;
pub mod parse_table;
pub mod symbol;
pub mod tree;

pub use grammar::{Grammar, Production};
pub use parse_table::{ParseTable, ParseTableAction, ParseTableEntry, SmallStateRow};
pub use symbol::{Symbol, SymbolKind};
pub use tree::{InternalNode, MutableTree, Node, Tree, TreeCursor};

/// Newtype wrapper around a `u32` identifying an LR parser state.
///
/// # Note on epsilon-log keys in `glr-engine`
/// The parser uses `(state.0 as u16, production_id)` as a `BTreeSet` key for
/// the ε-reduction dedup log. This truncates state IDs above 65 535. For
/// grammars whose parse tables stay well under that limit this is fine; for
/// larger grammars the key type should be widened to `(u32, u16)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StateId(pub u32);

/// Newtype wrapper around a `u32` identifying a grammar symbol (terminal or
/// non-terminal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SymbolId(pub u32);

impl SymbolId {
    /// Sentinel symbol id used for error-token nodes in the parse tree.
    pub const ERROR: Self = SymbolId(u32::MAX);
}

/// Newtype wrapper around a `u16` identifying a production rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ProductionId(pub u16);

/// Describes a single edit operation on source text, used for incremental
/// re-parsing.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct InputEdit {
    pub start_byte: u32,
    pub old_end_byte: u32,
    pub new_end_byte: u32,
    pub start_point: Point,
    pub old_end_point: Point,
    pub new_end_point: Point,
}

/// A row/column position in source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Point {
    pub row: u32,
    pub column: u32,
}
