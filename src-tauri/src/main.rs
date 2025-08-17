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
    if path_parts.is_empty() {
        return None;
    }
    
    find_node_by_path(nodes, &path_parts).and_then(|node| {
        // Try to get the value from parameters
        node.parameters.get("value").cloned()
    })
}

// Helper function to set a field value by path
fn set_field_value_by_path(nodes: &mut [OverseerNode], path: &str, value: OverseerValue) -> std::result::Result<(), OverseerError> {
    let path_parts: Vec<&str> = path.split('/').collect();
    if path_parts.is_empty() {
        return Err(OverseerError::ValidationError("Empty path".to_string()));
    }
    
    if let Some(node) = find_node_by_path_mut(nodes, &path_parts) {
        // Set the value in parameters
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
async fn parse_overseer_content_selective(content: String, changed_fields: Vec<String>) -> Result<Vec<OverseerNode>> {
    println!("🔄 Selective update called with {} changed fields: {:?}", changed_fields.len(), changed_fields);
    
    match parse_document(&content) {
        Ok((_remaining, mut nodes)) => {
            // If no specific fields changed, do full resolution
            if changed_fields.is_empty() {
                println!("📋 No specific fields changed, performing full resolution");
                resolver::resolve_document(&mut nodes);
            } else {
                // Build dependency graph and do selective updates
                let mut dep_graph = DependencyGraph::new();
                if let Err(e) = dep_graph.build_from_document(&nodes) {
                    // If dependency tracking fails, fall back to full resolution
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
                    println!("📊 Dependency cascade calculated: {} fields need updates from {} changed fields", 
                             all_fields_to_update.len(), changed_fields.len());
                    
                    // For now, do full resolution but preserve the user's changes in the changed fields
                    // TODO: Implement true selective resolution in resolver module
                    println!("⚠️  Using full resolution with field preservation");
                    
                    // First, capture the current values of the changed fields before resolution
                    let mut preserved_values = std::collections::HashMap::new();
                    for changed_field in &changed_fields {
                        if let Some(value) = get_field_value_by_path(&nodes, changed_field) {
                            preserved_values.insert(changed_field.clone(), value.clone());
                            println!("💾 Preserving field '{}' with value: {:?}", changed_field, value);
                        } else {
                            println!("❌ Could not find field '{}' to preserve", changed_field);
                        }
                    }
                    
                    // Do full resolution
                    resolver::resolve_document(&mut nodes);
                    
                    // Restore the preserved values to the changed fields
                    for (field_path, value) in preserved_values {
                        if let Err(e) = set_field_value_by_path(&mut nodes, &field_path, value) {
                            println!("⚠️  Failed to restore field '{}': {:?}", field_path, e);
                        } else {
                            println!("✅ Restored field '{}'", field_path);
                        }
                    }
                }
            }
            println!("✅ Selective update completed");
            Ok(nodes)
        },
        Err(e) => {
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
    ActionExecutor::tick(&mut nodes)?;
    Ok(nodes)
}

#[command]
async fn get_next_timer_due_ms(nodes: Vec<OverseerNode>) -> Result<Option<i64>> {
    Ok(ActionExecutor::next_due_ms(&nodes))
}

fn main() {
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
