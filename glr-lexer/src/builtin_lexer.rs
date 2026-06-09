use glr_core::SymbolId;

/// Table-driven lexer generated from grammar definitions.
pub struct BuiltinLexer<'src> {
    pub source: &'src [u8],
    pub cursor: usize,
}

impl<'src> BuiltinLexer<'src> {
    pub fn new(source: &'src [u8]) -> Self {
        Self { source, cursor: 0 }
    }

    /// Advance past the next token. Returns `None` at EOF.
    pub fn next_token(&mut self, _valid_symbols: &[bool]) -> Option<Token> {
        // TODO: Phase 1.1 — implement table-driven DFA lexer
        unimplemented!("BuiltinLexer — see Phase 1.1 of the roadmap")
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn reset_to(&mut self, byte_offset: usize) {
        self.cursor = byte_offset;
    }
}

/// A single token produced by the lexer.
#[derive(Debug, Clone)]
pub struct Token {
    pub kind: SymbolId,
    pub start_byte: usize,
    pub end_byte: usize,
}
