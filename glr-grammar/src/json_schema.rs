use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct GrammarJson {
    pub name: String,
    pub rules: HashMap<String, Rule>,
    pub extras: Option<Vec<Rule>>,
    pub conflicts: Option<Vec<Vec<String>>>,
    pub precedences: Option<Vec<Vec<String>>>,
    pub externals: Option<Vec<Rule>>,
    pub inline: Option<Vec<String>>,
    pub supertypes: Option<Vec<String>>,
    pub word: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Rule {
    /// Token string literal (shorthand for `{"type": "STRING", "value": "..."}`)
    String(String),
    /// Nonterminal or terminal symbol reference (shorthand for `{"type": "SYMBOL", "name": "..."}`)
    Symbol(String),
    /// Regex pattern: `{"type": "PATTERN", "value": "..."}`
    Pattern { pattern: String },
    /// Sequence: `{"type": "SEQ", "members": [...]}`
    Seq { members: Vec<Rule> },
    /// Choice / alternation: `{"type": "CHOICE", "members": [...]}`
    Choice { members: Vec<Rule> },
    /// Zero-or-more repetition: `{"type": "REPEAT", "content": ...}`
    Repeat { content: Box<Rule> },
    /// Named field: `{"type": "FIELD", "name": "...", "content": ...}`
    Field { name: String, content: Box<Rule> },
    /// Precedence: `{"type": "PREC", "value": N, "content": ...}`
    Prec { value: i32, content: Box<Rule> },
    /// Left-associative precedence: `{"type": "PREC_LEFT", "value": N, "content": ...}`
    PrecLeft { value: i32, content: Box<Rule> },
    /// Right-associative precedence: `{"type": "PREC_RIGHT", "value": N, "content": ...}`
    PrecRight { value: i32, content: Box<Rule> },
    /// Dynamic precedence: `{"type": "PREC_DYNAMIC", "value": N, "content": ...}`
    PrecDynamic { value: i32, content: Box<Rule> },
    /// Alias: `{"type": "ALIAS", "value": ..., "named": bool, "content": ...}`
    Alias { value: Box<Rule>, named: bool, content: Box<Rule> },
    /// Token (lexical rule): `{"type": "TOKEN", "content": ...}`
    Token { content: Box<Rule> },
    /// Immediate token: `{"type": "IMMEDIATE_TOKEN", "content": ...}`
    ImmediateToken { content: Box<Rule> },
    /// Catch-all for unrecognized rule types (BLANK, etc.)
    Unknown(serde_json::Value),
}

impl Rule {
    pub fn is_named(&self) -> bool {
        matches!(self, Rule::Symbol(_) | Rule::Choice { .. } | Rule::Seq { .. })
    }
}
