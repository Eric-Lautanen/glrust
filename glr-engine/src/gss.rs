use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use glr_core::StateId;

/// An edge from a child GSS node to its parent, labeled with the tree node
/// index that was shifted or reduced along this edge.
#[derive(Debug, Clone)]
pub struct GssEdge {
    pub parent: u32,
    pub tree_node: u32,
}

/// A node in the Graph-Structured Stack (GSS).
///
/// Keyed by `(state, input_position)`. When multiple parse heads reach
/// the same state at the same input position, they share a single GSS node
/// and their parent edges are merged.
#[derive(Clone)]
pub struct GssNode {
    pub state: StateId,
    pub position: u32,
    /// parent edges — more than one means GLR merge occurred here
    pub edges: Vec<GssEdge>,
}

impl fmt::Debug for GssNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GssNode")
            .field("state", &self.state)
            .field("position", &self.position)
            .field("edge_count", &self.edges.len())
            .finish()
    }
}

/// The Graph-Structured Stack.
#[derive(Debug, Clone)]
pub struct Gss {
    pub nodes: Vec<GssNode>,
    /// Active heads — indices into `nodes`. Multiple heads may point to the
    /// same GSS node.
    pub heads: Vec<u32>,
}

impl Gss {
    pub fn new(initial_state: StateId) -> Self {
        let root = GssNode {
            state: initial_state,
            position: 0,
            edges: Vec::new(),
        };
        Self {
            nodes: vec![root],
            heads: vec![0],
        }
    }

    /// Find or create a GSS node at `(state, position)`.
    /// If the node already exists, register an additional head for it (GLR merge).
    pub fn add_node(&mut self, state: StateId, position: u32) -> u32 {
        for (i, n) in self.nodes.iter().enumerate() {
            if n.state == state && n.position == position {
                let idx = i as u32;
                self.heads.push(idx);
                return idx;
            }
        }
        let id = self.nodes.len() as u32;
        self.nodes.push(GssNode {
            state,
            position,
            edges: Vec::new(),
        });
        self.heads.push(id);
        id
    }

    /// Add an edge from `child` to `parent`, labeled with a tree node index.
    pub fn add_edge(&mut self, child: u32, parent: u32, tree_node: u32) {
        if let Some(node) = self.nodes.get_mut(child as usize) {
            node.edges.push(GssEdge { parent, tree_node });
        }
    }

    /// Collect tree-node chains for every path of length `depth` from `node_idx`.
    /// Returns (ancestor_index, subtree_node_indices[0..depth]) for each valid path.
    /// Children are returned in RHS order (leftmost-first, i.e. the order they
    /// were pushed onto the LR stack).
    pub fn ancestor_paths(&self, node_idx: u32, depth: u32) -> Vec<(u32, Vec<u32>)> {
        if depth == 0 {
            return vec![(node_idx, Vec::new())];
        }
        let mut result = Vec::new();
        if let Some(node) = self.nodes.get(node_idx as usize) {
            for edge in &node.edges {
                for (ancestor, mut chain) in self.ancestor_paths(edge.parent, depth - 1) {
                    chain.push(edge.tree_node);
                    result.push((ancestor, chain));
                }
            }
        }
        result
    }

    /// Remove a head by index.
    #[allow(dead_code)]
    pub fn remove_head(&mut self, head_idx: usize) {
        if head_idx < self.heads.len() {
            self.heads.swap_remove(head_idx);
        }
    }

    #[allow(dead_code)]
    pub fn head_count(&self) -> usize {
        self.heads.len()
    }
}
