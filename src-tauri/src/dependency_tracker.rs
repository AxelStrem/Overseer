use std::collections::{HashMap, HashSet};
use crate::types::{OverseerNode, OverseerValue, OverseerError};
// use crate::formula_evaluator::FormulaEvaluator; // not used in this module

/// Information about a timer node
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct TimerInfo {
    pub node_path: String,
    pub next_due: Option<i64>,
    pub interval_ms: Option<i64>,
    pub last_fired: Option<i64>,
    pub active: bool,
}

/// Represents a field update that needs to be propagated
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct FieldUpdate {
    pub path: String,
    pub new_value: OverseerValue,
    pub cascaded: bool, // true if this update was caused by another update
}

/// Tracks dependencies between fields in the document
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    /// Maps field path to list of dependent field paths
    /// Example: "task_list/total_count" -> ["summary/completed_ratio", "dashboard/progress"]
    dependencies: HashMap<String, Vec<String>>,
    
    /// Reverse mapping: dependent field -> fields it depends on
    /// Example: "summary/completed_ratio" -> ["task_list/total_count", "task_list/completed_count"]
    dependents: HashMap<String, Vec<String>>,
    
    /// Timer nodes and their state
    timers: HashMap<String, TimerInfo>,
    
    /// Template instance relationships: instance_path -> template_path
    template_instances: HashMap<String, String>,
    
    /// Cache of formula dependencies to avoid re-parsing
    formula_cache: HashMap<String, Vec<String>>,
}

// Debug logging macro for dependency tracker
macro_rules! debug_dep {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-deps")]
        println!($($arg)*);
    };
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            dependencies: HashMap::new(),
            dependents: HashMap::new(),
            timers: HashMap::new(),
            template_instances: HashMap::new(),
            formula_cache: HashMap::new(),
        }
    }

    /// Build the dependency graph by analyzing the document
    pub fn build_from_document(&mut self, nodes: &[OverseerNode]) -> Result<(), OverseerError> {
        self.clear();
        
        debug_dep!("🔍 Building dependency graph from {} top-level nodes", nodes.len());
    for (_i, _node) in nodes.iter().enumerate() {
            debug_dep!("🔍 Analyzing top-level node {}: '{}' type='{}' params={:?}", 
                     i, node.name, node.node_type, node.parameters.keys().collect::<Vec<_>>());
        }
        
        // Walk the document tree and extract dependencies
        for node in nodes {
            self.analyze_node(node, &mut Vec::new())?;
        }

        // SECOND PASS: Expand synthetic aggregate markers produced by extract_formula_dependencies
        // Pattern: synthetic key "__agg__:<context>/<list_path>/<field_ident>"
        // We map this to dependencies on each current list item field's value parameter.
        // Approach: collect all (dependent -> synthetic_dep) pairs, then for each synthetic
        // generate real dependencies.
        let mut aggregate_links: Vec<(String, String)> = Vec::new(); // (dependent, synthetic)
        for (dependency, dependents) in self.dependencies.clone().into_iter() {
            if dependency.starts_with("__agg__:") {
                for d in dependents { aggregate_links.push((d.clone(), dependency.clone())); }
            }
        }
        if !aggregate_links.is_empty() {
            // Helper closure to find node by name path (transparent unaware; relies on actual stored names)
            fn find_node_mut<'a>(roots: &'a [OverseerNode], path: &str) -> Option<&'a OverseerNode> {
                let mut cur_slice: &[OverseerNode] = roots;
                let mut found: Option<&OverseerNode> = None;
                for seg in path.split('/') {
                    found = cur_slice.iter().find(|n| n.name == seg);
                    if let Some(f) = found { cur_slice = &f.children; } else { return None; }
                }
                found
            }
            for (dependent, synthetic) in aggregate_links {
                // synthetic format: __agg__:<context>/<list_path>/<field_ident>
                if let Some(rest) = synthetic.strip_prefix("__agg__:") {
                    // Split rest into context + list path + field ident by walking from right
                    let parts: Vec<&str> = rest.split('/').collect();
                    if parts.len() >= 2 {
                        let field_ident = parts.last().unwrap().to_string();
                        // list path may include context segments; we attempt to find the list node directly
                        let list_path = parts[..parts.len()-1].join("/");
                        if let Some(list_node) = find_node_mut(nodes, &list_path) {
                            // For each item child of list, locate target field (traverse transparency) and add dependency
                            for item in &list_node.children {
                                // BFS to find field_ident underneath item (respect transparency via is_hierarchy_transparent)
                                let mut queue: Vec<&OverseerNode> = Vec::new();
                                queue.extend(&item.children);
                                while let Some(ch) = queue.pop() {
                                    if ch.name == field_ident {
                                        let field_value_path = format!("{}/value", {
                                            // Reconstruct path: list_path + item + (trail down to field)
                                            let mut p = Vec::new();
                                            p.extend(list_path.split('/').map(|s| s.to_string()));
                                            p.push(item.name.clone());
                                            p.push(ch.name.clone());
                                            p.join("/")
                                        });
                                        self.add_dependency(dependent.clone(), field_value_path);
                                        break; // found for this item
                                    }
                                    if ch.is_hierarchy_transparent { queue.extend(&ch.children); }
                                }
                            }
                        }
                    }
                }
                // Remove synthetic mapping (optional: skip for now - leaving it won't hurt but might cause redundant lookups)
            }
        }
        
    debug_dep!("🔍 Dependency graph built. Dependencies: {:?}", self.dependencies);
    debug_dep!("🔍 Dependents: {:?}", self.dependents);
        
        Ok(())
    }

    /// Clear all tracked dependencies
    pub fn clear(&mut self) {
        self.dependencies.clear();
        self.dependents.clear();
        self.timers.clear();
        self.template_instances.clear();
        self.formula_cache.clear();
    }

    /// Add a dependency relationship
    pub fn add_dependency(&mut self, dependent: String, dependency: String) {
        // Add to forward mapping
        self.dependencies.entry(dependency.clone())
            .or_insert_with(Vec::new)
            .push(dependent.clone());
        
        // Add to reverse mapping
        self.dependents.entry(dependent)
            .or_insert_with(Vec::new)
            .push(dependency);
    }

    /// Get all fields that depend on the given field
    pub fn get_dependents(&self, field_path: &str) -> Vec<String> {
        self.dependencies.get(field_path).cloned().unwrap_or_default()
    }

    /// Get all fields that the given field depends on
    #[allow(dead_code)]
    pub fn get_dependencies(&self, field_path: &str) -> Vec<String> {
        self.dependents.get(field_path).cloned().unwrap_or_default()
    }

    /// Calculate which fields need to be updated when a field changes
    pub fn calculate_update_cascade(&self, changed_field: &str) -> Vec<String> {
        let mut to_update = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = vec![changed_field.to_string()];

        while let Some(current) = queue.pop() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current.clone());

            // Get all fields that depend on current field
            for dependent in self.get_dependents(&current) {
                if !visited.contains(&dependent) {
                    to_update.push(dependent.clone());
                    queue.push(dependent);
                }
            }
        }

        to_update
    }

    /// Add a timer to the tracking system
    pub fn add_timer(&mut self, path: String, timer_info: TimerInfo) {
        self.timers.insert(path, timer_info);
    }

    /// Get all active timers
    #[allow(dead_code)]
    pub fn get_active_timers(&self) -> Vec<&TimerInfo> {
        self.timers.values().filter(|t| t.active).collect()
    }

    /// Get the next timer due time
    #[allow(dead_code)]
    pub fn get_next_timer_due(&self) -> Option<i64> {
        self.get_active_timers()
            .iter()
            .filter_map(|t| t.next_due)
            .min()
    }

    /// Private: Recursively analyze a node for dependencies
    fn analyze_node(&mut self, node: &OverseerNode, path: &mut Vec<String>) -> Result<(), OverseerError> {
        path.push(node.name.clone());
        let node_path = path.join("/");
        
    debug_dep!("🔍 Analyzing node: '{}' at path '{}' type='{}' params={:?}", 
         node.name, node_path, node.node_type, node.parameters.keys().collect::<Vec<_>>());

        // Check if this is a timer node
        if node.node_type == "timer" {
            self.analyze_timer_node(node, &node_path)?;
        }

        // Check for template instances
        if node.template.is_some() {
            if let Some(template_path) = &node.template {
                self.template_instances.insert(node_path.clone(), template_path.clone());
            }
        }

    // Analyze parameters for formula dependencies
        for (param_name, value) in &node.parameters {
            let param_path = format!("{}/{}", node_path, param_name);
            
            debug_dep!("🔍   Parameter '{}' = {:?}", param_name, value);
            
            if let OverseerValue::Formula(formula) = value {
                debug_dep!("🔍   Found formula in '{}': '{}'", param_path, formula);
                // Extract dependencies from this formula
                let dependencies = self.extract_formula_dependencies(formula, &node_path)?;
                debug_dep!("🔍   Dependencies for '{}': {:?}", param_path, dependencies);
                for dep in dependencies {
                    debug_dep!("🔍   Adding dependency: '{}' depends on '{}'", param_path, dep);
                    self.add_dependency(param_path.clone(), dep);
                }
            }
        }

        // Also analyze fallback formulas, which affect a node's effective value when value is Null
        if let Some(OverseerValue::Formula(formula)) = node.parameters.get("fallback") {
            let param_path = format!("{}/{}", node_path, "fallback");
            debug_dep!("🔍   Found fallback formula in '{}': '{}'", param_path, formula);
            let dependencies = self.extract_formula_dependencies(formula, &node_path)?;
            debug_dep!("🔍   Fallback dependencies for '{}': {:?}", param_path, dependencies);
            for dep in dependencies {
                debug_dep!("🔍   Adding dependency: '{}' depends on '{}'", param_path, dep);
                self.add_dependency(param_path.clone(), dep);
            }
            // Additionally, the node's value depends on its fallback when value is Null; map value dependency to fallback
            let value_param = format!("{}/{}", node_path, "value");
            self.add_dependency(value_param, param_path);
        }

        // Recursively analyze children
        for child in &node.children {
            self.analyze_node(child, path)?;
        }

        path.pop();
        Ok(())
    }

    /// Analyze a timer node to extract timing information
    fn analyze_timer_node(&mut self, _node: &OverseerNode, path: &str) -> Result<(), OverseerError> {
    let timer_info = TimerInfo {
            node_path: path.to_string(),
            next_due: None, // Will be calculated by timer system
            interval_ms: None, // Extract from node parameters if available
            last_fired: None,
            active: true,
        };
        
        self.add_timer(path.to_string(), timer_info);
        Ok(())
    }

    /// Extract field dependencies from a formula string
    fn extract_formula_dependencies(&mut self, formula: &str, context_path: &str) -> Result<Vec<String>, OverseerError> {
        // Check cache first
        let cache_key = format!("{}:{}", context_path, formula);
        if let Some(cached) = self.formula_cache.get(&cache_key) {
            return Ok(cached.clone());
        }

        let mut dependencies = Vec::new();
        
        // Parse the formula to extract path references
        // This is a simplified implementation - in reality we'd use the full formula parser
        // For now, look for patterns like "/path/field", "../field", "field"
        
        let path_patterns = self.extract_path_references(formula);
        for pattern in path_patterns {
            let resolved_path = self.resolve_path_reference(&pattern, context_path);
            dependencies.push(resolved_path);
        }

        // SPECIAL CASES: Detect aggregate patterns so we can later expand them into item-field dependencies.
        // 1) Simple pattern: <listIdent>.sum(<fieldIdent>)  e.g. L.sum(C)
        if let Some(agg_caps) = regex::Regex::new(r"^\s*([A-Za-z_][A-Za-z0-9_]*)\.sum\(\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)\s*$").unwrap().captures(formula) {
            let list_ident = agg_caps.get(1).unwrap().as_str();
            let field_ident = agg_caps.get(2).unwrap().as_str();
            let list_path = self.resolve_path_reference(list_ident, context_path);
            // Synthetic marker format (no duplicated context): __agg__:<list_path>/<field_ident>
            let synthetic = format!("__agg__:{}/{}", list_path, field_ident);
            dependencies.push(synthetic);
        }
        // 2) map-sum pattern: <listIdent>.map(|x| x/<fieldIdent>).sum() (very narrow detection)
        if let Some(map_caps) = regex::Regex::new(r"^\s*([A-Za-z_][A-Za-z0-9_]*)\.map\(\s*\|([A-Za-z_][A-Za-z0-9_]*)\|\s*([A-Za-z_][A-Za-z0-9_]*)/([A-Za-z_][A-Za-z0-9_]*)\s*\)\.sum\(\s*\)\s*$").unwrap().captures(formula) {
            let list_ident = map_caps.get(1).unwrap().as_str();
            let var_decl = map_caps.get(2).unwrap().as_str();
            let var_use = map_caps.get(3).unwrap().as_str();
            let field_ident = map_caps.get(4).unwrap().as_str();
            if var_decl == var_use { // ensure x reused consistently
                let list_path = self.resolve_path_reference(list_ident, context_path);
                let synthetic = format!("__agg__:{}/{}", list_path, field_ident);
                dependencies.push(synthetic);
            }
        }

        // Cache the result (including any synthetic additions)
        self.formula_cache.insert(cache_key, dependencies.clone());
        
        Ok(dependencies)
    }

    /// Extract path reference patterns from formula string
    fn extract_path_references(&self, formula: &str) -> Vec<String> {
        use std::collections::HashSet;
        let mut references: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut path_segments: HashSet<String> = HashSet::new();
        
    debug_dep!("🔍   Extracting path references from formula: '{}'", formula);
        
        // 1) Absolute paths like "/root/field_b" (one or more segments)
        let abs_re = regex::Regex::new(r"/[A-Za-z_][A-Za-z0-9_]*(?:/[A-Za-z_][A-Za-z0-9_]*)*").unwrap();
        for m in abs_re.find_iter(formula) {
            let p = m.as_str().to_string();
            if seen.insert(p.clone()) {
                debug_dep!("🔍     Found absolute path: '{}'", p);
                // Track path segments to avoid later duplicate bare identifiers
                for seg in p.trim_start_matches('/').split('/') {
                    path_segments.insert(seg.to_string());
                }
                references.push(p);
            }
        }
        
        // 2) Relative-up paths like "../sibling" or "../../x/y"
        let rel_up_re = regex::Regex::new(r"(?:\.\./)+[A-Za-z_][A-Za-z0-9_]*(?:/[A-Za-z_][A-Za-z0-9_]*)*").unwrap();
        for m in rel_up_re.find_iter(formula) {
            let p = m.as_str().to_string();
            if seen.insert(p.clone()) {
                debug_dep!("🔍     Found relative-up path: '{}'", p);
                // Track segments after the ../ prefixes
                let after = p.trim_start_matches("../");
                for seg in after.split('/') {
                    if !seg.is_empty() { path_segments.insert(seg.to_string()); }
                }
                references.push(p);
            }
        }
        
        // 3) Bare identifiers (e.g., field_a) that are not keywords and not already captured as part of paths
        let ident_re = regex::Regex::new(r"\b[a-zA-Z_][a-zA-Z0-9_]*\b").unwrap();
        for mat in ident_re.find_iter(formula) {
            let token = mat.as_str();
            if self.is_formula_keyword(token) { 
                debug_dep!("🔍     Skipping keyword: '{}'", token);
                continue;
            }
            // Skip if the token is already part of a captured path (as a segment)
            if path_segments.contains(token) { continue; }
            if seen.insert(token.to_string()) {
                debug_dep!("🔍     Found bare field reference: '{}'", token);
                references.push(token.to_string());
            }
        }
        
    debug_dep!("🔍   Extracted references: {:?}", references);
        references
    }
    
    /// Check if a token is a formula keyword that should be ignored
    fn is_formula_keyword(&self, token: &str) -> bool {
        match token {
            // Mathematical / aggregation functions
            "sin" | "cos" | "tan" | "sqrt" | "abs" | "max" | "min" | "floor" | "ceil" | "round" |
            // Collection / list pipeline helpers (treated as keywords so they don't become dependencies)
            "sum" | "map" | "filter" | "reduce" | "count" |
            // Logical keywords  
            "true" | "false" | "and" | "or" | "not" |
            // Control flow
            "if" | "else" | "then" |
            // Common programming keywords
            "let" | "var" | "const" | "function" | "return" |
            // Built-in constants
            "pi" | "e" => true,
            _ => false,
        }
    }

    /// Resolve a path reference relative to the current context
    fn resolve_path_reference(&self, path_ref: &str, context_path: &str) -> String {
    debug_dep!("🔍     Resolving path reference '{}' in context '{}'", path_ref, context_path);
        
        if path_ref.starts_with('/') {
            // Absolute path from root
            let resolved = path_ref.trim_start_matches('/').to_string();
            debug_dep!("🔍     Absolute path resolved to: '{}'", resolved);
            resolved
        } else if path_ref.starts_with("../") {
            // Relative path going up
            let context_parts: Vec<&str> = context_path.split('/').collect();
            let mut resolved_parts = context_parts;
            
            let mut remaining = path_ref;
            while remaining.starts_with("../") {
                if !resolved_parts.is_empty() {
                    resolved_parts.pop();
                }
                remaining = &remaining[3..];
            }
            
            if !remaining.is_empty() {
                resolved_parts.push(remaining);
            }
            
            let resolved = resolved_parts.join("/");
            debug_dep!("🔍     Relative path resolved to: '{}'", resolved);
            resolved
        } else {
            // Simple relative reference (bare identifier): resolve in parent/sibling scope
            // e.g., for context "a/b/c" and ref "x" -> "a/b/x" (NOT "a/b/c/x")
            if context_path.is_empty() {
                let resolved = path_ref.to_string();
                debug_dep!("🔍     Bare identifier at root resolved to: '{}'", resolved);
                return resolved;
            }
            if let Some((parent, _last)) = context_path.rsplit_once('/') {
                let resolved = format!("{}/{}", parent, path_ref);
                debug_dep!("🔍     Bare identifier resolved to sibling path: '{}'", resolved);
                resolved
            } else {
                // Single-segment context (top-level node): sibling is another top-level
                let resolved = path_ref.to_string();
                debug_dep!("🔍     Bare identifier under top-level resolved to: '{}'", resolved);
                resolved
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{OverseerNode, OverseerValue};

    #[test]
    fn test_dependency_tracking() {
        let mut graph = DependencyGraph::new();
        
        // Add some dependencies
        graph.add_dependency("field_a".to_string(), "field_b".to_string());
        graph.add_dependency("field_c".to_string(), "field_b".to_string());
        graph.add_dependency("field_c".to_string(), "field_a".to_string());
        
        // Test getting dependents
        let dependents = graph.get_dependents("field_b");
        assert!(dependents.contains(&"field_a".to_string()));
        assert!(dependents.contains(&"field_c".to_string()));
        
        // Test cascade calculation
        let cascade = graph.calculate_update_cascade("field_b");
        assert!(cascade.contains(&"field_a".to_string()));
        assert!(cascade.contains(&"field_c".to_string()));
    }

    #[test]
    fn test_path_resolution() {
        let graph = DependencyGraph::new();
        
        // Test absolute path
        assert_eq!(graph.resolve_path_reference("/root/field", "context/path"), "root/field");
        
        // Test relative path up
        assert_eq!(graph.resolve_path_reference("../sibling", "context/path"), "context/sibling");
        
        // Test bare identifier resolves to sibling scope (parent of context)
        assert_eq!(graph.resolve_path_reference("child", "context/path"), "context/child");
    }

    #[test]
    fn test_path_extraction() {
        let graph = DependencyGraph::new();
        
        let formula = "$(field_a + /root/field_b + ../sibling)";
        let paths = graph.extract_path_references(formula);
        
        assert!(paths.iter().any(|p| p.contains("field_a")));
        assert!(paths.iter().any(|p| p.contains("/root/field_b")));
        assert!(paths.iter().any(|p| p.contains("../sibling")));
    }

    #[test]
    fn test_dependencies_with_transparent_wrapper_in_list_item() {
        // Build a minimal tree with transparent wrapper around A inside a list item
        let mut root = OverseerNode::new_with_type("tab".to_string(), Some("Root".to_string()));
        let mut list = OverseerNode::new_with_type("list".to_string(), Some("L".to_string()));
        let mut item = OverseerNode::new_with_type("T".to_string(), Some("T__1".to_string()));
        let mut wrap = OverseerNode::new_with_type("div".to_string(), None); // unnamed transparent div
        let mut a = OverseerNode::new_with_type("int".to_string(), Some("A".to_string()));
        a.parameters.insert("value".to_string(), OverseerValue::Integer(1));
        let mut b = OverseerNode::new_with_type("int".to_string(), Some("B".to_string()));
        b.parameters.insert("value".to_string(), OverseerValue::Integer(2));
        let mut c = OverseerNode::new_with_type("int".to_string(), Some("C".to_string()));
        c.parameters.insert("value".to_string(), OverseerValue::Formula("A*B".to_string()));

        wrap.children.push(a);
        item.children.push(wrap);
        item.children.push(b);
        item.children.push(c);
        list.children.push(item);
        root.children.push(list);
        let nodes = vec![root];

        let mut graph = DependencyGraph::new();
        graph.build_from_document(&nodes).expect("build graph");

        // Expect that C/value depends on sibling-scope A and B (not C/A)
        let dep_a = "Root/L/T__1/A";
        let dep_b = "Root/L/T__1/B";
        let dependent = "Root/L/T__1/C/value";
        let dependents_of_a = graph.get_dependents(dep_a);
        let dependents_of_b = graph.get_dependents(dep_b);
        assert!(dependents_of_a.contains(&dependent.to_string()), "C/value should depend on A");
        assert!(dependents_of_b.contains(&dependent.to_string()), "C/value should depend on B");
    }

    #[test]
    fn test_aggregate_map_sum_expansion_dependencies() {
        // Document structure replicating examples/basic/unnamed_divs.os simplified
        // tab Root {
        //   int total = $(L.map(|x| x/C).sum())
        //   list L { - { int A=1 int B=2 int C=$(A*B) } }
        // }
        let mut root = OverseerNode::new_with_type("tab".to_string(), Some("Root".to_string()));
        let mut total = OverseerNode::new_with_type("int".to_string(), Some("total".to_string()));
        total.parameters.insert("value".to_string(), OverseerValue::Formula("L.map(|x| x/C).sum()".to_string()));

        let mut list = OverseerNode::new_with_type("list".to_string(), Some("L".to_string()));
        // single item
        let mut item = OverseerNode::new_with_type("T".to_string(), Some("T__1".to_string()));
        let mut a = OverseerNode::new_with_type("int".to_string(), Some("A".to_string()));
        a.parameters.insert("value".to_string(), OverseerValue::Integer(1));
        let mut b = OverseerNode::new_with_type("int".to_string(), Some("B".to_string()));
        b.parameters.insert("value".to_string(), OverseerValue::Integer(2));
        let mut c = OverseerNode::new_with_type("int".to_string(), Some("C".to_string()));
        c.parameters.insert("value".to_string(), OverseerValue::Formula("A*B".to_string()));
        item.children.push(a);
        item.children.push(b);
        item.children.push(c);
        list.children.push(item);

        root.children.push(total);
        root.children.push(list);
        let nodes = vec![root];

        let mut graph = DependencyGraph::new();
        graph.build_from_document(&nodes).expect("graph build");

        // We expect total/value to depend on Root/L/T__1/C/value via synthetic aggregate expansion
        let c_value = "Root/L/T__1/C/value";
        let total_value = "Root/total/value";
        let dependents_of_c = graph.get_dependents(c_value);
        assert!(dependents_of_c.contains(&total_value.to_string()), "total/value should depend on C/value via aggregate expansion");
    }
}
