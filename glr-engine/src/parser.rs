use glr_core::{Grammar, InputEdit, Tree};

/// The GLR parser.
///
/// Implements the RNGLR algorithm with a Graph-Structured Stack and error recovery.
pub struct Parser {
    grammar: Grammar,
}

impl Parser {
    /// Create a new parser for the given grammar.
    pub fn new(grammar: Grammar) -> Self {
        Self { grammar }
    }

    /// Fully parse `source` bytes, producing a `Tree`.
    ///
    /// Never fails — ERROR nodes are inserted for invalid syntax.
    pub fn parse(&self, _source: &[u8]) -> Tree {
        // TODO: Phase 0.3 — implement the GLR parse loop
        unimplemented!("GLR parse loop — see Phase 0.3 of the roadmap")
    }

    /// Incrementally re-parse after an edit, reusing unchanged subtrees.
    pub fn parse_incremental(
        &mut self,
        _old_tree: &Tree,
        _edit: &InputEdit,
        _source: &[u8],
    ) -> Tree {
        // TODO: Phase 1.3 — implement incremental re-parse
        unimplemented!("incremental re-parse — see Phase 1.3 of the roadmap")
    }
}
