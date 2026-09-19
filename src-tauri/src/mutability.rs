//! Whether a node may be changed, and whether the change is meant to last.
//!
//! `mutable` has been a renderer idea: the three states, the inheritance up the tree and the
//! shadow a formula leaves all lived in `renderer.js`, and Rust had never heard of the parameter.
//! That was survivable while the only thing acting on it was the page - it decided what could be
//! typed into, and it collected the guarded fields to restore at save time.
//!
//! It stops being survivable as soon as anything else writes. The bot writes through `/v1`,
//! where the write *is* the action and there is no save step to hang a revert on, so a field
//! marked `guarded` is honoured in one door and ignored in the other. Answering the question
//! here, once, is what lets both doors agree.
//!
//! Three states, and the third is the interesting one:
//!
//!   - **fixed** - the page offers no way to change it.
//!   - **editable** - changes are the document's, and are written to it.
//!   - **guarded** - changes are the viewer's. Which day the food tracker is showing, whether a
//!     card is folded, what a filter box holds. Real while you are looking, and not a fact about
//!     anybody once you stop.
//!
//! Stated anywhere up the tree and inherited down, so a tab says `mutable=true` once rather than
//! every field repeating it. Unstated all the way to the root means fixed: a document that says
//! nothing about being edited is a document to read.

use crate::addressing::{effective_children, effective_roots, segments_for};
use crate::types::{OverseerNode, OverseerValue};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mutability {
    /// Not offered for editing.
    Fixed,
    /// Edited, and the edit belongs in the document.
    Editable,
    /// Edited, and the edit belongs to whoever is looking.
    Guarded,
}

impl Mutability {
    pub fn is_guarded(self) -> bool {
        matches!(self, Mutability::Guarded)
    }
}

/// What a node says about itself, or nothing when it leaves the question to its parent.
///
/// The computed shadow first: `mutable` may be a formula, and the formula's answer is what the
/// rest of the document sees. Anything unrecognised is read as saying nothing rather than as
/// saying no - a typo should inherit, not silently freeze a subtree.
fn stated(node: &OverseerNode) -> Option<Mutability> {
    let held = node
        .parameters
        .get("_computed_mutable")
        .or_else(|| node.parameters.get("mutable"))?;
    match held {
        OverseerValue::Boolean(true) => Some(Mutability::Editable),
        OverseerValue::Boolean(false) => Some(Mutability::Fixed),
        OverseerValue::String(said) => match said.trim().to_ascii_lowercase().as_str() {
            "true" => Some(Mutability::Editable),
            "false" => Some(Mutability::Fixed),
            "guarded" => Some(Mutability::Guarded),
            _ => None,
        },
        _ => None,
    }
}

/// Every node from the root down to an address, wrappers included.
///
/// Addresses skip unnamed wrappers, which is right for naming things and wrong for this: a
/// wrapper is still somewhere `mutable` can be written, and a subtree that inherits through one
/// would otherwise inherit from the wrong place.
fn chain<'a>(nodes: &'a [OverseerNode], address: &str) -> Option<Vec<&'a OverseerNode>> {
    let mut found: Vec<&OverseerNode> = Vec::new();
    let mut parent: Option<&OverseerNode> = None;
    let mut slice: &[OverseerNode] = nodes;

    for wanted in address.split('/').filter(|s| !s.is_empty()) {
        let children = match parent {
            None => effective_roots(nodes),
            Some(node) => effective_children(node),
        };
        let segments = segments_for(parent, &children);
        let at = segments.iter().position(|segment| segment == wanted)?;
        let (indices, node) = &children[at];

        // The indices reach the node through whatever wrappers stand between, and those are
        // nodes too.
        let mut cursor = slice;
        for (step, &index) in indices.iter().enumerate() {
            let here = cursor.get(index)?;
            if step + 1 < indices.len() {
                found.push(here);
            }
            cursor = &here.children;
        }

        found.push(node);
        parent = Some(node);
        slice = &node.children;
    }

    if found.is_empty() {
        None
    } else {
        Some(found)
    }
}

/// How a node at an address may be changed.
///
/// The nearest ancestor that states anything decides, itself included. Nothing stated anywhere,
/// or no node there at all, is fixed.
pub fn at(nodes: &[OverseerNode], address: &str) -> Mutability {
    let Some(found) = chain(nodes, address) else {
        return Mutability::Fixed;
    };
    for node in found.iter().rev() {
        if let Some(said) = stated(node) {
            return said;
        }
    }
    Mutability::Fixed
}

/// Whether a write to this address belongs to the viewer rather than to the document.
pub fn is_view_state(nodes: &[OverseerNode], address: &str) -> bool {
    at(nodes, address).is_guarded()
}

/// The same question, asked where a write happens.
///
/// An action resolves its target to a path of child indices, and that path is the ancestor chain
/// already - every node on it, wrappers included, in order. Cheaper and more exact than turning
/// it back into an address and looking it up again.
pub fn along(nodes: &[OverseerNode], indices: &[usize]) -> Mutability {
    let mut chain: Vec<&OverseerNode> = Vec::new();
    let mut level = nodes;
    for &index in indices {
        let Some(node) = level.get(index) else { break };
        chain.push(node);
        level = &node.children;
    }
    for node in chain.iter().rev() {
        if let Some(said) = stated(node) {
            return said;
        }
    }
    Mutability::Fixed
}
