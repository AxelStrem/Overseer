use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, OverseerError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OverseerNode {
    pub name: String,
    pub node_type: String, // The original type (tab, div, etc.)
    pub template: Option<String>, // Path to a template node, e.g., "../TaskTemplate"
    pub parameters: HashMap<String, OverseerValue>,
    pub children: Vec<OverseerNode>,
    pub is_hierarchy_transparent: bool, // If true, children are accessible as if they belong to parent
    // Parameter insertion order as authored (list of keys) - used to preserve ordering fidelity
    #[serde(default)]
    pub param_order: Vec<String>,
    // Raw literal for this node's value (if it was an explicitly authored numeric with formatting such as trailing zeros)
    #[serde(default)]
    pub raw_value_literal: Option<String>,
    // Whether this node's value line was authored using dash shorthand (- name = value)
    #[serde(default)]
    pub authored_dash: bool,
    // Original child order index relative to its siblings as parsed (used for stable round-trip ordering)
    #[serde(default)]
    pub child_original_index: Option<usize>,
    // Number of blank (empty) lines that preceded this node in the source
    #[serde(default)]
    pub leading_blank_lines: u8,
    // Snapshot of original source trivia and spans (runtime metadata only)
    #[serde(skip)]
    pub source_snapshot: Option<NodeSourceSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_fingerprint: Option<u64>,
}

impl OverseerNode {
    pub fn new(name: String) -> Self {
        Self {
            name: name.clone(),
            node_type: name.clone(),
            template: None,
            parameters: HashMap::new(),
            children: Vec::new(),
            is_hierarchy_transparent: false,
            param_order: Vec::new(),
            raw_value_literal: None,
            authored_dash: false,
            child_original_index: None,
            leading_blank_lines: 0,
            source_snapshot: None,
            source_id: None,
            source_fingerprint: None,
        }
    }
    
    /// Construct a node with an explicit `node_type` and optional `name`.
    ///
    /// Transparency rules (kept in sync with the renderer's "generic transparent" logic):
    /// - Unnamed `div` is considered hierarchy-transparent. We later default its `name` to
    ///   `"div"`, so the renderer's check (name empty OR name == type) will still treat it as
    ///   a generic transparent wrapper and elide it from DOM path segments.
    /// - `tab` is always hierarchy-transparent; its default name is also `"tab"` when unnamed.
    /// - Named `div` is NOT transparent by default.
    ///
    /// Note: We compute `is_transparent` BEFORE defaulting the name, because transparency for
    /// unnamed `div` depends on whether a name was provided by the author.
    pub fn new_with_type(node_type: String, name: Option<String>) -> Self {
        // A node is considered transparent for rendering if it's a top-level container
        // like 'tab', or an unnamed 'div' which is used for logical grouping.
        // This check must happen BEFORE we default the name.
        let is_transparent = match node_type.as_str() {
            "tab" => true,
            "div" if name.is_none() => true,
            _ => false,
        };
        
        // If a name is provided, use it. Otherwise, default the name to the node's type.
        // This is crucial for matching override fields in templates where the override
        // node is unnamed (e.g., `StepTasks { ... }`).
        let final_name = name.unwrap_or_else(|| node_type.clone());

        Self {
            name: final_name,
            node_type,
            template: None,
            parameters: HashMap::new(),
            children: Vec::new(),
            is_hierarchy_transparent: is_transparent,
            param_order: Vec::new(),
            raw_value_literal: None,
            authored_dash: false,
            child_original_index: None,
            leading_blank_lines: 0,
            source_snapshot: None,
            source_id: None,
            source_fingerprint: None,
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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodeSourceSnapshot {
    pub span: (usize, usize),
    pub leading_span: Option<(usize, usize)>,
    pub full_text: String,
    pub leading_trivia: String,
    pub indent_unit: Option<String>,
    pub newline: Option<String>,
    pub fingerprint: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unnamed_div_defaults_to_transparent_and_name_div() {
        let n = OverseerNode::new_with_type("div".to_string(), None);
        assert!(n.is_hierarchy_transparent, "Unnamed div should be hierarchy-transparent");
        assert_eq!(n.name, "div", "Unnamed div should default its name to its type");
    }

    #[test]
    fn named_div_is_not_transparent_by_default() {
        let n = OverseerNode::new_with_type("div".to_string(), Some("Container".to_string()));
        assert!(!n.is_hierarchy_transparent, "Named div should NOT be hierarchy-transparent");
        assert_eq!(n.name, "Container");
    }

    #[test]
    fn tab_is_always_transparent_and_defaults_name() {
        let unnamed = OverseerNode::new_with_type("tab".to_string(), None);
        assert!(unnamed.is_hierarchy_transparent, "Tab should be hierarchy-transparent");
        assert_eq!(unnamed.name, "tab", "Unnamed tab should default its name to its type");

        let named = OverseerNode::new_with_type("tab".to_string(), Some("Root".to_string()));
        assert!(named.is_hierarchy_transparent, "Named tab should still be hierarchy-transparent");
        assert_eq!(named.name, "Root");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OverseerValue {
    Null, // Explicit null sentinel used for fallback formulas and empty values
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Date(String), // We'll use string representation for now
    Timestamp(String), // RFC3339 timestamp string (UTC recommended)
    Formula(String), // Formula expressions like $(...)
    Template(String), // For <...> syntax in parameters, e.g. entry=<../Template>
    Color(Color), // Colors in various formats
    CssSize(CssSize), // CSS size units (px, em, %, etc.)
    BorderStyle(BorderStyle), // Border styling options
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Color {
    Hex(String), // #FF0000
    Named(String), // red, blue, etc.
    Rgb(f32, f32, f32), // rgb(0.2, 0.8, 0.5) - float values 0.0-1.0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CssSize {
    Pixels(f32), // 16px
    Percentage(f32), // 120%
    Em(f32), // 1.2em
    Rem(f32), // 1.2rem
    ViewportWidth(f32), // 50vw
    ViewportHeight(f32), // 50vh
    Auto, // auto
    FitContent, // fit-content
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BorderStyle {
    None, // No border
    Default, // Default rounded border with shadow (current overseer style)
    Solid(CssSize, Color), // Solid border: border-style: solid, thickness, color
    Dashed(CssSize, Color), // Dashed border
    Dotted(CssSize, Color), // Dotted border
}

#[derive(Error, Debug, Serialize, Deserialize)]
pub enum OverseerError {
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("IO error: {0}")]
    IoError(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Formula error: {0}")]
    FormulaError(String),
    
    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Runtime error: {0}")]
    RuntimeError(String),
}
