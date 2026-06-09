/// Trait for hand-written external scanners (indentation, heredocs, etc.).
///
/// Mirrors tree-sitter's C external scanner API exactly.
pub trait ExternalScanner {
    /// Attempt to scan the next external token.
    ///
    /// `valid_symbols` is a bitmask slice indexed by external token id.
    /// The scanner MUST check this before scanning.
    fn scan(&mut self, source: &[u8], cursor: &mut usize, valid_symbols: &[bool]) -> bool;

    /// Serialize scanner state to bytes (max 1024 bytes).
    fn serialize(&self, buffer: &mut [u8]) -> usize;

    /// Restore scanner state from bytes.
    fn deserialize(&mut self, buffer: &[u8]);

    /// Create a new scanner instance.
    fn create() -> Self
    where
        Self: Sized;
}
