use glr_core::StateId;
use alloc::vec::Vec;

/// A node in the Graph-Structured Stack (GSS).
///
/// Keyed by `(parser_state, input_position)`. Two stack heads reaching the
/// same state at the same input position share a single GSS node — this is
/// the core sharing mechanism that makes GLR efficient.
#[derive(Debug, Clone)]
pub struct GssNode {
    pub state: StateId,
    pub input_position: u32,
    pub children: Vec<GssEdge>,
}

/// An edge in the GSS, carrying the reduced subtree node index.
#[derive(Debug, Clone)]
pub struct GssEdge {
    pub target: u32, // index into GSS node list
    pub subtree: u32, // index into the tree's node list
}
