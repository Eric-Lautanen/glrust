use crate::{StateId, SymbolId};
use alloc::vec::Vec;

/// The LR parse table, split into large (dense) and small (sparse) states.
///
/// Lookup: for a given state `s` and symbol `sym`:
/// - If `s < large_state_count`: index into `large_entries` at offset
///   `s * symbol_count + sym`
/// - Otherwise: binary search `small_states[s - large_state_count]`
#[derive(Debug, Clone)]
pub struct ParseTable {
    pub symbol_count: u32,
    pub state_count: u32,
    pub large_state_count: u32,
    pub large_entries: Vec<ParseTableEntry>,
    pub small_states: Vec<SmallStateRow>,
}

impl ParseTable {
    pub fn lookup(&self, state: StateId, symbol: SymbolId) -> ParseTableEntry {
        let s = state.0 as usize;
        let sym = symbol.0 as usize;

        if s < self.large_state_count as usize {
            let idx = s * self.symbol_count as usize + sym;
            self.large_entries
                .get(idx)
                .copied()
                .unwrap_or(ParseTableEntry::Error)
        } else {
            let small_idx = s - self.large_state_count as usize;
            self.small_states
                .get(small_idx)
                .and_then(|row| row.lookup(sym))
                .unwrap_or(ParseTableEntry::Error)
        }
    }
}

/// A sparse row for small states: sorted `(symbol, action)` pairs.
#[derive(Debug, Clone)]
pub struct SmallStateRow {
    pub entries: Vec<(u32, ParseTableEntry)>,
}

impl SmallStateRow {
    fn lookup(&self, symbol: usize) -> Option<ParseTableEntry> {
        self.entries
            .binary_search_by_key(&(symbol as u32), |&(s, _)| s)
            .ok()
            .map(|i| self.entries[i].1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseTableEntry {
    Shift { state: StateId },
    Reduce { symbol: SymbolId, child_count: u16, dynamic_precedence: i32, production_id: u16 },
    Goto { state: StateId },
    Accept,
    Error,
}
