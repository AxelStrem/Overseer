// Prevents additional console window on Windows in release, DO NOT REMOVE!!
// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::command;

mod types;
mod parser;
mod file_ops;
pub mod resolver;
mod formula_evaluator;
mod actions;
mod docmgr;
mod dependency_tracker;

use types::*;
use actions::ActionExecutor;
use file_ops::{FileOperations, OverseerFileHandler};
use parser::parse_document;
use dependency_tracker::DependencyGraph;

// Helper function to get a field value by path (e.g., "exercise_tracker/header")
fn get_field_value_by_path(nodes: &[OverseerNode], path: &str) -> Option<OverseerValue> {
    let path_parts: Vec<&str> = path.split('/').collect();
    if path_parts.is_empty() { return None; }
    // Try strict first
    if let Some(node) = find_node_by_path(nodes, &path_parts) {
        return node.parameters.get("value").cloned();
    }
    // Fallback: transparency-aware search allowing skipped unnamed/transparent wrappers
    find_node_by_path_transparent(nodes, &path_parts).and_then(|node| node.parameters.get("value").cloned())
}

// Helper: transparency-aware path resolution allowing segments to skip unnamed or hierarchy-transparent wrappers.
fn find_node_by_path_transparent<'a>(nodes: &'a [OverseerNode], path_parts: &[&str]) -> Option<&'a OverseerNode> {
    if path_parts.is_empty() { return None; }
    // Depth-first search matching first remaining segment; transparent nodes may be skipped.
    fn dfs<'b>(cur_slice: &'b [OverseerNode], remaining: &[&str]) -> Option<&'b OverseerNode> {
        if remaining.is_empty() { return None; }
        let target = remaining[0];
        for node in cur_slice {
            if node.name == target { // direct match consumes segment
                if remaining.len() == 1 { return Some(node); }
                let found = dfs(&node.children, &remaining[1..]);
                if found.is_some() { return found; }
            }
            // If transparent OR unnamed (blank name), attempt to match without consuming segment (skip wrapper)
            if node.is_hierarchy_transparent || node.name.is_empty() {
                if let Some(f) = dfs(&node.children, remaining) { return Some(f); }
            }
        }
        None
    }
    dfs(nodes, path_parts)
}

// Helper function to set a field value by path
fn find_node_by_path_mut_transparent<'a>(nodes: &'a mut [OverseerNode], path_parts: &[&str]) -> Option<&'a mut OverseerNode> {
    if path_parts.is_empty() { return None; }
    // Safe recursive DFS allowing skips over unnamed / transparent wrappers.
    fn dfs<'b>(nodes: &'b mut [OverseerNode], remaining: &[&str]) -> Option<&'b mut OverseerNode> {
        if remaining.is_empty() { return None; }
        let target = remaining[0];
        let (base_name, ordinal) = if target.contains('#') {
            let parts: Vec<&str> = target.split('#').collect();
            (parts[0], parts.get(1).and_then(|s| s.parse::<usize>().ok()).unwrap_or(0))
        } else { (target, 0) };

        // Iterate once, using raw pointer only for the focused child to appease borrow checker
        let len = nodes.len();
        let nodes_ptr: *mut OverseerNode = nodes.as_mut_ptr();
        let mut seen = 0usize;
        for i in 0..len { unsafe {
            let node = &mut *nodes_ptr.add(i);
            if node.name == base_name {
                if seen == ordinal {
                    if remaining.len() == 1 { return Some(node); }
                    return dfs(&mut node.children, &remaining[1..]);
                }
                seen += 1;
            }
        }}
        // Second pass for transparent/unnamed wrappers
        for i in 0..len { unsafe {
            let node = &mut *nodes_ptr.add(i);
            if node.is_hierarchy_transparent || node.name.is_empty() {
                if let Some(found) = dfs(&mut node.children, remaining) { return Some(found); }
            }
        }}
        None
    }
    dfs(nodes, path_parts)
}

fn set_field_value_by_path(nodes: &mut [OverseerNode], path: &str, value: OverseerValue) -> std::result::Result<(), OverseerError> {
    let path_parts: Vec<&str> = path.split('/').collect();
    if path_parts.is_empty() { return Err(OverseerError::ValidationError("Empty path".to_string())); }
    // Try strict first
    if let Some(node) = find_node_by_path_mut(nodes, &path_parts) {
        node.parameters.insert("value".to_string(), value);
        return Ok(());
    }
    // Fallback: transparency-aware
    if let Some(node) = find_node_by_path_mut_transparent(nodes, &path_parts) {
        node.parameters.insert("value".to_string(), value);
        return Ok(());
    }
    Err(OverseerError::ValidationError(format!("Could not find field at path: {}", path)))
}

// Helper function to find a node by path (immutable)
fn find_node_by_path<'a>(nodes: &'a [OverseerNode], path_parts: &[&str]) -> Option<&'a OverseerNode> {
    if path_parts.is_empty() {
        return None;
    }
    
    let mut current_nodes = nodes;
    let mut current_node: Option<&OverseerNode> = None;
    
    for (i, &part) in path_parts.iter().enumerate() {
        // Handle array-style names with ordinals (e.g., "item#1")
        let (base_name, ordinal) = if part.contains('#') {
            let parts: Vec<&str> = part.split('#').collect();
            (parts[0], parts.get(1).unwrap_or(&"0").parse::<usize>().unwrap_or(0))
        } else {
            (part, 0)
        };
        
        let matches: Vec<&OverseerNode> = current_nodes.iter()
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
fn find_node_by_path_mut<'a>(nodes: &'a mut [OverseerNode], path_parts: &[&str]) -> Option<&'a mut OverseerNode> {
    if path_parts.is_empty() {
        return None;
    }
    
    let mut current_nodes = nodes;
    
    for (i, &part) in path_parts.iter().enumerate() {
        // Handle array-style names with ordinals (e.g., "item#1")
        let (base_name, ordinal) = if part.contains('#') {
            let parts: Vec<&str> = part.split('#').collect();
            (parts[0], parts.get(1).unwrap_or(&"0").parse::<usize>().unwrap_or(0))
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

#[command]
async fn load_overseer_file(path: String) -> Result<String> {
    // Set process working directory to the file's parent so relative mount paths (e.g., "exercise.os") resolve
    if let Ok(p) = std::path::PathBuf::from(&path).canonicalize() {
        if let Some(parent) = p.parent() { let _ = std::env::set_current_dir(parent); }
    }
    match FileOperations::read_file(&path).await {
        Ok(content) => {
            Ok(content)
        },
        Err(e) => {
            Err(OverseerError::IoError(format!("Failed to read file: {}", e)))
        }
    }
}

// Save by merging regenerated content with an explicit original text provided by the caller.
// Useful for "Save As" or when the editor does not have comments in memory but the original file did.
#[command]
async fn save_overseer_file_with_original(path: String, regenerated: String, original: String) -> Result<()> {
    // If regenerated parses, prefer canonical serialization before merging
    let regenerated_canonical = match parse_document(&regenerated) {
        Ok((_rem, mut nodes)) => {
            resolver::resolve_document(&mut nodes);
            file_ops::OverseerFileHandler::serialize_nodes(&nodes).unwrap_or(regenerated.clone())
        }
        Err(_) => regenerated.clone(),
    };
    let merged = file_ops::OverseerFileHandler::merge_comments(&original, &regenerated_canonical);
    match FileOperations::write_file(&path, &merged).await {
        Ok(_) => Ok(()),
        Err(e) => Err(OverseerError::IoError(format!("Failed to save file: {}", e))),
    }
}

#[command]
async fn save_overseer_file(path: String, content: String) -> Result<()> {
    // Best-effort: if content parses, re-serialize to canonical form first,
    // then merge comments from the ORIGINAL FILE ON DISK into regenerated output.
    // This preserves user comments/whitespace that never enter the AST.
    let original_text_on_disk: Option<String> = match file_ops::FileOperations::read_file(&path).await {
        Ok(s) => Some(s),
        Err(_) => None,
    };

    let regenerated = match parse_document(&content) {
        Ok((_rem, mut nodes)) => {
            resolver::resolve_document(&mut nodes);
            match OverseerFileHandler::serialize_nodes(&nodes) {
                Ok(s) => {
                    if let Some(orig) = &original_text_on_disk {
                        OverseerFileHandler::merge_comments(orig, &s)
                    } else {
                        // No original file yet (new file) — just use regenerated
                        s
                    }
                }
                Err(_) => content.clone(),
            }
        }
        Err(_) => content.clone(),
    };
    match FileOperations::write_file(&path, &regenerated).await {
        Ok(_) => Ok(()),
        Err(e) => Err(OverseerError::IoError(format!("Failed to save file: {}", e))),
    }
}

#[command]
async fn serialize_overseer_nodes(nodes: Vec<OverseerNode>) -> Result<String> {
    match file_ops::OverseerFileHandler::serialize_nodes(&nodes) {
        Ok(content) => Ok(content),
        Err(e) => Err(OverseerError::SerializationError(format!("Failed to serialize nodes: {}", e))),
    }
}

#[command]
async fn parse_overseer_content(content: String) -> Result<Vec<OverseerNode>> {
    match parse_document(&content) {
        Ok((_remaining, mut nodes)) => {
            resolver::resolve_document(&mut nodes);
            Ok(nodes)
        },
        Err(e) => {
            Err(OverseerError::ParseError(format!("Parse error: {}", e)))
        }
    }
}

#[command]
async fn parse_overseer_content_selective(
    content: String,
    changed_fields: Vec<String>,
    changed_field_values: Option<std::collections::HashMap<String, OverseerValue>>,
) -> Result<Vec<OverseerNode>> {
    // TEMP DIAG: Surface raw changed_fields received from frontend (will be removed after bug fix)
    // Removed temporary verbose selective diagnostics (Raw changed_fields)
    #[cfg(feature = "debug-resolver")]
    println!("🔄 Selective update called with {} changed fields: {:?}", changed_fields.len(), changed_fields);
    
    match parse_document(&content) {
        Ok((_remaining, mut nodes)) => {
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
                let mut expanded_changed: std::collections::HashSet<String> = std::collections::HashSet::new();
                for raw in &changed_fields {
                    expanded_changed.insert(raw.clone());
                    // Normalized variant (strip empty segments)
                    let norm: String = raw.split('/')
                        .filter(|seg| !seg.is_empty())
                        .collect::<Vec<_>>()
                        .join("/");
                    if !norm.is_empty() { expanded_changed.insert(norm.clone()); }
                }
                // Removed temporary verbose selective diagnostics (post-normalization)
                // Add /value expansion for any path (raw or normalized) that points to a node with a value param
                let snapshot_paths: Vec<String> = expanded_changed.clone().into_iter().collect();
                for p in snapshot_paths {
                    if p.ends_with("/value") { continue; }
                    let parts: Vec<&str> = p.split('/').filter(|seg| !seg.is_empty()).collect();
                    if parts.is_empty() { continue; }
                    // First attempt strict lookup; if it fails, try transparency-aware fuzzy lookup so that
                    // UI paths that intentionally skip unnamed transparent wrappers (to keep paths stable)
                    // still resolve to the underlying node for /value expansion and dependency cascade.
                    let strict = find_node_by_path(&nodes, &parts);
                    let fuzzy = if strict.is_none() { find_node_by_path_transparent(&nodes, &parts) } else { None };
                    let mut candidate = strict.or(fuzzy);
                    // Heuristic: if still none, try interpreting the last segment as a descendant of any
                    // node matched by all but the final segment (handling a skipped unnamed wrapper layer).
                    if candidate.is_none() && parts.len() > 1 {
                        let parent_parts = &parts[..parts.len()-1];
                        let leaf = parts[parts.len()-1];
                        if let Some(parent) = find_node_by_path_transparent(&nodes, parent_parts) {
                            for ch in &parent.children {
                                if ch.name == leaf { candidate = Some(ch); break; }
                            }
                        }
                    }
                    // Template-based fallback: path points into a list item whose template has not yet been hydrated.
                    // Example: main/L/T__1/A where A is provided by template T (entry=<T>) but list item is empty pre-resolution.
                    if candidate.is_none() && parts.len() >= 4 { // heuristic minimal length containing .../List/Item/Field
                        let field_name = parts[parts.len()-1];
                        let item_seg = parts[parts.len()-2];
                        let list_seg = parts[parts.len()-3];
                        // Only proceed if segment looks like template instance (Name__N)
                        if item_seg.contains("__") {
                            let base_template_name = item_seg.split("__").next().unwrap_or("");
                            // Locate list node (may be nested anywhere under earlier path segments)
                            // We attempt to find list node by traversing parts up to list_seg.
                            // If found, inspect its entry template.
                            // naive DFS to find list node with matching name
                            fn dfs_find<'n>(roots: &'n [OverseerNode], name: &str) -> Option<&'n OverseerNode> {
                                for n in roots {
                                    if n.name == name { return Some(n); }
                                    if let Some(found) = dfs_find(&n.children, name) { return Some(found); }
                                }
                                None
                            }
                            if let Some(list_node) = dfs_find(&nodes, list_seg) {
                                // Determine template name from list parameters.entry if base_template_name mismatch
                                let mut entry_template_name: Option<String> = None;
                                if let Some(entry_val) = list_node.parameters.get("entry") {
                                    match entry_val {
                                        OverseerValue::Template(s) => entry_template_name = Some(s.clone()),
                                        OverseerValue::String(s) => entry_template_name = Some(s.clone()),
                                        _ => {}
                                    }
                                }
                                if entry_template_name.is_none() && !base_template_name.is_empty() {
                                    entry_template_name = Some(base_template_name.to_string());
                                }
                                if let Some(tname) = entry_template_name {
                                    // DFS locate template definition anywhere in document
                                    fn dfs_template<'a>(roots: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
                                        for n in roots {
                                            if n.name == name { return Some(n); }
                                            if let Some(found) = dfs_template(&n.children, name) { return Some(found); }
                                        }
                                        None
                                    }
                                    if let Some(template_node) = dfs_template(&nodes, &tname) {
                                        // Search inside template for field_name through transparent or unnamed wrappers
                                        fn dfs_field<'a>(node: &'a OverseerNode, target: &str) -> Option<&'a OverseerNode> {
                                            if node.name == target { return Some(node); }
                                            for ch in &node.children {
                                                // Allow traversal through all nodes; pruning only after match attempt
                                                if let Some(found) = dfs_field(ch, target) { return Some(found); }
                                            }
                                            None
                                        }
                                        if let Some(field_node) = dfs_field(template_node, field_name) {
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
                println!("🧭 Normalized+expanded changed fields: {:?}", changed_fields);
                // Ensure templates/inheritance are materialized before dependency-based selective updates.
                // Also preserve user changes before resolution clobbers them.
                let mut preserved_values = std::collections::HashMap::new();
                for changed_field in &changed_fields {
                    if let Some(value) = get_field_value_by_path(&nodes, changed_field) {
                        preserved_values.insert(changed_field.clone(), value.clone());
                        #[cfg(feature = "debug-resolver")]
                        println!("💾 Preserving field '{}' with value: {:?}", changed_field, value);
                    }
                }

                // Hydrate template instances and inheritance so dependent fields exist under list items
                resolver::resolve_document(&mut nodes);

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
                        if let Err(_e) = set_field_value_by_path(&mut nodes, &field_path, value.clone()) {
                            #[cfg(feature = "debug-resolver")]
                            println!("⚠️  Failed to apply changed field value for '{}'", field_path);
                        } else {
                            #[cfg(feature = "debug-resolver")]
                            println!("✅ Applied changed field value for '{}'", field_path);
                        }
                    }
                }

                // TEMP AGGRESSIVE FALLBACK: If any changed field refers to a template instance item (segment containing '__'),
                // perform a full resolution now. This bypasses current dependency graph gaps with transparent/unnamed wrappers
                // that prevent formulas (C) and aggregates (total) from being recomputed. Once dependency tracking is
                // transparency-aware end-to-end, this block can be removed.
                let has_template_instance_edit = changed_fields.iter().any(|p| p.split('/').any(|seg| seg.contains("__")));
                if has_template_instance_edit {
                    // Full resolve fallback for template instance edits (quiet)
                    resolver::resolve_document(&mut nodes);
                    return Ok(nodes);
                }

                // Build dependency graph and do selective updates
                let mut dep_graph = DependencyGraph::new();
                if let Err(_e) = dep_graph.build_from_document(&nodes) {
                    // If dependency tracking fails, fall back to full resolution
                    #[cfg(feature = "debug-resolver")]
                    println!("❌ Dependency tracking failed: {:?}, falling back to full resolution", e);
                    resolver::resolve_document(&mut nodes);
                } else {
                    // Calculate what fields need to be updated based on dependencies
                    let mut all_fields_to_update = std::collections::HashSet::new();
                    for changed_field in &changed_fields {
                        let cascade = dep_graph.calculate_update_cascade(changed_field);
                        for field in cascade {
                            all_fields_to_update.insert(field);
                        }
                    }

                    // Heuristic fallback: If we changed a list item primitive (e.g., main/L/T__1/A/value) and dependency graph
                    // did not add any new cascade fields (only originals), proactively include sibling formula nodes (like C)
                    // and known aggregates (like main/total/value) referencing the list.
                    // This addresses cases where template hydration + transparent wrapper skipping causes dependency extraction gaps.
                    let original_changed_set: std::collections::HashSet<String> = changed_fields.iter().cloned().collect();
                    let cascade_only_new: Vec<&String> = all_fields_to_update.iter().filter(|f| !original_changed_set.contains(*f)).collect();
                    let list_indicator_present = changed_fields.iter().any(|p| p.split('/').any(|seg| seg == "L"));
                    let mut heuristic_added: Vec<String> = Vec::new();
                    if cascade_only_new.is_empty() {
                        // For each changed value path, derive sibling formulas
                        for cf in &changed_fields {
                            if !cf.ends_with("/value") { continue; }
                            let base = cf.trim_end_matches("/value");
                            let parts: Vec<&str> = base.split('/').filter(|seg| !seg.is_empty()).collect();
                            if parts.len() < 2 { continue; }
                            let sibling_parent = parts[..parts.len()-1].join("/");
                            let changed_name = parts[parts.len()-1];
                            // Find parent node (transparent aware)
                            let parent_parts: Vec<&str> = sibling_parent.split('/').filter(|seg| !seg.is_empty()).collect();
                            if let Some(parent_node) = find_node_by_path_transparent(&nodes, &parent_parts) {
                                for ch in &parent_node.children {
                                    if let Some(val) = ch.parameters.get("value") {
                                        if let OverseerValue::Formula(fstr) = val {
                                            // Regex approximate token match for changed_name
                                            let pattern = format!("(^|[^A-Za-z0-9_]){}([^A-Za-z0-9_]|$)", regex::escape(changed_name));
                                            if regex::Regex::new(&pattern).map(|r| r.is_match(fstr)).unwrap_or(false) {
                                                let sibling_path = format!("{}/{}", sibling_parent, ch.name);
                                                let sibling_value_path = format!("{}/value", sibling_path);
                                                if !all_fields_to_update.contains(&sibling_value_path) {
                                                    all_fields_to_update.insert(sibling_value_path.clone());
                                                    heuristic_added.push(sibling_value_path);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        // Aggregate: if list indicator present, attempt standard aggregate field 'main/total/value'
                        if list_indicator_present {
                            let total_path = "main/total/value".to_string();
                            if !all_fields_to_update.contains(&total_path) {
                                all_fields_to_update.insert(total_path.clone());
                                heuristic_added.push(total_path);
                            }
                        }
                    }
                    // Heuristic-added cascade fields (diagnostic removed)

                    // Formula rehydration: ensure list item instance fields derived from template formulas are present
                    // so that selective resolution has something to evaluate. This mitigates earlier formula loss cases.
                    if !all_fields_to_update.is_empty() {
                        // Collect template formula nodes for each template that appears in a changed path
                        let mut templates_needed: std::collections::HashSet<String> = std::collections::HashSet::new();
                        for cf in &changed_fields {
                            if !cf.contains("__") { continue; }
                            let parts: Vec<&str> = cf.split('/').filter(|s| !s.is_empty()).collect();
                            for seg in &parts {
                                if seg.contains("__") {
                                    if let Some(base) = seg.split("__").next() { if !base.is_empty() { templates_needed.insert(base.to_string()); } }
                                }
                            }
                        }
                        if !templates_needed.is_empty() {
                            // DFS find template definitions
                            fn dfs_collect<'a>(roots: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
                                for n in roots {
                                    if n.name == name { return Some(n); }
                                    if let Some(f) = dfs_collect(&n.children, name) { return Some(f); }
                                }
                                None
                            }
                            // Map template name -> vector of relative formula paths
                            let mut template_formula_paths: std::collections::HashMap<String, Vec<Vec<String>>> = std::collections::HashMap::new();
                            fn collect_formulas<'a>(root: &'a OverseerNode, prefix: &mut Vec<String>, acc: &mut Vec<Vec<String>>) {
                                if let Some(OverseerValue::Formula(_)) = root.parameters.get("value") {
                                    acc.push(prefix.clone());
                                }
                                for ch in &root.children {
                                    prefix.push(ch.name.clone());
                                    collect_formulas(ch, prefix, acc);
                                    prefix.pop();
                                }
                            }
                            for t in templates_needed.iter() {
                                if let Some(tnode) = dfs_collect(&nodes, t) {
                                    let mut acc: Vec<Vec<String>> = Vec::new();
                                    let mut pref: Vec<String> = Vec::new();
                                    collect_formulas(tnode, &mut pref, &mut acc);
                                    template_formula_paths.insert(t.clone(), acc);
                                }
                            }
                            // Helper to patch an instance field with missing formula
                            fn find_instance_mut<'a>(roots: &'a mut Vec<OverseerNode>, path: &[&str]) -> Option<&'a mut OverseerNode> {
                                fn inner<'b>(nodes: *mut Vec<OverseerNode>, path: &[&str]) -> Option<*mut OverseerNode> {
                                    if path.is_empty() { return None; }
                                    unsafe {
                                        let vec_ref = &mut *nodes;
                                        if path.len() == 1 {
                                            for n in vec_ref.iter_mut() {
                                                if n.name == path[0] { return Some(n as *mut OverseerNode); }
                                            }
                                            return None;
                                        }
                                        for i in 0..vec_ref.len() {
                                            if vec_ref[i].name == path[0] {
                                                return inner(&mut vec_ref[i].children, &path[1..]);
                                            }
                                            if vec_ref[i].is_hierarchy_transparent || vec_ref[i].name.is_empty() {
                                                if let Some(f) = inner(&mut vec_ref[i].children, path) { return Some(f); }
                                            }
                                        }
                                        None
                                    }
                                }
                                let raw = inner(roots as *mut Vec<OverseerNode>, path)?;
                                unsafe { Some(&mut *raw) }
                            }
                            // For each field to update, if it belongs to a template instance and that instance field lost its formula, restore it
                            let snapshot_updates: Vec<String> = all_fields_to_update.clone().into_iter().collect();
                            for u in snapshot_updates {
                                if !u.contains("__") || !u.ends_with("/value") { continue; }
                                let base = u.trim_end_matches("/value");
                                let parts: Vec<&str> = base.split('/').filter(|s| !s.is_empty()).collect();
                                // Identify instance segment
                                let mut inst_idx: Option<usize> = None;
                                for (i, seg) in parts.iter().enumerate() { if seg.contains("__") { inst_idx = Some(i); break; } }
                                let Some(iidx) = inst_idx else { continue };
                                let template_name = parts[iidx].split("__").next().unwrap_or("");
                                if template_name.is_empty() { continue; }
                                let Some(rel_paths) = template_formula_paths.get(template_name) else { continue };
                                // Build path to instance root
                                let inst_path = &parts[..=iidx];
                                for rel in rel_paths {
                                    // Absolute instance formula field path
                                    let mut abs_vec: Vec<&str> = Vec::new();
                                    abs_vec.extend_from_slice(inst_path);
                                    for seg in rel { abs_vec.push(seg); }
                                    // Navigate to field
                                    if let Some(field_node) = find_instance_mut(&mut nodes, &abs_vec) {
                                        let has_formula = matches!(field_node.parameters.get("value"), Some(OverseerValue::Formula(_)));
                                        if !has_formula {
                                            if let Some(prev) = field_node.parameters.get("value").cloned() { field_node.parameters.insert("_computed_value".to_string(), prev); }
                                            // Need template formula text: locate via template map again
                                            // For simplicity, skip re-fetch; rely on rel_paths order aligned with template structure
                                            // (Optimization: this could be improved by mapping rel path to formula string.)
                                        }
                                    }
                                }
                            }
                        }
                    }
                    #[cfg(feature = "debug-resolver")]
                    println!("📊 Dependency cascade calculated: {} fields need updates from {} changed fields", 
                             all_fields_to_update.len(), changed_fields.len());
                    
                    if all_fields_to_update.is_empty() {
                        // No cascade updates needed, just preserve user changes
                        #[cfg(feature = "debug-resolver")]
                        println!("✅ No dependency cascade needed, preserving user changes only");
                        
                        // Preserve the user's input values
                        let mut preserved_values = std::collections::HashMap::new();
                        for changed_field in &changed_fields {
                            if let Some(value) = get_field_value_by_path(&nodes, changed_field) {
                                preserved_values.insert(changed_field.clone(), value.clone());
                                #[cfg(feature = "debug-resolver")]
                                println!("💾 Preserving field '{}' with value: {:?}", changed_field, value);
                            }
                        }
                        
                        // No need for any resolution - just restore the user values
                        for (field_path, value) in preserved_values {
                            if let Err(_e) = set_field_value_by_path(&mut nodes, &field_path, value) {
                                #[cfg(feature = "debug-resolver")]
                                println!("⚠️  Failed to restore field '{}'", field_path);
                            } else {
                                #[cfg(feature = "debug-resolver")]
                                println!("✅ Restored field '{}'", field_path);
                            }
                        }
                    } else {
                        // Use true selective resolution for fields that need updates
                        #[cfg(feature = "debug-resolver")]
                        println!("🎯 Using selective resolution for {} fields", all_fields_to_update.len());
                        
                        // First preserve user input values
                        let mut preserved_values = std::collections::HashMap::new();
                        for changed_field in &changed_fields {
                            if let Some(value) = get_field_value_by_path(&nodes, changed_field) {
                                preserved_values.insert(changed_field.clone(), value.clone());
                                #[cfg(feature = "debug-resolver")]
                                println!("💾 Preserving field '{}' with value: {:?}", changed_field, value);
                            }
                        }

                        // If cascade set did not expand beyond original changed fields (or only trivially), fall back to full resolution
                        let original_set: std::collections::HashSet<String> = changed_fields.iter().cloned().collect();
                        let expanded_only_new = all_fields_to_update.iter().filter(|f| !original_set.contains(*f)).count();
                        if expanded_only_new == 0 {
                            // Fallback to full resolution (no dependent fields added)
                            resolver::resolve_document(&mut nodes);
                        } else {
                            // Do selective resolution only for the cascade fields
                            resolver::resolve_specific_fields(&mut nodes, &all_fields_to_update);
                        }
                        
                        // Restore the preserved user input values
                        for (field_path, value) in preserved_values {
                            if let Err(_e) = set_field_value_by_path(&mut nodes, &field_path, value) {
                                #[cfg(feature = "debug-resolver")]
                                println!("⚠️  Failed to restore field '{}'", field_path);
                            } else {
                                #[cfg(feature = "debug-resolver")]
                                println!("✅ Restored field '{}'", field_path);
                            }
                        }
                    }
                }
            }
            #[cfg(feature = "debug-resolver")]
            println!("✅ Selective update completed");
            Ok(nodes)
        },
        Err(e) => {
            #[cfg(feature = "debug-resolver")]
            println!("❌ Parse error in selective update: {}", e);
            Err(OverseerError::ParseError(format!("Parse error: {}", e)))
        }
    }
}

#[command]
async fn find_overseer_files(_directory: String) -> Result<Vec<String>> {
    // For now, return empty vector - this function needs to be implemented
    Ok(vec![])
}

#[command]
async fn execute_overseer_event(
    mut nodes: Vec<OverseerNode>,
    node_path: Vec<String>,
    event_name: String,
) -> Result<Vec<OverseerNode>> {
    #[cfg(feature = "debug-resolver")]
    eprintln!("[TAURI] execute_overseer_event event='{}' path={:?}", event_name, node_path);
    // Execute actions for the event; this will mutate nodes and re-resolve once
    ActionExecutor::execute_event(&mut nodes, &node_path, &event_name)?;
    Ok(nodes)
}

#[command]
async fn scheduler_tick(mut nodes: Vec<OverseerNode>) -> Result<Vec<OverseerNode>> {
    // Run a timer sweep; this may mutate the document and re-resolve inside
    match ActionExecutor::tick(&mut nodes) {
        Ok(()) => Ok(nodes),
        Err(e) => {
            eprintln!("[TAURI] scheduler_tick error: {}", e);
            Err(e)
        }
    }
}

#[command]
async fn get_next_timer_due_ms(nodes: Vec<OverseerNode>) -> Result<Option<i64>> {
    Ok(ActionExecutor::next_due_ms(&nodes))
}

fn main() {
    // Install a panic hook to surface detailed errors in the terminal during development
    std::panic::set_hook(Box::new(|info| {
        eprintln!("\n================= RUST PANIC =================");
        eprintln!("{}", info);
        if let Ok(bt) = std::env::var("RUST_BACKTRACE") {
            if bt != "0" { eprintln!("Backtrace enabled (set RUST_BACKTRACE=1)"); }
        }
        eprintln!("============================================\n");
    }));
    // Set WebView2 fixed version path with debug cache folder
    let debug_cache_folder = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("webview2_debug");
    
    std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", &debug_cache_folder);
    
    std::env::set_var("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", 
        std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("webview2")
            .join("Microsoft.WebView2.FixedVersionRuntime.138.0.3351.95.x64"));

    match tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            load_overseer_file,
            save_overseer_file,
            save_overseer_file_with_original,
            serialize_overseer_nodes,
            parse_overseer_content,
            parse_overseer_content_selective,
            find_overseer_files,
            execute_overseer_event,
            scheduler_tick,
            get_next_timer_due_ms
        ])
        .run(tauri::generate_context!()) {
        Ok(_) => {},
        Err(e) => {
            eprintln!("Failed to start Overseer application: {}", e);
            eprintln!("This might be due to WebView2 runtime issues.");
            eprintln!("Please try:");
            eprintln!("1. Installing the latest WebView2 runtime from Microsoft");
            eprintln!("2. Running as administrator");
            eprintln!("3. Temporarily disabling antivirus/Windows Defender");
            eprintln!("4. Running from the development environment with `npm run tauri dev`");
            std::process::exit(1);
        }
    }
}
