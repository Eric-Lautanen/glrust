use glr_core::parse_table::{ParseTable, ParseTableEntry};
use glr_core::symbol::{Symbol, SymbolKind};
use glr_core::{Grammar, ProductionId, StateId, SymbolId};
use glr_lexer::{Lexer, Token};
use std::vec::Vec;

pub struct TestGrammarBuilder {
    symbols: Vec<(String, SymbolKind)>,
    productions: Vec<(u32, Vec<u32>, i32)>,
}

impl TestGrammarBuilder {
    pub fn new() -> Self {
        Self {
            symbols: Vec::new(),
            productions: Vec::new(),
        }
    }

    pub fn terminal(&mut self, name: &str) -> u32 {
        let id = self.symbols.len() as u32;
        self.symbols.push((name.to_string(), SymbolKind::Terminal));
        id
    }

    pub fn nonterminal(&mut self, name: &str) -> u32 {
        let id = self.symbols.len() as u32;
        self.symbols
            .push((name.to_string(), SymbolKind::NonTerminal));
        id
    }

    pub fn production(&mut self, nt: u32, rhs: Vec<u32>, prec: i32) {
        self.productions.push((nt, rhs, prec));
    }

    pub fn build(&self) -> Grammar {
        let symbol_count = self.symbols.len() as u32;
        let syms: Vec<Symbol> = self
            .symbols
            .iter()
            .enumerate()
            .map(|(i, (name, kind))| Symbol {
                id: SymbolId(i as u32),
                name: name.clone(),
                kind: *kind,
            })
            .collect();

        let token_count = self
            .symbols
            .iter()
            .filter(|(_, k)| matches!(k, SymbolKind::Terminal))
            .count() as u32;

        let productions: Vec<_> = self
            .productions
            .iter()
            .enumerate()
            .map(|(i, (nt, rhs, prec))| glr_core::grammar::Production {
                id: ProductionId(i as u16),
                nonterminal: SymbolId(*nt),
                symbols: rhs.iter().map(|&s| SymbolId(s)).collect(),
                dynamic_precedence: *prec,
            })
            .collect();

        let mut state_count = 1u32;
        let mut large_entries = Vec::new();

        for (prod_idx, (nt, rhs, _)) in self.productions.iter().enumerate() {
            for (pos, &sym) in rhs.iter().enumerate() {
                let is_last = pos + 1 == rhs.len();
                let state = state_count;
                state_count += 1;

                for s in 0..symbol_count {
                    let entry = if s == sym as u32 {
                        if is_last {
                            ParseTableEntry::Reduce {
                                symbol: SymbolId(s),
                                child_count: rhs.len() as u16,
                                dynamic_precedence: 0,
                                production_id: prod_idx as u16,
                            }
                        } else {
                            ParseTableEntry::Shift {
                                state: StateId(state),
                            }
                        }
                    } else if s == *nt && is_last {
                        ParseTableEntry::Goto {
                            state: StateId(state),
                        }
                    } else {
                        ParseTableEntry::Error
                    };
                    large_entries.push(entry);
                }
            }
        }

        let table = ParseTable {
            symbol_count,
            state_count,
            large_state_count: state_count,
            large_entries,
            small_states: Vec::new(),
        };

        Grammar {
            version: 14,
            symbol_count,
            alias_count: 0,
            token_count,
            external_token_count: 0,
            state_count,
            large_state_count: state_count,
            production_id_count: productions.len() as u32,
            field_count: 0,
            max_alias_sequence_length: 0,
            symbols: syms,
            productions,
            parse_table: table,
        }
    }
}

pub struct TestLexer<'a> {
    source: &'a [u8],
    cursor: u32,
    token_map: Vec<(u32, &'a [u8])>,
}

impl<'a> TestLexer<'a> {
    pub fn new(source: &'a [u8], token_map: Vec<(u32, &'a [u8])>) -> Self {
        Self {
            source,
            cursor: 0,
            token_map,
        }
    }
}

impl<'a> Lexer for TestLexer<'a> {
    fn next_token(&mut self, _valid_symbols: &[bool]) -> Option<Token> {
        let len = self.source.len() as u32;
        if self.cursor >= len {
            return None;
        }
        while self.cursor < len && self.source[self.cursor as usize].is_ascii_whitespace() {
            self.cursor += 1;
        }
        if self.cursor >= len {
            return None;
        }

        let remaining = &self.source[self.cursor as usize..];
        for &(sym_id, literal) in &self.token_map {
            if remaining.starts_with(literal) {
                let start = self.cursor;
                self.cursor += literal.len() as u32;
                return Some(Token {
                    kind: SymbolId(sym_id),
                    start_byte: start,
                    end_byte: self.cursor,
                });
            }
        }

        let start = self.cursor;
        self.cursor += 1;
        Some(Token {
            kind: SymbolId(0),
            start_byte: start,
            end_byte: self.cursor,
        })
    }

    fn cursor(&self) -> u32 {
        self.cursor
    }

    fn reset_to(&mut self, byte_offset: u32) {
        self.cursor = byte_offset;
    }
}
