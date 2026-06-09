#![deny(unsafe_code)]
pub mod compile;
pub mod json_schema;
pub mod table_gen;

pub use compile::compile_grammar;
use glr_core::Grammar;
use glr_core::GRAMMAR_MAGIC;

/// Serialize a `Grammar` to bytes with a magic header.
/// Format: `b"GLRG"` + `format_version` (u32 LE) + JSON bytes.
pub fn serialize_grammar(grammar: &Grammar) -> Vec<u8> {
    let json = serde_json::to_vec(grammar).expect("Grammar serialization failed");
    let mut data = Vec::with_capacity(8 + json.len());
    data.extend_from_slice(&GRAMMAR_MAGIC);
    data.extend_from_slice(&grammar.format_version.to_le_bytes());
    data.extend_from_slice(&json);
    data
}

/// Deserialize a `Grammar` from bytes with magic header validation.
/// Returns `None` on missing/incorrect magic or deserialization failure.
pub fn deserialize_grammar(data: &[u8]) -> Option<Grammar> {
    if data.len() < 8 {
        return None;
    }
    if data[..4] != GRAMMAR_MAGIC {
        return None;
    }
    serde_json::from_slice(&data[8..]).ok()
}
