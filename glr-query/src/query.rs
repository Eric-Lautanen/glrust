use std::vec::Vec;

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
    pub captures: Vec<(usize, crate::execute::NodeRef)>,
}
