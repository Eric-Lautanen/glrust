use alloc::collections::BTreeSet;
use alloc::vec::Vec;
use glr_core::parse_table::ParseTableEntry;
use glr_core::tree::MutableTree;
use glr_core::{Grammar, InputEdit, StateId, SymbolId, Tree};
use glr_lexer::Lexer;

use crate::gss::Gss;

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
            // Per-token dedup sets. BTreeSet is no_std-safe (alloc only).
            // `seen`     — shift targets already added to next_heads
            // `enqueued` — reduction work-list guard against GOTO-merge cascades
            // `epsilon_log` — (state, production_id) pairs already processed this
            //                 token; prevents unbounded ε fixed-point loops.
            let mut epsilon_log: BTreeSet<(u16, u16)> = BTreeSet::new();

            // Token processing with work-list for cascading reductions
            let mut next_heads: Vec<u32> = Vec::new();
            let mut seen: BTreeSet<u32> = BTreeSet::new();

            let mut work: Vec<u32> = gss.heads.clone();
            // Seed the enqueued guard with the initial work items.
            let mut enqueued: BTreeSet<u32> = work.iter().copied().collect();

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
                            let (node_id, _) = gss.find_or_create_node(*target, token.end_byte);
                            gss.add_edge(node_id, head_idx, leaf);
                            // Only push to next_heads once per distinct shift target.
                            if seen.insert(node_id) {
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
                                    let (node_id, _) =
                                        gss.find_or_create_node(goto_state, head_pos);
                                    gss.add_edge(node_id, *ancestor, internal);
                                    if enqueued.insert(node_id) {
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

            // ε-rule fixed-point loop (RNGLR).
            // epsilon_log keys on (state.0, production_id) — same set as above,
            // already populated by any ε-reductions encountered in the work loop.
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
                            // Key is (state index, production_id) — O(log n) lookup.
                            let key = (state.0 as u16, *production_id);
                            if !epsilon_log.insert(key) {
                                continue;
                            }
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
                                let (node_id, _) = gss.find_or_create_node(goto_state, head_pos);
                                gss.add_edge(node_id, head_idx, internal);
                                if seen.insert(node_id) {
                                    next_heads.push(node_id);
                                }
                            }
                        }
                    }
                }
            }

            if next_heads.is_empty() {
                // All heads dead — panic-mode error recovery.
                // Emit an error node spanning from the last recovery point to
                // the end of the bad token.
                tree.alloc_token(
                    SymbolId::ERROR,
                    error_start,
                    token.end_byte,
                    (0, 0),
                    (0, 0),
                    true,
                );
                error_start = token.end_byte;

                // Skip tokens until at least one current head can shift or
                // reduce on the skipped token. When we find such a token we
                // leave it to be re-processed on the next outer loop iteration
                // by *not* consuming it here; instead we just update gss.heads
                // to the surviving heads and let the main loop handle it.
                //
                // Surviving heads are those that have a non-Error action for
                // the recovery token — we collect them into `recovered_heads`
                // so gss.heads is consistent before the next iteration.
                let mut recovered = false;
                while let Some(skip) = lexer.next_token(valid) {
                    error_start = skip.end_byte;
                    let mut recovered_heads: Vec<u32> = Vec::new();
                    let mut rh_seen: BTreeSet<u32> = BTreeSet::new();
                    for &head_idx in &gss.heads {
                        let state = gss.nodes[head_idx as usize].state;
                        let actions = self.grammar.parse_table.lookup(state, skip.kind);
                        let can_continue = actions.iter().any(|a| {
                            matches!(
                                a,
                                ParseTableEntry::Shift { .. }
                                    | ParseTableEntry::Reduce { .. }
                                    | ParseTableEntry::Accept
                            )
                        });
                        if can_continue && rh_seen.insert(head_idx) {
                            recovered_heads.push(head_idx);
                        }
                    }
                    if !recovered_heads.is_empty() {
                        gss.heads = recovered_heads;
                        recovered = true;
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
        let mut eof_enqueued: BTreeSet<u32> = eof_work.iter().copied().collect();
        // Same (state.0, production_id) keying as the per-token epsilon_log.
        let mut eof_epsilon: BTreeSet<(u16, u16)> = BTreeSet::new();
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
                                let key = (state.0 as u16, *production_id);
                                if !eof_epsilon.insert(key) {
                                    continue;
                                }
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
                                    let (nid, _) = gss.find_or_create_node(*gs, head_pos);
                                    gss.add_edge(nid, head_idx, internal);
                                    if eof_enqueued.insert(nid) {
                                        eof_work.push(nid);
                                    }
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
                                        let (nid, _) = gss.find_or_create_node(*gs, head_pos);
                                        gss.add_edge(nid, *ancestor, internal);
                                        if eof_enqueued.insert(nid) {
                                            eof_work.push(nid);
                                        }
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
        todo!("provide a lexer via parse_with_lexer, or implement a default lexer here")
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
