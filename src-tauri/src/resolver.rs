#[allow(unused_imports)]
use crate::types::{OverseerNode, OverseerValue, Color, CssSize};
use crate::formula_evaluator::{FormulaEvaluator, EvaluationContext};
use std::collections::HashMap;

// Debug logging macro for resolver
macro_rules! debug_resolver {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-resolver")]
        println!($($arg)*);
    };
}

/// Finds a specific template by name in the node tree (on-demand search).
/// This allows any node to be a template, not just div nodes with hidden=true.
fn find_template_by_name(nodes: &[OverseerNode], template_name: &str) -> Option<OverseerNode> {
    for node in nodes {
        // Any node can be a template if its name matches
        if node.name == template_name {
            debug_resolver!("[RESOLVER] Found template: {} (type: {}) with {} children", node.name, node.node_type, node.children.len());
            #[cfg(feature = "debug-resolver")]
            {
                for child in &node.children {
                    println!("[RESOLVER]   Template field: {} (type: {})", child.name, child.node_type);
                }
            }
            return Some(node.clone());
        }
        // Recurse into children to find nested templates.
        if !node.children.is_empty() {
            if let Some(template) = find_template_by_name(&node.children, template_name) {
                return Some(template);
            }
        }
    }
    None
}

/// Multi-pass template resolution that handles template dependencies.
/// Templates can depend on other templates, so we need multiple passes to resolve everything.
fn resolve_templates_multipass(nodes: &mut Vec<OverseerNode>) {
    const MAX_PASSES: usize = 10; // Prevent infinite loops
    let mut pass = 0;
    let mut made_progress = true;
    
    while made_progress && pass < MAX_PASSES {
        made_progress = false;
        pass += 1;
        debug_resolver!("[RESOLVER] Template resolution pass {}", pass);
        
        // Create a snapshot of nodes for template lookup (immutable reference)
        let nodes_snapshot = nodes.clone(); // We need this for template lookup
        
        // Try to resolve templates in this pass
        for node in nodes.iter_mut() {
            if resolve_node_templates(node, &nodes_snapshot, &mut made_progress) {
                made_progress = true;
            }
        }
        
        debug_resolver!("[RESOLVER] Pass {} complete, made_progress: {}", pass, made_progress);
    }
    
    if pass >= MAX_PASSES {
        debug_resolver!("[RESOLVER] Warning: Maximum template resolution passes reached. Some templates may have circular dependencies.");
    } else {
        debug_resolver!("[RESOLVER] Template resolution completed in {} passes", pass);
    }
}

/// Public entry point to resolve all templates in a document AST.
pub fn resolve_document(nodes: &mut Vec<OverseerNode>) {
    // Multi-pass template resolution to handle template dependencies
    resolve_templates_multipass(nodes);
    
    // After template resolution, resolve layout parameters
    resolve_layout_parameters(nodes, None);
    
    // After layout resolution, resolve parameter inheritance
    resolve_parameter_inheritance(nodes, &HashMap::new());
    
    // After parameter inheritance, evaluate formulas
    evaluate_formulas_in_document(nodes);

    // After formulas, compute chart series for charts/plots (MVP)
    compute_chart_series(nodes);

    // After formulas, compute UI sort keys for lists (presentation-only; do not reorder children)
    compute_list_ui_sort_keys(nodes);

    // Phase 1: initialize and validate mount nodes (lazy placeholders only)
    initialize_and_validate_mount_nodes(nodes);
}

/// Selective resolution that only processes specific field paths
pub fn resolve_specific_fields(nodes: &mut Vec<OverseerNode>, field_paths: &std::collections::HashSet<String>) {
    debug_resolver!("🎯 Selective resolution for {} fields: {:?}", field_paths.len(), field_paths);
    
    // For selective updates, we only need to:
    // 1. Re-evaluate formulas for the specific fields
    // 2. Re-compute charts that depend on those fields
    // 3. Skip template resolution, layout, and parameter inheritance (those don't change)
    
    // Only evaluate formulas for the specific field paths
    evaluate_formulas_for_specific_fields(nodes, field_paths);
    
    // Only recompute charts that contain references to the changed fields
    compute_chart_series_for_specific_fields(nodes, field_paths);
    
    debug_resolver!("✅ Selective resolution completed");
}

// Initialize default values and validate mount nodes across the document tree
fn initialize_and_validate_mount_nodes(nodes: &mut Vec<OverseerNode>) {
    let len = nodes.len();
    for i in 0..len {
        let node_ptr: *mut OverseerNode = &mut nodes[i] as *mut _;
        unsafe { initialize_and_validate_mount_nodes_rec(node_ptr); }
    }
}

unsafe fn initialize_and_validate_mount_nodes_rec(node_ptr: *mut OverseerNode) {
    use crate::types::OverseerValue;
    let node: &mut OverseerNode = &mut *node_ptr;
    if node.node_type == "mount" {
        // Default: unloaded (do not clobber if already set by actions)
        if !node.parameters.contains_key("_mount_status") {
            node.parameters.insert("_mount_status".to_string(), OverseerValue::String("unloaded".to_string()));
        }
        // Validate required 'source' parameter presence
        let has_source = node.parameters.contains_key("source");
        if !has_source {
            node.parameters.insert("_mount_status".to_string(), OverseerValue::String("error".to_string()));
            node.parameters.insert("_mount_error".to_string(), OverseerValue::String("mount: missing required 'source' parameter".to_string()));
        }
        // Default lazy=true if not provided; set as computed shadow so renderers can read via either path
        if !node.parameters.contains_key("lazy") && !node.parameters.contains_key("_computed_lazy") {
            node.parameters.insert("_computed_lazy".to_string(), OverseerValue::Boolean(true));
        }
    }
    for idx in 0..node.children.len() {
        let child_ptr: *mut OverseerNode = &mut node.children[idx] as *mut _;
        initialize_and_validate_mount_nodes_rec(child_ptr);
    }
}

/// Compute chart plot series by evaluating per-item x/y expressions on a source container.
fn compute_chart_series(nodes: &mut Vec<OverseerNode>) {
    let snapshot = nodes.clone();
    let len = nodes.len();
    for i in 0..len {
        let node_ptr: *mut OverseerNode = &mut nodes[i] as *mut _;
        let mut current_path = vec![unsafe { (&*node_ptr).name.clone() }];
        unsafe { recursively_compute_chart_series(node_ptr, std::ptr::null(), &mut current_path, &snapshot); }
    }
}

unsafe fn recursively_compute_chart_series(
    node_ptr: *mut OverseerNode,
    _parent_ptr: *const OverseerNode,
    current_path: &mut Vec<String>,
    document_root: &[OverseerNode],
) {
    use crate::formula_evaluator::{EvaluationContext, FormulaEvaluator};
    use crate::types::OverseerValue;

    let node: &mut OverseerNode = &mut *node_ptr;

    // Process chart nodes: collect bounds across plots
    if node.node_type == "chart" {
        let mut global_min_x: Option<f64> = None;
        let mut global_max_x: Option<f64> = None;
        let mut global_min_y: Option<f64> = None;
        let mut global_max_y: Option<f64> = None;

        // Iterate plot children
        for plot in node.children.iter_mut().filter(|c| c.node_type == "plot") {
            // Build an evaluation context for source resolution
            let ctx = EvaluationContext::new(current_path.clone(), document_root);

            // Resolve source: support plain path string OR a processed list via method chain
            enum SourceItems<'a> { FromPath(Vec<String>, &'a OverseerNode), FromList(Vec<String>, Vec<&'a OverseerNode>) }
            let source_items: Option<SourceItems> = match plot.parameters.get("source") {
                Some(OverseerValue::String(s)) => {
                    resolve_path_from(document_root, current_path, s).map(|(p, n)| SourceItems::FromPath(p, n))
                }
                Some(OverseerValue::Formula(f)) => {
                    // Try to evaluate to a string path first
                    match FormulaEvaluator::evaluate_formula(f, &ctx) {
                        Ok(OverseerValue::String(s)) => resolve_path_from(document_root, current_path, &s).map(|(p, n)| SourceItems::FromPath(p, n)),
                        _ => {
                            // Fall back: treat the formula as a list-source expression
                            match FormulaEvaluator::evaluate_list_source_nodes(f, &ctx) {
                                Ok((p, items)) => Some(SourceItems::FromList(p, items)),
                                Err(_) => None,
                            }
                        }
                    }
                }
                _ => None,
            };

            if let Some(source_items) = source_items {
                // Prepare item iteration and path seeds
                let (source_path_vec, items, source_ref_opt): (Vec<String>, Vec<&OverseerNode>, Option<&OverseerNode>) = match source_items {
                    SourceItems::FromPath(p, n) => (p.clone(), n.get_accessible_children(), Some(n)),
                    SourceItems::FromList(p, list) => (p, list, None),
                };
                let mut series: Vec<(f64, f64)> = Vec::new();

                // Fetch x/y expressions
                let x_src = if let Some(val) = plot.parameters.get("x") {
                    match val {
                        OverseerValue::Formula(s) => Some(s.as_str()),
                        OverseerValue::String(s) => Some(s.as_str()),
                        _ => None,
                    }
                } else { None };
                let y_src = match plot.parameters.get("y") {
                    Some(OverseerValue::Formula(s)) => Some(s.as_str()),
                    Some(OverseerValue::String(s)) => Some(s.as_str()),
                    _ => None,
                };
                if x_src.is_none() || y_src.is_none() { continue; }
                let x_src = x_src.unwrap();
                let y_src = y_src.unwrap();

                for item in items {
                    // Build item path for context
                    let mut item_path = source_path_vec.clone();
                    item_path.push(item.name.clone());
                    let ctx = EvaluationContext::new_with_current_and_parent(item, source_ref_opt, item_path, document_root);

                    let x_val = FormulaEvaluator::evaluate_lambda_on_item(x_src, &ctx, item).ok();
                    let y_val = FormulaEvaluator::evaluate_lambda_on_item(y_src, &ctx, item).ok();

                    if let (Some(xv), Some(yv)) = (to_f64(x_val), to_f64(y_val)) {
                        if xv.is_finite() && yv.is_finite() {
                            // Update bounds
                            global_min_x = Some(global_min_x.map_or(xv, |m| m.min(xv)));
                            global_max_x = Some(global_max_x.map_or(xv, |m| m.max(xv)));
                            global_min_y = Some(global_min_y.map_or(yv, |m| m.min(yv)));
                            global_max_y = Some(global_max_y.map_or(yv, |m| m.max(yv)));
                            series.push((xv, yv));
                        }
                    }
                }

                // Store series as JSON string
                let json = series_to_json(&series);
                plot.parameters.insert("_computed_series".to_string(), OverseerValue::String(json));
            }
        }

        // Store computed bounds on the chart
        if let (Some(xmin), Some(xmax), Some(ymin), Some(ymax)) = (global_min_x, global_max_x, global_min_y, global_max_y) {
            node.parameters.insert("_computed_x_min".to_string(), OverseerValue::Float(xmin));
            node.parameters.insert("_computed_x_max".to_string(), OverseerValue::Float(xmax));
            node.parameters.insert("_computed_y_min".to_string(), OverseerValue::Float(ymin));
            node.parameters.insert("_computed_y_max".to_string(), OverseerValue::Float(ymax));
        }
    }

    // Recurse
    for idx in 0..node.children.len() {
        let child_ptr: *mut OverseerNode = &mut node.children[idx] as *mut _;
        // Disambiguate duplicate sibling names by appending an ordinal index (name#k)
        {
            let child_ref = &*child_ptr;
            let name = child_ref.name.clone();
            let k = node.children
                .iter()
                .take(idx)
                .filter(|c| c.name == name)
                .count();
            if k > 0 {
                current_path.push(format!("{}#{}", name, k));
            } else {
                current_path.push(name);
            }
        }
        recursively_compute_chart_series(child_ptr, node as *const OverseerNode, current_path, document_root);
        current_path.pop();
    }

    // Helpers local to this function
    fn to_f64(v: Option<OverseerValue>) -> Option<f64> {
        match v? {
            OverseerValue::Integer(i) => Some(i as f64),
            OverseerValue::Float(f) => Some(f),
            // Accept numeric-looking strings directly
            OverseerValue::String(s) => {
                // Try plain number first
                if let Ok(n) = s.parse::<f64>() { return Some(n); }
                // Try RFC3339 timestamp
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                    return Some(dt.timestamp_millis() as f64);
                }
                // Try date-only YYYY-MM-DD -> midnight UTC
                if let Ok(nd) = chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
                    if let Some(ndt) = nd.and_hms_opt(0, 0, 0) {
                        let dt = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(ndt, chrono::Utc);
                        return Some(dt.timestamp_millis() as f64);
                    }
                }
                None
            }
            OverseerValue::Timestamp(ts) => {
                // Parse to epoch ms via chrono if available
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&ts) {
                    Some(dt.timestamp_millis() as f64)
                } else { None }
            }
            OverseerValue::Date(d) => {
                // Treat YYYY-MM-DD as midnight UTC
                let ts = format!("{}T00:00:00Z", d);
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&ts) {
                    Some(dt.timestamp_millis() as f64)
                } else { None }
            }
            _ => None,
        }
    }

    fn series_to_json(series: &Vec<(f64, f64)>) -> String {
        let mut s = String::from("[");
        for (i, (x, y)) in series.iter().enumerate() {
            if i > 0 { s.push(','); }
            s.push_str(&format!("[{},{}]", x, y));
        }
        s.push(']');
        s
    }
}

/// Resolve a simple path string from the current node path to a node and return both its path vec and ref.
/// Supports:
/// - Absolute paths starting with '/': resolved from root ("/Root/Child")
/// - Relative paths with optional leading '../'
    fn resolve_path_from<'a>(
    document_root: &'a [OverseerNode],
    current_path: &[String],
    path: &str,
) -> Option<(Vec<String>, &'a OverseerNode)> {
    // Build starting path
        let segments: Vec<&str> = path.split('/').collect();
    if path.starts_with('/') {
        // Absolute: first segment is empty, skip it and start from root
        if segments.len() < 2 { return None; }
        let mut out: Vec<String> = Vec::new();
        // First real segment must match a root node
        let first = segments[1];
        let root = document_root.iter().find(|n| n.name == first)?;
        out.push(first.to_string());
        let mut current = root;
        for seg in &segments[2..] {
            if seg.is_empty() { continue; }
            if let Some(next) = current.get_accessible_children().into_iter().find(|c| c.name == *seg) {
                out.push(seg.to_string());
                current = next;
            } else { return None; }
        }
        return Some((out, current));
    } else {
        // Relative: start from current_path, apply '../' hops, then descend
        let mut base: Vec<String> = current_path.to_vec();
        // Remove current node (we want to resolve from the chart node's parent for sibling lookup)
        if !base.is_empty() { base.pop(); }
        let mut idx = 0usize;
        while idx < segments.len() && segments[idx] == ".." {
            if base.is_empty() { return None; }
            base.pop();
            idx += 1;
        }
        // Resolve base to node
        let mut current = resolve_path_vec_to_node(document_root, &base)?;
        let mut out = base;
        for seg in &segments[idx..] {
            if seg.is_empty() { continue; }
            if let Some(next) = current.get_accessible_children().into_iter().find(|c| c.name == *seg) {
                out.push(seg.to_string());
                current = next;
            } else { return None; }
        }
        return Some((out, current));
    }
}

fn resolve_path_vec_to_node<'a>(root: &'a [OverseerNode], segments: &[String]) -> Option<&'a OverseerNode> {
    if segments.is_empty() { return None; }
    let mut current = root.iter().find(|n| n.name == segments[0])?;
    for seg in &segments[1..] {
        if let Some(next) = current.get_accessible_children().into_iter().find(|c| c.name == *seg) {
            current = next;
        } else { return None; }
    }
    Some(current)
}

/// Resolves templates for a single node and its children.
/// Returns true if any progress was made in this pass.
fn resolve_node_templates(node: &mut OverseerNode, all_nodes: &[OverseerNode], made_progress: &mut bool) -> bool {
    let mut local_progress = false;
    
    debug_resolver!("[RESOLVER] Resolving node: {} (type: {})", node.name, node.node_type);
    
    // Check if the current node is a list that uses a template or simple type.
    if node.node_type == "list" {
        if let Some(entry_value) = node.parameters.get("entry") {
            match entry_value {
                OverseerValue::Template(template_path) => {
                    debug_resolver!("[RESOLVER] List {} uses template: {}", node.name, template_path);
                    // Simplified path resolution: allow both <../Task> and <Task>
                    let template_name = template_path.trim_start_matches("../").split('/').last().unwrap_or("");
                    debug_resolver!("[RESOLVER] Resolved template name: {}", template_name);

                    if let Some(template_node) = find_template_by_name(all_nodes, template_name) {
                        debug_resolver!("[RESOLVER] Found template node for {}, processing {} children", template_name, node.children.len());
                        let mut resolved_children = Vec::new();
                        for (idx, list_item) in node.children.iter().enumerate() {
                            debug_resolver!("[RESOLVER]   Processing list item {}: {} (type: {})", idx, list_item.name, list_item.node_type);
                            // Handle both old "list_item" type and new "-" type (after parse_list_item removal)
                            if list_item.node_type == "list_item" || list_item.node_type == "-" {
                                if !list_item.children.is_empty() {
                                    debug_resolver!("[RESOLVER]     Complex list item with {} children", list_item.children.len());
                                    // Complex list item: create a node of the template's type
                                    let mut resolved_item = OverseerNode {
                                        name: {
                                            let n = list_item.name.clone();
                                            if n.is_empty() || n == "-" { format!("{}__{}", template_node.name, idx + 1) } else { n }
                                        },
                                        node_type: template_node.name.clone(),
                                        template: None,
                                        parameters: {
                                            // Start with template parameters as base, but mark them as template-derived
                                            let mut merged_params = HashMap::new();
                                            
                                            // Add template parameters with _template_ prefix to mark their origin
                                            for (key, value) in &template_node.parameters {
                                                merged_params.insert(format!("_template_{}", key), value.clone());
                                                merged_params.insert(key.clone(), value.clone());
                                            }
                                            
                                            // List item parameters override template parameters
                                            for (key, value) in &list_item.parameters {
                                                merged_params.insert(key.clone(), value.clone());
                                                // This key is explicitly overridden; remove template marker so it persists on save
                                                let marker = format!("_template_{}", key);
                                                merged_params.remove(&marker);
                                            }
                                            // Record original type of this template instance (e.g., div)
                                            merged_params.insert("_original_type".to_string(), OverseerValue::String(template_node.node_type.clone()));
                                            // Mark this node as coming from a template so serializer can suppress inherited children
                                            merged_params.insert("_from_template".to_string(), OverseerValue::Boolean(true));
                                            merged_params
                                        },
                                        children: template_node.children.clone(),
                                        is_hierarchy_transparent: template_node.is_hierarchy_transparent,
                                    };
                                    // Mark all cloned children as template-derived so serializer can omit them unless overridden
                                    for child in resolved_item.children.iter_mut() {
                                        mark_template_child_recursive(child);
                                        // Ensure override markers are clean on fresh clones; only true overrides will set these later
                                        if child.parameters.remove("_explicit_child_override").is_some() {
                                            debug_resolver!("[RESOLVER] cleaned _explicit_child_override on clone child '{}')", child.name);
                                        }
                                        if child.parameters.remove("_override_present").is_some() {
                                            debug_resolver!("[RESOLVER] cleaned _override_present on clone child '{}')", child.name);
                                        }
                                    }
                                    let overrides: HashMap<String, &OverseerNode> = list_item
                                        .children
                                        .iter()
                                        .map(|o| (o.name.clone(), o))
                                        .collect();
                                    debug_resolver!("[RESOLVER]     Override fields: {:?}", overrides.keys().collect::<Vec<_>>());
                                    merge_node(&mut resolved_item, &overrides);
                                    
                                    // Mark template-derived styling parameters for all field children
                                    for child in resolved_item.children.iter_mut() {
                                        // For field children, mark common styling parameters as template-derived
                                        let styling_params = ["width", "margin", "spacing", "padding", 
                                                            "margin-top", "margin-bottom", "margin-left", "margin-right",
                                                            "padding-top", "padding-bottom", "padding-left", "padding-right",
                                                            "color", "font-color", "background-color", "font-size"];
                                        
                                        for param in styling_params.iter() {
                                            if let Some(value) = child.parameters.get(*param) {
                                                // Mark this parameter as template-derived
                                                child.parameters.insert(format!("_template_{}", param), value.clone());
                                            }
                                        }
                                    }
                                    // Recursively infer '-' types based on template structure
                                    infer_dash_types_from_template(&mut resolved_item, &template_node);
                                    resolved_children.push(resolved_item);
                                } else if let Some(val) = list_item.parameters.get("value") {
                                    debug_resolver!("[RESOLVER]     Simple value list item: {:?}", val);
                                    // Simple value: create a node of the template's type, with value
                    let resolved_item = OverseerNode {
                                        name: {
                                            let n = list_item.name.clone();
                                            if n.is_empty() || n == "-" { format!("{}__{}", template_node.name, idx + 1) } else { n }
                                        },
                                        node_type: template_node.name.clone(),
                                        template: None,
                                        parameters: {
                                            // Start with template parameters as base  
                        let mut merged_params = template_node.parameters.clone();
                        merged_params.insert("_original_type".to_string(), OverseerValue::String(template_node.node_type.clone()));
                                            merged_params.insert("value".to_string(), val.clone());
                                            merged_params
                                        },
                                        children: Vec::new(),
                                        is_hierarchy_transparent: template_node.is_hierarchy_transparent,
                                    };
                                    resolved_children.push(resolved_item);
                                } else {
                                    // '-' with empty block and no value => instantiate default template clone; else, fallback clone
                                    if list_item.node_type == "-" {
                                        debug_resolver!("[RESOLVER]     Empty '-' item -> instantiate template '{}__{}'", template_node.name, idx + 1);
                                        let mut resolved_item = OverseerNode {
                                            name: {
                                                let n = list_item.name.clone();
                                                if n.is_empty() || n == "-" { format!("{}__{}", template_node.name, idx + 1) } else { n }
                                            },
                                            node_type: template_node.name.clone(),
                                            template: None,
                                            parameters: {
                                                let mut merged_params = HashMap::new();
                                                for (key, value) in &template_node.parameters {
                                                    merged_params.insert(format!("_template_{}", key), value.clone());
                                                    merged_params.insert(key.clone(), value.clone());
                                                }
                                                merged_params.insert("_original_type".to_string(), OverseerValue::String(template_node.node_type.clone()));
                                                merged_params.insert("_from_template".to_string(), OverseerValue::Boolean(true));
                                                merged_params
                                            },
                                            children: template_node.children.clone(),
                                            is_hierarchy_transparent: template_node.is_hierarchy_transparent,
                                        };
                                        for child in resolved_item.children.iter_mut() {
                                            mark_template_child_recursive(child);
                                            if child.parameters.remove("_explicit_child_override").is_some() {
                                                debug_resolver!("[RESOLVER] cleaned _explicit_child_override on clone child '{}')", child.name);
                                            }
                                            if child.parameters.remove("_override_present").is_some() {
                                                debug_resolver!("[RESOLVER] cleaned _override_present on clone child '{}')", child.name);
                                            }
                                        }
                                        // Recursively infer '-' types based on template structure
                                        infer_dash_types_from_template(&mut resolved_item, &template_node);
                                        resolved_children.push(resolved_item);
                                    } else {
                                        debug_resolver!("[RESOLVER]     Fallback: cloning list item as-is");
                                        // Fallback: just clone
                                        resolved_children.push(list_item.clone());
                                    }
                                }
                            } else {
                                debug_resolver!("[RESOLVER]   Not a complex list_item, passing through: {} (type: {})", list_item.name, list_item.node_type);
                                // Not a complex list_item, pass through
                                resolved_children.push(list_item.clone());
                            }
                        }
                        debug_resolver!("[RESOLVER] List resolution complete, {} -> {} children", node.children.len(), resolved_children.len());
                        node.children = resolved_children;
                        local_progress = true;
                    } else {
                        debug_resolver!("[RESOLVER] Warning: Template not found: {}", template_name);
                        // Template not found - might be resolved in a later pass
                    }
                },
                OverseerValue::String(type_name) => {
                    debug_resolver!("[RESOLVER] List {} uses simple type: {}", node.name, type_name);
                    // Handle simple type entries like entry=string
                    let mut resolved_children = Vec::new();
                    for (_i, list_item) in node.children.iter().enumerate() {
                        debug_resolver!("[RESOLVER]   Processing simple type list item {}: {} (type: {})", _i, list_item.name, list_item.node_type);
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
                    local_progress = true;
                },
                _ => {
                    debug_resolver!("[RESOLVER] List {} has unsupported entry parameter type: {:?}", node.name, entry_value);
                }
            }
        } else {
            debug_resolver!("[RESOLVER] List {} has no entry parameter", node.name);
        }
    }

    // If this node is a direct template instance (e.g., <Colorful> Instance { ... }),
    // clone the template's parameters and children, then merge overrides from the instance.
    if let Some(template_path) = &node.template.clone() {
        debug_resolver!("[RESOLVER] Node {} has template: {}", node.name, template_path);
        let template_name = template_path.trim_start_matches("../").split('/').last().unwrap_or("");
        if let Some(template_node) = find_template_by_name(all_nodes, template_name) {
            debug_resolver!(
                "[RESOLVER] Instantiating template {} for instance {}",
                template_name, node.name
            );

            // Start with a clone of the template's declared component name as type (e.g., "Task"),
            // mirroring list templating where we use the template's name as the instantiated type.
            node.node_type = template_node.name.clone();
            let mut merged_params: HashMap<String, OverseerValue> = HashMap::new();

            // Mark template parameters and copy them as defaults
            for (key, value) in &template_node.parameters {
                merged_params.insert(format!("_template_{}", key), value.clone());
                merged_params.insert(key.clone(), value.clone());
            }

            // Instance parameters override template parameters
            for (key, value) in node.parameters.clone() {
                // apply override and remove template marker so it persists
                let marker = format!("_template_{}", key);
                merged_params.insert(key.clone(), value);
                merged_params.remove(&marker);
            }

            // Annotate with original type of template (e.g., div) so renderer can treat it as container
            merged_params.insert("_original_type".to_string(), OverseerValue::String(template_node.node_type.clone()));
            // Mark this node as coming from a template so serializer can suppress inherited children
            merged_params.insert("_from_template".to_string(), OverseerValue::Boolean(true));
            // Preserve the original template path for serialization (<T> I { ... })
            merged_params.insert("_template_origin".to_string(), OverseerValue::Template(template_path.clone()));

            node.parameters = merged_params;
            // Clone template children and then merge overrides from the instance, just like list entries
            let instance_children = node.children.clone();
            node.children = template_node.children.clone();
            for child in node.children.iter_mut() {
                mark_template_child_recursive(child);
                // Ensure override markers are clean on fresh clones; only true overrides will set these later
                if child.parameters.remove("_explicit_child_override").is_some() {
                    debug_resolver!("[RESOLVER] cleaned _explicit_child_override on inst child '{}')", child.name);
                }
                if child.parameters.remove("_override_present").is_some() {
                    debug_resolver!("[RESOLVER] cleaned _override_present on inst child '{}')", child.name);
                }
            }
            if !instance_children.is_empty() {
                let overrides: HashMap<String, &OverseerNode> = instance_children
                    .iter()
                    .map(|o| (o.name.clone(), o))
                    .collect();
                let override_names: Vec<String> = overrides.keys().cloned().collect();
                debug_resolver!("[RESOLVER] instance '{}' overrides: {:?}", node.name, override_names);
                // Record explicit override names on the instance for serializer to consult
                node.parameters.insert("_explicit_overrides".to_string(), OverseerValue::String(override_names.join(",")));
                merge_node(node, &overrides);
            }

            // Recursively infer '-' types based on template structure
            infer_dash_types_from_template(node, &template_node);

            // Important: clear template reference so this instance isn't reprocessed in subsequent passes.
            // Without this, later passes would treat inherited template children as explicit overrides.
            node.template = None;

            local_progress = true;
        } else {
            debug_resolver!("[RESOLVER] Warning: Template not found for node: {}", template_name);
        }
    }

    // Recursively process children
    for child in node.children.iter_mut() {
        if resolve_node_templates(child, all_nodes, made_progress) {
            local_progress = true;
        }
    }
    
    debug_resolver!("[RESOLVER] Finished resolving node: {} (final type: {}) - progress: {}", node.name, node.node_type, local_progress);
    local_progress
}

/// Recursively infer '-' typed override children using the corresponding template node structure.
fn infer_dash_types_from_template(instance: &mut OverseerNode, template: &OverseerNode) {
    // For each child in instance, find matching template child by name
    for child in instance.children.iter_mut() {
        if let Some(t_child) = template.children.iter().find(|t| t.name == child.name) {
            if child.node_type == "-" {
                debug_resolver!(
                    "[RESOLVER]     Resolving '-' type for {}: {} -> {}",
                    child.name, child.node_type, t_child.node_type
                );
                child
                    .parameters
                    .insert("_original_type".to_string(), OverseerValue::String(child.node_type.clone()));
                child.node_type = t_child.node_type.clone();
            }
            // Recurse for grandchildren
            if !child.children.is_empty() {
                infer_dash_types_from_template(child, t_child);
            }
        }
    }
}

/// Resolves layout parameters for all nodes, calculating effective layout based on parent and parameter values
fn resolve_layout_parameters(nodes: &mut Vec<OverseerNode>, parent_layout: Option<&str>) {
    for node in nodes.iter_mut() {
        // Containers support layout. Treat template instances as containers if their _original_type is div/list.
        let mut is_container = node.node_type == "div" || node.node_type == "list";
        if !is_container {
            if let Some(OverseerValue::String(orig_ty)) = node.parameters.get("_original_type") {
                if orig_ty == "div" || orig_ty == "list" {
                    is_container = true;
                }
            }
        }

        if is_container {
            let effective_layout = calculate_effective_layout(node, parent_layout);
            
            // Store the calculated layout in parameters for the renderer to use
            node.parameters.insert("_effective_layout".to_string(), OverseerValue::String(effective_layout.clone()));

            // Optional alignment across the secondary axis: near|center|far
            if let Some(OverseerValue::String(align)) = node.parameters.get("alignment") {
                let a = match align.as_str() { "near"|"center"|"far" => align.clone(), _ => "near".to_string() };
                node.parameters.insert("_effective_alignment".to_string(), OverseerValue::String(a));
            }
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
                                
fn resolve_parameter_inheritance(nodes: &mut Vec<OverseerNode>, parent_params: &HashMap<String, OverseerValue>) {
    for node in nodes.iter_mut() {
        // List of inheritable styling parameters
        let inheritable_params = [
            "background-color", "font-color", "font-size"
        ];
        
        // Inherit each styling parameter from parent if not explicitly set
        for param_name in &inheritable_params {
            if !node.parameters.contains_key(*param_name) {
                if let Some(parent_value) = parent_params.get(*param_name) {
                    debug_resolver!("[RESOLVER] Inheriting {} = {:?} for node {}", param_name, parent_value, node.name);
                    node.parameters.insert(param_name.to_string(), parent_value.clone());
                    // Mark as template-derived so serializer will not persist inherited styling
                    node.parameters.insert(format!("_template_{}", param_name), parent_value.clone());
                }
            }
        }
        
        // Build inherited parameters map for children (including this node's parameters)
        let mut inherited_params = parent_params.clone();
    for (key, value) in &node.parameters {
            if inheritable_params.contains(&key.as_str()) {
                inherited_params.insert(key.clone(), value.clone());
            }
        }
        
        // Recursively resolve children with inherited parameters
        if !node.children.is_empty() {
            resolve_parameter_inheritance(&mut node.children, &inherited_params);
        }
    }
}

/// Merges override fields into a template clone.
/// Helper: find the path (indices) to a named field within a node's accessible hierarchy,
/// treating transparent nodes (unnamed divs, tabs) as invisible containers.
fn find_accessible_child_path(node: &OverseerNode, target_name: &str) -> Option<Vec<usize>> {
    for (i, child) in node.children.iter().enumerate() {
        if child.name == target_name {
            return Some(vec![i]);
        }
        if child.is_hierarchy_transparent {
                    if let Some(mut subpath) = find_accessible_child_path(child, target_name) {
                let mut path = vec![i];
                path.append(&mut subpath);
                return Some(path);
            }
        }
    }
    None
}

/// Helper: get a mutable reference to a child using an index path
fn get_child_mut_by_path<'a>(node: &'a mut OverseerNode, path: &[usize]) -> Option<&'a mut OverseerNode> {
    if path.is_empty() {
        return None;
    }
    let mut current: *mut OverseerNode = node as *mut _;
    // SAFETY: We ensure at most one mutable reference is active by walking iteratively.
    for (depth, &idx) in path.iter().enumerate() {
        unsafe {
            let current_ref = &mut *current;
            if idx >= current_ref.children.len() {
                return None;
            }
            let child_ptr: *mut OverseerNode = &mut current_ref.children[idx];
            if depth == path.len() - 1 {
                return Some(&mut *child_ptr);
            } else {
                current = child_ptr;
            }
        }
    }
    None
}

fn merge_node(template: &mut OverseerNode, overrides: &HashMap<String, &OverseerNode>) {
    debug_resolver!("[RESOLVER] Merging overrides into template with {} fields (transparent-aware)", template.children.len());

    // Apply each override by locating the target field path in the template via transparent-aware lookup
    for (ov_name, override_field) in overrides.iter() {
        if let Some(path) = find_accessible_child_path(template, ov_name) {
            if let Some(template_field) = get_child_mut_by_path(template, &path) {
            debug_resolver!(
                "[RESOLVER]   Merging field: {} (template type: {}, override type: {})",
                template_field.name, template_field.node_type, override_field.node_type
            );

            // Override a simple value (e.g., name = "...")
            if let Some(val) = override_field.parameters.get("value") {
                debug_resolver!("[RESOLVER]     Setting value: {:?}", val);
                // Always set the value from the explicit override
                template_field
                    .parameters
                    .insert("value".to_string(), val.clone());
                // Treat presence in source as an explicit override even if equal to template default
                template_field.parameters.insert("_override_present".to_string(), OverseerValue::Boolean(true));
                template_field.parameters.insert("_explicit_child_override".to_string(), OverseerValue::Boolean(true));
                debug_resolver!("[RESOLVER] set explicit override (value) on '{}'", template_field.name);
                // Remove template marker for value if present so serializers won't treat it as inherited
                if template_field.parameters.contains_key("_template_value") {
                    template_field.parameters.remove("_template_value");
                }
            }

            // If this field is a list, handle entry inheritance and recursive merge
            if template_field.node_type == "list" {
                // If override does not specify entry, inherit from template
                if !override_field.parameters.contains_key("entry") {
                    if let Some(entry) = template_field.parameters.get("entry") {
                        template_field
                            .parameters
                            .insert("entry".to_string(), entry.clone());
                    }
                } else if let Some(entry) = override_field.parameters.get("entry") {
                    // If override specifies entry, use it
                    template_field
                        .parameters
                        .insert("entry".to_string(), entry.clone());
                }
                // Recursively resolve/merge children for nested lists
                if !override_field.children.is_empty() {
                    template_field.children = override_field.children.clone();
                    template_field.parameters.insert("_override_present".to_string(), OverseerValue::Boolean(true));
                    template_field.parameters.insert("_explicit_child_override".to_string(), OverseerValue::Boolean(true));
                    debug_resolver!("[RESOLVER] set explicit override (list children) on '{}'", template_field.name);
                }
            } else if !override_field.children.is_empty() {
                // For non-list fields, override children only if they differ from template
                let differs = template_field.children != override_field.children;
                if differs {
                    template_field.children = override_field.children.clone();
                    template_field.parameters.insert("_override_present".to_string(), OverseerValue::Boolean(true));
                    template_field.parameters.insert("_explicit_child_override".to_string(), OverseerValue::Boolean(true));
                    debug_resolver!("[RESOLVER] set explicit override (children) on '{}'", template_field.name);
                } else {
                    debug_resolver!("[RESOLVER]     Override children identical to template; skipping override marking");
                }
            }
            // Ensure node_type is preserved from template (do not overwrite)
            } else {
                debug_resolver!(
                    "[RESOLVER]   Override '{}' path lookup failed unexpectedly",
                    ov_name
                );
            }
        } else {
            debug_resolver!(
                "[RESOLVER]   Override '{}' had no matching field in template (considering transparency)",
                ov_name
            );
        }
    }
}

/// Mark a node and its subtree as template-derived by adding _template_ markers for present params
fn mark_template_child_recursive(node: &mut OverseerNode) {
    // Mark a simple flag to indicate this whole node is from a template
    node.parameters.insert("_template_node".to_string(), OverseerValue::Boolean(true));
    // For all existing parameters, add a _template_ marker so serializer excludes them by default
    let keys: Vec<String> = node.parameters.keys().cloned().collect();
    for k in keys {
        if !k.starts_with("_") { // avoid internal keys
            if let Some(v) = node.parameters.get(&k).cloned() {
                node.parameters.insert(format!("_template_{}", k), v);
            }
        }
    }
    for child in node.children.iter_mut() {
        mark_template_child_recursive(child);
    }
}

/// Entry point for formula evaluation.
/// It creates an immutable snapshot of the document for safe lookups
/// and then starts the recursive evaluation process.
fn evaluate_formulas_in_document(nodes: &mut Vec<OverseerNode>) {
    debug_resolver!("[RESOLVER] Starting formula evaluation");
    let document_root_snapshot = nodes.clone();

    // Walk using raw pointers so we can pass parent immutable reference alongside child mutable
    let len = nodes.len();
    for i in 0..len {
        let node_ptr: *mut OverseerNode = &mut nodes[i] as *mut _;
        let mut current_path = vec![unsafe { (&*node_ptr).name.clone() }];
        unsafe { recursively_evaluate_node_formulas(node_ptr, std::ptr::null(), &mut current_path, &document_root_snapshot); }
    }

    debug_resolver!("[RESOLVER] Formula evaluation completed");
}

/// Selective formula evaluation that only processes specific field paths
fn evaluate_formulas_for_specific_fields(nodes: &mut Vec<OverseerNode>, field_paths: &std::collections::HashSet<String>) {
    debug_resolver!("[RESOLVER] Starting selective formula evaluation for {} fields", field_paths.len());
    let document_root_snapshot = nodes.clone();

    // Walk using raw pointers so we can pass parent immutable reference alongside child mutable
    let len = nodes.len();
    for i in 0..len {
        let node_ptr: *mut OverseerNode = &mut nodes[i] as *mut _;
        let mut current_path = vec![unsafe { (&*node_ptr).name.clone() }];
        unsafe { 
            recursively_evaluate_node_formulas_selective(
                node_ptr, 
                std::ptr::null(), 
                &mut current_path, 
                &document_root_snapshot,
                field_paths
            ); 
        }
    }

    debug_resolver!("[RESOLVER] Selective formula evaluation completed");
}

/// Selective chart computation that only processes charts affected by specific field changes
fn compute_chart_series_for_specific_fields(nodes: &mut Vec<OverseerNode>, field_paths: &std::collections::HashSet<String>) {
    debug_resolver!("🎯 Selective chart computation for fields: {:?}", field_paths);
    
    // Analyze if any charts actually depend on the changed field paths
    let charts_need_update = charts_depend_on_fields(nodes, field_paths);
    
    if charts_need_update {
    debug_resolver!("🔄 Charts depend on changed fields, performing selective chart recomputation");
        compute_chart_series(nodes);
    } else {
    debug_resolver!("✅ No charts depend on changed fields, skipping chart computation");
    }
}

/// Check if any charts in the document depend on the specified field paths
fn charts_depend_on_fields(nodes: &[OverseerNode], field_paths: &std::collections::HashSet<String>) -> bool {
    for node in nodes {
        if chart_node_depends_on_fields(node, field_paths, "") {
            return true;
        }
    }
    false
}

/// Recursively check if a chart node or its children depend on the specified field paths
fn chart_node_depends_on_fields(node: &OverseerNode, field_paths: &std::collections::HashSet<String>, current_path: &str) -> bool {
    let node_path = if current_path.is_empty() { 
        node.name.clone() 
    } else { 
        format!("{}/{}", current_path, node.name) 
    };
    
    // Check if this is a chart with plots
    if node.node_type == "chart" {
        for child in &node.children {
            if child.node_type == "plot" {
                if plot_depends_on_fields(child, field_paths, &node_path) {
                    debug_resolver!("📊 Chart plot '{}' depends on changed fields", format!("{}/{}", node_path, child.name));
                    return true;
                }
            }
        }
    }
    
    // Recursively check children
    for child in &node.children {
        if chart_node_depends_on_fields(child, field_paths, &node_path) {
            return true;
        }
    }
    
    false
}

/// Check if a specific plot depends on any of the changed field paths
fn plot_depends_on_fields(plot: &OverseerNode, field_paths: &std::collections::HashSet<String>, chart_path: &str) -> bool {
    use crate::types::OverseerValue;
    
    // Check the 'source' parameter to see what data the plot references
    if let Some(OverseerValue::String(source_path)) = plot.parameters.get("source") {
    debug_resolver!("🔍 Checking if plot source '{}' intersects with changed fields: {:?}", source_path, field_paths);
        
        // If the source path (like "/data") intersects with any changed field paths
        for field_path in field_paths {
            // Check if the changed field could affect the plot's data source
            let source_clean = source_path.trim_start_matches('/');
            
            // Direct path match (e.g., field "data" affects source "/data")
            if field_path == source_clean || field_path.starts_with(&format!("{}/", source_clean)) {
                debug_resolver!("� Plot source '{}' directly affected by field change '{}'", source_path, field_path);
                return true;
            }
            
            // Reverse check: source affects field (e.g., source "/data" affects field "data/item")
            if source_clean.starts_with(field_path) || source_clean.starts_with(&format!("{}/", field_path)) {
                debug_resolver!("📊 Plot source '{}' contains changed field '{}'", source_path, field_path);
                return true;
            }
        }
    }
    
    // For now, assume plot formulas (x, y parameters) only depend on lambda variables and source data
    // They typically don't depend on external fields like 'a' or 'b'
    debug_resolver!("✅ Plot '{}' does not depend on changed fields", format!("{}/{}", chart_path, plot.name));
    false
}

/// Recursively traverses the node tree, evaluating formulas along the way.
/// It maintains the path to the current node, which is crucial for the EvaluationContext.
unsafe fn recursively_evaluate_node_formulas(
    node_ptr: *mut OverseerNode,
    _parent_ptr: *const OverseerNode,
    current_path: &mut Vec<String>,
    document_root: &[OverseerNode],
) {
    let node: &mut OverseerNode = &mut *node_ptr;
    let _parent_ref: Option<&OverseerNode> = if _parent_ptr.is_null() { None } else { Some(&*_parent_ptr) };
    // Skip evaluating formulas for nodes inside action handler blocks (on click/timeout)
    if let Some(p) = _parent_ref {
        if p.node_type == "on" {
            // Do not evaluate formulas in action payloads at load time; they'll be evaluated on action execution
            return;
        }
    }
    // Context is created per-pass below to avoid long-lived borrows while we mutate parameters

    // Evaluate formulas in this node's parameters, but preserve original values.
    // Store computed results under shadow keys: _computed_<key> (or _computed_value for value).
    // To support intra-node dependencies (A depends on B on the same node), run a small fixed-point with 2 passes.
    // Collect owned copies of (key, formula_string) to avoid holding borrows while we later mutate parameters
    let formula_pairs: Vec<(String, String)> = node
        .parameters
        .iter()
        .filter_map(|(k, v)| match v {
            OverseerValue::Formula(s) => Some((k.clone(), s.clone())),
            _ => None,
        })
        .collect();

    // Run up to 2 passes so values depending on other same-node formulas can pick up computed shadows.
    for _ in 0..2 {
        // Create a fresh context each pass; its immutable borrow ends before we mutate parameters
        let context = EvaluationContext::new_with_current_and_parent(node, _parent_ref, current_path.to_vec(), document_root);
        let mut computed_params: Vec<(String, OverseerValue)> = Vec::new();
        for (key, formula_src) in &formula_pairs {
            debug_resolver!("[RESOLVER] Evaluating formula in {}.{}: {}", node.name, key, formula_src);
            let shadow_key = if key == "value" { "_computed_value".to_string() } else { format!("_computed_{}", key) };
            match FormulaEvaluator::evaluate_formula(formula_src.as_str(), &context) {
                Ok(result) => {
                    debug_resolver!("[RESOLVER] Formula result: {:?}", result);
                    computed_params.push((shadow_key, result));
                }
                Err(_err) => {
                    debug_resolver!("[RESOLVER] Formula error at {}.{}", node.name, key);
                    computed_params.push((shadow_key, OverseerValue::String("invalid formula error".to_string())));
                }
            }
        }
        // Drop context before mutating node.parameters
        drop(context);
        // Merge computed shadow params into node.parameters (do not overwrite originals).
        // Insert after each pass so subsequent passes can read newly available _computed_* values.
        for (k, v) in computed_params {
            node.parameters.insert(k, v);
        }
    }
    
    // Recursively evaluate formulas in children
    let child_len = node.children.len();
    for idx in 0..child_len {
        let child_ptr: *mut OverseerNode = &mut node.children[idx] as *mut _;
        // Disambiguate duplicate sibling names by appending ordinal (name#k)
        {
            let child_ref = &*child_ptr;
            let name = child_ref.name.clone();
            let k = node.children.iter().take(idx).filter(|c| c.name == name).count();
            if k > 0 { current_path.push(format!("{}#{}", name, k)); } else { current_path.push(name); }
        }
        recursively_evaluate_node_formulas(child_ptr, node as *const OverseerNode, current_path, document_root);
        current_path.pop();
    }
}

/// Selective version that only evaluates formulas for nodes in specific field paths
unsafe fn recursively_evaluate_node_formulas_selective(
    node_ptr: *mut OverseerNode,
    _parent_ptr: *const OverseerNode,
    current_path: &mut Vec<String>,
    document_root: &[OverseerNode],
    field_paths: &std::collections::HashSet<String>,
) {
    let node: &mut OverseerNode = &mut *node_ptr;
    let _parent_ref: Option<&OverseerNode> = if _parent_ptr.is_null() { None } else { Some(&*_parent_ptr) };
    
    // Skip evaluating formulas for nodes inside action handler blocks
    if let Some(p) = _parent_ref {
        if p.node_type == "on" {
            return;
        }
    }
    
    // Check if this node's path is in the fields we need to update
    let current_path_str = current_path.join("/");
    let should_evaluate_this_node = field_paths.contains(&current_path_str) || 
        field_paths.iter().any(|path| path.starts_with(&current_path_str));
    
    if should_evaluate_this_node {
    debug_resolver!("🔄 Selectively evaluating formulas for node at path: {}", current_path_str);
        
        // Same formula evaluation logic as the main function
        let formula_pairs: Vec<(String, String)> = node
            .parameters
            .iter()
            .filter_map(|(k, v)| match v {
                OverseerValue::Formula(s) => Some((k.clone(), s.clone())),
                _ => None,
            })
            .collect();

        // Run up to 2 passes for intra-node dependencies
        for _ in 0..2 {
            let context = EvaluationContext::new_with_current_and_parent(node, _parent_ref, current_path.to_vec(), document_root);
            let mut computed_params: Vec<(String, OverseerValue)> = Vec::new();
            for (key, formula_src) in &formula_pairs {
                debug_resolver!("[RESOLVER] Selectively evaluating formula in {}.{}: {}", node.name, key, formula_src);
                let shadow_key = if key == "value" { "_computed_value".to_string() } else { format!("_computed_{}", key) };
                match FormulaEvaluator::evaluate_formula(formula_src.as_str(), &context) {
                    Ok(result) => {
                        debug_resolver!("[RESOLVER] Formula result: {:?}", result);
                        computed_params.push((shadow_key, result));
                    }
                    Err(_err) => {
                        debug_resolver!("[RESOLVER] Formula error at {}.{}", node.name, key);
                        computed_params.push((shadow_key, OverseerValue::String("invalid formula error".to_string())));
                    }
                }
            }
            drop(context);
            for (k, v) in computed_params {
                node.parameters.insert(k, v);
            }
        }
    }
    
    // Always recurse into children to check their paths
    let child_len = node.children.len();
    for idx in 0..child_len {
        let child_ptr: *mut OverseerNode = &mut node.children[idx] as *mut _;
        {
            let child_ref = &*child_ptr;
            let name = child_ref.name.clone();
            let k = node.children.iter().take(idx).filter(|c| c.name == name).count();
            if k > 0 { current_path.push(format!("{}#{}", name, k)); } else { current_path.push(name); }
        }
        recursively_evaluate_node_formulas_selective(child_ptr, node as *const OverseerNode, current_path, document_root, field_paths);
        current_path.pop();
    }
}

// Note: child formula evaluation is handled via recursively_evaluate_node_formulas above

/// Compute UI sort keys for list items when a list declares sort_by (lambda or expression)
fn compute_list_ui_sort_keys(nodes: &mut Vec<OverseerNode>) {
    let snapshot = nodes.clone();
    let len = nodes.len();
    for i in 0..len {
        let node_ptr: *mut OverseerNode = &mut nodes[i] as *mut _;
        let mut current_path = vec![unsafe { (&*node_ptr).name.clone() }];
        unsafe { recursively_compute_sort_keys(node_ptr, std::ptr::null(), &mut current_path, &snapshot); }
    }
}

unsafe fn recursively_compute_sort_keys(
    node_ptr: *mut OverseerNode,
    _parent_ptr: *const OverseerNode,
    current_path: &mut Vec<String>,
    document_root: &[OverseerNode],
) {
    use crate::types::OverseerValue;
    use crate::formula_evaluator::{EvaluationContext, FormulaEvaluator};

    let node: &mut OverseerNode = &mut *node_ptr;

    // For list nodes, if sort_by parameter is present (as Formula or String), compute per-item keys
    if node.node_type == "list" {
        if let Some(sort_expr_val) = node.parameters.get("sort_by") {
            let sort_src = match sort_expr_val {
                OverseerValue::Formula(s) => s.as_str(),
                OverseerValue::String(s) => s.as_str(),
                _ => "",
            };
            if !sort_src.is_empty() {
                for (idx, child) in node.children.iter_mut().enumerate() {
                    // Build context for evaluating against the child; bind as current and provide parent
                    let mut path = current_path.clone();
                    path.push(child.name.clone());
                    let ctx = EvaluationContext::new_with_current_and_parent(child, Some(&*node_ptr), path, document_root);
                    let key = FormulaEvaluator::evaluate_lambda_on_item(sort_src, &ctx, child)
                        .unwrap_or(OverseerValue::Integer(idx as i64));
                    // Store as internal UI-only key
                    child.parameters.insert("_ui_sort_key".to_string(), key);
                }
            }
        }
    }

    // Recurse into children
    for c in 0..node.children.len() {
        let child_ptr: *mut OverseerNode = &mut node.children[c] as *mut _;
        {
            let child_ref = &*child_ptr;
            let name = child_ref.name.clone();
            let k = node.children
                .iter()
                .take(c)
                .filter(|c| c.name == name)
                .count();
            if k > 0 { current_path.push(format!("{}#{}", name, k)); }
            else { current_path.push(name); }
        }
        recursively_compute_sort_keys(child_ptr, node as *const OverseerNode, current_path, document_root);
        current_path.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_document;

    #[test]
    fn test_layout_resolution_vertical() {
        let input = r#"div Container (layout=vertical) {
            div child1 { string field = "First" }
            div child2 { string field = "Second" }
        }"#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        
        let container = &nodes[0];
        assert_eq!(container.parameters.get("_effective_layout"), Some(&OverseerValue::String("vertical".to_string())));
        
        // Children should alternate to horizontal
        let child1 = &container.children[0];
        let child2 = &container.children[1];
        assert_eq!(child1.parameters.get("_effective_layout"), Some(&OverseerValue::String("horizontal".to_string())));
        assert_eq!(child2.parameters.get("_effective_layout"), Some(&OverseerValue::String("horizontal".to_string())));
    }

    #[test]
    fn test_layout_resolution_horizontal() {
        let input = r#"div Container (layout=horizontal) {
            div child1 { string field = "First" }
            div child2 { string field = "Second" }
        }"#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        
        let container = &nodes[0];
        assert_eq!(container.parameters.get("_effective_layout"), Some(&OverseerValue::String("horizontal".to_string())));
        
        // Children should alternate to vertical
        let child1 = &container.children[0];
        let child2 = &container.children[1];
        assert_eq!(child1.parameters.get("_effective_layout"), Some(&OverseerValue::String("vertical".to_string())));
        assert_eq!(child2.parameters.get("_effective_layout"), Some(&OverseerValue::String("vertical".to_string())));
    }

    #[test]
    fn test_layout_resolution_inherit() {
        let input = r#"div Outer (layout=horizontal) {
            div Inner (layout=inherit) {
                div child { string field = "Test" }
            }
        }"#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        
        let outer = &nodes[0];
        let inner = &outer.children[0];
        let child = &inner.children[0];
        
        assert_eq!(outer.parameters.get("_effective_layout"), Some(&OverseerValue::String("horizontal".to_string())));
        assert_eq!(inner.parameters.get("_effective_layout"), Some(&OverseerValue::String("horizontal".to_string())));
        assert_eq!(child.parameters.get("_effective_layout"), Some(&OverseerValue::String("vertical".to_string())));
    }

    #[test]
    fn test_layout_resolution_opposite() {
        let input = r#"div Outer (layout=horizontal) {
            div Inner (layout=opposite) {
                div child { string field = "Test" }
            }
        }"#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        
        let outer = &nodes[0];
        let inner = &outer.children[0];
        let child = &inner.children[0];
        
        assert_eq!(outer.parameters.get("_effective_layout"), Some(&OverseerValue::String("horizontal".to_string())));
        assert_eq!(inner.parameters.get("_effective_layout"), Some(&OverseerValue::String("vertical".to_string())));
        assert_eq!(child.parameters.get("_effective_layout"), Some(&OverseerValue::String("horizontal".to_string())));
    }

    #[test]
    fn test_layout_resolution_default_alternation() {
        let input = r#"div Outer {
            div Inner {
                div child { string field = "Test" }
            }
        }"#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        
        let outer = &nodes[0];
        let inner = &outer.children[0];
        let child = &inner.children[0];
        
        // Default should be horizontal for root, then alternate
        assert_eq!(outer.parameters.get("_effective_layout"), Some(&OverseerValue::String("horizontal".to_string())));
        assert_eq!(inner.parameters.get("_effective_layout"), Some(&OverseerValue::String("vertical".to_string())));
        assert_eq!(child.parameters.get("_effective_layout"), Some(&OverseerValue::String("horizontal".to_string())));
    }

    #[test]
    fn test_template_resolution() {
        let input = r#"
        div Task (hidden=true) {
            string description = ""
            checkbox complete = false
        }
        
        <Task> my_task {
            string description = "My custom task"
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        
        // After resolution, we should still have both nodes (template and instance)
        // But hidden templates may be filtered out in actual rendering, not in tests
    debug_resolver!("Number of nodes after resolution: {}", nodes.len());
        for (i, node) in nodes.iter().enumerate() {
            debug_resolver!("Node {}: {} (type: {})", i, node.name, node.node_type);
        }
        
        // Find the resolved task (it should be the second node, or the only non-hidden one)
        let resolved_task = if nodes.len() == 2 {
            &nodes[1] // Both template and instance present
        } else {
            &nodes[0] // Only instance present (template filtered out)
        };
        
    debug_resolver!("Resolved task children: {}", resolved_task.children.len());
        for (i, child) in resolved_task.children.iter().enumerate() {
            debug_resolver!("  Child {}: {} (type: {})", i, child.name, child.node_type);
        }
        
    assert_eq!(resolved_task.node_type, "Task");
    // New behavior: copy all template fields, then merge overrides
    assert_eq!(resolved_task.children.len(), 2);
    // description should be overridden
    let description = resolved_task.get_accessible_children().into_iter().find(|c| c.name == "description").unwrap();
    assert_eq!(description.parameters.get("value"), Some(&OverseerValue::String("My custom task".to_string())));
    // checkbox should be present with default value from template
    let complete = resolved_task.get_accessible_children().into_iter().find(|c| c.name == "complete").unwrap();
    assert_eq!(complete.parameters.get("value"), Some(&OverseerValue::Boolean(false)));
    }

    #[test]
    fn test_overrides_persist_through_unnamed_div_in_template() {
        // Template Bug has an unnamed div grouping fields. Instance overrides should persist.
        let input = r#"
        div Bug {
            string description = ""
            div {
                int storypoints = 1
                int priority = 0
                checkbox fixed = false
            }
        }

        list BugList (entry=<Bug>) {
            - {
                - description = "override text"
                - storypoints = 5
                - priority = 2
                - fixed = true
            }
        }
        "#;

        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);

        // Find BugList list entry
        let bug_list = nodes.iter().find(|n| n.name == "BugList").expect("list present");
        assert_eq!(bug_list.node_type, "list");
        assert_eq!(bug_list.children.len(), 1);
        let item = &bug_list.children[0];
        // After resolution, list entry should be of type Bug with fields accessible (transparent unnamed div)
        assert_eq!(item.node_type, "Bug");
        // Fetch children in a transparent-aware way and verify overrides
    let children = item.get_accessible_children();
    let desc = children.iter().copied().find(|c| c.name == "description").expect("description field");
        assert_eq!(desc.parameters.get("value"), Some(&OverseerValue::String("override text".to_string())));
    let sp = children.iter().copied().find(|c| c.name == "storypoints").expect("storypoints field");
        assert_eq!(sp.parameters.get("value"), Some(&OverseerValue::Integer(5)));
    let pr = children.iter().copied().find(|c| c.name == "priority").expect("priority field");
        assert_eq!(pr.parameters.get("value"), Some(&OverseerValue::Integer(2)));
    let fx = children.iter().copied().find(|c| c.name == "fixed").expect("fixed field");
        assert_eq!(fx.parameters.get("value"), Some(&OverseerValue::Boolean(true)));

        // Serialize and reparse to simulate a round-trip; overrides should persist
        let ser = crate::file_ops::OverseerFileHandler::serialize_nodes(&nodes).unwrap();
        let mut nodes2 = parse_document(&ser).unwrap().1;
        resolve_document(&mut nodes2);
        let bug_list2 = nodes2.iter().find(|n| n.name == "BugList").unwrap();
        let item2 = &bug_list2.children[0];
    let ch2 = item2.get_accessible_children();
    let f_desc = ch2.iter().copied().find(|c| c.name == "description").expect("desc2");
    assert_eq!(f_desc.parameters.get("value"), Some(&OverseerValue::String("override text".to_string())));
    let f_sp = ch2.iter().copied().find(|c| c.name == "storypoints").expect("sp2");
    assert_eq!(f_sp.parameters.get("value"), Some(&OverseerValue::Integer(5)));
    let f_pr = ch2.iter().copied().find(|c| c.name == "priority").expect("pr2");
    assert_eq!(f_pr.parameters.get("value"), Some(&OverseerValue::Integer(2)));
    let f_fx = ch2.iter().copied().find(|c| c.name == "fixed").expect("fx2");
    assert_eq!(f_fx.parameters.get("value"), Some(&OverseerValue::Boolean(true)));
    }

    #[test]
    fn test_effective_layout_on_template_instance_container() {
        // Template declares a div; instance should be treated as container using _original_type
        let input = r#"
        div Outer (layout=vertical) {
            div Task (hidden=true) {
                string description = ""
            }
            <Task> my_task {
                string description = "Hello"
            }
        }
        "#;
        let mut nodes = crate::parser::parse_document(input).unwrap().1;
        resolve_document(&mut nodes);

        // Find the instance 'my_task' under Outer
        let outer = &nodes[0];
        let instance = outer.children.iter().find(|c| c.name == "my_task").expect("instance present");
        // Because parent layout is vertical, effective layout for children should be horizontal
        assert_eq!(instance.parameters.get("_effective_layout"), Some(&OverseerValue::String("horizontal".to_string())));
    }

    #[test]
    fn test_parameter_inheritance() {
        let input = r#"div Container (font-color=blue, font-size=16px) {
            div Inner {
                string field = "Test"
            }
            string direct = "Direct child"
        }"#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        
        let container = &nodes[0];
        let inner = &container.children[0];
        let field = &inner.children[0];
        let direct = &container.children[1];
        
        // Container should have its own styling
        assert_eq!(container.parameters.get("font-color"), Some(&OverseerValue::Color(Color::Named("blue".to_string()))));
        assert_eq!(container.parameters.get("font-size"), Some(&OverseerValue::CssSize(CssSize::Pixels(16.0))));
        
        // Inner div should inherit styling parameters
        assert_eq!(inner.parameters.get("font-color"), Some(&OverseerValue::Color(Color::Named("blue".to_string()))));
        assert_eq!(inner.parameters.get("font-size"), Some(&OverseerValue::CssSize(CssSize::Pixels(16.0))));
        
        // Field should inherit from both container and inner
        assert_eq!(field.parameters.get("font-color"), Some(&OverseerValue::Color(Color::Named("blue".to_string()))));
        assert_eq!(field.parameters.get("font-size"), Some(&OverseerValue::CssSize(CssSize::Pixels(16.0))));
        
        // Direct child should inherit from container
        assert_eq!(direct.parameters.get("font-color"), Some(&OverseerValue::Color(Color::Named("blue".to_string()))));
        assert_eq!(direct.parameters.get("font-size"), Some(&OverseerValue::CssSize(CssSize::Pixels(16.0))));
    }

    #[test]
    fn test_parameter_inheritance_override() {
        let input = r#"div Container (font-color=blue, font-size=16px) {
            string child1 = "Default styling"
            string child2 (font-color=red) = "Overridden color"
            string child3 (font-size=20px) = "Overridden size"
        }"#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        
        let container = &nodes[0];
        let child1 = &container.children[0];
        let child2 = &container.children[1];
        let child3 = &container.children[2];
        
        // Child1 should inherit both parameters
        assert_eq!(child1.parameters.get("font-color"), Some(&OverseerValue::Color(Color::Named("blue".to_string()))));
        assert_eq!(child1.parameters.get("font-size"), Some(&OverseerValue::CssSize(CssSize::Pixels(16.0))));
        
        // Child2 should override color but inherit size
        assert_eq!(child2.parameters.get("font-color"), Some(&OverseerValue::Color(Color::Named("red".to_string()))));
        assert_eq!(child2.parameters.get("font-size"), Some(&OverseerValue::CssSize(CssSize::Pixels(16.0))));
        
        // Child3 should override size but inherit color
        assert_eq!(child3.parameters.get("font-color"), Some(&OverseerValue::Color(Color::Named("blue".to_string()))));
        assert_eq!(child3.parameters.get("font-size"), Some(&OverseerValue::CssSize(CssSize::Pixels(20.0))));
    }

    #[test]
    fn test_serialize_standalone_template_instance_keeps_override_shorthand() {
        use crate::file_ops::OverseerFileHandler;
        let input = r#"
        div T {
            int A = 1
            int B = 2
        }
        <T> I {
            - A = 3
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let out = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
    // Expect concise override retained and inherited field omitted in the instance
    assert!(out.contains("<T> I {"));
    assert!(out.contains("- A = 3"));
    // Template block contains int B; instance should not add another occurrence
    let count_int_b = out.matches("int B").count();
    assert_eq!(count_int_b, 1, "should not serialize inherited B inside instance");
    // Ensure the instance did not expand to concrete type assignment
    assert!(!out.contains("int A = 3"));
    }

    #[test]
    fn test_explicit_equal_override_is_preserved() {
        use crate::file_ops::OverseerFileHandler;
        let input = r#"
        div T {
            int A = 1
            int B = 2
        }
        <T> I {
            - B = 2
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let out = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        // The explicit override of B should be preserved as a concise override
        assert!(out.contains("<T> I {"));
        assert!(out.contains("- B = 2"), "explicit override equal to default must persist");
        // And it should not expand to a full field or duplicate the template field
        let count_int_b = out.matches("int B").count();
        assert_eq!(count_int_b, 1, "template field 'int B' should not be duplicated inside instance");
    }

    #[test]
    fn test_instance_does_not_serialize_inherited_child() {
        use crate::file_ops::OverseerFileHandler;
        let input = r#"
        div T {
            int A = 1
            int B = 2
            int C = 3
        }
        <T> I {
            - A = 3
            - B = 2
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let out = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        // C should only appear in the template block, not inside the instance
        let count_int_c = out.matches("int C").count();
        assert_eq!(count_int_c, 1, "inherited C must not be serialized inside instance");
        assert!(!out.contains("- C = 3"), "concise override for C must not appear since C was not overridden");
    }

    #[test]
    fn test_inherited_styling_not_serialized_on_children() {
        use crate::file_ops::OverseerFileHandler;
        let input = r#"
        div Exercise (background-color=#ffcccc) {
            string Name = "Pushups"
            div History {
                list items(entry=div) {
                    div E1 { string when = "2024-01-01" }
                    div E2 { string when = "2024-01-02" }
                }
            }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let out = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        // Parent Exercise should have background-color emitted once
        assert!(out.contains("div Exercise (background-color=#ffcccc)"));
        // Children should not have background-color persisted
        let child_bc_mentions = out.matches("background-color").count();
        assert_eq!(child_bc_mentions, 1, "inherited background-color should not be serialized on children");
    }

    #[test]
    fn test_background_color_formula_computes() {
        // Use r## to allow embedded sequences like "#abcd" without terminating the raw string
        let input = r##"
        div D (background-color=$(days_since(test_date) >= 1 ? "#4b0a0aff" : "#bba0a0ff")) {
            timestamp test_date(mode="elapsed") = "2025-08-10T19:33:55.706634+00:00"
        }
        "##;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let d = &nodes[0];
        // Should have a computed background-color shadow
        assert!(d.parameters.contains_key("_computed_background-color"));
        let comp = d.parameters.get("_computed_background-color").unwrap();
        // Since test_date is at least 1 day before now, expect the first color branch
        match comp {
            OverseerValue::Color(Color::Hex(hex)) => assert_eq!(hex, "#4b0a0aff"),
            OverseerValue::String(s) => assert_eq!(s, "#4b0a0aff"),
            _ => panic!("unexpected computed color: {:?}", comp),
        }
    }

    #[test]
    fn test_mount_defaults_and_validation() {
        let input = r#"
        div Root {
            mount M1 (source="history.os") { }
            mount M2 { }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let root = &nodes[0];
        let m1 = root.get_accessible_children().into_iter().find(|c| c.name == "M1").unwrap();
        let m2 = root.get_accessible_children().into_iter().find(|c| c.name == "M2").unwrap();
        // M1: has source -> status defaults to 'unloaded'; lazy defaults true via _computed_lazy
        assert_eq!(m1.parameters.get("_mount_status"), Some(&OverseerValue::String("unloaded".to_string())));
        assert_eq!(m1.parameters.get("_computed_lazy"), Some(&OverseerValue::Boolean(true)));
        // M2: missing source -> error status and _mount_error present
        assert_eq!(m2.parameters.get("_mount_status"), Some(&OverseerValue::String("error".to_string())));
        assert!(m2.parameters.get("_mount_error").is_some());
    }

    #[test]
    fn test_files_function_stub_allows_count_reduce() {
        let input = r#"
        div Root {
            int n = $(files("*.os").count())
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let root = &nodes[0];
        let n = root.get_accessible_children().into_iter().find(|c| c.name == "n").unwrap();
        // Phase 1 stub returns empty list, so count is 0
        assert_eq!(n.parameters.get("_computed_value"), Some(&OverseerValue::Integer(0)));
    }

    #[test]
    fn test_parent_bg_depends_on_child_formula_computes_on_load() {
        // Parent background-color references a child field that itself is a formula.
    let input = r##"
        div Parent (background-color=$(score > 0 ? "inherit" : "#000000ff")) {
            int score = $(1)
        }
    "##;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let parent = &nodes[0];
        let bg = parent.parameters.get("_computed_background-color").cloned().expect("bg computed");
        match bg { OverseerValue::String(_) | OverseerValue::Color(_) => {}, other => panic!("unexpected bg: {:?}", other) }
    }

    #[test]
    fn test_chart_series_computed_simple() {
        let input = r#"
        div Root {
            div Data {
                div a { int t = 1 int v = 2 }
                div b { int t = 2 int v = 3 }
            }
            chart C {
                plot P1 (source="/Root/Data", x=$(x/t), y=$(x/v), color=red)
            }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let root = &nodes[0];
        let chart = root.get_accessible_children().into_iter().find(|c| c.node_type == "chart").unwrap();
        // Bounds should exist
        assert!(matches!(chart.parameters.get("_computed_x_min"), Some(OverseerValue::Float(1.0))));
        assert!(matches!(chart.parameters.get("_computed_x_max"), Some(OverseerValue::Float(2.0))));
        assert!(matches!(chart.parameters.get("_computed_y_min"), Some(OverseerValue::Float(2.0))));
        assert!(matches!(chart.parameters.get("_computed_y_max"), Some(OverseerValue::Float(3.0))));
        // Plot series should be computed
        let plot = chart.children.iter().find(|c| c.name == "P1").unwrap();
        let series = plot.parameters.get("_computed_series").cloned().unwrap();
        match series {
            OverseerValue::String(s) => {
                assert!(s.contains("[1,2]"));
                assert!(s.contains("[2,3]"));
            }
            _ => panic!("expected string json series"),
        }
    }

    #[test]
    fn test_chart_series_with_timestamp_x_strings() {
        let input = r#"
        div Root {
            list History (entry=record) {
                - { string time = "2025-08-10T19:33:55.706634+00:00" float weight = 9.6 }
                - { string time = "2025-08-12T10:53:02.760779800+00:00" float weight = 10.2 }
            }
            chart C {
                plot P (source="/Root/History", x=$(|x| x/time), y=$(|x| x/weight))
            }
        }
        "#;
        let mut nodes = crate::parser::parse_document(input).unwrap().1;
        super::resolve_document(&mut nodes);
        let root = &nodes[0];
        let chart = root.get_accessible_children().into_iter().find(|c| c.node_type == "chart").unwrap();
        // Bounds must be computed and increasing in x
        let xmin = match chart.parameters.get("_computed_x_min") { Some(crate::types::OverseerValue::Float(f)) => *f, _ => -1.0 };
        let xmax = match chart.parameters.get("_computed_x_max") { Some(crate::types::OverseerValue::Float(f)) => *f, _ => -1.0 };
        assert!(xmax > xmin);
        // Plot should have _computed_series
        let plot = chart.children.iter().find(|c| c.node_type == "plot").unwrap();
        let ser = plot.parameters.get("_computed_series");
        assert!(matches!(ser, Some(crate::types::OverseerValue::String(_))));
    }

    #[test]
    fn test_chart_series_with_processed_list_source() {
        let input = r#"
        div Root {
            list History (entry=record) {
                - { string time = "2025-08-10T19:33:55.706634+00:00" int reps = 12 }
                - { string time = "2025-08-12T10:53:02.760779800+00:00" int reps = 18 }
                - { string time = "2025-08-12T11:00:00.000000000+00:00" int reps = 22 }
            }
            chart C {
                // Use a processed list: only items with reps > 15
                plot P (source=$(/Root/History.filter(|x| x/reps > 15)), x=$(|x| x/time), y=$(|x| x/reps))
            }
        }
        "#;
        let mut nodes = crate::parser::parse_document(input).unwrap().1;
        super::resolve_document(&mut nodes);
        let root = &nodes[0];
        let chart = root.get_accessible_children().into_iter().find(|c| c.node_type == "chart").unwrap();
        // Bounds must be computed
        assert!(chart.parameters.get("_computed_x_min").is_some());
        assert!(chart.parameters.get("_computed_x_max").is_some());
        assert!(chart.parameters.get("_computed_y_min").is_some());
        assert!(chart.parameters.get("_computed_y_max").is_some());
        // Plot should have _computed_series with two points (reps 18 and 22)
        let plot = chart.children.iter().find(|c| c.node_type == "plot").unwrap();
        if let Some(OverseerValue::String(s)) = plot.parameters.get("_computed_series") {
            // Count occurrences of opening bracket '[' minus 1 for the array start, or parse
            let series: Vec<(f64, f64)> = serde_json::from_str(s).expect("valid series json");
            assert_eq!(series.len(), 2);
        } else {
            panic!("expected _computed_series string");
        }
    }

    // Note: additional integration tests for templated list items can be added once renderer/runtime semantics are finalized.

        #[test]
        fn test_formulas_inside_on_blocks_are_skipped_on_resolve() {
                // Ensure formulas within action payloads aren't evaluated during resolve (avoids recursion/crash)
                let input = r#"
div Root {
    string input = "Hello"
    list L (entry=string) { }
    button Create {
        on click {
            append(list="/Root/L") { - value = $(/Root/input) }
        }
    }
}
"#;
                let mut nodes = crate::parser::parse_document(input).unwrap().1;
                // Just resolving should not evaluate the formula inside on click block
                resolve_document(&mut nodes);
                // Find the on click node under Create and ensure its child parameter is still a Formula (no _computed_value)
                let root = &nodes[0];
                let btn = root.children.iter().find(|c| c.name == "Create").expect("Create button present");
                let on_click = btn.children.iter().find(|c| c.node_type == "on" && c.name == "click").expect("on click present");
                // Under on click, there's an append action with a child override node having value as Formula
                let append = on_click.children.first().expect("append action present");
                assert_eq!(append.node_type, "append");
                let ov = append.children.first().expect("override child present");
                // It should keep a Formula for 'value' and not have a computed shadow
                match ov.parameters.get("value") {
                        Some(OverseerValue::Formula(_)) => {},
                        other => panic!("expected raw Formula in action payload, got {:?}", other),
                }
                assert!(ov.parameters.get("_computed_value").is_none(), "no computed shadow should be created under on-blocks");
        }
}
