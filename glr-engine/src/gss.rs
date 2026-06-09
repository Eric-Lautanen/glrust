use alloc::vec::Vec;
use core::fmt;
use glr_core::StateId;

/// A node in the Graph-Structured Stack (GSS).
///
/// Keyed by `(state, input_position)`. When multiple parse heads reach
/// the same state at the same input position, they share a single GSS node.
#[derive(Clone)]
pub struct GssNode {
    pub state: StateId,
    pub position: u32,
}

impl fmt::Debug for GssNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GssNode")
            .field("state", &self.state)
            .field("position", &self.position)
            .finish()
    }
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
        for (i, n) in self.nodes.iter().enumerate() {
            if n.state == state && n.position == position {
                let idx = i as u32;
                self.heads.push(idx);
                return idx;
            }
        }
        let id = self.nodes.len() as u32;
        self.nodes.push(GssNode { state, position });
        self.heads.push(id);
        id
    }

    /// Return the number of active heads.
    pub fn head_count(&self) -> usize {
        self.heads.len()
    }
}
