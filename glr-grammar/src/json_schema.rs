use serde::de::{self, Deserializer};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

/// Grammar as serialized by `tree-sitter generate` into `grammar.json`.
///
/// Matches tree-sitter's `parse_grammar.rs` `GrammarJSON` struct.
#[derive(Debug, Deserialize)]
pub struct GrammarJson {
    pub name: String,
    pub rules: HashMap<String, Rule>,
    #[serde(default)]
    pub extras: Vec<Rule>,
    #[serde(default)]
    pub conflicts: Vec<Vec<String>>,
    #[serde(default)]
    pub precedences: Vec<Vec<String>>,
    #[serde(default)]
    pub externals: Vec<Rule>,
    #[serde(default)]
    pub inline: Vec<String>,
    #[serde(default)]
    pub supertypes: Vec<String>,
    #[serde(default)]
    pub word: Option<String>,
    #[serde(default)]
    pub reserved: HashMap<String, Vec<Rule>>,
}

/// Precedence value: either a string name or a number.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum PrecedenceValue {
    Named(String),
    Number(i32),
}

/// A grammar rule, matching tree-sitter's `RuleJSON` enum from
/// `parse_grammar.rs` with an `Unknown` catch-all for unrecognized types.
#[derive(Debug, Clone)]
pub enum Rule {
    Alias {
        content: Box<Rule>,
        named: bool,
        value: String,
    },
    Blank,
    String {
        value: String,
    },
    Pattern {
        value: String,
        flags: Option<String>,
    },
    Symbol {
        name: String,
    },
    Choice {
        members: Vec<Rule>,
    },
    Field {
        name: String,
        content: Box<Rule>,
    },
    Seq {
        members: Vec<Rule>,
    },
    Repeat {
        content: Box<Rule>,
    },
    Repeat1 {
        content: Box<Rule>,
    },
    Prec {
        value: PrecedenceValue,
        content: Box<Rule>,
    },
    PrecLeft {
        value: PrecedenceValue,
        content: Box<Rule>,
    },
    PrecRight {
        value: PrecedenceValue,
        content: Box<Rule>,
    },
    PrecDynamic {
        value: i32,
        content: Box<Rule>,
    },
    Token {
        content: Box<Rule>,
    },
    ImmediateToken {
        content: Box<Rule>,
    },
    Reserved {
        context_name: String,
        content: Box<Rule>,
    },
    Unknown(Value),
}

impl Rule {
    #[must_use]
    pub fn is_named(&self) -> bool {
        matches!(
            self,
            Rule::Symbol { .. } | Rule::Choice { .. } | Rule::Seq { .. }
        )
    }
}

/// Internal tagged enum matching tree-sitter's `RuleJSON` exactly.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum RuleTagged {
    Alias {
        content: Box<RuleTagged>,
        named: bool,
        value: String,
    },
    Blank,
    String {
        value: String,
    },
    Pattern {
        value: String,
        flags: Option<String>,
    },
    Symbol {
        name: String,
    },
    Choice {
        members: Vec<RuleTagged>,
    },
    Field {
        name: String,
        content: Box<RuleTagged>,
    },
    Seq {
        members: Vec<RuleTagged>,
    },
    Repeat {
        content: Box<RuleTagged>,
    },
    Repeat1 {
        content: Box<RuleTagged>,
    },
    Prec {
        value: PrecedenceValue,
        content: Box<RuleTagged>,
    },
    PrecLeft {
        value: PrecedenceValue,
        content: Box<RuleTagged>,
    },
    PrecRight {
        value: PrecedenceValue,
        content: Box<RuleTagged>,
    },
    PrecDynamic {
        value: i32,
        content: Box<RuleTagged>,
    },
    Token {
        content: Box<RuleTagged>,
    },
    ImmediateToken {
        content: Box<RuleTagged>,
    },
    Reserved {
        context_name: String,
        content: Box<RuleTagged>,
    },
}

fn convert_tagged(t: RuleTagged) -> Rule {
    match t {
        RuleTagged::Alias {
            content,
            named,
            value,
        } => Rule::Alias {
            content: Box::new(convert_tagged(*content)),
            named,
            value,
        },
        RuleTagged::Blank => Rule::Blank,
        RuleTagged::String { value } => Rule::String { value },
        RuleTagged::Pattern { value, flags } => Rule::Pattern { value, flags },
        RuleTagged::Symbol { name } => Rule::Symbol { name },
        RuleTagged::Choice { members } => Rule::Choice {
            members: members.into_iter().map(convert_tagged).collect(),
        },
        RuleTagged::Field { name, content } => Rule::Field {
            name,
            content: Box::new(convert_tagged(*content)),
        },
        RuleTagged::Seq { members } => Rule::Seq {
            members: members.into_iter().map(convert_tagged).collect(),
        },
        RuleTagged::Repeat { content } => Rule::Repeat {
            content: Box::new(convert_tagged(*content)),
        },
        RuleTagged::Repeat1 { content } => Rule::Repeat1 {
            content: Box::new(convert_tagged(*content)),
        },
        RuleTagged::Prec { value, content } => Rule::Prec {
            value,
            content: Box::new(convert_tagged(*content)),
        },
        RuleTagged::PrecLeft { value, content } => Rule::PrecLeft {
            value,
            content: Box::new(convert_tagged(*content)),
        },
        RuleTagged::PrecRight { value, content } => Rule::PrecRight {
            value,
            content: Box::new(convert_tagged(*content)),
        },
        RuleTagged::PrecDynamic { value, content } => Rule::PrecDynamic {
            value,
            content: Box::new(convert_tagged(*content)),
        },
        RuleTagged::Token { content } => Rule::Token {
            content: Box::new(convert_tagged(*content)),
        },
        RuleTagged::ImmediateToken { content } => Rule::ImmediateToken {
            content: Box::new(convert_tagged(*content)),
        },
        RuleTagged::Reserved {
            context_name,
            content,
        } => Rule::Reserved {
            context_name,
            content: Box::new(convert_tagged(*content)),
        },
    }
}

const KNOWN_TYPES: &[&str] = &[
    "ALIAS",
    "BLANK",
    "STRING",
    "PATTERN",
    "SYMBOL",
    "CHOICE",
    "FIELD",
    "SEQ",
    "REPEAT",
    "REPEAT1",
    "PREC",
    "PREC_LEFT",
    "PREC_RIGHT",
    "PREC_DYNAMIC",
    "TOKEN",
    "IMMEDIATE_TOKEN",
    "RESERVED",
];

impl<'de> Deserialize<'de> for Rule {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let type_name = match &value {
            Value::Object(map) => map.get("type").and_then(|v| v.as_str()).unwrap_or(""),
            _ => {
                return Err(de::Error::custom(
                    "rule must be a JSON object with a 'type' field",
                ))
            }
        };
        if KNOWN_TYPES.contains(&type_name) {
            serde_json::from_value::<RuleTagged>(value)
                .map(convert_tagged)
                .map_err(de::Error::custom)
        } else {
            Ok(Rule::Unknown(value))
        }
    }
}
