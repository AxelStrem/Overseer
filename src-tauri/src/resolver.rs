use crate::types::{OverseerNode, OverseerValue};
use std::collections::HashMap;

// Debug logging macro for resolver
macro_rules! debug_resolver {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-resolver")]
        println!($($arg)*);
    };
}

/// Recursively finds all nodes marked as templates (`hidden=true`) and adds them to the map.
fn find_all_templates(nodes: &[OverseerNode], templates: &mut HashMap<String, OverseerNode>) {
    for node in nodes {
        // A template is a div marked as hidden. We use its name as the key.
        if node.node_type == "div" && node.parameters.get("hidden") == Some(&OverseerValue::Boolean(true)) {
            if !node.name.is_empty() {
                debug_resolver!("[RESOLVER] Found template: {} with {} children", node.name, node.children.len());
                for child in &node.children {
                    debug_resolver!("[RESOLVER]   Template field: {} (type: {})", child.name, child.node_type);
                }
                templates.insert(node.name.clone(), node.clone());
            }
        }
        // Recurse into children to find nested templates.
        if !node.children.is_empty() {
            find_all_templates(&node.children, templates);
        }
    }
}

fn build_template_map(nodes: &[OverseerNode]) -> HashMap<String, OverseerNode> {
    let mut templates = HashMap::new();
    debug_resolver!("[RESOLVER] Building template map from {} root nodes", nodes.len());
    find_all_templates(nodes, &mut templates);
    debug_resolver!("[RESOLVER] Template map built with {} templates: {:?}", templates.len(), templates.keys().collect::<Vec<_>>());
    templates
}

/// Public entry point to resolve all templates in a document AST.
pub fn resolve_document(nodes: &mut Vec<OverseerNode>) {
    // Build a map of all available templates by traversing the entire document.
    let templates = build_template_map(nodes);

    // Start the recursive resolution process.
    for node in nodes.iter_mut() {
        resolve_node(node, &templates);
    }
    
    // After template resolution, resolve layout parameters
    resolve_layout_parameters(nodes, None);
}

/// Resolves layout parameters for all nodes, calculating effective layout based on parent and parameter values
fn resolve_layout_parameters(nodes: &mut Vec<OverseerNode>, parent_layout: Option<&str>) {
    for node in nodes.iter_mut() {
        // Only div and list nodes support layout
        if node.node_type == "div" || node.node_type == "list" {
            let effective_layout = calculate_effective_layout(node, parent_layout);
            
            // Store the calculated layout in parameters for the renderer to use
            node.parameters.insert("_effective_layout".to_string(), OverseerValue::String(effective_layout.clone()));
            debug_resolver!("[RESOLVER] Node {} effective layout: {}", node.name, effective_layout);
            
            // Recursively resolve children with this node's effective layout
            if !node.children.is_empty() {
                resolve_layout_parameters(&mut node.children, Some(&effective_layout));
            }
        } else {
            // For non-container nodes, just pass through the parent layout to children
            if !node.children.is_empty() {
                resolve_layout_parameters(&mut node.children, parent_layout);
            }
        }
    }
}

/// Calculate the effective layout for a node based on its layout parameter and parent layout
fn calculate_effective_layout(node: &OverseerNode, parent_layout: Option<&str>) -> String {
    // Check if node has explicit layout parameter
    if let Some(layout_param) = node.parameters.get("layout") {
        if let OverseerValue::String(layout_value) = layout_param {
            match layout_value.as_str() {
                "vertical" => return "vertical".to_string(),
                "horizontal" => return "horizontal".to_string(),
                "inherit" => {
                    return parent_layout.unwrap_or("vertical").to_string();
                },
                "opposite" => {
                    return match parent_layout.unwrap_or("vertical") {
                        "vertical" => "horizontal".to_string(),
                        "horizontal" => "vertical".to_string(),
                        _ => "horizontal".to_string(), // Default opposite of vertical
                    };
                },
                _ => {
                    debug_resolver!("[RESOLVER] Unknown layout value: {}, defaulting to opposite", layout_value);
                }
            }
        }
    }
    
    // Default behavior: opposite to parent (or horizontal if no parent)
    match parent_layout.unwrap_or("vertical") {
        "vertical" => "horizontal".to_string(),
        "horizontal" => "vertical".to_string(),
        _ => "horizontal".to_string(),
    }
}

/// Recursively traverses the AST, resolving templates as it goes.
fn resolve_node(node: &mut OverseerNode, templates: &HashMap<String, OverseerNode>) {
    debug_resolver!("[RESOLVER] Resolving node: {} (type: {})", node.name, node.node_type);
    
    // Check if the current node is a list that uses a template or simple type.
    if node.node_type == "list" {
        if let Some(entry_value) = node.parameters.get("entry") {
            match entry_value {
                OverseerValue::Template(template_path) => {
                    debug_resolver!("[RESOLVER] List {} uses template: {}", node.name, template_path);
                    // Simplified path resolution: "entry=<../Task>" -> "Task"
                    let template_name = template_path.split('/').last().unwrap_or("");
                    debug_resolver!("[RESOLVER] Resolved template name: {}", template_name);

                    if let Some(template_node) = templates.get(template_name) {
                        debug_resolver!("[RESOLVER] Found template node for {}, processing {} children", template_name, node.children.len());
                        let mut resolved_children = Vec::new();
                        for (i, list_item) in node.children.iter().enumerate() {
                            debug_resolver!("[RESOLVER]   Processing list item {}: {} (type: {})", i, list_item.name, list_item.node_type);
                            // Handle both old "list_item" type and new "-" type (after parse_list_item removal)
                            if list_item.node_type == "list_item" || (list_item.node_type == "-" && !list_item.children.is_empty()) {
                                if !list_item.children.is_empty() {
                                    debug_resolver!("[RESOLVER]     Complex list item with {} children", list_item.children.len());
                                    // Complex list item: create a node of the template's type
                                    let mut resolved_item = OverseerNode {
                                        name: list_item.name.clone(),
                                        node_type: template_node.node_type.clone(),
                                        template: None,
                                        parameters: list_item.parameters.clone(),
                                        children: template_node.children.clone(),
                                        is_hierarchy_transparent: template_node.is_hierarchy_transparent,
                                    };
                                    let overrides: HashMap<String, &OverseerNode> = list_item
                                        .children
                                        .iter()
                                        .map(|o| (o.name.clone(), o))
                                        .collect();
                                    debug_resolver!("[RESOLVER]     Override fields: {:?}", overrides.keys().collect::<Vec<_>>());
                                    merge_node(&mut resolved_item, &overrides);
                                    // Infer types for '-' children from template fields
                                    for child in resolved_item.children.iter_mut() {
                                        if child.node_type == "-" {
                                            if let Some(template_field) = template_node.children.iter().find(|f| f.name == child.name) {
                                                debug_resolver!("[RESOLVER]     Resolving '-' type for {}: {} -> {}", child.name, child.node_type, template_field.node_type);
                                                child.node_type = template_field.node_type.clone();
                                            } else {
                                                debug_resolver!("[RESOLVER]     Warning: No template field found for '-' type: {}", child.name);
                                            }
                                        }
                                    }
                                    resolved_children.push(resolved_item);
                                } else if let Some(val) = list_item.parameters.get("value") {
                                    debug_resolver!("[RESOLVER]     Simple value list item: {:?}", val);
                                    // Simple value: create a node of the template's type, with value
                                    let mut resolved_item = OverseerNode {
                                        name: list_item.name.clone(),
                                        node_type: template_node.node_type.clone(),
                                        template: None,
                                        parameters: HashMap::new(),
                                        children: Vec::new(),
                                        is_hierarchy_transparent: template_node.is_hierarchy_transparent,
                                    };
                                    resolved_item.parameters.insert("value".to_string(), val.clone());
                                    resolved_children.push(resolved_item);
                                } else {
                                    debug_resolver!("[RESOLVER]     Fallback: cloning list item as-is");
                                    // Fallback: just clone
                                    resolved_children.push(list_item.clone());
                                }
                            } else {
                                debug_resolver!("[RESOLVER]   Not a complex list_item, passing through: {} (type: {})", list_item.name, list_item.node_type);
                                // Not a complex list_item, pass through
                                resolved_children.push(list_item.clone());
                            }
                        }
                        debug_resolver!("[RESOLVER] List resolution complete, {} -> {} children", node.children.len(), resolved_children.len());
                        node.children = resolved_children;
                    } else {
                        debug_resolver!("[RESOLVER] Warning: Template not found: {}", template_name);
                    }
                },
                OverseerValue::String(type_name) => {
                    debug_resolver!("[RESOLVER] List {} uses simple type: {}", node.name, type_name);
                    // Handle simple type entries like entry=string
                    let mut resolved_children = Vec::new();
                    for (i, list_item) in node.children.iter().enumerate() {
                        debug_resolver!("[RESOLVER]   Processing simple type list item {}: {} (type: {})", i, list_item.name, list_item.node_type);
                        if let Some(val) = list_item.parameters.get("value") {
                            debug_resolver!("[RESOLVER]     Converting to {} with value: {:?}", type_name, val);
                            // Create a node of the specified simple type
                            let mut resolved_item = OverseerNode {
                                name: list_item.name.clone(),
                                node_type: type_name.clone(),
                                template: None,
                                parameters: HashMap::new(),
                                children: Vec::new(),
                                is_hierarchy_transparent: false,
                            };
                            resolved_item.parameters.insert("value".to_string(), val.clone());
                            resolved_children.push(resolved_item);
                        } else {
                            debug_resolver!("[RESOLVER]     No value found, keeping as-is: {} (type: {})", list_item.name, list_item.node_type);
                            resolved_children.push(list_item.clone());
                        }
                    }
                    debug_resolver!("[RESOLVER] Simple type list resolution complete, {} -> {} children", node.children.len(), resolved_children.len());
                    node.children = resolved_children;
                },
                _ => {
                    debug_resolver!("[RESOLVER] List {} has unsupported entry parameter type: {:?}", node.name, entry_value);
                }
            }
        } else {
            debug_resolver!("[RESOLVER] List {} has no entry parameter", node.name);
        }
    }

    // If this node is an instantiated template, infer types for '-' children from the template
    if let Some(template_path) = &node.template {
        debug_resolver!("[RESOLVER] Node {} has template: {}", node.name, template_path);
        let template_name = template_path.split('/').last().unwrap_or("");
        if let Some(template_node) = templates.get(template_name) {
            debug_resolver!("[RESOLVER] Resolving template fields for {}", template_name);
            for child in node.children.iter_mut() {
                if child.node_type == "-" {
                    if let Some(template_field) = template_node.children.iter().find(|f| f.name == child.name) {
                        debug_resolver!("[RESOLVER] Resolving template field {}: {} -> {}", child.name, child.node_type, template_field.node_type);
                        child.node_type = template_field.node_type.clone();
                    } else {
                        debug_resolver!("[RESOLVER] Warning: No template field found for {}", child.name);
                    }
                }
            }
        } else {
            debug_resolver!("[RESOLVER] Warning: Template not found for node: {}", template_name);
        }
    }

    // After processing the current node (e.g., resolving a list), recurse into
    // the children. This is a pre-order traversal, which is correct for this
    // problem because it resolves containers before their contents.
    for child in node.children.iter_mut() {
        resolve_node(child, templates);
    }
    
    debug_resolver!("[RESOLVER] Finished resolving node: {} (final type: {})", node.name, node.node_type);
}

/// Merges override fields into a template clone.
fn merge_node(template: &mut OverseerNode, overrides: &HashMap<String, &OverseerNode>) {
    debug_resolver!("[RESOLVER] Merging overrides into template with {} fields", template.children.len());
    for template_field in template.children.iter_mut() {
        if let Some(override_field) = overrides.get(&template_field.name) {
            debug_resolver!("[RESOLVER]   Merging field: {} (template type: {}, override type: {})", 
                    template_field.name, template_field.node_type, override_field.node_type);
            // Always preserve the node_type from the template
            // (do NOT overwrite with the override's name or type)
            // Only override value and children as appropriate

            // Override a simple value (e.g., name = "...")
            if let Some(val) = override_field.parameters.get("value") {
                debug_resolver!("[RESOLVER]     Setting value: {:?}", val);
                template_field.parameters.insert("value".to_string(), val.clone());
            }
            // If this field is a list, handle entry inheritance and recursive merge
            if template_field.node_type == "list" {
                // If override does not specify entry, inherit from template
                if !override_field.parameters.contains_key("entry") {
                    if let Some(entry) = template_field.parameters.get("entry") {
                        template_field.parameters.insert("entry".to_string(), entry.clone());
                    }
                } else {
                    // If override specifies entry, use it
                    if let Some(entry) = override_field.parameters.get("entry") {
                        template_field.parameters.insert("entry".to_string(), entry.clone());
                    }
                }
                // Recursively resolve/merge children for nested lists
                if !override_field.children.is_empty() {
                    template_field.children = override_field.children.clone();
                }
            } else if !override_field.children.is_empty() {
                // For non-list fields, just override children
                template_field.children = override_field.children.clone();
            }
            // Ensure node_type is preserved from template (do not overwrite)
            // (No action needed, as we never assign node_type from override)
        }
    }
}
