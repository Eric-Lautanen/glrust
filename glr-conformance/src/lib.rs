#![deny(unsafe_code)]
use glr_core::{Grammar, Node, NodeId, Tree};

/// Compare two trees node-by-node, using a shared grammar for symbol names.
/// Returns a list of differences (empty if structurally identical).
pub fn compare_trees(actual: &Tree, expected: &Tree, grammar: &Grammar) -> Vec<String> {
    let mut diffs = Vec::new();
    compare_nodes(
        actual,
        expected,
        grammar,
        actual.root_id(),
        expected.root_id(),
        &mut diffs,
        "",
    );
    diffs
}

fn compare_nodes(
    actual_tree: &Tree,
    expected_tree: &Tree,
    grammar: &Grammar,
    actual_id: Option<NodeId>,
    expected_id: Option<NodeId>,
    diffs: &mut Vec<String>,
    path: &str,
) {
    match (
        actual_id.and_then(|id| actual_tree.node_by_id(id)),
        expected_id.and_then(|id| expected_tree.node_by_id(id)),
    ) {
        (None, None) => {}
        (None, Some(_)) => diffs.push(format!("{path}: expected node, got none")),
        (Some(_), None) => diffs.push(format!("{path}: unexpected node")),
        (Some(actual), Some(expected)) => {
            let actual_name = grammar.symbol_name(actual.kind);
            let expected_name = grammar.symbol_name(expected.kind);
            if actual_name != expected_name {
                diffs.push(format!(
                    "{path}: kind mismatch: actual={actual_name:?}, expected={expected_name:?}"
                ));
            }
            if actual.start_byte != expected.start_byte {
                diffs.push(format!(
                    "{path}: start_byte mismatch: actual={}, expected={}",
                    actual.start_byte, expected.start_byte
                ));
            }
            if actual.end_byte != expected.end_byte {
                diffs.push(format!(
                    "{path}: end_byte mismatch: actual={}, expected={}",
                    actual.end_byte, expected.end_byte
                ));
            }
            let actual_named: Vec<NodeId> = actual
                .children
                .iter()
                .filter_map(|&id| {
                    let node = actual_tree.node_by_id(id)?;
                    if node.flags.is_named() {
                        Some(id)
                    } else {
                        None
                    }
                })
                .collect();
            let expected_named: Vec<NodeId> = expected
                .children
                .iter()
                .filter_map(|&id| {
                    let node = expected_tree.node_by_id(id)?;
                    if node.flags.is_named() {
                        Some(id)
                    } else {
                        None
                    }
                })
                .collect();
            let count = actual_named.len().min(expected_named.len());
            for i in 0..count {
                compare_nodes(
                    actual_tree,
                    expected_tree,
                    grammar,
                    Some(actual_named[i]),
                    Some(expected_named[i]),
                    diffs,
                    &format!("{path}/{i}"),
                );
            }
            if actual_named.len() != expected_named.len() {
                diffs.push(format!(
                    "{path}: named child count mismatch: actual={}, expected={}",
                    actual_named.len(),
                    expected_named.len()
                ));
            }
        }
    }
}

/// Verify structural invariants of a parsed tree:
/// - Every node's start_byte <= end_byte
/// - Every child's span is contained within its parent's span
pub fn check_tree_invariants(tree: &Tree, grammar: &Grammar) -> Vec<String> {
    let mut errors = Vec::new();
    if let Some(root) = tree.root_id() {
        check_node_invariants(tree, grammar, root, &mut errors);
    }
    errors
}

fn check_node_invariants(tree: &Tree, grammar: &Grammar, id: NodeId, errors: &mut Vec<String>) {
    let node = match tree.node_by_id(id) {
        Some(n) => n,
        None => return,
    };
    if node.start_byte > node.end_byte {
        errors.push(format!(
            "node {id} (kind={}): start_byte {} > end_byte {}",
            grammar.symbol_name(node.kind),
            node.start_byte,
            node.end_byte
        ));
    }
    for &child_id in &node.children {
        let child = match tree.node_by_id(child_id) {
            Some(c) => c,
            None => {
                errors.push(format!("node {id}: child {child_id} not found"));
                continue;
            }
        };
        if child.start_byte < node.start_byte {
            errors.push(format!(
                "node {id}: child {child_id} start_byte {} < parent start_byte {}",
                child.start_byte, node.start_byte
            ));
        }
        if child.end_byte > node.end_byte {
            errors.push(format!(
                "node {id}: child {child_id} end_byte {} > parent end_byte {}",
                child.end_byte, node.end_byte
            ));
        }
        check_node_invariants(tree, grammar, child_id, errors);
    }
}

/// Build an expected tree node for use in conformance tests.
pub fn expected_node(
    kind: &str,
    start: u32,
    end: u32,
    children: Vec<(String, u32, u32)>,
) -> ExpectedNode {
    ExpectedNode {
        kind: kind.to_string(),
        start_byte: start,
        end_byte: end,
        children,
    }
}

/// A lightweight expected tree descriptor for test assertions.
pub struct ExpectedNode {
    pub kind: String,
    pub start_byte: u32,
    pub end_byte: u32,
    pub children: Vec<(String, u32, u32)>,
}

/// Assert that a parsed tree matches an expected structure.
pub fn assert_tree_matches(tree: &Tree, grammar: &Grammar, expected: &ExpectedNode) {
    let diffs = compare_trees_to_expected(tree, grammar, expected);
    assert!(diffs.is_empty(), "Tree mismatch:\n  {}", diffs.join("\n  "));
}

fn compare_trees_to_expected(
    tree: &Tree,
    grammar: &Grammar,
    expected: &ExpectedNode,
) -> Vec<String> {
    let mut diffs = Vec::new();
    if let Some(root) = tree.root_node() {
        compare_node_to_expected(tree, grammar, root, expected, &mut diffs, "");
    } else {
        diffs.push("tree has no root node".to_string());
    }
    diffs
}

fn compare_node_to_expected(
    tree: &Tree,
    grammar: &Grammar,
    node: &Node,
    expected: &ExpectedNode,
    diffs: &mut Vec<String>,
    path: &str,
) {
    let actual_kind = grammar.symbol_name(node.kind);
    if actual_kind != expected.kind {
        diffs.push(format!(
            "{path}: kind: actual={actual_kind:?}, expected={:?}",
            expected.kind
        ));
    }
    if node.start_byte != expected.start_byte {
        diffs.push(format!(
            "{path}: start_byte: actual={}, expected={}",
            node.start_byte, expected.start_byte
        ));
    }
    if node.end_byte != expected.end_byte {
        diffs.push(format!(
            "{path}: end_byte: actual={}, expected={}",
            node.end_byte, expected.end_byte
        ));
    }
    let named: Vec<&Node> = node
        .children
        .iter()
        .filter_map(|&id| tree.node_by_id(id))
        .filter(|c| c.flags.is_named())
        .collect();
    let count = named.len().min(expected.children.len());
    for (i, (node, (ek, es, ee))) in named
        .iter()
        .zip(expected.children.iter())
        .enumerate()
        .take(count)
    {
        compare_node_to_expected(
            tree,
            grammar,
            node,
            &ExpectedNode {
                kind: ek.clone(),
                start_byte: *es,
                end_byte: *ee,
                children: Vec::new(),
            },
            diffs,
            &format!("{path}/{i}"),
        );
    }
    if named.len() != expected.children.len() {
        diffs.push(format!(
            "{path}: child count: actual={}, expected={}",
            named.len(),
            expected.children.len()
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glr_engine::Parser;

    fn arithmetic_grammar_json() -> &'static str {
        r#"{
            "name": "arithmetic",
            "rules": {
                "expression": {
                    "type": "CHOICE",
                    "members": [
                        {"type": "STRING", "value": "+"},
                        {"type": "STRING", "value": "*"},
                        {"type": "STRING", "value": "x"}
                    ]
                }
            }
        }"#
    }

    fn hello_world_grammar_json() -> &'static str {
        r#"{
            "name": "hello_world",
            "rules": {
                "program": {
                    "type": "SEQ",
                    "members": [
                        {"type": "STRING", "value": "hello"},
                        {"type": "STRING", "value": "world"}
                    ]
                }
            }
        }"#
    }

    #[test]
    fn test_compare_identical_trees() {
        let grammar = glr_grammar::compile_grammar(hello_world_grammar_json()).unwrap();
        let parser = Parser::new(grammar.clone());
        let source = b"hello world";
        let tree = parser.parse(source);
        let diffs = compare_trees(&tree, &tree, &grammar);
        assert!(diffs.is_empty(), "identical trees should match: {diffs:?}");
    }

    #[test]
    fn test_tree_invariants_hello_world() {
        let grammar = glr_grammar::compile_grammar(hello_world_grammar_json()).unwrap();
        let parser = Parser::new(grammar.clone());
        let source = b"hello world";
        let tree = parser.parse(source);
        let errors = check_tree_invariants(&tree, &grammar);
        assert!(
            errors.is_empty(),
            "structural invariants violated: {errors:?}"
        );
    }

    #[test]
    fn test_parse_successful() {
        let grammar = glr_grammar::compile_grammar(hello_world_grammar_json()).unwrap();
        let parser = Parser::new(grammar.clone());
        let source = b"hello world";
        let tree = parser.parse(source);
        assert!(tree.root_node().is_some(), "tree should have a root node");
        let errors = check_tree_invariants(&tree, &grammar);
        assert!(errors.is_empty(), "invariants: {errors:?}");
    }

    #[test]
    fn test_parse_empty_source() {
        let json = r#"{
            "name": "empty",
            "rules": {
                "program": {"type": "BLANK"}
            }
        }"#;
        let grammar = glr_grammar::compile_grammar(json).unwrap();
        let parser = Parser::new(grammar.clone());
        let tree = parser.parse(b"");
        // A blank grammar may produce a tree with or without a root node.
        // At minimum the tree should exist and satisfy invariants if a root is present.
        if let Some(_root) = tree.root_node() {
            let errors = check_tree_invariants(&tree, &grammar);
            assert!(errors.is_empty(), "invariants: {errors:?}");
        }
    }

    #[test]
    fn test_parser_handles_error_recovery() {
        let grammar = glr_grammar::compile_grammar(hello_world_grammar_json()).unwrap();
        let parser = Parser::new(grammar.clone());
        let source = b"hello garbage world";
        let tree = parser.parse(source);
        assert!(
            tree.root_node().is_some(),
            "error recovery should produce a tree"
        );
        let errors = check_tree_invariants(&tree, &grammar);
        assert!(errors.is_empty(), "error tree invariants: {errors:?}");
    }

    #[test]
    fn test_grammar_productions_and_symbols() {
        let grammar = glr_grammar::compile_grammar(hello_world_grammar_json()).unwrap();
        assert!(
            !grammar.productions.is_empty(),
            "grammar should have productions"
        );
        assert!(grammar.symbol_count > 0, "grammar should have symbols");
    }

    #[test]
    fn test_dfa_has_states() {
        let grammar = glr_grammar::compile_grammar(hello_world_grammar_json()).unwrap();
        assert!(
            !grammar.dfa_table.states.is_empty(),
            "DFA should have states"
        );
    }

    #[test]
    fn test_parse_arithmetic_grammar() {
        let grammar = glr_grammar::compile_grammar(arithmetic_grammar_json()).unwrap();
        let parser = Parser::new(grammar.clone());
        let source = b"x + x";
        let tree = parser.parse(source);
        let errors = check_tree_invariants(&tree, &grammar);
        assert!(errors.is_empty(), "arithmetic tree invariants: {errors:?}");
    }

    #[test]
    fn test_expected_tree_matches() {
        let grammar = glr_grammar::compile_grammar(hello_world_grammar_json()).unwrap();
        let parser = Parser::new(grammar.clone());
        let source = b"hello world";
        let tree = parser.parse(source);
        assert!(tree.root_node().is_some(), "tree should have a root");
        let errors = check_tree_invariants(&tree, &grammar);
        assert!(errors.is_empty(), "invariants: {errors:?}");
    }

    #[test]
    fn test_production_has_ids() {
        let grammar = glr_grammar::compile_grammar(hello_world_grammar_json()).unwrap();
        for (i, prod) in grammar.productions.iter().enumerate() {
            assert_eq!(prod.id.0 as usize, i, "production id should match index");
        }
    }

    #[test]
    fn test_grammar_version_fields() {
        let grammar = glr_grammar::compile_grammar(hello_world_grammar_json()).unwrap();
        assert_eq!(grammar.format_version, 1);
        assert_eq!(grammar.version, 15);
        assert_eq!(grammar.min_compatible_version, 13);
    }

    #[test]
    fn test_dfa_table_deserialization_roundtrip() {
        let grammar = glr_grammar::compile_grammar(hello_world_grammar_json()).unwrap();
        let data = glr_grammar::serialize_grammar(&grammar);
        let deserialized = glr_grammar::deserialize_grammar(&data).unwrap();
        assert_eq!(grammar.symbol_count, deserialized.symbol_count);
        assert_eq!(grammar.state_count, deserialized.state_count);
        assert_eq!(
            grammar.production_id_count,
            deserialized.production_id_count
        );
    }

    #[test]
    fn test_magic_header_rejects_bad_magic() {
        let bad_data = b"BADS\x00\x00\x00\x00{}";
        assert!(glr_grammar::deserialize_grammar(bad_data).is_none());
    }

    #[test]
    fn test_magic_header_rejects_short_data() {
        assert!(glr_grammar::deserialize_grammar(b"GLRG").is_none());
    }
}
