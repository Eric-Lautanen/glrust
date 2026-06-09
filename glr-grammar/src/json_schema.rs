use core::fmt;
use serde::de::{self, Deserialize, Deserializer, Visitor};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug)]
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

/// Visitor that deserializes a GrammarJson from a JSON object map.
struct GrammarJsonVisitor;

impl<'de> Visitor<'de> for GrammarJsonVisitor {
    type Value = GrammarJson;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a grammar JSON object")
    }

    fn visit_map<M>(self, mut map: M) -> Result<GrammarJson, M::Error>
    where
        M: de::MapAccess<'de>,
    {
        let mut name = None;
        let mut rules = None;
        let mut extras = None;
        let mut conflicts = None;
        let mut precedences = None;
        let mut externals = None;
        let mut inline = None;
        let mut supertypes = None;
        let mut word = None;

        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "name" => name = Some(map.next_value()?),
                "rules" => rules = Some(map.next_value()?),
                "extras" => extras = Some(map.next_value()?),
                "conflicts" => conflicts = Some(map.next_value()?),
                "precedences" => precedences = Some(map.next_value()?),
                "externals" => externals = Some(map.next_value()?),
                "inline" => inline = Some(map.next_value()?),
                "supertypes" => supertypes = Some(map.next_value()?),
                "word" => word = Some(map.next_value()?),
                _ => { let _: Value = map.next_value()?; }
            }
        }

        Ok(GrammarJson {
            name: name.ok_or_else(|| de::Error::missing_field("name"))?,
            rules: rules.ok_or_else(|| de::Error::missing_field("rules"))?,
            extras,
            conflicts,
            precedences,
            externals,
            inline,
            supertypes,
            word,
        })
    }
}

impl<'de> Deserialize<'de> for GrammarJson {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(GrammarJsonVisitor)
    }
}

#[derive(Debug)]
pub enum Rule {
    /// Symbol reference: bare string like `"expression"` is shorthand
    /// for `{"type": "SYMBOL", "name": "expression"}`.
    Symbol(String),
    /// Token string literal: `{"type": "STRING", "value": "..."}`
    String { value: String },
    /// Regex pattern: `{"type": "PATTERN", "value": "..."}`
    Pattern { value: String },
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
    /// Dynamic precedence: `{"type": "PREC_DYNAMIC", "value": ..., "content": ...}`
    PrecDynamic { value: Box<Rule>, content: Box<Rule> },
    /// Alias: `{"type": "ALIAS", "value": ..., "named": bool, "content": ...}`
    Alias { value: Box<Rule>, named: bool, content: Box<Rule> },
    /// Token (lexical rule): `{"type": "TOKEN", "content": ...}`
    Token { content: Box<Rule> },
    /// Immediate token: `{"type": "IMMEDIATE_TOKEN", "content": ...}`
    ImmediateToken { content: Box<Rule> },
    /// Catch-all for unrecognized rule types (BLANK, etc.)
    Unknown(Value),
}

impl Rule {
    pub fn is_named(&self) -> bool {
        matches!(self, Rule::Symbol(_) | Rule::Choice { .. } | Rule::Seq { .. })
    }
}

impl<'de> Deserialize<'de> for Rule {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RuleVisitor;

        impl<'de> Visitor<'de> for RuleVisitor {
            type Value = Rule;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a rule: bare string (symbol ref) or tagged object")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Rule, E> {
                Ok(Rule::Symbol(v.to_string()))
            }

            fn visit_map<M>(self, mut map: M) -> Result<Rule, M::Error>
            where
                M: de::MapAccess<'de>,
            {
                let mut type_name: Option<String> = None;
                let mut name: Option<String> = None;
                let mut str_value: Option<String> = None;
                let mut int_value: Option<i32> = None;
                let mut rule_value: Option<Box<Rule>> = None;
                let mut members: Option<Vec<Rule>> = None;
                let mut content: Option<Box<Rule>> = None;
                let mut named: Option<bool> = None;
                let mut other_fields: Vec<(String, Value)> = Vec::new();

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "type" => type_name = Some(map.next_value()?),
                        "name" => name = Some(map.next_value()?),
                        "value" => {
                            let v: Value = map.next_value()?;
                            match v {
                                Value::String(s) => str_value = Some(s),
                                Value::Number(n) => {
                                    int_value = n.as_i64().map(|i| i as i32);
                                }
                                other => {
                                    rule_value = Some(Box::new(
                                        Rule::deserialize(other).map_err(de::Error::custom)?,
                                    ));
                                }
                            }
                        }
                        "members" => members = Some(map.next_value()?),
                        "content" => {
                            let v: Value = map.next_value()?;
                            content = Some(Box::new(
                                Rule::deserialize(v).map_err(de::Error::custom)?,
                            ));
                        }
                        "named" => named = Some(map.next_value()?),
                        other => {
                            let v: Value = map.next_value()?;
                            other_fields.push((other.to_string(), v));
                        }
                    }
                }

                let type_name = type_name.as_deref().unwrap_or("UNKNOWN");

                match type_name {
                    "SYMBOL" => Ok(Rule::Symbol(
                        name.ok_or_else(|| de::Error::missing_field("name"))?,
                    )),
                    "STRING" => Ok(Rule::String {
                        value: str_value.clone()
                            .ok_or_else(|| de::Error::missing_field("value"))?,
                    }),
                    "PATTERN" => Ok(Rule::Pattern {
                        value: str_value.clone()
                            .ok_or_else(|| de::Error::missing_field("value"))?,
                    }),
                    "SEQ" => Ok(Rule::Seq {
                        members: members
                            .ok_or_else(|| de::Error::missing_field("members"))?,
                    }),
                    "CHOICE" => Ok(Rule::Choice {
                        members: members
                            .ok_or_else(|| de::Error::missing_field("members"))?,
                    }),
                    "REPEAT" => Ok(Rule::Repeat {
                        content: content
                            .ok_or_else(|| de::Error::missing_field("content"))?,
                    }),
                    "FIELD" => Ok(Rule::Field {
                        name: name.ok_or_else(|| de::Error::missing_field("name"))?,
                        content: content
                            .ok_or_else(|| de::Error::missing_field("content"))?,
                    }),
                    "PREC" => Ok(Rule::Prec {
                        value: int_value
                            .ok_or_else(|| de::Error::missing_field("value"))?,
                        content: content
                            .ok_or_else(|| de::Error::missing_field("content"))?,
                    }),
                    "PREC_LEFT" => Ok(Rule::PrecLeft {
                        value: int_value
                            .ok_or_else(|| de::Error::missing_field("value"))?,
                        content: content
                            .ok_or_else(|| de::Error::missing_field("content"))?,
                    }),
                    "PREC_RIGHT" => Ok(Rule::PrecRight {
                        value: int_value
                            .ok_or_else(|| de::Error::missing_field("value"))?,
                        content: content
                            .ok_or_else(|| de::Error::missing_field("content"))?,
                    }),
                    "PREC_DYNAMIC" => Ok(Rule::PrecDynamic {
                        value: rule_value
                            .ok_or_else(|| de::Error::custom("missing 'value' in PREC_DYNAMIC"))?,
                        content: content
                            .ok_or_else(|| de::Error::missing_field("content"))?,
                    }),
                    "ALIAS" => Ok(Rule::Alias {
                        value: rule_value
                            .ok_or_else(|| de::Error::custom("missing 'value' in ALIAS"))?,
                        named: named
                            .ok_or_else(|| de::Error::missing_field("named"))?,
                        content: content
                            .ok_or_else(|| de::Error::missing_field("content"))?,
                    }),
                    "TOKEN" => Ok(Rule::Token {
                        content: content
                            .ok_or_else(|| de::Error::missing_field("content"))?,
                    }),
                    "IMMEDIATE_TOKEN" => Ok(Rule::ImmediateToken {
                        content: content
                            .ok_or_else(|| de::Error::missing_field("content"))?,
                    }),
                    _ => {
                        let mut obj = serde_json::Map::new();
                        obj.insert("type".to_string(), Value::String(type_name.to_string()));
                        if let Some(n) = name {
                            obj.insert("name".to_string(), Value::String(n));
                        }
                        if let Some(s) = str_value {
                            obj.insert("value".to_string(), Value::String(s));
                        }
                        if let Some(i) = int_value {
                            obj.insert("value".to_string(), Value::Number(i.into()));
                        }
                        for (k, v) in other_fields {
                            obj.insert(k, v);
                        }
                        Ok(Rule::Unknown(Value::Object(obj)))
                    }
                }
            }
        }

        deserializer.deserialize_any(RuleVisitor)
    }
}
