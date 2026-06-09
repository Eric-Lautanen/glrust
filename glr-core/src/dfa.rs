use crate::SymbolId;
use alloc::string::String;
use alloc::vec::Vec;
use core::cmp::Ordering;

/// A single byte-range transition in a DFA state.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DfaTransition {
    pub start: u8,
    pub end: u8,
    pub next_state: u32,
}

/// A state in the lexer DFA.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DfaState {
    pub transitions: Vec<DfaTransition>,
    pub accept: Option<SymbolId>,
}

impl DfaState {
    #[must_use]
    pub fn next(&self, byte: u8) -> Option<u32> {
        let idx = self
            .transitions
            .binary_search_by(|t| {
                if byte < t.start {
                    Ordering::Greater
                } else if byte > t.end {
                    Ordering::Less
                } else {
                    Ordering::Equal
                }
            })
            .ok()?;
        Some(self.transitions[idx].next_state)
    }
}

/// A compiled DFA table for tokenization.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DfaTable {
    pub states: Vec<DfaState>,
    pub patterns: Vec<(SymbolId, String)>,
}

impl DfaTable {
    #[must_use]
    pub fn new(states: Vec<DfaState>) -> Self {
        Self {
            states,
            patterns: Vec::new(),
        }
    }
}
