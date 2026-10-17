//! Contexts: names whose value stacks down the hierarchy.
//!
//! `context theme { - accent = #2563eb }` declares a value for everything inside the container it
//! is written in, and a `context theme { - accent = red }` nearer down overrides it for what is
//! inside *that* container, the way an instance overrides its template: a named element replaces
//! the one it matches, `+` adds one, `base=` starts from another node altogether. Two declarations
//! of one name in the same container merge in the order they are written.
//!
//! A declaration is kept exactly as it was written - it is the document's text, and the writer
//! gives it back as such. What a container *sees* is worked out here and put in front of its
//! children as a copy marked `CONTEXT_COPY`: the context from above with every declaration of the
//! container merged onto it. A container that declares any context is given a copy of every
//! context visible there, not only of the ones it declares, so that a context's formulas are
//! worked out where they are used: a theme whose `line` is derived from `text` follows a nearer
//! `text`, and `context mode = "dark"` re-derives every theme below it.
//!
//! Being first among its container's children, a copy is what a formula reading `theme/accent`
//! meets as it climbs from where it is - the existing lookup, with nothing taught about contexts
//! but to leave them out of searches that go *down* into other containers, where a copy would be
//! some other container's context. Nothing else counts a context among a container's contents:
//! not a method chain over it, not the page, not the file.

use crate::types::{OverseerNode, OverseerValue};

/// The type a declaration is written with.
pub const CONTEXT: &str = "context";

/// The parameter marking a worked-out copy, which the writer leaves out of the file.
pub const CONTEXT_COPY: &str = "_context_copy";

/// Whether a node is a context: a declaration as written, or a copy worked out from them.
pub fn is_context(node: &OverseerNode) -> bool {
    node.node_type == CONTEXT
}

/// Whether a node is a copy worked out here, rather than a declaration the document wrote.
pub fn is_copy(node: &OverseerNode) -> bool {
    is_context(node)
        && matches!(
            node.parameters.get(CONTEXT_COPY),
            Some(OverseerValue::Boolean(true))
        )
}

/// Whether a node is a declaration as the document wrote it. Its formulas are not worked out
/// where it stands: they are worked out in the copies, where they mean something.
pub fn is_declaration(node: &OverseerNode) -> bool {
    is_context(node) && !is_copy(node)
}

/// Whether anything in the tree holds a copy - the writer's question.
pub fn holds_a_copy(nodes: &[OverseerNode]) -> bool {
    nodes
        .iter()
        .any(|node| is_copy(node) || holds_a_copy(&node.children))
}

/// Take every copy out of the tree.
pub fn leave_out_copies(nodes: &mut Vec<OverseerNode>) {
    if !holds_a_copy(nodes) {
        return;
    }
    nodes.retain(|node| !is_copy(node));
    for node in nodes.iter_mut() {
        leave_out_copies(&mut node.children);
    }
}

/// Whether the tree declares any context at all. Most documents do not, and pay only this walk.
fn declares_any(nodes: &[OverseerNode]) -> bool {
    nodes
        .iter()
        .any(|node| is_context(node) || declares_any(&node.children))
}

/// What a container sees, by name, in the order the names were first declared.
type Visible = Vec<(String, OverseerNode)>;

fn visible<'a>(seen: &'a Visible, name: &str) -> Option<&'a OverseerNode> {
    seen.iter().find(|(n, _)| n == name).map(|(_, node)| node)
}

fn set_visible(seen: &mut Visible, name: &str, node: OverseerNode) {
    match seen.iter_mut().find(|(n, _)| n == name) {
        Some(slot) => slot.1 = node,
        None => seen.push((name.to_string(), node)),
    }
}

/// What the walk found to do, done once it is over: the tree is only read while it walks, so that
/// a `base=` can be looked up in it.
#[derive(Default)]
struct Work {
    /// Where copies go - the indices of a container from the top, empty for the document itself -
    /// and the copies, in the order they are put in front of its children.
    copies: Vec<(Vec<usize>, Vec<OverseerNode>)>,
    /// Nodes refused, and why.
    refused: Vec<(Vec<usize>, String)>,
}

/// Work out what every container sees, and put it in front of the container's children.
///
/// Run on a tree whose templates are instantiated, so that a context an entry's template declares
/// is there in every entry. Run again on the same tree it does the same: the copies of the last
/// time are taken out first, and everything is worked out again from the declarations.
pub fn resolve_contexts(nodes: &mut Vec<OverseerNode>) {
    leave_out_copies(nodes);
    if !declares_any(nodes) {
        return;
    }
    let mut work = Work::default();
    walk_scope(nodes, nodes, &Vec::new(), &mut Vec::new(), &mut work);

    for (at, why) in work.refused {
        if let Some(node) = node_at_mut(nodes, &at) {
            node.parameters.insert(
                crate::bases::ERROR.to_string(),
                OverseerValue::String(why),
            );
        }
    }
    // Deepest first: putting copies in front of a container's children moves the place of every
    // child in it, and with it the indices recorded for any container below.
    for (at, copies) in work.copies.into_iter().rev() {
        let children = if at.is_empty() {
            Some(&mut *nodes)
        } else {
            node_at_mut(nodes, &at).map(|node| &mut node.children)
        };
        if let Some(children) = children {
            children.splice(0..0, copies);
        }
    }
}

fn node_at_mut<'a>(nodes: &'a mut [OverseerNode], at: &[usize]) -> Option<&'a mut OverseerNode> {
    let (first, rest) = at.split_first()?;
    let mut node = nodes.get_mut(*first)?;
    for index in rest {
        node = node.children.get_mut(*index)?;
    }
    Some(node)
}

/// The nodes of one container's level, looked through its wrappers - an unnamed div or tab, an
/// unnamed instance - with the indices of each from the top. A context written in a wrapper is
/// the container's, as anything else written there is: a wrapper adds no step to any address, and
/// putting one in or taking it out changes nothing a formula means.
fn level_of<'a>(
    children: &'a [OverseerNode],
    at: &[usize],
    out: &mut Vec<(Vec<usize>, &'a OverseerNode)>,
) {
    for (index, child) in children.iter().enumerate() {
        let mut here = at.to_vec();
        here.push(index);
        if crate::addressing::is_wrapper(child) && !is_context(child) {
            level_of(&child.children, &here, out);
        } else {
            out.push((here, child));
        }
    }
}

/// One container: `children` are its children, `at` where it is.
fn walk_scope(
    root: &[OverseerNode],
    children: &[OverseerNode],
    at: &Vec<usize>,
    seen: &mut Visible,
    work: &mut Work,
) {
    let mut level = Vec::new();
    level_of(children, at, &mut level);

    let declared: Vec<&OverseerNode> = level
        .iter()
        .map(|(_, node)| *node)
        .filter(|node| is_context(node))
        .collect();
    let mut here: Visible = seen.clone();
    if !declared.is_empty() {
        for declaration in declared {
            let worked_out = override_with(root, visible(&here, &declaration.name), declaration);
            set_visible(&mut here, &declaration.name, worked_out);
        }
        let copies = here
            .iter()
            .map(|(_, node)| node.clone())
            .collect::<Vec<_>>();
        work.copies.push((at.clone(), copies));
    }

    for (place, node) in &level {
        if is_context(node) {
            continue;
        }
        if !node.name.is_empty() && visible(&here, &node.name).is_some() {
            work.refused.push((
                place.clone(),
                format!("'{}' is a context here, and cannot name anything else", node.name),
            ));
        }
        // Not into what is out of view, which nothing works out, nor into what a press runs,
        // which is never drawn - a context copied into an action would be copied with it into
        // whatever the action makes.
        if crate::resolver::out_of_view(node) || node.node_type == "on" {
            continue;
        }
        walk_scope(root, &node.children, place, &mut here, work);
    }
}

/// A declaration merged onto what it overrides: the context visible from above, or the node its
/// `base=` names. With neither, the declaration is the whole of it.
fn override_with(
    root: &[OverseerNode],
    above: Option<&OverseerNode>,
    declaration: &OverseerNode,
) -> OverseerNode {
    let mut refused = None;
    let based = match declaration.parameters.get("base") {
        Some(OverseerValue::Template(path)) => match crate::bases::find_or_why(root, path) {
            Ok(Some(node)) => Some(node),
            Ok(None) => {
                refused = Some(format!("no '{}' to start from", crate::bases::reference(path).name));
                None
            }
            Err(why) => {
                refused = Some(why);
                None
            }
        },
        _ => None,
    };
    let start = based.as_ref().or(above);

    let mut worked_out = match start {
        Some(start) => {
            let mut copy = start.clone();
            for (key, value) in &declaration.parameters {
                if key != "base" {
                    copy.parameters.insert(key.clone(), value.clone());
                }
            }
            if !declaration.children.is_empty() {
                crate::resolver::merge_node(&mut copy, &declaration.children);
            }
            copy
        }
        None => declaration.clone(),
    };
    worked_out.name = declaration.name.clone();
    worked_out.node_type = CONTEXT.to_string();
    worked_out.template = None;
    worked_out.is_hierarchy_transparent = false;
    worked_out.parameters.remove("base");
    // What the walk of the block left on the merged nodes is the writer's, and this is never
    // written.
    worked_out.parameters.remove(crate::bases::ERROR);
    if let Some(why) = refused {
        worked_out
            .parameters
            .insert(crate::bases::ERROR.to_string(), OverseerValue::String(why));
    }
    worked_out
        .parameters
        .insert(CONTEXT_COPY.to_string(), OverseerValue::Boolean(true));
    forget_the_text(&mut worked_out);
    worked_out
}

/// A copy is no text of the document's: nothing of it is to be written, or found by the writer as
/// though it were.
fn forget_the_text(node: &mut OverseerNode) {
    node.source_snapshot = None;
    node.source_id = None;
    node.source_fingerprint = None;
    for child in node.children.iter_mut() {
        forget_the_text(child);
    }
}
