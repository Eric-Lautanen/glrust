use crate::SymbolId;
use alloc::vec::Vec;

/// Immutable parse tree backed by a compact arena of nodes.
#[derive(Debug, Clone)]
pub struct Tree {
    pub root: Option<Node>,
    pub nodes: Vec<Node>,
}

/// A single node in the parse tree.
#[derive(Debug, Clone)]
pub struct Node {
    pub kind: SymbolId,
    pub start_byte: u32,
    pub end_byte: u32,
    pub start_position: (u32, u32),
    pub end_position: (u32, u32),
    pub child_count: u32,
    pub named_child_count: u32,
    pub children: Vec<Node>,
    pub is_named: bool,
    pub is_missing: bool,
    pub is_extra: bool,
    pub has_changes: bool,
}

/// Mutable tree constructed during parsing; frozen to `Tree` upon completion.
#[derive(Debug)]
pub struct MutableTree {
    pub nodes: Vec<InternalNode>,
}

#[derive(Debug, Clone)]
pub struct InternalNode {
    pub kind: SymbolId,
    pub start_byte: u32,
    pub end_byte: u32,
    pub start_row: u32,
    pub start_col: u32,
    pub end_row: u32,
    pub end_col: u32,
    pub child_count: u32,
    pub named_child_count: u32,
    pub first_child: Option<u32>,
    pub next_sibling: Option<u32>,
    pub parent: Option<u32>,
    pub field_id: Option<u16>,
    pub is_named: bool,
    pub is_missing: bool,
    pub is_extra: bool,
    pub has_changes: bool,
}
