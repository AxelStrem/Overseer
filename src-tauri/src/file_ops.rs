use std::collections::HashMap;
use std::path::Path;
use tokio::fs;

use crate::source_registry::SourceRegistry;
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

struct FormattingPreferences {
    indent_unit: String,
    newline: String,
}

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
        #[cfg(test)]
        let _registry_guard = crate::source_registry::REGISTRY_TEST_MUTEX.lock();
        let prefs = Self::detect_formatting(nodes);
        let indent_fallback = prefs.indent_unit.clone();

        for node in nodes {
            Self::serialize_node(node, &mut output, 0, indent_fallback.as_str())?;
        }

        let trailing = SourceRegistry::take_document_trailing();
        if !trailing.is_empty() {
            Self::push_trivia(&mut output, &trailing);
        }

        if prefs.newline == "\r\n" {
            output = output.replace('\n', "\r\n");
        }

        Ok(output)
    }

    fn push_trivia(output: &mut String, trivia: &str) {
        if trivia.is_empty() {
            return;
        }
        let mut normalized = if trivia.contains("\r\n") {
            trivia.replace("\r\n", "\n")
        } else {
            trivia.to_string()
        };
        if output.ends_with('\n') && normalized.starts_with('\n') && normalized.trim().is_empty() {
            normalized.remove(0);
        }
        output.push_str(&normalized);
    }

    fn emit_trailing_trivia(snapshot: &Option<NodeSourceSnapshot>, output: &mut String) {
        if let Some(snap) = snapshot {
            if snap.trailing_trivia.trim().is_empty() {
                return;
            }
            Self::push_trivia(output, &snap.trailing_trivia);
        }
    }

    fn should_skip_trailing_trivia(node: &OverseerNode) -> bool {
        let from_template = node.parameters.get("_from_template");
        matches!(from_template, Some(OverseerValue::Boolean(true)))
    }

    fn emit_trailing_trivia_if_allowed(
        node: &OverseerNode,
        snapshot: &Option<NodeSourceSnapshot>,
        output: &mut String,
    ) {
        if Self::should_skip_trailing_trivia(node) {
            return;
        }
        Self::emit_trailing_trivia(snapshot, output);
    }

    fn snapshot_block_inner(snapshot: &NodeSourceSnapshot) -> Option<String> {
        let (body_start, body_end) = snapshot.body_span?;
        if body_end <= body_start {
            return None;
        }
        let span_start = snapshot.span.0;
        let rel_start = body_start.saturating_sub(span_start);
        let rel_end = body_end.saturating_sub(span_start);
        if rel_start >= snapshot.full_text.len() || rel_end > snapshot.full_text.len() || rel_start >= rel_end {
            return None;
        }
        let slice = &snapshot.full_text[rel_start..rel_end];
        let first_newline = slice.find('\n')?;
        let last_newline = slice.rfind('\n')?;
        if last_newline <= first_newline {
            return None;
        }
        let inner = &slice[first_newline + 1..last_newline + 1];
        if inner.trim().is_empty() {
            return None;
        }
        Some(inner.replace("\r\n", "\n"))
    }

    fn is_auto_generated_entry_name(name: &str) -> bool {
        let trimmed = name.trim();
        if let Some((prefix, suffix)) = trimmed.rsplit_once("__") {
            if prefix.is_empty() {
                return false;
            }
            return suffix.chars().all(|c| c.is_ascii_digit());
        }
        false
    }

    fn snapshot_for(node: &OverseerNode) -> Option<NodeSourceSnapshot> {
        if let Some(snapshot) = node.source_snapshot.clone() {
            return Some(snapshot);
        }
        node.source_id
            .as_deref()
            .and_then(SourceRegistry::get)
    }

    fn detect_formatting(nodes: &[OverseerNode]) -> FormattingPreferences {
        fn visit(nodes: &[OverseerNode], indent: &mut Option<String>, newline: &mut Option<String>) {
            for node in nodes {
                if indent.is_some() && newline.is_some() {
                    return;
                }
                if let Some(snapshot) = FileOperations::snapshot_for(node) {
                    if indent.is_none() {
                        if let Some(ind) = snapshot.indent_unit.clone() {
                            if !ind.is_empty() {
                                *indent = Some(ind);
                            }
                        }
                    }
                    if newline.is_none() {
                        if let Some(nl) = snapshot.newline.clone() {
                            if !nl.is_empty() {
                                *newline = Some(nl);
                            }
                        }
                    }
                }
                if indent.is_some() && newline.is_some() {
                    return;
                }
                if !node.children.is_empty() {
                    visit(&node.children, indent, newline);
                }
            }
        }

        let mut indent = None;
        let mut newline = None;
        visit(nodes, &mut indent, &mut newline);

        FormattingPreferences {
            indent_unit: indent.unwrap_or_else(|| "    ".to_string()),
            newline: newline.unwrap_or_else(|| "\n".to_string()),
        }
    }

    fn serialize_node(
        node: &OverseerNode,
        output: &mut String,
        indent_level: usize,
        indent_unit: &str,
    ) -> Result<()> {
        Self::serialize_node_context(node, output, indent_level, false, indent_unit)
    }

    fn serialize_node_context(
        node: &OverseerNode,
        output: &mut String,
        indent_level: usize,
        in_list_body: bool,
        fallback_indent_unit: &str,
    ) -> Result<()> {
        let snapshot = Self::snapshot_for(node);
        let allow_snapshot_trivia = !Self::should_skip_trailing_trivia(node);
        let fallback_indent_unit = if fallback_indent_unit.is_empty() { "    " } else { fallback_indent_unit };

        let mut emitted_leading_trivia = false;
        if allow_snapshot_trivia {
            if let Some(snap) = snapshot.as_ref() {
            if !snap.leading_trivia.is_empty() {
                Self::push_trivia(output, &snap.leading_trivia);
                emitted_leading_trivia = true;
            }
        }
        }

        if !emitted_leading_trivia {
            let mut blank_lines_to_emit = node.leading_blank_lines;
            if !output.is_empty() {
                if indent_level == 0 {
                    blank_lines_to_emit = 0;
                } else {
                    blank_lines_to_emit = blank_lines_to_emit.saturating_sub(1);
                }
            }
            for _ in 0..blank_lines_to_emit {
                output.push('\n');
            }
        }

        let indent = snapshot
            .as_ref()
            .and_then(|snap| snap.indent_unit.clone())
            .filter(|ind| !ind.is_empty())
            .unwrap_or_else(|| fallback_indent_unit.repeat(indent_level));
        let needs_indent = !emitted_leading_trivia || output.ends_with('\n') || output.is_empty();
        if needs_indent {
            output.push_str(&indent);
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
                        Self::emit_trailing_trivia_if_allowed(node, &snapshot, output);
                        return Ok(());
                    }
                }
                // Complex/template-based item: emit as "- name { ... }" (name optional) and use the standard child emission (with concise override rules)
                output.push_str("- ");
                let entry_name = node.name.trim();
                let has_displayable_name = !entry_name.is_empty()
                    && entry_name != "-"
                    && !Self::is_auto_generated_entry_name(entry_name);
                if has_displayable_name {
                    output.push_str(entry_name);
                    output.push_str(" {\n");
                } else {
                    output.push_str("{\n");
                }
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
                for child in &node.children {
                    let child_snapshot = Self::snapshot_for(child);
                    let child_indent = child_snapshot
                        .as_ref()
                        .and_then(|snap| snap.indent_unit.clone())
                        .unwrap_or_else(|| fallback_indent_unit.repeat(indent_level + 1));
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
                            for (n, v) in desc_overrides {
                                output.push_str(&format!(
                                    "{}- {} = {}\n",
                                    child_indent,
                                    n,
                                    Self::serialize_value_with_node(child, v)
                                ));
                            }
                            continue; // Skip normal emission of the transparent wrapper
                        }
                    }
                    // Skip template-derived children that are not explicitly overridden
                    if suppress_template_children && is_template_child && (!has_explicit_override) {
                        continue;
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
                                output.push_str(&format!(
                                    "{}- {} = {}\n",
                                    child_indent,
                                    child.name,
                                    Self::serialize_value_with_node(child, val)
                                ));
                                continue;
                            }
                        }
                    }
                    // Fallback: serialize child normally inside the block (not as list body)
                    // so field lines and nested blocks render correctly.
                    Self::serialize_node_context(child, output, indent_level + 1, false, fallback_indent_unit)?;
                }
                if node.children.is_empty() {
                    if let Some(body_text) = snapshot
                        .as_ref()
                        .and_then(|snap| Self::snapshot_block_inner(snap))
                    {
                        Self::push_trivia(output, &body_text);
                    }
                }
                output.push_str(&format!("{}}}\n", indent));
                Self::emit_trailing_trivia_if_allowed(node, &snapshot, output);
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
                        Self::emit_trailing_trivia_if_allowed(node, &snapshot, output);
                        return Ok(());
                    }
                }
                // Non-list context override: named nested block "- name { ... }"
                if !node.name.is_empty() && !node.children.is_empty() {
                    output.push_str("- ");
                    output.push_str(&node.name);
                    output.push_str(" {\n");
                    for child in &node.children {
                        Self::serialize_node_context(child, output, indent_level + 1, false, fallback_indent_unit)?;
                    }
                    output.push_str(&format!("{}}}\n", indent));
                    Self::emit_trailing_trivia_if_allowed(node, &snapshot, output);
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
                        Self::emit_trailing_trivia_if_allowed(node, &snapshot, output);
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
            // Emit parameters in a deterministic order to avoid random reordering in saves
            let keys: Vec<&String> = regular_params.keys().collect();
            let mut keys_sorted = keys.clone();
            keys_sorted.sort();
            let params_str: Vec<String> = keys_sorted
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
            output.push_str(&format!(" = {}\n", Self::serialize_value_with_node(node, value)));
        } else if node.children.is_empty() {
            if let Some(body_text) = snapshot
                .as_ref()
                .and_then(|snap| Self::snapshot_block_inner(snap))
            {
                Self::push_trivia(output, &body_text);
            }
            output.push('\n');
        } else {
            output.push_str(" {");
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
            for child in &node.children {
                let child_snapshot = Self::snapshot_for(child);
                let child_indent = child_snapshot
                    .as_ref()
                    .and_then(|snap| snap.indent_unit.clone())
                    .unwrap_or_else(|| fallback_indent_unit.repeat(indent_level + 1));
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
                            let has_template_value_marker = node.parameters.contains_key("_template_value");
                            if only_value_override && !has_template_value_marker {
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
            for (n, v) in desc_overrides {
                            output.push_str(&format!(
                                "{}- {} = {}\n",
                                child_indent,
                                n,
                Self::serialize_value_with_node(child, v)
                            ));
                        }
                        continue;
                    }
                }
                if suppress_template_children && is_template_child && (!has_explicit_override || !listed_in_instance_overrides) {
                    continue;
                }
                // For template instances/clones, if a child was overridden with only a simple value, prefer the concise "- name = value" form
                if suppress_template_children && has_explicit_override && listed_in_instance_overrides {
                    let has_value = child.parameters.contains_key("value");
                    let non_internal_non_value_params = child
                        .parameters
                        .iter()
                        .filter(|(k, _)| {
                            let ks = k.as_str();
                            // allow 'value' only; ignore internal keys starting with '_'
                            if ks.starts_with('_') || ks == "value" { return false; }
                            // also ignore params that are template-derived (paired _template_param exists)
                            let marker = format!("_template_{}", ks);
                            !child.parameters.contains_key(&marker)
                        })
                        .count();
                    let only_value_override = has_value && non_internal_non_value_params == 0 && child.children.is_empty();
                    // Guard: only treat as an explicit value override if the template value marker was removed.
                    let has_template_value_marker = child.parameters.contains_key("_template_value");
                    if only_value_override && !has_template_value_marker {
                        debug_serializer!(
                            "[SER] concise emit: name='{}' explicit={} tmpl_marker_removed={} suppress={} is_templ_child={} non_val_params={} has_val={}",
                            child.name,
                            has_explicit_override,
                            !has_template_value_marker,
                            suppress_template_children,
                            is_template_child,
                            non_internal_non_value_params,
                            has_value
                        );
                        let val = child.parameters.get("value").unwrap();
                        output.push_str(&format!(
                            "{}- {} = {}\n",
                            child_indent,
                            child.name,
                            Self::serialize_value_with_node(child, val)
                        ));
                        continue;
                    }
                }
                Self::serialize_node_context(child, output, indent_level + 1, children_in_list_body, fallback_indent_unit)?;
            }
            output.push_str(&format!("{}}}\n", indent));
        }

    Self::emit_trailing_trivia_if_allowed(node, &snapshot, output);

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
}

#[cfg(test)]
mod tests_serializer_comment_trivia {
    use super::*;

    fn parse_and_resolve(src: &str) -> Vec<OverseerNode> {
        let (_rem, mut nodes) = crate::parser::parse_document(src).expect("parse");
        crate::resolver::resolve_document(&mut nodes);
        nodes
    }

    #[test]
    fn field_update_preserves_comments() {
        let _guard = crate::source_registry::REGISTRY_TEST_MUTEX.lock();
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

        let mut nodes = parse_and_resolve(original);
        let div_t = nodes
            .iter_mut()
            .find(|n| n.node_type == "div" && n.name == "T")
            .expect("div T not found");
        let field_b = div_t
            .children
            .iter_mut()
            .find(|child| child.name == "B")
            .expect("field B not found");
    field_b.parameters.insert("value".to_string(), OverseerValue::Integer(42));
        field_b.source_fingerprint = None;

        let output = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        assert!(output.contains("// Header line 1"));
        assert!(output.contains("// comment before field A"));
        assert!(output.contains("// inline A"));
        assert!(output.contains("// Standalone comment before instance"));
        assert!(output.trim_end().ends_with("// Trailing file comment"));
        assert!(output.contains("int B = 42"));
    }

    #[test]
    fn list_comment_survives_entry_change() {
        let _guard = crate::source_registry::REGISTRY_TEST_MUTEX.lock();
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

        let mut nodes = parse_and_resolve(original);
        let list_l = nodes
            .iter_mut()
            .find(|n| n.node_type == "list" && n.name == "L")
            .expect("list L not found");
        list_l.source_fingerprint = None;
        let entry = list_l.children.first_mut().expect("entry not found");
        entry.source_fingerprint = None;
        if let Some(field_b) = entry.children.iter_mut().find(|child| child.name == "B") {
            field_b.parameters.insert("value".to_string(), OverseerValue::Integer(5));
            field_b.source_fingerprint = None;
        }

        let output = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        assert!(output.contains("// List as an example of correct behavior:"));
        assert!(output.contains("list L (entry=<T>)"));
        assert!(output.contains("- B = 5"));
    }

    #[test]
    fn empty_block_comment_preserved_after_rename() {
        let _guard = crate::source_registry::REGISTRY_TEST_MUTEX.lock();
        let original = r#"list FinalTemplate (entry=<ExtendedTemplate>) {
    - {
        // Should inherit BaseTemplate parameters through ExtendedTemplate
    }
}
"#;

        let mut nodes = parse_and_resolve(original);
        let list = nodes
            .iter_mut()
            .find(|n| n.node_type == "list" && n.name == "FinalTemplate")
            .expect("list not found");
        list.source_fingerprint = None;
        let entry = list.children.first_mut().expect("entry not found");
        entry.name = "Entry1".to_string();
        entry.source_fingerprint = None;

        let output = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        assert!(output.contains("// Should inherit BaseTemplate parameters through ExtendedTemplate"));
        assert!(output.contains("Entry1"));
    }

    #[test]
    fn chart_comment_survives_plot_update() {
        let _guard = crate::source_registry::REGISTRY_TEST_MUTEX.lock();
        let original = r##"chart C {
    plot P1 (color="#ff0000", label="A", source=$(foo), x=$(bar), y=$(baz))
    // P3: cumulative average over all exercises per day for each point's day
    plot P3 (color="#b42c22ff", label="Exercises", source=$(/drum_tracker/History.filter(|x| x/eid == 98) ), x=$(|t| t/time), y=$(|t| /drum_tracker/History.filter(|x| same_day(x/time, t/time)&&(x/eid!=33 && x/eid!=31 )).average(|x| x/avg_points)))
    plot P3 (color="#792eabff", label="Syncopated", source=$(/drum_tracker/History.filter(|x| x/eid == 80) ), x=$(|t| t/time), y=$(|t| /drum_tracker/History.filter(|x| same_day(x/time, t/time)&&(x/eid==80||x/eid==84)).average(|x| x/avg_points)))
}
"##;

        let mut nodes = parse_and_resolve(original);
        let chart = nodes
            .iter_mut()
            .find(|n| n.node_type == "chart" && n.name == "C")
            .expect("chart not found");
        chart.source_fingerprint = None;
        if let Some(plot) = chart.children.iter_mut().find(|child| child.name == "P1") {
            plot.parameters.insert("color".to_string(), OverseerValue::String("#00ff00".to_string()));
            plot.source_fingerprint = None;
        }

        let output = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        assert!(output.contains("// P3: cumulative average over all exercises per day for each point's day"));
        assert!(output.contains("color=\"#00ff00\""));
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
mod tests_serialization_formatting {
    use super::*;

    fn strip_snapshots(nodes: &mut [OverseerNode]) {
        for node in nodes {
            node.source_snapshot = None;
            if !node.children.is_empty() {
                strip_snapshots(&mut node.children);
            }
        }
    }

    #[test]
    fn serializer_preserves_indent_and_newlines_via_registry() {
    let _registry_guard = crate::source_registry::REGISTRY_TEST_MUTEX.lock();
        let original = "tab Root {\r\n  string title = \"Hi\"\r\n\r\n  div Group {\r\n    int value = 1\r\n  }\r\n}\r\n";

        let (_rem, mut nodes) = crate::parser::parse_document(original).expect("parse");
        crate::resolver::resolve_document(&mut nodes);

    let root = nodes.first().expect("root node");
    assert_eq!(root.children.len(), 2, "expected two children under root");
    assert_eq!(root.children[0].leading_blank_lines, 1, "parser encodes one newline before first child");
    assert_eq!(root.children[1].leading_blank_lines, 2, "parser encodes newline plus blank spacer before second child");
    let group = &root.children[1];
    assert_eq!(group.children.len(), 1, "group should have one child");
    assert_eq!(group.children[0].leading_blank_lines, 1, "nested child records only the structural newline");

        strip_snapshots(&mut nodes);

        let serialized = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");

        assert_eq!(
            serialized,
            original,
            "Serializer should retain CRLF newlines and two-space indentation fetched from SourceRegistry"
        );

        crate::source_registry::SourceRegistry::reset();
    }
}
