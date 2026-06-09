use crate::gss::Gss;
use glr_core::parse_table::ParseTableEntry;
use glr_core::tree::MutableTree;
use glr_core::{Grammar, InputEdit, StateId, SymbolId, Tree};
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

    pub fn parse_with_lexer<L: Lexer>(&self, _source: &[u8], lexer: &mut L) -> Tree {
        let start_state = StateId(0);
        let mut gss = Gss::new(start_state);
        let mut tree = MutableTree::new();
        let mut error_pos = 0u32;

        loop {
            let valid = &[];
            let token = match lexer.next_token(valid) {
                Some(t) => t,
                None => break,
            };

            let heads: Vec<u32> = gss.heads.clone();
            let mut shifted = false;

            for &head_idx in &heads {
                let state = gss.nodes[head_idx as usize].state;
                let action = self.grammar.parse_table.lookup(state, token.kind);

                match action {
                    ParseTableEntry::Shift { state: target } => {
                        let pos = lexer.cursor();

                        let _leaf = tree.alloc_token(
                            token.kind,
                            token.start_byte,
                            token.end_byte,
                            0, 0, 0, 0,
                            false,
                        );

                        gss.add_node(target, pos);
                        shifted = true;
                    }
                    ParseTableEntry::Reduce {
                        symbol: _,
                        child_count: _,
                        dynamic_precedence: _,
                        production_id: _,
                    } => {
                        shifted = true;
                    }
                    ParseTableEntry::Accept => {
                        return tree.freeze();
                    }
                    ParseTableEntry::Error | ParseTableEntry::Goto { .. } => {}
                }
            }

            if !shifted && gss.head_count() > 0 {
                let _error_node = tree.alloc_token(
                    SymbolId::ERROR,
                    error_pos,
                    token.start_byte,
                    0, 0, 0, 0,
                    true,
                );

                error_pos = token.end_byte;
                let mut recovered = false;
                while let Some(skip) = lexer.next_token(valid) {
                    error_pos = skip.end_byte;

                    for &head_idx in &gss.heads {
                        let state = gss.nodes[head_idx as usize].state;
                        match self.grammar.parse_table.lookup(state, skip.kind) {
                            ParseTableEntry::Shift { .. }
                            | ParseTableEntry::Reduce { .. }
                            | ParseTableEntry::Accept => {
                                recovered = true;
                                break;
                            }
                            _ => {}
                        }
                    }
                    if recovered {
                        break;
                    }
                }
                if !recovered {
                    break;
                }
            }
        }

        tree.freeze()
    }

    pub fn parse(&self, _source: &[u8]) -> Tree {
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
