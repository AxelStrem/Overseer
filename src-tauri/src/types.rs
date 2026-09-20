use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, OverseerError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OverseerNode {
    pub name: String,
    pub node_type: String, // The original type (tab, div, etc.)
    // Every field below that is usually at its default is left out of the wire form rather than
    // sent as `null`, `false`, `0` or `[]`. They all read back as their default, so nothing is
    // lost - what is saved is a fixed toll on every node, and a document is mostly nodes.
    //
    // It is a fixed toll worth minding: opening tasks.os hands back 5801 nodes, and seven such
    // fields at twenty-odd bytes each came to most of a megabyte of the four it sent. That is
    // paid on every open, over whatever connection the person happens to be on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>, // Path to a template node, e.g., "../TaskTemplate"
    pub parameters: HashMap<String, OverseerValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<OverseerNode>,
    // If true, children are accessible as if they belong to parent
    #[serde(default, skip_serializing_if = "not_so")]
    pub is_hierarchy_transparent: bool,
    // Parameter insertion order as authored (list of keys) - used to preserve ordering fidelity
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_order: Vec<String>,
    // Raw literal for this node's value (if it was an explicitly authored numeric with formatting such as trailing zeros)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_value_literal: Option<String>,
    // Whether this node's value line was authored using dash shorthand (- name = value)
    #[serde(default, skip_serializing_if = "not_so")]
    pub authored_dash: bool,
    // Original child order index relative to its siblings as parsed (used for stable round-trip ordering)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_original_index: Option<usize>,
    // Number of blank (empty) lines that preceded this node in the source
    #[serde(default, skip_serializing_if = "none_of_them")]
    pub leading_blank_lines: u8,
    // Snapshot of original source trivia and spans (runtime metadata only)
    #[serde(skip)]
    pub source_snapshot: Option<NodeSourceSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_fingerprint: Option<u64>,
}

/// For `skip_serializing_if` on a flag that is usually off.
fn not_so(flag: &bool) -> bool {
    !*flag
}

/// The same, for a count that is usually nought.
fn none_of_them(count: &u8) -> bool {
    *count == 0
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
    /// The child of this name, looking through the wrappers an address does not mention.
    ///
    /// `get_accessible_children` builds a vector of every child - and another for every
    /// transparent one it looks through - which is a great deal of work to then scan for one
    /// name. Path resolution did exactly that at every level, so it does this instead. Same
    /// answer: children in order, a transparent one contributing what is inside it rather than
    /// itself, first match wins.
    ///
    /// Written while looking for why a relative lookup is expensive, and worth saying that it
    /// was not the answer: `../has_deadline == false` costs 0.145 ms a call against 0.003 ms
    /// for a plain name in the same scope, and this made no measurable difference to that. The
    /// cost is that `has_deadline` is itself a formula, so reading it evaluates one - see
    /// `topo`. This is kept because not building a vector to find one item is simply better,
    /// not because it made anything faster.
    pub fn accessible_child(&self, name: &str) -> Option<&OverseerNode> {
        for child in &self.children {
            if child.is_hierarchy_transparent {
                if let Some(found) = child.accessible_child(name) {
                    return Some(found);
                }
            } else if child.name == name {
                return Some(child);
            }
        }
        None
    }

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

    /// Convert this node (and its descendants) to a synthetic snapshot clone of their existing snapshots.
    /// Used when duplicating already-parsed structures (e.g., template children) so we preserve authored
    /// trivia but mark them as synthetic clones.
    pub fn mark_snapshot_as_template_clone(&mut self) {
        if let Some(existing) = self.source_snapshot.clone() {
            let fingerprint = existing.fingerprint;
            self.source_snapshot = Some(NodeSourceSnapshot::synthetic_from_template(&existing));
            self.source_fingerprint = Some(fingerprint);
        } else {
            self.source_snapshot = None;
            self.source_fingerprint = None;
        }
        self.source_id = None;
        for child in self.children.iter_mut() {
            child.mark_snapshot_as_template_clone();
        }
    }

    /// Adopt structured snapshot metadata from a template node, marking it as a synthetic template clone.
    /// The caller is responsible for ensuring this node's structure mirrors the template's for best fidelity.
    pub fn adopt_template_snapshot(&mut self, template: &OverseerNode) {
        if let Some(template_snapshot) = template.source_snapshot.as_ref() {
            let fingerprint = template_snapshot.fingerprint;
            self.source_snapshot = Some(NodeSourceSnapshot::synthetic_from_template(
                template_snapshot,
            ));
            self.source_fingerprint = Some(fingerprint);
        } else {
            self.source_snapshot = None;
            self.source_fingerprint = None;
        }
        self.source_id = None;
        for (child, template_child) in self.children.iter_mut().zip(template.children.iter()) {
            child.adopt_template_snapshot(template_child);
        }
    }

    /// Attach a synthetic snapshot carrying only formatting style (indent/newline) when no source clone exists.
    pub fn synthesize_snapshot_with_style(
        &mut self,
        indent_unit: Option<String>,
        newline: Option<String>,
    ) {
        self.source_snapshot = Some(NodeSourceSnapshot::synthetic_with_style(
            indent_unit,
            newline,
        ));
        self.source_fingerprint = None;
        self.source_id = None;
    }

    /// Recursively apply style-only synthetic snapshots to this node and descendants.
    pub fn synthesize_snapshot_with_style_recursive(
        &mut self,
        indent_unit: Option<String>,
        newline: Option<String>,
    ) {
        let indent_clone = indent_unit.clone();
        let newline_clone = newline.clone();
        self.synthesize_snapshot_with_style(indent_unit, newline);
        for child in self.children.iter_mut() {
            child.synthesize_snapshot_with_style_recursive(
                indent_clone.clone(),
                newline_clone.clone(),
            );
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SourceSlice {
    pub span: (usize, usize),
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodeHeaderSnapshot {
    pub template: Option<SourceSlice>,
    pub type_token: Option<SourceSlice>,
    pub name: Option<SourceSlice>,
    pub parameters: Option<SourceSlice>,
    pub assignment_operator: Option<SourceSlice>,
    pub trailing: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodeChildEnvelopeSnapshot {
    pub open: Option<SourceSlice>,
    pub close: Option<SourceSlice>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodeBodySnapshot {
    pub value: Option<SourceSlice>,
    pub child_envelope: NodeChildEnvelopeSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SnapshotOrigin {
    Parsed,
    Synthetic(SyntheticSnapshotKind),
}

impl Default for SnapshotOrigin {
    fn default() -> Self {
        SnapshotOrigin::Parsed
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SyntheticSnapshotKind {
    TemplateClone,
    RuntimeConstructed,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodeSourceSnapshot {
    pub span: (usize, usize),
    pub leading_span: Option<(usize, usize)>,
    pub header_span: (usize, usize),
    pub body_span: Option<(usize, usize)>,
    pub trailing_span: Option<(usize, usize)>,
    pub full_text: String,
    pub leading_trivia: String,
    pub header: NodeHeaderSnapshot,
    pub body: NodeBodySnapshot,
    pub trailing_trivia: String,
    pub indent_unit: Option<String>,
    pub newline: Option<String>,
    pub fingerprint: u64,
    pub origin: SnapshotOrigin,
}

/// Trivia with its comment lines removed, and everything else left exactly as it was.
///
/// Only whole comment lines go. The rest is layout: the newline and indent that put a node
/// where it belongs, and - on the trailing side - the indent before a closing brace. A line
/// of pure whitespace looks blank and is not: dropping it as one unindents the brace.
fn without_comments(trivia: &str) -> String {
    trivia
        .split('\n')
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

impl NodeSourceSnapshot {
    pub fn synthetic_from_template(template: &NodeSourceSnapshot) -> Self {
        let mut snapshot = template.clone();
        snapshot.span = (0, 0);
        snapshot.leading_span = None;
        snapshot.header_span = (0, 0);
        snapshot.body_span = snapshot.body_span.map(|_| (0, 0));
        snapshot.trailing_span = None;
        snapshot.indent_unit = None;
        // A comment above a template explains the template. It is not part of what the
        // template makes: carried along, every entry added to a list arrived with a
        // copy, so a catalogue of forty foods held forty copies of the same remark
        // about where the defaults live.
        //
        // Only the comments go. The rest of the trivia is layout - the newline and
        // indent that put the node where it belongs, and on the trailing side the
        // indent before a closing brace - and dropping that reflows the document.
        snapshot.leading_trivia = without_comments(&snapshot.leading_trivia);
        snapshot.trailing_trivia = without_comments(&snapshot.trailing_trivia);
        snapshot.origin = SnapshotOrigin::Synthetic(SyntheticSnapshotKind::TemplateClone);
        snapshot
    }

    pub fn synthetic_with_style(indent_unit: Option<String>, newline: Option<String>) -> Self {
        let mut snapshot = NodeSourceSnapshot::default();
        snapshot.indent_unit = indent_unit;
        snapshot.newline = newline;
        snapshot.origin = SnapshotOrigin::Synthetic(SyntheticSnapshotKind::RuntimeConstructed);
        snapshot
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unnamed_div_defaults_to_transparent_and_name_div() {
        let n = OverseerNode::new_with_type("div".to_string(), None);
        assert!(
            n.is_hierarchy_transparent,
            "Unnamed div should be hierarchy-transparent"
        );
        assert_eq!(
            n.name, "div",
            "Unnamed div should default its name to its type"
        );
    }

    #[test]
    fn named_div_is_not_transparent_by_default() {
        let n = OverseerNode::new_with_type("div".to_string(), Some("Container".to_string()));
        assert!(
            !n.is_hierarchy_transparent,
            "Named div should NOT be hierarchy-transparent"
        );
        assert_eq!(n.name, "Container");
    }

    #[test]
    fn tab_is_always_transparent_and_defaults_name() {
        let unnamed = OverseerNode::new_with_type("tab".to_string(), None);
        assert!(
            unnamed.is_hierarchy_transparent,
            "Tab should be hierarchy-transparent"
        );
        assert_eq!(
            unnamed.name, "tab",
            "Unnamed tab should default its name to its type"
        );

        let named = OverseerNode::new_with_type("tab".to_string(), Some("Root".to_string()));
        assert!(
            named.is_hierarchy_transparent,
            "Named tab should still be hierarchy-transparent"
        );
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
    Date(String),             // We'll use string representation for now
    Timestamp(String),        // RFC3339 timestamp string (UTC recommended)
    Formula(String),          // Formula expressions like $(...)
    Template(String),         // For <...> syntax in parameters, e.g. entry=<../Template>
    Color(Color),             // Colors in various formats
    CssSize(CssSize),         // CSS size units (px, em, %, etc.)
    BorderStyle(BorderStyle), // Border styling options
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Color {
    Hex(String),        // #FF0000
    Named(String),      // red, blue, etc.
    Rgb(f32, f32, f32), // rgb(0.2, 0.8, 0.5) - float values 0.0-1.0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CssSize {
    Pixels(f32),         // 16px
    Percentage(f32),     // 120%
    Em(f32),             // 1.2em
    Rem(f32),            // 1.2rem
    ViewportWidth(f32),  // 50vw
    ViewportHeight(f32), // 50vh
    Auto,                // auto
    FitContent,          // fit-content
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BorderStyle {
    None,                   // No border
    Default,                // Default rounded border with shadow (current overseer style)
    Solid(CssSize, Color),  // Solid border: border-style: solid, thickness, color
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
