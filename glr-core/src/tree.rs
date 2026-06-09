use crate::SymbolId;
use alloc::vec::Vec;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Tree {
    pub root: Option<Node>,
}

impl Tree {
    pub fn root_node(&self) -> Option<&Node> {
        self.root.as_ref()
    }

    /// Return a cursor rooted at the tree's root (or no-op if empty).
    pub fn walk(&self) -> TreeCursor<'_> {
        TreeCursor {
            tree: self,
            path: Vec::new(),
        }
    }

    /// Find the deepest node that contains byte offset.
    pub fn node_at_byte(&self, offset: u32) -> Option<&Node> {
        self.root.as_ref().and_then(|n| n.node_at_byte(offset))
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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

    /// Look up a child node by field name (Phase 2 adds field support).
    pub fn child_by_field_name(&self, _name: &str) -> Option<&Node> {
        None
    }

    /// Recursively find the deepest descendant containing `offset`.
    pub fn node_at_byte(&self, offset: u32) -> Option<&Node> {
        if offset < self.start_byte || offset >= self.end_byte {
            return None;
        }
        for child in &self.children {
            if let Some(found) = child.node_at_byte(offset) {
                return Some(found);
            }
        }
        Some(self)
    }
}

/// A cursor for walking a `Tree` in DFS order, using a path of child indices.
pub struct TreeCursor<'a> {
    tree: &'a Tree,
    path: Vec<usize>,
}

impl<'a> TreeCursor<'a> {
    /// Resolve the current node from the path.
    pub fn node(&self) -> Option<&'a Node> {
        let mut node = self.tree.root.as_ref()?;
        for &idx in &self.path {
            node = node.children.get(idx)?;
        }
        Some(node)
    }

    /// Move to the first child of the current node.
    pub fn goto_first_child(&mut self) -> bool {
        let node = match self.node() {
            Some(n) => n,
            None => return false,
        };
        if node.children.is_empty() {
            return false;
        }
        self.path.push(0);
        true
    }

    /// Move to the next sibling of the current node.
    pub fn goto_next_sibling(&mut self) -> bool {
        if self.path.is_empty() {
            return false;
        }
        let len = self.path.len();
        self.path[len - 1] += 1;
        self.node().is_some()
    }

    /// Move to the parent of the current node.
    pub fn goto_parent(&mut self) -> bool {
        if self.path.is_empty() {
            return false;
        }
        self.path.pop();
        true
    }
}

// ---------------------------------------------------------------------------
// MutableTree – arena-backed construction used during parsing
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct MutableTree {
    pub nodes: Vec<InternalNode>,
    next_node_id: u32,
}

impl Default for MutableTree {
    fn default() -> Self {
        Self::new()
    }
}

impl MutableTree {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            next_node_id: 0,
        }
    }

    /// Allocate an already-configured InternalNode. Sets up parent back-pointers
    /// for its children.
    pub fn alloc(&mut self, mut node: InternalNode) -> u32 {
        let id = self.next_node_id;
        self.next_node_id += 1;

        let first_child = node.first_child;
        let named_count = count_named_children(&self.nodes, first_child);
        node.named_child_count = named_count;
        node.child_count = count_children(&self.nodes, first_child);

        self.nodes.push(node);

        let mut c = first_child;
        while let Some(child_id) = c {
            if let Some(child_node) = self.nodes.get_mut(child_id as usize) {
                child_node.parent = Some(id);
            }
            c = self
                .nodes
                .get(child_id as usize)
                .and_then(|n| n.next_sibling);
        }

        id
    }

    /// Allocate a leaf (token) node.
    pub fn alloc_token(
        &mut self,
        kind: SymbolId,
        start_byte: u32,
        end_byte: u32,
        start_position: (u32, u32),
        end_position: (u32, u32),
        is_named: bool,
    ) -> u32 {
        self.alloc(InternalNode {
            kind,
            start_byte,
            end_byte,
            start_row: start_position.0,
            start_col: start_position.1,
            end_row: end_position.0,
            end_col: end_position.1,
            child_count: 0,
            named_child_count: if is_named { 1 } else { 0 },
            first_child: None,
            next_sibling: None,
            parent: None,
            field_id: None,
            is_named,
            is_missing: false,
            is_extra: false,
            has_changes: false,
        })
    }

    /// Allocate an internal (nonterminal) node with given child indices.
    /// Links children into a sibling list and sets parent back-pointers.
    #[allow(clippy::too_many_arguments)]
    pub fn alloc_internal(
        &mut self,
        kind: SymbolId,
        children: &[u32],
        start_byte: u32,
        end_byte: u32,
        start_position: (u32, u32),
        end_position: (u32, u32),
        is_named: bool,
    ) -> u32 {
        let id = self.next_node_id;
        self.next_node_id += 1;

        let first_child = children.first().copied();
        for (i, &child_id) in children.iter().enumerate() {
            if let Some(child) = self.nodes.get_mut(child_id as usize) {
                child.parent = Some(id);
                child.next_sibling = children.get(i + 1).copied();
            }
        }

        let child_count = children.len() as u32;
        let named_child_count = children
            .iter()
            .filter(|&&cid| {
                self.nodes
                    .get(cid as usize)
                    .map(|n| n.is_named)
                    .unwrap_or(false)
            })
            .count() as u32;

        self.nodes.push(InternalNode {
            kind,
            start_byte,
            end_byte,
            start_row: start_position.0,
            start_col: start_position.1,
            end_row: end_position.0,
            end_col: end_position.1,
            child_count,
            named_child_count,
            first_child,
            next_sibling: None,
            parent: None,
            field_id: None,
            is_named,
            is_missing: false,
            is_extra: false,
            has_changes: false,
        });

        id
    }

    /// Freeze the mutable tree into an immutable `Tree`.
    pub fn freeze(&self) -> Tree {
        if self.nodes.is_empty() {
            return Tree { root: None };
        }

        let root_internal = self
            .nodes
            .iter()
            .find(|n| n.parent.is_none())
            .or_else(|| self.nodes.last());

        match root_internal {
            Some(internal) => {
                let root = build_immutable_node(&self.nodes, internal);
                Tree { root: Some(root) }
            }
            None => Tree { root: None },
        }
    }
}

fn build_immutable_node(all_nodes: &[InternalNode], node: &InternalNode) -> Node {
    let mut children = Vec::new();
    let mut c = node.first_child;
    while let Some(child_id) = c {
        if let Some(child) = all_nodes.get(child_id as usize) {
            children.push(build_immutable_node(all_nodes, child));
        }
        c = all_nodes
            .get(child_id as usize)
            .and_then(|n| n.next_sibling);
    }

    Node {
        kind: node.kind,
        start_byte: node.start_byte,
        end_byte: node.end_byte,
        start_position: (node.start_row, node.start_col),
        end_position: (node.end_row, node.end_col),
        children,
        is_named: node.is_named,
        is_missing: node.is_missing,
        is_extra: node.is_extra,
        has_changes: node.has_changes,
    }
}

fn count_named_children(nodes: &[InternalNode], first_child: Option<u32>) -> u32 {
    let mut count = 0;
    let mut c = first_child;
    while let Some(id) = c {
        if let Some(n) = nodes.get(id as usize) {
            if n.is_named {
                count += 1;
            }
            c = n.next_sibling;
        } else {
            break;
        }
    }
    count
}

fn count_children(nodes: &[InternalNode], first_child: Option<u32>) -> u32 {
    let mut count = 0;
    let mut c = first_child;
    while let Some(id) = c {
        if let Some(n) = nodes.get(id as usize) {
            count += 1;
            c = n.next_sibling;
        } else {
            break;
        }
    }
    count
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
