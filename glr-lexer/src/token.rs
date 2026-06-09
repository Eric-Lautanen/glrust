use glr_core::SymbolId;

/// A token produced by the lexer.
#[derive(Debug, Clone)]
pub struct Token {
    pub kind: SymbolId,
    pub start_byte: u32,
    pub end_byte: u32,
}

/// The reason `next_token` returned `None`.
///
/// Distinguishing EOF from a lex error matters for error recovery: at EOF the
/// engine should run its end-of-input reduction phase; on a lex error it should
/// attempt panic-mode recovery and keep scanning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexError {
    /// The input was fully consumed — no more tokens will ever be produced.
    Eof,
    /// No rule matched at the current position. The lexer's cursor has *not*
    /// advanced; the engine is responsible for skipping forward and retrying.
    NoMatch,
}

/// Trait for lexers consumed by the GLR parser.
pub trait Lexer {
    /// Advance past the next token and return it, or return `None` with the
    /// reason stored in `last_error()` (see below).
    ///
    /// `valid_symbols` is a bitmask-as-slice: index `i` is `true` if symbol
    /// `SymbolId(i as u32)` is a valid lookahead at the current parse state.
    /// The slice length is guaranteed by the engine to equal
    /// `Grammar::symbol_count`; implementations may assume this and index
    /// directly without bounds-checking each entry.
    ///
    /// Returning `None` must set the value returned by `last_lex_error` so
    /// callers can distinguish EOF from a failed match.
    fn next_token(&mut self, valid_symbols: &[bool]) -> Option<Token>;

    /// The reason the most recent `next_token` call returned `None`.
    /// Undefined if `next_token` has not yet been called or if it last
    /// returned `Some`.
    fn last_lex_error(&self) -> LexError;

    /// Current byte offset in the source (the position the *next* call to
    /// `next_token` will scan from).
    fn cursor(&self) -> u32;

    /// Reposition the lexer to `byte_offset`.
    ///
    /// - May seek both forward and backward within the source.
    /// - Invalidates any state accumulated since the last `next_token` call
    ///   (e.g. partially-matched rule state machines must be reset).
    /// - Does **not** invalidate previously returned `Token` values, which are
    ///   plain data.
    /// - Calling `reset_to` with the value returned by `cursor()` is a no-op.
    fn reset_to(&mut self, byte_offset: u32);
}
