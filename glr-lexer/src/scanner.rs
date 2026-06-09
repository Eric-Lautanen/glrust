use crate::{BuiltinLexer, ExternalScanner, LexError, Lexer, Token};
use glr_core::position_at;
use glr_core::SymbolId;

#[derive(Debug)]
pub struct CompositeLexer<'src, S: ExternalScanner> {
    builtin: BuiltinLexer<'src>,
    external: Option<S>,
    internal_token_count: u32,
    last_error: LexError,
}

impl<'src, S: ExternalScanner> CompositeLexer<'src, S> {
    pub fn new(
        builtin: BuiltinLexer<'src>,
        external: Option<S>,
        internal_token_count: u32,
    ) -> Self {
        Self {
            builtin,
            external,
            internal_token_count,
            last_error: LexError::Eof,
        }
    }
}

impl<S: ExternalScanner> Lexer for CompositeLexer<'_, S> {
    fn next_token(&mut self, valid_symbols: &[bool]) -> Option<Token> {
        if let Some(tok) = self.builtin.next_token(valid_symbols) {
            return Some(tok);
        }

        if self.builtin.cursor() >= self.builtin.source_len() {
            self.last_error = LexError::Eof;
            return None;
        }

        if let Some(ref mut scanner) = self.external {
            let cursor = self.builtin.cursor();
            let mut ext_cursor = cursor;
            let start_pos = position_at(self.builtin.source(), cursor);

            let ext_count = valid_symbols
                .len()
                .saturating_sub(self.internal_token_count as usize);
            let ext_valid: alloc::vec::Vec<bool> = (0..ext_count)
                .map(|i| {
                    let idx = self.internal_token_count as usize + i;
                    idx < valid_symbols.len() && valid_symbols[idx]
                })
                .collect();

            if let Some(sym_id) = scanner.scan(self.builtin.source(), &mut ext_cursor, &ext_valid) {
                let span_len = ext_cursor - cursor;
                self.builtin.advance(span_len);
                let end_pos = position_at(self.builtin.source(), ext_cursor);
                return Some(Token {
                    kind: sym_id,
                    start_byte: cursor,
                    end_byte: ext_cursor,
                    start_position: start_pos,
                    end_position: end_pos,
                });
            }
        }

        let cursor = self.builtin.cursor();
        let start_pos = position_at(self.builtin.source(), cursor);
        self.builtin.advance(1);
        let end_pos = position_at(self.builtin.source(), cursor + 1);
        Some(Token {
            kind: SymbolId::UNKNOWN,
            start_byte: cursor,
            end_byte: cursor + 1,
            start_position: start_pos,
            end_position: end_pos,
        })
    }

    fn cursor(&self) -> u32 {
        self.builtin.cursor()
    }

    fn last_lex_error(&self) -> LexError {
        self.last_error
    }

    fn reset_to(&mut self, byte_offset: u32) {
        self.builtin.reset_to(byte_offset);
    }

    fn serialize_state(&self, buffer: &mut [u8]) -> usize {
        if let Some(ref scanner) = self.external {
            scanner.serialize(buffer)
        } else {
            0
        }
    }

    fn deserialize_state(&mut self, buffer: &[u8]) {
        if let Some(ref mut scanner) = self.external {
            scanner.deserialize(buffer);
        }
    }
}
