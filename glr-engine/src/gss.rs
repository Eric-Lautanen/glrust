use glr_core::StateId;
use alloc::vec::Vec;
use core::fmt;

/// A node in the Graph-Structured Stack (GSS).
///
/// Keyed by `(state, input_position)`. When multiple parse heads reach
/// the same state at the same input position, they share a single GSS node.
#[derive(Clone)]
pub struct GssNode {
    pub state: StateId,
    pub position: u32,
    pub edges: Vec<GssEdge>,
}

impl fmt::Debug for GssNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GssNode")
            .field("state", &self.state)
            .field("position", &self.position)
            .field("edges", &self.edges.len())
            .finish()
    }
}

/// An edge in the GSS, carrying the reduced subtree node index.
#[derive(Debug, Clone)]
pub struct GssEdge {
    pub target: u32,
    pub subtree: u32,
    pub production_id: u16,
}

/// The Graph-Structured Stack.
#[derive(Debug, Clone)]
pub struct Gss {
    pub nodes: Vec<GssNode>,
    /// Active heads — indices into `nodes`. Multiple heads may point to the
    /// same GSS node (that is the GLR merge: when two parse paths converge
    /// on the same state at the same input position).
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

    /// Add or find a GSS node at `(state, position)`.
    ///
    /// If a node already exists at this key, it is reused and a **new head**
    /// is registered pointing to it (the GLR merge). This is what makes GLR
    /// O(n) in practice for unambiguous grammars.
    ///
    /// Returns the index of the node.
    pub fn add_node(&mut self, state: StateId, position: u32) -> u32 {
        // Check all nodes (not just heads) for a merge candidate.
        for (i, n) in self.nodes.iter().enumerate() {
            if n.state == state && n.position == position {
                let idx = i as u32;
                // Register a new head at the existing node — this is the
                // GLR merge: two parse paths now share this node.
                self.heads.push(idx);
                return idx;
            }
        }
        // No existing node — create a new one.
        let id = self.nodes.len() as u32;
        self.nodes.push(GssNode {
            state,
            position,
            edges: Vec::new(),
        });
        self.heads.push(id);
        id
    }

    /// Add an edge from `from` to `to`, labeled with a subtree and production.
    pub fn add_edge(&mut self, from: u32, to: u32, subtree: u32, production_id: u16) {
        self.nodes[from as usize]
            .edges
            .push(GssEdge { target: to, subtree, production_id });
    }

    /// Remove a head by index (swap-removes for O(1)).
    pub fn remove_head(&mut self, index: usize) {
        self.heads.swap_remove(index);
    }

    /// Return the number of active heads.
    pub fn head_count(&self) -> usize {
        self.heads.len()
    }

    /// Check if there are no active heads.
    pub fn is_dead(&self) -> bool {
        self.heads.is_empty()
    }
}
