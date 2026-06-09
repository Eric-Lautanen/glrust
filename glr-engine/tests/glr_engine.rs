mod common;

use common::{TestGrammarBuilder, TestLexer};
use glr_engine::Parser;

/// Simple arithmetic: E → E + E | int
#[test]
#[ignore = "Phase 0.3 — GLR engine parse loop not yet complete"]
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
    let _tree = parser.parse_with_lexer(src, &mut lexer);
}

/// Dangling else: S → if E then S | if E then S else S
#[test]
#[ignore = "Phase 0.3 — GLR engine parse loop not yet complete"]
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
        (if_t, b"if"), (e_t, b"a"), (then_t, b"then"),
        (if_t, b"if"), (e_t, b"b"), (then_t, b"then"),
        (e_t, b"c"), (else_t, b"else"), (e_t, b"d"),
    ];
    let mut lexer = TestLexer::new(src, token_map);
    let _tree = parser.parse_with_lexer(src, &mut lexer);
}

/// Empty production: S → A | ε ; A → "a"
#[test]
#[ignore = "Phase 0.3 — GLR engine parse loop not yet complete"]
fn empty_production() {
    let mut g = TestGrammarBuilder::new();
    let a_t = g.terminal("a");
    let a_nt = g.nonterminal("A");
    let s_nt = g.nonterminal("S");
    g.production(s_nt, vec![a_nt], 0);
    g.production(s_nt, vec![], 0);
    g.production(a_nt, vec![a_t], 0);

    let grammar = g.build();
    let parser = Parser::new(grammar);
    let src = b"";
    let mut lexer = TestLexer::new(src, vec![]);
    let _tree = parser.parse_with_lexer(src, &mut lexer);
}

/// Long chain: S → A+ ; A → "a"
#[test]
#[ignore = "Phase 0.3 — GLR engine parse loop not yet complete"]
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
    let _tree = parser.parse_with_lexer(src, &mut lexer);
}

/// Ambiguous expression: E → E + E | E * E | int
#[test]
#[ignore = "Phase 0.3 — GLR engine parse loop not yet complete"]
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
    let _tree = parser.parse_with_lexer(src, &mut lexer);
}

/// Conflicted production: S → "a" S | "a"
#[test]
#[ignore = "Phase 0.3 — GLR engine parse loop not yet complete"]
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
    let _tree = parser.parse_with_lexer(src, &mut lexer);
}
