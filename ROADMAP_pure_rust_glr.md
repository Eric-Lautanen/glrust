# Pure Rust GLR Parser -- Roadmap

**Goal**: Build a first-class, purely idiomatic Rust GLR parser ecosystem -- no
`cc`, no `build.rs` C compilation, no `extern "C"` FFI -- that is independently
useful as a crate (`glr` on crates.io) while being capable of consuming
tree-sitter grammar JSON files and matching tree-sitter's output for
compatibility with the existing 200+ grammar ecosystem.

**What this is not**: A hostility toward tree-sitter (a great project). It is
recognition that the Rust ecosystem lacks a native incremental GLR parser that
works in `no_std`, WASM, and embedded targets without a C toolchain. That gap
is what this project fills.

**End goal -- published as a Cargo workspace of composable crates** (see
Repository Structure below). The top-level facade crate name must be reserved
on crates.io before work begins -- `glr` is already taken (see Crate Design
section). Grammar authors who never touch tree-sitter can use the native DSL;
users migrating from tree-sitter can feed in existing `grammar.json` files.
The project targets editor embedding, language servers, and any
latency-sensitive tool requiring incremental re-parse.

## How to use this document

This roadmap is designed for an AI agent or human engineer to execute
**sequentially by phase**. Each section lists concrete deliverables,
testable criteria, and ship gates. Do not skip phases.

**Web search is required throughout.** Many details (tree-sitter source layout,
grammar JSON schema, query file format, existing external scanner C sources)
are not fully specified here -- the agent must fetch the latest canonical
information from:

- <https://github.com/tree-sitter/tree-sitter> -- source code, issue tracker
- <https://tree-sitter.github.io/tree-sitter/> -- official docs
- <https://crates.io/crates/tree-sitter> -- Rust crate API
- Individual grammar repos under <https://github.com/tree-sitter/> -- grammar
  JSON, scanner C sources, query files
- <https://github.com/softdevteam/grmtools/> -- grmtools/lrpar (LR/GLR, Yacc grammars)
- <https://github.com/igordejanovic/rustemo> -- rustemo (RNGLR + SPPF, closest engine)
- <https://github.com/ehwan/RustyLR> -- RustyLR (IELR/LALR/GLR proc-macro)

Before starting any phase, search for relevant tree-sitter issue discussions
and existing crates to avoid duplicating effort.

---

## Phase 0 -- Foundation (3-4 months)

### 0.1 Understand the target
Study tree-sitter's C source exhaustively (see Appendix A for current line
counts -- they are much larger than the original 2018 figures):
- `lib/src/parser.c` -- the GLR engine core
- `lib/src/stack.c` -- Graph-Structured Stack (separate file since ~2019)
- `lib/src/subtree.c` -- immutable subtree/node storage
- `lib/src/language.c` -- language instantiation & serialization
- `lib/src/lexer.c` -- built-in DFA lexer + external scanner dispatch
- `lib/src/node.c` -- tree cursor & node API
- `lib/src/query.c` -- query engine (separate concern, can wait)

**Deliverable**: Internal design doc mapping every C struct, function, and state
machine to Rust equivalents.

**Gap -- thread safety model**: No existing design addresses how `Tree`,
`Node`, and `Parser` behave across threads. Editors and LSPs parse on a
background thread while queries and cursor walks happen on the main thread.
Decisions needed in Phase 0: (a) whether `Tree: Send + Sync` (likely yes --
it is immutable after freezing), (b) whether `Node: Send` (it references an
arena, so only if the arena is `Sync`), (c) whether `Parser` can be `Clone`
for copy-on-write incremental re-parsing. The tree-sitter C API has a
design constraint that `ts_parser_parse` can run on any thread as long as
two threads don't call it concurrently on the same parser. Document the
Rust analogue before implementation.

### 0.2 Core data structures
Define the in-memory grammar format:

```rust
// The compiled grammar -- what a .so/.dll currently provides
struct Grammar {
    version: u32,  // ABI version; must support 14 and 15 (current default)
    symbol_count: u32,
    alias_count: u32,
    token_count: u32,
    external_token_count: u32,  // ADDED: external scanner tokens
    state_count: u32,
    large_state_count: u32,     // ADDED: tree-sitter splits states into "large" and "small"
    production_id_count: u32,   // ADDED: for alias and field tracking
    field_count: u32,           // ADDED: named fields on nodes
    max_alias_sequence_length: u32,
    // Parse table: the big one
    parse_table: Vec<ParseTableEntry>,
    // ... aliases, field mapping, metadata
}

// IMPORTANT: LR parse tables have TWO separate tables:
//   1. The ACTION table  [state x terminal   -> shift/reduce/accept/error]
//   2. The GOTO table    [state x nonterminal -> new_state]
// Tree-sitter merges these into a single parse_actions_and_gotos array.
// The Rust version should keep them logically separate for clarity.
enum ParseTableEntry {
    Shift { state: StateId },
    Reduce { symbol: SymbolId, child_count: u16, dynamic_precedence: i32, production_id: u16 },
    // Goto is a separate table entry type, not a Shift
    // (Goto is consulted after a Reduce, not on a terminal token)
    Goto { state: StateId },
    Accept,
    Error,
}
```

**Key insight**: tree-sitter's parse table is a compact 2D array
`[state][symbol] -> action`. The Rust version should be a flat `Vec<Action>` with
lookup via `state * symbol_count + symbol`.

**Important edge case -- large vs small states**: tree-sitter splits parse states
into "large states" (where every possible symbol has a non-error entry, so the
full row is stored) and "small states" (sparse, stored as sorted lists of
`(symbol, action)` pairs). Both representations must be supported. Naively
storing a full dense row for every state is correct but uses ~10x more memory
for real grammars (the C grammar has 2,015 states x 360 symbols).

**Deliverable**: Crate `glr-core` with `Grammar`, `ParseTable`, `Symbol`,
`StateId`, `Production` -- all `#[no_std]` compatible + `serde` for caching.

**Gap -- `no_std` verification**: `glr-core` declares `#![no_std]` but the
roadmap has no phase to verify it actually compiles without `std`. The
workspace must include a CI job that builds every `no_std`-eligible crate
(`glr-core`, `glr-engine`, `glr-lexer`) against a `thumbv7em-none-eabihf`
or `x86_64-unknown-none` target. Without this, `no_std` compatibility will
silently rot. Gate `std::error::Error` impls, file I/O, and `serde`
behind a default-enabled `std` feature. Use `hashbrown` (or `rustc-hash`)
for `HashMap`/`HashSet` in `no_std` paths.

### 0.3 The GLR engine
Implement the core `Parser::parse()` loop:

```
Input:  Grammar + source bytes
Output: MutableTree (write-only during parse)

1. Create a Graph-Structured Stack (GSS) -- not just a "fork stack"
2. For each token:
   a. Lex the next token (see Phase 1)
   b. For each active GLR stack head:
      - Lookup action in parse_table[current_state][token]
      - Shift   -> push new node onto GSS, consume token
      - Reduce  -> traverse GSS edges backward N steps, add new head
      - Accept  -> done
      - Error   -> try other heads; if all dead, report error
3. Handle ambiguity: when two heads converge on the same GSS node,
   *share* the node (the "G" in GLR). This is the real GSS merge,
   not just keeping separate fork lists.
4. epsilon-rule handling: reductions of zero-length productions must be
   processed in a fixed-point loop within a single token step
   (they don't consume input). This is the tricky part of GLR --
   naive implementations loop infinitely. Use the RNGLR algorithm
   (Right-Nulled GLR) to handle epsilon-rules correctly and efficiently.
```

**Algorithm choice**: Start with the RNGLR algorithm (Scott & Johnstone,
"Right Nulled GLR Parsers", 2006) rather than original Tomita. RNGLR correctly
handles epsilon-rules with O(n^3) worst-case complexity and is the algorithm used by
rustemo (which can be studied as a reference implementation). Tomita's original
Algorithm 1 fails on grammars with epsilon-rules.

**GSS note**: tree-sitter's `stack.c` is a dedicated ~600-line file implementing
the real GSS. The GLR "fork" concept in tree-sitter is implemented as stack
*versions* (handles into the GSS), not independent stacks. Study this carefully
before implementing -- the version handle design is key to the incremental reparse
algorithm in Phase 1.3.

The trickiest part is the **GSS sharing** logic: when two stack versions arrive
at the same `(state, subtree_start)` pair, they can share a single GSS node.
This is what makes GLR O(n) in practice for unambiguous grammars.

**GSS node key**: In standard RNGLR the GSS nodes are keyed by
`(parser_state, input_position)`. Two stack heads that reach the same state
at the same input position can share a single GSS node -- that is the merge.
Tree-sitter adds a further optimization: it also keys on the hash of the
subtree being reduced, allowing sharing across positions in some cases.
Implement the standard `(state, input_position)` key first; the subtree-hash
optimization is a follow-on.

**Deliverable**: `Parser::parse(&self, grammar: &Grammar, source: &[u8]) -> Tree`
producing correct output for unambiguous grammars.

### 0.3.1 Test: GLR engine correctness

Validate the engine on hand-crafted ambiguous and unambiguous grammars before
moving on:

| Test | Grammar | Input | Expected behavior |
|------|---------|-------|-------------------|
| Simple arithmetic | `E -> E + E \| int` | `"1 + 2 + 3"` | Right-associative parse tree (or left, per precedence) |
| Dangling else | `S -> if E then S \| if E then S else S \| ...` | `"if a then if b then c else d"` | else binds to innermost if |
| Empty production | `S -> A \| epsilon ; A -> "a"` | `""` | Single reduction to S |
| Long chain | `S -> A+ ; A -> "a"` | `"a" x 1000` | 1000 A nodes, no stack overflow |
| Ambiguous expression | `E -> E + E \| E * E \| int` | `"1 + 2 * 3"` | Both parses produced, GLR forks merged |
| Conflicted production | `S -> "a" S \| "a"` | `"a" x 10` | Terminates, accepts, no infinite loop |

Each test asserts:
- Parse succeeds or fails as expected
- Tree structure matches expected node count, depth, and symbol kinds
- No panics, no assertion failures
- With `cfg(debug_assertions)`, internal invariants hold (fork count <= limit,
  stack depth >= reduction pop count)

Run as `cargo test --test glr_engine` -- must pass before Phase 0 ships.

### 0.4 Tree construction
Build a mutable tree during reduce actions:

```rust
struct MutableTree {
    nodes: Vec<InternalNode>,
    // parent pointers and sibling links built incrementally
    // Field names (named children) tracked per-node via field_id
}

struct InternalNode {
    kind: SymbolId,
    start_byte: u32,
    end_byte: u32,
    start_row: u32,     // ADDED: row/column tracking for error messages
    start_col: u32,
    end_row: u32,
    end_col: u32,
    child_count: u32,
    named_child_count: u32,  // ADDED: anonymous vs named children differ
    first_child: Option<u32>,
    next_sibling: Option<u32>,
    parent: Option<u32>,
    field_id: Option<u16>,  // ADDED: which field slot this node fills in its parent
    is_named: bool,         // ADDED: anonymous tokens are not "named" in tree-sitter
    is_missing: bool,       // ADDED: nodes inserted during error recovery
    is_extra: bool,         // ADDED: nodes from `extras` (e.g. comments)
    has_changes: bool,      // ADDED: set during incremental re-parse
}
```

Freeze to `Tree` (immutable, backed by a bump-allocated arena of `Node`
structs, wrapped in `Arc` for shared ownership) on completion. The freeze
step is a single allocation + memcopy -- keeping nodes in a compact arena
(not individually `Box`ed) is essential for cache performance and for the
incremental re-parse subtree-reuse path.

**Gap -- arena/allocator strategy**: The freeze mentions a "bump-allocated
arena" but the roadmap does not specify which allocator. The GSS during
parsing also produces many short-lived intermediate nodes. Three concerns:
(a) separate scratch arena for parse-phase allocations that is dropped
after freeze, (b) dedicated `TreeArena` for the final tree, (c) whether
to use a third-party crate (`bumpalo`, `slab`, `typed-arena`) or a custom
`Vec<InternalNode>` with index-based references. The `Arc` wrapper on the
arena adds atomic reference-counting overhead; consider `Rc` (not `Send`)
for single-threaded use and `Arc` behind a feature flag, or keep `Arc` and
document `Send` / `Sync` bounds as decided in 0.1.

**Deliverable**: Immutable `Tree` with cursor API matching tree-sitter's:
`root_node()`, `walk()`, `node_at_byte()`, `named_children()`, `child_by_field_name()`.

### 0.5 Error recovery (ERROR nodes)

For editor use, the parser must never reject input -- it must produce a tree
with ERROR nodes wherever syntax is invalid. Tree-sitter's approach:

1. When no valid action exists for the current state + token, create an ERROR
   node that spans the problematic region
2. Skip tokens until a state is found that can continue (a sync strategy)
3. ERROR nodes are real tree nodes with a `kind() == "ERROR"` -- queries can
   match them, the editor can style them

The simplest recovery: when stuck, consume tokens one at a time, checking after
each if the current state can shift or reduce. Once resync succeeds, resume
normal parsing.

**Deliverable**: `Parser::parse()` always returns a `Tree`, never fails. ERROR
nodes correctly bracket invalid ranges. Test with malformed inputs.

---

## Phase 1 -- Lexer (2-3 months)

### 1.1 Built-in lexer
Replace tree-sitter's hand-written C lexer with a generic, table-driven lexer:

```rust
trait Lexer {
    /// Advance past the next token starting at the current cursor position.
    /// Returns the token kind (as a SymbolId), updating internal cursor state.
    /// Returns None only at EOF.
    fn next_token(&mut self, valid_symbols: &[bool]) -> Option<Token>;

    /// Current byte offset in the source.
    fn cursor(&self) -> usize;

    /// Reset to a specific byte offset (used during incremental re-parse).
    fn reset_to(&mut self, byte_offset: usize);
}

/// Table-driven lexer generated from grammar `.json`
struct BuiltinLexer<'src> {
    source: &'src [u8],
    cursor: usize,
    dfa: &'static [DFAState],  // 'static: generated at compile time
}
```

**Note**: The lexer holds a reference to the source and owns the cursor
position. The parser calls `next_token` and the lexer advances internally.
This is the correct inversion of control -- the parser should not manage byte
offsets directly.

**Gap -- Unicode beyond byte offsets**: The lexer works on `&[u8]` and the
test mentions multi-byte UTF-8, but several Unicode concerns are not addressed.
(a) Grapheme clusters -- `"é"` can be one codepoint (U+00E9) or two
(U+0065 + U+0301). Token spans report byte offsets, but `start_row` /
`start_col` columns should be in Unicode columns, not byte offsets. Editors
expect column counts based on grapheme clusters (what the cursor shows).
(b) Unicode normalization -- two semantically identical files with different
normalization forms (NFC vs NFD) should parse identically. This is the
lexer's responsibility, not the parser's. (c) BOM handling -- files starting
with U+FEFF should either skip it or treat it as whitespace, not produce a
syntax error. Document a column-counting strategy (the `unicode-width` crate
is the standard approach) early; retrofitting column tracking later is
painful.

### 1.2 External scanner API
Most real grammars need hand-written "external scanners" for indentation
(Python), heredocs (Ruby, Bash), template strings (JavaScript), etc.

```rust
/// The external scanner trait -- implemented in Rust by grammar authors.
/// Mirrors tree-sitter's C external scanner API exactly.
trait ExternalScanner {
    /// Attempt to scan the next external token.
    ///
    /// `valid_symbols` is a bitmask slice indexed by external token id.
    /// The scanner MUST check this before scanning -- at any given parse state,
    /// only certain external tokens are grammatically valid. Ignoring
    /// valid_symbols causes shift/reduce conflicts and infinite loops.
    fn scan(&mut self, source: &[u8], cursor: &mut usize, valid_symbols: &[bool]) -> bool;

    /// Serialize scanner state to bytes (for incremental re-parse).
    /// Max serialized size is 1024 bytes (tree-sitter's limit). Return the
    /// number of bytes written.
    fn serialize(&self, buffer: &mut [u8]) -> usize;

    /// Restore scanner state from bytes.
    fn deserialize(&mut self, buffer: &[u8]);

    /// Called when the scanner is created; initialize any state here.
    fn create() -> Self where Self: Sized;

    /// Called when the scanner is destroyed (not strictly needed in Rust,
    /// but matches the C API for completeness).
    fn destroy(&mut self) {}
}
```

**Critical detail -- `valid_symbols`**: This parameter is the most important and
most commonly misunderstood part of the external scanner API. At every call, the
parser passes which external tokens are legal at the current parse state. A
scanner that ignores this and always scans any token will produce incorrect
results (or infinite loops) on ambiguous inputs. Every scanner implementation
**must** begin by checking `valid_symbols`.

### 1.2.1 Test: Lexer & scanner coverage

Each lexer mode (built-in DFA, external scanner) gets:

| Test | What it validates |
|------|-------------------|
| Token-by-token comparison | Lex `"if x == 3 then return"` via library and via our lexer; every token kind, span, and value must match |
| External scanner integration | Python scanner on `"if x:\n  pass"` -- produces INDENT/DEDENT at correct positions |
| EOF behavior | Empty input -> single EOF token. Input ending mid-token -> error |
| Multi-byte UTF-8 | `"// JapaneseJapaneseJapanese\nlet x = 1"` -- comment span covers all bytes, not just ASCII |
| Maximum token length | 10 MB string literal -- lexer doesn't OOM, produces single string token |
| Scanner serialization | Round-trip: scan -> serialize -> deserialize -> scan again from deserialized state, identical token stream |

Build a **lexer fuzz harness** (`cargo fuzz --target lexer`) that feeds random
byte strings and asserts:
- No panics
- Token kinds are valid per the grammar
- Spans are contiguous and non-overlapping
- Spans cover the entire input (end of last span = input length)

### 1.3 Incremental re-parse
Core selling point of tree-sitter. When source changes:
1. Accept an `InputEdit { start_byte, old_end_byte, new_end_byte, ... }` descriptor
2. Walk the old tree, marking all nodes that overlap `[start_byte, old_end_byte)`
   as `has_changes = true`
3. Re-lex from `start_byte`. For unchanged nodes to the right of the edit,
   **reuse the cached parser state stored on each node** -- not lexer DFA state.
   Tree-sitter caches the LR *parser* state at each node boundary so that
   re-parsing can resume from any node without replaying prior input.
4. Re-parse from the first changed node:
   - Before each reduce, check if the top N nodes on the stack are all unchanged
     and their combined span still matches -- if so, **reuse the old subtree**
     without re-reducing
5. Stop re-parsing as soon as the new parse state matches the old parse state
   at a node boundary (the "lookahead reuse" / "right-side reuse" optimization)

**Key invariant**: `parse_incremental(old_tree, edit)` must produce exactly the
same tree as a full `parse(new_source)`. This is verified by the fuzz harness
in Tier 3.

**Edge cases that must be tested explicitly**:
- Zero-byte edit (replace text with identical text) -- must produce identical tree
  without any re-parsing
- Edit at byte offset 0 (beginning of file)
- Edit spanning the entire file (forces full re-parse but must not regress)
- Multi-byte UTF-8 characters straddling the edit boundary
- Edit that changes a node's kind (e.g. turns a string into an identifier) --
  tree structure changes, not just spans
- Consecutive edits without re-parse between them (caller applies edits in batch)

This is ~500-1,000 lines of subtle state management. tree-sitter's incremental
re-parse is its killer feature -- must get this right.

**Deliverable**: `Parser::parse_incremental(&mut self, old_tree: &Tree, edit: &InputEdit, source: &[u8]) -> Tree`

---

## Phase 2 -- Grammar compilation (3-4 months)

### 2.1 Grammar DSL
Define a Rust-native DSL for writing grammars:

```rust
glr_grammar! {
    language: "javascript",

    // `word` token: used for keyword detection in error recovery
    word: "identifier",

    // `extras`: tokens that can appear anywhere (comments, whitespace)
    extras: ["/\\s/", "comment"],

    // `conflicts`: explicit GLR ambiguities the grammar author declares
    conflicts: [
        ["expression", "pattern"],  // valid both as expr and pattern
    ],

    // `precedences`: ordered precedence groups (lower = binds looser)
    precedences: [
        ["addition", "multiplication"],
    ],

    tokens: {
        "identifier" = /[a-zA-Z_$][\w$]*/,
        "number"     = /\d+(\.\d+)?/,
        "string"     = /"[^"]*"/,
        "+"          = "+",
        ";"          = ";",
        "{"          = "{",
        "}"          = "}",
        "comment"    = /\/\/.*/,
    },

    rules: {
        Program       = { Statement* },
        Statement     = { Expression ";" },
        // Precedence annotations on rules:
        Expression    = { "identifier" }
                      | { "number" }
                      | { Expression "+" Expression } #[prec_left("addition")]
                      | { "{" Expression "}" },
    },
}
```

This generates the LR parse table + lexer DFA at compile time via a proc-macro.
**Missing these features (word, extras, conflicts, precedences) means the macro
cannot correctly generate tables for any real-world language grammar** -- they
are not optional extensions.

**Gap -- conflict diagnostics**: When the table generator encounters an
unresolved shift/reduce or reduce/reduce conflict, the roadmap says "error
or warn" but gives no specification for diagnostic quality. Tree-sitter and
LALRPOP produce human-readable reports showing the grammar state, the
offending tokens, the conflicting productions, and (in LALRPOP's case) a
suggested fix. Without this, grammar authors will debug blind. The compiler
must produce, at minimum: (a) the conflicting state number, (b) the
lookahead token(s) causing the conflict, (c) both conflicting productions
with source locations, (d) whether the conflict is shift/reduce or
reduce/reduce. Optionally: a dot-file of the LR automaton for visual
debugging (like rustemo's `--dot` flag).

**Gap -- grammar testing framework**: Tree-sitter ships `tree-sitter test`
which runs a corpus of `(input, expected_tree)` pairs encoded in a DSL
against the generated parser. Grammar authors need equivalent tooling:
a test runner that loads a grammar, parses a set of `.corpus` files, and
asserts tree structure. Without this, grammar ports in Phase 4 will be
validated only by the conformance suite (which requires the tree-sitter C
baseline), blocking community contributions.

### 2.2 LR table generation from `.json`
Alternatively (interop): read tree-sitter's `grammar.json`, run our own
LR(1)/GLR table generator, produce `Grammar` struct.
This lets us consume existing tree-sitter grammar repos without rewriting them.

Implementation:
1. Parse `grammar.json` -> internal `GrammarAst`
2. Inline `inline` rules (tree-sitter's `inline` key specifies rules to expand
   inline rather than represent as separate nodes -- must be done before item set construction)
3. Compute FIRST/FOLLOW sets
4. Build canonical LR(1) items -> state machine (or LALR(1) for smaller tables)
5. Resolve conflicts (declared `conflicts` -> allow as GLR; `precedences` ->
   shift/reduce resolution; unresolved -> error or warn)
6. Process `supertypes` (nodes that should show as their supertype in the CST)
7. Emit compressed parse table (large-state + small-state split, see S0.2)

**Key grammar.json fields** (all must be handled, not just `rules`):
- `rules` -- the grammar productions
- `extras` -- tokens that may appear anywhere (usually whitespace, comments)
- `conflicts` -- explicit GLR ambiguity declarations
- `precedences` -- precedence group ordering
- `externals` -- tokens provided by the external scanner
- `inline` -- rules to be inlined (removed as separate AST nodes)
- `supertypes` -- abstract node types for the query engine
- `word` -- keyword capture token for error recovery

**Deliverable**: `cargo run -- compile path/to/grammar.json -o grammar.bin`

**Gap -- compile-time caching**: The compiler produces `grammar.bin` via
serde, but the roadmap does not address cold-start cost. Loading and
deserializing a grammar for a language like JavaScript (~2 MB serialized)
takes measurable time at editor startup. Consider: (a) `include_bytes!` at
compile time to embed the grammar in the binary, (b) a lazy-static
initializer so the grammar is loaded on first use, not at library load,
(c) a memory-mapped (`mmap`-compatible) format to avoid deserialization
entirely. For the build.rs and proc-macro paths the grammar can be generated
at compile time, but CLI users loading `grammar.bin` at runtime will hit
this cost.

---

## Phase 3 -- Query engine (2-3 months)

### 3.1 Pattern matching

tree-sitter's query system is a mini-language for finding syntax patterns.
Port `lib/src/query.c` (see Appendix A for current line count -- substantially
more than ~2,000): compile query strings into a state machine, execute against
a `Tree`.

**Query syntax to support** (from tree-sitter's `.scm` files):

| Feature | Example | Priority |
|---------|---------|----------|
| Node kind match | `(function_definition)` | P0 (required) |
| Anonymous node match | `"return"` | P0 |
| Field match | `body: (block)` | P0 |
| Wildcard | `(_)` | P0 |
| Nested patterns | `(function name: (identifier) @name)` | P0 |
| Capture | `@name` | P0 |
| Quantifiers | `+`, `*`, `?` | P1 |
| Alternation | `(_ "if" _)` / `(_ "else" _)` | P1 |
| Predicates | `(#eq? @name "foo")` | P1 |
| Anchors | `. (program)` (start-of-root) | P2 |
| `#set!` directives | `#set! priority 1` (highlight overrides) | P1 |
| Match negation | `(identifier) @name (#not-eq? @name "a")` | P2 |
| `make-syntax-query`-style | Wildcard field `(_)* @capture` | P2 |

**Implementation sketch:**

```rust
/// Compiled representation of a `.scm` query file.
struct Query {
    /// State machine for matching patterns against a tree cursor.
    states: Vec<QueryState>,
    /// List of capture names in declaration order.
    captures: Vec<String>,
    /// Predicates that must be evaluated after a match.
    predicates: Vec<Predicate>,
}

struct QueryMatch {
    pattern_index: usize,
    captures: Vec<(CaptureName, Node)>,
}

impl Tree {
    fn query(&self, query: &Query) -> QueryMatches<'_>;
}

impl<'tree> Iterator for QueryMatches<'tree> {
    type Item = QueryMatch;
}
```

**Deliverable**: `cargo test --test query` passes all tree-sitter `.scm` test
files from the `tree-sitter-javascript` and `tree-sitter-python` repos.

### 3.2 Highlight integration
Hook queries into a syntax highlighting pipeline, providing a reference
implementation that consumes `.scm` highlight query files.

**Deliverable**: `glr_syntax::highlight(source, grammar, queries)` that takes
source bytes + a compiled `Grammar` + a set of compiled `Query` objects and
returns an iterator of `(byte_range, highlight_name)` pairs -- compatible with
the highlight names used by existing tree-sitter `.scm` files so editors can
reuse their existing highlight themes without modification.

**Gap -- highlight pipeline underspecified**: The deliverable above is a
single function signature, but a real highlighting pipeline involves several
non-trivial steps not detailed in the roadmap. (a) **Query ordering**:
highlight queries declare `#set! priority` and `#set! injection.language` --
the pipeline must sort captures by priority and resolve injection languages.
(b) **Theme resolution**: raw capture names (`@keyword`, `@string`) must be
mapped to editor-specific theme scopes; the pipeline should ship with a
default theme map. (c) **Overlapping ranges**: when two captures overlap
(e.g. `@keyword` and `@keyword.function`), the pipeline must decide which
wins (higher priority or narrower span). (d) **Multi-line spans**: some
highlight captures span multiple lines (e.g. string templates) -- the
iterator must handle incremental consumption line-by-line for editor use.
(e) **Injection parsing**: embedded languages (JS in HTML, SQL in Python)
require recursively parsing an embedded range with a different grammar and
merging the highlight ranges. This is the hardest part of the tree-sitter
highlight system and is not mentioned at all. Ship Phase 3.2 as a flat
single-language highlighter first; document injection parsing as a Phase 3.3
follow-up.

---

## Phase 4 -- Migration & polish (3-4 months)

### 4.1 Port priority grammars
Tree-sitter has 100+ grammar repos in the `tree-sitter` GitHub org, and
hundreds more in the community. Porting all of them is not the goal of v1.

**Priority order for v1** (target the 10 grammars used by the most editors):

| Priority | Grammar | External scanner? | Notes |
|----------|---------|------------------|-------|
| P0 | JavaScript / TypeScript | Yes (regex, template strings) | Shares scanner |
| P0 | Python | Yes (INDENT/DEDENT) | Classic scanner complexity |
| P0 | Rust | No | Good self-hosting test |
| P0 | JSON | No | Trivial -- good smoke test |
| P1 | HTML | Yes (self-closing, raw text) | |
| P1 | CSS / SCSS | Partial | |
| P1 | Bash | Yes (heredocs) | |
| P2 | Go | No | |
| P2 | Ruby | Yes (heredocs, regex) | |
| P2 | C / C++ | No / Yes | C++ scanner is complex |

For each grammar:
1. Fetch the upstream `grammar.json` from `github.com/tree-sitter/tree-sitter-<lang>`
2. Run through the Phase 2.2 compiler to produce `grammar.bin`
3. Re-implement external scanner in Rust (this is the manual labor)
4. Test against known-good parse trees via the Tier 2 conformance suite

~10 grammars have no external scanner -> compile-only effort. The scanner
ports range from ~50 lines (HTML) to ~500 lines (Python, Bash) of Rust.

### 4.2 Conformance validation

For each grammar, run through the **Validation strategy** conformance suite
(Tier 2) against the tree-sitter C baseline. No grammar ships until it passes
node-by-node comparison on the full corpus.

### 4.3 Performance benchmarks

Run the criterion benchmarks (Tier 4) for the grammar's language group.
Regression thresholds are release-blocking.

### 4.4 CLI tooling

The roadmap currently defines only a single CLI command:
`cargo run -- compile path/to/grammar.json -o grammar.bin`. A production
parser ecosystem needs a proper CLI binary published via `cargo install`.

**Minimum CLI commands for v1:**
- `glr compile <grammar.json> -o <grammar.bin>` -- compile grammar to binary
- `glr parse <grammar.bin> <file>` -- parse a file and print the CST
- `glr highlight <grammar.bin> <queries.scm> <file>` -- highlight a file,
  outputting capture ranges (for piping into editors or scripts)
- `glr test <grammar.json> <corpus/>` -- run corpus test files against a
  grammar (the grammar testing framework from Phase 2.1 gap)
- `glr init <lang>` -- scaffold a new grammar crate with template files

The CLI should be a thin crate (`glr-cli`) that depends on the library
crates, not a monolithic binary. This keeps the CLI as a consumer of the
library API, which doubles as a dogfooding exercise.

---

## Crate design -- publishing strategy

### As a crate or standalone project?

**Both, via a Cargo workspace.** The workspace is developed as a standalone
repository but each sub-crate is published independently to crates.io, so
users can depend only on the parts they need.

Recommended crates.io names (check availability before starting):

| Crate | crates.io name | `no_std`? | Purpose |
|-------|----------------|-----------|---------|
| `glr-core` | `glr-core` | **Yes** | Grammar, ParseTable, Tree -- pure data |
| `glr-engine` | `glr-engine` | **Yes** (alloc) | Parser loop, GLR algorithm |
| `glr-lexer` | `glr-lexer` | **Yes** (alloc) | DFA lexer, ExternalScanner trait |
| `glr-grammar` | `glr-grammar` | No (std + serde) | grammar.json -> Grammar compiler (build tool / library) |
| `glr-macro` | `glr-macro` | No (proc-macro) | `glr_grammar!` DSL proc-macro (Phase 2.1) |
| `glr-query` | `glr-query` | No | Query compiler + executor |
| `glr-syntax` | `glr-syntax` | No | Highlight pipeline |
| *(facade)* | TBD (not `glr`) | Feature-gated | Facade re-export of all the above |

**`no_std` + `alloc` strategy**: `glr-core` and `glr-engine` must compile with
`#![no_std]` + `extern crate alloc`. This allows embedding in firmwares,
kernel modules, and WASM hosts that do not have a standard library. Gate
`std`-dependent features (file I/O, `std::error::Error` impls) behind a
default-enabled `std` feature flag so `no_std` users can opt out.

### What the community is missing

The precise gap:

1. **A Rust-native incremental re-parse library** -- rustemo and RustyLR have
   GLR, but neither has incremental re-parse. This is the biggest gap.
2. **A query engine for parsed trees** -- no existing Rust parser has anything
   comparable to tree-sitter's `.scm` query system.
3. **tree-sitter grammar JSON consumption** -- the 200+ grammar ecosystem is
   locked inside tree-sitter. A converter unlocks it for all Rust parsers.
4. **`no_std` GLR** -- grmtools and rustemo both require std.

Addressing all four makes this project genuinely novel and immediately useful
to the Rust editor/LSP ecosystem (Helix, zed, rust-analyzer all have
tree-sitter dependencies they would swap for a pure-Rust alternative).

### Cross-cutting gap: documentation

The roadmap has no documentation phase, but a parser library without clear
docs will not gain adoption. Documentation must be produced in parallel with
each phase, not deferred to the end.

**Required documentation by phase:**
- Phase 0 doc: architecture overview, `glr-core` API reference, GSS and
  RNGLR explanation with diagrams, thread safety model
- Phase 1 doc: lexer authoring guide, `ExternalScanner` trait walkthrough
  with a worked example (port the Python INDENT/DEDENT scanner),
  unicode handling rules
- Phase 2 doc: grammar DSL reference, `grammar.json` consumption guide,
  conflict resolution tutorial, grammar testing framework CLI reference,
  grammar testing framework tutorial
- Phase 3 doc: query file format reference, predicate reference, highlight
  pipeline configuration, injection language guide
- Phase 4 doc: migration guide from tree-sitter (how to port an existing
  grammar), per-language porting notes, FAQ

**Format**: Each crate ships its API docs via `#[doc]` attributes. The
project also maintains a book (using `mdBook`) with tutorials and
architecture docs. CI must check that doc examples compile (`cargo test
--doc`).

---

## Repository structure

Proposed workspace layout in a single git repository:

```
glr/
|-- Cargo.toml              # workspace root
|-- glr-core/               # Phase 0 -- Grammar, ParseTable, Tree, cursors
|-- glr-engine/             # Phase 0 -- Parser, GLR loop, error recovery
|-- glr-lexer/              # Phase 1 -- BuiltinLexer, ExternalScanner trait
|-- glr-grammar/            # Phase 2 -- grammar JSON -> ParseTable compiler (lib + bin)
|-- glr-macro/              # Phase 2 -- glr_grammar! proc-macro DSL
|-- glr-query/              # Phase 3 -- query compiler + executor
|-- glr-syntax/             # Phase 3 -- highlight pipeline (consumes queries)
|-- glr-conformance/        # Validation Tier 2 -- tree-sitter C comparison runner
|-- glr-fuzz/               # Validation Tier 3 -- cargo-fuzz targets
|-- glr-bench/              # Validation Tier 4 -- criterion benchmarks
|-- grammars/               # Phase 4 -- vendored grammar JSON + scanner ports
|   |-- javascript/
|   |-- python/
|   |-- rust/
|   \-- ...
\-- corpus/                 # Validation Tier 2 -- source file corpus (git-lfs)
    |-- javascript/
    |-- python/
    \-- ...
```

**Crate dependency order** (arrow = "depends on"):
```
glr-syntax -> glr-query -> glr-engine -> glr-lexer -> glr-core
                                    ^
                             glr-grammar -> glr-core
                             glr-macro  -> glr-grammar
```

**No circular deps**: `glr-lexer` depends on `glr-core` (for `Symbol` types)
but not on `glr-engine`. The engine orchestrates lex + parse.

**Grammar JSON source**: Each `grammars/<lang>/` directory mirrors the
corresponding upstream tree-sitter grammar repo. The `grammar.json` is copied
from <tt>https://github.com/tree-sitter/tree-sitter-&lt;lang&gt;</tt>. Scanner
C sources are ported to Rust in the same directory.

---

## Staffing estimate

| Phase | Effort | Best person |
|-------|--------|-------------|
| 0. Foundation | 1 person x 3-4 mo | Rust compiler hacker |
| 1. Lexer | 1 person x 2-3 mo | PL/grammar expert |
| 2. Grammar compilation | 1 person x 3-4 mo | PL theory (LR items, FIRST/FOLLOW) |
| 3. Query engine | 1 person x 2-3 mo | Pattern matching enthusiast |
| 4. Migration | 1 person x 3-4 mo (repeat for each grammar) | Diligent generalist |
| **Validation** (horizontal) | **1 person x ongoing** | **Testing/infra engineer** |

The validation role is full-time from Phase 1 onward: writing conformance tests,
running fuzzers, triaging regressions, maintaining the corpus, and operating the
benchmark dashboard. This is not a "QA at the end" position -- they participate
in design reviews, write the property tests alongside each feature, and define
the correctness model before implementation starts.

**Total**: ~15-18 person-months for a v1 that supports the top 5 grammars
(JavaScript, Python, Rust, TypeScript, JSON). All 20 grammars -> ~24-30
person-months. Plus 1 ongoing FTE for validation across all phases.

---

## Validation strategy (horizontal track)

This is **not a phase** -- testing runs throughout the entire project, with
increasing rigor at each milestone. Every phase ship gate requires the test
suite at that tier to pass.

### Correctness model -- what does "correct" mean?

Three levels, in order of trust:

1. **Structural parity** -- Our `Tree` and tree-sitter's output share the same
   node kind at every byte offset. This is the cheapest and most important
   assertion.
2. **Semantic parity** -- For any query, our tree and tree-sitter's tree return
   the same match set over the same corpus.
3. **Stability parity** -- The same incremental edits produce the same final
   tree regardless of when they are applied (real-time or batched).

### Tier 1 -- Unit & property tests (per-PR)

Runs in CI on every commit. Budget: < 30 s.

```
cargo test
cargo test --features proptest
```

| Suite | Tool | What it checks | When it must pass |
|-------|------|----------------|-------------------|
| GLR engine | `#[test]` | Shift/reduce/accept for hand-crafted grammars (S0.3.1) | Phase 0 |
| Lexer | `#[test]` | Token-by-token parity against tree-sitter library on 20 hand-written cases (S1.2.1) | Phase 1 |
| Tree construction | `#[test]` | Parent links, sibling order, depth invariants | Phase 0 |
| Parse table generation | `#[test]` | Table is deterministic -- same grammar `.json` -> same table | Phase 2 |
| Query compilation | `#[test]` | Query patterns compile without error | Phase 3 |
| Property: parse identity | `proptest` | `parse(source) = parse(parse(source).to_string())` for any valid AST | Phase 0+ |

**Property-based tests** using `proptest`:

```rust
proptest! {
    // No node span exceeds the input length.
    // Note: Parser::parse() always returns a Tree -- it never returns Err.
    // ERROR nodes are inserted for invalid syntax rather than failing.
    fn spans_within_bounds(src: String) {
        let tree = Parser::new(GRAMMAR).parse(src.as_bytes());
        for node in tree.root_node().walk() {
            prop_assert!(node.end_byte() <= src.len());
        }
    }

    // Span start <= span end for every node.
    fn spans_are_ordered(src: String) {
        let tree = Parser::new(GRAMMAR).parse(src.as_bytes());
        for node in tree.root_node().walk() {
            prop_assert!(node.start_byte() <= node.end_byte());
        }
    }

    // Every child's span is contained within its parent's span.
    fn child_spans_within_parent(src: String) {
        let tree = Parser::new(GRAMMAR).parse(src.as_bytes());
        for node in tree.root_node().walk() {
            for child in node.children() {
                prop_assert!(child.start_byte() >= node.start_byte());
                prop_assert!(child.end_byte()   <= node.end_byte());
            }
        }
    }
}
```

### Tier 2 -- Conformance suite (per-release)

A standalone crate that compares our parser against tree-sitter C on a corpus
of real-world source files. Budget: < 5 min.

```rust
// Conformance test runner -- pseudo-code
// Uses tree-sitter Rust bindings (current API as of v0.24+)
fn test_conformance(language: &str, files: &[PathBuf]) {
    // tree-sitter side (ground truth)
    let mut c_parser = tree_sitter::Parser::new();
    c_parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .expect("language version mismatch");

    // our side
    let grammar = glr::grammar::load(language);
    let rs_parser = glr::Parser::new(&grammar);

    for file in files {
        let source = std::fs::read(file).unwrap();

        let c_tree = c_parser.parse(&source, None).unwrap();
        let rs_tree = rs_parser.parse(&source).unwrap();

        // Node-by-node comparison
        compare_trees(c_tree.root_node(), rs_tree.root_node(), &source);
    }
}

fn compare_trees(c: tree_sitter::Node, rs: glr::Node, source: &[u8]) {
    assert_eq!(c.kind(), rs.kind());
    assert_eq!(c.start_byte(), rs.start_byte());
    assert_eq!(c.end_byte(), rs.end_byte());
    // Compare named children only -- anonymous node counts can differ
    // if our grammar inlines extras differently
    assert_eq!(c.named_child_count(), rs.named_child_count(),
        "named child count mismatch at {}", c.kind());

    for i in 0..c.named_child_count() {
        compare_trees(
            c.named_child(i).unwrap(),
            rs.named_child(i).unwrap(),
            source
        );
    }
}
```

**Corpus sources:**

| Language | Corpus | Files | Lines |
|----------|--------|-------|-------|
| JavaScript | `mdn/content` (top 200 pages) | 200 | ~50K |
| Python | CPython `Lib/` (first 100 modules) | 100 | ~80K |
| Rust | `rust-analyzer` source | 300 | ~120K |
| TypeScript | `TypeScript/src/compiler/` | 150 | ~200K |
| JSON | `npm` package.json collection | 500 | ~50K |
| HTML | W3C spec samples | 50 | ~30K |
| CSS | Bootstrap + Tailwind source | 20 | ~40K |
| C | `tree-sitter` own source | 50 | ~20K |
| Go | Standard library | 100 | ~200K |
| Ruby | Rails models (10 projects) | 50 | ~10K |

**Regression lockbox**: Any bug found in production is reduced to the smallest
reproduction, added to this suite, and **never allowed to regress**.

### Tier 3 -- Fuzz testing (continuous)

Runs 24/7 on a dedicated machine or CI cron. Budget: unlimited.

| Target | Tool | Input | Checks |
|--------|------|-------|--------|
| GLR engine | `cargo fuzz` | Random byte strings from grammar token alphabet | No crash, no OOM, no assertion failure |
| Incremental re-parse | Custom harness | Random edits on real source files | parse(full) = parse_incremental(previous, edit) |
| Lexer | `cargo fuzz` | Arbitrary bytes | No panic, spans are valid |
| Query engine | `cargo fuzz` | Random query strings + random parse trees | No panic, matches are valid |
| Grammar compilation | `cargo fuzz` | Mutated grammar JSON | No crash during table generation |
| Concurrency | `loom` | Thread interleavings on shared tree access | No data races, consistent tree |

**Key fuzz: incremental re-parse identity**. This is the most important test in
the entire project:

```
for _ in 0..N:
    source = random_real_source(language)
    tree_a = parse(source)

    // Apply a random edit
    start      = random_byte_offset(source)
    old_end    = random_byte_offset_after(start, source)
    new_text   = random_bytes()

    edit = InputEdit {
        start_byte:     start,
        old_end_byte:   old_end,
        new_end_byte:   start + len(new_text),
        // row/column fields computed from source + start/old_end
    }

    source_b = source[..start] + new_text + source[old_end..]
    tree_b_full = parse(source_b)                         // full re-parse (ground truth)
    tree_b_incr = parse_incremental(tree_a, edit, source_b)  // incremental

    assert full_trees_equal(tree_b_full, tree_b_incr)
```

The fuzzer should find the minimal counterexample where `full != incr` --
that's a bug in the incremental logic.

### Tier 4 -- Performance benchmarks (per-release)

Criterion benchmarks in a standalone crate, never run in CI (too noisy), gated
on release tagging.

```rust
fn bench_full_parse(c: &mut Criterion) {
    let source = include_str!("corpus/python/large.py");
    c.group("python")
        .bench_function("full_parse", |b| b.iter(|| {
            Parser::new(PYTHON).parse(source.as_bytes())
        }))
        .bench_function("incremental_reparse", |b| b.iter(|| {
            let tree = Parser::new(PYTHON).parse(source.as_bytes()).unwrap();
            let edited = edit_at_line(&source, 42, "    return x + 1\n");
            Parser::new(PYTHON).parse_incremental(&tree, edited.as_bytes())
        }));
}
```

| Metric | Target | Regression threshold |
|--------|--------|---------------------|
| Cold parse, 10K LOC | <= tree-sitter C x 1.5 | > 2x -> block merge |
| Incremental re-parse, single-line edit | <= 100 us | > 200 us -> block merge |
| Incremental re-parse, 50% file replaced | <= cold parse | N/A (monitor only) |
| Query, 20 patterns on 10K LOC | <= 5 ms | > 10 ms -> flag |
| Peak memory, 10K LOC Python | <= 50 MB | > 100 MB -> flag |
| Throughput, large JSON (100 MB) | >= 100 MB/s | < 50 MB/s -> block merge |
| Compile grammar from `.json` | <= 200 ms | > 1 s -> flag |

All benchmarks run on a reference machine (GitHub Actions `ubuntu-24.04-arm`,
4 vCPU). Results are published to a dashboard.

### Tier 5 -- Long-running stability (pre-release)

| Test | Duration | What it validates |
|------|----------|-------------------|
| Memory leak soak | 24 h parse loop on Python stdlib | RSS stable, no growth |
| Editor simulation | 1 h of random edits in a virtual buffer | Incremental re-parse never diverges from full |
| Concurrent read harness | 8 threads querying a shared tree for 1 h | No races, no panics |
| Hanging indent stress | 10K rapid edits in Python file | Lexer indentation stack doesn't drift |

### Cross-cutting gap: release & CI process

The roadmap defines test tiers but has no release process or CI
infrastructure plan. Without this, the project will accumulate regressions
and have no path to crates.io.

**Required CI gates (on every PR):**
1. `cargo test` -- all unit and integration tests (Tier 1), must pass
2. `cargo clippy --all-targets -- -D warnings` -- no new warnings
3. `cargo build --no-default-features` on `glr-core`, `glr-engine`,
   `glr-lexer` -- verify `no_std` compat (target `x86_64-unknown-none`)
4. `cargo test --doc` -- all doc examples compile and run
5. `cargo semver-checks` -- detect accidental breaking changes

**Release automation (per release):**
- Tag with `vX.Y.Z` following semver
- Changelog generated from conventional commits (or maintained manually
  in `CHANGELOG.md`)
- Publish all workspace crates in dependency order
- Run Tier 2 (conformance) and Tier 4 (benchmark) suites; results published
  to a dashboard
- After release, file a PR to the `grammars/` repo bumping the pinned
  grammar versions

**Grammar ecosystem**: The workspace currently has a `grammars/` directory
with vendored grammar JSON. The roadmap should specify whether grammars
are published as their own crates (`tree-sitter-javascript` equivalents),
bundled into the `glr` workspace, or fetched at build time. Recommend:
publish each ported grammar as `glr-grammar-<lang>` on crates.io, with a
`glr-grammars` meta-crate that re-exports all of them by feature flag.

## Why this is hard

1. **epsilon-rules in GLR** -- Productions that reduce to the empty string (epsilon) cause
   naive GLR implementations to loop infinitely. The RNGLR algorithm solves
   this correctly, but it is subtle to implement. This is phase-0 difficulty
   and must be designed in from the start, not bolted on later.

2. **GLR ambiguity handling** -- The Shared Packed Parse Forest (SPPF) correctly
   represents all parse trees for ambiguous inputs without duplicating shared
   subtrees. Tree-sitter chooses a different approach: it picks one parse tree
   using dynamic precedence heuristics rather than returning all parses. The
   Rust version must decide: SPPF (correct but complex, like rustemo) or
   single-tree with heuristics (simpler, like tree-sitter). **Recommendation**:
   support SPPF internally, but provide a `resolve_ambiguity()` API that
   applies tree-sitter-compatible heuristics for users who want a single tree.

3. **Incremental re-parse** -- Tree-sitter's incremental algorithm is the result
   of ~5 years of iteration. The edge cases (zero-byte edits, multi-byte
   characters, huge deletions) are numerous.

4. **External scanners** -- Python's indentation, Bash's heredocs, Ruby's
   `%w(...)` literals -- these are hand-written C that depends on parser
   internals. Porting them to Rust is straightforward but tedious.

5. **Grammar compatibility** -- Thousands of existing `.so` parsers and `.scm`
   queries must keep working. Any format change breaks the ecosystem.

6. **Performance** -- C's `goto`-based state machine dispatcher is hard to beat.
   Rust's `match` is close, but LLVM inlining thresholds and aliasing analysis
   matter at this level. Expect 1.5-2x C performance initially; optimizing to
   parity is a separate project phase.

---

## Related work -- existing Rust parser ecosystem

These projects already exist but **none** cover the full niche of incremental
GLR parsing with a query engine and tree-sitter grammar compatibility:

| Project | Type | Stars (~2026) | Has GLR? | Incremental? | Query engine? | Works with existing grammars? |
|---------|------|-------|----------|-------------|---------------|-------------------------------|
| **lalrpop** | LR(1) proc-macro | ~3.5k | LALR(1) opt | No | No | No (own DSL) |
| **pest** | PEG proc-macro | ~5k | No (PEG) | No | No | No (own `.pest` files) |
| **rust-peg** | PEG proc-macro | ~1.6k | No (PEG) | No | No | No (own DSL) |
| **grmtools/lrpar** | LR/GLR build.rs | ~574 | **Yes** (GLR mode) | No | No | Yacc `.y` files |
| **rustemo** | LR/GLR build.rs | ~47 | **Yes** (RNGLR + SPPF) | No | No | Own grammar DSL |
| **RustyLR** | IELR(1)/LALR(1)/GLR proc-macro | ~27 | **Yes** | No | No | Own bison-like DSL |
| **glr-parser** (crate) | GLR generator | <100 | **Yes** | No | No | No |
| **tree-sitter** | C GLR library | ~25.8k (Rust+C) | Yes | **Yes** | **Yes** | Yes (200+ grammars) |

> **Note**: tree-sitter's main repo is now listed as "Written in: Rust, C" --
> the CLI and bindings are in Rust, but the runtime library (the part that
> actually parses) is still C11. The most recent stable release at time of
> writing is v0.26.9 (May 2026). See Appendix B for ABI version details.
> The star count for the combined
> tree-sitter ecosystem (grammar repos included) is far higher.

**grmtools / lrpar** is the closest pure-Rust GLR foundation: GLR mode,
Yacc grammar support, well-maintained by a university research group. It lacks
incremental re-parse, has no query/highlight system, and doesn't consume
tree-sitter grammar JSON. A potential foundation to build on -- or a fork point.

**rustemo** is also relevant: it implements the RNGLR algorithm (Right-Nulled
GLR, which correctly handles epsilon-rules without Tomita's edge cases), produces a
Shared Packed Parse Forest (SPPF), and is actively developed. It explicitly
lists incremental parsing and tree-sitter-style error recovery as future goals.
This is probably the closest pure-Rust engine to build on for Phase 0.

**RustyLR** provides IELR(1)/LALR(1)/GLR via proc-macros with a
bison-inspired DSL and good diagnostics. Solid option for the macro layer in
Phase 2.

### tree-sitter project's own 1.0 roadmap

tree-sitter's issue [#930](https://github.com/tree-sitter/tree-sitter/issues/930)
lays out their 1.0 goals. Two items are relevant here:

1. **WASM parser loading** -- Stretch goal to compile parsers to WASM and load
   them via wasmtime. The parse table stays native, only lexing runs in WASM.
   This is essentially a pragmatic hybrid that the tree-sitter team itself
   considers the right path forward.

2. **CLI ergonomics** -- Generate Rust bindings from grammars, structure
   Node.js bindings consistently. No Rust rewrite is planned or discussed.

**Conclusion**: The upstream tree-sitter project is not planning a Rust
runtime rewrite. Their preferred long-term path is WASM-compiled grammars
loaded via wasmtime, which is a pragmatic hybrid. **This project is not
competing with tree-sitter** -- it is filling the gap for use cases where a C
toolchain or WASM runtime is not acceptable (bare metal, `no_std`, strict
supply-chain environments, or projects that simply want 100% Rust).

---

## Alternatives to a full rewrite

| Approach | Effort | Keeps C? | Rust% of codebase |
|----------|--------|----------|-------------------|
| Full Rust rewrite (from scratch) | 24-30 PM | No | 100% |
| Fork rustemo, add incremental + queries + tree-sitter JSON | 10-15 PM | No | 100% |
| Fork grmtools/lrpar, add incremental + queries | 12-18 PM | No | 100% |
| Vendored C (current common approach) | 1 PM | Yes | ~95% |
| Bindings to tree-sitter .so | 0.5 PM | Yes (runtime) | ~95% |
| WASM-compiled tree-sitter grammars | 2 PM | No (WASM) | ~99% |

The **WASM approach** (upstream's chosen direction): compile tree-sitter C
grammars to `.wasm` with `wasm32-unknown-unknown`, load at runtime via
wasmtime. Parse table stays native (fast), only lexing runs in WASM. Incurs
WASM call overhead on every external scanner token (~10-40% slower depending on
scanner complexity -- the 10% figure is optimistic for scanner-heavy grammars
like Python). Still requires a WASM runtime as a dependency.

**Recommended hybrid for fastest time-to-value**: use rustemo as the GLR
engine (it has RNGLR + SPPF already working), implement incremental re-parse
on top (the hard part), and write a tree-sitter JSON -> rustemo grammar
converter. This reuses the most existing Rust infrastructure. Roughly 10-15 PM
vs 24-30 for a total rewrite.

**Crate name note**: The name `glr` is already taken on crates.io (a dormant
2022 RNGLR parser generator crate by axelf4, last updated Sept 2022).
It is also worth noting `glr-parser` (2015, dormant) is taken. Among the
alternative names suggested below, `treacle` and `arborist` are also taken on
crates.io (unrelated projects). `glrust` and `sylvan` are available.
Reserve the name early.

---

## Appendix A: Key tree-sitter source files to study

All files are from the main tree-sitter repository:
<https://github.com/tree-sitter/tree-sitter/tree/master/lib>

| File | Lines (approx.) | Purpose |
|------|-------|---------|
| `src/parser.c` | ~2,000-3,000 | Core GLR engine (has grown significantly from the original ~800 lines) |
| `src/language.c` | ~500 | Language loading, serialization |
| `src/lexer.c` | ~600-800 | Built-in lexer logic |
| `src/node.c` | ~400 | Tree node API |
| `src/query.c` | ~3,000-4,000 | Query engine (substantially larger than the ~2,000 line figure often cited) |
| `src/alloc.h` | ~50 | Arena allocator |
| `src/subtree.c` | ~800 | Subtree management (immutable node storage) |
| `src/stack.c` | ~600 | Graph-structured stack implementation |
| `include/tree_sitter/parser.h` | ~200 | API for grammar C code |

> **Critical note**: The roadmap's original `~800 lines` figure for `parser.c`
> refers to tree-sitter circa 2018. The current codebase is substantially
> larger, and `stack.c` and `subtree.c` are now separate files from
> `parser.c` -- the split is important to understand before starting Phase 0.
> Do a fresh `git clone` and count lines before writing the design doc.

## Appendix B: Grammar JSON schema

The `grammar.json` file in each grammar repo follows a well-defined schema.
Before Phase 2, fetch the canonical schema from:

- <https://raw.githubusercontent.com/tree-sitter/tree-sitter/master/cli/src/generate/grammar_schema.json>
- Inspect any grammar repo: <https://github.com/tree-sitter/tree-sitter-javascript/blob/master/grammar.json>

Key fields to understand: `rules`, `precedences`, `conflicts`, `extras`,
`word_token`, `inline`, `supertypes`.

> **ABI version warning -- critical for the compiler (Phase 2.2)**:
> As of tree-sitter v0.25.0 (early 2025), the default generated ABI is **15**
> (up from 14). ABI 15 adds language name, version, supertype info, and
> reserved word tables directly into the parser struct. It also requires a
> `tree-sitter.json` metadata file in the grammar repo alongside `grammar.json`.
> The current latest release is **v0.26.9** (May 2026).
>
> The grammar.json compiler in Phase 2.2 must handle **both ABI 14 and ABI 15**
> grammars -- many community grammars are still ABI 14, and the ecosystem is
> split. ABI 14 remains a supported target via `tree-sitter generate --abi=14`.
>
> When writing the conformance test runner, use `tree-sitter = "0.26.9"` (or
> latest) in `Cargo.toml`. As of June 2026, the tree-sitter master branch has
> moved to **v0.27.0** with MSRV **1.90** and Rust edition **2024** --
> the conformance test must pin a stable version rather than tracking master.

## Appendix C: Glossary

| Term | Definition |
|------|------------|
| **GLR** | Generalized LR -- an LR parsing algorithm that handles ambiguous grammars by maintaining multiple parse "forks" simultaneously via a Graph-Structured Stack |
| **RNGLR** | Right-Nulled GLR -- a variant of GLR (Scott & Johnstone, 2006) that correctly handles epsilon-rules without looping. The recommended algorithm for this project |
| **GSS** | Graph-Structured Stack -- a DAG data structure GLR uses to share common stack prefixes across multiple parse paths. Not just parallel stacks -- it is a graph where nodes are `(parser_state, input_position)` pairs and edges carry the reduced subtree. Two parse heads that reach the same `(state, position)` share a single GSS node rather than duplicating work. |
| **SPPF** | Shared Packed Parse Forest -- a compact graph that represents all possible parse trees for an ambiguous input, sharing identical subtrees. Returned by full GLR parsers; tree-sitter instead resolves to a single tree |
| **Shift** | LR action: consume the current token and push a new GSS node with the target state |
| **Reduce** | LR action: traverse GSS edges backward N steps (matching the RHS of a production), then push a new GSS node for the LHS nonterminal via the GOTO table |
| **Accept** | LR action: parsing complete |
| **Fork** | A single active parse path (stack head) in the GLR GSS; forks split at shift/reduce or reduce/reduce conflicts and merge when they converge on the same GSS node |
| **State** | An LR automaton state (from the parse table); identifies a set of possible LR items (partially-matched productions + positions) |
| **Symbol** | Either a terminal (token) or nonterminal (rule LHS) |
| **Production** | A grammar rule: `Nonterminal -> Symbol1 Symbol2 ... Symboln` |
| **epsilon-rule / epsilon-rule** | A production with an empty right-hand side: `Nonterminal -> epsilon`. These cause infinite loops in naive GLR; RNGLR handles them correctly |
| **Parse table** | Two tables: ACTION `[state x terminal -> shift/reduce/accept/error]` and GOTO `[state x nonterminal -> new_state]`. Often stored merged |
| **External scanner** | Hand-written code (C in tree-sitter, Rust in our version) that handles lexing tokens that can't be expressed as regular expressions (indentation, heredocs, etc.) |
| **Incremental re-parse** | Re-parsing after an edit by reusing unchanged subtrees from the previous parse, yielding O(edit size) amortized time |
| **InputEdit** | A descriptor `{ start_byte, old_end_byte, new_end_byte, start_point, old_end_point, new_end_point }` that describes a single text change for incremental re-parse |
| **ERROR node** | A tree node inserted when the parser encounters invalid syntax, allowing the parse to continue and produce a full tree |
| **Query** | A pattern expression (`.scm` file) that matches nodes in a parse tree, used for syntax highlighting and code analysis |
| **Named node** | A node that corresponds to a named rule in the grammar (as opposed to anonymous literal tokens like `"+"` or `";"`) |
| **Extra** | A token declared in `extras` (usually whitespace/comments) that may appear anywhere in the input without being part of any production |

---

## Appendix D: Edge cases -- complete catalogue

This appendix exists because edge cases are where GLR parsers diverge silently
from correct behavior. Every item here must have a corresponding test before
that subsystem ships.

### GLR engine edge cases

| Case | Why it is hard | Required test |
|------|---------------|---------------|
| epsilon-rule in the start production (`S -> epsilon`) | Parser must accept empty input and produce a tree with a single empty node | `parse("")` -> tree with `root.start_byte == root.end_byte == 0` |
| Deeply nested right-recursion (`A -> "a" A \| epsilon`) | If reductions are processed recursively (DFS on the GSS), right-recursion causes O(n) recursive calls -> stack overflow. Must use an iterative worklist for reductions. | Input of 100,000 `"a"` tokens -- no stack overflow, worklist stays bounded |
| Deeply nested left-recursion (`A -> A "a" \| epsilon`) | GSS must share nodes; naive impl is O(n^2) space | Input of 100,000 `"a"` tokens -- RSS bounded |
| Reduce/reduce conflict | Two different rules reduce to different nonterminals at the same position | Both reductions are tried; tree matches whichever has higher dynamic precedence |
| Shift/reduce conflict | Both a shift and a reduce are valid | Both are tried; the result is the same as tree-sitter's for the same grammar |
| Max fork limit | Pathological ambiguous grammar can produce exponential forks | Parser applies fork count limit and reports error rather than OOM |
| Repeated epsilon-reduce cycle | GLR step could loop on `A -> epsilon; B -> A; A -> B` | Fixed-point detection terminates after one full pass |
| Unicode in identifiers | Token `start_byte` != `start_col` when input has multi-byte chars | `"let pi = 3"` -- identifier node byte-span covers the 2-byte `pi` correctly |

### Lexer edge cases

| Case | Why it is hard | Required test |
|------|---------------|---------------|
| Overlapping regex alternatives | DFA must prefer longest match, then declared priority | `"return"` lexed as keyword, not identifier, when both rules match |
| Keyword vs identifier | `word` token in grammar.json controls this | `"for_each"` lexed as identifier (not the keyword `for`) |
| External scanner and built-in scanner conflict | Both could claim the same input prefix | `valid_symbols` must correctly disable one |
| Zero-width tokens | Tokens that match empty string (e.g. INDENT/DEDENT) | Python: `"if True:\n  pass"` produces INDENT at column 2 position with `start_byte == end_byte` for the INDENT token |
| Scanner state not serialized | Incremental re-parse restores stale scanner state | After edit, scanner produces same INDENT/DEDENT sequence as full re-lex |
| Input ending mid-token | Incomplete token at EOF | `parse("\"unterminated`)` -> ERROR node covering the unclosed string |

### Incremental re-parse edge cases

| Case | Why it is hard | Required test |
|------|---------------|---------------|
| Edit at byte 0 | First node of tree must be invalidated | Replace `"fn"` with `"pub fn"` at offset 0; tree reflects `pub` keyword |
| Edit spans multiple nodes | Multiple subtrees invalidated simultaneously | Replace 3 lines; all 3 old nodes gone, new nodes correct |
| Edit shrinks a token to zero bytes | Node kind may change | Replace `"=="` with `"="` -- assignment, not comparison |
| Edit produces identical text | Zero-change edit -- tree must be identical | `old[3..5] = old[3..5]` -- tree bitwise-equal to original |
| Consecutive edits before re-parse | Client applies multiple `InputEdit`s without calling `parse_incremental` | Must accumulate edits; final tree is same as parsing result from scratch on final text |
| Edit in a comment | Comment is an `extra`; surrounding non-comment nodes are unaffected | Edit inside `// comment` -> only comment node changes |
| Edit crosses a scanner state boundary | Scanner was in state S1 before edit, new text changes it to S2 | Python: inserting a blank line between two indented blocks must re-scan INDENT/DEDENT from that line |

### Query engine edge cases

| Case | Why it is hard | Required test |
|------|---------------|---------------|
| Pattern matches zero times | `(identifier)* @ids` on empty input | `ids` capture set is empty, not an error |
| Nested capture | `(function (identifier) @name) @fn` | Both captures are returned for the same match |
| Predicate on missing node | `(#eq? @name "foo")` when `@name` is a MISSING node | Predicate returns false (not a crash) |
| Anchor at end of child list | `(. (identifier) @last .)` -- anchors | Only matches identifier that is the last named child |
| Overlapping matches | Two patterns match at the same byte range | Both matches returned; iteration order is document order |
| Pattern on ERROR node | Query `(ERROR) @err` | Matches all error nodes in the tree |

### Error recovery edge cases

| Case | Required behavior |
|------|-------------------|
| Entire file is invalid syntax | Tree root is ERROR; no panic |
| ERROR node at start of file | First token consumed into ERROR; remaining file parses normally if possible |
| ERROR node at end of file | Unclosed delimiter produces ERROR spanning to EOF |
| Nested ERROR nodes | ERROR inside ERROR -- allowed; queries must handle this |
| ERROR node in incremental re-parse | Old ERROR node may be resolved by new text or expanded -- must not leave dangling pointers |
