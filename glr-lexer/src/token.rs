use glr_core::SymbolId;

/// A token produced by the lexer.
#[derive(Debug, Clone)]
pub struct Token {
    pub kind: SymbolId,
    pub start_byte: usize,
    pub end_byte: usize,
}

/// Trait for lexers consumed by the GLR parser.
pub trait Lexer {
    /// Advance past the next token. Returns `None` at EOF.
    fn next_token(&mut self, valid_symbols: &[bool]) -> Option<Token>;

    /// Current byte offset in the source.
    fn cursor(&self) -> usize;

    /// Reset to a specific byte offset.
    fn reset_to(&mut self, byte_offset: usize);
}
