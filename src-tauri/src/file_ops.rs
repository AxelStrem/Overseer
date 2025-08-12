use std::path::Path;
use std::collections::HashMap;
use tokio::fs;
use crate::types::*;

// Debug logging macro for serializer (module scope). Reuse existing 'debug-resolver' feature to avoid adding new Cargo feature.
macro_rules! debug_serializer {
    ($($arg:tt)*) => {
    #[cfg(feature = "debug-resolver")]
        eprintln!($($arg)*);
    };
}

pub struct FileOperations;

impl FileOperations {
    pub async fn read_file(path: &str) -> Result<String> {
        match fs::read_to_string(path).await {
            Ok(content) => Ok(content),
            Err(e) => Err(OverseerError::IoError(format!("Failed to read file '{}': {}", path, e))),
        }
    }

    pub async fn write_file(path: &str, content: &str) -> Result<()> {
        // Create parent directories if they don't exist
        if let Some(parent) = Path::new(path).parent() {
            if let Err(e) = fs::create_dir_all(parent).await {
                return Err(OverseerError::IoError(format!("Failed to create directories for '{}': {}", path, e)));
            }
        }

        match fs::write(path, content).await {
            Ok(_) => Ok(()),
            Err(e) => Err(OverseerError::IoError(format!("Failed to write file '{}': {}", path, e))),
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
                Err(e) => Err(OverseerError::IoError(format!("Failed to create backup of '{}': {}", path, e))),
            }
        } else {
            Err(OverseerError::IoError(format!("Cannot backup file '{}': file does not exist", path)))
        }
    }

    pub fn validate_file_path(path: &str) -> Result<()> {
        if path.is_empty() {
            return Err(OverseerError::IoError("File path cannot be empty".to_string()));
        }

        // Check for invalid characters (Windows specific)
        let invalid_chars = ['<', '>', ':', '"', '|', '?', '*'];
        if path.chars().any(|c| invalid_chars.contains(&c)) {
            return Err(OverseerError::IoError(format!("File path contains invalid characters: {}", path)));
        }

        // Check if path is too long (Windows limit is 260 characters)
        if path.len() > 260 {
            return Err(OverseerError::IoError("File path is too long".to_string()));
        }

        Ok(())
    }
}

pub struct OverseerFileHandler;

impl OverseerFileHandler {
    pub async fn load_overseer_file(path: &str) -> Result<Vec<OverseerNode>> {
        FileOperations::validate_file_path(path)?;
        
        let content = FileOperations::read_file(path).await?;
        
        // Parse the content using our parser
        match crate::parser::parse_document(&content) {
            Ok((_, nodes)) => Ok(nodes),
            Err(e) => Err(OverseerError::ParseError(format!("Failed to parse Overseer file: {:?}", e))),
        }
    }

    /// Merge comments and some whitespace from the original content into the regenerated content.
    /// Strategy:
    /// - Capture leading comment block at file start.
    /// - For each non-comment line in original, associate any immediately preceding contiguous
    ///   comment lines (//...) as that line's leading comment block. Also capture any inline
    ///   end-of-line comment on the anchor line itself.
    /// - When emitting regenerated lines, insert the captured leading block at the top, and for
    ///   each matching anchor line (by text before //, trimmed), insert the associated comment
    ///   block above and append the inline comment if present and not already present.
    /// Notes:
    /// - Best-effort only; if anchors don't match (content shifted/changed), comments may be dropped.
    /// - This function is pure text-based and does not require AST changes.
    pub fn merge_comments(original: &str, regenerated: &str) -> String {
        use std::collections::{HashMap, HashSet};
        fn anchor_key(line: &str) -> String {
            let mut s = line.to_string();
            if let Some(idx) = s.find("//") {
                s.truncate(idx);
            }
            // Normalize whitespace differences so anchors match even if formatting changes:
            // 1) Collapse all runs of whitespace to a single space
            // 2) Remove any single space immediately before '(' to treat "Name(" and "Name (" the same
            let mut norm = String::with_capacity(s.len());
            let mut prev_space = false;
            for ch in s.chars() {
                if ch.is_whitespace() {
                    if !prev_space {
                        norm.push(' ');
                        prev_space = true;
                    }
                } else {
                    norm.push(ch);
                    prev_space = false;
                }
            }
            // Trim trailing spaces
            while norm.ends_with(' ') { norm.pop(); }
            // Remove spaces before '(' characters
            let norm = norm.replace(" (", "(");
            norm
        }

        let mut leading_block: Vec<String> = Vec::new();
        let mut map_block: HashMap<String, Vec<String>> = HashMap::new();
        let mut map_inline: HashMap<String, String> = HashMap::new();
        let mut pending_block: Vec<String> = Vec::new();
        let mut seen_non_comment = false;

        for line in original.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                if !seen_non_comment {
                    leading_block.push(line.to_string());
                } else {
                    pending_block.push(line.to_string());
                }
                continue;
            }

            // Non-comment line (could be blank or code)
            if trimmed.is_empty() {
                // Treat as spacer; keep it inside pending block only if we already started a block
                if !pending_block.is_empty() {
                    pending_block.push(line.to_string());
                } else if !seen_non_comment && !leading_block.is_empty() {
                    // Preserve a single blank after leading block
                    leading_block.push(line.to_string());
                }
                continue;
            }

            // Now an anchor line
            let key = anchor_key(line);
            if !pending_block.is_empty() && !key.is_empty() && !map_block.contains_key(&key) {
                map_block.insert(key.clone(), std::mem::take(&mut pending_block));
            } else {
                pending_block.clear();
            }
            // Capture inline comment if present and not yet set
            if let Some(idx) = line.find("//") {
                let inline = &line[idx..];
                if !key.is_empty() && !map_inline.contains_key(&key) {
                    map_inline.insert(key.clone(), inline.to_string());
                }
            }
            seen_non_comment = true;
        }

        // Build merged output
        let mut out = String::new();
        let mut inserted_leading = false;
        let mut used_blocks: HashSet<String> = HashSet::new();

        for (i, line) in regenerated.lines().enumerate() {
            if i == 0 && !inserted_leading && !leading_block.is_empty() {
                for l in &leading_block { out.push_str(l); out.push('\n'); }
                inserted_leading = true;
            }
            let key = anchor_key(line);
            if !key.is_empty() {
                if let Some(block) = map_block.get(&key) {
                    if !used_blocks.contains(&key) {
                        for l in block { out.push_str(l); out.push('\n'); }
                        used_blocks.insert(key.clone());
                    }
                }
                // Append line, possibly with inline comment
                if let Some(inl) = map_inline.get(&key) {
                    if line.contains("//") {
                        // already has comment; just write as-is
                        out.push_str(line);
                        out.push('\n');
                    } else {
                        // add a space before inline for readability
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

        out
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
    
    fn serialize_node_context(node: &OverseerNode, output: &mut String, indent_level: usize, in_list_body: bool) -> Result<()> {
        let indent = "    ".repeat(indent_level); // Use 4 spaces for indentation
        output.push_str(&indent);
        
        // Handle list-style items which start with '-'
        if in_list_body || node.node_type == "list_item" || (indent_level > 0 && node.node_type == "-") {
            // Special handling for real list bodies vs non-list contexts
            if in_list_body {
                // Determine if this is a simple primitive list item (only when not template-derived)
                let is_template_instance = node.template.is_some() || matches!(node.parameters.get("_from_template"), Some(OverseerValue::Boolean(true)));
                if let Some(value) = node.parameters.get("value") {
                    // Only emit as simple value when NOT a template instance (true primitive lists)
                    if !is_template_instance {
                        output.push_str("- ");
                        output.push_str(&Self::serialize_value(value));
                        output.push('\n');
                        return Ok(());
                    }
                }
                // Complex/template-based item: emit as "- { ... }" and use the standard child emission (with concise override rules)
                output.push_str("- {\n");
                // Suppress template-derived children for instances and emit concise overrides
                let suppress_template_children = node.template.is_some() || matches!(node.parameters.get("_from_template"), Some(OverseerValue::Boolean(true)));
                for child in &node.children {
                    let is_template_child_flag = matches!(child.parameters.get("_template_node"), Some(OverseerValue::Boolean(true)));
                    let has_template_param_markers = child.parameters.keys().any(|k| k.starts_with("_template_"));
                    let is_template_child = is_template_child_flag || has_template_param_markers;
                    let has_explicit_override = matches!(child.parameters.get("_explicit_child_override"), Some(OverseerValue::Boolean(true)));
                    let mut listed_in_instance_overrides = true;
                    if suppress_template_children {
                        if let Some(OverseerValue::String(list)) = node.parameters.get("_explicit_overrides") {
                            let names: Vec<&str> = list.split(',').filter(|s| !s.is_empty()).collect();
                            listed_in_instance_overrides = names.iter().any(|n| *n == child.name);
                        }
                    }
                    if suppress_template_children && is_template_child && (!has_explicit_override || !listed_in_instance_overrides) {
                        continue;
                    }
                    if suppress_template_children && has_explicit_override && listed_in_instance_overrides {
                        let has_value = child.parameters.contains_key("value");
                        let non_internal_non_value_params = child.parameters.iter().filter(|(k, _)| {
                            let ks = k.as_str();
                            !ks.starts_with('_') && ks != "value"
                        }).count();
                        let only_value_override = has_value && non_internal_non_value_params == 0 && child.children.is_empty();
                        let has_template_value_marker = child.parameters.contains_key("_template_value");
                        if only_value_override && !has_template_value_marker {
                            let val = child.parameters.get("value").unwrap();
                            output.push_str(&format!("{}    - {} = {}\n", indent, child.name, Self::serialize_value(val)));
                            continue;
                        }
                    }
                    // Fallback: serialize child normally inside the block
                    Self::serialize_node_context(child, output, indent_level + 1, false)?;
                }
                output.push_str(&format!("{}}}\n", indent));
                return Ok(());
            } else {
                // Non-list context override: allow "- name = value" syntax
                if let Some(value) = node.parameters.get("value") {
                    if !node.name.is_empty() {
                        output.push_str("- ");
                        output.push_str(&node.name);
                        output.push_str(" = ");
                        output.push_str(&Self::serialize_value(value));
                        output.push('\n');
                        return Ok(());
                    }
                }
                // Else: fall through to normal serialization
            }
        } else {
            // For children of list items, always use "-" even if the type was resolved
            if in_list_body {
                // Check if this node had its type resolved from "-" and restore original
                if let Some(OverseerValue::String(original_type)) = node.parameters.get("_original_type") {
                    if original_type == "-" {
                        output.push('-');
                    } else {
                        output.push_str(original_type);
                    }
                } else {
                    output.push('-');
                }
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
        let regular_params: HashMap<String, OverseerValue> = node.parameters.iter()
            .filter(|(k, _)| {
                let key = k.as_str();
                // Always exclude 'value' parameter and internal computed parameters (except _template_ markers)
                if key == "value" || key == "_original_type" || (key.starts_with("_") && !key.starts_with("_template_")) {
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
            let params_str: Vec<String> = regular_params.iter()
                .map(|(k, v)| {
                    let value_str = if k.as_str() == "entry" {
                        // Special handling for entry parameters - they should be type names, not quoted strings
                        match v {
                            OverseerValue::String(s) => s.clone(), // Don't quote type names
                            OverseerValue::Template(t) => format!("<{}>", t),
                            _ => Self::serialize_value(&v)
                        }
                    } else {
                        Self::serialize_value(&v)
                    };
                    format!("{}={}", k, value_str)
                })
                .collect();
            output.push_str(&params_str.join(", "));
            output.push(')');
        }
        
        // Handle body (value assignment, block, or nothing)
        if let Some(value) = node.parameters.get("value") {
            output.push_str(&format!(" = {}\n", Self::serialize_value(value)));
        } else if node.children.is_empty() {
            output.push('\n');
        } else {
            output.push_str(" {\n");
            // Children under a list node are list-body items (render as '-')
            let children_in_list_body = node.node_type == "list";
            // Suppress template-derived children for any node that originated from a template (standalone instances or list entries)
            let suppress_template_children = node.template.is_some() || matches!(node.parameters.get("_from_template"), Some(OverseerValue::Boolean(true)));
            for child in &node.children {
                // Skip template-derived children unless they were explicitly overridden
                let is_template_child_flag = matches!(child.parameters.get("_template_node"), Some(OverseerValue::Boolean(true)));
                let has_template_param_markers = child.parameters.keys().any(|k| k.starts_with("_template_"));
                let is_template_child = is_template_child_flag || has_template_param_markers;
                // Only treat as includeable if it was explicitly overridden by the source, not just equal/diff logic
                let has_explicit_override = matches!(child.parameters.get("_explicit_child_override"), Some(OverseerValue::Boolean(true)));
                // Additional safety: in a template instance, only consider overrides that were explicitly named in the instance source
                let mut listed_in_instance_overrides = true;
                if suppress_template_children {
                    if let Some(OverseerValue::String(list)) = node.parameters.get("_explicit_overrides") {
                        let names: Vec<&str> = list.split(',').filter(|s| !s.is_empty()).collect();
                        listed_in_instance_overrides = names.iter().any(|n| *n == child.name);
                    }
                }
                if suppress_template_children && is_template_child && (!has_explicit_override || !listed_in_instance_overrides) {
                    continue;
                }
                // For template instances/clones, if a child was overridden with only a simple value, prefer the concise "- name = value" form
    if suppress_template_children && has_explicit_override && listed_in_instance_overrides {
                    let has_value = child.parameters.contains_key("value");
                    let non_internal_non_value_params = child.parameters.iter().filter(|(k, _)| {
                        let ks = k.as_str();
                        // allow 'value' only; ignore internal keys starting with '_'
                        !ks.starts_with('_') && ks != "value"
                    }).count();
                    let only_value_override = has_value && non_internal_non_value_params == 0 && child.children.is_empty();
                    // Guard: only treat as an explicit value override if the template value marker was removed.
                    let has_template_value_marker = child.parameters.contains_key("_template_value");
              if only_value_override && !has_template_value_marker {
        debug_serializer!("[SER] concise emit: name='{}' explicit={} tmpl_marker_removed={} suppress={} is_templ_child={} non_val_params={} has_val={}",
            child.name, has_explicit_override, !has_template_value_marker, suppress_template_children, is_template_child, non_internal_non_value_params, has_value);
                        let val = child.parameters.get("value").unwrap();
                        output.push_str(&format!("{}    - {} = {}\n", indent, child.name, Self::serialize_value(val)));
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
                    format!("solid {} {}", 
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
                },
                BorderStyle::Dashed(thickness, color) => {
                    format!("dashed {} {}", 
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
                },
                BorderStyle::Dotted(thickness, color) => {
                    format!("dotted {} {}", 
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
                },
            },
        }
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
}
