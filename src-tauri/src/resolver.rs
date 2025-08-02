use crate::types::{OverseerNode, OverseerValue};
use std::collections::HashMap;

/// Recursively finds all nodes marked as templates (`hidden=true`) and adds them to the map.
fn find_all_templates(nodes: &[OverseerNode], templates: &mut HashMap<String, OverseerNode>) {
    for node in nodes {
        // A template is a div marked as hidden. We use its name as the key.
        if node.node_type == "div" && node.parameters.get("hidden") == Some(&OverseerValue::Boolean(true)) {
            if !node.name.is_empty() {
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
    find_all_templates(nodes, &mut templates);
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
}

/// Recursively traverses the AST, resolving templates as it goes.
fn resolve_node(node: &mut OverseerNode, templates: &HashMap<String, OverseerNode>) {
    // Check if the current node is a list that uses a template.
    if node.node_type == "list" {
        if let Some(OverseerValue::Template(template_path)) = node.parameters.get("entry") {
            // Simplified path resolution: "entry=<../Task>" -> "Task"
            let template_name = template_path.split('/').last().unwrap_or("");

            if let Some(template_node) = templates.get(template_name) {
                let mut resolved_children = Vec::new();
                for list_item in &node.children {
                    if list_item.node_type == "list_item" {
                        if !list_item.children.is_empty() {
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
                            merge_node(&mut resolved_item, &overrides);
                            // Infer types for '-' children from template fields
                            for child in resolved_item.children.iter_mut() {
                                if child.node_type == "-" {
                                    if let Some(template_field) = template_node.children.iter().find(|f| f.name == child.name) {
                                        child.node_type = template_field.node_type.clone();
                                    }
                                }
                            }
                            resolved_children.push(resolved_item);
                        } else if let Some(val) = list_item.parameters.get("value") {
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
                            // Fallback: just clone
                            resolved_children.push(list_item.clone());
                        }
                    } else {
                        // Not a list_item, pass through
                        resolved_children.push(list_item.clone());
                    }
                }
                node.children = resolved_children;
            }
        }
    }

    // If this node is an instantiated template, infer types for '-' children from the template
    if let Some(template_path) = &node.template {
        let template_name = template_path.split('/').last().unwrap_or("");
        if let Some(template_node) = templates.get(template_name) {
            for child in node.children.iter_mut() {
                if child.node_type == "-" {
                    if let Some(template_field) = template_node.children.iter().find(|f| f.name == child.name) {
                        child.node_type = template_field.node_type.clone();
                    }
                }
            }
        }
    }

    // After processing the current node (e.g., resolving a list), recurse into
    // the children. This is a pre-order traversal, which is correct for this
    // problem because it resolves containers before their contents.
    for child in node.children.iter_mut() {
        resolve_node(child, templates);
    }
}

/// Merges override fields into a template clone.
fn merge_node(template: &mut OverseerNode, overrides: &HashMap<String, &OverseerNode>) {
    for template_field in template.children.iter_mut() {
        if let Some(override_field) = overrides.get(&template_field.name) {
            // Always preserve the node_type from the template
            // (do NOT overwrite with the override's name or type)
            // Only override value and children as appropriate

            // Override a simple value (e.g., name = "...")
            if let Some(val) = override_field.parameters.get("value") {
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