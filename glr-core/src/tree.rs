use crate::SymbolId;
use alloc::vec::Vec;

/// Immutable parse tree.
#[derive(Debug, Clone)]
pub struct Tree {
    pub root: Option<Node>,
}

impl Tree {
    pub fn root_node(&self) -> Option<&Node> {
        self.root.as_ref()
    }

    pub fn walk(&self) -> TreeCursor<'_> {
        TreeCursor {
            stack: self.root.as_ref().map(|n| NodeIter::new(n)).into_iter().collect(),
        }
    }
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

    pub fn child_by_field_name(&self, _name: &str) -> Option<&Node> {
        // field names will be tracked via field_id in Phase 1
        None
    }
}

/// Tree cursor for depth-first walk.
pub struct TreeCursor<'a> {
    stack: Vec<NodeIter<'a>>,
}

impl<'a> TreeCursor<'a> {
    /// Current node at the cursor.
    pub fn node(&self) -> Option<&'a Node> {
        self.stack.last().and_then(|iter| iter.current())
    }

    /// Enter the current node's first child.
    pub fn goto_first_child(&mut self) -> bool {
        match self.node() {
            Some(current) if !current.children.is_empty() => {
                self.stack.push(NodeIter::new(&current.children[0]));
                true
            }
            _ => false,
        }
    }

    /// Step to the next sibling of the current node.
    pub fn goto_next_sibling(&mut self) -> bool {
        match self.stack.last_mut() {
            Some(iter) => {
                iter.advance();
                iter.current().is_some()
            }
            None => false,
        }
    }

    /// Move back up to the parent.
    pub fn goto_parent(&mut self) -> bool {
        if self.stack.len() <= 1 {
            return false;
        }
        self.stack.pop();
        true
    }
}

struct NodeIter<'a> {
    node: &'a Node,
    index: usize,
}

impl<'a> NodeIter<'a> {
    fn new(node: &'a Node) -> Self {
        Self { node, index: 0 }
    }

    fn current(&self) -> Option<&'a Node> {
        if self.index < self.node.children.len() {
            Some(&self.node.children[self.index])
        } else {
            None
        }
    }

    fn advance(&mut self) {
        self.index += 1;
    }
}

// ── Mutable tree (built during parse) ──────────────────────────────

/// Mutable tree used during parsing; frozen to `Tree` when complete.
#[derive(Debug)]
pub struct MutableTree {
    pub nodes: Vec<InternalNode>,
    next_node_id: u32,
}

impl MutableTree {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            next_node_id: 0,
        }
    }

    /// Allocate a new node and return its index.
    pub fn alloc(&mut self, mut node: InternalNode) -> u32 {
        let id = self.next_node_id;
        self.next_node_id += 1;

        // Fix up child linkages
        let first_child = node.first_child;
        let named_count = count_named_children(&self.nodes, first_child);
        node.named_child_count = named_count;
        node.child_count = count_children(&self.nodes, first_child);

        self.nodes.push(node);

        // Set parent pointer on children
        let mut c = first_child;
        while let Some(child_id) = c {
            if let Some(child_node) = self.nodes.get_mut(child_id as usize) {
                child_node.parent = Some(id);
            }
            c = self.nodes.get(child_id as usize).and_then(|n| n.next_sibling);
        }

        id
    }

    /// Create a leaf node for a token.
    pub fn alloc_token(
        &mut self,
        kind: SymbolId,
        start_byte: u32,
        end_byte: u32,
        is_named: bool,
    ) -> u32 {
        self.alloc(InternalNode {
            kind,
            start_byte,
            end_byte,
            start_row: 0,
            start_col: 0,
            end_row: 0,
            end_col: 0,
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

    /// Freeze the mutable tree into an immutable `Tree`.
    pub fn freeze(&self) -> Tree {
        if self.nodes.is_empty() {
            return Tree { root: None };
        }

        // Find the root (node with no parent)
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
