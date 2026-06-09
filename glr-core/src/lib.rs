#![cfg_attr(not(feature = "std"), no_std)]
#![allow(missing_docs)]

extern crate alloc;

pub mod grammar;
pub mod parse_table;
pub mod symbol;
pub mod tree;

pub use grammar::Grammar;
pub use parse_table::{ParseTable, ParseTableEntry, SmallStateRow};
pub use symbol::{Symbol, SymbolKind};
pub use tree::{InternalNode, MutableTree, Node, Tree, TreeCursor, NodeIter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StateId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymbolId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProductionId(pub u16);

#[derive(Debug, Clone)]
pub struct InputEdit {
    pub start_byte: u32,
    pub old_end_byte: u32,
    pub new_end_byte: u32,
    pub start_point: Point,
    pub old_end_point: Point,
    pub new_end_point: Point,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub row: u32,
    pub column: u32,
}
