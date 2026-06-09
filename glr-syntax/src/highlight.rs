use glr_core::Grammar;
use glr_query::Query;

/// Highlight result: a byte range with a highlight name.
#[derive(Debug, Clone)]
pub struct HighlightRange {
    pub start_byte: usize,
    pub end_byte: usize,
    pub name: String,
}

/// Run the highlight pipeline, returning an iterator of highlight ranges.
pub fn highlight<'src>(
    _source: &'src [u8],
    _grammar: &Grammar,
    _queries: &[Query],
) -> impl Iterator<Item = HighlightRange> + 'src {
    // TODO: Phase 3.2 — execute highlight queries and produce ranges
    std::iter::empty()
}
