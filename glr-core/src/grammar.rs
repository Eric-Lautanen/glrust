use crate::dfa::DfaTable;
use crate::parse_table::ParseTable;
use crate::position_at_with_lines;
use crate::symbol::{Symbol, SymbolKind};
use crate::{ProductionId, StateId, SymbolId};
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Grammar {
    pub format_version: u32,
    pub version: u32,
    pub min_compatible_version: u32,
    pub symbol_count: u32,
    pub alias_count: u32,
    pub token_count: u32,
    pub external_token_count: u32,
    pub state_count: u32,
    pub large_state_count: u32,
    pub production_id_count: u32,
    pub field_count: u32,
    pub max_alias_sequence_length: u32,
    pub symbols: Vec<Symbol>,
    pub productions: Vec<Production>,
    pub parse_table: ParseTable,
    pub fields: Vec<String>,
    pub supertypes: Vec<String>,
    pub word_token: Option<String>,
    pub word_symbol_id: Option<SymbolId>,
    pub precedence_groups: Vec<Vec<String>>,
    pub conflict_decls: Vec<Vec<String>>,
    pub reserved: Vec<(String, Vec<String>)>,
    pub dfa_table: DfaTable,
    pub line_start_offsets: Vec<u32>,
}

impl Grammar {
    /// Precompute line-start byte offsets from source bytes.
    #[must_use]
    pub fn compute_line_starts(source: &[u8]) -> Vec<u32> {
        let mut offsets = Vec::new();
        offsets.push(0);
        for (i, &b) in source.iter().enumerate() {
            if b == b'\n' {
                offsets.push(u32::try_from(i + 1).expect("source too large"));
            }
        }
        offsets
    }

    /// Look up (row, col) for a byte offset using precomputed line starts.
    #[must_use]
    pub fn position_of(&self, source: &[u8], offset: u32) -> (u32, u32) {
        position_at_with_lines(source, offset, &self.line_start_offsets)
    }

    #[must_use]
    pub fn symbol_name(&self, id: SymbolId) -> &str {
        self.symbols
            .get(id.0 as usize)
            .map_or("<unknown>", |s| s.name.as_str())
    }

    #[must_use]
    pub fn production(&self, id: ProductionId) -> Option<&Production> {
        self.productions.get(id.0 as usize)
    }

    #[must_use]
    pub fn valid_symbols(&self, state: StateId) -> Vec<bool> {
        let count = self.symbol_count as usize;
        let mut valid = alloc::vec![false; count];
        self.fill_valid_symbols(state, &mut valid);
        valid
    }

    /// Fill a pre-allocated buffer with valid symbols for the given state.
    /// The buffer length must equal `self.symbol_count as usize`.
    pub fn fill_valid_symbols(&self, state: StateId, buf: &mut [bool]) {
        for (i, sym) in self.symbols.iter().enumerate() {
            if sym.kind != SymbolKind::Terminal {
                continue;
            }
            let actions = self
                .parse_table
                .lookup(state, SymbolId(u32::try_from(i).unwrap()));
            let has_valid = actions
                .iter()
                .any(|a| !matches!(a, crate::parse_table::ParseTableEntry::Error));
            buf[i] = has_valid;
        }
    }

    #[must_use]
    pub fn field_name(&self, field_id: u16) -> &str {
        self.fields
            .get(field_id as usize)
            .map_or("<unknown>", |s| s.as_str())
    }

    #[must_use]
    pub fn word_symbol(&self) -> Option<SymbolId> {
        self.word_symbol_id.or_else(|| {
            self.word_token.as_ref().and_then(|name| {
                let id = self
                    .symbols
                    .iter()
                    .position(|s| s.name == *name)
                    .map(|i| SymbolId(u32::try_from(i).unwrap()));
                id
            })
        })
    }
}

/// Magic header bytes for serialized grammar files.
pub const GRAMMAR_MAGIC: [u8; 4] = *b"GLRG";

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Production {
    pub id: ProductionId,
    pub nonterminal: SymbolId,
    pub symbols: Vec<SymbolId>,
    pub dynamic_precedence: i32,
    pub field_map: Vec<(u16, u16)>,
    pub alias_map: Vec<(u16, SymbolId, bool)>,
}
