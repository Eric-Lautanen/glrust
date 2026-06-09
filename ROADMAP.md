# Pure Rust GLR Parser — Roadmap

**Goal**: Build a first-class, purely idiomatic Rust GLR parser ecosystem — no
`cc`, no `build.rs` C compilation, no `extern "C"` FFI — that is independently
useful as a crate while being capable of consuming tree-sitter grammar JSON
files and matching tree-sitter's output for compatibility with the existing
200+ grammar ecosystem.

See [`ROADMAP_pure_rust_glr.md`](ROADMAP_pure_rust_glr.md) for the full
detailed roadmap document with all phases, crate design, and validation strategy.

## Completion status

### Phase 0 — Foundation ✅

Clean build: `cargo clippy` → 0 warnings, `cargo fmt --check` → 0 diffs, zero `#[allow]` blocks. (Jun 2026)

| Section | Status | Details |
|---------|--------|---------|
| 0.2 Core data structures | ✅ | `glr-core`: Grammar, ParseTable (flat Vec + large/small states), Symbol, StateId, ProductionId — all `#[no_std]` + serde |
| 0.3 GLR engine | ✅ | RNGLR algorithm with GSS node sharing by `(state, position)`, GSS edges for reduce traversal, GOTO lookup, ε-rule fixed-point (RNGLR), cascading reductions via work-list with GOTO-merge cycle prevention. |
| 0.3.1 Tests | ✅ | 7 integration tests with LR(0) table generation via `TestGrammarBuilder`. All tests pass: arithmetic, dangling-else, ε-rule, long-chain, ambiguity, error-recovery, conflicted production. |
| 0.4 Tree construction | ✅ | MutableTree (arena) → Tree (immutable), TreeCursor with DFS walk (path-index based), named children, `node_at_byte`. |
| 0.5 Error recovery | ✅ | `SymbolId::ERROR` sentinel, token-skip resync loop, returns a Tree on malformed input. |

### Phase 1 — Lexer ⏳ (next)

| Section | Status | Details |
|---------|--------|---------|
| 1.1 Built-in lexer | ⏳ | DFA table-driven lexer — trait defined but implementation pending |
| 1.2 External scanner | ⏳ | `ExternalScanner` trait — API defined, implementations pending |
| 1.3 Incremental re-parse | ⏳ | Tree diff + subtree reuse — algorithm designed, implementation pending |

### Phase 2 — Grammar compilation ❌

| Section | Status |
|---------|--------|
| 2.1 Grammar DSL (proc-macro) | ❌ |
| 2.2 LR table generation from `.json` | ❌ |

### Phase 3 — Query engine ❌

| Section | Status |
|---------|--------|
| 3.1 Pattern matching | ❌ |
| 3.2 Highlight integration | ❌ |

### Phase 4 — Migration & polish ❌

| Section | Status |
|---------|--------|
| 4.1 Port priority grammars | ❌ |
| 4.2 Conformance validation | ❌ |
| 4.3 Performance benchmarks | ❌ |
