use std::collections::HashMap;
use std::path::Path;
use tokio::fs;

use crate::types::*;

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
                    let mut emit_pre_blanks_if_needed = |count: usize, out: &mut String| {
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
                let mut emit_pre_blanks_if_needed = |count: usize, out: &mut String| {
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

    /// Internal: deterministic segment-based merge implementation.
    ///
    /// Strategy (replaces the previous heuristic anchor/block reinsertion logic):
    /// 1. Parse the ORIGINAL text into an ordered list of (leading_comment_block, code_line) segments.
    ///    - A segment's leading block contains every contiguous full-line comment or blank line that
    ///      immediately precedes a code line. These are preserved verbatim and never deduplicated.
    /// 2. Build anchor maps for the REGENERATED canonical text keyed by a whitespace-insensitive
    ///    version of each code line (and a relaxed variant that truncates at the first parameter/brace
    ///    delimiter). This tolerates benign formatting shifts (e.g. added space before '(' or param order changes
    ///    that do not affect the prefix).
    /// 3. For each original segment:
    ///      a. Emit its leading block exactly.
    ///      b. If the segment's code line anchor matches a regenerated line anchor, emit the regenerated line
    ///         (canonical form). If the original had an inline comment and the regenerated canonical line
    ///         does NOT, append the original inline comment to retain author intent.
    ///      c. Otherwise (no anchor match), fall back to emitting the original code line (ensures we never
    ///         drop user-authored lines whose anchors disappeared due to upstream parser/serializer drift).
    /// 4. Preserve any trailing comment/blank block that followed the last code line exactly.
    /// 5. Append any regenerated code lines whose anchors were never used (i.e., brand new lines) at the end,
    ///    separated by a single blank line to make additions visually distinct without disturbing original layout.
    ///
    /// Benefits:
    /// - Absolute immunity to comment duplication: original comment segments are emitted once and only once.
    /// - Stable ordering: unchanged code keeps its surrounding commentary in place.
    /// - Inline comment preservation for anchors whose regenerated form normalizes away spacing.
    /// - Simplicity: no multi-phase reinjection heuristics or placeholder suppression needed.
    ///
    /// NOTE: This function is intentionally kept private; call `merge_comments` for the public API.
    fn segment_merge(original: &str, regenerated: &str) -> String {
        // Helper classifiers
        fn is_code(l: &str) -> bool { let t = l.trim_start(); !t.is_empty() && !t.starts_with("//") }
        fn anchor_key(line: &str) -> String {
            let code = match line.find("//") { Some(idx) => &line[..idx], None => line };
            code.chars().filter(|c| !c.is_whitespace()).collect::<String>()
        }
        use std::collections::{HashMap, HashSet};
        // Maps
        let mut leading_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut trailing_comment_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut inline_map: HashMap<String, String> = HashMap::new();
        let mut seen_anchor: HashSet<String> = HashSet::new();
        let mut trailing_block: Vec<String> = Vec::new();
        let mut prev_anchor: Option<String> = None;
        let orig_lines: Vec<&str> = original.lines().collect();
        let mut i = 0usize;
        while i < orig_lines.len() {
            // Collect leading comments/blank lines for upcoming code line
            let mut lead: Vec<String> = Vec::new();
            while i < orig_lines.len() && !is_code(orig_lines[i]) { lead.push(orig_lines[i].to_string()); i += 1; }
            if i >= orig_lines.len() { trailing_block = lead; break; }
            let code_line = orig_lines[i]; i += 1;
            // Detect trailing comment group for previous anchor: contiguous comment lines at start of lead
            if let Some(prev) = prev_anchor.clone() {
                if !lead.is_empty() && lead[0].trim_start().starts_with("//") {
                    let mut run_len = 0usize;
                    for l in &lead { if l.trim_start().starts_with("//") { run_len += 1; } else { break; } }
                    if run_len > 0 {
                        let trailing = lead.drain(0..run_len).collect::<Vec<_>>();
                        trailing_comment_map.entry(prev).or_insert_with(Vec::new).extend(trailing);
                        // Collapse excessive blank lines after moved trailing group: keep at most one
                        if !lead.is_empty() && lead[0].trim().is_empty() {
                            let mut j = 1usize; while j < lead.len() && lead[j].trim().is_empty() { j += 1; }
                            lead.drain(1..j);
                        }
                    }
                }
            }
            let key = anchor_key(code_line);
            // Normalize: if leading block has >1 consecutive blank lines immediately before first comment line, collapse to 1
            if !lead.is_empty() {
                if let Some(first_nonblank) = lead.iter().position(|l| !l.trim().is_empty()) {
                    if lead[first_nonblank].trim_start().starts_with("//") && first_nonblank > 0 {
                        let mut new_lead: Vec<String> = Vec::new();
                        let mut blank_added = false;
                        for (idx, line) in lead.into_iter().enumerate() {
                            if idx < first_nonblank {
                                if line.trim().is_empty() {
                                    if !blank_added { new_lead.push(line); blank_added = true; }
                                    // skip additional blanks
                                } else { new_lead.push(line); }
                            } else { new_lead.push(line); }
                        }
                        lead = new_lead;
                    }
                }
            }
            if !key.is_empty() && !seen_anchor.contains(&key) { leading_map.insert(key.clone(), lead); seen_anchor.insert(key.clone()); }
            if let Some(idx) = code_line.find("//") { inline_map.entry(key.clone()).or_insert(code_line[idx..].to_string()); }
            prev_anchor = Some(key);
        }
        // Walk regenerated lines in order, injecting preserved segments
        let mut out = String::new();
        let mut emitted_blocks: HashSet<String> = HashSet::new();
        for line in regenerated.lines() {
            if is_code(line) {
                let key = anchor_key(line);
                // Leading block (verbatim) – emit once
                if let Some(block) = leading_map.get(&key) {
                    if !emitted_blocks.contains(&key) {
                        // Determine if block pattern is blank + comment(s) and previous emitted line already blank
                        let mut last_line_blank = {
                            // Inspect last emitted line in out
                            if out.is_empty() { false } else {
                                // Find last line
                                let mut rev = out.rsplitn(2, '\n');
                                let current_last = rev.next().unwrap_or("");
                                current_last.trim().is_empty()
                            }
                        };
                        let has_comment_after_blank = block.len() >= 2 && block[0].trim().is_empty() && block[1].trim_start().starts_with("//");
                        for (idx, b) in block.iter().enumerate() {
                            let is_blank = b.trim().is_empty();
                            if is_blank && has_comment_after_blank && last_line_blank {
                                // Skip this extra blank to avoid duplicating separation before comment group
                                continue;
                            }
                            out.push_str(b); out.push('\n');
                            last_line_blank = is_blank;
                        }
                        emitted_blocks.insert(key.clone());
                    }
                }
                // Inline comment merge
                let mut emitted_line = false;
                if !line.contains("//") {
                    if let Some(inl) = inline_map.get(&key) { if !inl.is_empty() { let mut composed = line.to_string(); if !composed.ends_with(' ') { composed.push(' '); } composed.push_str(inl); out.push_str(&composed); out.push('\n'); emitted_line = true; } }
                }
                if !emitted_line { out.push_str(line); out.push('\n'); }
                // Trailing comment group
                if let Some(trails) = trailing_comment_map.get(&key) { for t in trails { out.push_str(t); out.push('\n'); } }
            } else {
                out.push_str(line); out.push('\n');
            }
        }
        // Orphan leading blocks (anchors removed) before overall trailing block
        for (k, block) in leading_map.iter() { if !emitted_blocks.contains(k) { for b in block { out.push_str(b); out.push('\n'); } } }
        for l in &trailing_block { out.push_str(l); out.push('\n'); }
        // Post-process: collapse runs of >2 blank lines before comment groups to a single blank separation
        let mut res = out;
        // First collapse any 3+ consecutive newlines globally to 2
        while res.contains("\n\n\n") { res = res.replace("\n\n\n", "\n\n"); }
        // Ensure pattern code\n\n// (i.e., exactly one blank line) not expanded further – already ensured by collapse
        res
    }

    // Merge comments from original text into regenerated canonical text.
    // Strategy:
    // - Capture leading and trailing standalone comment blocks from original.
    // - Build maps from anchor keys (whitespace-insensitive code lines) to:
    //   a) preceding standalone comment blocks, and b) inline comments.
    // - Walk regenerated lines and inject preserved comments at corresponding anchors.
    /// Public merge facade.
    ///
    /// Performs a deterministic, idempotent merge that preserves all original
    /// comment/blank line segments and substitutes only the underlying code
    /// lines whose anchors still exist in the regenerated canonical text.
    ///
    /// Historical note: A prior implementation attempted heuristic reinsertion
    /// (mapping standalone/inline comments to anchors with multiple passes).
    /// That approach proved fragile and occasionally duplicated comment lines
    /// in large documents. It has been fully replaced by the segment-based
    /// algorithm implemented in `segment_merge`.
    pub fn merge_comments(original: &str, regenerated: &str) -> String {
        OverseerFileHandler::segment_merge(original, regenerated)
    }
}

#[cfg(test)]
mod tests_merge_comments {
    use super::*;

    #[test]
    fn preserves_standalone_comment_before_block_with_param_spacing_change() {
        let original = r#"div T {
    int A = 1
    int B = 2
    int C = 3
}
<T> I {
    - A = 3
    - B = 2
}

// List as an example of correct behavior:
list L(entry=<T>) {
    - {
        - A = 3
        - B = 2
    }
}
"#;
        // Regenerated content may insert a space before '(' in parameter list
        let regenerated = r#"div T {
    int A = 1
    int B = 2
    int C = 3
}
<T> I {
    - A = 3
    - B = 2
}
list L (entry=<T>) {
    - {
        - A = 3
        - B = 2
    }
}
"#;
        let merged = OverseerFileHandler::merge_comments(original, regenerated);
        assert!(merged.contains("// List as an example of correct behavior:"), "Expected the standalone comment to be preserved in merged output.\nMerged:\n{}", merged);
        // Ensure the comment appears before the list line
        let pos_comment = merged.find("// List as an example of correct behavior:").unwrap();
        let pos_list = merged.find("list L (entry=<T>)").unwrap_or_else(|| merged.find("list L(entry=<T>)").unwrap());
        assert!(pos_comment < pos_list, "Comment should precede the list anchor line");
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

    fn rand_suffix() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        format!("{}", nanos)
    }
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
                let mut is_blank = code.trim().is_empty();
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
