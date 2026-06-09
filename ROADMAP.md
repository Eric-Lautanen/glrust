# Pure Rust GLR Parser — Roadmap

**Goal**: Build a first-class, purely idiomatic Rust GLR parser ecosystem — no
`cc`, no `build.rs` C compilation, no `extern "C"` FFI — that is independently
useful as a crate while being capable of consuming tree-sitter grammar JSON
files and matching tree-sitter's output for compatibility with the existing
200+ grammar ecosystem.

See [`ROADMAP_pure_rust_glr.md`](ROADMAP_pure_rust_glr.md) for the full
detailed roadmap document with all phases, crate design, and validation strategy.

## Completion status

### Phase 0 — Foundation 🏗️ (scaffolded, not functionally complete)

| Section | Status | Details |
|---------|--------|---------|
| 0.2 Core data structures | ✅ | `glr-core`: Grammar, ParseTable (flat Vec + large/small states), Symbol, StateId, ProductionId — all `#[no_std]` + serde |
| 0.3 GLR engine | 🏗️ | GSS with node sharing by `(state, position)`, but **reduce loop not wired** — no GSS edges created, no reductions performed, no GOTO lookup, no ε-rule fixed-point. Parser struct and shift handler are scaffolded. |
| 0.3.1 Tests | 🏗️ | 6 integration tests with TestGrammarBuilder + TestLexer. **All tests are `#[ignore]`d** because the engine cannot parse anything. |
| 0.4 Tree construction | ✅ | MutableTree (arena) → Tree (immutable), TreeCursor with DFS walk, named children. |
| 0.5 Error recovery | 🏗️ | `SymbolId::ERROR` sentinel and token-skip resync loop are present, but **untestable until engine reduce loop is wired**. Error recovery test is `#[ignore]`d. |

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
