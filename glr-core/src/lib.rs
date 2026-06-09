#![cfg_attr(not(feature = "std"), no_std)]
#![allow(missing_docs)]

#[cfg(feature = "serde")]
extern crate serde;

extern crate alloc;

pub mod grammar;
pub mod parse_table;
pub mod symbol;
pub mod tree;

pub use grammar::{Grammar, Production};
pub use parse_table::{ParseTable, ParseTableEntry, SmallStateRow};
pub use symbol::{Symbol, SymbolKind};
pub use tree::{InternalNode, MutableTree, Node, Tree, TreeCursor};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StateId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SymbolId(pub u32);

impl SymbolId {
    pub const ERROR: Self = SymbolId(u32::MAX);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ProductionId(pub u16);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Point {
    pub row: u32,
    pub column: u32,
}
