use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, OverseerError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverseerNode {
    pub name: String,
    pub parameters: HashMap<String, OverseerValue>,
    pub children: Vec<OverseerNode>,
}

impl OverseerNode {
    pub fn new(name: String) -> Self {
        Self {
            name,
            parameters: HashMap::new(),
            children: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OverseerValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Date(String), // We'll use string representation for now
    Formula(String), // Formula expressions like $(...)
}

#[derive(Error, Debug, Serialize, Deserialize)]
pub enum OverseerError {
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("IO error: {0}")]
    IoError(String),
    
    #[error("Formula error: {0}")]
    FormulaError(String),
    
    #[error("Validation error: {0}")]
    ValidationError(String),
}
