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
//! container merged onto it. A container is given a copy of each context it declares, and of each
//! context whose formulas read one of those, so that a context's formulas are worked out where
//! what they read changes: a theme whose `line` is derived from `text` follows a nearer `text`,
//! and `context mode = "dark"` re-derives every theme below it. A context that reads nothing
//! declared there is the same as the copy above, and a lookup climbs on to that one - an entry
//! whose template declares one context carries one copy, not one of every context in view.
//!
//! The same holds inside a copy: it keeps only the keys its container changes - those declared
//! there, those whose formulas read one of them or a context that changed - and what those read.
//! A key it leaves out has the value of the copy above, where a read climbs on to, and which the
//! page's stylesheet hands down: a comment that says one backdrop does not carry the whole theme,
//! once for each comment. A key that reads anything but the context's own keys and other contexts
//! stays, for what it reads may be another thing here. A copy stays whole where there is nothing
//! to climb on to, or where a `base=` starts it over.
//!
//! Being first among its container's children, a copy is what a formula reading `theme/accent`
//! meets as it climbs from where it is - the existing lookup, with nothing taught about contexts
//! but to leave them out of searches that go *down* into other containers, where a copy would be
//! some other container's context. Nothing else counts a context among a container's contents:
//! not a method chain over it, not the page, not the file.
//!
//! Above a document's top is the environment - the user's theme and mode, see `environment` - which
//! is seen there as though declared before anything the document declares. Its names are the
//! document's only until the document uses one for something else: a field named `mode` hides the
//! environment's `mode` from what is inside it, where a context the document declares itself would
//! be refused, since a document that never asked for a theme cannot be told its names are taken.

use crate::types::{OverseerNode, OverseerValue};
use std::rc::Rc;

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

/// One name a container sees: its worked-out node, whether the document declared it rather than
/// only the environment, and the words its formulas are written with.
#[derive(Clone)]
struct Seen {
    name: String,
    node: OverseerNode,
    declared: bool,
    reads: Vec<String>,
    /// The same for each of the node's children, in order.
    key_reads: Vec<Vec<String>>,
}

/// What a container sees, by name, in the order the names were first declared. Shared, for every
/// container walked starts from what the one around it sees, and a theme is dozens of nodes.
type Visible = Vec<Rc<Seen>>;

fn visible<'a>(seen: &'a Visible, name: &str) -> Option<&'a OverseerNode> {
    seen.iter().find(|s| s.name == name).map(|s| &s.node)
}

fn set_visible(seen: &mut Visible, name: &str, node: OverseerNode, declared: bool) {
    let mut reads = Vec::new();
    words_of(&node, &mut reads);
    let key_reads = node
        .children
        .iter()
        .map(|key| {
            let mut words = Vec::new();
            words_of(key, &mut words);
            words
        })
        .collect();
    let slot = seen.iter_mut().find(|s| s.name == name);
    let declared = declared || slot.as_ref().is_some_and(|s| s.declared);
    let worked_out = Rc::new(Seen {
        name: name.to_string(),
        node,
        declared,
        reads,
        key_reads,
    });
    match slot {
        Some(slot) => *slot = worked_out,
        None => seen.push(worked_out),
    }
}

/// Every word the formulas in a node are written with - a name, or a step of an address. More
/// than the names a formula reads (a word inside a quoted string counts too), which costs at most a
/// copy that was not needed, never a value worked out in the wrong place.
fn words_of(node: &OverseerNode, out: &mut Vec<String>) {
    let word = |c: char| c.is_alphanumeric() || c == '_' || c == '-';
    for value in node.parameters.values() {
        if let OverseerValue::Formula(text) = value {
            for found in text.split(|c: char| !word(c)) {
                if !found.is_empty() && !out.iter().any(|w| w == found) {
                    out.push(found.to_string());
                }
            }
        }
    }
    for child in &node.children {
        words_of(child, out);
    }
}

/// The names a container is to carry copies of: those it declares, and every context that reads
/// one of them, or reads a context that does, and so on.
fn changed_here(here: &Visible, declared: &[&OverseerNode]) -> Vec<bool> {
    let mut changed: Vec<bool> = here
        .iter()
        .map(|s| declared.iter().any(|d| d.name == s.name))
        .collect();
    loop {
        let mut grew = false;
        for (index, seen) in here.iter().enumerate() {
            if changed[index] {
                continue;
            }
            let reads_one = here
                .iter()
                .zip(&changed)
                .any(|(other, &is)| is && seen.reads.iter().any(|w| *w == other.name));
            if reads_one {
                changed[index] = true;
                grew = true;
            }
        }
        if !grew {
            return changed;
        }
    }
}

/// The names of a declaration's keys, looked through wrappers.
fn key_names<'a>(nodes: &'a [OverseerNode], out: &mut Vec<&'a str>) {
    for node in nodes {
        if crate::addressing::is_wrapper(node) {
            key_names(&node.children, out);
        } else {
            out.push(&node.name);
        }
    }
}

/// Which keys of a context a container's copy keeps, one for each child of the context. See the
/// top of this file for what and why. `changed` names the contexts the container changes, `here`
/// is all it sees.
fn keys_held(context: &Seen, declared: &[&OverseerNode], changed: &[&str], here: &Visible) -> Vec<bool> {
    let keys = &context.node.children;
    let mut said = Vec::new();
    for declaration in declared.iter().filter(|d| d.name == context.name) {
        key_names(&declaration.children, &mut said);
    }
    let is_key = |word: &str| keys.iter().any(|key| key.name == word);
    let mut held: Vec<bool> = keys
        .iter()
        .zip(&context.key_reads)
        .map(|(key, words)| {
            key.name.is_empty()
                || said.contains(&key.name.as_str())
                || words.iter().any(|word| {
                    changed.contains(&word.as_str())
                        || !(is_key(word) || here.iter().any(|seen| seen.name == *word))
                })
        })
        .collect();
    // What reads a key that changed has changed, and so on.
    loop {
        let mut grew = false;
        for index in 0..keys.len() {
            let reads_one = context.key_reads[index]
                .iter()
                .any(|word| keys.iter().zip(&held).any(|(key, &is)| is && key.name == *word));
            if !held[index] && reads_one {
                held[index] = true;
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    // What a kept key reads is found beside it. That is no change: what reads *that* can go.
    loop {
        let mut grew = false;
        for index in 0..keys.len() {
            if !held[index] {
                continue;
            }
            for (other, key) in keys.iter().enumerate() {
                if !held[other] && context.key_reads[index].iter().any(|word| *word == key.name) {
                    held[other] = true;
                    grew = true;
                }
            }
        }
        if !grew {
            return held;
        }
    }
}

/// What a container's copy of a context holds. `above` is what it sees from above.
fn copy_of(context: &Seen, declared: &[&OverseerNode], changed: &[&str], here: &Visible, above: &Visible) -> OverseerNode {
    let starts_over = declared
        .iter()
        .any(|d| d.name == context.name && d.parameters.contains_key("base"));
    if starts_over || visible(above, &context.name).is_none() {
        return context.node.clone();
    }
    let mut held = keys_held(context, declared, changed, here).into_iter();
    let mut copy = context.node.clone();
    copy.children.retain(|_| held.next().unwrap_or(true));
    copy
}

/// What the walk found to do, done once it is over: the tree is only read while it walks, so that
/// a `base=` can be looked up in it.
#[derive(Default)]
struct Work {
    /// Where copies go - the indices of a container from the top, empty for the document itself -
    /// the copies, in the order they are put among its children, and whether after them.
    copies: Vec<(Vec<usize>, Vec<OverseerNode>, bool)>,
    /// Nodes refused, and why.
    refused: Vec<(Vec<usize>, String)>,
    /// The one container worked out, when only one is - see `resolve_contexts_within`: nothing
    /// off the way down to it is walked.
    only: Option<Vec<usize>>,
}

/// Work out what every container sees, and put it in front of the container's children.
///
/// Run on a tree whose templates are instantiated, so that a context an entry's template declares
/// is there in every entry. Run again on the same tree it does the same: the copies of the last
/// time are taken out first, and everything is worked out again from the declarations.
pub fn resolve_contexts(nodes: &mut Vec<OverseerNode>) {
    leave_out_copies(nodes);
    let environment = crate::environment::declarations();
    if environment.is_empty() && !declares_any(nodes) {
        return;
    }
    // The environment is worked out as the settings document's own top would be: each declaration
    // merged onto those before it. Its `base=` names a file, so there is no tree to look it up in.
    let mut seen = Visible::new();
    for declaration in &environment {
        let worked_out = override_with(&[], visible(&seen, &declaration.name), declaration);
        set_visible(&mut seen, &declaration.name, worked_out, false);
    }
    let mut work = Work::default();
    let at_the_top = !seen.is_empty();
    walk_scope(nodes, nodes, &Vec::new(), &mut seen, at_the_top, &mut work);

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
    for (at, copies, after) in work.copies.into_iter().rev() {
        let children = if at.is_empty() {
            Some(&mut *nodes)
        } else {
            node_at_mut(nodes, &at).map(|node| &mut node.children)
        };
        if let Some(children) = children {
            if after {
                children.extend(copies);
            } else {
                children.splice(0..0, copies);
            }
        }
    }
}

/// The same for one container and what is inside it, the rest of the tree left as it stands.
///
/// For an entry an action has just made. It is made from its template as the template's file was
/// read - for a project tracker, the library's - where nothing worked its contexts out, so it
/// holds the declarations and none of the copies a whole work-out gives every entry. The page
/// applies a change by the index of the node it is about, so the same text held in two shapes
/// sent changes two places along: a project item's Talk button drew as one labelled "moved".
/// Only the way down to the container is walked, for what it sees from above.
pub fn resolve_contexts_within(nodes: &mut Vec<OverseerNode>, at: &[usize]) {
    let Some(container) = node_at_mut(nodes, at) else {
        return;
    };
    leave_out_copies(&mut container.children);
    if !declares_any(&container.children) {
        return;
    }
    let environment = crate::environment::declarations();
    let mut seen = Visible::new();
    for declaration in &environment {
        let worked_out = override_with(&[], visible(&seen, &declaration.name), declaration);
        set_visible(&mut seen, &declaration.name, worked_out, false);
    }
    let mut work = Work { only: Some(at.to_vec()), ..Work::default() };
    let at_the_top = !seen.is_empty();
    walk_scope(nodes, nodes, &Vec::new(), &mut seen, at_the_top, &mut work);
    let within = |place: &Vec<usize>| place.starts_with(at);
    for (place, why) in work.refused.into_iter().filter(|(place, _)| within(place)) {
        if let Some(node) = node_at_mut(nodes, &place) {
            node.parameters.insert(crate::bases::ERROR.to_string(), OverseerValue::String(why));
        }
    }
    // Deepest first, as above. The containers on the way down hold their copies already.
    for (place, copies, after) in work.copies.into_iter().filter(|(place, _, _)| within(place)).rev() {
        if let Some(node) = node_at_mut(nodes, &place) {
            if after {
                node.children.extend(copies);
            } else {
                node.children.splice(0..0, copies);
            }
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
    always: bool,
    work: &mut Work,
) {
    let mut level = Vec::new();
    level_of(children, at, &mut level);

    let declared: Vec<&OverseerNode> = level
        .iter()
        .map(|(_, node)| *node)
        // Not a copy: a walk of one container meets those the containers above it already hold.
        .filter(|node| is_declaration(node))
        .collect();
    let mut here: Visible = seen.clone();
    // Where the document declares nothing, the copies above serve: but at its top there is nothing
    // above, and the environment is seen there only by being put there.
    if !declared.is_empty() || always {
        for declaration in &declared {
            let worked_out = override_with(root, visible(&here, &declaration.name), declaration);
            set_visible(&mut here, &declaration.name, worked_out, true);
        }
        // At the top every name is copied: there is nothing above for a lookup to climb on to.
        let carried = if always {
            vec![true; here.len()]
        } else {
            changed_here(&here, &declared)
        };
        let changed = here
            .iter()
            .zip(&carried)
            .filter(|(_, &carry)| carry)
            .map(|(seen, _)| seen.name.as_str())
            .collect::<Vec<_>>();
        let copies = here
            .iter()
            .zip(&carried)
            .filter(|(_, &carry)| carry)
            .map(|(seen_here, _)| {
                if always {
                    seen_here.node.clone()
                } else {
                    copy_of(seen_here, &declared, &changed, &here, seen)
                }
            })
            .collect::<Vec<_>>();
        // A copy goes first so that a lookup meets it before the declaration of the same name.
        // With no declaration here - only the environment, at the top of a document that declares
        // nothing - nothing shares its name, and it goes last, where it moves no node's place.
        work.copies.push((at.clone(), copies, declared.is_empty()));
    }

    for (place, node) in &level {
        if is_context(node) {
            continue;
        }
        let mut hides = None;
        if !node.name.is_empty() {
            match here.iter().position(|s| s.name == node.name) {
                Some(found) if here[found].declared => work.refused.push((
                    place.clone(),
                    format!("'{}' is a context here, and cannot name anything else", node.name),
                )),
                // Only the environment's: the node takes the name, for itself and what is inside.
                Some(found) => hides = Some(found),
                None => {}
            }
        }
        // Not into what is out of view, which nothing works out, nor into what a press runs,
        // which is never drawn - a context copied into an action would be copied with it into
        // whatever the action makes.
        if crate::resolver::not_worked_out(node) || node.node_type == "on" {
            continue;
        }
        if let Some(only) = &work.only {
            if !(only.starts_with(place) || place.starts_with(only)) {
                continue;
            }
        }
        match hides {
            Some(found) => {
                let mut inside = here.clone();
                inside.remove(found);
                walk_scope(root, &node.children, place, &mut inside, false, work);
            }
            None => walk_scope(root, &node.children, place, &mut here, false, work),
        }
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
                crate::resolver::merge_node(&mut copy, &declaration.children, root);
            }
            // An element of it that names a node to start from that is not there is said on the
            // copy, as a `base=` of the context's own is, rather than deep inside it.
            if refused.is_none() {
                refused = take_refusal(&mut copy.children);
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
    crate::environment::settle_mode(&mut worked_out);
    forget_the_text(&mut worked_out);
    worked_out
}

/// The first reason an element inside a copy could not start from the node it named, taken off it.
fn take_refusal(nodes: &mut [OverseerNode]) -> Option<String> {
    for node in nodes {
        if let Some(OverseerValue::String(why)) = node.parameters.remove(crate::bases::ERROR) {
            return Some(why);
        }
        if let Some(why) = take_refusal(&mut node.children) {
            return Some(why);
        }
    }
    None
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
