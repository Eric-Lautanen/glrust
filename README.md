# glrust — Pure Rust GLR Parser

> **Status: On Hold** — This project has been started with a solid foundation
> (core data structures, RNGLR engine, DFA lexer, query compiler, grammar
> compiler from JSON, conformance/bench/fuzz scaffolding) and is slated for
> future completion. Active development is paused for now. See
> [ROADMAP.md](ROADMAP.md) for the full plan and what each phase covers.

A purely idiomatic Rust GLR parser ecosystem — no `cc`, no `build.rs` C
compilation, no `extern "C"` FFI. Independently useful as composable crates
while being capable of consuming tree-sitter grammar JSON files and matching
tree-sitter's output for compatibility with the existing 200+ grammar ecosystem.

## Project status

This project is in **early development** and currently on hold.
See [ROADMAP.md](ROADMAP.md) for the full plan and current phase.

## Why?

The Rust ecosystem lacks a native incremental GLR parser that works in `no_std`,
WASM, and embedded targets without a C toolchain. That gap is what this project
fills.

## Workspace layout

| Crate | Description | `no_std`? |
|-------|-------------|-----------|
| `glr-core` | Grammar, ParseTable, Tree, cursors | Yes |
| `glr-engine` | Parser loop, GLR algorithm, error recovery | Yes (alloc) |
| `glr-lexer` | DFA lexer, ExternalScanner trait | Yes (alloc) |
| `glr-grammar` | grammar.json → Grammar compiler | No (std + serde) |
| `glr-macro` | `glr_grammar!` DSL proc-macro | No (proc-macro) |
| `glr-query` | Query compiler + executor | No |
| `glr-syntax` | Highlight pipeline | No |
| `glr-conformance` | Tree-sitter C comparison runner | No |
| `glr-fuzz` | cargo-fuzz targets | No |
| `glr-bench` | Criterion benchmarks | No |

## License

Licensed under the Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE)).
