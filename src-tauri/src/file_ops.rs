use std::path::Path;
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
        let indent = "    ".repeat(indent_level); // Use 4 spaces for indentation
        output.push_str(&indent);
        
        // Handle special case for list items which start with '-'
        if node.node_type == "list_item" || (indent_level > 0 && node.node_type == "-") {
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
            // Handle node type or template path
            if let Some(template_path) = &node.template {
                output.push_str(&format!("<{}>", template_path));
            } else {
                // Use "-" for type-inferred nodes, otherwise use the actual type
                if node.node_type == "-" || (node.name == "-" && node.node_type != "list_item") {
                    output.push('-');
                } else {
                    output.push_str(&node.node_type);
                }
            }

            // Handle node name - skip if it's "-" (unnamed node marker)
            if !node.name.is_empty() && node.name != "-" {
                output.push(' ');
                output.push_str(&node.name);
            }
        }
        
        // Handle parameters (excluding the special 'value' parameter for fields)
        let regular_params: Vec<_> = node.parameters.iter().filter(|(k, _)| k.as_str() != "value").collect();
        if !regular_params.is_empty() {
            output.push_str(" (");
            let params_str: Vec<String> = regular_params.iter()
                .map(|(k, v)| {
                    let value_str = if k.as_str() == "entry" {
                        // Special handling for entry parameters - they should be type names, not quoted strings
                        match v {
                            OverseerValue::String(s) => s.clone(), // Don't quote type names
                            OverseerValue::Template(t) => format!("<{}>", t),
                            _ => Self::serialize_value(v)
                        }
                    } else {
                        Self::serialize_value(v)
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
            for child in &node.children {
                Self::serialize_node(child, output, indent_level + 1)?;
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
            OverseerValue::Formula(f) => format!("$({})", f),
            OverseerValue::Template(t) => format!("<{}>", t),
        }
    }
}
