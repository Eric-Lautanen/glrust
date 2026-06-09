use alloc::vec::Vec;
use glr_core::parse_table::ParseTableEntry;
use glr_core::tree::MutableTree;
use glr_core::{Grammar, InputEdit, StateId, SymbolId, Tree};
use glr_lexer::Lexer;

use crate::gss::Gss;

fn mark_seen(seen: &mut Vec<bool>, idx: u32) -> bool {
    let i = idx as usize;
    if i >= seen.len() {
        seen.resize(i + 1, false);
    }
    let already = seen[i];
    seen[i] = true;
    already
}

/// The GLR parser using the RNGLR algorithm.
pub struct Parser {
    grammar: Grammar,
}

impl Parser {
    pub fn new(grammar: Grammar) -> Self {
        Self { grammar }
    }

    /// Parse source bytes using a provided lexer.
    pub fn parse_with_lexer<L: Lexer>(&self, _source: &[u8], lexer: &mut L) -> Tree {
        let mut gss = Gss::new(StateId(0));
        let mut tree = MutableTree::new();
        let mut error_start = 0u32;

        loop {
            let valid = &[];
            let token = match lexer.next_token(valid) {
                Some(t) => t,
                None => break,
            };
            let mut epsilon_log: Vec<(StateId, u16)> = Vec::new();

            // Token processing with work-list for cascading reductions
            let mut next_heads: Vec<u32> = Vec::new();
            let mut seen: Vec<bool> = Vec::new();

            let mut work: Vec<u32> = gss.heads.clone();
            // Track enqueued nodes to prevent infinite GOTO-merge cascades.
            let mut enqueued: Vec<bool> = Vec::new();
            for &w in &work {
                let i = w as usize;
                if i >= enqueued.len() {
                    enqueued.resize(i + 1, false);
                }
                enqueued[i] = true;
            }

            while let Some(head_idx) = work.pop() {
                let head_pos = gss.nodes[head_idx as usize].position;
                let state = gss.nodes[head_idx as usize].state;
                let actions = self.grammar.parse_table.lookup(state, token.kind);

                for action in actions {
                    match action {
                        ParseTableEntry::Shift { state: target } => {
                            let leaf = tree.alloc_token(
                                token.kind,
                                token.start_byte,
                                token.end_byte,
                                (0, 0),
                                (0, 0),
                                false,
                            );
                            let node_id = gss.add_node(*target, token.end_byte);
                            gss.add_edge(node_id, head_idx, leaf);
                            if !mark_seen(&mut seen, node_id) {
                                next_heads.push(node_id);
                            }
                        }
                        ParseTableEntry::Reduce {
                            symbol,
                            child_count,
                            ..
                        } => {
                            if *child_count == 0 {
                                continue;
                            }
                            let paths = gss.ancestor_paths(head_idx, *child_count as u32);
                            for (ancestor, children) in &paths {
                                let internal = tree.alloc_internal(
                                    *symbol,
                                    children.as_slice(),
                                    token.start_byte,
                                    token.end_byte,
                                    (0, 0),
                                    (0, 0),
                                    true,
                                );
                                let anc_state = gss.nodes[*ancestor as usize].state;
                                let goto_actions =
                                    self.grammar.parse_table.lookup(anc_state, *symbol);
                                let mut goto_target: Option<StateId> = None;
                                for ga in goto_actions {
                                    if let ParseTableEntry::Goto { state: s } = ga {
                                        goto_target = Some(*s);
                                        break;
                                    }
                                }
                                if let Some(goto_state) = goto_target {
                                    let node_id = gss.add_node(goto_state, head_pos);
                                    gss.add_edge(node_id, *ancestor, internal);
                                    let n = node_id as usize;
                                    if n >= enqueued.len() {
                                        enqueued.resize(n + 1, false);
                                    }
                                    if !enqueued[n] {
                                        enqueued[n] = true;
                                        work.push(node_id);
                                    }
                                }
                            }
                        }
                        ParseTableEntry::Accept => {}
                        ParseTableEntry::Error | ParseTableEntry::Goto { .. } => {}
                    }
                }
            }

            // ε-rule fixed-point loop (RNGLR)
            let mut changed = true;
            while changed {
                changed = false;
                let heads_ep: Vec<u32> = next_heads.clone();
                for &head_idx in &heads_ep {
                    let head_pos = gss.nodes[head_idx as usize].position;
                    let state = gss.nodes[head_idx as usize].state;
                    let actions = self.grammar.parse_table.lookup(state, token.kind);
                    for action in actions {
                        if let ParseTableEntry::Reduce {
                            symbol,
                            child_count: 0,
                            production_id,
                            ..
                        } = action
                        {
                            let key = (state, *production_id);
                            if epsilon_log.contains(&key) {
                                continue;
                            }
                            epsilon_log.push(key);
                            changed = true;

                            let internal = tree.alloc_internal(
                                *symbol,
                                &[],
                                head_pos,
                                head_pos,
                                (0, 0),
                                (0, 0),
                                true,
                            );
                            let goto_actions = self.grammar.parse_table.lookup(state, *symbol);
                            let mut goto_target: Option<StateId> = None;
                            for ga in goto_actions {
                                if let ParseTableEntry::Goto { state: s } = ga {
                                    goto_target = Some(*s);
                                    break;
                                }
                            }
                            if let Some(goto_state) = goto_target {
                                let node_id = gss.add_node(goto_state, head_pos);
                                gss.add_edge(node_id, head_idx, internal);
                                if !mark_seen(&mut seen, node_id) {
                                    next_heads.push(node_id);
                                }
                            }
                        }
                    }
                }
            }

            if next_heads.is_empty() {
                // All heads dead — error recovery
                tree.alloc_token(
                    SymbolId::ERROR,
                    error_start,
                    token.end_byte,
                    (0, 0),
                    (0, 0),
                    true,
                );
                error_start = token.end_byte;

                let mut recovered = false;
                while let Some(skip) = lexer.next_token(valid) {
                    error_start = skip.end_byte;
                    for &head_idx in &gss.heads {
                        let state = gss.nodes[head_idx as usize].state;
                        let actions = self.grammar.parse_table.lookup(state, skip.kind);
                        for action in actions {
                            match action {
                                ParseTableEntry::Shift { .. }
                                | ParseTableEntry::Reduce { .. }
                                | ParseTableEntry::Accept => {
                                    recovered = true;
                                }
                                _ => {}
                            }
                        }
                        if recovered {
                            break;
                        }
                    }
                    if recovered {
                        break;
                    }
                }
                if !recovered {
                    break;
                }
            } else {
                gss.heads = next_heads;
            }
        }

        // End-of-input reduction phase
        let mut eof_work: Vec<u32> = gss.heads.clone();
        let mut eof_epsilon: Vec<(StateId, u16)> = Vec::new();
        while let Some(head_idx) = eof_work.pop() {
            let head_pos = gss.nodes[head_idx as usize].position;
            let state = gss.nodes[head_idx as usize].state;
            for sym in 0..self.grammar.symbol_count {
                let actions = self.grammar.parse_table.lookup(state, SymbolId(sym));
                for action in actions {
                    match action {
                        ParseTableEntry::Accept => {
                            return tree.freeze();
                        }
                        ParseTableEntry::Reduce {
                            symbol,
                            child_count,
                            production_id,
                            ..
                        } => {
                            if *child_count == 0 {
                                let key = (state, *production_id);
                                if eof_epsilon.contains(&key) {
                                    continue;
                                }
                                eof_epsilon.push(key);
                                let internal = tree.alloc_internal(
                                    *symbol,
                                    &[],
                                    head_pos,
                                    head_pos,
                                    (0, 0),
                                    (0, 0),
                                    true,
                                );
                                let goto_actions = self.grammar.parse_table.lookup(state, *symbol);
                                if let Some(ParseTableEntry::Goto { state: gs }) = goto_actions
                                    .iter()
                                    .find(|a| matches!(a, ParseTableEntry::Goto { .. }))
                                {
                                    let nid = gss.add_node(*gs, head_pos);
                                    gss.add_edge(nid, head_idx, internal);
                                    eof_work.push(nid);
                                }
                            } else {
                                let paths = gss.ancestor_paths(head_idx, *child_count as u32);
                                for (ancestor, children) in &paths {
                                    let internal = tree.alloc_internal(
                                        *symbol,
                                        children.as_slice(),
                                        head_pos,
                                        head_pos,
                                        (0, 0),
                                        (0, 0),
                                        true,
                                    );
                                    let anc_state = gss.nodes[*ancestor as usize].state;
                                    let goto_actions =
                                        self.grammar.parse_table.lookup(anc_state, *symbol);
                                    if let Some(ParseTableEntry::Goto { state: gs }) = goto_actions
                                        .iter()
                                        .find(|a| matches!(a, ParseTableEntry::Goto { .. }))
                                    {
                                        let nid = gss.add_node(*gs, head_pos);
                                        gss.add_edge(nid, *ancestor, internal);
                                        eof_work.push(nid);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
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
