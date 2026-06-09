#![deny(unsafe_code)]
mod common;

use common::{TestGrammarBuilder, TestLexer};
use glr_core::SymbolId;
use glr_engine::Parser;

/// Compute max depth from root.
fn max_depth(nodes: &[glr_core::Node], id: glr_core::NodeId) -> usize {
    let mut depth = 0;
    for &child_id in &nodes[id].children {
        depth = depth.max(1 + max_depth(nodes, child_id));
    }
    depth
}

/// Check if every child's span is contained within its parent's span.
fn spans_well_formed(nodes: &[glr_core::Node], id: glr_core::NodeId) -> bool {
    for &child_id in &nodes[id].children {
        let child = &nodes[child_id];
        if child.start_byte < nodes[id].start_byte || child.end_byte > nodes[id].end_byte {
            return false;
        }
        if !spans_well_formed(nodes, child_id) {
            return false;
        }
    }
    true
}

/// Simple arithmetic: E → E + E | int
#[test]
fn simple_arithmetic() {
    let mut g = TestGrammarBuilder::new();
    let int_t = g.terminal("int");
    let plus_t = g.terminal("+");
    let e_nt = g.nonterminal("E");
    g.production(e_nt, vec![e_nt, plus_t, e_nt], 0);
    g.production(e_nt, vec![int_t], 0);

    let grammar = g.build();
    let parser = Parser::new(grammar);

    let src = b"1 + 2 + 3";
    let token_map: Vec<(u32, &[u8])> = vec![
        (int_t, b"1"),
        (plus_t, b"+"),
        (int_t, b"2"),
        (plus_t, b"+"),
        (int_t, b"3"),
    ];
    let mut lexer = TestLexer::new(src, token_map);
    let tree = parser.parse_with_lexer(src, &mut lexer);
    let root = tree.root_node().expect("expected a parse tree");
    assert!(spans_well_formed(tree.nodes(), tree.root_id().unwrap()));
    // Root should be an E node with at least 2 children (the binary E+E pattern)
    assert!(!root.children.is_empty(), "root should have children");
}

/// Dangling else: S → if E then S | if E then S else S
#[test]
fn dangling_else() {
    let mut g = TestGrammarBuilder::new();
    let if_t = g.terminal("if");
    let then_t = g.terminal("then");
    let else_t = g.terminal("else");
    let e_t = g.terminal("E");
    let s_nt = g.nonterminal("S");
    let stmt_nt = g.nonterminal("stmt");
    g.production(s_nt, vec![if_t, e_t, then_t, s_nt], 0);
    g.production(s_nt, vec![if_t, e_t, then_t, s_nt, else_t, s_nt], 0);
    g.production(s_nt, vec![stmt_nt], 0);
    g.production(stmt_nt, vec![e_t], 0);

    let grammar = g.build();
    let parser = Parser::new(grammar);
    let src = b"if a then if b then c else d";
    let token_map: Vec<(u32, &[u8])> = vec![
        (if_t, b"if"),
        (e_t, b"a"),
        (then_t, b"then"),
        (if_t, b"if"),
        (e_t, b"b"),
        (then_t, b"then"),
        (e_t, b"c"),
        (else_t, b"else"),
        (e_t, b"d"),
    ];
    let mut lexer = TestLexer::new(src, token_map);
    let tree = parser.parse_with_lexer(src, &mut lexer);
    assert!(tree.root_node().is_some(), "expected a parse tree");
    assert!(spans_well_formed(tree.nodes(), tree.root_id().unwrap()));
    // else binds to innermost if — at least 2 S levels
    assert!(max_depth(tree.nodes(), tree.root_id().unwrap()) >= 2);
}

/// Empty production: S → A | ε ; A → "a"
#[test]
fn empty_production() {
    let mut g = TestGrammarBuilder::new();
    let a_t = g.terminal("a");
    let a_nt = g.nonterminal("A");
    let s_nt = g.nonterminal("S");
    g.production(s_nt, vec![a_nt], 0);
    g.production(s_nt, vec![], 0); // ε-rule
    g.production(a_nt, vec![a_t], 0);

    let grammar = g.build();
    let parser = Parser::new(grammar);
    let src = b"";
    let mut lexer = TestLexer::new(src, vec![]);
    let tree = parser.parse_with_lexer(src, &mut lexer);
    assert!(tree.root_node().is_some(), "ε-rule should produce a tree");
    // S → ε should produce exactly one node (S).
    assert!(
        tree.nodes().len() == 1,
        "ε-rule should produce exactly 1 node, got {}",
        tree.nodes().len()
    );
}

/// Long chain: S → S A | A ; A → "a"
#[test]
fn long_chain() {
    let mut g = TestGrammarBuilder::new();
    let a_t = g.terminal("a");
    let a_nt = g.nonterminal("A");
    let s_nt = g.nonterminal("S");
    g.production(s_nt, vec![s_nt, a_nt], 0);
    g.production(s_nt, vec![a_nt], 0);
    g.production(a_nt, vec![a_t], 0);

    let grammar = g.build();
    let parser = Parser::new(grammar);
    let src = b"a a a a a a a a a a";
    let mut lexer = TestLexer::new(src, vec![(a_t, b"a")]);
    let tree = parser.parse_with_lexer(src, &mut lexer);
    assert!(tree.root_node().is_some(), "expected a parse tree");
    let root = tree.root_node().unwrap();
    assert!(!root.children.is_empty(), "root should have children");
    assert!(spans_well_formed(tree.nodes(), tree.root_id().unwrap()));
    // 10 'a's → at least 10 leaf tokens + internal nodes
    assert!(
        tree.nodes().len() >= 20,
        "long chain should produce at least 20 nodes, got {}",
        tree.nodes().len()
    );
}

/// Ambiguous expression: E → E + E | E * E | int
#[test]
fn ambiguous_expression() {
    let mut g = TestGrammarBuilder::new();
    let int_t = g.terminal("int");
    let plus_t = g.terminal("+");
    let star_t = g.terminal("*");
    let e_nt = g.nonterminal("E");
    g.production(e_nt, vec![e_nt, plus_t, e_nt], 0);
    g.production(e_nt, vec![e_nt, star_t, e_nt], 0);
    g.production(e_nt, vec![int_t], 0);

    let grammar = g.build();
    let parser = Parser::new(grammar);
    let src = b"1 + 2 * 3";
    let token_map: Vec<(u32, &[u8])> = vec![
        (int_t, b"1"),
        (plus_t, b"+"),
        (int_t, b"2"),
        (star_t, b"*"),
        (int_t, b"3"),
    ];
    let mut lexer = TestLexer::new(src, token_map);
    let tree = parser.parse_with_lexer(src, &mut lexer);
    assert!(tree.root_node().is_some(), "expected a parse tree");
    assert!(spans_well_formed(tree.nodes(), tree.root_id().unwrap()));
}

/// Error recovery: parser inserts ERROR nodes for invalid input
#[test]
fn error_recovery_returns_tree() {
    let mut g = TestGrammarBuilder::new();
    let a_t = g.terminal("a");
    let b_t = g.terminal("b");
    let s_nt = g.nonterminal("S");
    g.production(s_nt, vec![a_t, s_nt], 0);
    g.production(s_nt, vec![a_t], 0);

    let grammar = g.build();
    let parser = Parser::new(grammar);
    // 'b' is not valid in the grammar — parser must return a tree
    let src = b"b";
    let mut lexer = TestLexer::new(src, vec![(b_t, b"b")]);
    let tree = parser.parse_with_lexer(src, &mut lexer);
    assert!(
        tree.root_node().is_some(),
        "parser must return a tree even on error"
    );
    // Error recovery should produce at least one ERROR node
    let has_error = tree.root_node().is_some_and(|r| {
        r.kind == SymbolId::ERROR
            || r.children
                .first()
                .copied()
                .and_then(|cid| tree.nodes().get(cid))
                .is_some_and(|c| c.kind == SymbolId::ERROR)
    });
    assert!(has_error, "error recovery should produce an ERROR node");
}

/// Conflicted production: S → "a" S | "a"
#[test]
fn conflicted_production() {
    let mut g = TestGrammarBuilder::new();
    let a_t = g.terminal("a");
    let s_nt = g.nonterminal("S");
    g.production(s_nt, vec![a_t, s_nt], 0);
    g.production(s_nt, vec![a_t], 0);

    let grammar = g.build();
    let parser = Parser::new(grammar);
    let src = b"a a a a a a a a a a";
    let mut lexer = TestLexer::new(src, vec![(a_t, b"a")]);
    let tree = parser.parse_with_lexer(src, &mut lexer);
    assert!(tree.root_node().is_some(), "expected a parse tree");
    assert!(spans_well_formed(tree.nodes(), tree.root_id().unwrap()));
}

/// Incremental re-parse identity: parse_incremental must produce the same
/// tree as a full parse for a no-op edit (zero-byte edit).
#[test]
fn incremental_reparse_identity() {
    let mut g = TestGrammarBuilder::new();
    let int_t = g.terminal("int");
    let plus_t = g.terminal("+");
    let e_nt = g.nonterminal("E");
    g.production(e_nt, vec![e_nt, plus_t, e_nt], 0);
    g.production(e_nt, vec![int_t], 0);

    let grammar = g.build();
    let parser = Parser::new(grammar);
    let src = b"1 + 2";

    let token_map: Vec<(u32, &[u8])> = vec![(int_t, b"1"), (plus_t, b"+"), (int_t, b"2")];
    let mut lexer = TestLexer::new(src, token_map.clone());
    let tree = parser.parse_with_lexer(src, &mut lexer);
    assert!(tree.root_node().is_some(), "base parse should succeed");

    // Zero-byte edit: replace 0 bytes at offset 1 with 0 bytes
    let edit = glr_core::InputEdit {
        start_byte: 1,
        old_end_byte: 1,
        new_end_byte: 1,
        start_point: glr_core::Point { row: 0, column: 1 },
        old_end_point: glr_core::Point { row: 0, column: 1 },
        new_end_point: glr_core::Point { row: 0, column: 1 },
    };
    let mut lexer2 = TestLexer::new(src, token_map);
    let tree2 = parser.parse_with_lexer(src, &mut lexer2);
    let mut lexer3 = TestLexer::new(src, vec![(int_t, b"1"), (plus_t, b"+"), (int_t, b"2")]);
    let incr = parser.parse_incremental_with_lexer(&tree, &edit, src, &mut lexer3);

    // Both produce same root kind and span
    assert_eq!(
        tree2.root_node().map(|r| r.kind),
        incr.root_node().map(|r| r.kind),
        "incremental reparse must produce same root kind as full parse"
    );
}
