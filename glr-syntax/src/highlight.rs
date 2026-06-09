use glr_core::Grammar;
use glr_engine::Parser;
use glr_query::{Query, Queryable};

/// Highlight result: a byte range with a highlight name.
#[derive(Debug, Clone)]
pub struct HighlightRange {
    pub start_byte: usize,
    pub end_byte: usize,
    pub name: String,
}

/// Run the highlight pipeline, returning an iterator of highlight ranges.
///
/// For each query in `queries` (earlier = higher priority), all matches are
/// collected. Overlapping ranges are resolved so that higher-priority ranges
/// take precedence – their portions are kept and the overlapping parts of
/// lower-priority ranges are trimmed away. The final ranges are sorted by
/// start byte.
pub fn highlight<'src>(
    source: &'src [u8],
    grammar: &Grammar,
    queries: &[Query],
) -> impl Iterator<Item = HighlightRange> + 'src {
    let parser = Parser::new(grammar.clone());
    let tree = parser.parse(source);

    // Collect (priority, range) pairs. Priority is encoded as a single u64:
    //   high 32 bits = inverted query index (earlier query = higher priority)
    //   low  32 bits = pattern_index within the query
    let count = queries.len();
    let mut entries: Vec<(u64, HighlightRange)> = Vec::new();

    for (qi, query) in queries.iter().enumerate() {
        let base = ((count - 1 - qi) as u64) << 32;
        for qm in tree.query(query) {
            let prio = base | (qm.pattern_index as u64);
            for &(ci, ref node) in &qm.captures {
                if let Some(cap) = query.captures.get(ci) {
                    entries.push((
                        prio,
                        HighlightRange {
                            start_byte: node.start_byte as usize,
                            end_byte: node.end_byte as usize,
                            name: cap.name.clone(),
                        },
                    ));
                }
            }
        }
    }

    // Sort by priority descending (highest first), then start byte ascending.
    entries.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.start_byte.cmp(&b.1.start_byte)));

    // Resolve overlaps: higher-priority ranges take precedence.
    // Each incoming range is trimmed against all already-resolved ranges.
    let mut resolved: Vec<HighlightRange> = Vec::new();

    for (_, range) in entries {
        let mut segments = vec![(range.start_byte, range.end_byte)];
        for r in &resolved {
            let mut next: Vec<(usize, usize)> = Vec::new();
            for (s, e) in segments {
                if e <= r.start_byte || s >= r.end_byte {
                    next.push((s, e));
                } else {
                    if s < r.start_byte {
                        next.push((s, r.start_byte));
                    }
                    if e > r.end_byte {
                        next.push((r.end_byte, e));
                    }
                }
            }
            segments = next;
            if segments.is_empty() {
                break;
            }
        }
        for (s, e) in segments {
            resolved.push(HighlightRange {
                start_byte: s,
                end_byte: e,
                name: range.name.clone(),
            });
        }
    }

    resolved.sort_by_key(|a| a.start_byte);
    resolved.into_iter()
}
