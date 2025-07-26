use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, OverseerError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverseerNode {
    pub name: String,
    pub node_type: String, // The original type (tab, div, etc.)
    pub parameters: HashMap<String, OverseerValue>,
    pub children: Vec<OverseerNode>,
    pub is_hierarchy_transparent: bool, // If true, children are accessible as if they belong to parent
}

impl OverseerNode {
    pub fn new(name: String) -> Self {
        Self {
            name: name.clone(),
            node_type: name.clone(),
            parameters: HashMap::new(),
            children: Vec::new(),
            is_hierarchy_transparent: false,
        }
    }
    
    pub fn new_with_type(node_type: String, name: Option<String>) -> Self {
        let is_transparent = name.is_none();
        let full_name = if let Some(name) = name {
            format!("{}_{}", node_type, name)
        } else {
            node_type.clone()
        };
        
        Self {
            name: full_name,
            node_type,
            parameters: HashMap::new(),
            children: Vec::new(),
            is_hierarchy_transparent: is_transparent,
        }
    }
    
    /// Get all accessible children, including transparent children's children
    pub fn get_accessible_children(&self) -> Vec<&OverseerNode> {
        let mut result = Vec::new();
        
        for child in &self.children {
            if child.is_hierarchy_transparent {
                // If child is transparent, add its children instead
                result.extend(child.get_accessible_children());
            } else {
                // Normal child, add it directly
                result.push(child);
            }
        }
        
        result
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
