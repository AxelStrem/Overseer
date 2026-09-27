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
    pub parameters: Params,
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
    // Snapshot of original source trivia and spans (runtime metadata only).
    //
    // Shared rather than owned. Every node carries one, and a template's instances carry copies
    // of the declaration's - so the food tracker held its 110 KB of text thirty-five times over,
    // 584 bytes of each node were this field, and every copy of a document copied all of it.
    // Read far more often than changed; a change makes the node its own copy, `Arc::make_mut`.
    #[serde(skip)]
    pub source_snapshot: Option<std::sync::Arc<NodeSourceSnapshot>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_fingerprint: Option<u64>,
}

/// A node's parameters: a short list of names and values, looked through in order.
///
/// It was a hash table per node, and a node holds eight to ten parameters - twenty at the most, on
/// the real documents - so the table was most of what the parameters cost: sixteen slots of
/// seventy-three bytes for ten entries, on each of the food tracker's nine thousand nodes. A list
/// is the entries and nothing else, and looking through ten names is no slower than hashing one.
///
/// The same methods the table was used through, so the code reading and writing parameters did
/// not change. Equal regardless of order, as the table was: a change to a node is found by
/// comparing its parameters, and the same ones in another order are not a change. Written and
/// read as a map, so the page sees the object it always has.
#[derive(Clone, Default)]
pub struct Params(Vec<(String, OverseerValue)>);

impl Params {
    pub fn new() -> Self {
        Params(Vec::new())
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Params(Vec::with_capacity(capacity))
    }

    fn position(&self, key: &str) -> Option<usize> {
        self.0.iter().position(|(k, _)| k == key)
    }

    pub fn get(&self, key: &str) -> Option<&OverseerValue> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut OverseerValue> {
        self.0.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.position(key).is_some()
    }

    /// Set a parameter, handing back what it held before.
    pub fn insert(&mut self, key: String, value: OverseerValue) -> Option<OverseerValue> {
        match self.position(&key) {
            Some(at) => Some(std::mem::replace(&mut self.0[at].1, value)),
            None => {
                self.0.push((key, value));
                None
            }
        }
    }

    pub fn remove(&mut self, key: &str) -> Option<OverseerValue> {
        self.position(key).map(|at| self.0.remove(at).1)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &OverseerValue)> {
        self.0.iter().map(|(k, v)| (k, v))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&String, &mut OverseerValue)> {
        self.0.iter_mut().map(|(k, v)| (&*k, v))
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.0.iter().map(|(k, _)| k)
    }

    pub fn values(&self) -> impl Iterator<Item = &OverseerValue> {
        self.0.iter().map(|(_, v)| v)
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut OverseerValue> {
        self.0.iter_mut().map(|(_, v)| v)
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&String, &mut OverseerValue) -> bool) {
        self.0.retain_mut(|(k, v)| keep(k, v));
    }

    /// The parameter of this name, to fill in if it is not there - `.entry(k).or_insert(v)`.
    pub fn entry(&mut self, key: String) -> ParamEntry<'_> {
        ParamEntry { params: self, key }
    }
}

/// See `Params::entry`.
pub struct ParamEntry<'a> {
    params: &'a mut Params,
    key: String,
}

impl<'a> ParamEntry<'a> {
    pub fn or_insert(self, default: OverseerValue) -> &'a mut OverseerValue {
        let at = match self.params.position(&self.key) {
            Some(at) => at,
            None => {
                self.params.0.push((self.key, default));
                self.params.0.len() - 1
            }
        };
        &mut self.params.0[at].1
    }
}

/// `params[key]`, as the table allowed - and like it, a missing key is a mistake that panics.
impl<Q: AsRef<str> + ?Sized> std::ops::Index<&Q> for Params {
    type Output = OverseerValue;
    fn index(&self, key: &Q) -> &OverseerValue {
        self.get(key.as_ref())
            .unwrap_or_else(|| panic!("no parameter `{}`", key.as_ref()))
    }
}

impl PartialEq for Params {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().all(|(k, v)| other.get(k) == Some(v))
    }
}

impl std::fmt::Debug for Params {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<'a> IntoIterator for &'a Params {
    type Item = (&'a String, &'a OverseerValue);
    type IntoIter = std::iter::Map<
        std::slice::Iter<'a, (String, OverseerValue)>,
        fn(&'a (String, OverseerValue)) -> (&'a String, &'a OverseerValue),
    >;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().map(|(k, v)| (k, v))
    }
}

impl IntoIterator for Params {
    type Item = (String, OverseerValue);
    type IntoIter = std::vec::IntoIter<(String, OverseerValue)>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl FromIterator<(String, OverseerValue)> for Params {
    fn from_iter<I: IntoIterator<Item = (String, OverseerValue)>>(entries: I) -> Self {
        let mut params = Params::new();
        params.extend(entries);
        params
    }
}

impl Extend<(String, OverseerValue)> for Params {
    fn extend<I: IntoIterator<Item = (String, OverseerValue)>>(&mut self, entries: I) {
        for (k, v) in entries {
            self.insert(k, v);
        }
    }
}

impl From<HashMap<String, OverseerValue>> for Params {
    fn from(map: HashMap<String, OverseerValue>) -> Self {
        map.into_iter().collect()
    }
}

impl Serialize for Params {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.collect_map(self.iter())
    }
}

impl<'de> Deserialize<'de> for Params {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct Entries;
        impl<'de> serde::de::Visitor<'de> for Entries {
            type Value = Params;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a map of parameters")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> std::result::Result<Params, A::Error> {
                let mut params = Params::with_capacity(map.size_hint().unwrap_or(0));
                while let Some((k, v)) = map.next_entry::<String, OverseerValue>()? {
                    params.insert(k, v);
                }
                Ok(params)
            }
        }
        deserializer.deserialize_map(Entries)
    }
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
            parameters: Params::new(),
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
            parameters: Params::new(),
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
            self.source_snapshot = Some(NodeSourceSnapshot::template_clone_of(&existing));
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
            self.source_snapshot = Some(NodeSourceSnapshot::template_clone_of(template_snapshot));
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
        self.source_snapshot = Some(std::sync::Arc::new(NodeSourceSnapshot::synthetic_with_style(
            indent_unit,
            newline,
        )));
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

/// A stretch of a document's text, held as a view into one shared copy of the whole.
///
/// A node's snapshot keeps the text it was parsed from, and that text includes everything nested
/// in it - so a document held its own text once for every level of nesting, and the food tracker
/// kept 3.6 MB of snapshot text for 110 KB of file. As views, they are the file once, and the
/// spans say where each one is. Reads like a `str`; something that has to change the text takes
/// a `String` of it first.
#[derive(Clone, Default)]
pub struct SharedText {
    source: Option<std::sync::Arc<str>>,
    start: usize,
    end: usize,
}

impl SharedText {
    /// The part of `source` from `start` to `end`.
    pub fn view(source: std::sync::Arc<str>, start: usize, end: usize) -> Self {
        let end = end.min(source.len());
        let start = start.min(end);
        SharedText { source: Some(source), start, end }
    }

    pub fn as_str(&self) -> &str {
        match &self.source {
            Some(source) => &source[self.start..self.end],
            None => "",
        }
    }

    /// The shared copy this is a view into, by identity and size - for counting what is held once.
    pub fn backing(&self) -> Option<(usize, usize)> {
        self.source
            .as_ref()
            .map(|source| (std::sync::Arc::as_ptr(source) as *const u8 as usize, source.len()))
    }
}

impl std::ops::Deref for SharedText {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl From<String> for SharedText {
    fn from(text: String) -> Self {
        let end = text.len();
        SharedText { source: Some(std::sync::Arc::from(text)), start: 0, end }
    }
}

impl From<&str> for SharedText {
    fn from(text: &str) -> Self {
        SharedText::from(text.to_string())
    }
}

impl PartialEq for SharedText {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl std::fmt::Debug for SharedText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self.as_str(), f)
    }
}

impl std::fmt::Display for SharedText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodeSourceSnapshot {
    pub span: (usize, usize),
    pub leading_span: Option<(usize, usize)>,
    pub header_span: (usize, usize),
    pub body_span: Option<(usize, usize)>,
    pub trailing_span: Option<(usize, usize)>,
    pub full_text: SharedText,
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

    /// What a node made from a template carries: the template's snapshot as `synthetic_from_template`
    /// makes it, one copy shared by every node made from the same one.
    ///
    /// Each of a list's entries used to get a copy of its own, text and all, so a template with
    /// forty entries held its text forty-one times. The copy is a function of the template's
    /// snapshot alone, so it is made once per snapshot and handed out after - remembered by the
    /// snapshot it came from, weakly, so a template that has gone cannot be mistaken for a new one
    /// that happens to sit where it did. A copy of a copy is the copy: making one again changes
    /// nothing.
    pub fn template_clone_of(template: &std::sync::Arc<Self>) -> std::sync::Arc<Self> {
        if matches!(template.origin, SnapshotOrigin::Synthetic(SyntheticSnapshotKind::TemplateClone)) {
            return template.clone();
        }
        type Made = (std::sync::Weak<NodeSourceSnapshot>, std::sync::Arc<NodeSourceSnapshot>);
        thread_local! {
            static MADE: std::cell::RefCell<std::collections::HashMap<usize, Made>> =
                std::cell::RefCell::new(std::collections::HashMap::new());
        }
        let key = std::sync::Arc::as_ptr(template) as usize;
        MADE.with(|made| {
            let mut made = made.borrow_mut();
            if let Some((source, copy)) = made.get(&key) {
                if source.upgrade().is_some_and(|alive| std::sync::Arc::ptr_eq(&alive, template)) {
                    return copy.clone();
                }
            }
            // Kept from growing without bound: what was made from templates since dropped goes.
            if made.len() > 4096 {
                made.retain(|_, (source, _)| source.strong_count() > 0);
            }
            let copy = std::sync::Arc::new(Self::synthetic_from_template(template));
            made.insert(key, (std::sync::Arc::downgrade(template), copy.clone()));
            copy
        })
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
