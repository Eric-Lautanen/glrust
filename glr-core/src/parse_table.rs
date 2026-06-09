use crate::{StateId, SymbolId};
use alloc::vec::Vec;

/// Wraps one or more parse-table entries for a single (state, symbol) cell.
/// Most cells contain a single entry; GLR-conflicted cells may have multiple.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ParseTableAction {
    pub entries: Vec<ParseTableEntry>,
}

impl ParseTableAction {
    pub fn single(entry: ParseTableEntry) -> Self {
        Self {
            entries: vec![entry],
        }
    }

    pub fn multiple(entries: Vec<ParseTableEntry>) -> Self {
        Self { entries }
    }

    pub fn is_error(&self) -> bool {
        self.entries.len() == 1 && self.entries[0] == ParseTableEntry::Error
    }
}

impl From<ParseTableEntry> for ParseTableAction {
    fn from(e: ParseTableEntry) -> Self {
        Self::single(e)
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ParseTable {
    pub symbol_count: u32,
    pub state_count: u32,
    pub large_state_count: u32,
    pub large_entries: Vec<ParseTableAction>,
    pub small_states: Vec<SmallStateRow>,
}

impl ParseTable {
    pub fn lookup(&self, state: StateId, symbol: SymbolId) -> &[ParseTableEntry] {
        let s = state.0 as usize;
        let sym = symbol.0 as usize;
        if s < self.large_state_count as usize {
            let idx = s * self.symbol_count as usize + sym;
            self.large_entries
                .get(idx)
                .map(|a| a.entries.as_slice())
                .unwrap_or(&[])
        } else {
            let small_idx = s - self.large_state_count as usize;
            self.small_states
                .get(small_idx)
                .and_then(|row| row.lookup(sym))
                .map(|a| a.entries.as_slice())
                .unwrap_or(&[])
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SmallStateRow {
    pub entries: Vec<(u32, ParseTableAction)>,
}

impl SmallStateRow {
    pub fn lookup(&self, symbol: usize) -> Option<&ParseTableAction> {
        self.entries
            .binary_search_by_key(&(symbol as u32), |&(s, _)| s)
            .ok()
            .map(|i| &self.entries[i].1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ParseTableEntry {
    Shift {
        state: StateId,
    },
    Reduce {
        symbol: SymbolId,
        child_count: u16,
        dynamic_precedence: i32,
        production_id: u16,
    },
    Goto {
        state: StateId,
    },
    Accept,
    Error,
}
