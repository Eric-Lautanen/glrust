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
    #[must_use]
    pub fn single(entry: ParseTableEntry) -> Self {
        Self {
            entries: vec![entry],
        }
    }

    #[must_use]
    pub fn multiple(entries: Vec<ParseTableEntry>) -> Self {
        Self { entries }
    }

    #[must_use]
    pub fn is_error(&self) -> bool {
        // A cell is only a pure error if every entry in it is Error.
        // A GLR-conflicted cell that contains Error alongside a Shift or
        // Reduce is not an error cell — it has at least one valid action.
        !self.entries.is_empty() && self.entries.iter().all(|e| *e == ParseTableEntry::Error)
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
    #[must_use]
    pub fn lookup(&self, state: StateId, symbol: SymbolId) -> &[ParseTableEntry] {
        let s = state.0 as usize;
        let sym = symbol.0 as usize;
        if s < self.large_state_count as usize {
            let idx = s * self.symbol_count as usize + sym;
            self.large_entries
                .get(idx)
                .map_or(&[], |a| a.entries.as_slice())
        } else {
            let small_idx = s - self.large_state_count as usize;
            self.small_states
                .get(small_idx)
                .and_then(|row| row.lookup(sym))
                .map_or(&[], |a| a.entries.as_slice())
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SmallStateRow {
    /// Entries sorted ascending by symbol id. The sort invariant is required
    /// by `lookup`, which uses `binary_search_by_key`. Always construct via
    /// `SmallStateRow::new` rather than building the `entries` field directly.
    pub entries: Vec<(u32, ParseTableAction)>,
}

impl SmallStateRow {
    /// Build a `SmallStateRow` from an unsorted list of `(symbol_id, action)`
    /// pairs. Duplicate symbol ids are not checked here — the grammar builder
    /// is responsible for merging GLR conflicts into a single
    /// `ParseTableAction::multiple` entry before calling this.
    #[must_use]
    pub fn new(mut entries: Vec<(u32, ParseTableAction)>) -> Self {
        entries.sort_unstable_by_key(|&(s, _)| s);
        Self { entries }
    }

    #[must_use]
    pub fn lookup(&self, symbol: usize) -> Option<&ParseTableAction> {
        let Ok(i) = self
            .entries
            .binary_search_by_key(&u32::try_from(symbol).unwrap_or(u32::MAX), |&(s, _)| s)
        else {
            return None;
        };
        Some(&self.entries[i].1)
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
