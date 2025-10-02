use std::collections::{HashMap, VecDeque};
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
    let prefs = Self::detect_formatting(nodes);
        let indent_fallback = prefs.indent_unit.clone();

        for node in nodes {
            Self::serialize_node(node, &mut output, 0, indent_fallback.as_str())?;
        }

        if prefs.newline == "\r\n" {
            output = output.replace('\n', "\r\n");
        }

        Ok(output)
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
        let fallback_indent_unit = if fallback_indent_unit.is_empty() { "    " } else { fallback_indent_unit };

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

        let indent = snapshot
            .as_ref()
            .and_then(|snap| snap.indent_unit.clone())
            .unwrap_or_else(|| fallback_indent_unit.repeat(indent_level));
        output.push_str(&indent);

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
                    for child in &node.children {
                        Self::serialize_node_context(child, output, indent_level + 1, false, fallback_indent_unit)?;
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

    // Merge comments from original text into regenerated canonical text.
    // Strategy:
    // - Capture leading and trailing standalone comment blocks from original.
    // - Build maps from anchor keys (whitespace-insensitive code lines) to:
    //   a) preceding standalone comment blocks, and b) inline comments.
    // - Walk regenerated lines and inject preserved comments at corresponding anchors.
    pub fn merge_comments(original: &str, regenerated: &str) -> String {
        // Helper: compute an anchor key by stripping inline comments and whitespace
        fn anchor_key(line: &str) -> String {
            let code = match line.find("//") {
                Some(idx) => &line[..idx],
                None => line,
            };
            let indent = code.chars().take_while(|c| c.is_whitespace()).count();
            let trimmed_original = code.trim_start();

            let mut canonical = code.trim_end().to_string();
            loop {
                let trimmed = canonical.trim_end();
                if trimmed.ends_with("{}") {
                    let new_len = trimmed.len().saturating_sub(2);
                    canonical.truncate(new_len);
                    continue;
                }
                if trimmed.ends_with('{') {
                    let new_len = trimmed.len().saturating_sub(1);
                    canonical.truncate(new_len);
                    continue;
                }
                break;
            }
            let trimmed = canonical.trim_start();

            if trimmed_original.starts_with("- {") {
                return "list_entry".to_string();
            }
            if trimmed.starts_with("plot") {
                let mut parts = trimmed.split_whitespace();
                let _ = parts.next(); // "plot"
                let name = parts.next().unwrap_or("");
                let label_value = trimmed
                    .split("label=")
                    .nth(1)
                    .map(|rest| {
                        rest.split(|c| c == ',' || c == ')')
                            .next()
                            .unwrap_or("")
                            .trim_matches('"')
                            .trim_matches('\'')
                            .to_string()
                    })
                    .unwrap_or_else(|| "nolabel".to_string());
                return format!("plot:{}:{}", name, label_value);
            }

            fn filtered(s: &str) -> String {
                s.chars()
                    .filter(|c| {
                        !c.is_whitespace()
                            && *c != '"'
                            && *c != '\''
                            && !c.is_ascii_digit()
                    })
                    .collect()
            }

            let normalized: String = if let Some(start) = trimmed.find('(') {
                let (prefix, rest) = trimmed.split_at(start);
                if let Some(end) = rest.rfind(')') {
                    let inside = &rest[1..end];
                    let mut parts: Vec<String> = inside
                        .split(',')
                        .map(|p| filtered(p))
                        .filter(|p| !p.is_empty())
                        .collect();
                    parts.sort();
                    let mut key = String::new();
                    key.push_str(&filtered(prefix));
                    key.push('(');
                    key.push_str(&parts.join(","));
                    key.push(')');
                    let trailing = &rest[end + 1..];
                    let trailing_filtered = filtered(trailing);
                    if !trailing_filtered.is_empty() {
                        key.push_str(&trailing_filtered);
                    }
                    key
                } else {
                    filtered(trimmed)
                }
            } else {
                filtered(trimmed)
            };
            if normalized.is_empty() {
                String::new()
            } else if normalized == "}" {
                format!("{}:}}", indent)
            } else {
                normalized
            }
        }

        // Extract leading comment block
        let mut leading_block: Vec<String> = Vec::new();
        let mut started = false;
        for line in original.lines() {
            if line.trim_start().starts_with("//") || line.trim().is_empty() && !started {
                leading_block.push(line.to_string());
            } else {
                let _started_flag = { started = true; started };
                break;
            }
        }

        // Identify the first non-comment code line to avoid double-inserting its leading block later
        let first_code_key = original
            .lines()
            .find(|line| {
                let trimmed = line.trim_start();
                !trimmed.starts_with("//") && !trimmed.is_empty()
            })
            .map(anchor_key);

        // Extract trailing comment block
        let mut trailing_block: Vec<String> = Vec::new();
        for line in original.lines().rev() {
            if line.trim_start().starts_with("//") || line.trim().is_empty() {
                trailing_block.push(line.to_string());
            } else {
                break;
            }
        }
        trailing_block.reverse();

        // Build maps of inline and block comments keyed by anchor
        let mut map_inline: HashMap<String, String> = HashMap::new();
        let mut map_block: HashMap<String, VecDeque<Vec<String>>> = HashMap::new();
        let mut pending_block: Vec<String> = Vec::new();
        for line in original.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.is_empty() {
                // Accumulate standalone comment lines
                pending_block.push(line.to_string());
                continue;
            }
            // Code line: attach pending block (if any)
            let key = anchor_key(line);
            if !pending_block.is_empty() {
                let block = std::mem::take(&mut pending_block);
                map_block
                    .entry(key.clone())
                    .or_insert_with(VecDeque::new)
                    .push_back(block);
            }
            // Capture inline comment (if any)
            if let Some(idx) = line.find("//") {
                let inline = &line[idx..];
                map_inline.insert(key, inline.to_string());
            }
        }
        // Any remaining pending_block becomes trailing block (handled above already)
        if !pending_block.is_empty() && trailing_block.is_empty() {
            trailing_block = std::mem::take(&mut pending_block);
        }

        // If we already captured a leading block and know the first anchor, drop its pending entry to avoid duplicates
        if !leading_block.is_empty() {
            if let Some(key) = first_code_key {
                let mut remove_key = false;
                if let Some(blocks) = map_block.get_mut(&key) {
                    let _ = blocks.pop_front();
                    remove_key = blocks.is_empty();
                }
                if remove_key {
                    map_block.remove(&key);
                }
            }
        }

        // Precompute whether regenerated already includes captured leading/trailing blocks
        let regen_lines: Vec<&str> = regenerated.lines().collect();
        let leading_block_present = !leading_block.is_empty()
            && leading_block.len() <= regen_lines.len()
            && leading_block
                .iter()
                .zip(regen_lines.iter())
                .all(|(expected, actual)| expected == actual);
        let trailing_block_present = !trailing_block.is_empty()
            && trailing_block.len() <= regen_lines.len()
            && trailing_block
                .iter()
                .rev()
                .zip(regen_lines.iter().rev())
                .all(|(expected, actual)| expected == actual);

        // Build merged output by walking regenerated
        let mut out = String::new();
        let mut inserted_leading = false;
        for (i, line) in regenerated.lines().enumerate() {
            if i == 0 && !inserted_leading && !leading_block.is_empty() && !leading_block_present {
                for l in &leading_block { out.push_str(l); out.push('\n'); }
                inserted_leading = true;
            }
            let key = anchor_key(line);
            if !key.is_empty() {
                let mut remove_key = false;
                if let Some(blocks) = map_block.get_mut(&key) {
                    if let Some(block) = blocks.pop_front() {
                        for l in &block { out.push_str(l); out.push('\n'); }
                    }
                    remove_key = blocks.is_empty();
                }
                if remove_key {
                    map_block.remove(&key);
                }
                if let Some(inl) = map_inline.get(&key) {
                    if line.contains("//") {
                        out.push_str(line);
                        out.push('\n');
                    } else {
                        out.push_str(line);
                        if !line.ends_with(' ') { out.push(' '); }
                        out.push_str(inl);
                        out.push('\n');
                    }
                    continue;
                }
            }
            out.push_str(line);
            out.push('\n');
        }

        // Append trailing block, if any
        if !trailing_block.is_empty() && !trailing_block_present {
            if !out.ends_with('\n') { out.push('\n'); }
            for l in &trailing_block { out.push_str(l); out.push('\n'); }
        }
        out
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
    fn preserves_comment_inside_empty_block_entry() {
        let original = r#"list FinalTemplate (entry=<ExtendedTemplate>) {
    - {
        // Should inherit BaseTemplate parameters through ExtendedTemplate
    }
}
"#;
        let regenerated = r#"list FinalTemplate (entry=<ExtendedTemplate>) {
    - {
    }
}
"#;
        let merged = OverseerFileHandler::merge_comments(original, regenerated);
        assert!(
            merged.contains("// Should inherit BaseTemplate parameters through ExtendedTemplate"),
            "Expected comment inside empty list entry to be preserved. Got:\n{}",
            merged
        );
    }

    #[test]
    fn preserves_chart_comment_before_plot_line() {
    let original = r##"chart C {
    plot P1 (color="#ff0000", label="A", source=$(foo), x=$(bar), y=$(baz))
    // P3: cumulative average over all exercises per day for each point's day
    plot P3 (color="#b42c22ff", label="Exercises", source=$(/drum_tracker/History.filter(|x| x/eid == 98) ), x=$(|t| t/time), y=$(|t| /drum_tracker/History.filter(|x| same_day(x/time, t/time)&&(x/eid!=33 && x/eid!=31 )).average(|x| x/avg_points)))
    plot P3 (color="#792eabff", label="Syncopated", source=$(/drum_tracker/History.filter(|x| x/eid == 80) ), x=$(|t| t/time), y=$(|t| /drum_tracker/History.filter(|x| same_day(x/time, t/time)&&(x/eid==80||x/eid==84)).average(|x| x/avg_points)))
}
"##;
    let regenerated = r##"chart C {
    plot P1 (color="#ff0000", label="A", source=$(foo), x=$(bar), y=$(baz))
    plot P3 (color="#2eab35ff", label="Exercises", source=$(/drum_tracker/History.filter(|x| x/eid == 98)), x=$(|t| t/time), y=$(|t| /drum_tracker/History.filter(|x| same_day(x/time, t/time)&&(x/eid!=33 && x/eid!=31 )).average(|x| x/avg_points)))
    plot P3 (color="#792eabff", label="Syncopated", source=$(/drum_tracker/History.filter(|x| x/eid == 80)), x=$(|t| t/time), y=$(|t| /drum_tracker/History.filter(|x| same_day(x/time, t/time)&&(x/eid==80||x/eid==84)).average(|x| x/avg_points)))
}
"##;
        let merged = OverseerFileHandler::merge_comments(original, regenerated);
        assert!(
            merged.contains("// P3: cumulative average over all exercises per day for each point's day"),
            "Expected chart comment to be preserved. Got:\n{}",
            merged
        );
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
