use crate::SymbolId;
use alloc::vec::Vec;

/// Immutable parse tree.
#[derive(Debug, Clone)]
pub struct Tree {
    pub root: Option<Node>,
}

/// A node in the parse tree. Children are stored inline.
#[derive(Debug, Clone)]
pub struct Node {
    pub kind: SymbolId,
    pub start_byte: u32,
    pub end_byte: u32,
    pub start_position: (u32, u32),
    pub end_position: (u32, u32),
    pub children: Vec<Node>,
    pub is_named: bool,
    pub is_missing: bool,
    pub is_extra: bool,
    pub has_changes: bool,
}

impl Node {
    pub fn named_children(&self) -> impl Iterator<Item = &Node> {
        self.children.iter().filter(|c| c.is_named)
    }
}

/// Tree cursor for walking the tree in depth-first order.
pub struct TreeCursor<'a> {
    stack: Vec<NodeIter<'a>>,
}

impl<'a> TreeCursor<'a> {
    pub fn new(node: &'a Node) -> Self {
        Self {
            stack: vec![NodeIter::new(node)],
        }
    }
}

impl<'a> Iterator for TreeCursor<'a> {
    type Item = &'a Node;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let iter = self.stack.last_mut()?;
            match iter.next() {
                Some(child) => {
                    if child.children.is_empty() {
                        return Some(child);
                    }
                    self.stack.push(NodeIter::new(child));
                }
                None => {
                    self.stack.pop();
                    if self.stack.is_empty() {
                        return None;
                    }
                    // Return the parent after visiting all children
                    let parent_iter = self.stack.last_mut()?;
                    // We need to return the parent node itself
                    match parent_iter.next() {
                        Some(parent) => return Some(parent),
                        None => continue,
                    }
                }
            }
        }
    }
}

/// Internal iterator over a node's children.
pub struct NodeIter<'a> {
    node: &'a Node,
    index: usize,
}

impl<'a> NodeIter<'a> {
    fn new(node: &'a Node) -> Self {
        Self { node, index: 0 }
    }
}

impl<'a> Iterator for NodeIter<'a> {
    type Item = &'a Node;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.node.children.len() {
            let child = &self.node.children[self.index];
            self.index += 1;
            Some(child)
        } else {
            None
        }
    }
}

/// Mutable tree used during parsing; frozen to `Tree` when complete.
#[derive(Debug)]
pub struct MutableTree {
    pub nodes: Vec<InternalNode>,
}

impl MutableTree {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    pub fn alloc(&mut self, node: InternalNode) -> u32 {
        let id = self.nodes.len() as u32;
        self.nodes.push(node);
        id
    }

    pub fn freeze(&self) -> Tree {
        Tree { root: None }
    }
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
