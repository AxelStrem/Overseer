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

pub fn load_document(content: String) -> Result<Vec<OverseerNode>> {
    match parse_document(&content) {
        Ok((_remaining, mut nodes)) => {
            resolver::resolve_document(&mut nodes);
            // Mounted content is not part of the host document's text, so it has to be
            // brought in after every parse. Only re-resolve when something actually arrived:
            // resolving is a full pass over every node, and a document without mounts was
            // paying for a second one that could not change anything.
            if ActionExecutor::preload_mounts(&mut nodes) {
                resolver::resolve_document(&mut nodes);
            }
            Ok(nodes)
        }
        Err(e) => Err(OverseerError::ParseError(format!("Parse error: {}", e))),
    }
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
    ActionExecutor::execute_event(&mut nodes, &node_path, &event_name)?;
    with_text(nodes)
}

/// A timer sweep over a document given as text.
pub fn tick_on_text(content: String) -> Result<ResolvedDocument> {
    let mut nodes = load_document(content)?;
    ActionExecutor::tick(&mut nodes)?;
    with_text(nodes)
}

/// When the next timer in a document given as text is due.
pub fn next_due_ms_on_text(content: String) -> Result<Option<i64>> {
    Ok(ActionExecutor::next_due_ms(&load_document(content)?))
}

fn with_text(nodes: Vec<OverseerNode>) -> Result<ResolvedDocument> {
    let text = OverseerFileHandler::serialize_nodes(&nodes).map_err(|e| {
        OverseerError::SerializationError(format!("Failed to serialize resolved document: {}", e))
    })?;
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
    let profiling = std::env::var("OVERSEER_PROFILE").is_ok();
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

                // Recompute everything the edit can reach. Dependency-directed updates ran
                // here before, but they missed cascades through unnamed transparent wrappers,
                // so the frontend compensated by following every selective resolve with a
                // second full one - paying two resolves and four document transfers per edit
                // to get what one pass can guarantee. The structure is already resolved above
                // and only values have been written since, so resolving values is all that is
                // outstanding; on a 15K-node document it also measures faster than building
                // and walking the dependency graph.
                resolver::resolve_values(&mut nodes);
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

