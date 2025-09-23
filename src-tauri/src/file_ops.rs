use std::collections::{HashMap, HashSet};
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
        Self::serialize_node_context(node, output, indent_level, false)
    }

    fn serialize_node_context(
        node: &OverseerNode,
        output: &mut String,
        indent_level: usize,
        in_list_body: bool,
    ) -> Result<()> {
        let indent = "    ".repeat(indent_level); // Use 4 spaces for indentation

        // Global early handling for template-derived simple leaf nodes at ANY depth:
        // If a node is template-derived and has only a value override (or matches template and is not explicit),
        // either emit the concise "- name = value" form (if changed/explicit) or skip entirely (if pure template).
        // This prevents materializing typed field lines like "float weight = 300" where the original source had
        // a concise override line inside nested blocks (e.g., per_item { - weight = 300 }).
        let is_template_child_flag = matches!(
            node.parameters.get("_template_node"),
            Some(OverseerValue::Boolean(true))
        );
        let has_template_param_markers = node
            .parameters
            .keys()
            .any(|k| k.starts_with("_template_"));
        let is_template_child_any_depth = is_template_child_flag || has_template_param_markers;
        if is_template_child_any_depth && !in_list_body {
            let has_value = node.parameters.contains_key("value");
            if has_value {
                let non_internal_non_value_params = node
                    .parameters
                    .iter()
                    .filter(|(k, _)| {
                        let ks = k.as_str();
                        if ks.starts_with('_') || ks == "value" { return false; }
                        // Ignore params that are template-derived (paired marker exists)
                        let marker = format!("_template_{}", ks);
                        !node.parameters.contains_key(&marker)
                    })
                    .count();
                let only_value_override = non_internal_non_value_params == 0 && node.children.is_empty();
                if only_value_override {
                    let explicit = matches!(
                        node.parameters.get("_explicit_child_override"),
                        Some(OverseerValue::Boolean(true))
                    ) || matches!(
                        node.parameters.get("_override_present"),
                        Some(OverseerValue::Boolean(true))
                    );
                    let value = node.parameters.get("value").unwrap();
                    let template_value_opt = node.parameters.get("_template_value");
                    let differs_from_template = match template_value_opt {
                        Some(tv) => tv != value,
                        None => true, // no marker => treat as meaningful
                    };
                    if !explicit && !differs_from_template {
                        // Pure template leaf untouched: skip emission entirely
                        return Ok(());
                    }
                    // Emit concise override form
                    output.push_str(&indent);
                    output.push_str("- ");
                    output.push_str(&node.name);
                    output.push_str(" = ");
                    output.push_str(&Self::serialize_value_with_node(node, value));
                    output.push('\n');
                    return Ok(());
                }
            }
        }

        // Default path continues with normal emission; write indent now.
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
                                    "{}    - {} = {}\n",
                                    indent,
                                    n,
                                    Self::serialize_value_with_node(child, v)
                                ));
                            }
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
                                output.push_str(&format!(
                                    "{}    - {} = {}\n",
                                    indent,
                                    child.name,
                                    Self::serialize_value_with_node(child, val)
                                ));
                                continue;
                            }
                        }
                    }
                    // Fallback: serialize child normally inside the block (not as list body)
                    // so field lines and nested blocks render correctly.
                    Self::serialize_node_context(child, output, indent_level + 1, false)?;
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
                        Self::serialize_node_context(child, output, indent_level + 1, false)?;
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
                                "{}    - {} = {}\n",
                                indent,
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
                            "{}    - {} = {}\n",
                            indent,
                            child.name,
                            Self::serialize_value_with_node(child, val)
                        ));
                        continue;
                    }
                }
                Self::serialize_node_context(child, output, indent_level + 1, children_in_list_body)?;
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
            code.chars().filter(|c| !c.is_whitespace()).collect::<String>()
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
        let mut map_block: HashMap<String, Vec<String>> = HashMap::new();
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
                map_block.insert(key.clone(), std::mem::take(&mut pending_block));
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

        // Augment maps with relaxed/root keys so formatting / param order changes still match
        // Root key heuristic: take existing anchor key up to first '(' or '{' or '='.
        let mut extra_block_entries: Vec<(String, Vec<String>)> = Vec::new();
        let mut extra_inline_entries: Vec<(String, String)> = Vec::new();
        for (k, v) in map_block.iter() {
            let root = k.split(|c| c == '(' || c == '{' || c == '=').next().unwrap_or("").to_string();
            if !root.is_empty() && root != *k && !map_block.contains_key(&root) {
                extra_block_entries.push((root, v.clone()));
            }
        }
        for (k, v) in map_inline.iter() {
            let root = k.split(|c| c == '(' || c == '{' || c == '=').next().unwrap_or("").to_string();
            if !root.is_empty() && root != *k && !map_inline.contains_key(&root) {
                extra_inline_entries.push((root, v.clone()));
            }
        }
        for (k,v) in extra_block_entries { map_block.insert(k, v); }
        for (k,v) in extra_inline_entries { map_inline.insert(k, v); }

        // Build merged output by walking regenerated
        let mut out = String::new();
        let mut inserted_leading = false;
        let mut used_blocks: HashSet<String> = HashSet::new();
        // Precompute relaxed anchor variants for regenerated lines to better match nodes whose definition line formatting changed
        // Relaxation strategy: drop anything after first '(' or '{' or 'link=' style param to stabilize anchor across param injection/stripping.
        fn relaxed_anchor(base: &str) -> String { 
            let mut s = base.to_string();
            for sep in ["(", "{", "link="] { if let Some(idx) = s.find(sep) { s = s[..idx].to_string(); break; } }
            s
        }
        for (i, line) in regenerated.lines().enumerate() {
            if i == 0 && !inserted_leading && !leading_block.is_empty() {
                for l in &leading_block { out.push_str(l); out.push('\n'); }
                inserted_leading = true;
            }
            let key = anchor_key(line);
            let relaxed_key = relaxed_anchor(&key);
            if !key.is_empty() {
                // Try exact key
                let mut block_opt = map_block.get(&key);
                // Fallback: try relaxed key if different
                if block_opt.is_none() && relaxed_key != key { block_opt = map_block.get(&relaxed_key); }
                if let Some(block) = block_opt {
                    if !used_blocks.contains(&key) {
                        for l in block { out.push_str(l); out.push('\n'); }
                        used_blocks.insert(key.clone());
                    }
                }
                let mut inline_opt = map_inline.get(&key);
                if inline_opt.is_none() && relaxed_key != key { inline_opt = map_inline.get(&relaxed_key); }
                if let Some(inl) = inline_opt {
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
        if !trailing_block.is_empty() {
            if !out.ends_with('\n') { out.push('\n'); }
            for l in &trailing_block { out.push_str(l); out.push('\n'); }
        }
        // Salvage pass disabled (previous version caused duplicated top-of-file comment blocks when anchors changed repeatedly).
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
"##;

        let (_rem, mut nodes) = crate::parser::parse_document(original).expect("parse weight_tracker_new");
        crate::resolver::resolve_document(&mut nodes);
        let serialized = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");

        fn canon(s: &str) -> String { s.lines().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n") }
        let orig_c = canon(original);
        let ser_c = canon(&serialized);

        assert_eq!(ser_c, orig_c, "Round-trip serialization for weight_tracker_new is not idempotent. Differs after resolve.\n--- ORIGINAL ---\n{}\n--- SERIALIZED ---\n{}", orig_c, ser_c);
    }
}
