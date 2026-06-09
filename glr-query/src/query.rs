use glr_core::Node;
use std::vec::Vec;

/// Re-export of Node for use across query sub-modules.
pub type NodeRef = Node;

pub struct Query {
    pub states: Vec<QueryState>,
    pub captures: Vec<Capture>,
}

pub struct QueryState;

pub struct Capture {
    pub name: String,
}

pub struct QueryMatch {
    pub pattern_index: usize,
    pub captures: Vec<(usize, NodeRef)>,
}
