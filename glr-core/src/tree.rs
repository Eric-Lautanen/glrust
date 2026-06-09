use crate::SymbolId;
use alloc::vec::Vec;

/// Bitmask of boolean flags on tree nodes.
///
/// Avoids clippy's `struct_excessive_bools` without changing the public API:
/// the accessor methods mirror the old field names so callers can migrate
/// with a mechanical `node.is_named` → `node.flags.is_named()` change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeFlags(u8);

impl NodeFlags {
    const NAMED: u8 = 0b0001;
    const MISSING: u8 = 0b0010;
    const EXTRA: u8 = 0b0100;
    const CHANGES: u8 = 0b1000;

    #[must_use]
    pub const fn new() -> Self {
        Self(0)
    }

    #[must_use]
    pub const fn is_named(self) -> bool {
        self.0 & Self::NAMED != 0
    }

    #[must_use]
    pub const fn is_missing(self) -> bool {
        self.0 & Self::MISSING != 0
    }

    #[must_use]
    pub const fn is_extra(self) -> bool {
        self.0 & Self::EXTRA != 0
    }

    #[must_use]
    pub const fn has_changes(self) -> bool {
        self.0 & Self::CHANGES != 0
    }

    pub fn set_named(&mut self, v: bool) {
        self.0 = (self.0 & !Self::NAMED) | (if v { Self::NAMED } else { 0 });
    }

    pub fn set_missing(&mut self, v: bool) {
        self.0 = (self.0 & !Self::MISSING) | (if v { Self::MISSING } else { 0 });
    }

    pub fn set_extra(&mut self, v: bool) {
        self.0 = (self.0 & !Self::EXTRA) | (if v { Self::EXTRA } else { 0 });
    }

    pub fn set_changes(&mut self, v: bool) {
        self.0 = (self.0 & !Self::CHANGES) | (if v { Self::CHANGES } else { 0 });
    }
}

impl Default for NodeFlags {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Tree {
    pub root: Option<Node>,
}

impl Tree {
    #[must_use]
    pub fn root_node(&self) -> Option<&Node> {
        self.root.as_ref()
    }

    /// Return a cursor rooted at the tree's root (or no-op if empty).
    #[must_use]
    pub fn walk(&self) -> TreeCursor<'_> {
        TreeCursor {
            tree: self,
            path: Vec::new(),
        }
    }

    /// Find the deepest node that contains byte offset.
    #[must_use]
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
    pub flags: NodeFlags,
}

impl Node {
    pub fn named_children(&self) -> impl Iterator<Item = &Node> {
        self.children.iter().filter(|c| c.flags.is_named())
    }

    /// Look up a child node by field name (Phase 2 adds field support).
    #[must_use]
    pub fn child_by_field_name(&self, _name: &str) -> Option<&Node> {
        None
    }

    /// Recursively find the deepest descendant containing `offset`.
    #[must_use]
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
    #[must_use]
    pub fn node(&self) -> Option<&'a Node> {
        let mut node = self.tree.root.as_ref()?;
        for &idx in &self.path {
            node = node.children.get(idx)?;
        }
        Some(node)
    }

    /// Move to the first child of the current node.
    pub fn goto_first_child(&mut self) -> bool {
        let Some(node) = self.node() else {
            return false;
        };
        if node.children.is_empty() {
            return false;
        }
        self.path.push(0);
        true
    }

    /// Move to the next sibling of the current node.
    ///
    /// Returns `false` and leaves the cursor unchanged if there is no next
    /// sibling. The path index is only committed when the sibling exists, so
    /// a subsequent `goto_parent` / `goto_first_child` round-trip is safe.
    pub fn goto_next_sibling(&mut self) -> bool {
        if self.path.is_empty() {
            return false;
        }
        let len = self.path.len();
        self.path[len - 1] += 1;
        if self.node().is_some() {
            true
        } else {
            // Roll back: leave the cursor pointing at the last valid sibling.
            self.path[len - 1] -= 1;
            false
        }
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
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            next_node_id: 0,
        }
    }

    /// Allocate an already-configured `InternalNode`. Sets up parent back-pointers
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
        // `is_named` marks this node itself as named; it has no children, so
        // both child counts are always 0 for leaf tokens.
        let mut flags = NodeFlags::new();
        flags.set_named(is_named);
        self.alloc(InternalNode {
            kind,
            start_byte,
            end_byte,
            start_row: start_position.0,
            start_col: start_position.1,
            end_row: end_position.0,
            end_col: end_position.1,
            child_count: 0,
            named_child_count: 0,
            first_child: None,
            next_sibling: None,
            parent: None,
            field_id: None,
            flags,
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

        let child_count = u32::try_from(children.len()).unwrap_or(u32::MAX);
        let named_child_count = children
            .iter()
            .filter(|&&cid| {
                self.nodes
                    .get(cid as usize)
                    .is_some_and(|n| n.flags.is_named())
            })
            .count();
        let named_child_count = u32::try_from(named_child_count).unwrap_or(u32::MAX);

        let mut flags = NodeFlags::new();
        flags.set_named(is_named);
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
            flags,
        });

        id
    }

    /// Freeze the mutable tree into an immutable `Tree`.
    #[must_use]
    ///
    /// The root is the *last* node with no parent. During GLR parsing, every
    /// shift and reduction allocates nodes in order; the final Accept reduction
    /// produces the topmost node last. Nodes from dead GLR branches are never
    /// linked as children of anything, so they also have no parent — but they
    /// are always allocated earlier than the root. Taking the last parentless
    /// node therefore reliably selects the accepted root.
    pub fn freeze(&self) -> Tree {
        if self.nodes.is_empty() {
            return Tree { root: None };
        }

        let root_internal = self.nodes.iter().rev().find(|n| n.parent.is_none());

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
    // Guard against corrupted next_sibling cycles: track visited ids.
    // In a well-formed tree this never fires; it's a safety net.
    let mut visited_siblings: alloc::collections::BTreeSet<u32> =
        alloc::collections::BTreeSet::new();
    while let Some(child_id) = c {
        if !visited_siblings.insert(child_id) {
            // Cycle detected — stop rather than loop forever.
            break;
        }
        if let Some(child) = all_nodes.get(child_id as usize) {
            children.push(build_immutable_node(all_nodes, child));
            c = child.next_sibling;
        } else {
            break;
        }
    }

    Node {
        kind: node.kind,
        start_byte: node.start_byte,
        end_byte: node.end_byte,
        start_position: (node.start_row, node.start_col),
        end_position: (node.end_row, node.end_col),
        children,
        flags: node.flags,
    }
}

fn count_named_children(nodes: &[InternalNode], first_child: Option<u32>) -> u32 {
    let mut count = 0;
    let mut c = first_child;
    while let Some(id) = c {
        if let Some(n) = nodes.get(id as usize) {
            if n.flags.is_named() {
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
    pub flags: NodeFlags,
}
