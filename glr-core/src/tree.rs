use crate::{StateId, SymbolId};
use alloc::string::String;
use alloc::vec::Vec;

#[cfg(feature = "rc")]
use alloc::rc::Rc;
#[cfg(not(feature = "rc"))]
use alloc::sync::Arc;

#[cfg(not(feature = "rc"))]
type TreeRef = Arc<TreeInner>;
#[cfg(feature = "rc")]
type TreeRef = Rc<TreeInner>;

#[cfg(not(feature = "rc"))]
fn make_mut(this: &mut TreeRef) -> &mut TreeInner {
    Arc::make_mut(this)
}
#[cfg(feature = "rc")]
fn make_mut(this: &mut TreeRef) -> &mut TreeInner {
    Rc::make_mut(this)
}

/// Byte range and row/column position of a parsed node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextSpan {
    pub start_byte: u32,
    pub end_byte: u32,
    pub start_position: (u32, u32),
    pub end_position: (u32, u32),
}

/// Bitmask of boolean flags on tree nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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

/// Index into a [`Tree`] node arena.
pub type NodeId = usize;

/// An immutable tree backed by a compact arena of [`Node`]s, wrapped in
/// [`Arc`] (or `Rc` behind the `rc` feature) for cheap clone / shared
/// ownership.
#[derive(Debug, Clone)]
pub struct Tree {
    inner: TreeRef,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
struct TreeInner {
    nodes: Vec<Node>,
    root: Option<NodeId>,
    symbol_names: Vec<String>,
    field_names: Vec<String>,
}

impl Clone for TreeInner {
    fn clone(&self) -> Self {
        Self {
            nodes: self.nodes.clone(),
            root: self.root,
            symbol_names: self.symbol_names.clone(),
            field_names: self.field_names.clone(),
        }
    }
}

impl Tree {
    /// Build a tree from a pre-built arena.
    #[must_use]
    pub fn from_arena(nodes: Vec<Node>, root: Option<NodeId>) -> Self {
        Self {
            inner: TreeRef::new(TreeInner {
                nodes,
                root,
                symbol_names: Vec::new(),
                field_names: Vec::new(),
            }),
        }
    }

    /// Attach symbol name lookup information (from a `Grammar`).
    #[must_use]
    pub fn with_symbol_names(mut self, names: Vec<String>) -> Self {
        let inner = make_mut(&mut self.inner);
        inner.symbol_names = names;
        self
    }

    /// Attach field name lookup information (from a `Grammar`).
    #[must_use]
    pub fn with_field_names(mut self, names: Vec<String>) -> Self {
        let inner = make_mut(&mut self.inner);
        inner.field_names = names;
        self
    }

    /// Resolve a symbol id to its string name.
    #[must_use]
    pub fn symbol_name(&self, id: SymbolId) -> &str {
        self.inner
            .symbol_names
            .get(id.0 as usize)
            .map_or("<unknown>", |s| s.as_str())
    }

    /// Resolve a field id to its string name.
    #[must_use]
    pub fn field_name(&self, field_id: u16) -> &str {
        self.inner
            .field_names
            .get(field_id as usize)
            .map_or("<unknown>", |s| s.as_str())
    }

    /// Return the field names slice.
    #[must_use]
    pub fn field_names(&self) -> &[String] {
        &self.inner.field_names
    }

    /// The root node, if the tree is non-empty.
    #[must_use]
    pub fn root_node(&self) -> Option<&Node> {
        self.inner.root.map(|idx| &self.inner.nodes[idx])
    }

    /// Return a cursor rooted at the tree's root (or no-op if empty).
    #[must_use]
    pub fn walk(&self) -> TreeCursor<'_> {
        TreeCursor {
            nodes: &self.inner.nodes,
            root: self.inner.root,
            path: Vec::new(),
        }
    }

    /// Find the deepest node that contains byte offset.
    #[must_use]
    pub fn node_at_byte(&self, offset: u32) -> Option<&Node> {
        let idx = self.inner.root?;
        self.inner.nodes[idx].node_at_byte(offset, &self.inner.nodes)
    }

    /// Mark all nodes whose byte range overlaps `[start, end)` as changed.
    /// Since the tree is immutable (behind `Arc`/`Rc`), this requires making
    /// a unique (mutable) clone of the inner data.
    pub fn mark_edit_range(&mut self, start: u32, end: u32) {
        let inner = make_mut(&mut self.inner);
        if let Some(root) = &mut inner.root {
            mark_range_inner(&mut inner.nodes, *root, start, end);
        }
    }

    /// Access the underlying node arena (for query / traversal utilities).
    #[must_use]
    pub fn nodes(&self) -> &[Node] {
        &self.inner.nodes
    }

    /// Return the index of the root node, if any.
    #[must_use]
    pub fn root_id(&self) -> Option<NodeId> {
        self.inner.root
    }

    /// Look up a node by its arena index.
    #[must_use]
    pub fn node_by_id(&self, id: NodeId) -> Option<&Node> {
        self.inner.nodes.get(id)
    }
}

fn mark_range_inner(nodes: &mut [Node], root_idx: NodeId, start: u32, end: u32) {
    let mut changed = alloc::vec![false; nodes.len()];
    let mut stack: Vec<(NodeId, bool)> = Vec::new();
    stack.push((root_idx, false));

    while let Some((idx, processed)) = stack.pop() {
        if nodes[idx].end_byte <= start || nodes[idx].start_byte >= end {
            continue;
        }
        if processed {
            let overlaps = nodes[idx].start_byte < end && nodes[idx].end_byte > start;
            let any_child = nodes[idx]
                .children
                .iter()
                .any(|&cid| changed.get(cid).copied().unwrap_or(false));
            if overlaps || any_child {
                changed[idx] = true;
                nodes[idx].flags.set_changes(true);
            }
        } else {
            stack.push((idx, true));
            for &cid in nodes[idx].children.iter().rev() {
                stack.push((cid, false));
            }
        }
    }
}

/// A node in the parse tree, stored inside a [`Tree`] arena.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Node {
    pub kind: SymbolId,
    pub start_byte: u32,
    pub end_byte: u32,
    pub start_position: (u32, u32),
    pub end_position: (u32, u32),
    /// Indices into the owning [`Tree`]'s node arena.
    pub children: Vec<NodeId>,
    pub flags: NodeFlags,
    /// LR parser state at this node boundary, used for incremental re-parse
    /// subtree reuse. `None` for leaf / token nodes.
    pub parser_state: Option<StateId>,
    /// Field name id of this node relative to its parent.
    pub field_id: Option<u16>,
}

impl Node {
    /// Iterate over named children by resolving child indices against the
    /// owning tree's node arena.
    pub fn named_children<'a>(
        &'a self,
        nodes: &'a [Node],
    ) -> impl Iterator<Item = &'a Node> + use<'a> {
        self.children
            .iter()
            .filter_map(move |&idx| nodes.get(idx))
            .filter(|c| c.flags.is_named())
    }

    /// Recursively find the deepest descendant containing `offset`.
    /// `nodes` must be the arena of the tree that owns this node.
    #[must_use]
    pub fn node_at_byte<'a>(&'a self, offset: u32, nodes: &'a [Node]) -> Option<&'a Node> {
        if offset < self.start_byte || offset >= self.end_byte {
            return None;
        }
        for &child_idx in &self.children {
            if let Some(child) = nodes.get(child_idx) {
                if let Some(found) = child.node_at_byte(offset, nodes) {
                    return Some(found);
                }
            }
        }
        Some(self)
    }
}

/// A cursor for walking a [`Tree`] in DFS order, using a path of child
/// indices into the arena.
pub struct TreeCursor<'a> {
    nodes: &'a [Node],
    root: Option<NodeId>,
    path: Vec<usize>,
}

impl<'a> TreeCursor<'a> {
    /// Resolve the current node from the path.
    #[must_use]
    pub fn node(&self) -> Option<&'a Node> {
        let mut idx = self.root?;
        for &depth in &self.path {
            let node = self.nodes.get(idx)?;
            idx = *node.children.get(depth)?;
        }
        self.nodes.get(idx)
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
    pub fn goto_next_sibling(&mut self) -> bool {
        if self.path.is_empty() {
            return false;
        }
        let len = self.path.len();
        self.path[len - 1] += 1;
        if self.node().is_some() {
            true
        } else {
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

    /// Allocate an already-configured `InternalNode`. Sets up parent
    /// back-pointers for its children.
    #[must_use]
    pub fn alloc(&mut self, mut node: InternalNode) -> u32 {
        let id = self.next_node_id;
        self.next_node_id += 1;

        let first_child = node.first_child;
        node.named_child_count = count_children(&self.nodes, first_child, true);
        node.child_count = count_children(&self.nodes, first_child, false);

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
            parser_state: None,
        })
    }

    /// Compute the combined span covering all given child nodes.
    #[must_use]
    pub fn span_for_children(&self, children: &[u32]) -> TextSpan {
        if children.is_empty() {
            return TextSpan {
                start_byte: 0,
                end_byte: 0,
                start_position: (0, 0),
                end_position: (0, 0),
            };
        }
        let first = &self.nodes[children[0] as usize];
        let last = &self.nodes[children[children.len() - 1] as usize];
        TextSpan {
            start_byte: first.start_byte,
            end_byte: last.end_byte,
            start_position: (first.start_row, first.start_col),
            end_position: (last.end_row, last.end_col),
        }
    }

    /// Allocate an internal (nonterminal) node with given child indices.
    ///
    /// # Panics
    /// Panics if `children.len()` exceeds `u32::MAX`.
    pub fn alloc_internal(
        &mut self,
        kind: SymbolId,
        children: &[u32],
        span: TextSpan,
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

        let child_count = u32::try_from(children.len()).expect("child count exceeds u32");
        let named_child_count = children
            .iter()
            .filter(|&&cid| {
                self.nodes
                    .get(cid as usize)
                    .is_some_and(|n| n.flags.is_named())
            })
            .count();
        let named_child_count =
            u32::try_from(named_child_count).expect("named child count exceeds u32");

        let mut flags = NodeFlags::new();
        flags.set_named(is_named);
        self.nodes.push(InternalNode {
            kind,
            start_byte: span.start_byte,
            end_byte: span.end_byte,
            start_row: span.start_position.0,
            start_col: span.start_position.1,
            end_row: span.end_position.0,
            end_col: span.end_position.1,
            child_count,
            named_child_count,
            first_child,
            next_sibling: None,
            parent: None,
            field_id: None,
            flags,
            parser_state: None,
        });

        id
    }

    /// Freeze the mutable tree into an immutable [`Tree`] backed by a
    /// compact arena of [`Node`]s wrapped in [`Arc`].
    ///
    /// The root is the *last* node with no parent. During GLR parsing, every
    /// shift and reduction allocates nodes in order; the final Accept
    /// reduction produces the topmost node last. Nodes from dead GLR branches
    /// are never linked as children of anything, so they also have no parent
    /// — but they are always allocated earlier than the root. Taking the last
    /// parentless node therefore reliably selects the accepted root.
    ///
    /// # Panics
    /// Panics if a root node is found but its arena index cannot be resolved
    /// after the post-order traversal.
    #[must_use]
    pub fn freeze(&self) -> Tree {
        fn collect_ids(
            internal: &[InternalNode],
            id: u32,
            out: &mut Vec<u32>,
            visited: &mut alloc::collections::BTreeSet<u32>,
        ) {
            if !visited.insert(id) {
                return;
            }
            let node = &internal[id as usize];
            let mut c = node.first_child;
            while let Some(cid) = c {
                collect_ids(internal, cid, out, visited);
                c = internal.get(cid as usize).and_then(|n| n.next_sibling);
            }
            out.push(id);
        }

        if self.nodes.is_empty() {
            return Tree::from_arena(Vec::new(), None);
        }

        let Some(root_idx) = self.nodes.iter().rposition(|n| n.parent.is_none()) else {
            return Tree::from_arena(Vec::new(), None);
        };
        let root_idx_u32 = u32::try_from(root_idx).expect("root index exceeds u32");

        // Build the flat arena via post-order traversal so children precede
        // their parent (good for cache locality).
        let mut arena_nodes: Vec<Node> = Vec::with_capacity(self.nodes.len());
        let mut id_map: alloc::vec::Vec<Option<NodeId>> = alloc::vec![None; self.nodes.len()];

        let mut order: Vec<u32> = Vec::new();
        let mut visited = alloc::collections::BTreeSet::new();
        collect_ids(&self.nodes, root_idx_u32, &mut order, &mut visited);

        for &old_id in &order {
            let old = &self.nodes[old_id as usize];
            let new_id = arena_nodes.len();

            let mut children: Vec<NodeId> = Vec::new();
            let mut c = old.first_child;
            while let Some(cid) = c {
                if let Some(mapped) = id_map[cid as usize] {
                    children.push(mapped);
                }
                c = self.nodes.get(cid as usize).and_then(|n| n.next_sibling);
            }

            arena_nodes.push(Node {
                kind: old.kind,
                start_byte: old.start_byte,
                end_byte: old.end_byte,
                start_position: (old.start_row, old.start_col),
                end_position: (old.end_row, old.end_col),
                children,
                flags: old.flags,
                parser_state: old.parser_state,
                field_id: old.field_id,
            });
            id_map[old_id as usize] = Some(new_id);
        }

        let root_new_id = id_map[root_idx_u32 as usize].unwrap();
        Tree::from_arena(arena_nodes, Some(root_new_id))
    }
}

fn count_children(nodes: &[InternalNode], first_child: Option<u32>, named_only: bool) -> u32 {
    let mut count = 0;
    let mut c = first_child;
    while let Some(id) = c {
        if let Some(n) = nodes.get(id as usize) {
            if !named_only || n.flags.is_named() {
                count += 1;
            }
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
    /// LR parser state at this node boundary. Set by the parser during
    /// reduction; used by incremental re-parse for subtree reuse.
    pub parser_state: Option<StateId>,
}
