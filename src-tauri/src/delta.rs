//! What changed between two resolved documents.
//!
//! An interaction currently answers with the whole resolved document - about 9 MB for a large
//! one - when a field edit typically changes a handful of nodes. The IPC moves a couple of MB
//! per second, and the client rebuilds every element it is given, so the size of the answer is
//! most of what an interaction costs. Describing the change instead makes both proportional to
//! what actually moved.
//!
//! Nodes are matched by the addresses in [`crate::addressing`], which survive a resolve, so
//! "the node at this address now reads 3" is meaningful across two separate parses.
//!
//! A list whose entries came, went or moved is reported as its entries in their new order, each
//! one either kept from where it was in the recipient's document or sent whole - see
//! [`DocumentChange::Entries`]. The recipient builds the list's children from that in one step,
//! so no index arithmetic crosses from one change to another. Any other node whose children
//! changed shape is sent as a whole subtree, which is what every list used to be: a trade that
//! held while a change of shape was rare and a list small, and stopped holding at a history of
//! tasks - marking one done sent 4.5 MB of it.

use crate::addressing;
use crate::types::*;
use std::collections::{HashMap, HashSet};

/// Where a change applies.
///
/// `address` survives a resolve and is what the two documents are matched by. `path` is the
/// child index at each level, which is what the recipient applies it with: it needs no
/// knowledge of names, keys or ordinals, so the addressing scheme lives in one language
/// rather than being reimplemented on the other side of the boundary and drifting.
///
/// The index path is computed from the new document and is equally valid in the old one.
/// A change is only reported below an ancestor whose children matched segment for segment,
/// so every level above it holds the same nodes in the same order in both.
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DocumentChange {
    /// The node here carries different parameters; its shape is unchanged.
    Parameters {
        address: String,
        path: Vec<usize>,
        parameters: crate::types::Params,
    },
    /// The node here has a different shape, and is sent whole.
    Subtree {
        address: String,
        path: Vec<usize>,
        node: OverseerNode,
    },
    /// There is no longer a node here.
    Removed { address: String, path: Vec<usize> },
    /// The entries of the list here, in their new order - each kept from where it stood among the
    /// list's children in the recipient's document, or sent whole. An entry not named is gone.
    ///
    /// A kept entry carries its name, which a list naming its entries by place changes for every
    /// entry after one taken out or put in ahead of it, while its address - its key - stays. What
    /// changed inside a kept entry follows as changes of its own, with paths in the new order,
    /// so this one has to be applied before them.
    Entries {
        address: String,
        path: Vec<usize>,
        entries: Vec<Entry>,
    },
}

/// One entry of a list, as [`DocumentChange::Entries`] names it.
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
#[serde(untagged)]
pub enum Entry {
    /// The entry at this index among the list's children before, now called `name`.
    Kept { kept: usize, name: String },
    /// An entry the recipient has never had.
    New { node: OverseerNode },
}

impl DocumentChange {
    pub fn address(&self) -> &str {
        match self {
            DocumentChange::Parameters { address, .. }
            | DocumentChange::Subtree { address, .. }
            | DocumentChange::Removed { address, .. }
            | DocumentChange::Entries { address, .. } => address,
        }
    }

    pub fn path(&self) -> &[usize] {
        match self {
            DocumentChange::Parameters { path, .. }
            | DocumentChange::Subtree { path, .. }
            | DocumentChange::Removed { path, .. }
            | DocumentChange::Entries { path, .. } => path,
        }
    }
}

fn join(prefix: &str, segment: &str) -> String {
    if prefix.is_empty() {
        segment.to_string()
    } else {
        format!("{}/{}", prefix, segment)
    }
}

/// Everything that differs between two resolved documents.
///
/// An empty result means the two render identically.
pub fn diff(before: &[OverseerNode], after: &[OverseerNode]) -> Vec<DocumentChange> {
    let mut previous: HashMap<String, &OverseerNode> = HashMap::new();
    addressing::walk(before, &mut |address, node| {
        previous.insert(address.to_string(), node);
    });
    let previous_paths = index_paths(before);

    let mut changes = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut left_out: Vec<String> = Vec::new();
    compare(
        None,
        &addressing::effective_roots(after),
        "",
        &[],
        &previous,
        &mut Walk { seen: &mut seen, changes: &mut changes, left_out: &mut left_out },
    );

    // Whatever the previous document had here and this one does not. A node inside a subtree
    // that was replaced wholesale is already accounted for by that subtree, and an entry a list's
    // new entries leave out, with everything inside it, by the list.
    //
    // Nothing else below a list reported by its entries can be gone: a node goes only from a
    // parent whose children changed shape, and that parent is reported by its entries or whole.
    // So every removal left is located where the lists above it have not moved.
    let replaced: Vec<&str> = changes
        .iter()
        .filter(|c| matches!(c, DocumentChange::Subtree { .. }))
        .map(|c| c.address())
        .collect();
    let mut gone: Vec<&String> = previous
        .keys()
        .filter(|address| !seen.contains(*address))
        .filter(|address| {
            !replaced
                .iter()
                .any(|parent| address.starts_with(&format!("{}/", parent)))
        })
        .filter(|address| {
            !left_out
                .iter()
                .any(|entry| *address == entry || address.starts_with(&format!("{}/", entry)))
        })
        .collect();
    gone.sort();
    for address in gone {
        // A removal is located in the document that still has the node - the recipient's.
        let Some(path) = previous_paths.get(address) else {
            continue;
        };
        changes.push(DocumentChange::Removed {
            address: address.clone(),
            path: path.clone(),
        });
    }
    changes
}

/// The address of the node these child indices reach, if it has one - a wrapper has none.
pub fn address_at(nodes: &[OverseerNode], indices: &[usize]) -> Option<String> {
    index_paths(nodes)
        .into_iter()
        .find(|(_, path)| path.as_slice() == indices)
        .map(|(address, _)| address)
}

/// The child indices that reach an address, in this document.
pub fn indices_of(nodes: &[OverseerNode], address: &str) -> Option<Vec<usize>> {
    index_paths(nodes).remove(address)
}

/// Every address in a document with the child indices that reach it.
fn index_paths(nodes: &[OverseerNode]) -> HashMap<String, Vec<usize>> {
    fn go(
        parent: Option<&OverseerNode>,
        children: &[(Vec<usize>, &OverseerNode)],
        prefix: &str,
        path: &[usize],
        out: &mut HashMap<String, Vec<usize>>,
    ) {
        for (segment, (relative, node)) in addressing::segments_for(parent, children).iter().zip(children) {
            let address = join(prefix, segment);
            let mut here = path.to_vec();
            here.extend(relative.iter().copied());
            out.insert(address.clone(), here.clone());
            go(
                Some(node),
                &addressing::effective_children(node),
                &address,
                &here,
                out,
            );
        }
    }
    let mut out = HashMap::new();
    go(None, &addressing::effective_roots(nodes), "", &[], &mut out);
    out
}

/// What a walk of the new document collects.
struct Walk<'a> {
    /// Every address the new document has that the old one had too.
    seen: &'a mut HashSet<String>,
    changes: &'a mut Vec<DocumentChange>,
    /// The entries a list's new entries leave out, by their addresses in the old document.
    left_out: &'a mut Vec<String>,
}

fn compare(
    parent: Option<&OverseerNode>,
    children: &[(Vec<usize>, &OverseerNode)],
    prefix: &str,
    path: &[usize],
    previous: &HashMap<String, &OverseerNode>,
    walk: &mut Walk,
) {
    let segments = addressing::segments_for(parent, children);
    for (segment, (relative, node)) in segments.iter().zip(children) {
        let mut here = path.to_vec();
        here.extend(relative.iter().copied());
        compare_one(join(prefix, segment), here, node, previous, walk);
    }
}

fn compare_one(
    address: String,
    here: Vec<usize>,
    node: &OverseerNode,
    previous: &HashMap<String, &OverseerNode>,
    walk: &mut Walk,
) {
    walk.seen.insert(address.clone());

    let Some(was) = previous.get(&address) else {
        // Nothing was here before, so there is nothing to compare against.
        walk.changes.push(DocumentChange::Subtree { address, path: here, node: node.clone() });
        return;
    };

    let before_children = addressing::child_segments(was);
    let after_children = addressing::child_segments(node);
    if before_children != after_children {
        if let Some((entries, kept, left_out)) = entries_of(was, node, &before_children, &after_children) {
            if was.parameters != node.parameters {
                walk.changes.push(DocumentChange::Parameters {
                    address: address.clone(),
                    path: here.clone(),
                    parameters: node.parameters.clone(),
                });
            }
            walk.changes.push(DocumentChange::Entries {
                address: address.clone(),
                path: here.clone(),
                entries,
            });
            walk.left_out.extend(left_out.iter().map(|segment| join(&address, segment)));
            // What changed inside each entry kept, in the order the entries are in now.
            for at in kept {
                let mut inside = here.clone();
                inside.push(at);
                compare_one(join(&address, &after_children[at]), inside, &node.children[at], previous, walk);
            }
            return;
        }
        // The shape moved; send the subtree and stop descending. Its descendants stay out of
        // `seen` on purpose - they are covered by the subtree, and the removal pass skips
        // anything beneath a replaced address.
        walk.changes.push(DocumentChange::Subtree { address, path: here, node: node.clone() });
        return;
    }

    if was.parameters != node.parameters {
        walk.changes.push(DocumentChange::Parameters {
            address: address.clone(),
            path: here.clone(),
            parameters: node.parameters.clone(),
        });
    }
    compare(Some(node), &addressing::effective_children(node), &address, &here, previous, walk);
}

/// A node's children as entries kept or sent, when they can be told that way: with no wrapper
/// among them, so each child is one step of an address, and with at least one kept - otherwise
/// the whole node says the same in fewer words. Matched by address, as everything here is, so in
/// a list naming its entries by place an entry kept may be a neighbour of the one it was, with
/// what differs following as changes of its own.
///
/// The entries, the indices of those kept among the new children, and the old steps of those
/// left out.
fn entries_of(
    was: &OverseerNode,
    node: &OverseerNode,
    before: &[String],
    after: &[String],
) -> Option<(Vec<Entry>, Vec<usize>, Vec<String>)> {
    let plain = |n: &OverseerNode| !n.children.iter().any(addressing::is_wrapper);
    if !plain(was) || !plain(node) || before.len() != was.children.len() || after.len() != node.children.len() {
        return None;
    }
    let stood: HashMap<&str, usize> = before.iter().enumerate().map(|(at, s)| (s.as_str(), at)).collect();
    let mut entries = Vec::with_capacity(after.len());
    let mut kept = Vec::new();
    let mut taken: HashSet<usize> = HashSet::new();
    for (at, segment) in after.iter().enumerate() {
        match stood.get(segment.as_str()) {
            Some(&from) => {
                taken.insert(from);
                kept.push(at);
                entries.push(Entry::Kept { kept: from, name: node.children[at].name.clone() });
            }
            None => entries.push(Entry::New { node: node.children[at].clone() }),
        }
    }
    if kept.is_empty() {
        return None;
    }
    let left_out = before
        .iter()
        .enumerate()
        .filter(|(at, _)| !taken.contains(at))
        .map(|(_, segment)| segment.clone())
        .collect();
    Some((entries, kept, left_out))
}

/// Apply changes to the document they were worked out against, as the page does.
///
/// The page has its own copy, in `applyDocumentChanges`; this one is what the tests hold an
/// answer to - that applied to the document before, it gives the document after. In order,
/// because what changed inside a list's entries is located in their new order, and removals
/// last and from the end, because they are located in the document as it was.
pub fn apply(nodes: &mut Vec<OverseerNode>, changes: Vec<DocumentChange>) {
    fn siblings<'a>(nodes: &'a mut Vec<OverseerNode>, parent: &[usize]) -> Option<&'a mut Vec<OverseerNode>> {
        let mut list = nodes;
        for at in parent {
            list = &mut list.get_mut(*at)?.children;
        }
        Some(list)
    }
    fn at<'a>(nodes: &'a mut Vec<OverseerNode>, path: &[usize]) -> Option<&'a mut OverseerNode> {
        let (last, parent) = path.split_last()?;
        siblings(nodes, parent)?.get_mut(*last)
    }
    let mut removals = Vec::new();
    for change in changes {
        match change {
            DocumentChange::Parameters { path, parameters, .. } => {
                if let Some(node) = at(nodes, &path) {
                    node.parameters = parameters;
                }
            }
            DocumentChange::Subtree { path, node, .. } => {
                let Some((last, parent)) = path.split_last() else { continue };
                let Some(list) = siblings(nodes, parent) else { continue };
                if *last == list.len() {
                    list.push(node);
                } else if let Some(slot) = list.get_mut(*last) {
                    *slot = node;
                }
            }
            DocumentChange::Entries { path, entries, .. } => {
                let Some(list) = at(nodes, &path) else { continue };
                let mut before: Vec<Option<OverseerNode>> =
                    std::mem::take(&mut list.children).into_iter().map(Some).collect();
                list.children = entries
                    .into_iter()
                    .filter_map(|entry| match entry {
                        Entry::Kept { kept, name } => before.get_mut(kept)?.take().map(|mut node| {
                            node.name = name;
                            node
                        }),
                        Entry::New { node } => Some(node),
                    })
                    .collect();
            }
            DocumentChange::Removed { path, .. } => removals.push(path),
        }
    }
    removals.sort();
    for path in removals.into_iter().rev() {
        let Some((last, parent)) = path.split_last() else { continue };
        if let Some(list) = siblings(nodes, parent) {
            if *last < list.len() {
                list.remove(*last);
            }
        }
    }
}
