use crate::gss::Gss;
use glr_core::parse_table::ParseTableEntry;
use glr_core::tree::MutableTree;
use glr_core::{Grammar, InputEdit, StateId, Tree};
use glr_lexer::Lexer;
use alloc::vec::Vec;

/// The GLR parser using the RNGLR algorithm.
pub struct Parser {
    grammar: Grammar,
}

impl Parser {
    pub fn new(grammar: Grammar) -> Self {
        Self { grammar }
    }

    /// Fully parse `source` using the given lexer.
    pub fn parse_with_lexer<L: Lexer>(&self, source: &[u8], lexer: &mut L) -> Tree {
        let _ = source;
        let start_state = StateId(0);
        let mut gss = Gss::new(start_state);
        let tree = MutableTree::new();

        loop {
            // Fetch the next token. valid_symbols is a placeholder — proper
            // valid-symbol tracking per state will be added in Phase 1.
            let valid = &[];
            let token = lexer.next_token(valid);
            let token = match token {
                Some(t) => t,
                None => break,
            };

            // Process the token against all active heads
            let mut processed = false;

            // We need to clone the heads list because we'll modify gss in the loop
            let heads: Vec<u32> = gss.heads.clone();

            for &head_idx in &heads {
                let state = gss.nodes[head_idx as usize].state;
                let action = self.grammar.parse_table.lookup(state, token.kind);

                match action {
                    ParseTableEntry::Shift { state: target } => {
                        let pos = lexer.cursor() as u32;
                        gss.add_node(target, pos);
                        processed = true;
                    }
                    ParseTableEntry::Reduce {
                        symbol: _,
                        child_count: _,
                        dynamic_precedence: _,
                        production_id: _,
                    } => {
                        // Reduce: will be fully implemented in the next step
                        processed = true;
                    }
                    ParseTableEntry::Accept => {
                        return tree.freeze();
                    }
                    ParseTableEntry::Error => {
                        // Head can't proceed — remove it
                        // (will use proper head tracking later)
                    }
                    ParseTableEntry::Goto { state: _ } => {
                        // Goto is only used after a reduce
                    }
                }
            }

            if !processed {
                // All heads dead — insert ERROR node and try to recover
                // (Phase 0.5)
                break;
            }
        }

        tree.freeze()
    }

    /// Full parse using a default built-in lexer.
    pub fn parse(&self, _source: &[u8]) -> Tree {
        // Phase 0.3: requires a lexer — use parse_with_lexer for now
        Tree { root: None }
    }

    pub fn parse_incremental(
        &mut self,
        _old_tree: &Tree,
        _edit: &InputEdit,
        _source: &[u8],
    ) -> Tree {
        unimplemented!("Phase 1.3 — incremental re-parse")
    }
}
