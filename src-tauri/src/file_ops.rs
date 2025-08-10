use std::path::Path;
use std::collections::HashMap;
use tokio::fs;
use crate::types::*;

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
        
        // Handle list body items which start with '-'
        if in_list_body || node.node_type == "list_item" || (indent_level > 0 && node.node_type == "-") {
            output.push_str("- ");
            // Simple value list item: - "value"
            if let Some(value) = node.parameters.get("value") {
                output.push_str(&Self::serialize_value(value));
                output.push('\n');
                return Ok(());
            }
            // Complex object list item: - { ... }
            // Fall through to handle as block
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

            // Handle node name - skip if it's "-" (unnamed node marker)
            if !node.name.is_empty() && node.name != "-" {
                output.push(' ');
                output.push_str(&node.name);
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
            for child in &node.children {
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
