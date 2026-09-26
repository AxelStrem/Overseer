//! The logic behind the Tauri commands, separated from them so it can be tested.
//!
//! The commands in `main.rs` live in the binary crate, which no integration test can reach.
//! While the real load/save path was only reachable through the app, every test of it was a
//! reimplementation of what those commands were assumed to do - and a document-mangling bug
//! sat in the gap between assumption and behaviour for some time, reproducible by hand and
//! by nothing else. What the app runs and what the tests run is now the same code.

use crate::actions::ActionExecutor;
use crate::file_ops::OverseerFileHandler;
use crate::parser::parse_document;
use crate::resolver;
use crate::types::*;

/// Re-serialize a document through parse and resolve, the way saving does.
///
/// Both save commands do this so snapshot-driven trivia is reapplied without needing the
/// original text. On a parse failure the input is returned untouched: refusing to write is
/// worse than writing exactly what the caller asked for.
pub fn canonicalize_document(text: &str) -> String {
    match parse_document(text) {
        Ok((_rem, mut nodes)) => {
            resolver::resolve_document(&mut nodes);
            OverseerFileHandler::serialize_nodes(&nodes).unwrap_or_else(|_| text.to_string())
        }
        Err(_) => text.to_string(),
    }
}

/// Run an event against a document, as `execute_overseer_event` does.
pub fn execute_event(
    nodes: &mut Vec<OverseerNode>,
    node_path: &[String],
    event_name: &str,
) -> Result<()> {
    ActionExecutor::execute_event(nodes, node_path, event_name)
}

pub(crate) fn get_field_value_by_path(nodes: &[OverseerNode], path: &str) -> Option<OverseerValue> {
    let path_parts: Vec<&str> = path.split('/').collect();
    if path_parts.is_empty() {
        return None;
    }
    // Try strict first
    if let Some(node) = find_node_by_path(nodes, &path_parts) {
        return node.parameters.get("value").cloned();
    }
    // Fallback: transparency-aware search allowing skipped unnamed/transparent wrappers
    find_node_by_path_transparent(nodes, &path_parts)
        .and_then(|node| node.parameters.get("value").cloned())
}

// Helper: transparency-aware path resolution allowing segments to skip unnamed or hierarchy-transparent wrappers.
pub(crate) fn find_node_by_path_transparent<'a>(
    nodes: &'a [OverseerNode],
    path_parts: &[&str],
) -> Option<&'a OverseerNode> {
    if path_parts.is_empty() {
        return None;
    }
    // Depth-first search matching first remaining segment; transparent nodes may be skipped.
    fn dfs<'b>(cur_slice: &'b [OverseerNode], remaining: &[&str]) -> Option<&'b OverseerNode> {
        if remaining.is_empty() {
            return None;
        }
        let target = remaining[0];
        for node in cur_slice {
            if node.name == target {
                // direct match consumes segment
                if remaining.len() == 1 {
                    return Some(node);
                }
                let found = dfs(&node.children, &remaining[1..]);
                if found.is_some() {
                    return found;
                }
            }
            // If transparent OR unnamed (blank name), attempt to match without consuming segment (skip wrapper)
            if node.is_hierarchy_transparent || node.name.is_empty() {
                if let Some(f) = dfs(&node.children, remaining) {
                    return Some(f);
                }
            }
        }
        None
    }
    dfs(nodes, path_parts)
}

// Helper function to set a field value by path
pub(crate) fn find_node_by_path_mut_transparent<'a>(
    nodes: &'a mut [OverseerNode],
    path_parts: &[&str],
) -> Option<&'a mut OverseerNode> {
    if path_parts.is_empty() {
        return None;
    }
    // Safe recursive DFS allowing skips over unnamed / transparent wrappers.
    fn dfs<'b>(nodes: &'b mut [OverseerNode], remaining: &[&str]) -> Option<&'b mut OverseerNode> {
        if remaining.is_empty() {
            return None;
        }
        let target = remaining[0];
        let (base_name, ordinal) = if target.contains('#') {
            let parts: Vec<&str> = target.split('#').collect();
            (
                parts[0],
                parts
                    .get(1)
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(0),
            )
        } else {
            (target, 0)
        };

        // Iterate once, using raw pointer only for the focused child to appease borrow checker
        let len = nodes.len();
        let nodes_ptr: *mut OverseerNode = nodes.as_mut_ptr();
        let mut seen = 0usize;
        for i in 0..len {
            unsafe {
                let node = &mut *nodes_ptr.add(i);
                if node.name == base_name {
                    if seen == ordinal {
                        if remaining.len() == 1 {
                            return Some(node);
                        }
                        return dfs(&mut node.children, &remaining[1..]);
                    }
                    seen += 1;
                }
            }
        }
        // Second pass for transparent/unnamed wrappers
        for i in 0..len {
            unsafe {
                let node = &mut *nodes_ptr.add(i);
                if node.is_hierarchy_transparent || node.name.is_empty() {
                    if let Some(found) = dfs(&mut node.children, remaining) {
                        return Some(found);
                    }
                }
            }
        }
        None
    }
    dfs(nodes, path_parts)
}

/// Write a value the user edited, and record that it is now the user's rather than the
/// template's.
///
/// A field hydrated from a template carries `_template_*` markers, and the serializer skips
/// those children unless they are marked as an explicit override - otherwise every instance
/// would restate everything it inherited. Writing only `value` therefore produces a document
/// that looks right in memory and loses the edit the moment it is serialized.
fn write_edited_value(node: &mut OverseerNode, value: OverseerValue) {
    node.parameters.insert("value".to_string(), value);
    // The serializer replays a node verbatim from its source snapshot while the fingerprint
    // it was parsed with still matches, which is what preserves comments and spacing. That
    // fingerprint is stored on the node and says nothing about the value now held in memory,
    // so an edited node would be written back out as the text it was read from. Clearing it
    // is how the rest of the codebase marks a node as no longer matching its source.
    node.source_fingerprint = None;
    node.parameters.insert(
        "_explicit_child_override".to_string(),
        OverseerValue::Boolean(true),
    );
    node.parameters
        .insert("_override_present".to_string(), OverseerValue::Boolean(true));
}

pub(crate) fn set_field_value_by_path(
    nodes: &mut [OverseerNode],
    path: &str,
    value: OverseerValue,
) -> std::result::Result<(), OverseerError> {
    let path_parts: Vec<&str> = path.split('/').collect();
    if path_parts.is_empty() {
        return Err(OverseerError::ValidationError("Empty path".to_string()));
    }
    // Try strict first
    if let Some(node) = find_node_by_path_mut(nodes, &path_parts) {
        write_edited_value(node, value);
        return Ok(());
    }
    // Fallback: transparency-aware
    if let Some(node) = find_node_by_path_mut_transparent(nodes, &path_parts) {
        write_edited_value(node, value);
        return Ok(());
    }
    Err(OverseerError::ValidationError(format!(
        "Could not find field at path: {}",
        path
    )))
}

// Helper function to find a node by path (immutable)
pub(crate) fn find_node_by_path<'a>(
    nodes: &'a [OverseerNode],
    path_parts: &[&str],
) -> Option<&'a OverseerNode> {
    if path_parts.is_empty() {
        return None;
    }

    let mut current_nodes = nodes;
    let mut current_node: Option<&OverseerNode> = None;

    for (i, &part) in path_parts.iter().enumerate() {
        // Handle array-style names with ordinals (e.g., "item#1")
        let (base_name, ordinal) = if part.contains('#') {
            let parts: Vec<&str> = part.split('#').collect();
            (
                parts[0],
                parts.get(1).unwrap_or(&"0").parse::<usize>().unwrap_or(0),
            )
        } else {
            (part, 0)
        };

        let matches: Vec<&OverseerNode> = current_nodes
            .iter()
            .filter(|node| node.name == base_name)
            .collect();

        if ordinal >= matches.len() {
            return None;
        }

        current_node = Some(matches[ordinal]);

        // If not the last part, move to children
        if i < path_parts.len() - 1 {
            current_nodes = &current_node?.children;
        }
    }

    current_node
}

// Helper function to find a node by path (mutable)
pub(crate) fn find_node_by_path_mut<'a>(
    nodes: &'a mut [OverseerNode],
    path_parts: &[&str],
) -> Option<&'a mut OverseerNode> {
    if path_parts.is_empty() {
        return None;
    }

    let mut current_nodes = nodes;

    for (i, &part) in path_parts.iter().enumerate() {
        // Handle array-style names with ordinals (e.g., "item#1")
        let (base_name, ordinal) = if part.contains('#') {
            let parts: Vec<&str> = part.split('#').collect();
            (
                parts[0],
                parts.get(1).unwrap_or(&"0").parse::<usize>().unwrap_or(0),
            )
        } else {
            (part, 0)
        };

        // Find all nodes with the base name
        let mut matches_indices = Vec::new();
        for (idx, node) in current_nodes.iter().enumerate() {
            if node.name == base_name {
                matches_indices.push(idx);
            }
        }

        if ordinal >= matches_indices.len() {
            return None;
        }

        let target_index = matches_indices[ordinal];

        // If this is the last part, return the node
        if i == path_parts.len() - 1 {
            return Some(&mut current_nodes[target_index]);
        }

        // Otherwise, move to children
        current_nodes = &mut current_nodes[target_index].children;
    }

    None
}

/// Open a document and record what everything was worked out from, whatever the setting says.
///
/// For tests that need a graph to exist regardless of how the build is configured. Ordinary
/// callers want `load_document`, which records too.
pub fn load_document_with_dependencies(content: String) -> Result<Vec<OverseerNode>> {
    load_document_maybe_recording(content, true)
}

/// Whether a resolve records what each value was worked out from.
///
/// On, now, everywhere. It used to be off unless asked for, and the desktop app asked while the
/// server did not - because recording cost about seventy per cent more on every open, which was a
/// good bargain for something that opens a document once and then edits it and a bad one for the
/// bot, which opens a document afresh on every request.
///
/// That arithmetic no longer holds. With the food tracker showing three days rather than
/// forty-three, recording costs a quarter of an open rather than seventy per cent - 1.31 seconds
/// against 1.05, measured either way round so neither gets the benefit of a warm mount cache - and
/// what it buys is a second open at 0.16 seconds instead of 1.05. The overhead is proportional to
/// the work and there is far less of it, while the saving is most of a resolve.
///
/// That is a good bargain for the bot too: it opens a document afresh on every request, and the
/// server is long-running, so the request that pays is the first one. And both builds doing the
/// same thing is one fewer way for what runs on the server to differ from what was tested on a
/// desktop.
///
/// `OVERSEER_DEPENDENCY_GRAPH=0` turns it off - for measuring against it, or if a graph is ever
/// suspected of holding a stale answer.
pub fn recording_is_on() -> bool {
    match std::env::var("OVERSEER_DEPENDENCY_GRAPH") {
        Ok(setting) => !matches!(
            setting.trim().to_ascii_lowercase().as_str(),
            "0" | "off" | "no" | "false"
        ),
        Err(_) => true,
    }
}

pub fn load_document(content: String) -> Result<Vec<OverseerNode>> {
    load_document_maybe_recording(content, recording_is_on())
}

/// The document exactly as it was last worked out, when working it out again could not change it.
///
/// Only when three things hold: this is the same text, a graph was recorded for it, and nothing in
/// it read the clock. The last is the one that matters - most of what these documents compute is a
/// function of their text, but a task's priority climbs by the day and its deadline passes, and
/// handing back yesterday's answer for those would be wrong in the way nobody notices.
fn already_worked_out(content: &str) -> Option<Vec<OverseerNode>> {
    let graph = graph_for(content)?;
    if graph.is_empty() {
        return None;
    }
    let mut previous = baseline_copy(content)?;
    if !graph.reads_the_clock() {
        return Some(previous);
    }

    // It does read the clock - but the graph says *where*, and everything else in the document is
    // a function of its text and has not moved. So only what descends from the clock is worked
    // out again: on a list of tasks that is the priorities and the deadlines, not the rules, not
    // the history, not the hundred fields that spell out what each one is.
    let stale = graph.nodes_to_work_out_again(&[crate::formula_evaluator::CLOCK.to_string()]);
    if stale.is_empty() {
        return Some(previous);
    }
    let targets: std::collections::HashSet<String> = stale.into_iter().collect();
    resolver::resolve_specific_fields(&mut previous, &targets);
    // Charts outright, for the reason given where an edit does the same: a series hangs on a plot
    // child while the reads are recorded against the chart.
    resolver::compute_chart_series(&mut previous);
    Some(previous)
}

fn load_document_maybe_recording(content: String, record: bool) -> Result<Vec<OverseerNode>> {
    // A document with a list showing only part of itself is the same text for everyone and not the
    // same document: a write names an address that has to stay in view, and a copy resolved for
    // somebody else will have left it out. So a resolve holding something in view neither takes
    // what is cached nor offers what it produces.
    //
    // This only became reachable when recording was turned on everywhere. Before that the server
    // kept no graph, `already_worked_out` never had one to answer from, and every write resolved
    // afresh by accident rather than on purpose.
    let ordinary = !resolver::is_keeping_anything_in_view();
    if ordinary {
        if let Some(unchanged) = already_worked_out(&content) {
            return Ok(unchanged);
        }
    }
    match parse_document(&content) {
        Ok((_remaining, mut nodes)) => {
            // Mounted content is not part of the host document's text, so it has to be
            // brought in after every parse - and before resolving, because until it is there
            // every formula that reads through the mount resolves to an error and then has to
            // be worked out a second time. This is the order the selective path has always
            // used; this one resolved first, then mounted, then resolved again, and on the food
            // tracker those extra passes were about half the cost of opening it.
            //
            // `preload` and `lazy` are read as written rather than as computed, which is what
            // lets this run before anything is resolved.
            ActionExecutor::preload_mounts(&mut nodes);
            // Recorded while resolving, so an edit afterwards can be told what it reaches rather
            // than having the whole document worked out again.
            //
            // Off unless asked for, because the trade is not the same for everyone. Measured on
            // the food tracker: opening it goes from 27 seconds to 124 while recording, and an
            // edit from 38 seconds to 3. That is a good bargain for something that opens a
            // document once and then edits it - the desktop app - and a bad one for the bot,
            // which opens the document afresh on every request and would pay the recording every
            // time to save an edit it often does not make.
            //
            // What makes it dear is a string built and kept for every read, and there are tens of
            // millions of them. Interning those is what would let this be the default.
            let recording = record && ordinary;
            if recording {
                crate::dependencies::start_recording();
            }
            resolver::resolve_document(&mut nodes);
            if recording {
                remember_graph(&content, crate::dependencies::take_recording());
            }
            // The caller holds this text and this document, so the next interaction
            // can be answered with a change rather than with the document.
            if ordinary {
                remember(&content, &nodes);
            }
            Ok(nodes)
        }
        Err(e) => Err(OverseerError::ParseError(format!("Parse error: {}", e))),
    }
}





/// Describing a change needs the state it changed from, and answering an edit from the graph needs
/// what the last resolve read. Rust owns the document, so it keeps both rather than having the
/// caller send them back - which is the whole point, since sending them back is what costs seconds.
///
/// Both live in `document_cache`, which holds several documents within a memory budget. It used to
/// be two single slots, and the server evicted each document by serving the next; see that module
/// for what that cost and why the budget is in bytes.
fn remember_graph(text: &str, graph: crate::dependencies::Graph) {
    crate::document_cache::put_graph(text, graph);
}

/// The graph for this text, if one is held for this text.
fn graph_for(text: &str) -> Option<crate::dependencies::Graph> {
    crate::document_cache::graph_for(text)
}

/// Drop every graph, so the next resolve works everything out. For tests that need the slow answer
/// to compare against, and for anything that wants to be sure it is not reading a stale one.
pub fn forget_dependencies() {
    crate::document_cache::forget_graphs();
}

/// The document as it last stood for this text, without taking it.
///
/// `take_baseline` hands it over for the delta to consume; this only wants to read what was
/// worked out last time.
fn baseline_copy(content: &str) -> Option<Vec<OverseerNode>> {
    crate::document_cache::nodes_for(content)
}

/// Copy every worked-out value from one tree onto the other, matching node for node.
///
/// The two are the same document - one parsed afresh, one as it last stood - so they are walked
/// together rather than by path. A shape that does not match is reported, and the caller works
/// everything out instead of trusting a half-copied tree.
fn carry_over_computed(
    into: &mut [OverseerNode],
    from: &[OverseerNode],
    edited: &std::collections::HashSet<String>,
    trail: &mut Vec<String>,
) -> bool {
    carry_over_computed_at(into, from, edited, trail, &mut crate::addressing::Level::default())
}

/// The same, a level at a time, naming each node as the edited paths do - a wrapper is no step,
/// see `addressing::Level` - or an edit inside one would not be recognised and its old answer
/// copied back over it.
fn carry_over_computed_at(
    into: &mut [OverseerNode],
    from: &[OverseerNode],
    edited: &std::collections::HashSet<String>,
    trail: &mut Vec<String>,
    level: &mut crate::addressing::Level,
) -> bool {
    if into.len() != from.len() {
        return false;
    }
    for (fresh, previous) in into.iter_mut().zip(from.iter()) {
        if fresh.name != previous.name {
            return false;
        }
        if crate::addressing::is_wrapper(fresh) {
            // Its own worked-out values too - a `hidden` on a row - which no edit names.
            for (key, value) in &previous.parameters {
                if key.starts_with("_computed_") {
                    fresh.parameters.insert(key.clone(), value.clone());
                }
            }
            if !carry_over_computed_at(&mut fresh.children, &previous.children, edited, trail, level) {
                return false;
            }
            continue;
        }
        trail.push(level.segment(&fresh.name));
        // Not onto what the edit just changed. A field holding a plain value has no formula to
        // work it out again, so copying the old answer over the new one would put the edit back -
        // and everything reading it would agree, convincingly and wrongly.
        if !edited.contains(&trail.join("/")) {
            for (key, value) in &previous.parameters {
                if key.starts_with("_computed_") {
                    fresh.parameters.insert(key.clone(), value.clone());
                }
            }
        }
        let ok = carry_over_computed(&mut fresh.children, &previous.children, edited, trail);
        trail.pop();
        if !ok {
            return false;
        }
    }
    true
}

fn remember(text: &str, nodes: &[OverseerNode]) {
    crate::document_cache::put_nodes(text, nodes);
}

/// Drop every remembered document.
///
/// Nothing depends on this for correctness - a baseline is only used when its text matches
/// what the caller sent, so a stale one is ignored rather than misapplied. It exists so a
/// closed document does not sit in memory, and so tests can start from a known state.
pub fn forget_baseline() {
    crate::document_cache::forget_nodes();
}

/// A resolved document, described as a change where that was possible.
#[derive(serde::Serialize)]
pub struct ResolvedUpdate {
    /// The text of the new document, for the caller to send with the next interaction.
    pub text: String,
    /// What changed, when the previous document was known.
    pub changes: Option<Vec<crate::delta::DocumentChange>>,
    /// The whole document, when it was not and there is nothing to describe a change against.
    pub nodes: Option<Vec<OverseerNode>>,
    /// Addresses the press wrote that belong to whoever is looking rather than to the document.
    ///
    /// The text above has them applied, because the caller's view is that text and the day it is
    /// showing has to move. Anything about to write that text to a file has to take them out
    /// again, and this says which.
    #[serde(default)]
    pub view_state: Vec<String>,
    /// Whether this changed the file, and what the file says now if that differs from `text`.
    ///
    /// The caller keeps the text the file held when it last heard, so that a save built on a
    /// document something else has written since can be refused rather than overwrite it. A
    /// write made here moves the file without the caller sending anything, so without being
    /// told, that baseline is stale from the first change onwards and every later save is
    /// refused - which is what happened: a third edit in a row, or a first tag, and the page
    /// said the document had changed since it was opened.
    ///
    /// `wrote` false means the file was left alone - a press that moved only the viewer - and
    /// the caller's baseline still stands. `file_text` is sent only when the file differs from
    /// what the caller is being shown, which is when a guarded field is in play; otherwise
    /// `text` is what the file says too.
    #[serde(default)]
    pub wrote: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_text: Option<String>,
    /// Textboxes a press copied from, named as the page named them, for the page to empty.
    ///
    /// What is typed into one lives in the page and reaches the backend only with a press - see
    /// `actions::hold_typed_text` - so nothing here can empty it. Saying which were used is the
    /// most it can do.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emptied: Vec<Vec<String>>,
}

/// The document as its own text reads, rather than as it happens to sit in memory.
///
/// List entries are named by position - `Task__5` - and that name is given when a document is
/// parsed, to a `- { ... }` item that carries no name of its own. Remove an entry in memory and
/// the survivors keep the names they were given: the list runs `Task__4, Task__6, Task__7`,
/// with a hole where the removed one was. Serialize and parse that same document and the names
/// come out contiguous again, because the items are numbered as they are instantiated.
///
/// Both are self-consistent; they simply disagree. That mattered because the caller addresses
/// events by name and the server answers them by parsing text. After one task was closed, the
/// page went on offering the names it had been handed - and the next press named an entry that
/// the text resolves to a different task. Closing two tasks in a row closed the wrong second
/// one, reliably, and only a save and reload put the two back in step.
///
/// So the text is made the authority: what the caller is given is what its text produces.
fn as_its_text_reads(nodes: &[OverseerNode]) -> Result<(String, Vec<OverseerNode>)> {
    let text = OverseerFileHandler::serialize_nodes(nodes).map_err(|e| {
        OverseerError::SerializationError(format!("Failed to serialize resolved document: {}", e))
    })?;
    let reparsed = load_document(text.clone())?;
    Ok((text, reparsed))
}

/// Serialize a resolved document and describe it as a change where there was a baseline.
///
/// `was` is the text the interaction started from. What is held for it - the graph in particular -
/// still describes this document, and is moved onto the new text rather than thrown away. It has
/// to be named because the cache holds several documents now, and only this one has moved.
fn finish_update(
    was: &str,
    resolved: Vec<OverseerNode>,
    baseline: Option<Vec<OverseerNode>>,
) -> Result<ResolvedUpdate> {
    let text = OverseerFileHandler::serialize_nodes(&resolved).map_err(|e| {
        OverseerError::SerializationError(format!("Failed to serialize resolved document: {}", e))
    })?;
    finish_update_with(was, text, resolved, baseline)
}

/// The same, when the text has already been worked out.
fn finish_update_with(
    was: &str,
    text: String,
    resolved: Vec<OverseerNode>,
    baseline: Option<Vec<OverseerNode>>,
) -> Result<ResolvedUpdate> {
    match baseline.map(|before| crate::delta::diff(&before, &resolved)) {
        // The caller gets the changes, so the document itself can be handed to the cache
        // rather than copied into it.
        Some(changes) => {
            crate::document_cache::rekey(was, &text);
            crate::document_cache::own_nodes(&text, resolved);
            Ok(ResolvedUpdate {
                text,
                changes: Some(changes),
                nodes: None,
                view_state: Vec::new(),
                wrote: false,
                file_text: None,
                emptied: Vec::new(),
            })
        }
        None => {
            crate::document_cache::rekey(was, &text);
            remember(&text, &resolved);
            Ok(ResolvedUpdate {
                text,
                changes: None,
                nodes: Some(resolved),
                view_state: Vec::new(),
                wrote: false,
                file_text: None,
                emptied: Vec::new(),
            })
        }
    }
}

/// Resolve an edit and answer with what changed rather than with the document.
pub fn resolve_selective_update(
    content: String,
    changed_fields: Vec<String>,
    changed_field_values: Option<std::collections::HashMap<String, OverseerValue>>,
) -> Result<ResolvedUpdate> {
    // Copied rather than taken, for the same reason the event path copies: `resolve_selective`
    // asks for this very baseline a moment later to decide what the edit can reach, and taking
    // it meant that question always answered no - so every edit fell back to working out every
    // value in the document, which is the thing the dependency graph exists to avoid.
    let baseline = baseline_copy(&content);
    let resolved = resolve_selective(content.clone(), changed_fields, changed_field_values)?;
    finish_update(&content, resolved, baseline)
}

/// Run an event and answer with what changed rather than with the document.
pub fn execute_event_update(
    content: String,
    node_path: Vec<String>,
    event_name: String,
) -> Result<ResolvedUpdate> {
    // Copied, not taken. Taking it emptied the cache entry for this exact text, and the very
    // next line asked for that document back - so every press paid a full parse and resolve to
    // rebuild what had just been thrown away, to save one clone. On the food tracker that was
    // 1.6 seconds of a 2.2 second press.
    let baseline = baseline_copy(&content);
    let was = content.clone();
    // The post-event document is stored below, so what is remembered is what the caller holds.
    let phase = std::time::Instant::now();
    let mut nodes = load_document(content)?;
    if resolver::profile_enabled() {
        eprintln!("[PHASE] press load {:.1} ms", phase.elapsed().as_secs_f64() * 1000.0);
    }

    crate::actions::start_reporting_and_settling();
    let phase = std::time::Instant::now();
    let outcome = ActionExecutor::execute_event(&mut nodes, &node_path, &event_name);
    if resolver::profile_enabled() {
        eprintln!("[PHASE] press act {:.1} ms", phase.elapsed().as_secs_f64() * 1000.0);
    }
    let report = crate::actions::take_report();
    outcome?;

    // What the document said belongs to the viewer is applied here rather than left out.
    //
    // This caller is the page, and the page's view *is* the document it holds: the day it is
    // showing has to move for the press to have done anything. It holds the result as text,
    // sends that text back to be saved, and the save restores the authored values - which is
    // how `guarded` has always worked on this path. The server's own path does the opposite
    // and keeps these against a session, because there the write is the file.

    // An event that only wrote values can be finished the way an edit is: ask the graph what
    // reads them, work those out, and serialize once. Anything that moved the document's shape
    // still takes the long way - serialize, parse again, resolve the lot - because an append or
    // a remove renames everything after it and the graph is describing the document as it was.
    //
    // Without a graph for this text, or with nothing reported, the long way is also what
    // happens, which is what happened before this existed and is never wrong.
    let viewers: Vec<String> = report
        .as_ref()
        .map(|changed| changed.view_state.iter().map(|(a, _)| a.clone()).collect())
        .unwrap_or_default();

    let quick = report
        .filter(|changed| !changed.structural && !changed.fields.is_empty())
        .and_then(|changed| graph_for(&was).map(|graph| (changed, graph)))
        .and_then(|(changed, graph)| {
            let to_redo = graph.nodes_to_work_out_again(&changed.fields);
            if to_redo.is_empty() {
                return None;
            }
            if resolver::profile_enabled() {
                eprintln!("[PHASE] press changed {:?} and reaches {} values",
                          changed.fields, to_redo.len());
            }
            // The graph is left exactly as it was, rather than absorbing what this resolve
            // recorded. `absorb` replaces a value's sources with the newer ones, and a
            // selective resolve only re-reads what it recomputed - so a value that was reached
            // but settled without reading again came back with fewer sources than it has, and
            // the graph shrank a little on every press. Two presses in, the cascade stopped
            // reaching `quadrupled` and it held the previous press's answer.
            //
            // Nothing here changes what reads what: an action wrote a value, not a formula.
            // A press that redirects a lookup is the case this does not cover, and it is the
            // same case the edit path does not cover either; `topo` is where that gets settled.
            let phase = std::time::Instant::now();
            resolver::resolve_specific_fields(&mut nodes, &to_redo.into_iter().collect());
            if resolver::profile_enabled() {
                eprintln!("[PHASE] press resolve {:.1} ms", phase.elapsed().as_secs_f64() * 1000.0);
            }
            // Charts outright rather than by the cascade, for the reason the edit path gives:
            // a series hangs on a plot child while the reads are recorded against the chart.
            resolver::compute_chart_series(&mut nodes);
            let phase = std::time::Instant::now();
            let text = OverseerFileHandler::serialize_nodes(&nodes).ok()?;
            if resolver::profile_enabled() {
                eprintln!("[PHASE] press serialize {:.1} ms", phase.elapsed().as_secs_f64() * 1000.0);
            }
            // Under the text this started from, not the one it produced: `finish_update_with`
            // moves the whole entry onto the new text, and moving it discards whatever was
            // already held there - so a graph stored under the new text is deleted a moment
            // later, and the press after this one pays a full resolve. The edit path has
            // always done it this way round.
            remember_graph(&was, graph);
            Some(text)
        });

    if resolver::profile_enabled() {
        eprintln!("[PHASE] press took the {} way", if quick.is_some() { "short" } else { "long" });
    }
    let (text, nodes) = match quick {
        Some(text) => (text, nodes),
        None => as_its_text_reads(&nodes)?,
    };
    let phase = std::time::Instant::now();
    let done = finish_update_with(&was, text, nodes, baseline).map(|mut update| {
        update.view_state = viewers;
        update
    });
    if resolver::profile_enabled() {
        eprintln!("[PHASE] press finish {:.1} ms", phase.elapsed().as_secs_f64() * 1000.0);
    }
    done
}

/// A guarded field and the value the document authored for it.
///
/// `value` absent means the override existed only in this session and should not be written
/// at all.
#[derive(serde::Deserialize)]
pub struct GuardedRevert {
    pub path: String,
    #[serde(default)]
    pub value: Option<OverseerValue>,
}

/// What a write should leave in the file, once the viewer's own values are taken back out.
pub struct WhatToWrite {
    /// The text for the file: the viewer's values removed, what the document authored restored.
    pub text: String,
    /// False when nothing but the viewer moved. The file is already right, and writing it would
    /// make an undo step out of nothing and a commit out of somebody looking.
    pub worth_writing: bool,
    /// What belonged to the viewer, for the caller to keep against their session.
    pub viewers: Vec<(String, OverseerValue)>,
}

/// Take the report an action left, and settle the viewer's values out of the text to be written.
///
/// An action writes a field the document marks `mutable="guarded"` like any other, because
/// whoever asked for it is looking at the result and the day has to move. It is the *file* that
/// must not have it. So the report says which addresses were the viewer's, those are put back to
/// what the document authored - read by parsing the text as it stood, since the authored value is
/// wanted rather than the worked-out one - and the caller is told whether anything is left worth
/// writing at all.
///
/// Shared because both hosts have to agree about it. The server had this inside its own edit
/// path, and a second copy written for the desktop would be a second set of rules about what
/// reaches a file.
pub fn settle_the_viewers_values(
    serialized: String,
    as_it_stood: &str,
    already_the_viewers: &[String],
    report: Option<&crate::actions::Changed>,
) -> WhatToWrite {
    let reported: Vec<(String, OverseerValue)> =
        report.map(|r| r.view_state.clone()).unwrap_or_default();

    // A press that moved nothing but the viewer leaves the file alone entirely: no write, no
    // undo point, nothing for the backup to commit.
    let only_the_viewers = !reported.is_empty()
        && report.map_or(false, |r| r.fields.is_empty() && !r.structural);

    // Everything this viewer holds, not only what this action happened to move.
    //
    // That distinction was a data loss. The document is worked out with the viewer's values
    // applied, because that is the day they are looking at - so those values are in the text
    // whatever the action was. Stripping only the addresses the action reported meant the next
    // unrelated write carried the rest into the file: navigate two days back, then record
    // anything at all, and `showing (mutable="guarded") = $(today())` was replaced in the
    // document by the literal day one person happened to be looking at. The formula was gone,
    // and with it every other viewer's idea of today.
    let mut named: Vec<String> = already_the_viewers.to_vec();
    for (address, _) in &reported {
        if !named.contains(address) {
            named.push(address.clone());
        }
    }

    let text = if only_the_viewers || named.is_empty() {
        serialized
    } else {
        without_the_viewers_values(&serialized, as_it_stood, &named).unwrap_or(serialized)
    };
    WhatToWrite { text, worth_writing: !only_the_viewers, viewers: reported }
}

/// Make one change to a document on disk, and answer with what the caller should now show.
///
/// The page used to send the whole document back with every change and have that text written
/// over the file. Whatever else had written in between - the bot recording a meal, another tab -
/// was gone, and nothing said so. This reads what the file says now, applies the one change to
/// that, and writes it: the way `/v1` has always worked, and the reason the bot cannot lose a
/// change it did not make.
///
/// It also means the document never travels upward. Sending one back measured about 5.5 seconds
/// on a large one, against 250 KB of text coming the other way, and that was on every change.
fn change_document(
    path: &str,
    document: &str,
    session: &str,
    work: impl FnOnce(&mut Vec<OverseerNode>) -> Result<()>,
) -> Result<ResolvedUpdate> {
    let as_it_stood = std::fs::read_to_string(path)
        .map_err(|e| OverseerError::IoError(format!("could not read the document: {}", e)))?;
    let baseline = baseline_copy(&as_it_stood);
    // Worked out as this viewer sees it, or a second press on a day button would start from what
    // the file says again and never get past the first step back.
    let looking_at = crate::viewstate::overlay(session, document);
    let held_for_the_viewer: Vec<String> = looking_at.keys().cloned().collect();
    let mut nodes = if !looking_at.is_empty() {
        resolve_selective(as_it_stood.clone(), held_for_the_viewer.clone(), Some(looking_at))?
    } else if let Some(already) = baseline.clone() {
        // The document as it was last worked out, for exactly this text. Parsing and resolving
        // it again produces the same thing and costs what opening it cost - which on tasks.os
        // was most of the second every edit took. The cache is keyed by the text, so a hit means
        // the file has not moved since, and the graph for it is held under the same key.
        already
    } else {
        load_document(as_it_stood.clone())?
    };

    crate::actions::start_reporting_and_settling();
    work(&mut nodes)?;
    let report = crate::actions::take_report();

    // What was written changes what derives from it, and the caller is about to be shown it -
    // but working the whole document out again to find out costs as much as opening it did. The
    // graph already knows what reads what, so it is asked instead, exactly as the press path
    // asks it. On tasks.os that was most of a second per edit.
    //
    // The long way when the graph cannot say: a change of shape renames everything after it, so
    // the graph is describing a document that no longer exists. Also when there is no graph for
    // this text, which is what happens when the document was worked out for a viewer rather
    // than plainly. Never wrong, only slower.
    let settled_what_it_reached = report
        .as_ref()
        .filter(|changed| !changed.structural && !changed.fields.is_empty())
        .and_then(|changed| graph_for(&as_it_stood).map(|graph| (changed, graph)))
        .and_then(|(changed, graph)| {
            let to_redo = graph.nodes_to_work_out_again(&changed.fields);
            if to_redo.is_empty() {
                return None;
            }
            crate::resolver::resolve_specific_fields(&mut nodes, &to_redo.into_iter().collect());
            // A series hangs on a plot child while the reads are recorded against the chart, so
            // the cascade does not reach it. The other two paths say the same.
            crate::resolver::compute_chart_series(&mut nodes);
            // Left as it was rather than absorbing what this resolve recorded: a selective
            // resolve only re-reads what it recomputed, so absorbing shrinks the graph a little
            // every time. See the press path, where that cost a value its cascade.
            remember_graph(&as_it_stood, graph);
            Some(())
        });
    if settled_what_it_reached.is_none() {
        crate::resolver::resolve_document(&mut nodes);
    }

    let serialized = OverseerFileHandler::serialize_nodes(&nodes)
        .map_err(|e| OverseerError::SerializationError(format!("could not serialize: {}", e)))?;

    let settled = settle_the_viewers_values(
        serialized.clone(),
        &as_it_stood,
        &held_for_the_viewer,
        report.as_ref(),
    );
    for (address, value) in settled.viewers {
        crate::viewstate::set(session, document, &address, value);
    }
    let wrote = settled.worth_writing && settled.text != as_it_stood;
    if wrote {
        write_file_keeping_a_step_back(path, &settled.text)
            .map_err(|e| OverseerError::IoError(format!("could not write the document: {}", e)))?;
    }

    // The caller is shown what it asked for, viewer's values and all. Only the file goes
    // without - and it is told so, because it holds the baseline a later save is checked
    // against and has no other way to learn that this write moved the file.
    let mut update = finish_update_with(&as_it_stood, serialized, nodes, baseline)?;
    update.wrote = wrote;
    if wrote && settled.text != update.text {
        update.file_text = Some(settled.text);
    }
    Ok(update)
}

/// Work a document out as one viewer sees it.
///
/// A guarded field belongs to whoever is looking, so what it holds for them is kept here rather
/// than in the file. Resolving without it hands back what the document authored - which is
/// correct for the file and wrong for the person: opening a page put them back to today while
/// the day they had moved to was still being remembered, so the next press stepped back from
/// *that* and the display jumped. What they are looking at has to be applied on the way in as
/// well as on the way out.
pub fn load_document_for(content: String, document: &str, session: &str) -> Result<Vec<OverseerNode>> {
    let looking_at = crate::viewstate::overlay(session, document);
    if looking_at.is_empty() {
        return load_document(content);
    }
    let addresses = looking_at.keys().cloned().collect();
    resolve_selective(content, addresses, Some(looking_at))
}

/// The override nodes for an entry, from a map of field name to value.
///
/// A field name may be a path - `macros/protein` - so a value can be given for something the
/// template keeps one level in, and the nodes for the levels between are made as needed. Shared
/// rather than kept with the bot's door, because the page appends entries the same way now.
pub fn entry_overrides(
    fields: &std::collections::HashMap<String, OverseerValue>,
) -> Vec<OverseerNode> {
    let mut roots: Vec<OverseerNode> = Vec::new();
    for (field, value) in fields {
        let mut segments = field.split('/').filter(|s| !s.is_empty()).peekable();
        let mut level = &mut roots;
        while let Some(segment) = segments.next() {
            let leaf = segments.peek().is_none();
            let existing = level.iter().position(|n| n.name == segment);
            let index = match existing {
                Some(i) => i,
                None => {
                    let mut node = if leaf {
                        let mut n =
                            OverseerNode::new_with_type("-".to_string(), Some(segment.to_string()));
                        n.authored_dash = true;
                        n
                    } else {
                        OverseerNode::new_with_type("div".to_string(), Some(segment.to_string()))
                    };
                    node.name = segment.to_string();
                    level.push(node);
                    level.len() - 1
                }
            };
            if leaf {
                level[index]
                    .parameters
                    .insert("value".to_string(), value.clone());
            }
            level = &mut level[index].children;
        }
    }
    roots
}

/// Add an entry to a list, named by the path of node names the page already speaks in.
///
/// The last thing the page did by sending the whole document and having it written over the
/// file. A value could be named and written; a change of shape could not, so adding an entry
/// meant building it in the page and letting the save carry it - which is the pattern that lost
/// somebody else's writes, and the reason `instructions` was filed.
///
/// What comes back is the whole document rather than a list of changes, and that is not a
/// shortcoming: an entry added or taken out renames every entry after it, so the addresses the
/// page holds no longer mean what they did. Saying so plainly is cheaper than a page acting on
/// names that have moved under it.
pub fn append_entry_at(
    path: &str,
    document: &str,
    list_path: Vec<String>,
    fields: std::collections::HashMap<String, OverseerValue>,
    session: &str,
) -> Result<ResolvedUpdate> {
    let overrides = entry_overrides(&fields);
    change_document(path, document, session, move |nodes| {
        crate::actions::ActionExecutor::append_entry(
            nodes,
            &format!("/{}", list_path.join("/")),
            &overrides,
        )
    })
}

/// What a view onto a key the list lacks asks for when one of its fields is edited.
///
/// The key it is pointed at, the template its preview was drawn from, and where the entry should
/// go if it has to be made. `fields` is the edit that caused all this, named relative to the
/// entry - `weight`, or `targets/target_calories` - and applied whether the entry was just made
/// or was already there. Both together, because a person edited one field once: one file write,
/// and one step to take back.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EntryWanted {
    #[serde(alias = "listPath")]
    pub list_path: Vec<String>,
    #[serde(alias = "keyField", default)]
    pub key_field: String,
    #[serde(alias = "keyValue")]
    pub key_value: OverseerValue,
    pub template: String,
    /// `first`, `prepend`, `prepend-on-edit` for the front; anything or nothing for the back.
    #[serde(default)]
    pub position: Option<String>,
    #[serde(default)]
    pub fields: std::collections::HashMap<String, OverseerValue>,
    /// A press to run inside the entry once it is there - see `ensure_entry_at`.
    #[serde(default)]
    pub then: Option<PressWithin>,
}

/// A press on something inside an entry, named from the entry down, because the entry's own name
/// is not known until it has been made.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PressWithin {
    pub within: Vec<String>,
    pub event: String,
    /// What the page's textboxes hold, as a press carries it - see `run_event_at`.
    #[serde(default)]
    pub typed: Vec<ValueWrite>,
}

/// Make sure the list has an entry with this key, then apply the edit that asked for it.
///
/// This is the whole of what a preview becoming real means, said in one instruction against
/// whatever the file holds at that moment - which is the point. The page used to do the making
/// itself, in its own copy of the document, and leave the whole text to be saved over the file;
/// a write that landed in between was gone without a word.
///
/// Doing nothing when the entry is already there is not a special case, it is the same sentence:
/// the edit lands on the entry either way, so a view does not have to know whether what it is
/// showing was a preview a moment ago.
///
/// A press is the same sentence with a different ending. Pressing a button on a day the history
/// has not got used to be two instructions - make the day, then press - and so two file writes
/// and two presses of Undo for one thing done once. `then` says the press, and it runs here, on
/// the entry just made, in the same change.
pub fn ensure_entry_at(
    path: &str,
    document: &str,
    wanted: EntryWanted,
    session: &str,
) -> Result<ResolvedUpdate> {
    let emptied = std::cell::RefCell::new(Vec::new());
    let mut update = change_document(path, document, session, |nodes| {
        let EntryWanted { list_path, key_field, key_value, template, position, fields, then } = wanted;
        let list = format!("/{}", list_path.join("/"));
        let goes = crate::actions::WhereItGoes::from_said(position.as_deref());
        let entry = crate::actions::ActionExecutor::ensure_entry(
            nodes,
            &list,
            &template,
            &key_field,
            key_value,
            goes,
        )?;
        // Named through the entry the list answered with, rather than through the key: the key
        // selects an entry, and the path a write takes is a path of names.
        for (field, value) in fields {
            let target = format!("{}/{}/{}", list, entry, field);
            crate::actions::ActionExecutor::assign_value(nodes, &target, value)?;
        }
        if let Some(press) = then {
            // An entry just made holds only what it was given until the document is worked out:
            // the template supplies the rest, the button included. So it is worked out before the
            // press looks for it - what an event does between an action that changes the shape
            // and the next one, for the same reason.
            crate::resolver::resolve_document(nodes);
            let mut owner = list_path.clone();
            owner.push(entry.clone());
            owner.extend(press.within);
            let typed: Vec<(Vec<String>, OverseerValue)> =
                press.typed.into_iter().map(|t| (t.node_path, t.value)).collect();
            let held = crate::actions::hold_typed_text(nodes, &typed);
            let ran = crate::actions::ActionExecutor::execute_event(nodes, &owner, &press.event);
            crate::actions::let_go_of_typed_text(nodes, held);
            *emptied.borrow_mut() = crate::actions::take_emptied();
            ran?;
        }
        Ok(())
    })?;
    update.emptied = emptied.into_inner();
    Ok(update)
}

/// And take one out again.
pub fn remove_entry_at(
    path: &str,
    document: &str,
    entry_path: Vec<String>,
    session: &str,
) -> Result<ResolvedUpdate> {
    change_document(path, document, session, move |nodes| {
        crate::actions::ActionExecutor::remove_entry(nodes, &format!("/{}", entry_path.join("/")))
    })
}

/// Set one value, named by the path of node names the page already speaks in.
pub fn write_value_at(
    path: &str,
    document: &str,
    node_path: Vec<String>,
    value: OverseerValue,
    session: &str,
) -> Result<ResolvedUpdate> {
    write_values_at(path, document, vec![ValueWrite { node_path, value }], session)
}

/// One value to write, named by the path of node names the page already speaks in.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ValueWrite {
    #[serde(alias = "nodePath")]
    pub node_path: Vec<String>,
    pub value: OverseerValue,
}

/// Set several values as one change.
///
/// Not the same as writing each in turn. A change is read, applied and written in one pass, so
/// several values arriving together make one file write and one step to take back - where three
/// writes in a row would make three of each, and the person who typed once would press Undo
/// three times to get back to where they started.
///
/// Applied in the order given, against the document as the file says it, which is what keeps a
/// change made elsewhere in the meantime from being thrown away. If one of them cannot be
/// applied the whole change is refused rather than half written: the page is told, and the file
/// still says what it did.
pub fn write_values_at(
    path: &str,
    document: &str,
    writes: Vec<ValueWrite>,
    session: &str,
) -> Result<ResolvedUpdate> {
    change_document(path, document, session, move |nodes| {
        for write in writes {
            crate::actions::ActionExecutor::assign_value(
                nodes,
                &format!("/{}", write.node_path.join("/")),
                write.value,
            )?;
        }
        Ok(())
    })
}

/// Run one handler, named the same way.
///
/// `typed` is what the page's textboxes hold as the press is made. It is put on them for the
/// length of the press and taken off again before anything is worked out or written - see
/// `actions::hold_typed_text` - and the answer names the ones a copy used, for the page to empty.
pub fn run_event_at(
    path: &str,
    document: &str,
    node_path: Vec<String>,
    event_name: String,
    session: &str,
    typed: Vec<ValueWrite>,
) -> Result<ResolvedUpdate> {
    let emptied = std::cell::RefCell::new(Vec::new());
    let mut update = change_document(path, document, session, |nodes| {
        let typed: Vec<(Vec<String>, OverseerValue)> =
            typed.into_iter().map(|t| (t.node_path, t.value)).collect();
        let held = crate::actions::hold_typed_text(nodes, &typed);
        let ran = crate::actions::ActionExecutor::execute_event(nodes, &node_path, &event_name);
        crate::actions::let_go_of_typed_text(nodes, held);
        *emptied.borrow_mut() = crate::actions::take_emptied();
        ran.map(|_| ())
    })?;
    update.emptied = emptied.into_inner();
    Ok(update)
}

/// Whether the file still says what the caller was working from.
///
/// A page holds the document as text and sends that text back to be written. If the file has
/// moved on since it was loaded - the bot logged a meal, another tab saved - then the text in
/// hand was built without that write, and writing it discards the other one silently. Refusing
/// is the only answer that cannot lose somebody's work.
///
/// Compared after canonicalizing, so a difference that is only formatting is not a conflict.
/// `None` for `was` means the caller did not say what it started from, and nothing is checked -
/// which is what a document being saved for the first time looks like.
pub fn still_says_what_it_did(on_disk: &str, was: Option<&str>) -> bool {
    match was {
        None => true,
        Some(was) => canonicalize_document(on_disk) == canonicalize_document(was),
    }
}

/// Write a document, keeping what it said so the write can be taken back.
///
/// The server has its own version of this, because it has a root directory and a document name.
/// The desktop app has a path and nothing else, so the undo history sits beside the document -
/// which is where the server puts it too.
pub fn write_file_keeping_a_step_back(path: &str, text: &str) -> std::io::Result<()> {
    let at = std::path::Path::new(path);
    if let (Some(beside), Some(named)) = (at.parent(), at.file_name()) {
        if let Ok(previous) = std::fs::read_to_string(at) {
            // A write that says what the file already says is not a change, and recording a step
            // for it makes an undo that does nothing. That was not rare once every change began
            // saving itself: a button that moves a `guarded` field does something visible and
            // correctly leaves the file alone, so every press left an empty step behind and the
            // count stopped meaning how many undos would move anything.
            if previous == text {
                return Ok(());
            }
            crate::undo::remember(beside, &named.to_string_lossy(), &previous);
        }
    }
    let temporary = at.with_extension("os.writing");
    std::fs::write(&temporary, text.as_bytes())?;
    std::fs::rename(&temporary, at).inspect_err(|_| {
        let _ = std::fs::remove_file(&temporary);
    })
}

/// Put a document back the way it was before the last write. Answers with the steps remaining.
///
/// The undo write records no step of its own, so undoing twice walks back two writes rather than
/// swapping between the same two states.
pub fn undo_document(path: &str) -> Result<usize> {
    let at = std::path::Path::new(path);
    let (Some(beside), Some(named)) = (at.parent(), at.file_name()) else {
        return Err(OverseerError::ValidationError(format!("'{}' has no directory", path)));
    };
    let named = named.to_string_lossy().to_string();
    let Some(previous) = crate::undo::take(beside, &named) else {
        return Err(OverseerError::ValidationError(format!(
            "there is nothing to take back for '{}'",
            named
        )));
    };
    let temporary = at.with_extension("os.writing");
    std::fs::write(&temporary, previous.as_bytes())
        .and_then(|_| std::fs::rename(&temporary, at))
        .map_err(|e| OverseerError::IoError(format!("could not write '{}': {}", named, e)))?;
    Ok(crate::undo::depth(beside, &named))
}

/// What a document's text says at one address, without working the whole thing out.
///
/// For reading back what a press put in a viewer's field: the value is already in the text the
/// press produced, and parsing is enough to find it. Resolving would be the wrong answer as well
/// as the slower one - what is wanted is the value that was written, not what it computes to.
pub fn value_at(text: &str, address: &str) -> Option<OverseerValue> {
    let (_rest, nodes) = parse_document(text).ok()?;
    get_field_value_by_path(&nodes, address)
}

/// The document to write, with the viewer's fields put back to what the document authored.
///
/// For a press that moved both something real and something that is only the viewer's: the real
/// change has to be written and the viewer's must not be. What was authored for those fields is
/// in the text as it stood, so it is read from there - parsed and not resolved, because what is
/// wanted is the formula the document holds rather than the number it came to.
///
/// `None` when the text cannot be parsed, which leaves the caller writing what it had.
pub fn without_the_viewers_values(
    serialized: &str,
    as_it_stood: &str,
    addresses: &[String],
) -> Option<String> {
    let (_rest, authored) = parse_document(as_it_stood).ok()?;
    let reverts: Vec<GuardedRevert> = addresses
        .iter()
        .map(|path| GuardedRevert {
            path: path.clone(),
            // Absent means the field had no authored value at all, and the override should
            // simply go - which is what `save_document_from_text` does with `None`.
            value: get_field_value_by_path(&authored, path),
        })
        .collect();
    save_document_from_text(serialized.to_string(), reverts).ok()
}

/// Serialize a document held as text, restoring guarded fields to what the document authored.
///
/// `mutable="guarded"` means a field can be changed in the open document and the change is
/// never written to disk - navigating days in the calorie tracker is the motivating case. The
/// text a caller holds comes from resolving *with* those changes applied, because that is how
/// the view updates, so the text used for resolving and the text to be saved legitimately
/// differ. The difference is a handful of fields, and they travel here rather than the caller
/// sending back the whole document to be serialized.
pub fn save_document_from_text(content: String, guarded: Vec<GuardedRevert>) -> Result<String> {
    match parse_document(&content) {
        Ok((_rem, mut nodes)) => {
            resolver::resolve_document(&mut nodes);
            for revert in guarded {
                let parts: Vec<&str> = revert.path.split('/').filter(|p| !p.is_empty()).collect();
                let node = match find_node_by_path_mut(&mut nodes, &parts) {
                    Some(n) => Some(n),
                    None => find_node_by_path_mut_transparent(&mut nodes, &parts),
                };
                if let Some(node) = node {
                    match revert.value {
                        Some(value) => {
                            node.parameters.insert("value".to_string(), value);
                        }
                        None => {
                            node.parameters.remove("value");
                        }
                    }
                    // The text this was parsed from holds the edited value, so the node no
                    // longer matches the source it would otherwise be replayed from. No
                    // override marker: this value belongs to the document, not to the user.
                    node.source_fingerprint = None;
                }
            }
            OverseerFileHandler::serialize_nodes(&nodes).map_err(|e| {
                OverseerError::SerializationError(format!("Failed to serialize document: {}", e))
            })
        }
        // Refusing to write is worse than writing exactly what the caller asked for.
        Err(_) => Ok(content),
    }
}

/// Run an event against a document given as text, returning the result and its new text.
///
/// The caller used to send the document itself, which on a large one costs seconds: the IPC
/// moves a couple of MB per second and the document is the biggest thing in the system. It
/// already holds the text from the previous resolve, and the text is two orders of magnitude
/// smaller, so that is what travels now. Rust owns the document; the caller owns a view of it.
pub fn execute_event_on_text(
    content: String,
    node_path: Vec<String>,
    event_name: String,
) -> Result<ResolvedDocument> {
    let mut nodes = load_document(content)?;
    crate::actions::start_reporting();
    let outcome = ActionExecutor::execute_event(&mut nodes, &node_path, &event_name);
    let report = crate::actions::take_report();
    outcome?;
    // The value stays where the action put it: this caller's view *is* the document it holds,
    // and the day it is showing has to move.
    drop(report);
    with_text(nodes)
}



fn with_text(nodes: Vec<OverseerNode>) -> Result<ResolvedDocument> {
    let text = OverseerFileHandler::serialize_nodes(&nodes).map_err(|e| {
        OverseerError::SerializationError(format!("Failed to serialize resolved document: {}", e))
    })?;
    // This pairing is what the caller now holds, and the baseline the next change
    // will be described against.
    remember(&text, &nodes);
    Ok(ResolvedDocument { nodes, text })
}

/// A resolved document together with its serialized text.
///
/// The caller needs both: the nodes to render, and the text to send back as the basis for the
/// next edit. Returning the text here is what lets the caller stop uploading the document -
/// serializing it costs about 25 ms on the machine that already holds it, against seconds to
/// move it across the IPC boundary.
#[derive(serde::Serialize)]
pub struct ResolvedDocument {
    pub nodes: Vec<OverseerNode>,
    pub text: String,
}

pub fn resolve_selective_with_text(
    content: String,
    changed_fields: Vec<String>,
    changed_field_values: Option<std::collections::HashMap<String, OverseerValue>>,
) -> Result<ResolvedDocument> {
    with_text(resolve_selective(content, changed_fields, changed_field_values)?)
}

pub fn resolve_selective(
    content: String,
    changed_fields: Vec<String>,
    changed_field_values: Option<std::collections::HashMap<String, OverseerValue>>,
) -> Result<Vec<OverseerNode>> {
    // TEMP DIAG: Surface raw changed_fields received from frontend (will be removed after bug fix)
    // Removed temporary verbose selective diagnostics (Raw changed_fields)
    #[cfg(feature = "debug-resolver")]
    println!(
        "🔄 Selective update called with {} changed fields: {:?}",
        changed_fields.len(),
        changed_fields
    );

    // Phase timings for one interaction, with OVERSEER_PROFILE=1.
    let profiling = resolver::profile_enabled();
    let phase = std::time::Instant::now();

    match parse_document(&content) {
        Ok((_remaining, mut nodes)) => {
            if profiling {
                eprintln!("[PHASE] parse {:.1} ms", phase.elapsed().as_secs_f64() * 1000.0);
            }
            let phase = std::time::Instant::now();
            // Mounted content never round-trips through the document text, so it is absent
            // again after every re-parse. Bring it back before resolving, or formulas that
            // read through a mount would resolve to errors on every edit.
            ActionExecutor::preload_mounts(&mut nodes);
            if profiling {
                eprintln!("[PHASE] preload {:.1} ms", phase.elapsed().as_secs_f64() * 1000.0);
            }
            let phase = std::time::Instant::now();
            // If no specific fields changed, do full resolution
            if changed_fields.is_empty() {
                #[cfg(feature = "debug-resolver")]
                println!("📋 No specific fields changed, performing full resolution");
                resolver::resolve_document(&mut nodes);
            } else {
                // NORMALIZATION + EXPANSION: produce a working set that includes:
                // 1) Original provided paths
                // 2) Normalized variants with empty segments removed (handles unnamed transparent wrappers)
                // 3) '/value' suffixed forms for nodes that own a value parameter
                let mut expanded_changed: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                for raw in &changed_fields {
                    expanded_changed.insert(raw.clone());
                    // Normalized variant (strip empty segments)
                    let norm: String = raw
                        .split('/')
                        .filter(|seg| !seg.is_empty())
                        .collect::<Vec<_>>()
                        .join("/");
                    if !norm.is_empty() {
                        expanded_changed.insert(norm.clone());
                    }
                }
                // Removed temporary verbose selective diagnostics (post-normalization)
                // Add /value expansion for any path (raw or normalized) that points to a node with a value param
                let snapshot_paths: Vec<String> = expanded_changed.clone().into_iter().collect();
                for p in snapshot_paths {
                    if p.ends_with("/value") {
                        continue;
                    }
                    let parts: Vec<&str> = p.split('/').filter(|seg| !seg.is_empty()).collect();
                    if parts.is_empty() {
                        continue;
                    }
                    // First attempt strict lookup; if it fails, try transparency-aware fuzzy lookup so that
                    // UI paths that intentionally skip unnamed transparent wrappers (to keep paths stable)
                    // still resolve to the underlying node for /value expansion and dependency cascade.
                    let strict = find_node_by_path(&nodes, &parts);
                    let fuzzy = if strict.is_none() {
                        find_node_by_path_transparent(&nodes, &parts)
                    } else {
                        None
                    };
                    let mut candidate = strict.or(fuzzy);
                    // Heuristic: if still none, try interpreting the last segment as a descendant of any
                    // node matched by all but the final segment (handling a skipped unnamed wrapper layer).
                    if candidate.is_none() && parts.len() > 1 {
                        let parent_parts = &parts[..parts.len() - 1];
                        let leaf = parts[parts.len() - 1];
                        if let Some(parent) = find_node_by_path_transparent(&nodes, parent_parts) {
                            for ch in &parent.children {
                                if ch.name == leaf {
                                    candidate = Some(ch);
                                    break;
                                }
                            }
                        }
                    }
                    // Template-based fallback: path points into a list item whose template has not yet been hydrated.
                    // Example: main/L/T__1/A where A is provided by template T (entry=<T>) but list item is empty pre-resolution.
                    if candidate.is_none() && parts.len() >= 4 {
                        // heuristic minimal length containing .../List/Item/Field
                        let field_name = parts[parts.len() - 1];
                        let item_seg = parts[parts.len() - 2];
                        let list_seg = parts[parts.len() - 3];
                        // Only proceed if segment looks like template instance (Name__N)
                        if item_seg.contains("__") {
                            let base_template_name = item_seg.split("__").next().unwrap_or("");
                            // Locate list node (may be nested anywhere under earlier path segments)
                            // We attempt to find list node by traversing parts up to list_seg.
                            // If found, inspect its entry template.
                            // naive DFS to find list node with matching name
                            fn dfs_find<'n>(
                                roots: &'n [OverseerNode],
                                name: &str,
                            ) -> Option<&'n OverseerNode> {
                                for n in roots {
                                    if n.name == name {
                                        return Some(n);
                                    }
                                    if let Some(found) = dfs_find(&n.children, name) {
                                        return Some(found);
                                    }
                                }
                                None
                            }
                            if let Some(list_node) = dfs_find(&nodes, list_seg) {
                                // Determine template name from list parameters.entry if base_template_name mismatch
                                let mut entry_template_name: Option<String> = None;
                                if let Some(entry_val) = list_node.parameters.get("entry") {
                                    match entry_val {
                                        OverseerValue::Template(s) => {
                                            entry_template_name = Some(s.clone())
                                        }
                                        OverseerValue::String(s) => {
                                            entry_template_name = Some(s.clone())
                                        }
                                        _ => {}
                                    }
                                }
                                if entry_template_name.is_none() && !base_template_name.is_empty() {
                                    entry_template_name = Some(base_template_name.to_string());
                                }
                                if let Some(tname) = entry_template_name {
                                    // DFS locate template definition anywhere in document
                                    fn dfs_template<'a>(
                                        roots: &'a [OverseerNode],
                                        name: &str,
                                    ) -> Option<&'a OverseerNode>
                                    {
                                        for n in roots {
                                            if n.name == name {
                                                return Some(n);
                                            }
                                            if let Some(found) = dfs_template(&n.children, name) {
                                                return Some(found);
                                            }
                                        }
                                        None
                                    }
                                    if let Some(template_node) = dfs_template(&nodes, &tname) {
                                        // Search inside template for field_name through transparent or unnamed wrappers
                                        fn dfs_field<'a>(
                                            node: &'a OverseerNode,
                                            target: &str,
                                        ) -> Option<&'a OverseerNode>
                                        {
                                            if node.name == target {
                                                return Some(node);
                                            }
                                            for ch in &node.children {
                                                // Allow traversal through all nodes; pruning only after match attempt
                                                if let Some(found) = dfs_field(ch, target) {
                                                    return Some(found);
                                                }
                                            }
                                            None
                                        }
                                        if let Some(field_node) =
                                            dfs_field(template_node, field_name)
                                        {
                                            if field_node.parameters.contains_key("value") {
                                                // We know hydration will create this node with a value param—permit /value expansion.
                                                expanded_changed.insert(format!("{}/value", p));
                                                continue; // done with this path
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if let Some(node) = candidate {
                        if node.parameters.contains_key("value") {
                            expanded_changed.insert(format!("{}/value", p));
                        }
                    }
                }
                // Removed temporary verbose selective diagnostics (after /value expansion)
                let changed_fields: Vec<String> = expanded_changed.into_iter().collect();
                #[cfg(feature = "debug-resolver")]
                println!(
                    "🧭 Normalized+expanded changed fields: {:?}",
                    changed_fields
                );
                // Ensure templates/inheritance are materialized before dependency-based selective updates.
                // Also preserve user changes before resolution clobbers them.
                let mut preserved_values = std::collections::HashMap::new();
                for changed_field in &changed_fields {
                    if let Some(value) = get_field_value_by_path(&nodes, changed_field) {
                        preserved_values.insert(changed_field.clone(), value.clone());
                        #[cfg(feature = "debug-resolver")]
                        println!(
                            "💾 Preserving field '{}' with value: {:?}",
                            changed_field, value
                        );
                    }
                }

                // Hydrate template instances and inheritance so dependent fields exist under
                // list items. Only the tree's shape is needed here: the edited values are
                // written immediately below and everything derived from them is computed by
                // the resolve that follows, so evaluating formulas and rebuilding chart
                // series at this point is work that is about to be thrown away.
                resolver::resolve_structure(&mut nodes);

                // Restore preserved values (user edits) obtained from pre-resolve tree (when present)
                for (field_path, value) in preserved_values.into_iter() {
                    if let Err(_e) = set_field_value_by_path(&mut nodes, &field_path, value) {
                        #[cfg(feature = "debug-resolver")]
                        println!("⚠️  Failed to restore field '{}'", field_path);
                    } else {
                        #[cfg(feature = "debug-resolver")]
                        println!("✅ Restored field '{}'", field_path);
                    }
                }

                // Additionally, apply explicit changed field values provided by the frontend.
                // This covers edits to fields inherited from templates (which may not exist pre-resolve),
                // ensuring recomputation uses the latest user input values.
                if let Some(map) = changed_field_values {
                    for (field_path, value) in map.into_iter() {
                        if let Err(_e) =
                            set_field_value_by_path(&mut nodes, &field_path, value.clone())
                        {
                            #[cfg(feature = "debug-resolver")]
                            println!(
                                "⚠️  Failed to apply changed field value for '{}'",
                                field_path
                            );
                        } else {
                            #[cfg(feature = "debug-resolver")]
                            println!("✅ Applied changed field value for '{}'", field_path);
                        }
                    }
                }

                // Recompute what the edit can reach, and nothing else.
                //
                // Dependency-directed updates ran here once and were taken out because they
                // missed cascades through unnamed transparent wrappers - a field inside an
                // unnamed div answers to the div's parent, so a read recorded a path to a node
                // that is not where the resolver keeps it. That is fixed, and there is now a test
                // that would have caught it: `the_cascade_covers_the_change` edits fields in the
                // real documents, resolves everything the slow way, and insists that every value
                // which moved was named by the graph first.
                //
                // Without a graph for this exact text - a document opened elsewhere, or one whose
                // text has moved on - everything is resolved, which is what happened before and
                // is never wrong.
                let ready = graph_for(&content)
                    .zip(baseline_copy(&content))
                    .map(|(graph, previous)| {
                        (graph.nodes_to_work_out_again(&changed_fields), graph, previous)
                    })
                    .filter(|(to_redo, _, _)| !to_redo.is_empty());

                match ready {
                    // Everything that was worked out last time, plus whatever this edit reaches.
                    Some((to_redo, mut graph, previous))
                        if carry_over_computed(
                            &mut nodes,
                            &previous,
                            &changed_fields.iter().cloned().collect(),
                            &mut Vec::new(),
                        ) =>
                    {
                        let targets: std::collections::HashSet<String> =
                            to_redo.into_iter().collect();
                        // Recorded again, so whatever was worked out says what it read this time -
                        // an edit that sends a lookup somewhere else leaves the graph describing
                        // where it goes now.
                        crate::dependencies::start_recording();
                        resolver::resolve_specific_fields(&mut nodes, &targets);
                        // Charts outright, not by the cascade: a series hangs on a plot child
                        // while the reads are recorded against the chart, so the names do not
                        // line up. Cheap enough that deciding is not worth the risk of a graph
                        // drawn from last week's numbers.
                        resolver::compute_chart_series(&mut nodes);
                        graph.absorb(crate::dependencies::take_recording());
                        remember_graph(&content, graph);
                    }
                    _ => resolver::resolve_values(&mut nodes),
                }
            }
            if profiling {
                eprintln!(
                    "[PHASE] resolve {:.1} ms",
                    phase.elapsed().as_secs_f64() * 1000.0
                );
            }
            #[cfg(feature = "debug-resolver")]
            println!("✅ Selective update completed");
            Ok(nodes)
        }
        Err(e) => {
            #[cfg(feature = "debug-resolver")]
            println!("❌ Parse error in selective update: {}", e);
            Err(OverseerError::ParseError(format!("Parse error: {}", e)))
        }
    }
}

