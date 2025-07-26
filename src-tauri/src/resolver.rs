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
                    // We only resolve complex list items defined with blocks `{...}`
                    if list_item.node_type == "list_item" && !list_item.children.is_empty() {
                        let mut resolved_item = (*template_node).clone();

                        // The list item's children are the fields that override the template.
                        let overrides: HashMap<String, &OverseerNode> = list_item
                            .children
                            .iter()
                            .map(|o| (o.name.clone(), o))
                            .collect();

                        merge_node(&mut resolved_item, &overrides);

                        resolved_children.push(resolved_item);
                    } else {
                        // Pass through simple list items (e.g., - "a string")
                        resolved_children.push(list_item.clone());
                    }
                }
                // Replace the list's original children with the fully resolved nodes.
                node.children = resolved_children;
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
            // Override a simple value (e.g., name = "...")
            if let Some(val) = override_field.parameters.get("value") {
                template_field.parameters.insert("value".to_string(), val.clone());
            }
            // Override children for nested lists (e.g., StepTasks { ... })
            if !override_field.children.is_empty() {
                template_field.children = override_field.children.clone();
            }
        }
    }
}