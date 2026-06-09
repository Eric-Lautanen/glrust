use alloc::boxed::Box;
use alloc::collections::BTreeSet;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use glr_core::parse_table::ParseTableEntry;
use glr_core::position_at_with_lines;
use glr_core::tree::MutableTree;
use glr_core::{Grammar, InputEdit, ProductionId, StateId, SymbolId, TextSpan, Tree};
use glr_lexer::Lexer;

use crate::gss::Gss;

fn eps_key(state: StateId, production_id: u16) -> (u32, u16) {
    (state.0, production_id)
}

/// Per-token mutable state shared between work-list processing and the
/// epsilon fixed-point loop.
struct TokenState {
    epsilon_log: BTreeSet<(u32, u16)>,
    next_heads: Vec<u32>,
    seen: BTreeSet<u32>,
    enqueued: BTreeSet<u32>,
    work: Vec<u32>,
}

fn find_goto(actions: &[ParseTableEntry]) -> Option<StateId> {
    actions.iter().find_map(|a| {
        if let ParseTableEntry::Goto { state } = a {
            Some(*state)
        } else {
            None
        }
    })
}

fn apply_production_metadata(
    tree: &mut MutableTree,
    production_id: ProductionId,
    children: &[u32],
    grammar: &Grammar,
) {
    if let Some(prod) = grammar.production(production_id) {
        for &(child_idx, field_id) in &prod.field_map {
            if let Some(&child_id) = children.get(child_idx as usize) {
                if let Some(child) = tree.nodes.get_mut(child_id as usize) {
                    child.field_id = Some(field_id);
                }
            }
        }
        for &(child_idx, alias_symbol, is_named) in &prod.alias_map {
            if let Some(&child_id) = children.get(child_idx as usize) {
                if let Some(child) = tree.nodes.get_mut(child_id as usize) {
                    child.kind = alias_symbol;
                    if is_named {
                        child.flags.set_named(true);
                    }
                }
            }
        }
    }
}

fn alloc_epsilon_node(
    tree: &mut MutableTree,
    symbol: SymbolId,
    head_pos: u32,
    head_pos_rowcol: (u32, u32),
) -> u32 {
    tree.alloc_internal(
        symbol,
        &[],
        TextSpan {
            start_byte: head_pos,
            end_byte: head_pos,
            start_position: head_pos_rowcol,
            end_position: head_pos_rowcol,
        },
        true,
    )
}

/// Shared mutable state reported to the progress callback during parsing.
pub struct ParseState {
    /// Byte offset of the current token being processed.
    pub current_token: u32,
    /// Total length of the source text in bytes.
    pub source_len: u32,
    /// Number of GLR operations performed so far.
    pub operation_count: u64,
}

/// Callback invoked periodically during parsing. Return `true` to continue,
/// `false` to abort.
pub type ProgressCallback = Box<dyn Fn(&ParseState) -> bool>;

/// Cost-based score for an error recovery candidate.
struct ErrorRecoveryCost {
    total_cost: u64,
}

struct ErrorRecoveryCtx<'a, 'b> {
    source: &'a [u8],
    gss: &'a mut Gss,
    tree: &'a mut MutableTree,
    error_start: &'a mut u32,
    line_starts: &'a [u32],
    token: &'b glr_lexer::Token,
}

/// The GLR parser using the RNGLR algorithm.
pub struct Parser {
    grammar: Grammar,
    op_count: AtomicU64,
    progress_callback: Option<ProgressCallback>,
}

impl core::fmt::Debug for Parser {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Parser")
            .field("grammar", &self.grammar)
            .finish_non_exhaustive()
    }
}

impl Parser {
    #[must_use]
    pub fn new(grammar: Grammar) -> Self {
        Self {
            grammar,
            op_count: AtomicU64::new(0),
            progress_callback: None,
        }
    }

    /// Set a progress callback that is called periodically during parsing.
    /// The callback receives the current [`ParseState`] and should return
    /// `true` to continue or `false` to abort.
    pub fn set_progress_callback(&mut self, cb: ProgressCallback) {
        self.progress_callback = Some(cb);
    }

    #[must_use]
    pub fn grammar(&self) -> &Grammar {
        &self.grammar
    }

    /// Perform an epsilon (child_count == 0) reduction and GOTO.
    ///
    /// Creates an epsilon tree node, looks up the GOTO for `state` on `symbol`,
    /// creates/retrieves the GSS node at `(goto_state, head_pos)`, and adds a
    /// GSS edge from the new node to `head_idx`. Returns the new GSS node id
    /// (or `None` if there is no GOTO entry).
    fn reduce_epsilon_goto(
        &self,
        tree: &mut MutableTree,
        gss: &mut Gss,
        state: StateId,
        head_pos_rowcol: (u32, u32),
        head_idx: u32,
        symbol: SymbolId,
    ) -> Option<u32> {
        let head_pos = gss.nodes[head_idx as usize].position;
        let internal = alloc_epsilon_node(tree, symbol, head_pos, head_pos_rowcol);
        let goto_actions = self.grammar.parse_table.lookup(state, symbol);
        let gs = find_goto(goto_actions)?;
        let (nid, _) = gss.find_or_create_node(gs, head_pos);
        gss.add_edge(nid, head_idx, internal);
        Some(nid)
    }

    /// Perform a non-epsilon reduction and GOTO.
    ///
    /// Walks GSS ancestor edges, allocates the reduced tree node, applies
    /// field/alias metadata, looks up GOTO, creates the result GSS node, and
    /// wires the GSS edge. Returns `Vec` of `(new_gss_node_id, ancestor_idx)`
    /// for the caller to manage the worklist.
    fn reduce_non_epsilon_goto(
        &self,
        tree: &mut MutableTree,
        gss: &mut Gss,
        head_idx: u32,
        child_count: u16,
        symbol: SymbolId,
        production_id: u16,
    ) -> Vec<(u32, u32)> {
        let head_pos = gss.nodes[head_idx as usize].position;
        let paths = gss.ancestor_paths(head_idx, u32::from(child_count));
        let mut results = Vec::with_capacity(paths.len());
        for (ancestor, children) in &paths {
            let span = tree.span_for_children(children.as_slice());
            let internal = tree.alloc_internal(symbol, children.as_slice(), span, true);
            apply_production_metadata(tree, ProductionId(production_id), children, &self.grammar);
            let anc_state = gss.nodes[*ancestor as usize].state;
            let goto_actions = self.grammar.parse_table.lookup(anc_state, symbol);
            if let Some(gs) = find_goto(goto_actions) {
                let (nid, _) = gss.find_or_create_node(gs, head_pos);
                gss.add_edge(nid, *ancestor, internal);
                results.push((nid, *ancestor));
            }
        }
        results
    }

    fn fill_valid_symbols(&self, heads: &[u32], gss: &Gss, buf: &mut [bool]) {
        buf.fill(false);
        for &h in heads {
            if let Some(node) = gss.nodes.get(h as usize) {
                self.grammar.fill_valid_symbols(node.state, buf);
            }
        }
    }

    fn try_error_recovery_impl<L: Lexer>(
        &self,
        ctx: &mut ErrorRecoveryCtx,
        lexer: &mut L,
    ) -> Option<Vec<u32>> {
        let mut error_children: Vec<u32> = Vec::new();
        let err_start_pos = position_at_with_lines(ctx.source, *ctx.error_start, ctx.line_starts);
        error_children.push(ctx.tree.alloc_token(
            SymbolId::ERROR,
            *ctx.error_start,
            ctx.token.end_byte,
            err_start_pos,
            ctx.token.end_position,
            true,
        ));
        *ctx.error_start = ctx.token.end_byte;

        let mut best_cost: Option<ErrorRecoveryCost> = None;
        let mut best_heads: Option<Vec<u32>> = None;
        let mut best_error_node: Option<u32> = None;
        let mut best_skip_byte: u32 = 0;

        let mut skipped = 0u32;
        while let Some(skip) = lexer.next_token(&[]) {
            skipped += 1;
            let mut recovered_heads: Vec<u32> = Vec::new();
            let mut rh_seen: BTreeSet<u32> = BTreeSet::new();
            for &head_idx in &ctx.gss.heads {
                let state = ctx.gss.nodes[head_idx as usize].state;
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
                let err_span = ctx.tree.span_for_children(&error_children);
                let error_node_id =
                    ctx.tree.alloc_internal(SymbolId::ERROR, &error_children, err_span, true);
                let mut error_heads: Vec<u32> = Vec::new();
                let mut err_seen: BTreeSet<u32> = BTreeSet::new();
                for &head_idx in &recovered_heads {
                    let head_state = ctx.gss.nodes[head_idx as usize].state;
                    let (nid, _) = ctx.gss.find_or_create_node(head_state, skip.start_byte);
                    ctx.gss.add_edge(nid, head_idx, error_node_id);
                    if err_seen.insert(nid) {
                        error_heads.push(nid);
                    }
                }
                let cost = ErrorRecoveryCost {
                    total_cost: u64::from(skipped) * 10,
                };
                let is_better = best_cost
                    .as_ref()
                    .is_none_or(|c| cost.total_cost < c.total_cost);
                if is_better {
                    best_cost = Some(cost);
                    best_heads = Some(error_heads);
                    best_error_node = Some(error_node_id);
                    best_skip_byte = skip.start_byte;
                }
            }
            error_children.push(ctx.tree.alloc_token(
                SymbolId::ERROR,
                skip.start_byte,
                skip.end_byte,
                skip.start_position,
                skip.end_position,
                true,
            ));
            *ctx.error_start = skip.end_byte;
        }

        if let (Some(heads), Some(_), skip_byte) = (best_heads, best_error_node, best_skip_byte) {
            lexer.reset_to(skip_byte);
            return Some(heads);
        }
        None
    }

    /// Run end-of-input reduction phase.
    fn eof_reduction(
        &self,
        source: &[u8],
        gss: &mut Gss,
        tree: &mut MutableTree,
        line_starts: &[u32],
    ) -> Tree {
        let mut work: Vec<u32> = gss.heads.clone();
        let mut enqueued: BTreeSet<u32> = work.iter().copied().collect();
        let mut epsilon: BTreeSet<(u32, u16)> = BTreeSet::new();
        while let Some(head_idx) = work.pop() {
            let head_pos = gss.nodes[head_idx as usize].position;
            let state = gss.nodes[head_idx as usize].state;
            for sym in 0..self.grammar.symbol_count {
                let actions = self.grammar.parse_table.lookup(state, SymbolId(sym));
                for action in actions {
                    match action {
                        ParseTableEntry::Accept => return tree.freeze(),
                        ParseTableEntry::Reduce {
                            symbol,
                            child_count,
                            production_id,
                            ..
                        } => {
                            if *child_count == 0 {
                                let key = eps_key(state, *production_id);
                                if !epsilon.insert(key) {
                                    continue;
                                }
                                let hp = position_at_with_lines(source, head_pos, line_starts);
                                if let Some(nid) = self
                                    .reduce_epsilon_goto(tree, gss, state, hp, head_idx, *symbol)
                                {
                                    if enqueued.insert(nid) {
                                        work.push(nid);
                                    }
                                }
                            } else {
                                for (nid, _) in self.reduce_non_epsilon_goto(
                                    tree,
                                    gss,
                                    head_idx,
                                    *child_count,
                                    *symbol,
                                    *production_id,
                                ) {
                                    if enqueued.insert(nid) {
                                        work.push(nid);
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

    fn epsilon_fixed_point(
        &self,
        source: &[u8],
        token: &glr_lexer::Token,
        gss: &mut Gss,
        tree: &mut MutableTree,
        ts: &mut TokenState,
        line_starts: &[u32],
    ) {
        let mut changed = true;
        while changed {
            changed = false;
            let len = ts.next_heads.len();
            for i in 0..len {
                let head_idx = ts.next_heads[i];
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
                        let key = eps_key(state, *production_id);
                        if !ts.epsilon_log.insert(key) {
                            continue;
                        }
                        changed = true;
                        let hp = position_at_with_lines(source, head_pos, line_starts);
                        if let Some(node_id) =
                            self.reduce_epsilon_goto(tree, gss, state, hp, head_idx, *symbol)
                        {
                            if ts.seen.insert(node_id) {
                                ts.next_heads.push(node_id);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Process a single token through the work-list and epsilon fixed-point
    /// loops. Returns the set of next GSS heads (non-empty) or `None` if all
    /// heads are dead (caller should attempt error recovery).
    fn process_token<L: Lexer>(
        &self,
        source: &[u8],
        token: &glr_lexer::Token,
        gss: &mut Gss,
        tree: &mut MutableTree,
        lexer: &mut L,
        line_starts: &[u32],
    ) -> Option<Vec<u32>> {
        let mut ts = TokenState {
            epsilon_log: BTreeSet::new(),
            next_heads: Vec::new(),
            seen: BTreeSet::new(),
            enqueued: gss.heads.iter().copied().collect(),
            work: gss.heads.clone(),
        };

        while let Some(head_idx) = ts.work.pop() {
            let head_pos = gss.nodes[head_idx as usize].position;
            let state = gss.nodes[head_idx as usize].state;
            let actions = self.grammar.parse_table.lookup(state, token.kind);

            for action in actions {
                match action {
                    ParseTableEntry::Shift { state: target } => {
                        let mut scanner_buf = [0u8; 1024];
                        let n = lexer.serialize_state(&mut scanner_buf);
                        let leaf = tree.alloc_token(
                            token.kind,
                            token.start_byte,
                            token.end_byte,
                            token.start_position,
                            token.end_position,
                            false,
                        );
                        let (node_id, is_new) = gss.find_or_create_node(*target, token.end_byte);
                        if is_new && n > 0 {
                            if let Some(node) = gss.nodes.get_mut(node_id as usize) {
                                node.scanner_state = scanner_buf[..n].to_vec();
                            }
                        }
                        gss.add_edge(node_id, head_idx, leaf);
                        if ts.seen.insert(node_id) {
                            ts.next_heads.push(node_id);
                        }
                    }
                    ParseTableEntry::Reduce {
                        symbol,
                        child_count,
                        production_id,
                        ..
                    } => {
                        if *child_count == 0 {
                            let key = eps_key(state, *production_id);
                            if !ts.epsilon_log.insert(key) {
                                continue;
                            }
                            let hp = position_at_with_lines(source, head_pos, line_starts);
                            if let Some(node_id) =
                                self.reduce_epsilon_goto(tree, gss, state, hp, head_idx, *symbol)
                            {
                                if ts.enqueued.insert(node_id) {
                                    ts.work.push(node_id);
                                }
                            }
                        } else {
                            for (node_id, _) in self.reduce_non_epsilon_goto(
                                tree,
                                gss,
                                head_idx,
                                *child_count,
                                *symbol,
                                *production_id,
                            ) {
                                if ts.enqueued.insert(node_id) {
                                    ts.work.push(node_id);
                                }
                            }
                        }
                    }
                    ParseTableEntry::Accept
                    | ParseTableEntry::Error
                    | ParseTableEntry::Goto { .. } => {}
                }
            }
        }

        self.epsilon_fixed_point(source, token, gss, tree, &mut ts, line_starts);

        if ts.next_heads.is_empty() {
            None
        } else {
            Some(ts.next_heads)
        }
    }

    /// Parse source bytes using a provided lexer.
    pub fn parse_with_lexer<L: Lexer>(&self, source: &[u8], lexer: &mut L) -> Tree {
        let mut gss = Gss::new(StateId(0));
        let mut tree = MutableTree::new();
        let mut error_start = 0u32;
        let mut valid_buf = alloc::vec![false; self.grammar.symbol_count as usize];
        let source_len = source.len() as u32;
        let line_starts = Grammar::compute_line_starts(source);

        loop {
            let oc = self.op_count.fetch_add(1, Ordering::Relaxed) + 1;
            if oc.is_multiple_of(100) {
                if let Some(ref cb) = self.progress_callback {
                    let state = ParseState {
                        current_token: lexer.cursor(),
                        source_len,
                        operation_count: oc,
                    };
                    if !cb(&state) {
                        break;
                    }
                }
            }

            self.fill_valid_symbols(&gss.heads, &gss, &mut valid_buf);
            let Some(token) = lexer.next_token(&valid_buf) else {
                break;
            };
            if token.start_byte >= token.end_byte {
                lexer.reset_to(token.start_byte.saturating_add(1));
                continue;
            }

            let Some(next_heads) =
                self.process_token(source, &token, &mut gss, &mut tree, lexer, &line_starts)
            else {
                let mut recovery_ctx = ErrorRecoveryCtx {
                    source,
                    gss: &mut gss,
                    tree: &mut tree,
                    error_start: &mut error_start,
                    line_starts: &line_starts,
                    token: &token,
                };
                let Some(heads) = self.try_error_recovery_impl(
                    &mut recovery_ctx,
                    lexer,
                ) else {
                    break;
                };
                gss.heads = heads;
                continue;
            };
            gss.heads = next_heads;
        }

        self.eof_reduction(source, &mut gss, &mut tree, &line_starts)
    }

    #[must_use]
    pub fn parse(&self, source: &[u8]) -> Tree {
        let dfa = &self.grammar.dfa_table;
        let mut lexer = glr_lexer::BuiltinLexer::new(source, dfa);
        self.parse_with_lexer(source, &mut lexer)
    }

    /// Walk the old tree and mark all nodes that overlap the edit range.
    fn mark_changed_nodes(old_tree: &mut Tree, edit: &InputEdit) {
        old_tree.mark_edit_range(edit.start_byte, edit.old_end_byte);
    }

    /// Incrementally re-parse after an edit.
    pub fn parse_incremental_with_lexer<L: Lexer>(
        &self,
        old_tree: &Tree,
        edit: &InputEdit,
        source: &[u8],
        lexer: &mut L,
    ) -> Tree {
        let mut old_tree_clone = old_tree.clone();
        Self::mark_changed_nodes(&mut old_tree_clone, edit);

        lexer.reset_to(edit.start_byte);

        let mut gss = Gss::new(StateId(0));
        let mut tree = MutableTree::new();
        let mut error_start = edit.start_byte;
        let mut valid_buf = alloc::vec![false; self.grammar.symbol_count as usize];
        let line_starts = Grammar::compute_line_starts(source);

        let old_nodes = old_tree.nodes();
        let old_root = old_tree.root_id();

        let mut prev_byte = 0u32;

        if let Some(root_id) = old_root {
            if let Some(root) = old_nodes.get(root_id) {
                Self::reuse_prefix_nodes(
                    root,
                    edit.start_byte,
                    old_tree,
                    &mut gss,
                    &mut tree,
                    &mut prev_byte,
                );
            }
        }

        gss.heads.dedup();

        loop {
            self.fill_valid_symbols(&gss.heads, &gss, &mut valid_buf);
            let Some(token) = lexer.next_token(&valid_buf) else {
                break;
            };
            if token.start_byte >= token.end_byte {
                lexer.reset_to(token.start_byte.saturating_add(1));
                continue;
            }

            let Some(next_heads) =
                self.process_token(source, &token, &mut gss, &mut tree, lexer, &line_starts)
            else {
                let mut recovery_ctx = ErrorRecoveryCtx {
                    source,
                    gss: &mut gss,
                    tree: &mut tree,
                    error_start: &mut error_start,
                    line_starts: &line_starts,
                    token: &token,
                };
                let Some(heads) = self.try_error_recovery_impl(
                    &mut recovery_ctx,
                    lexer,
                ) else {
                    break;
                };
                gss.heads = heads;
                continue;
            };
            gss.heads = next_heads;

            if token.end_byte >= edit.new_end_byte {
                if let Some(old_root_id) = old_root {
                    if let Some(matched) = self.try_reuse_suffix(
                        old_tree,
                        old_root_id,
                        token.end_byte,
                        &mut gss,
                        &mut tree,
                    ) {
                        if matched {
                            break;
                        }
                    }
                }
            }
        }

        self.eof_reduction(source, &mut gss, &mut tree, &line_starts)
    }

    fn reuse_prefix_nodes(
        node: &glr_core::Node,
        edit_start: u32,
        old_tree: &Tree,
        _gss: &mut Gss,
        _tree: &mut MutableTree,
        prev_byte: &mut u32,
    ) {
        if node.end_byte <= edit_start && !node.flags.has_changes() {
            if node.start_byte >= *prev_byte {
                *prev_byte = node.end_byte;
            }
            return;
        }
        if node.flags.has_changes() || (node.start_byte <= edit_start && node.end_byte > edit_start)
        {
            for &child_id in &node.children {
                if let Some(child) = old_tree.node_by_id(child_id) {
                    Self::reuse_prefix_nodes(child, edit_start, old_tree, _gss, _tree, prev_byte);
                }
            }
        }
    }

    fn try_reuse_suffix(
        &self,
        _old_tree: &Tree,
        _root_id: usize,
        _current_end: u32,
        _gss: &mut Gss,
        _tree: &mut MutableTree,
    ) -> Option<bool> {
        None
    }

    #[must_use]
    pub fn parse_incremental(&self, old_tree: &Tree, edit: &InputEdit, source: &[u8]) -> Tree {
        let dfa = &self.grammar.dfa_table;
        let mut lexer = glr_lexer::BuiltinLexer::new(source, dfa);
        self.parse_incremental_with_lexer(old_tree, edit, source, &mut lexer)
    }
}
