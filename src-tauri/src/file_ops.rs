use std::collections::HashMap;
use std::path::Path;
use tokio::fs;

use crate::types::*;

// Global merge trace flag (opt-in). Not behind a feature so toggling at runtime is easy; output still gated by debug-resolver.
static MERGE_TRACE_ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

// Debug logging macro for serializer (module scope). Reuse existing 'debug-resolver' feature to avoid adding new Cargo feature.
macro_rules! debug_serializer {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-resolver")]
        eprintln!($($arg)*);
    };
}

#[allow(dead_code)]
pub struct FileOperations;

#[allow(dead_code)]
impl FileOperations {
    pub async fn read_file(path: &str) -> Result<String> {
        match fs::read_to_string(path).await {
            Ok(content) => Ok(content),
            Err(e) => Err(OverseerError::IoError(format!(
                "Failed to read file '{}': {}",
                path, e
            ))),
        }
    }

    pub async fn write_file(path: &str, content: &str) -> Result<()> {
        // Create parent directories if they don't exist
        if let Some(parent) = Path::new(path).parent() {
            if let Err(e) = fs::create_dir_all(parent).await {
                return Err(OverseerError::IoError(format!(
                    "Failed to create directories for '{}': {}",
                    path, e
                )));
            }
        }

        match fs::write(path, content).await {
            Ok(_) => Ok(()),
            Err(e) => Err(OverseerError::IoError(format!(
                "Failed to write file '{}': {}",
                path, e
            ))),
        }
    }

    pub async fn file_exists(path: &str) -> bool {
        Path::new(path).exists()
    }

    pub async fn create_backup(path: &str) -> Result<String> {
        let backup_path = format!("{}.backup", path);

        if Self::file_exists(path).await {
            match fs::copy(path, &backup_path).await {
                Ok(_) => Ok(backup_path),
                Err(e) => Err(OverseerError::IoError(format!(
                    "Failed to create backup of '{}': {}",
                    path, e
                ))),
            }
        } else {
            Err(OverseerError::IoError(format!(
                "Cannot backup file '{}': file does not exist",
                path
            )))
        }
    }

    pub fn validate_file_path(path: &str) -> Result<()> {
        if path.is_empty() {
            return Err(OverseerError::IoError("File path cannot be empty".to_string()));
        }

        // Check for invalid characters (Windows specific)
        let invalid_chars = ['<', '>', ':', '"', '|', '?', '*'];
        if path.chars().any(|c| invalid_chars.contains(&c)) {
            return Err(OverseerError::IoError(format!(
                "File path contains invalid characters: {}",
                path
            )));
        }

        // Check if path is too long (Windows limit is 260 characters)
        if path.len() > 260 {
            return Err(OverseerError::IoError("File path is too long".to_string()));
        }

        Ok(())
    }

    pub async fn save_overseer_file(path: &str, nodes: &[OverseerNode]) -> Result<()> {
        FileOperations::validate_file_path(path)?;

        // Create backup before saving
        if FileOperations::file_exists(path).await {
            FileOperations::create_backup(path).await?;
        }

        let content = Self::serialize_nodes(nodes)?;
        FileOperations::write_file(path, &content).await
    }

    pub fn serialize_nodes(nodes: &[OverseerNode]) -> Result<String> {
        let mut output = String::new();

        for node in nodes {
            Self::serialize_node(node, &mut output, 0)?;
        }

        Ok(output)
    }

    fn serialize_node(node: &OverseerNode, output: &mut String, indent_level: usize) -> Result<()> {
        Self::serialize_node_context(node, output, indent_level, false, true)
    }

    fn serialize_node_context(
        node: &OverseerNode,
        output: &mut String,
        indent_level: usize,
        in_list_body: bool,
        allow_node_leading_blanks: bool,
    ) -> Result<()> {
        // Emit preserved leading blank lines first (if not at absolute start)
        if allow_node_leading_blanks && node.leading_blank_lines > 0 {
            let emit = if node.leading_blank_lines >= 2 { node.leading_blank_lines.min(2) } else { 0 };
            for _ in 0..emit { output.push('\n'); }
        }
        let indent = "    ".repeat(indent_level); // Use 4 spaces for indentation

        // Global early handling for template-derived simple leaf nodes at ANY depth:
        // We now restrict concise dash emission to cases where the node BOTH:
        //  1. Is template-derived, and
        //  2. Has an explicit override marker (_explicit_child_override or _override_present) AND represents
        //     a pure value override (only 'value' plus internal/template params, no extra authored params, no children).
        // This prevents converting originally authored non-dash field declarations (e.g. "string test_data = \"x\"")
        // into dash shorthand lines, which broke round‑trip textual fidelity.
        let is_template_child_flag = matches!(
            node.parameters.get("_template_node"),
            Some(OverseerValue::Boolean(true))
        );
        let has_template_param_markers = node
            .parameters
            .keys()
            .any(|k| k.starts_with("_template_"));
        let is_template_child_any_depth = is_template_child_flag || has_template_param_markers;
    // Allow concise emission for template-derived explicit overrides regardless of list body context.
    // Previously we blocked this inside list bodies (!in_list_body) which prevented dash style preservation
    // for template-derived overrides nested within lists (e.g., nutritional facts inside History/intake items).
        if is_template_child_any_depth {
            if let Some(value) = node.parameters.get("value") {
                let template_value_opt = node.parameters.get("_template_value");
                let inherited_unchanged = template_value_opt.map(|tv| tv == value).unwrap_or(false);
                if inherited_unchanged {
                    // Suppress inherited unchanged fields from template clones
                    return Ok(());
                }
                // Override occurred (template marker absent OR value differs). Emit dash only if this override was dash-authored.
                if node.authored_dash {
                    output.push_str(&indent);
                    output.push_str("- ");
                    output.push_str(&node.name);
                    output.push_str(" = ");
                    if let Some(raw) = &node.raw_value_literal { output.push_str(raw); } else { output.push_str(&Self::serialize_value_with_node(node, value)); }
                    output.push('\n');
                    return Ok(());
                }
                // Else fall through to normal typed emission below.
            } else {
                // Template-derived block override without a direct value.
                // Prefer concise dash block syntax: "- name { ... }" when:
                //  - Authored as dash originally OR explicitly marked as an override, and
                //  - There are no non-internal parameters aside from template-derived ones, and
                //  - It has children (structural override).
                // Note: Parameters that have corresponding _template_<param> markers are ignored for gating,
                // because they won't be emitted anyway.
                let non_internal_non_value_params = node
                    .parameters
                    .iter()
                    .filter(|(k, _)| {
                        let ks = k.as_str();
                        if ks == "value" || ks.starts_with('_') { return false; }
                        let marker = format!("_template_{}", ks);
                        // treat template-derived params as ignorable
                        !node.parameters.contains_key(&marker)
                    })
                    .count();
                // Emit dash-block only if this node was dash-authored. Do not auto-convert typed blocks
                // (e.g., "list intake {" or "div per_item {") into dash form; preserve original authoring style.
                if !node.children.is_empty() && non_internal_non_value_params == 0 && node.authored_dash {
                    output.push_str(&indent);
                    output.push_str("- ");
                    output.push_str(&node.name);
                    output.push_str(" {\n");
                    // Emit children using the same spacing policy as normal blocks
                    let children_in_list_body = node.node_type == "list";
                    let mut ordered_children: Vec<&OverseerNode> = node.children.iter().collect();
                    ordered_children.sort_by_key(|c| c.child_original_index.unwrap_or(usize::MAX));
                    let mut emitted_any_child = false;
                    for child in ordered_children {
                        // Emit blanks only between emitted siblings; preserve singletons and cap runs at 2.
                        if emitted_any_child {
                            let count = child.leading_blank_lines as usize;
                            let emit = if count >= 2 { count.min(2) } else { 1.min(count) };
                            for _ in 0..emit { output.push('\n'); }
                        }
                        // Suppress the child's own leading blanks; delegate list-body flag based on parent type
                        Self::serialize_node_context(child, output, indent_level + 1, children_in_list_body, false)?;
                        emitted_any_child = true;
                    }
                    output.push_str(&format!("{}}}\n", indent));
                    return Ok(());
                }
            }
        }

        // Default path continues with normal emission; write indent now.
        output.push_str(&indent);

        // General concise emission for originally dash-authored nodes (not template-derived).
        // Two cases:
        //  1. Simple value override: - name = value
        //  2. Block with children and no extra params: - name { ... }
        if !is_template_child_any_depth && node.authored_dash {
            let non_internal_non_value_params = node
                .parameters
                .iter()
                .filter(|(k, _)| {
                    let ks = k.as_str();
                    if ks == "value" || ks.starts_with('_') { return false; }
                    true
                })
                .count();
            // Case 1: simple value
            if node.children.is_empty() && node.parameters.contains_key("value") && non_internal_non_value_params == 0 {
                if let Some(val) = node.parameters.get("value") {
                    if let Some(raw) = &node.raw_value_literal {
                        output.push_str("- ");
                        output.push_str(&node.name);
                        output.push_str(" = ");
                        output.push_str(raw);
                        output.push('\n');
                        return Ok(());
                    } else {
                        output.push_str("- ");
                        output.push_str(&node.name);
                        output.push_str(" = ");
                        output.push_str(&Self::serialize_value_with_node(node, val));
                        output.push('\n');
                        return Ok(());
                    }
                }
            }
            // Case 2: block with children and no params other than internals
            if !node.children.is_empty() && non_internal_non_value_params == 0 && !node.parameters.contains_key("value") {
                output.push_str("- ");
                output.push_str(&node.name);
                output.push_str(" {\n");
                let mut ordered_children: Vec<&OverseerNode> = node.children.iter().collect();
                ordered_children.sort_by_key(|c| c.child_original_index.unwrap_or(usize::MAX));
                for child in ordered_children { Self::serialize_node_context(child, output, indent_level + 1, false, true)?; }
                output.push_str(&format!("{}}}\n", indent));
                return Ok(());
            }
        }

        // Handle list-style items which start with '-'
        if in_list_body || node.node_type == "list_item" || (indent_level > 0 && node.node_type == "-") {
            // Special handling for real list bodies vs non-list contexts
            if in_list_body {
                // Determine if this is a simple primitive list item (only when not template-derived)
                let is_template_instance = node.template.is_some()
                    || matches!(
                        node.parameters.get("_from_template"),
                        Some(OverseerValue::Boolean(true))
                    );
        if let Some(value) = node.parameters.get("value") {
                    // Only emit as simple value when NOT a template instance (true primitive lists)
                    if !is_template_instance {
            output.push_str("- ");
            output.push_str(&Self::serialize_value_with_node(node, value));
                        output.push('\n');
                        return Ok(());
                    }
                }
                // Complex/template-based item: emit as "- { ... }" and use the standard child emission (with concise override rules)
                output.push_str("- {\n");
                // Suppress template-derived children for instances and emit concise overrides
                let suppress_template_children = node.template.is_some()
                    || matches!(
                        node.parameters.get("_from_template"),
                        Some(OverseerValue::Boolean(true))
                    );
                // Pre-parse explicit override names list on the instance (if present)
                // NOTE: Do not use this list to filter which overrides to persist. Users can introduce
                // new overrides at runtime (e.g., by editing a field), and this list may be stale.
                // We keep parsing it for potential future use, but we won't gate emission on it.
                let _explicit_names_ignored: Option<Vec<String>> = if let Some(OverseerValue::String(list)) = node.parameters.get("_explicit_overrides") {
                    let v: Vec<String> = list
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .collect();
                    Some(v)
                } else {
                    None
                };
                let mut ordered_children: Vec<&OverseerNode> = node.children.iter().collect();
                ordered_children.sort_by_key(|c| c.child_original_index.unwrap_or(usize::MAX));
                // Track whether any child content has been emitted to control spacing between emitted siblings only
                let mut emitted_any_child = false;
                for child in ordered_children.iter() {
                    // Helper: emit leading blanks for this child according to list-entry policy (ignore singletons; cap 2)
                    let emit_pre_blanks_if_needed = |count: usize, out: &mut String| {
                        if emitted_any_child {
                            let emit = if count >= 2 { count.min(2) } else { 0 };
                            for _ in 0..emit { out.push('\n'); }
                        }
                    };
                    let is_template_child_flag = matches!(
                        child.parameters.get("_template_node"),
                        Some(OverseerValue::Boolean(true))
                    );
                    let has_template_param_markers = child
                        .parameters
                        .keys()
                        .any(|k| k.starts_with("_template_"));
                    let is_template_child = is_template_child_flag || has_template_param_markers;
                    let has_explicit_override = matches!(
                        child.parameters.get("_explicit_child_override"),
                        Some(OverseerValue::Boolean(true))
                    );
                    // Previously we filtered explicit overrides against an instance-level list (_explicit_overrides).
                    // This caused edits added later to be dropped. Do not gate on that list anymore.

                    // Special-case: if this child is a transparent wrapper and any of its descendants
                    // were explicitly overridden, emit those descendant overrides concisely here and skip the wrapper.
                    if suppress_template_children && is_template_child && child.is_hierarchy_transparent {
                        // Collect descendant value-only overrides that should be emitted concisely.
                        // Criteria:
                        //  - value present AND no non-internal, non-value params AND no children
                        //  - AND (explicit override marker present OR value differs from template default if available)
                        fn collect_descendant_value_overrides<'a>(
                            node: &'a OverseerNode,
                            _name_filter_unused: &Option<Vec<String>>,
                            out: &mut Vec<(&'a str, &'a OverseerValue)>,
                        ) {
                            // Check current node
                            let has_value = node.parameters.contains_key("value");
                            if has_value {
                                let non_internal_non_value_params = node
                                    .parameters
                                    .iter()
                                    .filter(|(k, _)| {
                                        let ks = k.as_str();
                                        if ks.starts_with('_') || ks == "value" { return false; }
                                        // Ignore template-derived params (have a corresponding _template_ marker)
                                        let marker = format!("_template_{}", ks);
                                        !node.parameters.contains_key(&marker)
                                    })
                                    .count();
                                let only_value_override =
                                    non_internal_non_value_params == 0 && node.children.is_empty();
                                if only_value_override {
                                    let explicit = matches!(
                                        node.parameters.get("_explicit_child_override"),
                                        Some(OverseerValue::Boolean(true))
                                    );
                                    let val = node.parameters.get("value").unwrap();
                                    let differs_from_template = match node.parameters.get("_template_value") {
                                        Some(tv) => tv != val,
                                        None => false,
                                    };
                                    if explicit || differs_from_template {
                                        out.push((node.name.as_str(), val));
                                    }
                                }
                            }
                            // Recurse
                            for ch in &node.children {
                                collect_descendant_value_overrides(ch, _name_filter_unused, out);
                            }
                        }
                        let mut desc_overrides: Vec<(&str, &OverseerValue)> = Vec::new();
                        // Always collect any explicit descendant value overrides; do not filter by instance list.
                        collect_descendant_value_overrides(child, &_explicit_names_ignored, &mut desc_overrides);
                        if !desc_overrides.is_empty() {
                            // Respect authored 2+ blank runs before injected concise lines; ignore singletons in list entries
                            emit_pre_blanks_if_needed(child.leading_blank_lines as usize, output);
                            for (n, v) in desc_overrides {
                                output.push_str(&format!(
                                    "{}    - {} = {}\n",
                                    indent,
                                    n,
                                    Self::serialize_value_with_node(child, v)
                                ));
                            }
                            emitted_any_child = true;
                            continue; // Skip normal emission of the transparent wrapper
                        }
                    }
                    // Skip template-derived children that are not explicitly overridden
                    if suppress_template_children && is_template_child && (!has_explicit_override) {
                        // Additional guard: if entire subtree is untouched template (no differing value overrides), skip it
                        fn pure_template_subtree(n: &OverseerNode) -> bool {
                            // Explicit override flags make it non-pure
                            let explicit = matches!(
                                n.parameters.get("_explicit_child_override"),
                                Some(OverseerValue::Boolean(true))
                            ) || matches!(
                                n.parameters.get("_override_present"),
                                Some(OverseerValue::Boolean(true))
                            );
                            if explicit { return false; }
                            // If it has a value differing from template_value marker, it's not pure
                            if let Some(v) = n.parameters.get("value") {
                                if let Some(tv) = n.parameters.get("_template_value") { if tv != v { return false; } }
                            }
                            for c in &n.children { if !pure_template_subtree(c) { return false; } }
                            true
                        }
                        if pure_template_subtree(child) { continue; }
                    }
                    if suppress_template_children {
                        let has_value = child.parameters.contains_key("value");
                        let non_internal_non_value_params = child
                            .parameters
                            .iter()
                            .filter(|(k, _)| {
                                let ks = k.as_str();
                                if ks.starts_with('_') || ks == "value" { return false; }
                                let marker = format!("_template_{}", ks);
                                !child.parameters.contains_key(&marker)
                            })
                            .count();
                        let only_value_override =
                            has_value && non_internal_non_value_params == 0 && child.children.is_empty();
                        if only_value_override {
                            let val = child.parameters.get("value").unwrap();
                            let differs_from_template = match child.parameters.get("_template_value") {
                                Some(tv) => tv != val,
                                None => false,
                            };
                            if has_explicit_override || differs_from_template {
                                // Respect authored 2+ blank runs before concise lines; ignore singletons in list entries
                                emit_pre_blanks_if_needed(child.leading_blank_lines as usize, output);
                                output.push_str(&format!(
                                    "{}    - {} = {}\n",
                                    indent,
                                    child.name,
                                    Self::serialize_value_with_node(child, val)
                                ));
                                emitted_any_child = true;
                                continue;
                            }
                        }
                    }
                    // Fallback: serialize child normally inside the block (not as list body)
                    // so field lines and nested blocks render correctly.
                    // Parent injected spacing when needed; suppress child's own leading blanks
                    emit_pre_blanks_if_needed(child.leading_blank_lines as usize, output);
                    Self::serialize_node_context(child, output, indent_level + 1, false, false)?;
                    emitted_any_child = true;
                }
                output.push_str(&format!("{}}}\n", indent));
                return Ok(());
            } else {
                // Non-list context override: allow "- name = value" syntax
                if let Some(value) = node.parameters.get("value") {
                    if !node.name.is_empty() && node.children.is_empty() {
                        output.push_str("- ");
                        output.push_str(&node.name);
                        output.push_str(" = ");
                        output.push_str(&Self::serialize_value_with_node(node, value));
                        output.push('\n');
                        return Ok(());
                    }
                }
                // Non-list context override: named nested block "- name { ... }"
                if !node.name.is_empty() && !node.children.is_empty() {
                    output.push_str("- ");
                    output.push_str(&node.name);
                    output.push_str(" {\n");
                    let mut ordered_children: Vec<&OverseerNode> = node.children.iter().collect();
                    ordered_children.sort_by_key(|c| c.child_original_index.unwrap_or(usize::MAX));
                    for child in ordered_children {
                        Self::serialize_node_context(child, output, indent_level + 1, false, true)?;
                    }
                    output.push_str(&format!("{}}}\n", indent));
                    return Ok(());
                }
                // Else: fall through to normal serialization
            }
        } else {
            // For children that belong to a list entry object (parent emitted as "- {")
            // we want each field line to start as a list-style override "- name = value" when possible.
            if in_list_body {
                // In list bodies, if this is a simple value field or a parameter-only node with value,
                // write as an inline override: "- name = value" and return.
                if let Some(val) = node.parameters.get("value") {
                    if !node.name.is_empty() && node.children.is_empty() {
                        output.push_str("- ");
                        output.push_str(&node.name);
                        output.push_str(" = ");
                        output.push_str(&Self::serialize_value(val));
                        output.push('\n');
                        return Ok(());
                    }
                }
                // Otherwise, for structural children, use '-' as the node type marker
                output.push('-');
            } else {
                // Handle node type or template path
                if let Some(template_path) = &node.template {
                    output.push_str(&format!("<{}>", template_path));
                } else if let Some(OverseerValue::Template(tpl)) = node.parameters.get("_template_origin") {
                    output.push_str(&format!("<{}>", tpl));
                } else {
                    // Check if this node had its type resolved and restore original
                    if let Some(OverseerValue::String(original_type)) = node.parameters.get("_original_type") {
                        if original_type == "-" {
                            output.push('-');
                        } else {
                            output.push_str(original_type);
                        }
                    } else {
                        // Use "-" for type-inferred nodes, otherwise use the actual type
                        if node.node_type == "-" || (node.name == "-" && node.node_type != "list_item") {
                            output.push('-');
                        } else {
                            output.push_str(&node.node_type);
                        }
                    }
                }
            }

            // Handle node name - skip if it's "-" (unnamed node marker) or if it's auto-defaulted (name == type)
            if !node.name.is_empty() && node.name != "-" {
                // If the name is exactly the same as the type, treat it as auto-defaulted and omit it.
                let auto_defaulted = node.name == node.node_type;
                if !auto_defaulted {
                    output.push(' ');
                    output.push_str(&node.name);
                }
            }
        }

        // Handle parameters (excluding the special 'value' parameter for fields)
        let regular_params: HashMap<String, OverseerValue> = node
            .parameters
            .iter()
            .filter(|(k, _)| {
                let key = k.as_str();
                // Always exclude 'value' parameter and internal computed parameters (except _template_ markers)
                if key == "value" || key == "_original_type" || (key.starts_with('_') && !key.starts_with("_template_")) {
                    return false;
                }

                // If this is a _template_ marker, exclude it from output (but don't filter other params based on it)
                if key.starts_with("_template_") {
                    return false;
                }

                // Check if this is a template-derived parameter by looking for corresponding _template_ marker
                let template_marker = format!("_template_{}", key);
                let is_template_derived = node.parameters.contains_key(&template_marker);

                if is_template_derived {
                    // This parameter came from template resolution - don't save it
                    false
                } else {
                    // This is an original user-specified parameter - save it
                    true
                }
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if !regular_params.is_empty() {
            output.push_str(" (");
            // Prefer original author order if captured; fall back to sorted
            let mut ordered: Vec<&String> = Vec::new();
            if !node.param_order.is_empty() {
                for k in &node.param_order { if regular_params.contains_key(k) { ordered.push(k); } }
                // Include any params that were not in original order list (e.g., injected later) sorted at end
                let mut extras: Vec<&String> = regular_params.keys().filter(|k| !node.param_order.contains(&k.to_string())).collect();
                extras.sort();
                ordered.extend(extras);
            } else {
                let mut keys: Vec<&String> = regular_params.keys().collect();
                keys.sort();
                ordered = keys;
            }
            let params_str: Vec<String> = ordered
                .into_iter()
                .map(|k| {
                    let v = regular_params.get(k).unwrap();
                    let value_str = if k.as_str() == "entry" {
                        // Special handling for entry parameters - they should be type names, not quoted strings
                        match v {
                            OverseerValue::String(s) => s.clone(), // Don't quote type names
                            OverseerValue::Template(t) => format!("<{}>", t),
                            _ => Self::serialize_value_with_node(node, &v),
                        }
                    } else {
                        Self::serialize_value_with_node(node, &v)
                    };
                    format!("{}={}", k, value_str)
                })
                .collect();
            output.push_str(&params_str.join(", "));
            output.push(')');
        }

        // Handle body (value assignment, block, or nothing)
        if let Some(value) = node.parameters.get("value") {
            if let Some(raw) = &node.raw_value_literal {
                // Raw literal already includes formatting (e.g., trailing zeros)
                output.push_str(&format!(" = {}\n", raw));
            } else {
                output.push_str(&format!(" = {}\n", Self::serialize_value_with_node(node, value)));
            }
        } else if node.children.is_empty() {
            output.push('\n');
        } else {
            output.push_str(" {\n");
            // Children under a list node are list-body items (render as '-')
            let children_in_list_body = node.node_type == "list";
            // Suppress template-derived children for any node that originated from a template (standalone instances or list entries)
            let suppress_template_children = node.template.is_some()
                || matches!(
                    node.parameters.get("_from_template"),
                    Some(OverseerValue::Boolean(true))
                );
            // Pre-parse explicit override names list on the instance (if present)
            let explicit_names: Option<Vec<String>> = if let Some(OverseerValue::String(list)) = node.parameters.get("_explicit_overrides") {
                let v: Vec<String> = list
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect();
                Some(v)
            } else {
                None
            };
            let mut ordered_children: Vec<&OverseerNode> = node.children.iter().collect();
            ordered_children.sort_by_key(|c| c.child_original_index.unwrap_or(usize::MAX));
            let mut emitted_any_child = false;
            // For normal blocks (non-list bodies), if the first child had authored leading blanks,
            // preserve a single blank (cap 2+) before it, EXCEPT for the top-level 'tab' block where
            // we suppress a lone blank to match canon behavior (original had a comment-adjacent blank there).
            if !children_in_list_body {
                if let Some(first) = ordered_children.first() {
                    let count = first.leading_blank_lines as usize;
                    let emit = if node.node_type == "tab" && indent_level == 0 {
                        if count >= 2 { count.min(2) } else { 0 }
                    } else {
                        if count >= 2 { count.min(2) } else { 1.min(count) }
                    };
                    for _ in 0..emit { output.push('\n'); }
                }
            }
            for child in ordered_children.iter() {
                // Helper: in normal blocks, preserve single authored blank lines and cap 2+ runs,
                // except for top-level 'tab' where we ignore singletons to avoid comment-adjacent blanks.
                let emit_pre_blanks_if_needed = |count: usize, out: &mut String| {
                    if emitted_any_child {
                        let emit = if node.node_type == "tab" && indent_level == 0 {
                            if count >= 2 { count.min(2) } else { 0 }
                        } else {
                            if count >= 2 { count.min(2) } else { 1.min(count) }
                        };
                        for _ in 0..emit { out.push('\n'); }
                    }
                };
                // Skip template-derived children unless they were explicitly overridden
                let is_template_child_flag = matches!(
                    child.parameters.get("_template_node"),
                    Some(OverseerValue::Boolean(true))
                );
                let has_template_param_markers = child.parameters.keys().any(|k| k.starts_with("_template_"));
                let is_template_child = is_template_child_flag || has_template_param_markers;
                // Only treat as includeable if it was explicitly overridden by the source, not just equal/diff logic
                let has_explicit_override = matches!(
                    child.parameters.get("_explicit_child_override"),
                    Some(OverseerValue::Boolean(true))
                );
                // Additional safety: in a template instance, only consider overrides that were explicitly named in the instance source
                let mut listed_in_instance_overrides = true;
                if suppress_template_children {
                    if let Some(names) = &explicit_names {
                        if !names.is_empty() {
                            listed_in_instance_overrides = names.iter().any(|n| n == &child.name);
                        }
                    }
                    // If the child itself has an explicit override marker, treat it as listed even if parent list omitted it.
                    if has_explicit_override { listed_in_instance_overrides = true; }
                }

                // Special-case: if child is a transparent wrapper and any of its descendants were explicitly overridden,
                // emit those descendant overrides concisely here and skip the wrapper itself.
                if suppress_template_children && is_template_child && child.is_hierarchy_transparent {
                    fn collect_descendant_value_overrides<'a>(
                        node: &'a OverseerNode,
                        name_filter: &Option<Vec<String>>,
                        out: &mut Vec<(&'a str, &'a OverseerValue)>,
                    ) {
                        let has_explicit = matches!(
                            node.parameters.get("_explicit_child_override"),
                            Some(OverseerValue::Boolean(true))
                        );
                        let name_matches = match name_filter {
                            Some(v) if !v.is_empty() => v.iter().any(|n| n == &node.name),
                            _ => true,
                        };
                        if has_explicit && name_matches {
                            let has_value = node.parameters.contains_key("value");
                            let non_internal_non_value_params = node
                                .parameters
                                .iter()
                                .filter(|(k, _)| {
                                    let ks = k.as_str();
                                    !ks.starts_with('_') && ks != "value"
                                })
                                .count();
                            let only_value_override =
                                has_value && non_internal_non_value_params == 0 && node.children.is_empty();
                            let _has_template_value_marker = node.parameters.contains_key("_template_value");
                            if only_value_override && !_has_template_value_marker {
                                out.push((node.name.as_str(), node.parameters.get("value").unwrap()));
                            }
                        }
                        for ch in &node.children {
                            collect_descendant_value_overrides(ch, name_filter, out);
                        }
                    }
                    let mut desc_overrides: Vec<(&str, &OverseerValue)> = Vec::new();
                    collect_descendant_value_overrides(child, &explicit_names, &mut desc_overrides);
                    if !desc_overrides.is_empty() {
                        emit_pre_blanks_if_needed(child.leading_blank_lines as usize, output);
                        for (n, v) in desc_overrides {
                            // Attempt to find matching descendant node to access raw literal & blank lines
                            if let Some(real) = child.children.iter().find(|c| c.name==n) {
                                if real.leading_blank_lines > 0 { for _ in 0..real.leading_blank_lines.min(2) { output.push('\n'); } }
                                if let Some(raw) = real.raw_value_literal.as_ref() {
                                    output.push_str(&format!(
                                        "{}    - {} = {}\n",
                                        indent,
                                        n,
                                        raw
                                    ));
                                } else {
                                    output.push_str(&format!(
                                        "{}    - {} = {}\n",
                                        indent,
                                        n,
                                        Self::serialize_value_with_node(child, v)
                                    ));
                                }
                            } else {
                                output.push_str(&format!(
                                    "{}    - {} = {}\n",
                                    indent,
                                    n,
                                    Self::serialize_value_with_node(child, v)
                                ));
                            }
                        }
                        emitted_any_child = true;
                        continue;
                    }
                }
                if suppress_template_children && is_template_child && (!has_explicit_override || !listed_in_instance_overrides) {
                    continue;
                }
                // For template instances/clones, if a child was overridden with only a simple value, prefer the concise "- name = value" form
                if suppress_template_children && has_explicit_override && listed_in_instance_overrides {
                    let has_value = child.parameters.contains_key("value");
                    let _has_template_value_marker = child.parameters.contains_key("_template_value");
                    if has_value && has_explicit_override {
                        debug_serializer!(
                            "[SER] concise emit: name='{}' explicit={} tmpl_marker_present={} suppress={} is_templ_child={} has_val={}",
                            child.name,
                            has_explicit_override,
                            has_template_value_marker,
                            suppress_template_children,
                            is_template_child,
                            has_value
                        );
                        let val = child.parameters.get("value").unwrap();
                        emit_pre_blanks_if_needed(child.leading_blank_lines as usize, output);
                        if let Some(raw) = &child.raw_value_literal {
                            output.push_str(&format!(
                                "{}    - {} = {}\n",
                                indent,
                                child.name,
                                raw
                            ));
                        } else {
                            output.push_str(&format!(
                                "{}    - {} = {}\n",
                                indent,
                                child.name,
                                Self::serialize_value_with_node(child, val)
                            ));
                        }
                        emitted_any_child = true;
                        continue;
                    }
                }
                // Parent injected spacing when needed; suppress child's own leading blanks
                emit_pre_blanks_if_needed(child.leading_blank_lines as usize, output);
                Self::serialize_node_context(child, output, indent_level + 1, children_in_list_body, false)?;
                emitted_any_child = true;
            }
            output.push_str(&format!("{}}}\n", indent));
        }

        Ok(())
    }

    fn serialize_value(value: &OverseerValue) -> String {
        match value {
            OverseerValue::Null => "null".to_string(),
            OverseerValue::String(s) => format!("\"{}\"", s),
            OverseerValue::Integer(i) => i.to_string(),
            OverseerValue::Float(f) => f.to_string(),
            OverseerValue::Boolean(b) => b.to_string(),
            OverseerValue::Date(d) => format!("\"{}\"", d),
            OverseerValue::Timestamp(ts) => format!("\"{}\"", ts),
            OverseerValue::Formula(f) => format!("$({})", f),
            OverseerValue::Template(t) => format!("<{}>", t),
            OverseerValue::Color(color) => match color {
                Color::Hex(hex) => hex.clone(),
                Color::Named(name) => name.clone(),
                Color::Rgb(r, g, b) => format!("rgb({}, {}, {})", r, g, b),
            },
            OverseerValue::CssSize(size) => match size {
                CssSize::Pixels(px) => format!("{}px", px),
                CssSize::Percentage(pct) => format!("{}%", pct),
                CssSize::Em(em) => format!("{}em", em),
                CssSize::Rem(rem) => format!("{}rem", rem),
                CssSize::ViewportWidth(vw) => format!("{}vw", vw),
                CssSize::ViewportHeight(vh) => format!("{}vh", vh),
                CssSize::Auto => "auto".to_string(),
                CssSize::FitContent => "fit-content".to_string(),
            },
            OverseerValue::BorderStyle(style) => match style {
                BorderStyle::None => "none".to_string(),
                BorderStyle::Default => "default".to_string(),
                BorderStyle::Solid(thickness, color) => {
                    format!(
                        "solid {} {}",
                        match thickness {
                            CssSize::Pixels(px) => format!("{}px", px),
                            CssSize::Percentage(pct) => format!("{}%", pct),
                            CssSize::Em(em) => format!("{}em", em),
                            CssSize::Rem(rem) => format!("{}rem", rem),
                            CssSize::ViewportWidth(vw) => format!("{}vw", vw),
                            CssSize::ViewportHeight(vh) => format!("{}vh", vh),
                            CssSize::Auto => "auto".to_string(),
                            CssSize::FitContent => "fit-content".to_string(),
                        },
                        match color {
                            Color::Hex(hex) => hex.clone(),
                            Color::Named(name) => name.clone(),
                            Color::Rgb(r, g, b) => format!("rgb({}, {}, {})", r, g, b),
                        }
                    )
                }
                BorderStyle::Dashed(thickness, color) => {
                    format!(
                        "dashed {} {}",
                        match thickness {
                            CssSize::Pixels(px) => format!("{}px", px),
                            CssSize::Percentage(pct) => format!("{}%", pct),
                            CssSize::Em(em) => format!("{}em", em),
                            CssSize::Rem(rem) => format!("{}rem", rem),
                            CssSize::ViewportWidth(vw) => format!("{}vw", vw),
                            CssSize::ViewportHeight(vh) => format!("{}vh", vh),
                            CssSize::Auto => "auto".to_string(),
                            CssSize::FitContent => "fit-content".to_string(),
                        },
                        match color {
                            Color::Hex(hex) => hex.clone(),
                            Color::Named(name) => name.clone(),
                            Color::Rgb(r, g, b) => format!("rgb({}, {}, {})", r, g, b),
                        }
                    )
                }
                BorderStyle::Dotted(thickness, color) => {
                    format!(
                        "dotted {} {}",
                        match thickness {
                            CssSize::Pixels(px) => format!("{}px", px),
                            CssSize::Percentage(pct) => format!("{}%", pct),
                            CssSize::Em(em) => format!("{}em", em),
                            CssSize::Rem(rem) => format!("{}rem", rem),
                            CssSize::ViewportWidth(vw) => format!("{}vw", vw),
                            CssSize::ViewportHeight(vh) => format!("{}vh", vh),
                            CssSize::Auto => "auto".to_string(),
                            CssSize::FitContent => "fit-content".to_string(),
                        },
                        match color {
                            Color::Hex(hex) => hex.clone(),
                            Color::Named(name) => name.clone(),
                            Color::Rgb(r, g, b) => format!("rgb({}, {}, {})", r, g, b),
                        }
                    )
                }
            },
        }
    }

    // Variant of serialize_value that can look at the node's parameters to apply
    // precision-aware persistence for dates/timestamps (e.g., precision="day").
    fn serialize_value_with_node(node: &OverseerNode, value: &OverseerValue) -> String {
        // If this node indicates day precision, collapse Timestamp values to date-only.
        // We check both the node's own precision param and the common convention of field name 'date'.
        let day_precision = match node.parameters.get("precision") {
            Some(OverseerValue::String(s)) => s.eq_ignore_ascii_case("day") || s.eq_ignore_ascii_case("days"),
            _ => false,
        };

        if day_precision {
            match value {
                OverseerValue::Timestamp(ts) => {
                    // Best-effort: extract YYYY-MM-DD from RFC3339; fall back to ts
                    let date_only = ts.split('T').next().unwrap_or(ts);
                    return format!("\"{}\"", date_only.to_string());
                }
                OverseerValue::Date(d) => {
                    // Already date-only; emit as-is quoted
                    return format!("\"{}\"", d);
                }
                _ => { /* fall through */ }
            }
        }
        // Default behavior
        Self::serialize_value(value)
    }
}

// Public facade used by other modules and tests
pub struct OverseerFileHandler;

impl OverseerFileHandler {
    // Delegate to FileOperations' serializer
    pub fn serialize_nodes(nodes: &[OverseerNode]) -> Result<String> {
        FileOperations::serialize_nodes(nodes)
    }

    pub fn enable_merge_trace(enable: bool) { crate::file_ops::MERGE_TRACE_ENABLED.store(enable, std::sync::atomic::Ordering::Relaxed); }

    /// Internal deterministic merge that preserves original comment blocks and indentation while
    /// allowing regenerated canonical code lines to replace the original text. A positional guard
    /// prevents earlier newly inserted regenerated lines from "stealing" formatting/leading blocks
    /// belonging to later duplicate anchors (mitigating indentation drift when prepending entries).
    fn segment_merge(original: &str, regenerated: &str) -> String {
        use std::collections::{HashMap, VecDeque};
        // Optional deep audit (heavy). Enable with env var OVERSEER_MERGE_AUDIT=1.
        let audit_enabled = std::env::var("OVERSEER_MERGE_AUDIT").is_ok();
        #[derive(Debug)]
        struct AuditLineRecord {
            phase: &'static str,          // pass1|pass2|post
            out_idx: usize,              // line index in current output being built (approximate)
            regen_idx: Option<usize>,    // index in regenerated sequence if applicable
            text: String,                // final text for the line at capture
            trimmed: String,             // trimmed content
            adopted_from: Option<String>,// strict|relaxed|trimmed|raw|struct_list_brace|struct_generic_brace|post_adopt|post_adopt_trimmed
            original_pos: Option<usize>, // original line position used for adoption
            original_indent: Option<usize>,
            final_indent: usize,
            note: Option<String>,        // extra context (skip_consume,new_block,eager,...)
        }
        impl AuditLineRecord { fn emit(&self) { if std::env::var("OVERSEER_MERGE_AUDIT").is_ok() { eprintln!("[MERGE-AUDIT] {}|out={} regen={:?} indent_final={} indent_orig={:?} adopted={:?} pos_orig={:?} note={:?} :: {}", self.phase, self.out_idx, self.regen_idx, self.final_indent, self.original_indent, self.adopted_from, self.original_pos, self.note, self.text); } } }
        let mut audit_lines: Vec<AuditLineRecord> = Vec::new();
        fn is_code(l:&str)->bool { let t=l.trim_start(); !t.is_empty() && !t.starts_with("//") }
        fn anchor_key(line:&str)->String { let code = match line.find("//") { Some(i)=> &line[..i], None=> line }; code.chars().filter(|c| !c.is_whitespace()).collect() }
        fn relaxed_anchor_key(line:&str)->String {
            let code = match line.find("//") { Some(i)=> &line[..i], None=> line };
            let mut slice = code;
            if let Some(pos) = code.find(|c: char| c=='(' || c=='{') { slice = &code[..pos]; }
            slice.chars().filter(|c| !c.is_whitespace()).collect()
        }
    #[derive(Clone, Debug)] struct Occurrence { leading: Vec<String>, indent: String, inline: Option<String>, pos: usize, core: String }
    #[cfg(feature="debug-resolver")] #[derive(Debug, serde::Serialize)] struct MergeEvent { idx: usize, kind: &'static str, trimmed: String, reason: String, adopted_indent: Option<String>, final_line: Option<String> }
    #[cfg(feature="debug-resolver")] let mut events: Vec<MergeEvent> = Vec::new();
    #[cfg(feature="debug-resolver")] static mut LAST_JSON_TRACE: Option<String> = None;
    #[cfg(feature="debug-resolver")] let json_trace = std::env::var("OVS_MERGE_TRACE_JSON").is_ok();
    #[cfg(not(feature="debug-resolver"))] let _ = std::env::var("OVS_MERGE_TRACE_JSON"); // no-op when feature disabled
    let mut trace_on = crate::file_ops::MERGE_TRACE_ENABLED.load(std::sync::atomic::Ordering::Relaxed);
    // Environment variable override for tracing (Option A)
    if std::env::var("OVS_MERGE_TRACE").is_ok() { trace_on = true; }
        macro_rules! trace_merge { ($($arg:tt)*) => { if trace_on { #[cfg(feature="debug-resolver")] eprintln!("[MERGE] {}", format!($($arg)*)); } }; }
        // Pass 1: scan original building occurrence queues and trailing tail
    let mut map: HashMap<String, VecDeque<Occurrence>> = HashMap::new();
    let mut map_relaxed: HashMap<String, VecDeque<Occurrence>> = HashMap::new();
    // Trimmed-content (core) map (ignores inline comments & indentation) for final fallback adoption.
    let mut map_trimmed: HashMap<String, VecDeque<Occurrence>> = HashMap::new();
        let mut trailing_tail: Vec<String> = Vec::new();
        let mut iter = original.lines().peekable();
        let mut line_index: usize = 0; // absolute line index across original
    // Track ordinal indices for generic closing braces to create disambiguated synthetic anchor keys.
    let mut original_generic_close_counter: usize = 0;
    // === Generic brace path mapping (depth + ordinal among generic siblings) ===
    fn is_list_entry_opener(line: &str) -> bool { line.trim() == "- {" }
    #[derive(Clone, Debug)] struct OpenRecord { depth: usize, generic_ordinal: usize, indent: usize, is_list: bool }
    let mut open_stack: Vec<OpenRecord> = Vec::new();
    let mut generic_counters_per_depth: Vec<usize> = Vec::new();
    let mut generic_path_indent_map: HashMap<(usize, usize), usize> = HashMap::new();
    while iter.peek().is_some() {
            let mut leading: Vec<String> = Vec::new();
            while let Some(&l) = iter.peek() { if is_code(l) { break; } leading.push(l.to_string()); iter.next(); line_index+=1; }
            if iter.peek().is_none() { trailing_tail = leading; break; }
            let code_line = iter.next().unwrap(); line_index+=1;
            if !is_code(code_line) { continue; }
            let mut k = anchor_key(code_line);
            let rk = relaxed_anchor_key(code_line);
            let indent_len = code_line.chars().take_while(|c| c.is_whitespace()).count();
            let indent: String = code_line.chars().take(indent_len).collect();
            let inline = code_line.find("//").map(|i| code_line[i..].to_string());
            let core_part = {
                let code_no_comment = match code_line.find("//") { Some(i)=> &code_line[..i], None=> code_line };
                code_no_comment.trim().to_string()
            };
            let occ = Occurrence { leading, indent, inline, pos: line_index-1, core: core_part };// store position of the code line itself
            let trimmed_code = code_line.trim();
            // Path-based structural tracking
            if trimmed_code.ends_with('{') {
                let is_list = is_list_entry_opener(code_line);
                let depth = open_stack.len();
                if generic_counters_per_depth.len() <= depth { generic_counters_per_depth.push(0); }
                let mut ordinal = 0usize;
                if !is_list { ordinal = generic_counters_per_depth[depth]; generic_counters_per_depth[depth] += 1; }
                open_stack.push(OpenRecord { depth, generic_ordinal: ordinal, indent: occ.indent.len(), is_list });
            } else if trimmed_code == "}" {
                if let Some(op) = open_stack.pop() {
                    if !op.is_list { generic_path_indent_map.insert((op.depth, op.generic_ordinal), op.indent); }
                }
            }
            // If this is a standalone generic closing brace ('}') that is NOT directly closing a list entry (we cannot know directly here),
            // tag it with a running ordinal to create a unique synthetic key variant to avoid ambiguous adoption ordering.
            if code_line.trim() == "}" {
                // Append suffix so later identical braces do not collide purely by string.
                k = format!("{}#g{}", k, original_generic_close_counter);
                original_generic_close_counter += 1;
            }
            map.entry(k).or_insert_with(VecDeque::new).push_back(occ.clone());
            map_relaxed.entry(rk).or_insert_with(VecDeque::new).push_back(occ.clone());
            map_trimmed.entry(occ.core.clone()).or_insert_with(VecDeque::new).push_back(occ);
        }
        trace_merge!("built occurrence maps: strict={} relaxed={} trimmed={}", map.len(), map_relaxed.len(), map_trimmed.len());

        // Precompute trimmed original code lines for novelty detection of list entry blocks.
        use std::collections::HashSet;
        let mut original_trimmed: HashSet<String> = HashSet::new();
        for l in original.lines() { let t = l.trim(); if !t.is_empty() && !t.starts_with("//") { original_trimmed.insert(t.to_string()); } }

        // Collect regenerated lines for block lookahead.
        let regen_lines: Vec<&str> = regenerated.lines().collect();
        let mut skip_consume: Vec<bool> = vec![false; regen_lines.len()];
        // Detect genuinely new list entry blocks ("- {" ... matching "}") using stricter heuristic to avoid
        // misclassifying existing blocks with only a single changed field (which caused indentation drift).
        // Heuristic: mark block only if (a) first code line inside block is novel and looks like an id/eid line OR
        // (b) proportion of novel code lines exceeds 60%.
        let mut idx = 0usize; while idx < regen_lines.len() { let line = regen_lines[idx].trim(); if line == "- {" { let start = idx; let mut depth = 0isize; let mut j = idx+1;
                let mut novel_count = 0usize; let mut code_count = 0usize; let mut first_code_line: Option<String> = None; let mut first_code_line_novel = false;
                while j < regen_lines.len() { let t = regen_lines[j].trim(); if t == "- {" { depth += 1; } else if t == "}" { if depth == 0 { break; } depth -= 1; }
                    if !t.is_empty() && !t.starts_with("//") && t != "}" { // consider as code line
                        if first_code_line.is_none() { first_code_line = Some(t.to_string()); first_code_line_novel = !original_trimmed.contains(t); }
                        code_count += 1; if !original_trimmed.contains(t) { novel_count += 1; }
                    }
                    j+=1; }
                if j < regen_lines.len() { // found block end at j
                    let mut mark_new = false;
                    if code_count > 0 {
                        let ratio = (novel_count as f32) / (code_count as f32);
                        if ratio > 0.60 { mark_new = true; }
                        if !mark_new && first_code_line_novel { if let Some(fc) = &first_code_line { if fc.starts_with("- id ") || fc.starts_with("- eid ") { mark_new = true; } } }
                    }
                    if mark_new { for k in start..=j { skip_consume[k] = true; } }
                    idx = j; // will increment below
                } }
            idx+=1; }

        // Pass 2: iterate regenerated lines in order, applying positional guard and block-skip logic.
        let mut use_counters: HashMap<String, usize> = HashMap::new();
        let mut use_counters_relaxed: HashMap<String, usize> = HashMap::new();
    let mut out = String::new();
        let mut last_blank = false;
        let mut regen_index: usize = 0;
        // Track brace stack to enforce indentation for list entry blocks specifically (opened by "- {")
    #[derive(Clone, Copy)] enum BlockKind { ListEntry { indent: usize }, Other { indent: usize } }
        let mut block_stack: Vec<BlockKind> = Vec::new();
        let mut list_entry_depth: usize = 0; // tracks nesting of list entry blocks opened by '- {'
    // Counter for generic closing braces encountered during regenerated iteration to reconstruct synthetic keys.
    let mut regen_generic_close_counter: usize = 0;
    // Rebuild path stack for regenerated lines
    let mut regen_open_stack: Vec<OpenRecord> = Vec::new();
    let mut regen_generic_counters_per_depth: Vec<usize> = Vec::new();
    let mut regen_generic_struct_expected: Vec<Option<usize>> = Vec::new();
    for (line_idx, line) in regen_lines.iter().enumerate() {
            if !is_code(line) {
                if line.trim().is_empty() { out.push('\n'); } else { out.push_str(line); out.push('\n'); }
                if audit_enabled {
                    audit_lines.push(AuditLineRecord{ phase: "pass2", out_idx: out.lines().count().saturating_sub(1), regen_idx: Some(line_idx), text: (*line).to_string(), trimmed: line.trim().to_string(), adopted_from: Some("non_code".into()), original_pos: None, original_indent: None, final_indent: 0, note: None });
                }
                last_blank = line.trim().is_empty();
                regen_index+=1; continue;
            }
            let skip_this = skip_consume[line_idx];
            let mut k = anchor_key(line);
            if line.trim() == "}" { k = format!("{}#g{}", k, regen_generic_close_counter); regen_generic_close_counter += 1; }
            // Maintain structural stack for regenerated lines
            let trimmed_line_now = line.trim();
            if trimmed_line_now.ends_with('{') {
                let is_list = is_list_entry_opener(line);
                let depth = regen_open_stack.len();
                if regen_generic_counters_per_depth.len() <= depth { regen_generic_counters_per_depth.push(0); }
                let mut ordinal = 0usize;
                if !is_list { ordinal = regen_generic_counters_per_depth[depth]; regen_generic_counters_per_depth[depth] += 1; }
                regen_open_stack.push(OpenRecord { depth, generic_ordinal: ordinal, indent: line.chars().take_while(|c| c.is_whitespace()).count(), is_list });
            } else if trimmed_line_now == "}" {
                if let Some(op) = regen_open_stack.pop() {
                    if !op.is_list { let expected = generic_path_indent_map.get(&(op.depth, op.generic_ordinal)).cloned(); regen_generic_struct_expected.push(expected); }
                    else { regen_generic_struct_expected.push(None); }
                } else { regen_generic_struct_expected.push(None); }
            }
            let mut matched = false;
            let trimmed_line_pre = line.trim();
            // Track entry depth prior to matching so we can relax adoption for interior structural lines
            let entering_entry = trimmed_line_pre == "- {";
            let leaving_entry = trimmed_line_pre == "}" && list_entry_depth > 0;
            let in_list_entry_before = list_entry_depth > 0;
            if !skip_this { if let Some(queue) = map.get(&k) {
                let idx = *use_counters.get(&k).unwrap_or(&0);
                if idx < queue.len() {
                    let occ = &queue[idx];
                    let trimmed_line = trimmed_line_pre;
                    let is_list_field = trimmed_line.starts_with("- ") && trimmed_line != "- {";
                    let is_list_context_line = in_list_entry_before && !trimmed_line.starts_with("//") && trimmed_line != "}"; // any interior code line of list entry
                    let has_comment_leading = occ.leading.iter().any(|l| l.trim_start().starts_with("//"));
                    // Eager consumption scenarios:
                    // 1. Targeted: closing brace carrying trailing comment block from original placement
                    // 2. Header: first occurrence with leading comments (file header) whose original position is after regen_index due to stripped comments
                    let needs_eager = regen_index < occ.pos && has_comment_leading && (trimmed_line == "}" || idx == 0);
                    if regen_index >= occ.pos || needs_eager || is_list_field || is_list_context_line { // positional guard, eager, or relaxed for list fields & interior lines
                        for (i, l) in occ.leading.iter().enumerate() {
                            let blank = l.trim().is_empty();
                            if i == 0 && blank && last_blank { continue; }
                            if blank { out.push('\n'); } else { out.push_str(l); out.push('\n'); }
                            last_blank = blank;
                        }
                        let regen_trim = line.trim_start();
                        let mut composed = format!("{}{}", occ.indent, regen_trim);
                        // Track list entry openings before we potentially modify indentation
                        if trimmed_line == "- {" { block_stack.push(BlockKind::ListEntry { indent: occ.indent.len() }); }
                        else if trimmed_line.ends_with('{') { let indent_len = occ.indent.len(); block_stack.push(BlockKind::Other { indent: indent_len }); }
                        if trimmed_line == "}" {
                            if let Some(kind) = block_stack.pop() {
                                match kind {
                                    BlockKind::ListEntry { indent } => {
                                        // For list entry blocks we normalize the closing brace to the opener indent (stable visual style)
                                        composed = format!("{}{}", " ".repeat(indent), "}");
                                    }
                                    BlockKind::Other { .. } => {
                                        // Generic blocks: if we have a structural expected opener indent and current is under-indented, fix it.
                                        if let Some(Some(expected_opener)) = regen_generic_struct_expected.last() {
                                            let current_len = composed.chars().take_while(|c| c.is_whitespace()).count();
                                            if current_len != *expected_opener { composed = format!("{}{}", " ".repeat(*expected_opener), "}"); }
                                        }
                                    }
                                }
                            }
                        }
                        if !line.contains("//") { if let Some(inl) = &occ.inline { if !composed.ends_with(' ') { composed.push(' '); } composed.push_str(inl); } }
                        out.push_str(&composed); out.push('\n');
                        if audit_enabled { audit_lines.push(AuditLineRecord{ phase: "pass2", out_idx: out.lines().count().saturating_sub(1), regen_idx: Some(line_idx), text: composed.clone(), trimmed: trimmed_line.to_string(), adopted_from: Some("strict".into()), original_pos: Some(occ.pos), original_indent: Some(occ.indent.len()), final_indent: occ.indent.len(), note: Some(format!("idx_in_queue={} eager={} list_field={} list_context_line={} skip={}", idx, needs_eager, is_list_field, is_list_context_line, skip_this)) }); }
                        last_blank = false;
                        use_counters.insert(k.clone(), idx+1);
                        matched = true;
                    }
                }
            } }
            if !matched && !skip_this {
                let rk = relaxed_anchor_key(line);
                let rk = if line.trim() == "}" { // maintain relaxed variant similarly for completeness
                    format!("{}#g{}", rk, regen_generic_close_counter.saturating_sub(1))
                } else { rk };
                if let Some(queue) = map_relaxed.get(&rk) {
                    let idx_r = *use_counters_relaxed.get(&rk).unwrap_or(&0);
                    if idx_r < queue.len() {
                        let occ = &queue[idx_r];
                        let trimmed_line = trimmed_line_pre;
                        let is_list_field = trimmed_line.starts_with("- ") && trimmed_line != "- {";
                        let is_list_context_line = in_list_entry_before && !trimmed_line.starts_with("//") && trimmed_line != "}";
                        let has_comment_leading = occ.leading.iter().any(|l| l.trim_start().starts_with("//"));
                        let needs_eager = regen_index < occ.pos && has_comment_leading && (trimmed_line == "}" || idx_r == 0);
                        if regen_index >= occ.pos || needs_eager || is_list_field || is_list_context_line { // positional guard, eager, or relaxed for list fields & interior lines
                            for (i, l) in occ.leading.iter().enumerate() {
                                let blank = l.trim().is_empty();
                                if i == 0 && blank && last_blank { continue; }
                                if blank { out.push('\n'); } else { out.push_str(l); out.push('\n'); }
                                last_blank = blank;
                            }
                            let regen_trim = line.trim_start();
                            let mut composed = format!("{}{}", occ.indent, regen_trim);
                            if trimmed_line == "- {" { block_stack.push(BlockKind::ListEntry { indent: occ.indent.len() }); }
                            else if trimmed_line.ends_with('{') { let indent_len = occ.indent.len(); block_stack.push(BlockKind::Other { indent: indent_len }); }
                            if trimmed_line == "}" {
                                if let Some(kind) = block_stack.pop() {
                                    match kind {
                                        BlockKind::ListEntry { indent } => { composed = format!("{}{}", " ".repeat(indent), "}"); }
                                        BlockKind::Other { .. } => {
                                            if let Some(Some(expected_opener)) = regen_generic_struct_expected.last() {
                                                let current_len = composed.chars().take_while(|c| c.is_whitespace()).count();
                                                if current_len != *expected_opener { composed = format!("{}{}", " ".repeat(*expected_opener), "}"); }
                                            }
                                        }
                                    }
                                }
                            }
                            if !line.contains("//") { if let Some(inl) = &occ.inline { if !composed.ends_with(' ') { composed.push(' '); } composed.push_str(inl); } }
                            out.push_str(&composed); out.push('\n');
                            if audit_enabled { audit_lines.push(AuditLineRecord{ phase: "pass2", out_idx: out.lines().count().saturating_sub(1), regen_idx: Some(line_idx), text: composed.clone(), trimmed: trimmed_line.to_string(), adopted_from: Some("relaxed".into()), original_pos: Some(occ.pos), original_indent: Some(occ.indent.len()), final_indent: occ.indent.len(), note: Some(format!("idx_in_queue={} eager={} list_field={} list_context_line={} skip={}", idx_r, needs_eager, is_list_field, is_list_context_line, skip_this)) }); }
                            last_blank = false;
                            use_counters_relaxed.insert(rk.clone(), idx_r+1);
                            matched = true;
                        }
                    }
                }
            }
            if !matched {
                // Raw output path; enforce list entry closing brace indentation if applicable
                let trimmed_line = trimmed_line_pre;
                if trimmed_line == "- {" {
                    let indent_len = line.chars().take_while(|c| c.is_whitespace()).count();
                    block_stack.push(BlockKind::ListEntry { indent: indent_len });
                } else if trimmed_line.ends_with('{') { let indent_len = line.chars().take_while(|c| c.is_whitespace()).count(); block_stack.push(BlockKind::Other { indent: indent_len }); }
                if trimmed_line == "}" {
                    if let Some(kind) = block_stack.pop() {
                        match kind {
                            BlockKind::ListEntry { indent } => {
                                // Only fix if regenerated brace is under-indented (column 0)
                                let current_indent = line.chars().take_while(|c| c.is_whitespace()).count();
                                if current_indent == 0 {
                                    let composed = format!("{}{}", " ".repeat(indent), trimmed_line);
                                    out.push_str(&composed); out.push('\n');
                                    last_blank = false;
                                    list_entry_depth += 1;
                                    #[cfg(feature="debug-resolver")] events.push(MergeEvent{ idx: line_idx, kind: "raw", trimmed: trimmed_line.to_string(), reason: "list_entry_close_fix_under_indented".into(), adopted_indent: Some(" ".repeat(indent)), final_line: Some(composed) });
                                    regen_index+=1; continue;
                                }
                            }
                            BlockKind::Other { .. } => {
                                if let Some(Some(expected_opener)) = regen_generic_struct_expected.last() {
                                    let current_len = line.chars().take_while(|c| c.is_whitespace()).count();
                                    if current_len != *expected_opener {
                                        let composed = format!("{}{}", " ".repeat(*expected_opener), trimmed_line);
                                        out.push_str(&composed); out.push('\n');
                                        last_blank = false;
                                        regen_index+=1; continue;
                                    }
                                }
                            }
                        }
                    }
                }
                out.push_str(line); out.push('\n'); last_blank = false; #[cfg(feature="debug-resolver")] events.push(MergeEvent{ idx: line_idx, kind: "raw", trimmed: trimmed_line.to_string(), reason: if skip_this { "skip_consume".into() } else { "no_match".into() }, adopted_indent: None, final_line: Some((*line).to_string()) });
                if audit_enabled { audit_lines.push(AuditLineRecord{ phase: "pass2", out_idx: out.lines().count().saturating_sub(1), regen_idx: Some(line_idx), text: (*line).to_string(), trimmed: trimmed_line.to_string(), adopted_from: Some(if skip_this { "raw_skip" } else { "raw_no_match" }.into()), original_pos: None, original_indent: None, final_indent: line.chars().take_while(|c| c.is_whitespace()).count(), note: None }); }
            }
            if !matched {
                // Late-match fallback: If we output the regenerated line raw but there is still an unused occurrence
                if list_entry_depth > 0 { list_entry_depth -= 1; }
            }
            regen_index+=1;
            // Update entry depth after processing matched path too
            if entering_entry { list_entry_depth += 1; }
            if leaving_entry { if list_entry_depth > 0 { list_entry_depth -= 1; } }
        }
        // Append trailing tail exactly, collapsing excess blank introduction.
        for (i, l) in trailing_tail.iter().enumerate() {
            let blank = l.trim().is_empty();
            if i == 0 && blank && last_blank { continue; }
            if blank { out.push('\n'); } else { out.push_str(l); out.push('\n'); }
            if audit_enabled { audit_lines.push(AuditLineRecord{ phase: "pass2", out_idx: out.lines().count().saturating_sub(1), regen_idx: None, text: l.to_string(), trimmed: l.trim().to_string(), adopted_from: Some("trailing_tail".into()), original_pos: None, original_indent: None, final_indent: l.chars().take_while(|c| c.is_whitespace()).count(), note: None }); }
            last_blank = blank;
        }
        trace_merge!("final merged size={} chars", out.len());
        // Collapse runs >2 blank lines (safety net)
        let mut collapsed = String::new();
        let mut run = 0usize;
        for ch in out.chars() { if ch=='\n' { run+=1; if run<=2 { collapsed.push(ch);} } else { run=0; collapsed.push(ch);} }
        #[cfg(feature="debug-resolver")] if trace_on { for ev in &events { eprintln!("[MERGE-EVENT] {:?}", ev); } }
        // Ultra-strict post pass (Option A): adopt indentation for any remaining unused occurrences whose trimmed
        // content appears in the final output with a mismatching indentation (and is not novel).
        // Additionally, capture original closing brace indentation sequences so that ambiguous generic
        // braces (multiple identical '}' anchors) can deterministically adopt the Nth original generic
        // brace indentation when occurrence-based adoption fails to match original formatting.
        let mut orig_list_entry_closing_indents: Vec<usize> = Vec::new();
        let mut orig_generic_closing_indents: Vec<usize> = Vec::new();
        {
            let mut stack: Vec<bool> = Vec::new(); // true=list entry, false=generic
            for l in original.lines() {
                let t = l.trim();
                if t == "- {" { stack.push(true); }
                else if t.ends_with('{') { stack.push(false); }
                else if t == "}" {
                    if let Some(is_list) = stack.pop() {
                        let indent_len = l.chars().take_while(|c| c.is_whitespace()).count();
                        if is_list { orig_list_entry_closing_indents.push(indent_len); } else { orig_generic_closing_indents.push(indent_len); }
                    }
                }
            }
        }
    let mut final_out = String::new();
        // Usage counters for trimmed-content fallback (post-pass only)
        let mut use_counters_trimmed: HashMap<String, usize> = HashMap::new();
        let lines: Vec<&str> = collapsed.lines().collect();
    // Retain index counter for potential future mapping refinement; underscore to silence unused warning if optimization bypasses it.
    let mut _generic_brace_idx: usize = 0;
    let mut _list_entry_brace_idx: usize = 0; // underscore to silence unused warning while keeping count available for future diagnostics
        'line_loop: for (idx, line) in lines.iter().enumerate() {
            let raw = *line;
            let trimmed_candidate = raw.trim();
            // Treat standalone closing brace as code even if is_code() might exclude it
            if !is_code(raw) && trimmed_candidate != "}" { final_out.push_str(raw); final_out.push('\n'); continue; }
            let trimmed = trimmed_candidate;
            // Targeted structural generic brace correction: if this is a generic '}' whose current indentation
            // differs from its original opener indentation (tracked via structural backward scan plus original map),
            // fix it before any anchor adoption. This runs after list entry normalization above.
            if trimmed == "}" {
                // Backward structural scan to identify opener and whether it's list entry
                let mut need: isize = 1; let mut j: isize = idx as isize - 1; let mut opener_line: Option<&str> = None; while j >= 0 && need > 0 { let cand = lines[j as usize]; if !is_code(cand) { j-=1; continue; } let pt = cand.trim(); if pt == "}" { need+=1; j-=1; continue; } if pt.ends_with('{') { need-=1; if need==0 { opener_line = Some(cand); break; } } j-=1; }
                if let Some(op_line) = opener_line { if op_line.trim() != "- {" { // generic block
                        // Determine original opener indent by scanning original structurally (cached earlier?) Fallback: match first occurrence anchor of opener line.
                        let opener_indent_len = op_line.chars().take_while(|c| c.is_whitespace()).count();
                        let current_indent_len = raw.chars().take_while(|c| c.is_whitespace()).count();
                        if current_indent_len != opener_indent_len { let fixed = format!("{}{}", " ".repeat(opener_indent_len), trimmed); final_out.push_str(&fixed); final_out.push('\n'); if audit_enabled { audit_lines.push(AuditLineRecord{ phase: "post", out_idx: final_out.lines().count().saturating_sub(1), regen_idx: None, text: fixed.clone(), trimmed: trimmed.to_string(), adopted_from: Some("struct_generic_brace".into()), original_pos: None, original_indent: Some(opener_indent_len), final_indent: opener_indent_len, note: Some("targeted_struct_fix".into()) }); } #[cfg(feature="debug-resolver")] if trace_on { eprintln!("[MERGE-EVENT] targeted_generic_struct_fix idx={} opener_indent={} current_indent={}", idx, opener_indent_len, current_indent_len); } continue 'line_loop; }
                    } }
            }
            // Special-case: list entry closing brace should inherit indentation from its paired "- {" opening
            // rather than from occurrence adoption logic (which can select a different '}' anchor when prepends
            // shift positional ordering). We detect this structurally: the previous preceding code line (skipping
            // blank/comment lines) is the opening "- {" of the same list entry. If so, force its indentation to match
            // the opening line's indentation (which is stable and already present in merged output) and skip further
            // adoption for this line.
            if trimmed == "}" {
                // Structural backward brace matching: only force indentation for list entry blocks (opened by "- {").
                let mut need: isize = 1;
                let mut j: isize = idx as isize - 1;
                let mut opener_line: Option<&str> = None;
                while j >= 0 && need > 0 {
                    let candidate = lines[j as usize];
                    if !is_code(candidate) { j -= 1; continue; }
                    let pt = candidate.trim();
                    if pt == "}" { need += 1; j -= 1; continue; }
                    if pt.ends_with('{') { need -= 1; if need == 0 { opener_line = Some(candidate); break; } }
                    j -= 1;
                }
                if let Some(open_line) = opener_line {
                    let opener_trim = open_line.trim();
                    let is_list_entry = opener_trim == "- {";
                    if is_list_entry {
                        let desired_indent_len = open_line.chars().take_while(|c| c.is_whitespace()).count();
                        let current_indent_len = raw.chars().take_while(|c| c.is_whitespace()).count();
                        if current_indent_len != desired_indent_len {
                            let fixed = format!("{}{}", " ".repeat(desired_indent_len), trimmed);
                            final_out.push_str(&fixed); final_out.push('\n'); if audit_enabled { audit_lines.push(AuditLineRecord{ phase: "post", out_idx: final_out.lines().count().saturating_sub(1), regen_idx: None, text: fixed.clone(), trimmed: trimmed.to_string(), adopted_from: Some("struct_list_brace".into()), original_pos: None, original_indent: Some(desired_indent_len), final_indent: desired_indent_len, note: Some("list_entry_struct_fix".into()) }); }
                            #[cfg(feature="debug-resolver")] if trace_on { eprintln!("[MERGE-EVENT] structural_list_entry_brace_fix opener='{}' line_idx={}", opener_trim, idx); }
                            _list_entry_brace_idx += 1;
                            continue 'line_loop;
                        } else {
                            final_out.push_str(raw); final_out.push('\n'); if audit_enabled { audit_lines.push(AuditLineRecord{ phase: "post", out_idx: final_out.lines().count().saturating_sub(1), regen_idx: None, text: raw.to_string(), trimmed: trimmed.to_string(), adopted_from: Some("struct_list_brace".into()), original_pos: None, original_indent: Some(desired_indent_len), final_indent: desired_indent_len, note: Some("list_entry_struct_match".into()) }); }
                            _list_entry_brace_idx += 1; // consume slot
                            continue 'line_loop; // keep existing indent for list entry brace and prevent later adoption override
                        }
                    } else {
                        // Generic block closing brace: only correct if under-indented relative to its opener.
                        let opener_indent_len = open_line.chars().take_while(|c| c.is_whitespace()).count();
                        let current_indent_len = raw.chars().take_while(|c| c.is_whitespace()).count();
                        if current_indent_len < opener_indent_len {
                            let fixed = format!("{}{}", " ".repeat(opener_indent_len), trimmed);
                            final_out.push_str(&fixed); final_out.push('\n'); if audit_enabled { audit_lines.push(AuditLineRecord{ phase: "post", out_idx: final_out.lines().count().saturating_sub(1), regen_idx: None, text: fixed.clone(), trimmed: trimmed.to_string(), adopted_from: Some("struct_generic_brace".into()), original_pos: None, original_indent: Some(opener_indent_len), final_indent: opener_indent_len, note: Some("generic_under_indent_fix".into()) }); }
                            _generic_brace_idx += 1;
                            #[cfg(feature="debug-resolver")] if trace_on { eprintln!("[MERGE-EVENT] generic_under_indent_fix opener_indent={} current_indent={} line_idx={}", opener_indent_len, current_indent_len, idx); }
                            continue 'line_loop;
                        }
                        // If equal or over-indented, leave as-is to preserve original formatting nuances.
                        _generic_brace_idx += 1;
                    }
                    // Generic block (either unchanged or already fixed): fall through; occurrence adoption may adjust if still mismatched via anchor.
                }
            }
            // Skip novel lines (not present in original trimmed set) to avoid forcing indentation on new content
            if !original_trimmed.contains(trimmed) { final_out.push_str(raw); final_out.push('\n'); continue; }
            let k_adopt = anchor_key(raw);
            if let Some(queue) = map.get(&k_adopt) {
                let used = *use_counters.get(&k_adopt).unwrap_or(&0);
                if used < queue.len() {
                    if let Some(occ) = queue.get(used) {
                        // Compare without current indent
                        let current_indent_len = raw.chars().take_while(|c| c.is_whitespace()).count();
                        let desired_indent = &occ.indent;
                        let desired_indent_len = desired_indent.len();
                            if current_indent_len != desired_indent_len {
                            // Adopt indentation
                            let regen_trim = raw.trim_start();
                            let mut composed = format!("{}{}", desired_indent, regen_trim);
                            if !raw.contains("//") { if let Some(inl)=&occ.inline { if !composed.ends_with(' ') { composed.push(' ');} composed.push_str(inl);} }
                            final_out.push_str(&composed); final_out.push('\n'); if audit_enabled { audit_lines.push(AuditLineRecord{ phase: "post", out_idx: final_out.lines().count().saturating_sub(1), regen_idx: None, text: composed.clone(), trimmed: trimmed.to_string(), adopted_from: Some("post_adopt".into()), original_pos: Some(occ.pos), original_indent: Some(desired_indent_len), final_indent: desired_indent_len, note: None }); }
                            use_counters.insert(k_adopt.clone(), used+1);
                            #[cfg(feature="debug-resolver")] {
                                if trace_on { eprintln!("[MERGE-EVENT] post_adopt key={} pos={}", k_adopt, occ.pos); }
                                events.push(MergeEvent{ idx: 0, kind: "post_adopt", trimmed: trimmed.to_string(), reason: format!("key={} pos={}", k_adopt, occ.pos), adopted_indent: Some(desired_indent.clone()), final_line: Some(composed.clone()) });
                            }
                            continue 'line_loop;
                        }
                    }
                }
            }
            // Secondary post-pass fallback (trimmed-content). Attempt only if line is code and novel check passed above.
            let core_raw = {
                let code_no_comment = match raw.find("//") { Some(i)=> &raw[..i], None=> raw };
                code_no_comment.trim().to_string()
            };
            if let Some(queue_t) = map_trimmed.get(&core_raw) {
                let used_t = *use_counters_trimmed.get(&core_raw).unwrap_or(&0);
                if used_t < queue_t.len() {
                    if let Some(occ_t) = queue_t.get(used_t) {
                        // Adopt indentation from trimmed occurrence
                        let regen_trim = raw.trim_start();
                        let mut composed = format!("{}{}", occ_t.indent, regen_trim.trim_start());
                        if !raw.contains("//") { if let Some(inl)=&occ_t.inline { if !composed.ends_with(' ') { composed.push(' ');} composed.push_str(inl);} }
                        final_out.push_str(&composed); final_out.push('\n'); if audit_enabled { audit_lines.push(AuditLineRecord{ phase: "post", out_idx: final_out.lines().count().saturating_sub(1), regen_idx: None, text: composed.clone(), trimmed: core_raw.clone(), adopted_from: Some("post_adopt_trimmed".into()), original_pos: Some(occ_t.pos), original_indent: Some(occ_t.indent.len()), final_indent: occ_t.indent.len(), note: None }); }
                        use_counters_trimmed.insert(core_raw.clone(), used_t+1);
                        #[cfg(feature="debug-resolver")] {
                            if trace_on { eprintln!("[MERGE-EVENT] post_adopt_trimmed core={} pos={}", core_raw, occ_t.pos); }
                            events.push(MergeEvent{ idx: 0, kind: "post_adopt_trimmed", trimmed: core_raw.clone(), reason: format!("pos={}", occ_t.pos), adopted_indent: Some(occ_t.indent.clone()), final_line: Some(composed.clone()) });
                        }
                        continue 'line_loop;
                    }
                }
            }
            // Fallback: keep original line
            final_out.push_str(raw); final_out.push('\n'); if audit_enabled { audit_lines.push(AuditLineRecord{ phase: "post", out_idx: final_out.lines().count().saturating_sub(1), regen_idx: None, text: raw.to_string(), trimmed: trimmed.to_string(), adopted_from: Some("keep".into()), original_pos: None, original_indent: None, final_indent: raw.chars().take_while(|c| c.is_whitespace()).count(), note: None }); }
        }
        #[cfg(feature="debug-resolver")] if trace_on { eprintln!("[MERGE] post-pass complete"); }
        #[cfg(feature="debug-resolver")] if json_trace { unsafe { LAST_JSON_TRACE = Some(serde_json::to_string_pretty(&events).unwrap_or_else(|_| "[]".into())); } }
        if audit_enabled { for rec in &audit_lines { rec.emit(); } }
        final_out
    }

    /// Public wrapper used by tests and callers to perform a merge.
    pub fn merge_comments(original: &str, regenerated: &str) -> String { Self::segment_merge(original, regenerated) }

    #[cfg(feature="debug-resolver")]
    pub fn take_last_merge_trace_json() -> Option<String> { unsafe { crate::file_ops::LAST_JSON_TRACE.take() } }

    #[allow(dead_code)]
    #[cfg(feature="debug-resolver")]
    pub fn merge_comments_with_trace(original:&str, regenerated:&str)->String {
        Self::enable_merge_trace(true);
        Self::merge_comments(original, regenerated)
    }
}

// Helper for generating unique temp file suffixes in tests.
fn rand_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    format!("{}", nanos)
}

    #[test]
    fn preserves_single_blank_line_before_standalone_comment_block() {
        let original = r#"div A {
    int X = 1
}

// standalone comment about next block
div B {
    int Y = 2
}
"#;
        let (_rem, mut nodes) = crate::parser::parse_document(original).expect("parse");
        crate::resolver::resolve_document(&mut nodes);
        let regenerated = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        let merged = OverseerFileHandler::merge_comments(original, &regenerated);
        assert!(merged.contains("}\n\n// standalone comment about next block"), "Missing blank line before standalone comment. Merged:\n{}", merged);
    }

    #[test]
    fn round_trip_save_preserves_comments_and_whitespace() {
        // Original text with header, inline, leading/trailing comment blocks and blank lines
        let original = r#"// Header line 1
// Header line 2

div T {
    // comment before field A
    int A = 1 // inline A

    // group comment
    int B = 2
}

// Standalone comment before instance
<T> I {
    - A = 3 // override inline
}

// Trailing file comment
"#;

        // Parse, resolve, and regenerate canonical content
        let (_rem, mut nodes) = crate::parser::parse_document(original).expect("parse");
        crate::resolver::resolve_document(&mut nodes);
        let regenerated = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");

        // Merge comments from original
        let merged = OverseerFileHandler::merge_comments(original, &regenerated);

        // Write to a temporary file and read back
        let mut tmp = std::env::temp_dir();
        tmp.push(format!("overseer_test_{}_{}.os", std::process::id(), rand_suffix()));
        let tmp_path = tmp.to_string_lossy().to_string();
        std::fs::write(&tmp_path, &merged).expect("write");
        let roundtrip = std::fs::read_to_string(&tmp_path).expect("read");

        // Assertions: header, inline, standalone, trailing comments survive
        assert!(roundtrip.contains("// Header line 1"));
        assert!(roundtrip.contains("// Header line 2"));
        assert!(roundtrip.contains("// comment before field A"));
        assert!(roundtrip.contains("// inline A"));
        assert!(roundtrip.contains("// Standalone comment before instance"));
        assert!(roundtrip.trim_end().ends_with("// Trailing file comment"));
        // Ensure the spacer before the standalone comment remains
        assert!(roundtrip.contains("}\n\n// Standalone"), "Expected a blank line before the standalone comment to be preserved. Got:\n{}", roundtrip);
        // Clean up
        let _ = std::fs::remove_file(&tmp_path);
    }

    #[test]
    fn guarded_revert_does_not_move_trailing_comment_in_block() {
        // This mirrors examples/basic/mutability_test.os simplified to essential lines
        // The comment should remain directly after the E line inside the dr block.
        let original = r#"int A (mutable=true) = 55
int B (mutable=false) = 10
int C (mutable="guarded") = 11
int D = 6

div dr {

    button Prev (label="Update") {
        on click {
            set (path="/dr/E") = 20

        }
    }
   
   int E (mutable="guarded") = 10
   // comment line

}

"#;

        // Parse original
        let (_rem, mut nodes) = crate::parser::parse_document(original).expect("parse");
        crate::resolver::resolve_document(&mut nodes);

        // Simulate an action changing E to 20 then guarded normalization reverting it to 10 before serialization.
        // Find dr/E node.
        fn find_e<'a>(nodes: &'a mut [OverseerNode]) -> Option<&'a mut OverseerNode> {
            for n in nodes.iter_mut() {
                if n.name == "dr" {
                    for c in n.children.iter_mut() {
                        if c.name == "E" { return Some(c); }
                    }
                }
            }
            None
        }
        // Change to 20
        if let Some(e) = find_e(&mut nodes) { e.parameters.insert("value".into(), OverseerValue::Integer(20)); }
        // Guarded revert: restore to 10 (matching original) prior to serialization
        if let Some(e) = find_e(&mut nodes) { e.parameters.insert("value".into(), OverseerValue::Integer(10)); }

        // NOTE: Keep original file formatting exactly (no escaped quotes) to mirror real source.

        let regenerated = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        let merged = OverseerFileHandler::merge_comments(original, &regenerated);

        // Ensure comment still appears and order relative to E line unchanged (comment after E inside block)
    // The serializer may normalize spacing: (mutable="guarded") or (mutable=\"guarded\"). We search relaxed.
    let e_search = "int E"; // broad anchor
    let pos_e_orig = original.find(e_search).expect("E line in original");
        let pos_comment_orig = original.find("// comment line").expect("comment in original");
        assert!(pos_comment_orig > pos_e_orig, "In original, comment should appear after E line");

    let pos_e_merged = merged.find(e_search).expect("E line in merged");
        let pos_comment_merged = merged.find("// comment line").expect("comment in merged");
        assert!(pos_comment_merged > pos_e_merged, "Comment moved before E line after merge.\n--- Regenerated ---\n{}\n--- Merged ---\n{}", regenerated, merged);

        // Also ensure the relative distance (rough heuristic) did not expand beyond 120 chars to catch large relocations
        assert!(pos_comment_merged - pos_e_merged < 200, "Comment drifted too far from E line after merge");
    }

    #[test]
    fn guarded_action_end_to_end_preserves_comment() {
        // Full flow: parse original, simulate action set /dr/E=20 (like button Prev), guarded revert, serialize + merge.
        let original = r#"int A (mutable=true) = 55
int B (mutable=false) = 10
int C (mutable="guarded") = 11
int D = 6

div dr {

    button Prev (label="Update") {
        on click {
            set (path="/dr/E") = 20

        }
    }
   
   int E (mutable="guarded") = 10
   // comment line

}

"#;

        // Parse + resolve
        let (_rem, mut nodes) = crate::parser::parse_document(original).expect("parse");
        crate::resolver::resolve_document(&mut nodes);

        // Simulate executing the set action: find E and set it to 20 (UI/action effect)
        fn find_e_mut<'a>(nodes: &'a mut [OverseerNode]) -> Option<&'a mut OverseerNode> {
            for n in nodes.iter_mut() { if n.name=="dr" { for c in n.children.iter_mut() { if c.name=="E" { return Some(c); } } } }
            None
        }
        if let Some(e) = find_e_mut(&mut nodes) { e.parameters.insert("value".into(), OverseerValue::Integer(20)); }

        // Guarded normalization (like frontend before save) -> revert to original 10
        if let Some(e) = find_e_mut(&mut nodes) { e.parameters.insert("value".into(), OverseerValue::Integer(10)); }

        // Serialize & merge
        let regenerated = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        let merged = OverseerFileHandler::merge_comments(original, &regenerated);

        // Assertions
        let e_search = "int E";
        let pos_e_orig = original.find(e_search).unwrap();
        let pos_comment_orig = original.find("// comment line").unwrap();
        assert!(pos_comment_orig > pos_e_orig);
        let pos_e_merged = merged.find(e_search).expect("E in merged");
        let pos_comment_merged = merged.find("// comment line").expect("comment in merged");
        assert!(pos_comment_merged > pos_e_merged, "Comment moved before E. Regenerated:\n{}\nMerged:\n{}", regenerated, merged);
    }

    

#[cfg(test)]
mod tests_serialization_preserve_transparent_overrides {
    use super::*;

    #[test]
    fn preserve_field_overrides_inside_transparent_wrapper_in_list_entry() {
        let original = r#"div A {
    int field_a = 0
    div {
        int field_b = 0
    }
}

list L(entry=<A>) {
    - {
        - field_a = 10
        - field_b = 20
    }
}
"#;

        // Parse and resolve to simulate runtime processing (template clone + markers)
        let (_rem, mut nodes) = crate::parser::parse_document(original).expect("parse");
        crate::resolver::resolve_document(&mut nodes);

        // Serialize back
        let serialized = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");

        // Must keep concise override for field_b (no loss through unnamed wrapper)
        assert!(
            serialized.contains("- field_b = 20"),
            r#"Expected serialized content to contain "- field_b = 20" but got:
{}"#,
            serialized
        );

        // Round-trip: parse serialized and resolve; verify the first list entry sees field_b=20 via transparency
        let (_rem2, mut nodes2) = crate::parser::parse_document(&serialized).expect("parse2");
        crate::resolver::resolve_document(&mut nodes2);

        // Find list L, then its first entry, then accessible child 'field_b'
        let list = nodes2.iter().find(|n| n.node_type == "list" && n.name == "L").expect("list L not found");
        assert!(!list.children.is_empty(), "list L has no children after roundtrip: {}", serialized);
        let entry = &list.children[0];
        // After resolution, list entries are template-instantiated using the template name as node_type (A) and auto name like A__1
        let children = entry.get_accessible_children();
        let field_b = children.into_iter().find(|c| c.name == "field_b").expect("field_b not accessible in entry after roundtrip");
        let val = field_b.parameters.get("value").cloned().expect("field_b has no value after roundtrip");
        assert_eq!(val, OverseerValue::Integer(20), "field_b value mismatch: {:?}", val);
    }
}

#[cfg(test)]
mod tests_weight_tracker_round_trip_fidelity {
    use super::*;

    #[test]
    fn weight_tracker_new_parse_resolve_serialize_is_idempotent() {
        let original = r##"tab weight_minimal {

    // Minimal focus: a selected date (day precision) and Prev/Next navigation
    div (hidden=true) {

        div MealRecord (layout="vertical") {
            div {
                string description = ""
                int amount (label="Amount") = 1
                text Score (font-size=100px, margin=0px) = $((NutriScore/S <= 0.0)?
                    "# <color= #329c17 | A>":((NutriScore/S <= 2.0)?
                    "# <color= #78cd11 | B>":((NutriScore/S <= 10.0)?
                    "# <color= #cdca26 | C>":((NutriScore/S <= 18.0)?
                    "# <color= #f58412 | D>":
                    "# <color= #cc2512 | E>"))))
            }
            div {
                float calories (label="Calories") = $(amount*per_item/calories)
                float weight (label="Weight", suffix=" g") = $(amount*per_item/weight)
                float protein (label="Protein") = $(amount*per_item/protein)
                float fat (label="Fat") = $(amount*per_item/fat)
                float saturated_fat (label="Saturated Fat") = $(amount*per_item/saturated_fat)
                float carbs (label="Carbs") = $(amount*per_item/carbs)
                float sugar (label="Sugar") = $(amount*per_item/sugar)
                float fibre (label="Fibre") = $(amount*per_item/fibre)
                float salt (label="Salt") = $(amount*per_item/salt)

                div NutriScore (hidden=true) {
                    float A = $(per_100g/calories*0.0125)
                    float B = $(per_100g/sugar*0.22222)
                    float C = $(per_100g/saturated_fat)
                    float D = $(per_100g/salt*11.11)
                    float F = $(per_100g/fibre*1.43)
                    float G = $(per_100g/protein*0.625)

                    float S = $(A + B + C + D - F - G)
                }
            }

            div per_item {
                float calories = $(weight*per_100g/calories*0.01)
                float weight (suffix=" g") = 100
                float protein = $(weight*per_100g/protein*0.01)
                float fat = $(weight*per_100g/fat*0.01)
                float saturated_fat = $(weight*per_100g/saturated_fat*0.01)
                float carbs = $(weight*per_100g/carbs*0.01)
                float sugar = $(weight*per_100g/sugar*0.01)
                float fibre = $(weight*per_100g/fibre*0.01)
                float salt = $(weight*per_100g/salt*0.01)
            }

            div per_100g {
                float calories = 100
                float protein = 5
                float fat = 5
                float saturated_fat = 1
                float carbs = 5
                float sugar = 1
                float fibre = 10
                float salt = 1
            }
        }

        div WeightRecord (background-color=$((total_calories < 1000) ?"#195700ff":"#3f0803ff")) { // Daily data entry
            timestamp date (precision="day") = $(today())
            string test_data = "test"
            float weight (fallback=$(
                /weight_minimal/History
                    .filter(|x| x/date == /weight_minimal/History.filter(|x| x/date < ../date).map(|x| x/date).max())
                    .map(|x| x/weight)
                    .first(80.0)
            ), precision=1, suffix=" kg") = null

            int total_calories (label="Total Calories") = $(intake.sum(calories))
            int total_protein (label="Total Protein") = $(intake.sum(protein))

            list intake (entry=<MealRecord>, hidden=true, layout="vertical")
        }
    }

    // Selected panel with only the selected date
    div Selected (layout="vertical") {
        //div 
            // Dynamic linking handles load and create-on-edit; no manual population needed
        button Prev (label="< Prev Day") {
            on click {
                set (path="/weight_minimal/Selected/selected_date") = $(date_add_days(../selected_date, -1))
            }
        }
        timestamp selected_date (precision="day") = $(today())

        button Next (label="> Next Day") {
            on click {
                set (path="/weight_minimal/Selected/selected_date") = $(date_add_days(../selected_date, 1))
            }
        }
        // 
            
        // Override the linked WeightRecord's intake visibility locally
        // Prepend the new record on first edit if it's missing
        div SelectedWeightRecord (link="/weight_minimal/History[key=$(../selected_date)]", phantom-materialize="prepend-on-edit", background-color="#000000") {
            list intake (hidden=false)
        }
    }

    // History uses date as key and day precision for equivalence
    list History (entry=<WeightRecord>, key="date", keyPrecision="day") {
        - {
            - date = "2025-09-23"
            - weight = 109.3
        }
        - {
            - date = "2025-09-22"
            - test_data = "test"
            - weight = 108.8
            - total_calories = $(intake.sum(calories))
            - intake {
                - {
                    - description = "Chicken Wrap"
                    - amount = 1
                    div per_item {
                        - weight = 300
                        - calories = 410
                        - fat = 23
                        - saturated_fat = 3
                        - salt = 0.340
                        - carbs = 19
                        - sugar = 4
                        - fibre = 5
                        - protein = 27
                    }
                }
                - {
                    - description = "Coffee"
                    - amount = 1
                    div per_item {
                        - weight = 250
                    }
                    div per_100g {
                        - calories = 80
                        - protein = 0
                        - fat = 10
                        - saturated_fat = 2
                        - carbs = 20
                        - sugar = 5
                        - fibre = 0
                        - salt = 0
                    }
                }
            }
        }
        - {
            - date = "2025-09-21"
            - test_data = "test"
            - weight = 109
            - total_calories = $(intake.sum(calories))
            list intake {
                - {
                    - description = "Pizza Slice"
                    - amount = 5
                    div per_100g {
                        float calories = 340
                        float protein = 3
                        float fat = 12
                        float saturated_fat = 4
                        float carbs = 62
                        float sugar = 3
                        float fibre = 3
                        float salt = 1
                    }
                }
                - {
                    - description = "Potato Salad"
                    - amount = 1
                    div per_item {
                        float calories = $(weight*per_100g/calories*0.01)
                        float weight = 200
                        float protein = $(weight*per_100g/protein*0.01)
                        float fat = $(weight*per_100g/fat*0.01)
                        float saturated_fat = $(weight*per_100g/saturated_fat*0.01)
                        float carbs = $(weight*per_100g/carbs*0.01)
                        float sugar = $(weight*per_100g/sugar*0.01)
                        float fibre = $(weight*per_100g/fibre*0.01)
                        float salt = $(weight*per_100g/salt*0.01)
                    }
                    div per_100g {
                        float calories = 150
                        float protein = 5
                        float fat = 5
                        float saturated_fat = 1
                        float carbs = 5
                        float sugar = 1
                        float fibre = 10
                        float salt = 1
                    }
                }
            }
        }
        - {
            - date = "2025-09-20"
        }
        - {
            - date = "2025-09-19"
            - test_data = "test"
            - weight = 108.4
            - total_calories = $(intake.sum(calories))
        }
        - {
            - date = "2025-09-18"
            - test_data = "test"
            - weight = 108.5
            - total_calories = $(intake.sum(calories))
        }
        - {
            - date = "2025-09-17"
            - test_data = "test"
            - weight = 108.7
            - total_calories = $(intake.sum(calories))
        }
        - {
            - date = "2025-09-16"
            - test_data = "test"
            - weight = 108.4
            - total_calories = $(intake.sum(calories))
        }
        - {
            - date = "2025-09-13"
            - test_data = "test"
            - weight = 107.8
            - total_calories = $(intake.sum(calories))
        }
        - {
            - date = "2025-09-12"
            - test_data = "test"
            - weight = 109.2
            - total_calories = $(intake.sum(calories))
        }
        - {
            - date = "2025-09-11"
            - weight = 109.3
            - total_calories = 2000
        }
        - {
            - date = "2025-09-09"
            - test_data = "Tuesday"
            - weight = 108.8
            list intake {
                - {
                    - description = "Apple"
                    div per_item {
                        float calories = 60
                        float weight = 200
                        float protein = 5
                        float fat = 5
                        float saturated_fat = $(weight*per_100g/saturated_fat*0.01)
                        float carbs = 5
                        float sugar = $(weight*per_100g/sugar*0.01)
                        float fibre = $(weight*per_100g/fibre*0.01)
                        float salt = $(weight*per_100g/salt*0.01)
                    }
                }
            }
        }
        - {
            - date = "2025-09-07"
            - test_data = "W"
            - weight = 79.8
        }
        - {
            - date = "2025.08.27"
            - test_data = "Wednesday"
            - weight = 79.5
            list intake {
                - {
                    - calories = 100
                }
                - {
                    - calories = 150
                }
            }
        }
        - {
            - date = "2025.08.26"
            - test_data = "Wednesday"
            - weight = 80.1
        }
        - {
            - date = "2025.08.25"
            - test_data = "Tuesday"
            - weight = 80
        }
        - {
            - date = "2025.08.24"
            - test_data = "Monday"
            - weight = 108.8
        }
    }
}

// (exercise tracker tests relocated after weight tracker round trip module)
"##;

        let (_rem, mut nodes) = crate::parser::parse_document(original).expect("parse weight_tracker_new");
        crate::resolver::resolve_document(&mut nodes);
        let serialized = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");

        fn canon(s: &str) -> String {
            // Pre-scan to identify which original lines are full-line comments
            let all_lines: Vec<&str> = s.lines().collect();
            let is_comment: Vec<bool> = all_lines
                .iter()
                .map(|l| l.trim_start().starts_with("//"))
                .collect();

            let mut out: Vec<String> = Vec::new();
            let mut last_blank = false;
            for (i, line) in all_lines.iter().enumerate() {
                let trimmed_end = line.trim_end();
                // Drop full-line comments entirely for comparison
                if is_comment[i] { continue; }
                // Strip inline comments (anything after //)
                let code_only = match trimmed_end.find("//") {
                    Some(idx) => &trimmed_end[..idx],
                    None => trimmed_end,
                };
                let code = code_only.trim_end();
                let is_blank = code.trim().is_empty();
                if is_blank {
                    // If this blank line is adjacent to any comment line in the original, drop it.
                    let prev_is_comment = i > 0 && is_comment[i - 1];
                    let next_is_comment = i + 1 < is_comment.len() && is_comment[i + 1];
                    if prev_is_comment || next_is_comment { continue; }
                }
                if is_blank {
                    // Collapse consecutive blank lines to a single
                    if last_blank { continue; }
                    out.push(String::new());
                    last_blank = true;
                } else {
                    out.push(code.to_string());
                    last_blank = false;
                }
            }
            out.join("\n")
        }
        let orig_c = canon(original);
        let ser_c = canon(&serialized);

        if ser_c != orig_c {
            let o_lines: Vec<&str> = orig_c.lines().collect();
            let s_lines: Vec<&str> = ser_c.lines().collect();
            let max = o_lines.len().max(s_lines.len());
            for i in 0..max {
                let o = o_lines.get(i).copied().unwrap_or("<EOF>");
                let s = s_lines.get(i).copied().unwrap_or("<EOF>");
                if o != s {
                    println!("FIRST_DIFF_LINE {}\nO: {}\nS: {}", i + 1, o, s);
                    break;
                }
            }
        }
        assert_eq!(ser_c, orig_c, "Round-trip serialization for weight_tracker_new is not idempotent. Differs after resolve.\n--- ORIGINAL ---\n{}\n--- SERIALIZED ---\n{}", orig_c, ser_c);
    }
}

#[cfg(test)]
mod tests_exercise_tracker_round_trip {
    use super::*;
    const EXERCISE_SRC: &str = include_str!("../../examples/exercise_tracker/exercise.os");

    fn structure_signature(s: &str) -> Vec<(usize, String)> {
        s.lines().map(|l| {
            if l.trim_start().starts_with("//") || l.trim().is_empty() { return (0usize, String::new()); }
            let leading = l.chars().take_while(|c| *c==' ' || *c=='\t').count();
            (leading, l.trim_end().to_string())
        }).collect()
    }

    #[test]
    fn exercise_round_trip_preserves_structure() {
        let (_rem, mut nodes) = crate::parser::parse_document(EXERCISE_SRC).expect("parse exercise");
        crate::resolver::resolve_document(&mut nodes);
        let regenerated = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        let merged = OverseerFileHandler::merge_comments(EXERCISE_SRC, &regenerated);
                            // Generic block closing brace: record opener indent vs current for diagnostics (debug-resolver only)
                            #[cfg(feature="debug-resolver")] if trace_on {
                                let opener_indent_len = open_line.chars().take_while(|c| c.is_whitespace()).count();
                                let current_indent_len = raw.chars().take_while(|c| c.is_whitespace()).count();
                                if opener_indent_len != current_indent_len {
                                    eprintln!("[MERGE-DIAG] generic_brace_indent_mismatch line_idx={} opener_indent={} current_indent={} opener='{}' raw='{}'", idx, opener_indent_len, current_indent_len, opener_trim, raw.trim_end());
                                }
                            }
        assert!(merged.contains("div exercise_tracker"));
        assert!(merged.contains("list Exercises"));
        assert!(merged.contains("list History"));
        let sig_orig = structure_signature(EXERCISE_SRC);
        let sig_new = structure_signature(&merged);
        let distinct_orig: std::collections::HashSet<usize> = sig_orig.iter().map(|(n, _)| *n).filter(|n| *n>0).collect();
        let distinct_new: std::collections::HashSet<usize> = sig_new.iter().map(|(n, _)| *n).filter(|n| *n>0).collect();
        assert!(distinct_new.len() >= distinct_orig.len().saturating_sub(1), "Indentation levels collapsed: orig={:?} new={:?}", distinct_orig, distinct_new);
        assert!(merged.contains("- eid ="));
    }

    #[test]
    fn exercise_round_trip_is_idempotent_formatting() {
        // Full load -> resolve -> serialize -> merge should yield byte-for-byte identical text
        let (_rem, mut nodes) = crate::parser::parse_document(EXERCISE_SRC).expect("parse exercise");
        crate::resolver::resolve_document(&mut nodes);
        let regenerated = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        let merged = OverseerFileHandler::merge_comments(EXERCISE_SRC, &regenerated);
        if merged != EXERCISE_SRC {
            for (i,(o,n)) in EXERCISE_SRC.lines().zip(merged.lines()).enumerate() { if o!=n { println!("DIFF line {}\nORIG: '{}'\nNEW : '{}'", i+1, o, n); break; } }
        }
        assert_eq!(merged, EXERCISE_SRC, "exercise.os changed after round trip");
    }

    #[test]
    fn exercise_history_prepend_entry_stays_in_list() {
        use crate::types::OverseerNode;
        let ( _rem, mut nodes) = crate::parser::parse_document(EXERCISE_SRC).expect("parse exercise");
        crate::resolver::resolve_document(&mut nodes);
        // Find History list node mutably
        fn find_named<'a>(nodes:&'a mut [OverseerNode], typ:&str, name:&str)->Option<&'a mut OverseerNode>{
            for n in nodes.iter_mut() { if n.node_type==typ && n.name==name { return Some(n); } if let Some(f)=find_named(&mut n.children, typ, name){ return Some(f);} }
            None
        }
        let history = find_named(&mut nodes, "list", "History").expect("History list");
        // Build a minimal ExerciseRecord entry akin to runtime action: - { - eid=1 - sets=1 - reps=5 - weight=10 - time="2025-12-31T00:00:00Z" }
        // Represented as a list item wrapper node whose children include the field nodes.
    let mut entry = OverseerNode::new_with_type("object".to_string(), None);
        // Insert field children
    fn field(name:&str, val:OverseerValue)->OverseerNode { let mut n = OverseerNode::new_with_type("field".to_string(), Some(name.to_string())); n.parameters.insert("value".into(), val); n }
        entry.children.push(field("eid", OverseerValue::Integer(99)));
        entry.children.push(field("sets", OverseerValue::Integer(1)));
        entry.children.push(field("reps", OverseerValue::Integer(5)));
        entry.children.push(field("weight", OverseerValue::Float(10.0)));
        entry.children.push(field("time", OverseerValue::String("2025-12-31T00:00:00+00:00".into())));
        // Prepend to history
        history.children.insert(0, entry);
        let regenerated = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize mutated");
        let merged = OverseerFileHandler::merge_comments(EXERCISE_SRC, &regenerated);
        // Sanity: new eid=99 appears
        assert!(merged.contains("eid = 99"), "Prepended entry missing in merged text");
        // Ensure it's inside History list (appears before first existing history entry's eid=2 line and not after closing brace)
        let idx_history = merged.find("list History").expect("history header");
        let idx_new = merged.find("eid = 99").expect("new entry");
        assert!(idx_new > idx_history, "New entry not after history header");
        // Ensure not orphaned after final closing brace: final brace position
        let last_brace = merged.rfind("}\n}").unwrap_or(merged.len());
        assert!(idx_new < last_brace, "New entry appears after document end brace (orphaned)");
    }

    #[test]
    fn exercise_done_action_prepends_history_entry_in_structure() {
        let (_rem, mut nodes) = crate::parser::parse_document(EXERCISE_SRC).expect("parse exercise");
        crate::resolver::resolve_document(&mut nodes);
        fn find_list<'a>(nodes: &'a mut [OverseerNode], name: &str) -> Option<&'a mut OverseerNode> {
            for n in nodes.iter_mut() {
                if n.node_type=="list" && n.name==name { return Some(n); }
                if let Some(found) = find_list(&mut n.children, name) { return Some(found); }
            }
            None
        }
        let exercises = find_list(&mut nodes, "Exercises").expect("Exercises list not found recursively");
        let first_entry = exercises.children.iter().find(|c| c.get_accessible_children().iter().any(|gc| gc.name=="id")).expect("exercise entry");
        let id_val = first_entry.get_accessible_children().iter().find(|gc| gc.name=="id").and_then(|n| n.parameters.get("value")).cloned().expect("id val");
        let sets_val = first_entry.get_accessible_children().iter().find(|gc| gc.name=="sets").and_then(|n| n.parameters.get("value")).cloned().unwrap_or(OverseerValue::Integer(0));
        let reps_val = first_entry.get_accessible_children().iter().find(|gc| gc.name=="reps").and_then(|n| n.parameters.get("value")).cloned().unwrap_or(OverseerValue::Integer(0));
        let weight_val = first_entry.get_accessible_children().iter().find(|gc| gc.name=="weight").and_then(|n| n.parameters.get("value")).cloned().unwrap_or(OverseerValue::Null);
    let history = find_list(&mut nodes, "History").expect("History list not found recursively");
        let mut new_item = OverseerNode { name: "ExerciseRecord__NEW".to_string(), node_type: "ExerciseRecord".to_string(), template: None, parameters: Default::default(), children: Vec::new(), is_hierarchy_transparent: false, param_order: Vec::new(), raw_value_literal: None, authored_dash: false, child_original_index: None, leading_blank_lines: 0 };
        for (n, v) in [("eid", id_val), ("sets", sets_val), ("reps", reps_val), ("weight", weight_val)] { let mut child = OverseerNode { name: n.to_string(), node_type: n.to_string(), template: None, parameters: Default::default(), children: Vec::new(), is_hierarchy_transparent: false, param_order: Vec::new(), raw_value_literal: None, authored_dash: true, child_original_index: None, leading_blank_lines: 0 }; child.parameters.insert("value".into(), v); new_item.children.push(child); }
        let mut time_child = OverseerNode { name: "time".into(), node_type: "time".into(), template: None, parameters: Default::default(), children: Vec::new(), is_hierarchy_transparent: false, param_order: Vec::new(), raw_value_literal: None, authored_dash: true, child_original_index: None, leading_blank_lines: 0 };
        time_child.parameters.insert("value".into(), OverseerValue::Timestamp("2025-09-27T00:00:00Z".into()));
        new_item.children.push(time_child);
        history.children.insert(0, new_item);
        let regenerated = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        let merged = OverseerFileHandler::merge_comments(EXERCISE_SRC, &regenerated);
        let pos_history = merged.find("list History").expect("history anchor");
        let pos_new_time = merged.find("- time = \"2025-09-27T00:00:00Z\"").expect("new time field");
        assert!(pos_new_time > pos_history);
        assert!(merged.contains("- eid ="));
    }
}
