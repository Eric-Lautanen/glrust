use crate::{LexError, Lexer, Token};
#[cfg(feature = "regex")]
use alloc::collections::BTreeMap;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use glr_core::dfa::{DfaState, DfaTable, DfaTransition};
use glr_core::position_at;
use glr_core::SymbolId;

/// Extension trait providing DfaTable construction
/// (requires access to `DfaBuilder`).
pub trait DfaTableExt {
    fn from_literals(literals: &[(SymbolId, &[u8])]) -> Self;
    fn from_literals_and_patterns(
        literals: &[(SymbolId, &[u8])],
        patterns: &[(SymbolId, &str)],
    ) -> Self;
}

impl DfaTableExt for DfaTable {
    fn from_literals(literals: &[(SymbolId, &[u8])]) -> Self {
        let mut builder = DfaBuilder::new();
        for &(kind, literal) in literals {
            builder.add_literal(literal, kind);
        }
        builder.build()
    }

    fn from_literals_and_patterns(
        literals: &[(SymbolId, &[u8])],
        patterns: &[(SymbolId, &str)],
    ) -> Self {
        let mut builder = DfaBuilder::new();
        for &(kind, literal) in literals {
            builder.add_literal(literal, kind);
        }
        let mut dfa = builder.build();
        dfa.patterns = patterns
            .iter()
            .map(|&(id, pat)| (id, pat.to_string()))
            .collect();
        dfa
    }
}

/// Internal builder that constructs a DFA from literal tokens via a trie.
#[derive(Debug, Clone)]
struct DfaBuilder {
    states: Vec<DfaState>,
}

impl DfaBuilder {
    fn new() -> Self {
        Self {
            states: vec![DfaState {
                transitions: Vec::new(),
                accept: None,
            }],
        }
    }

    fn alloc_state(&mut self) -> u32 {
        let id = u32::try_from(self.states.len()).expect("DFA state count exceeds u32");
        self.states.push(DfaState {
            transitions: Vec::new(),
            accept: None,
        });
        id
    }

    fn add_literal(&mut self, literal: &[u8], kind: SymbolId) {
        let mut state_id = 0u32;
        for &byte in literal {
            let next = self.states[state_id as usize]
                .transitions
                .iter()
                .position(|t| t.start == byte && t.end == byte)
                .map(|i| self.states[state_id as usize].transitions[i].next_state);
            if let Some(id) = next {
                state_id = id;
            } else {
                let new_id = self.alloc_state();
                self.states[state_id as usize]
                    .transitions
                    .push(DfaTransition {
                        start: byte,
                        end: byte,
                        next_state: new_id,
                    });
                state_id = new_id;
            }
        }
        self.states[state_id as usize].accept = Some(kind);
    }

    fn build(mut self) -> DfaTable {
        for state in &mut self.states {
            state.transitions.sort_unstable_by_key(|t| (t.start, t.end));
            #[cfg(debug_assertions)]
            for w in state.transitions.windows(2) {
                debug_assert!(
                    w[0].end < w[1].start || w[0].start > w[1].end,
                    "overlapping DFA transitions: [{}, {}] and [{}, {}]",
                    w[0].start,
                    w[0].end,
                    w[1].start,
                    w[1].end,
                );
            }
        }
        DfaTable {
            states: self.states,
            patterns: Vec::new(),
        }
    }
}

/// A table-driven DFA lexer.
///
/// Reads source bytes, advances through the DFA, and emits tokens with
/// longest-match semantics. Tracks row/column position incrementally.
#[derive(Debug)]
pub struct BuiltinLexer<'src> {
    source: &'src [u8],
    cursor: u32,
    row: u32,
    col: u32,
    dfa: &'src DfaTable,
    last_error: LexError,
    #[cfg(feature = "regex")]
    regex_cache: BTreeMap<String, Option<regex::bytes::Regex>>,
}

impl<'src> BuiltinLexer<'src> {
    #[must_use]
    pub fn new(source: &'src [u8], dfa: &'src DfaTable) -> Self {
        Self {
            source,
            cursor: 0,
            row: 0,
            col: 0,
            dfa,
            last_error: LexError::Eof,
            #[cfg(feature = "regex")]
            regex_cache: BTreeMap::new(),
        }
    }

    /// Return the total length of the source in bytes.
    #[must_use]
    pub fn source_len(&self) -> u32 {
        u32::try_from(self.source.len()).expect("source length exceeds u32")
    }

    #[must_use]
    pub fn source(&self) -> &'src [u8] {
        self.source
    }

    /// Advance the cursor by `n` bytes, tracking row/column.
    pub fn advance(&mut self, n: u32) {
        let max = u32::try_from(self.source.len()).expect("source length exceeds u32");
        let actual = n.min(max.saturating_sub(self.cursor));
        let start = self.cursor as usize;
        let end = (self.cursor + actual) as usize;
        for &b in &self.source[start..end] {
            if b == b'\n' {
                self.row += 1;
                self.col = 0;
            } else {
                self.col += 1;
            }
        }
        self.cursor = (self.cursor + actual).min(max);
    }

    fn position_at(&self, byte_offset: u32) -> (u32, u32) {
        if byte_offset == self.cursor {
            return (self.row, self.col);
        }
        position_at(self.source, byte_offset)
    }

    fn match_patterns_cached(&mut self, start: u32) -> Option<(u32, SymbolId)> {
        #[cfg(feature = "regex")]
        {
            let mut best_match: Option<(u32, SymbolId)> = None;
            for &(sym_id, ref pat_str) in &self.dfa.patterns {
                let re = self
                    .regex_cache
                    .entry(pat_str.clone())
                    .or_insert_with(|| regex::bytes::Regex::new(pat_str).ok());
                if let Some(ref re) = re {
                    if let Some(m) = re.find_at(self.source, start as usize) {
                        if m.start() == start as usize {
                            let end = u32::try_from(m.end()).expect("match end exceeds u32::MAX");
                            let is_better = best_match.is_none_or(|(best_end, _)| end > best_end);
                            if is_better {
                                best_match = Some((end, sym_id));
                            }
                        }
                    }
                }
            }
            best_match
        }
        #[cfg(not(feature = "regex"))]
        {
            let _ = start;
            None
        }
    }
}

impl Lexer for BuiltinLexer<'_> {
    fn next_token(&mut self, valid_symbols: &[bool]) -> Option<Token> {
        let len = u32::try_from(self.source.len()).expect("source length exceeds u32");

        // Handle BOM only at position 0.
        if self.cursor == 0
            && self.cursor + 3 <= len
            && self.source[0] == 0xEF
            && self.source[1] == 0xBB
            && self.source[2] == 0xBF
        {
            self.advance(3);
        }

        // Skip ASCII whitespace.
        while self.cursor < len && self.source[self.cursor as usize].is_ascii_whitespace() {
            self.advance(1);
        }

        if self.cursor >= len {
            self.last_error = LexError::Eof;
            return None;
        }

        let start = self.cursor;
        let start_pos = (self.row, self.col);
        let mut pos = start;
        let mut state: u32 = 0;
        let mut last_accept: Option<(u32, SymbolId)> = None;

        while pos < len {
            let byte = self.source[pos as usize];
            let dfa_state = &self.dfa.states[state as usize];

            if let Some(kind) = dfa_state.accept {
                last_accept = Some((pos, kind));
            }

            match dfa_state.next(byte) {
                Some(next) => {
                    state = next;
                    pos += 1;
                }
                None => break,
            }
        }

        if pos == len {
            if let Some(kind) = self.dfa.states[state as usize].accept {
                last_accept = Some((pos, kind));
            }
        }

        if let Some((end, kind)) = last_accept {
            let sym_id = kind.0 as usize;
            if sym_id < valid_symbols.len() && !valid_symbols[sym_id] {
                let end_pos = self.position_at(end);
                self.cursor = end;
                self.row = end_pos.0;
                self.col = end_pos.1;
                return Some(Token {
                    kind: SymbolId::UNKNOWN,
                    start_byte: start,
                    end_byte: end,
                    start_position: start_pos,
                    end_position: end_pos,
                });
            }
            let end_pos = self.position_at(end);
            self.cursor = end;
            self.row = end_pos.0;
            self.col = end_pos.1;
            Some(Token {
                kind,
                start_byte: start,
                end_byte: end,
                start_position: start_pos,
                end_position: end_pos,
            })
        } else if let Some((end, kind)) = self.match_patterns_cached(start) {
            let end_pos = self.position_at(end);
            self.cursor = end;
            self.row = end_pos.0;
            self.col = end_pos.1;
            Some(Token {
                kind,
                start_byte: start,
                end_byte: end,
                start_position: start_pos,
                end_position: end_pos,
            })
        } else {
            self.advance(1);
            let end_pos = (self.row, self.col);
            Some(Token {
                kind: SymbolId::UNKNOWN,
                start_byte: start,
                end_byte: self.cursor,
                start_position: start_pos,
                end_position: end_pos,
            })
        }
    }

    fn cursor(&self) -> u32 {
        self.cursor
    }

    fn last_lex_error(&self) -> LexError {
        self.last_error
    }

    fn reset_to(&mut self, byte_offset: u32) {
        let pos = self.position_at(byte_offset);
        self.cursor = byte_offset;
        self.row = pos.0;
        self.col = pos.1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DfaTableExt;
    use glr_core::DfaTable;

    #[test]
    fn literal_lexer_basic() {
        let literals = &[
            (SymbolId(1), b"if" as &[u8]),
            (SymbolId(2), b"then"),
            (SymbolId(3), b"else"),
        ];
        let dfa = DfaTable::from_literals(literals);
        let source = b"if then else";
        let mut lex = BuiltinLexer::new(source, &dfa);

        let t = lex.next_token(&[]).unwrap();
        assert_eq!(t.kind, SymbolId(1));
        assert_eq!(t.start_byte, 0);
        assert_eq!(t.end_byte, 2);

        let t = lex.next_token(&[]).unwrap();
        assert_eq!(t.kind, SymbolId(2));
        assert_eq!(t.start_byte, 3);
        assert_eq!(t.end_byte, 7);

        let t = lex.next_token(&[]).unwrap();
        assert_eq!(t.kind, SymbolId(3));
        assert_eq!(t.start_byte, 8);
        assert_eq!(t.end_byte, 12);

        assert!(lex.next_token(&[]).is_none());
    }

    #[test]
    fn longest_match() {
        let literals = &[(SymbolId(1), b"if" as &[u8]), (SymbolId(2), b"ifx")];
        let dfa = DfaTable::from_literals(literals);
        let source = b"ifx";
        let mut lex = BuiltinLexer::new(source, &dfa);
        let t = lex.next_token(&[]).unwrap();
        assert_eq!(t.kind, SymbolId(2));
    }

    #[test]
    fn empty_input() {
        let dfa = DfaTable::from_literals(&[(SymbolId(1), b"a" as &[u8])]);
        let source = b"";
        let mut lex = BuiltinLexer::new(source, &dfa);
        assert!(lex.next_token(&[]).is_none());
    }

    #[test]
    fn unknown_byte_fallback() {
        let dfa = DfaTable::from_literals(&[(SymbolId(1), b"abc" as &[u8])]);
        let source = b"xyz";
        let mut lex = BuiltinLexer::new(source, &dfa);
        let t = lex.next_token(&[]).unwrap();
        assert_eq!(t.kind, SymbolId::UNKNOWN);
        assert_eq!(t.start_byte, 0);
        assert_eq!(t.end_byte, 1);
    }

    #[test]
    fn multi_byte_utf8_transitions() {
        let literals = &[
            (SymbolId(1), "/*".as_bytes()),
            (SymbolId(2), "→".as_bytes()),
        ];
        let dfa = DfaTable::from_literals(literals);
        let source = "/* → */".as_bytes();
        let mut lex = BuiltinLexer::new(source, &dfa);

        let t = lex.next_token(&[]).unwrap();
        assert_eq!(t.kind, SymbolId(1));
        assert_eq!(t.start_byte, 0);
        assert_eq!(t.end_byte, 2);

        let t = lex.next_token(&[]).unwrap();
        assert_eq!(t.kind, SymbolId(2));
        assert_eq!(
            &source[t.start_byte as usize..t.end_byte as usize],
            "→".as_bytes()
        );
        assert_eq!(t.end_byte - t.start_byte, 3);
    }

    #[test]
    fn max_token_length() {
        let size = 10 * 1024 * 1024;
        let mut long = alloc::vec![0u8; size];
        long[0] = b'x';
        long[1..size - 1].fill(b'a');
        long[size - 1] = b'y';
        let literals = &[(SymbolId(1), &long[..])];
        let dfa = DfaTable::from_literals(literals);

        let source = &long[..];
        let mut lex = BuiltinLexer::new(source, &dfa);
        let t = lex.next_token(&[]).unwrap();
        assert_eq!(t.kind, SymbolId(1));
        assert_eq!(t.start_byte, 0);
        assert_eq!(t.end_byte, size as u32);
    }

    #[test]
    fn contiguous_spans() {
        let literals = &[(SymbolId(1), b"a" as &[u8]), (SymbolId(2), b"b")];
        let dfa = DfaTable::from_literals(literals);
        let source = b"a b a";
        let mut lex = BuiltinLexer::new(source, &dfa);

        let mut prev_end = 0u32;
        while let Some(t) = lex.next_token(&[]) {
            assert!(t.start_byte >= prev_end, "spans must not overlap");
            assert!(t.start_byte <= t.end_byte, "spans must be ordered");
            prev_end = t.end_byte;
        }
        assert!(prev_end <= source.len() as u32);
    }

    #[test]
    fn bom_skipped_at_start() {
        let literals = &[(SymbolId(1), b"hello" as &[u8])];
        let dfa = DfaTable::from_literals(literals);
        let source = b"\xEF\xBB\xBFhello";
        let mut lex = BuiltinLexer::new(source, &dfa);
        let t = lex.next_token(&[]).unwrap();
        assert_eq!(t.kind, SymbolId(1));
        assert_eq!(t.start_byte, 3);
        assert_eq!(t.end_byte, 8);
    }

    #[test]
    fn valid_symbols_filtering() {
        let literals = &[(SymbolId(1), b"a" as &[u8]), (SymbolId(2), b"b")];
        let dfa = DfaTable::from_literals(literals);
        let source = b"a b";
        let mut lex = BuiltinLexer::new(source, &dfa);

        let valid = &[false, false, true];
        let t = lex.next_token(valid).unwrap();
        assert_eq!(t.kind, SymbolId::UNKNOWN);
        assert_eq!(t.start_byte, 0);
        assert_eq!(t.end_byte, 1);
    }
}
