//! Stable addresses for the nodes of a resolved document.
//!
//! Every interaction currently re-parses the document, and the ids minted during a parse are
//! positional: they change wholesale when anything about the text changes, even where the
//! document did not. That makes it impossible to say "this node changed" across two resolves,
//! which is what sending a change instead of a whole document requires.
//!
//! An address here is derived from the document's own structure, so the same logical node
//! keeps the same address across resolves, and nothing has to be stored in the file to
//! achieve it. Two documents that differ only in a value produce identical address sets.
//!
//! Entries of a list that declares `key=` are addressed by that key rather than by position,
//! because entries get inserted: prepending a record to a history shifts every entry after it,
//! and a positional address would then name the entry's neighbour instead. An entry of a list
//! without a key is addressed by position and shifts with it - a limitation of the document
//! rather than of this scheme, and the reason `key=` is worth declaring.
//!
//! Sorting does not come into it. `sort_by` computes a `_ui_sort_key` per entry and the
//! renderer orders by that; the tree keeps the order the document was written in.

use crate::types::*;

/// Characters that would otherwise be read as address structure.
fn escape(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for c in segment.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '/' => out.push_str("\\/"),
            '[' => out.push_str("\\["),
            ']' => out.push_str("\\]"),
            _ => out.push(c),
        }
    }
    out
}

/// A key value rendered for use in an address.
///
/// Formulas and templates are deliberately absent: an unresolved value cannot identify a row,
/// and falling back to position is more honest than inventing a stable-looking address.
fn key_text(value: &OverseerValue) -> Option<String> {
    match value {
        OverseerValue::String(s) => Some(s.clone()),
        OverseerValue::Integer(i) => Some(i.to_string()),
        OverseerValue::Float(f) => Some(format!("{}", f)),
        OverseerValue::Boolean(b) => Some(b.to_string()),
        OverseerValue::Date(d) => Some(d.clone()),
        OverseerValue::Timestamp(t) => Some(t.clone()),
        _ => None,
    }
}

/// The name of the field a list identifies its entries by, if it declares one.
fn key_field(node: &OverseerNode) -> Option<&str> {
    if node.node_type != "list" {
        return None;
    }
    match node.parameters.get("key") {
        Some(OverseerValue::String(s)) if !s.is_empty() => Some(s.as_str()),
        _ => None,
    }
}

/// The value a node carries for its list's key field.
fn key_of(node: &OverseerNode, field: &str) -> Option<String> {
    let child = node.children.iter().find(|c| c.name == field)?;
    // The computed value is what the rest of the document sees, so it is what identifies the
    // entry; the raw value may still be the formula that produced it.
    child
        .parameters
        .get("_computed_value")
        .or_else(|| child.parameters.get("value"))
        .and_then(key_text)
}

/// The address segment for `siblings[index]`.
fn segment(parent: Option<&OverseerNode>, siblings: &[OverseerNode], index: usize) -> String {
    let node = &siblings[index];
    if let Some(field) = parent.and_then(key_field) {
        if let Some(key) = key_of(node, field) {
            return format!("[{}]", escape(&key));
        }
    }
    let name = &node.name;
    let ordinal = siblings[..index].iter().filter(|s| &s.name == name).count();
    if ordinal == 0 {
        escape(name)
    } else {
        format!("{}#{}", escape(name), ordinal)
    }
}


/// The address segment of each immediate child of `parent`, in order.
///
/// The parent is required: it is what says whether these are entries of a keyed list, and a
/// segment computed without it silently falls back to position.
pub fn child_segments(parent: &OverseerNode) -> Vec<String> {
    (0..parent.children.len())
        .map(|i| segment(Some(parent), &parent.children, i))
        .collect()
}

/// The address segment of each root node, in order.
pub fn root_segments(nodes: &[OverseerNode]) -> Vec<String> {
    (0..nodes.len()).map(|i| segment(None, nodes, i)).collect()
}

/// Visit every node with its address, outermost first.
pub fn walk<'a, F: FnMut(&str, &'a OverseerNode)>(nodes: &'a [OverseerNode], visit: &mut F) {
    fn go<'a, F: FnMut(&str, &'a OverseerNode)>(
        parent: Option<&'a OverseerNode>,
        nodes: &'a [OverseerNode],
        prefix: &str,
        visit: &mut F,
    ) {
        for (i, node) in nodes.iter().enumerate() {
            let address = if prefix.is_empty() {
                segment(parent, nodes, i)
            } else {
                format!("{}/{}", prefix, segment(parent, nodes, i))
            };
            visit(&address, node);
            go(Some(node), &node.children, &address, visit);
        }
    }
    go(None, nodes, "", visit);
}

/// Every address in the document, in document order.
pub fn addresses(nodes: &[OverseerNode]) -> Vec<String> {
    let mut out = Vec::new();
    walk(nodes, &mut |address, _| out.push(address.to_string()));
    out
}

/// The node at an address, if the document still has one there.
pub fn find<'a>(nodes: &'a [OverseerNode], address: &str) -> Option<&'a OverseerNode> {
    let mut found = None;
    walk(nodes, &mut |candidate, node| {
        if found.is_none() && candidate == address {
            found = Some(node);
        }
    });
    found
}

/// The path of node names that reaches an address, as actions address nodes.
///
/// Two schemes are in play and both are needed: an address survives a resolve and is what a
/// caller names a node by, while an action resolves a path of names. Segments carry an ordinal
/// where siblings share a name, so the path names one node.
pub fn name_path(nodes: &[OverseerNode], address: &str) -> Option<Vec<String>> {
    fn go(
        segments: &[String],
        nodes: &[OverseerNode],
        prefix: &str,
        names: &[String],
        target: &str,
    ) -> Option<Vec<String>> {
        for (i, node) in nodes.iter().enumerate() {
            let here = if prefix.is_empty() {
                segments[i].clone()
            } else {
                format!("{}/{}", prefix, segments[i])
            };
            let ordinal = nodes[..i].iter().filter(|s| s.name == node.name).count();
            let mut named = names.to_vec();
            named.push(if ordinal == 0 {
                node.name.clone()
            } else {
                format!("{}#{}", node.name, ordinal)
            });
            if here == target {
                return Some(named);
            }
            if target.starts_with(&format!("{}/", here)) {
                // Parent-aware, or an entry of a keyed list would be addressed by position
                // and the descent would go looking down the wrong branch.
                if let Some(found) =
                    go(&child_segments(node), &node.children, &here, &named, target)
                {
                    return Some(found);
                }
            }
        }
        None
    }
    go(&root_segments(nodes), nodes, "", &[], address)
}
