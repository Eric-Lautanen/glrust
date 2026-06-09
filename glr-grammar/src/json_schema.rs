//! Types representing the tree-sitter `grammar.json` schema.
//!
//! Supports both ABI 14 and ABI 15 grammar formats.

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
    Symbol(String),
    String(String),
    Pattern { pattern: String },
    Seq { members: Vec<Rule> },
    Choice { members: Vec<Rule> },
    Repeat { content: Box<Rule> },
    Field { name: String, content: Box<Rule> },
    Prec { value: i32, content: Box<Rule> },
    PrecLeft { value: i32, content: Box<Rule> },
    PrecRight { value: i32, content: Box<Rule> },
    Alias { value: Box<Rule>, named: bool, content: Box<Rule> },
    Token { content: Box<Rule> },
    ImmediateToken { content: Box<Rule> },
}
