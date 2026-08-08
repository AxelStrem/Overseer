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
//! A node whose children changed shape is reported as a whole subtree rather than as a
//! sequence of insertions and removals. That is a deliberate trade: a shape change is rare
//! next to a value change, the subtree is usually small next to the document, and it spares
//! both sides an index arithmetic that is easy to get subtly wrong.

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
        parameters: HashMap<String, OverseerValue>,
    },
    /// The node here has a different shape, and is sent whole.
    Subtree {
        address: String,
        path: Vec<usize>,
        node: OverseerNode,
    },
    /// There is no longer a node here.
    Removed { address: String, path: Vec<usize> },
}

impl DocumentChange {
    pub fn address(&self) -> &str {
        match self {
            DocumentChange::Parameters { address, .. }
            | DocumentChange::Subtree { address, .. }
            | DocumentChange::Removed { address, .. } => address,
        }
    }

    pub fn path(&self) -> &[usize] {
        match self {
            DocumentChange::Parameters { path, .. }
            | DocumentChange::Subtree { path, .. }
            | DocumentChange::Removed { path, .. } => path,
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
    compare(
        &addressing::root_segments(after),
        after,
        "",
        &[],
        &previous,
        &mut seen,
        &mut changes,
    );

    // Whatever the previous document had here and this one does not. A node inside a subtree
    // that was replaced wholesale is already accounted for by that subtree.
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

/// Every address in a document with the child indices that reach it.
fn index_paths(nodes: &[OverseerNode]) -> HashMap<String, Vec<usize>> {
    fn go(
        segments: &[String],
        nodes: &[OverseerNode],
        prefix: &str,
        path: &[usize],
        out: &mut HashMap<String, Vec<usize>>,
    ) {
        for (i, (segment, node)) in segments.iter().zip(nodes).enumerate() {
            let address = join(prefix, segment);
            let mut here = path.to_vec();
            here.push(i);
            out.insert(address.clone(), here.clone());
            go(
                &addressing::child_segments(node),
                &node.children,
                &address,
                &here,
                out,
            );
        }
    }
    let mut out = HashMap::new();
    go(&addressing::root_segments(nodes), nodes, "", &[], &mut out);
    out
}

fn compare(
    segments: &[String],
    nodes: &[OverseerNode],
    prefix: &str,
    path: &[usize],
    previous: &HashMap<String, &OverseerNode>,
    seen: &mut HashSet<String>,
    changes: &mut Vec<DocumentChange>,
) {
    for (index, (segment, node)) in segments.iter().zip(nodes).enumerate() {
        let address = join(prefix, segment);
        let mut here = path.to_vec();
        here.push(index);
        seen.insert(address.clone());

        let Some(was) = previous.get(&address) else {
            // Nothing was here before, so there is nothing to compare against.
            changes.push(DocumentChange::Subtree {
                address,
                path: here,
                node: node.clone(),
            });
            continue;
        };

        let before_children = addressing::child_segments(was);
        let after_children = addressing::child_segments(node);
        if before_children != after_children {
            // The shape moved; send the subtree and stop descending. Its descendants stay out
            // of `seen` on purpose - they are covered by the subtree, and the removal pass
            // skips anything beneath a replaced address.
            changes.push(DocumentChange::Subtree {
                address,
                path: here,
                node: node.clone(),
            });
            continue;
        }

        if was.parameters != node.parameters {
            changes.push(DocumentChange::Parameters {
                address: address.clone(),
                path: here.clone(),
                parameters: node.parameters.clone(),
            });
        }
        compare(
            &after_children,
            &node.children,
            &address,
            &here,
            previous,
            seen,
            changes,
        );
    }
}
