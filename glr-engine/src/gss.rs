use alloc::collections::BTreeMap;
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
    /// Serialized external scanner state at this node boundary.
    pub scanner_state: Vec<u8>,
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
    /// Map from `(state.0, position)` to index in `nodes` for O(log n) lookup.
    node_map: BTreeMap<(u32, u32), u32>,
}

impl Gss {
    pub fn new(initial_state: StateId) -> Self {
        let root = GssNode {
            state: initial_state,
            position: 0,
            edges: Vec::new(),
            scanner_state: Vec::new(),
        };
        let mut map = BTreeMap::new();
        map.insert((initial_state.0, 0), 0);
        Self {
            nodes: vec![root],
            heads: vec![0],
            node_map: map,
        }
    }

    /// Find or create a GSS node at `(state, position)`.
    ///
    /// Returns `(id, is_new)`. The caller is responsible for deciding whether
    /// to enqueue `id` as a head — this method never touches `self.heads`.
    /// Separating node allocation from head management avoids spurious duplicate
    /// head entries during GLR merge.
    #[must_use]
    pub fn find_or_create_node(&mut self, state: StateId, position: u32) -> (u32, bool) {
        let key = (state.0, position);
        if let Some(&id) = self.node_map.get(&key) {
            return (id, false);
        }
        let id = u32::try_from(self.nodes.len()).expect("GSS node count exceeds u32");
        self.nodes.push(GssNode {
            state,
            position,
            edges: Vec::new(),
            scanner_state: Vec::new(),
        });
        self.node_map.insert(key, id);
        (id, true)
    }

    /// Add an edge from `child` to `parent`, labeled with a tree node index.
    pub fn add_edge(&mut self, child: u32, parent: u32, tree_node: u32) {
        if let Some(node) = self.nodes.get_mut(child as usize) {
            node.edges.push(GssEdge { parent, tree_node });
        }
    }

    /// Collect tree-node chains for every path of length `depth` from `node_idx`.
    /// Returns `(ancestor_index, children)` for each valid path where
    /// `children[0]` is the leftmost (earliest-shifted) RHS symbol and
    /// `children[depth-1]` is the rightmost.
    ///
    /// The LR stack grows left-to-right as symbols are shifted, so the current
    /// node holds the *rightmost* symbol of the RHS and the walk proceeds
    /// toward the *leftmost*. Children are built outermost-first then reversed
    /// to give natural LR order [leftmost, ..., rightmost] without recursion.
    ///
    /// Uses a single mutable `children` Vec with push/pop to avoid cloning at
    /// every edge — the Vec is cloned only at leaf nodes where a full path
    /// has been assembled.
    #[must_use]
    pub fn ancestor_paths(&self, node_idx: u32, depth: u32) -> Vec<(u32, Vec<u32>)> {
        if depth == 0 {
            return vec![(node_idx, Vec::new())];
        }
        // Single mutable children Vec reused across all paths (push/pop).
        // Stack tracks: (node_index, next_edge_index, children_len_before_node).
        let mut children: Vec<u32> = Vec::with_capacity(depth as usize);
        let mut stack: Vec<(u32, usize, usize)> = Vec::new();
        let mut result = Vec::new();

        stack.push((node_idx, 0, 0));
        while let Some((idx, edge_idx, prefix_len)) = stack.last() {
            let idx = *idx;
            let edge_idx = *edge_idx;
            let prefix_len = *prefix_len;

            if children.len() as u32 == depth {
                let mut path = children.clone();
                path.reverse();
                result.push((idx, path));
                stack.pop();
                children.truncate(prefix_len);
                continue;
            }

            let node = match self.nodes.get(idx as usize) {
                Some(n) => n,
                None => {
                    stack.pop();
                    children.truncate(prefix_len);
                    continue;
                }
            };

            if edge_idx < node.edges.len() {
                let edge = &node.edges[edge_idx];
                stack.last_mut().unwrap().1 = edge_idx + 1;
                children.push(edge.tree_node);
                stack.push((edge.parent, 0, children.len() - 1));
            } else {
                stack.pop();
                children.truncate(prefix_len);
            }
        }
        result
    }
}
