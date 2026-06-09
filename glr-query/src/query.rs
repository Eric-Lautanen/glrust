use glr_core::Node;
use std::vec::Vec;

/// Re-export of Node for use across query sub-modules.
pub type NodeRef = Node;

/// A compiled query.
pub struct Query {
    /// The patterns to match.
    pub patterns: Vec<Pattern>,
    /// Named capture targets `@name`.
    pub captures: Vec<Capture>,
}

/// A single pattern in a query.
#[derive(Debug, Clone)]
pub struct Pattern {
    /// Node kind to match. `None` for wildcard `_` or `*`.
    pub kind: Option<String>,
    /// If true, this pattern matches only named nodes (`_` wildcard).
    /// If `kind` is `None` and this is false, it matches any node (`*`).
    pub named: bool,
    /// If true, `kind` is a quoted anonymous string like `"return"`.
    pub is_anonymous: bool,
    /// Field constraints: `name: (child_pattern)`.
    pub field_constraints: Vec<FieldConstraint>,
    /// Positional child patterns (matched in order against children).
    pub child_patterns: Vec<Pattern>,
    /// Index into `Query::captures` if this pattern is captured with `@name`.
    pub capture_index: Option<usize>,
}

/// A field-constrained child pattern.
#[derive(Debug, Clone)]
pub struct FieldConstraint {
    pub name: String,
    pub pattern: Box<Pattern>,
}

/// A named capture target.
#[derive(Debug, Clone)]
pub struct Capture {
    pub name: String,
}

/// A single match result from executing a query.
#[derive(Debug, Clone)]
pub struct QueryMatch {
    /// Index into `Query::patterns` that matched.
    pub pattern_index: usize,
    /// Captured nodes: `(capture_index, node)` pairs.
    pub captures: Vec<(usize, NodeRef)>,
}
