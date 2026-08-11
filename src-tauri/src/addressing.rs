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
///
/// Through wrappers, because grouping an entry's fields for layout must not change what the
/// entry is called. It did once: a day whose date was put in a row with the rest of its
/// summary lost its key, and every address that named it by date stopped resolving.
fn key_of(node: &OverseerNode, field: &str) -> Option<String> {
    let child = effective_children(node)
        .into_iter()
        .map(|(_, child)| child)
        .find(|c| c.name == field)?;
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


/// Whether a node is a wrapper rather than something a reader would name.
///
/// A `div` written without a name groups things for layout - it is marked transparent, and the
/// rest of the system already looks straight through it: a formula path, a rendered path and
/// the frontend's own lookups all treat it as if its children belonged to its parent. An
/// address that mentioned it would be the odd one out, and would say `div#2` where every other
/// part of the system says nothing at all.
///
/// A `tab` is also transparent, for layout, but it has a name of its own and keeps it. The
/// question is not whether a node affects rendering; it is whether anyone would call it
/// something.
/// Whether a node groups its children for layout and stands for nothing itself.
///
/// Public because more than addressing has to agree about it. A rule that only one layer
/// honours is worse than no rule: a document laid out the obvious way then keeps its
/// addresses but loses its action targets, and nothing says why.
pub fn is_wrapper(node: &OverseerNode) -> bool {
    node.is_hierarchy_transparent && (node.name.is_empty() || node.name == node.node_type)
}

/// The children a node has for addressing, with the child indices that reach each.
///
/// A wrapper contributes what is inside it rather than itself, so the path to a node under one
/// is longer than its address - which is why both are returned together.
pub fn effective_children(parent: &OverseerNode) -> Vec<(Vec<usize>, &OverseerNode)> {
    let mut out = Vec::new();
    collect(&parent.children, &[], &mut out);
    out
}

/// The same, for the document's roots.
pub fn effective_roots(nodes: &[OverseerNode]) -> Vec<(Vec<usize>, &OverseerNode)> {
    let mut out = Vec::new();
    collect(nodes, &[], &mut out);
    out
}

fn collect<'a>(
    nodes: &'a [OverseerNode],
    prefix: &[usize],
    out: &mut Vec<(Vec<usize>, &'a OverseerNode)>,
) {
    for (index, node) in nodes.iter().enumerate() {
        let mut path = prefix.to_vec();
        path.push(index);
        if is_wrapper(node) {
            collect(&node.children, &path, out);
        } else {
            out.push((path, node));
        }
    }
}

/// The address segment of each of these siblings, in order.
///
/// `parent` is what says whether they are entries of a keyed list; without it a keyed entry
/// silently falls back to its position.
pub fn segments_for(
    parent: Option<&OverseerNode>,
    children: &[(Vec<usize>, &OverseerNode)],
) -> Vec<String> {
    let mut out = Vec::with_capacity(children.len());
    for (i, (_, node)) in children.iter().enumerate() {
        if let Some(field) = parent.and_then(key_field) {
            if let Some(key) = key_of(node, field) {
                out.push(format!("[{}]", escape(&key)));
                continue;
            }
        }
        // Counted across the effective siblings, so two wrappers holding same-named children
        // still produce one address each.
        let ordinal = children[..i].iter().filter(|(_, s)| s.name == node.name).count();
        out.push(if ordinal == 0 {
            escape(&node.name)
        } else {
            format!("{}#{}", escape(&node.name), ordinal)
        });
    }
    out
}

/// The address segment of each immediate child of `parent`, in order.
pub fn child_segments(parent: &OverseerNode) -> Vec<String> {
    segments_for(Some(parent), &effective_children(parent))
}

/// The address segment of each root node, in order.
pub fn root_segments(nodes: &[OverseerNode]) -> Vec<String> {
    segments_for(None, &effective_roots(nodes))
}

/// Visit every node with its address, outermost first.
pub fn walk<'a, F: FnMut(&str, &'a OverseerNode)>(nodes: &'a [OverseerNode], visit: &mut F) {
    fn go<'a, F: FnMut(&str, &'a OverseerNode)>(
        parent: Option<&'a OverseerNode>,
        children: &[(Vec<usize>, &'a OverseerNode)],
        prefix: &str,
        visit: &mut F,
    ) {
        let segments = segments_for(parent, children);
        for (segment, (_, node)) in segments.iter().zip(children) {
            let address = if prefix.is_empty() {
                segment.clone()
            } else {
                format!("{}/{}", prefix, segment)
            };
            visit(&address, node);
            go(Some(node), &effective_children(node), &address, visit);
        }
    }
    go(None, &effective_roots(nodes), "", visit);
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
/// Two schemes, and both are needed. An address survives a resolve and is what a caller names
/// a node by; an action resolves a path of names, and it does *not* look through the unnamed
/// wrappers an address skips - so this path is often longer than the address that produced it.
/// Segments carry an ordinal where siblings share a name.
pub fn name_path(nodes: &[OverseerNode], address: &str) -> Option<Vec<String>> {
    fn names_along(children: &[OverseerNode], path: &[usize]) -> Vec<String> {
        let mut out = Vec::new();
        let mut level = children;
        for &index in path {
            let node = &level[index];
            let ordinal = level[..index]
                .iter()
                .filter(|sibling| sibling.name == node.name)
                .count();
            out.push(if ordinal == 0 {
                node.name.clone()
            } else {
                format!("{}#{}", node.name, ordinal)
            });
            level = &node.children;
        }
        out
    }

    fn go(
        parent: Option<&OverseerNode>,
        siblings: &[OverseerNode],
        prefix: &str,
        names: &[String],
        target: &str,
    ) -> Option<Vec<String>> {
        let children = match parent {
            Some(node) => effective_children(node),
            None => effective_roots(siblings),
        };
        let segments = segments_for(parent, &children);
        for (segment, (index_path, node)) in segments.iter().zip(&children) {
            let here = if prefix.is_empty() {
                segment.clone()
            } else {
                format!("{}/{}", prefix, segment)
            };
            let mut named = names.to_vec();
            named.extend(names_along(siblings, index_path));
            if here == target {
                return Some(named);
            }
            if target.starts_with(&format!("{}/", here)) {
                if let Some(found) = go(Some(node), &node.children, &here, &named, target) {
                    return Some(found);
                }
            }
        }
        None
    }
    go(None, nodes, "", &[], address)
}
