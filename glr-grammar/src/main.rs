#![deny(unsafe_code)]
use std::fs;
use std::path::PathBuf;
use std::process;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage:");
        eprintln!("  glr-grammar compile <grammar.json> [-o output.bin]");
        eprintln!("  glr-grammar parse <grammar.bin> <source.gt>");
        process::exit(1);
    }

    let command = &args[1];
    match command.as_str() {
        "compile" => cmd_compile(&args[2..]),
        "parse" => cmd_parse(&args[2..]),
        _ => {
            eprintln!("Unknown command '{command}'. Use 'compile' or 'parse'.");
            process::exit(1);
        }
    }
}

fn cmd_compile(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: glr-grammar compile <grammar.json> [-o output.bin]");
        process::exit(1);
    }

    let input_path = &args[0];
    let output_path: Option<PathBuf> = if args.len() >= 3 && args[1] == "-o" {
        Some(PathBuf::from(&args[2]))
    } else {
        None
    };

    let json_str = match fs::read_to_string(input_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading {input_path}: {e}");
            process::exit(1);
        }
    };

    match glr_grammar::compile_grammar(&json_str) {
        Ok(grammar) => {
            eprintln!(
                "Compiled grammar: {symbols} symbols, {prods} productions, {states} states, {dfa_states} DFA states",
                symbols = grammar.symbol_count,
                prods = grammar.productions.len(),
                states = grammar.state_count,
                dfa_states = grammar.dfa_table.states.len(),
            );
            if let Some(out) = output_path {
                let data = glr_grammar::serialize_grammar(&grammar);
                if let Err(e) = fs::write(&out, &data) {
                    eprintln!("Error writing {}: {e}", out.display());
                    process::exit(1);
                }
                eprintln!(
                    "Wrote {len} bytes to {path}",
                    len = data.len(),
                    path = out.display()
                );
            }
        }
        Err(e) => {
            eprintln!("Compilation error: {e}");
            process::exit(1);
        }
    }
}

fn cmd_parse(args: &[String]) {
    if args.len() < 2 {
        eprintln!("Usage: glr-grammar parse <grammar.bin> <source.gt>");
        process::exit(1);
    }

    let grammar_path = &args[0];
    let source_path = &args[1];

    let grammar_data = match fs::read(grammar_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error reading {grammar_path}: {e}");
            process::exit(1);
        }
    };

    let grammar: glr_core::Grammar = match glr_grammar::deserialize_grammar(&grammar_data) {
        Some(g) => g,
        None => {
            eprintln!("Error: invalid or corrupt grammar file (bad magic header)");
            process::exit(1);
        }
    };

    let source = match fs::read(source_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error reading {source_path}: {e}");
            process::exit(1);
        }
    };

    let parser = glr_engine::Parser::new(grammar);

    let mut lexer = glr_lexer::BuiltinLexer::new(&source, &parser.grammar().dfa_table);
    let tree = parser.parse_with_lexer(&source, &mut lexer);

    if let Some(root) = tree.root_node() {
        eprintln!(
            "Parsed successfully: root spans bytes {}-{}",
            root.start_byte, root.end_byte
        );
        print_node(&tree, root, 0);
    } else {
        eprintln!("Parse produced an empty tree");
    }
}

fn print_node(tree: &glr_core::Tree, node: &glr_core::Node, depth: usize) {
    let indent = "  ".repeat(depth);
    let kind = if node.kind == glr_core::SymbolId::ERROR {
        "ERROR".to_string()
    } else {
        format!("sym#{}", node.kind.0)
    };
    let named = if node.flags.is_named() { " named" } else { "" };
    println!(
        "{indent}{kind} [{}, {}){}",
        node.start_byte, node.end_byte, named
    );
    for child_id in &node.children {
        if let Some(child) = tree.node_by_id(*child_id) {
            print_node(tree, child, depth + 1);
        }
    }
}
