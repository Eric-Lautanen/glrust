#![cfg_attr(not(feature = "std"), no_std)]
#![deny(unsafe_code)]
//! Core data structures for GLR parsing: [`Grammar`], [`ParseTable`], [`Symbol`],
//! [`Tree`], and newtype ids ([`StateId`], [`SymbolId`], [`ProductionId`]).

#[cfg(feature = "serde")]
extern crate serde;

extern crate alloc;

pub mod dfa;
pub mod grammar;
pub mod parse_table;
pub mod symbol;
pub mod tree;

pub use dfa::{DfaState, DfaTable, DfaTransition};
pub use grammar::{Grammar, Production, GRAMMAR_MAGIC};
pub use parse_table::{ParseTable, ParseTableAction, ParseTableEntry, SmallStateRow};
pub use symbol::{Symbol, SymbolKind};
pub use tree::{InternalNode, MutableTree, Node, NodeId, TextSpan, Tree, TreeCursor};

/// Compute (row, column) position from byte offset in source bytes.
/// Falls back to linear scan when no precomputed line starts are provided.
/// Column counting uses Unicode width when the `std` feature is enabled.
#[must_use]
pub fn position_at(source: &[u8], offset: u32) -> (u32, u32) {
    position_at_impl(source, offset)
}

/// Compute row/column using precomputed line-start offsets for O(log N)
/// row lookup. Pass an empty slice to fall back to linear scan.
#[must_use]
pub fn position_at_with_lines(source: &[u8], offset: u32, line_starts: &[u32]) -> (u32, u32) {
    if line_starts.is_empty() {
        return position_at_impl(source, offset);
    }
    let line_idx = match line_starts.binary_search(&offset) {
        Ok(i) => return (i as u32, 0),
        Err(i) if i > 0 => i - 1,
        _ => 0,
    };
    let line_start = line_starts[line_idx];
    (line_idx as u32, column_offset(source, line_start as usize, offset as usize))
}

fn column_offset(source: &[u8], start: usize, end: usize) -> u32 {
    let mut col = 0u32;
    let mut i = start;
    let limit = end.min(source.len());
    while i < limit {
        let b = source[i];
        if b == b'\n' {
            break;
        } else if b & 0x80 == 0 {
            col += 1;
            i += 1;
        } else {
            let len = utf8_byte_length(b);
            if i + len > source.len() {
                col += 1;
                break;
            }
            #[cfg(feature = "std")]
            {
                if let Ok(s) = core::str::from_utf8(&source[i..i + len]) {
                    if let Some(ch) = s.chars().next() {
                        col += unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1) as u32;
                    } else {
                        col += 1;
                    }
                } else {
                    col += 1;
                }
            }
            #[cfg(not(feature = "std"))]
            {
                let _ = len;
                col += 1;
            }
            i += len;
        }
    }
    col
}

fn position_at_impl(source: &[u8], offset: u32) -> (u32, u32) {
    let mut row = 0u32;
    let mut line_start = 0usize;
    let limit = (offset as usize).min(source.len());
    for (i, &b) in source.iter().enumerate().take(limit) {
        if b == b'\n' {
            row += 1;
            line_start = i + 1;
        }
    }
    let col = column_offset(source, line_start, limit);
    (row, col)
}

fn utf8_byte_length(first: u8) -> usize {
    if first & 0x80 == 0 {
        1
    } else if first & 0xE0 == 0xC0 {
        2
    } else if first & 0xF0 == 0xE0 {
        3
    } else if first & 0xF8 == 0xF0 {
        4
    } else {
        1
    }
}

/// Newtype wrapper around a `u32` identifying an LR parser state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StateId(pub u32);

/// Newtype wrapper around a `u32` identifying a grammar symbol (terminal or
/// non-terminal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SymbolId(pub u32);

impl SymbolId {
    /// Sentinel symbol id used for error-token nodes in the parse tree.
    pub const ERROR: Self = SymbolId(u32::MAX);
    /// Sentinel symbol id used for unknown/fallback tokens by the lexer.
    pub const UNKNOWN: Self = SymbolId(u32::MAX - 1);
}

/// Newtype wrapper around a `u16` identifying a production rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ProductionId(pub u16);

/// Describes a single edit operation on source text, used for incremental
/// re-parsing.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct InputEdit {
    pub start_byte: u32,
    pub old_end_byte: u32,
    pub new_end_byte: u32,
    pub start_point: Point,
    pub old_end_point: Point,
    pub new_end_point: Point,
}

/// A row/column position in source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Point {
    pub row: u32,
    pub column: u32,
}

// ---------------------------------------------------------------------------
// Thread safety model
// ---------------------------------------------------------------------------
//
// `Tree` is `Send + Sync` when using `Arc` (default). The `rc` feature
// switches to `Rc`, which is neither `Send` nor `Sync` — this is
// intentional for single-threaded use where atomic ref-counting overhead
// is undesirable.
//
// `Node` is an owned type (no arena reference) — it is always `Send + Sync`.
// `Grammar`, `ParseTable`, `MutableTree` are all owned types and are
// `Send + Sync`.
// `TreeCursor<'a>` borrows the `Tree` arena and inherits `Send + Sync` from
// the `&'a [Node]` reference.
// `Parser` (in `glr-engine`) holds only a `Grammar` — it is `Clone` and
// `Send + Sync`. The tree-sitter API guarantee that `ts_parser_parse` can
// run on any thread as long as no two threads call it on the same parser
// applies equally here.
//
// Compile-time assertions for the default (thread-safe) configuration:
#[cfg(all(test, not(feature = "rc")))]
mod send_sync_assertions {
    use super::*;

    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}

    #[test]
    fn tree_is_send_sync() {
        assert_send::<Tree>();
        assert_sync::<Tree>();
    }

    #[test]
    fn node_is_send_sync() {
        assert_send::<Node>();
        assert_sync::<Node>();
    }

    #[test]
    fn grammar_is_send_sync() {
        assert_send::<Grammar>();
        assert_sync::<Grammar>();
    }

    #[test]
    fn parse_table_is_send_sync() {
        assert_send::<ParseTable>();
        assert_sync::<ParseTable>();
    }

    #[test]
    fn mutable_tree_is_send_sync() {
        assert_send::<MutableTree>();
        assert_sync::<MutableTree>();
    }
}
