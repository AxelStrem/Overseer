use crate::resolver;
use crate::types::{OverseerError, OverseerNode, OverseerValue};
use chrono::{Local, Utc};

// Debug logging macro for actions
macro_rules! debug_actions {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-resolver")] // reuse resolver flag for now
        println!($($arg)*);
    };
}

pub struct ActionExecutor;

impl ActionExecutor {
    fn opposite_layout(layout: &str) -> String {
        match layout {
            "horizontal" => "vertical".to_string(),
            _ => "horizontal".to_string(), // default opposite of vertical
        }
    }
    /// Execute an on <eventName> block under the node at node_path, mutating the document.
    /// Transaction: apply actions in order, then run resolve_document once.
    pub fn execute_event(
        nodes: &mut Vec<OverseerNode>,
        node_path: &[String],
        event_name: &str,
    ) -> Result<(), OverseerError> {
        debug_actions!("[ACTIONS] execute_event at {:?} on '{}'", node_path, event_name);
        // 1) Locate owning node mutably by path
        let (owner_ptr, owner_indices) = match Self::get_node_mut_by_path(nodes, node_path) {
            Some(res) => res,
            None => {
                eprintln!("[ACTIONS] Owner node not found at path {:?}", node_path);
                return Err(OverseerError::ValidationError(format!(
                    "Owner node not found at path {:?}",
                    node_path
                )));
            }
        };

        // SAFETY: we use raw pointer to allow nested borrows during traversal of action children
        let owner: &mut OverseerNode = unsafe { &mut *owner_ptr };

        // 2) Find matching on block(s)
    let mut executed_any = false;
    eprintln!("[ACTIONS] Owner: {} (type={}) children: {:?}", owner.name, owner.node_type, owner.children.iter().map(|c| format!("{}/{}", c.node_type, c.name)).collect::<Vec<_>>() );
    for child in owner.children.clone() {
            if child.node_type == "on" && child.name == event_name {
                executed_any = true;
                // Execute each action child in order
        for action in child.children {
            eprintln!("[ACTIONS] Action node: type='{}' name='{}' params={:?}", action.node_type, action.name, action.parameters);
                    Self::execute_action(nodes, &owner_indices, &node_path, &action)?;
                }
            }
        }

        if executed_any {
            // 3) Re-resolve document and recompute formulas once after all actions
            resolver::resolve_document(nodes);
        }
        Ok(())
    }

    fn execute_action(
        nodes: &mut Vec<OverseerNode>,
        owner_indices: &[usize],
        owner_path: &[String],
        action: &OverseerNode,
    ) -> Result<(), OverseerError> {
        debug_actions!("[ACTIONS] Executing action {} with params {:?}", action.node_type, action.parameters);
        match action.node_type.as_str() {
            "set" => {
                let target = Self::require_string(&action.parameters, "path")?;
                let value = Self::require_value(&action.parameters, "value")?;
                Self::set_value(nodes, owner_indices, owner_path, &target, value)
            }
            "inc" => {
                let target = Self::require_string(&action.parameters, "path")?;
                let by = match action.parameters.get("by") {
                    Some(OverseerValue::Integer(i)) => *i as f64,
                    Some(OverseerValue::Float(f)) => *f,
                    None => 1.0,
                    _ => return Err(OverseerError::ValidationError("inc.by must be number".to_string())),
                };
                Self::inc_value(nodes, owner_indices, owner_path, &target, by)
            }
            "dec" => {
                let target = Self::require_string(&action.parameters, "path")?;
                let by = match action.parameters.get("by") {
                    Some(OverseerValue::Integer(i)) => *i as f64,
                    Some(OverseerValue::Float(f)) => *f,
                    None => 1.0,
                    _ => return Err(OverseerError::ValidationError("dec.by must be number".to_string())),
                };
                Self::inc_value(nodes, owner_indices, owner_path, &target, -by)
            }
            "toggle" => {
                let target = Self::require_string(&action.parameters, "path")?;
                Self::toggle_value(nodes, owner_indices, owner_path, &target)
            }
            "clear" => {
                let target = Self::require_string(&action.parameters, "path")?;
                Self::clear_value(nodes, owner_indices, owner_path, &target)
            }
            "set_now" => {
                let target = Self::require_string(&action.parameters, "path")?;
                let clock = match action.parameters.get("clock") {
                    Some(OverseerValue::String(s)) => s.as_str(),
                    _ => "local",
                };
                let now = if clock == "utc" { Utc::now().date_naive() } else { Local::now().date_naive() };
                let val = OverseerValue::Date(now.to_string());
                Self::set_value(nodes, owner_indices, owner_path, &target, val)
            }
            // Increment 3: list identity and mutations (MVP subset)
            "ensure_in_list" => {
                let list_path = Self::require_string(&action.parameters, "list")?;
                let key_field = match action.parameters.get("keyField") {
                    Some(OverseerValue::String(s)) => s.clone(),
                    _ => "".to_string(),
                };
                let key_value = match action.parameters.get("keyValue") {
                    Some(OverseerValue::String(s)) => OverseerValue::String(s.clone()),
                    Some(OverseerValue::Integer(i)) => OverseerValue::Integer(*i),
                    Some(OverseerValue::Float(f)) => OverseerValue::Float(*f),
                    Some(OverseerValue::Boolean(b)) => OverseerValue::Boolean(*b),
                    Some(OverseerValue::Date(d)) => OverseerValue::Date(d.clone()),
                    _ => return Err(OverseerError::ValidationError("ensure_in_list.keyValue required".to_string())),
                };
                // Template to clone
                let template_name = match action.parameters.get("template") {
                    Some(OverseerValue::Template(t)) => {
                        let raw = t.split('/').last().unwrap_or("");
                        let trimmed = raw.trim();
                        if trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.len() >= 2 {
                            trimmed[1..trimmed.len()-1].to_string()
                        } else {
                            trimmed.to_string()
                        }
                    }
                    Some(OverseerValue::String(s)) => {
                        let raw = s.split('/').last().unwrap_or("");
                        let trimmed = raw.trim();
                        if trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.len() >= 2 {
                            trimmed[1..trimmed.len()-1].to_string()
                        } else {
                            trimmed.to_string()
                        }
                    }
                    _ => return Err(OverseerError::ValidationError("ensure_in_list.template required".to_string())),
                };
                Self::ensure_in_list(nodes, owner_path, &list_path, &template_name, &key_field, key_value)
            }
            "remove_from_list" | "remove" => {
                let list_path = match action.parameters.get("list").or(action.parameters.get("from")) {
                    Some(OverseerValue::String(s)) => s.clone(),
                    _ => return Err(OverseerError::ValidationError("remove.list (or from) required".to_string())),
                };
                let key_field = match action.parameters.get("keyField") {
                    Some(OverseerValue::String(s)) => s.clone(),
                    _ => "".to_string(),
                };
                let key_value = match action.parameters.get("keyValue") {
                    Some(OverseerValue::String(s)) => OverseerValue::String(s.clone()),
                    Some(OverseerValue::Integer(i)) => OverseerValue::Integer(*i),
                    Some(OverseerValue::Float(f)) => OverseerValue::Float(*f),
                    Some(OverseerValue::Boolean(b)) => OverseerValue::Boolean(*b),
                    Some(OverseerValue::Date(d)) => OverseerValue::Date(d.clone()),
                    _ => return Err(OverseerError::ValidationError("remove.keyValue required".to_string())),
                };
                Self::remove_from_list(nodes, owner_path, &list_path, &key_field, &key_value)
            }
            "append" => {
                // append(list=/path, template=<...>?) for template lists
                // or append(list=/path, value=...) for simple-type lists
                let list_path = Self::require_string(&action.parameters, "list").or_else(|_| Self::require_string(&action.parameters, "to"))?;
                // Optional template override
                let template_name_opt: Option<String> = match action.parameters.get("template") {
                    Some(OverseerValue::Template(t)) => {
                        let raw = t.split('/').last().unwrap_or("");
                        let trimmed = raw.trim();
                        if trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.len() >= 2 {
                            Some(trimmed[1..trimmed.len()-1].to_string())
                        } else { Some(trimmed.to_string()) }
                    }
                    Some(OverseerValue::String(s)) => {
                        let raw = s.split('/').last().unwrap_or("");
                        let trimmed = raw.trim();
                        if trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.len() >= 2 {
                            Some(trimmed[1..trimmed.len()-1].to_string())
                        } else { Some(trimmed.to_string()) }
                    }
                    _ => None,
                };
                let value_opt = action.parameters.get("value").cloned();
                Self::append_to_list(nodes, owner_path, &list_path, template_name_opt.as_deref(), value_opt)
            }
            "move" => {
                // move(from=/list, keyField=..., keyValue=..., to=/targetList?, at=index?)
                let from_path = Self::require_string(&action.parameters, "from")?;
                let to_path = match action.parameters.get("to") { Some(OverseerValue::String(s)) => s.clone(), _ => from_path.clone() };
                let key_field = match action.parameters.get("keyField") { Some(OverseerValue::String(s)) => s.clone(), _ => "".to_string() };
                let key_value = match action.parameters.get("keyValue") {
                    Some(OverseerValue::String(s)) => OverseerValue::String(s.clone()),
                    Some(OverseerValue::Integer(i)) => OverseerValue::Integer(*i),
                    Some(OverseerValue::Float(f)) => OverseerValue::Float(*f),
                    Some(OverseerValue::Boolean(b)) => OverseerValue::Boolean(*b),
                    Some(OverseerValue::Date(d)) => OverseerValue::Date(d.clone()),
                    _ => return Err(OverseerError::ValidationError("move.keyValue required".to_string())),
                };
                let at_index = match action.parameters.get("at") {
                    Some(OverseerValue::Integer(i)) => Some(*i as usize),
                    _ => None,
                };
                Self::move_in_list(nodes, owner_path, &from_path, &to_path, &key_field, &key_value, at_index)
            }
            _ => {
                // Unknown action: no-op for now
                debug_actions!("[ACTIONS] Unknown action '{}', skipping", action.node_type);
                Ok(())
            }
        }
    }

    fn require_string(map: &std::collections::HashMap<String, OverseerValue>, key: &str) -> Result<String, OverseerError> {
        match map.get(key) {
            Some(OverseerValue::String(s)) => Ok(s.clone()),
            Some(_) => Err(OverseerError::ValidationError(format!("Parameter '{}' must be string", key))),
            None => Err(OverseerError::ValidationError(format!("Missing parameter '{}'", key))),
        }
    }

    fn require_value(map: &std::collections::HashMap<String, OverseerValue>, key: &str) -> Result<OverseerValue, OverseerError> {
        match map.get(key) {
            Some(v) => Ok(v.clone()),
            None => Err(OverseerError::ValidationError(format!("Missing parameter '{}'", key))),
        }
    }

    fn get_effective<'a>(params: &'a std::collections::HashMap<String, OverseerValue>, key: &str) -> Option<&'a OverseerValue> {
        if key == "value" {
            if let Some(v) = params.get("_computed_value") { return Some(v); }
        } else {
            let shadow = format!("_computed_{}", key);
            if let Some(v) = params.get(&shadow) { return Some(v); }
        }
        params.get(key)
    }

    fn split_path_and_param(path: &str) -> (Vec<String>, Option<String>, bool) {
        // returns (segments, explicit_param, anchored)
        let anchored = path.starts_with('/') || path.starts_with("/ ");
        let mut p = path.trim();
        if p.starts_with('/') { p = &p[1..]; }
        let mut explicit_param: Option<String> = None;
        // support trailing .param on the last segment only, not for ".."
        let last_slash = p.rfind('/');
        let last_dot = p.rfind('.');
        if let Some(d) = last_dot {
            let is_after_slash = last_slash.map_or(true, |s| d > s);
            let is_not_parent = if d > 0 { &p[d-1..=d] != ".." } else { true };
            let has_rhs = d + 1 < p.len();
            if is_after_slash && is_not_parent && has_rhs {
                let (lhs, rhs) = p.split_at(d);
                explicit_param = Some(rhs[1..].to_string());
                p = lhs;
            }
        }
        let segments = p.split('/').map(|s| s.trim().to_string()).collect();
        (segments, explicit_param, anchored)
    }

    /// Resolve a mutable reference to a node given a base owner path and target path string.
    fn resolve_target_indices(
        nodes: &Vec<OverseerNode>,
        owner_path: &[String],
        anchored: bool,
        segments: &[String],
    ) -> Option<Vec<usize>> {
        // For anchored paths (starting with '/'): treat as absolute from root
        // For relative paths: use the owner_path as the base
        let bases: Vec<Vec<String>> = if anchored {
            vec![Vec::new()]
        } else {
            vec![owner_path.to_vec()]
        };

        for base in bases {
            // Build absolute name path by applying segments (support "..")
            let mut abs = base.clone();
            let mut valid = true;
            // Minimum depth allowed before processing ".."
            let min_len = if anchored { 0 } else { 1 };
            for seg in segments {
                if seg == ".." {
                    if abs.len() > min_len { abs.pop(); } else { valid = false; break; }
                } else if seg.is_empty() {
                    continue;
                } else {
                    abs.push(seg.clone());
                }
            }
            if !valid { continue; }
            eprintln!("[ACTIONS] resolve_target_indices: base={:?} segs={:?} => abs={:?}", base, segments, abs);
            if let Some(indices) = Self::find_indices_by_name_path(nodes, &abs) {
                return Some(indices);
            }
        }
        None
    }

    fn find_indices_by_name_path(nodes: &Vec<OverseerNode>, path: &[String]) -> Option<Vec<usize>> {
        if path.is_empty() { return None; }
        let mut indices: Vec<usize> = Vec::new();
        let mut cur_slice: &[OverseerNode] = nodes.as_slice();
        // root
        let mut pos = cur_slice.iter().position(|n| n.name == path[0])?;
        indices.push(pos);
        let mut cur: &OverseerNode = &cur_slice[pos];
        for name in &path[1..] {
            pos = cur.children.iter().position(|c| c.name == *name)?;
            indices.push(pos);
            cur = &cur.children[pos];
        }
        Some(indices)
    }

    fn get_node_mut_by_indices<'a>(nodes: &'a mut Vec<OverseerNode>, indices: &[usize]) -> Option<&'a mut OverseerNode> {
        if indices.is_empty() { return None; }
        let mut cur_ptr: *mut OverseerNode = &mut nodes[indices[0]] as *mut _;
        for (i, idx) in indices.iter().enumerate() {
            if i == 0 { continue; }
            unsafe {
                let cur_ref = &mut *cur_ptr;
                if *idx >= cur_ref.children.len() { return None; }
                cur_ptr = &mut cur_ref.children[*idx] as *mut _;
            }
        }
        unsafe { Some(&mut *cur_ptr) }
    }

    /// Return raw pointer to node and its indices path for reuse.
    fn get_node_mut_by_path(
        nodes: &mut Vec<OverseerNode>,
        path: &[String],
    ) -> Option<(*mut OverseerNode, Vec<usize>)> {
        if path.is_empty() { return None; }
        let mut indices: Vec<usize> = Vec::new();
        let mut cur: *mut OverseerNode = std::ptr::null_mut();
        // Find root index
        let root_idx = nodes.iter().position(|n| n.name == path[0])?;
        indices.push(root_idx);
        cur = &mut nodes[root_idx] as *mut _;
        for name in &path[1..] {
            unsafe {
                let cur_ref = &mut *cur;
                let pos = cur_ref.children.iter().position(|c| c.name == *name)?;
                indices.push(pos);
                cur = &mut cur_ref.children[pos] as *mut _;
            }
        }
        Some((cur, indices))
    }

    fn set_value(
        nodes: &mut Vec<OverseerNode>,
        _owner_indices: &[usize],
        owner_path: &[String],
        target: &str,
        value: OverseerValue,
    ) -> Result<(), OverseerError> {
        let (segments, explicit_param, anchored) = Self::split_path_and_param(target);
        let indices = match Self::resolve_target_indices(&nodes, owner_path, anchored, &segments) {
            Some(ix) => ix,
            None => {
                eprintln!("[ACTIONS] set: Target not found: {} (owner_path={:?}, anchored={}, segments={:?})", target, owner_path, anchored, segments);
                return Err(OverseerError::ValidationError(format!("Target not found: {}", target)));
            }
        };
        let node = Self::get_node_mut_by_indices(nodes, &indices)
            .ok_or_else(|| OverseerError::ValidationError(format!("Target not found: {}", target)))?;
        let key = explicit_param.unwrap_or_else(|| "value".to_string());
        node.parameters.insert(key, value);
        Ok(())
    }

    fn inc_value(
        nodes: &mut Vec<OverseerNode>,
        _owner_indices: &[usize],
        owner_path: &[String],
        target: &str,
        by: f64,
    ) -> Result<(), OverseerError> {
        let (segments, explicit_param, anchored) = Self::split_path_and_param(target);
        let indices = match Self::resolve_target_indices(&nodes, owner_path, anchored, &segments) {
            Some(ix) => ix,
            None => {
                eprintln!("[ACTIONS] inc: Target not found: {} (owner_path={:?}, anchored={}, segments={:?})", target, owner_path, anchored, segments);
                return Err(OverseerError::ValidationError(format!("Target not found: {}", target)));
            }
        };
        let node = Self::get_node_mut_by_indices(nodes, &indices)
            .ok_or_else(|| OverseerError::ValidationError(format!("Target not found: {}", target)))?;
        let key = explicit_param.unwrap_or_else(|| "value".to_string());
        let cur_val = Self::get_effective(&node.parameters, &key).or_else(|| node.parameters.get(&key))
            .ok_or_else(|| OverseerError::ValidationError(format!("No current value at {}.{}", target, key)))?;
        let new_val = match cur_val {
            OverseerValue::Integer(i) => {
                let res = *i as f64 + by;
                if res.fract() == 0.0 { OverseerValue::Integer(res as i64) } else { OverseerValue::Float(res) }
            }
            OverseerValue::Float(f) => OverseerValue::Float(*f + by),
            _ => return Err(OverseerError::ValidationError("inc target must be number".to_string())),
        };
        node.parameters.insert(key, new_val);
        Ok(())
    }

    fn toggle_value(
        nodes: &mut Vec<OverseerNode>,
        _owner_indices: &[usize],
        owner_path: &[String],
        target: &str,
    ) -> Result<(), OverseerError> {
        let (segments, explicit_param, anchored) = Self::split_path_and_param(target);
        let indices = match Self::resolve_target_indices(&nodes, owner_path, anchored, &segments) {
            Some(ix) => ix,
            None => {
                eprintln!("[ACTIONS] toggle: Target not found: {} (owner_path={:?}, anchored={}, segments={:?})", target, owner_path, anchored, segments);
                return Err(OverseerError::ValidationError(format!("Target not found: {}", target)));
            }
        };
        let node = Self::get_node_mut_by_indices(nodes, &indices)
            .ok_or_else(|| OverseerError::ValidationError(format!("Target not found: {}", target)))?;
        let key = explicit_param.unwrap_or_else(|| "value".to_string());
        let cur_val = Self::get_effective(&node.parameters, &key).or_else(|| node.parameters.get(&key))
            .ok_or_else(|| OverseerError::ValidationError(format!("No current value at {}.{}", target, key)))?;
        let new_val = match cur_val {
            OverseerValue::Boolean(b) => OverseerValue::Boolean(!b),
            _ => return Err(OverseerError::ValidationError("toggle target must be boolean".to_string())),
        };
        node.parameters.insert(key, new_val);
        Ok(())
    }

    fn clear_value(
        nodes: &mut Vec<OverseerNode>,
        _owner_indices: &[usize],
        owner_path: &[String],
        target: &str,
    ) -> Result<(), OverseerError> {
        let (segments, explicit_param, anchored) = Self::split_path_and_param(target);
        let indices = match Self::resolve_target_indices(&nodes, owner_path, anchored, &segments) {
            Some(ix) => ix,
            None => {
                eprintln!("[ACTIONS] clear: Target not found: {} (owner_path={:?}, anchored={}, segments={:?})", target, owner_path, anchored, segments);
                return Err(OverseerError::ValidationError(format!("Target not found: {}", target)));
            }
        };
        let node = Self::get_node_mut_by_indices(nodes, &indices)
            .ok_or_else(|| OverseerError::ValidationError(format!("Target not found: {}", target)))?;
        let key = explicit_param.unwrap_or_else(|| "value".to_string());
        node.parameters.remove(&key);
        Ok(())
    }

    // Find any node by name recursively
    fn find_node_by_name<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
        for n in nodes {
            if n.name == name { return Some(n); }
            if let Some(found) = Self::find_node_by_name(&n.children, name) { return Some(found); }
        }
        None
    }

    // Create a list item by cloning template definition (shallow clone children and params)
    fn clone_from_template(template: &OverseerNode) -> OverseerNode {
        OverseerNode {
            name: template.name.clone(),
            node_type: template.name.clone(),
            template: None,
            parameters: template.parameters.clone(),
            children: template.children.clone(),
            is_hierarchy_transparent: template.is_hierarchy_transparent,
        }
    }

    fn set_field_value_on_item(item: &mut OverseerNode, field: &str, value: OverseerValue) {
        if let Some(child) = item.children.iter_mut().find(|c| c.name == field) {
            child.parameters.insert("value".to_string(), value);
        } else {
            // Add simple string field if missing
            let mut new_field = OverseerNode {
                name: field.to_string(),
                node_type: "string".to_string(),
                template: None,
                parameters: {
                    let mut m = std::collections::HashMap::new();
                    m.insert("value".to_string(), value);
                    m
                },
                children: Vec::new(),
                is_hierarchy_transparent: false,
            };
            item.children.push(new_field);
        }
    }

    fn value_equals(a: &OverseerValue, b: &OverseerValue) -> bool {
        a == b
    }

    fn get_field_value<'a>(item: &'a OverseerNode, field: &'a str) -> Option<&'a OverseerValue> {
        item.children.iter().find(|c| c.name == field).and_then(|c| c.parameters.get("value"))
    }

    fn ensure_in_list(
        nodes: &mut Vec<OverseerNode>,
        owner_path: &[String],
        list_path: &str,
        template_name: &str,
        key_field: &str,
        key_value: OverseerValue,
    ) -> Result<(), OverseerError> {
    let (segments, _explicit_param, anchored) = Self::split_path_and_param(list_path);
    // Clone nodes snapshot for immutable searches to avoid aliasing
    let snapshot = nodes.clone();
    let indices = Self::resolve_target_indices(&snapshot, owner_path, anchored, &segments)
            .ok_or_else(|| OverseerError::ValidationError(format!("List not found: {}", list_path)))?;
    let list_node = Self::get_node_mut_by_indices(nodes, &indices)
            .ok_or_else(|| OverseerError::ValidationError(format!("List not found: {}", list_path)))?;
        if list_node.node_type != "list" { return Err(OverseerError::ValidationError("ensure_in_list.target is not a list".to_string())); }

        // Derive key field from list parameters if not provided
        let effective_key_field = if !key_field.is_empty() {
            key_field.to_string()
        } else if let Some(OverseerValue::String(s)) = list_node.parameters.get("key") {
            s.clone()
        } else { return Err(OverseerError::ValidationError("ensure_in_list.keyField missing and list has no key".to_string())); };

        // If exists, do nothing
        if list_node.children.iter().any(|it| Self::get_field_value(it, &effective_key_field).map_or(false, |v| Self::value_equals(v, &key_value))) {
            return Ok(());
        }

        // Find template by name
    let template_def = Self::find_node_by_name(&snapshot, template_name)
            .ok_or_else(|| OverseerError::ValidationError(format!("Template not found: {}", template_name)))?;
        let mut new_item = Self::clone_from_template(template_def);
        Self::set_field_value_on_item(&mut new_item, &effective_key_field, key_value);
        // Apply layout opposite to parent list's effective layout (to match default alternation rule)
        if let Some(OverseerValue::String(parent_eff)) = list_node.parameters.get("_effective_layout") {
            let opp = Self::opposite_layout(parent_eff);
            new_item.parameters.insert("_effective_layout".to_string(), OverseerValue::String(opp));
        }
        list_node.children.push(new_item);
        Ok(())
    }

    fn remove_from_list(
        nodes: &mut Vec<OverseerNode>,
        owner_path: &[String],
        list_path: &str,
        key_field: &str,
        key_value: &OverseerValue,
    ) -> Result<(), OverseerError> {
        let (segments, _explicit_param, anchored) = Self::split_path_and_param(list_path);
        let indices = Self::resolve_target_indices(&nodes, owner_path, anchored, &segments)
            .ok_or_else(|| OverseerError::ValidationError(format!("List not found: {}", list_path)))?;
        let list_node = Self::get_node_mut_by_indices(nodes, &indices)
            .ok_or_else(|| OverseerError::ValidationError(format!("List not found: {}", list_path)))?;
        if list_node.node_type != "list" { return Err(OverseerError::ValidationError("remove.target is not a list".to_string())); }

        // Determine key field
        let effective_key_field = if !key_field.is_empty() {
            key_field.to_string()
        } else if let Some(OverseerValue::String(s)) = list_node.parameters.get("key") {
            s.clone()
        } else { return Err(OverseerError::ValidationError("remove.keyField missing and list has no key".to_string())); };

        if let Some(pos) = list_node.children.iter().position(|it| Self::get_field_value(it, &effective_key_field).map_or(false, |v| Self::value_equals(v, key_value))) {
            list_node.children.remove(pos);
        }
        Ok(())
    }

    fn append_to_list(
        nodes: &mut Vec<OverseerNode>,
        owner_path: &[String],
        list_path: &str,
        template_name: Option<&str>,
        value_opt: Option<OverseerValue>,
    ) -> Result<(), OverseerError> {
        let (segments, _explicit_param, anchored) = Self::split_path_and_param(list_path);
        let snapshot = nodes.clone();
        let indices = Self::resolve_target_indices(&snapshot, owner_path, anchored, &segments)
            .ok_or_else(|| OverseerError::ValidationError(format!("List not found: {}", list_path)))?;
        let list_node = Self::get_node_mut_by_indices(nodes, &indices)
            .ok_or_else(|| OverseerError::ValidationError(format!("List not found: {}", list_path)))?;
        if list_node.node_type != "list" { return Err(OverseerError::ValidationError("append.target is not a list".to_string())); }

        // Determine entry type
        if let Some(entry) = list_node.parameters.get("entry") {
            match entry {
                OverseerValue::Template(t) => {
                    // Choose template: explicit override or list entry template
                    let chosen_template_name = if let Some(name) = template_name { name.to_string() } else {
                        let raw = t.split('/').last().unwrap_or("");
                        let trimmed = raw.trim();
                        if trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.len() >= 2 {
                            trimmed[1..trimmed.len()-1].to_string()
                        } else { trimmed.to_string() }
                    };
                    let template_def = Self::find_node_by_name(&snapshot, &chosen_template_name)
                        .ok_or_else(|| OverseerError::ValidationError(format!("Template not found: {}", chosen_template_name)))?;
                    let mut new_item = Self::clone_from_template(template_def);
                    // New item should adopt opposite of parent list effective layout
                    if let Some(OverseerValue::String(parent_eff)) = list_node.parameters.get("_effective_layout") {
                        let opp = Self::opposite_layout(parent_eff);
                        new_item.parameters.insert("_effective_layout".to_string(), OverseerValue::String(opp));
                    }
                    list_node.children.push(new_item);
                }
                OverseerValue::String(type_name) => {
                    // Simple type list requires a value
                    let val = value_opt.ok_or_else(|| OverseerError::ValidationError("append.value required for simple list".to_string()))?;
                    let mut item = OverseerNode {
                        name: "".to_string(),
                        node_type: type_name.clone(),
                        template: None,
                        parameters: Default::default(),
                        children: Vec::new(),
                        is_hierarchy_transparent: false,
                    };
                    item.parameters.insert("value".to_string(), val);
                    list_node.children.push(item);
                }
                _ => return Err(OverseerError::ValidationError("append: unsupported entry type".to_string())),
            }
        } else {
            return Err(OverseerError::ValidationError("append: list has no entry parameter".to_string()));
        }
        Ok(())
    }

    fn move_in_list(
        nodes: &mut Vec<OverseerNode>,
        owner_path: &[String],
        from_path: &str,
        to_path: &str,
        key_field: &str,
        key_value: &OverseerValue,
        at_index: Option<usize>,
    ) -> Result<(), OverseerError> {
        let (from_segments, _p1, from_anchored) = Self::split_path_and_param(from_path);
        let (to_segments, _p2, to_anchored) = Self::split_path_and_param(to_path);
        let snapshot = nodes.clone();
        let from_indices = Self::resolve_target_indices(&snapshot, owner_path, from_anchored, &from_segments)
            .ok_or_else(|| OverseerError::ValidationError(format!("From list not found: {}", from_path)))?;
        let to_indices = Self::resolve_target_indices(&snapshot, owner_path, to_anchored, &to_segments)
            .ok_or_else(|| OverseerError::ValidationError(format!("To list not found: {}", to_path)))?;
        let same_list = from_indices == to_indices;

        // Borrow lists mutably via indices carefully
        if same_list {
            let list_node = Self::get_node_mut_by_indices(nodes, &from_indices)
                .ok_or_else(|| OverseerError::ValidationError(format!("List not found: {}", from_path)))?;
            if list_node.node_type != "list" { return Err(OverseerError::ValidationError("move.target is not a list".to_string())); }
            let effective_key_field = if !key_field.is_empty() {
                key_field.to_string()
            } else if let Some(OverseerValue::String(s)) = list_node.parameters.get("key") { s.clone() } else {
                return Err(OverseerError::ValidationError("move.keyField missing and list has no key".to_string()));
            };
            if let Some(pos) = list_node.children.iter().position(|it| Self::get_field_value(it, &effective_key_field).map_or(false, |v| Self::value_equals(v, key_value))) {
                let item = list_node.children.remove(pos);
                let insert_at = at_index.unwrap_or(list_node.children.len());
                let idx = if insert_at > list_node.children.len() { list_node.children.len() } else { insert_at };
                list_node.children.insert(idx, item);
            }
        } else {
            // Different lists: remove from source, push/insert into dest
            let item_opt = {
                let from_node = Self::get_node_mut_by_indices(nodes, &from_indices)
                    .ok_or_else(|| OverseerError::ValidationError(format!("From list not found: {}", from_path)))?;
                if from_node.node_type != "list" { return Err(OverseerError::ValidationError("move.from is not a list".to_string())); }
                let effective_key_field = if !key_field.is_empty() { key_field.to_string() } else if let Some(OverseerValue::String(s)) = from_node.parameters.get("key") { s.clone() } else { return Err(OverseerError::ValidationError("move.keyField missing and list has no key".to_string())); };
                if let Some(pos) = from_node.children.iter().position(|it| Self::get_field_value(it, &effective_key_field).map_or(false, |v| Self::value_equals(v, key_value))) {
                    Some(from_node.children.remove(pos))
                } else { None }
            };
            if let Some(item) = item_opt {
                let to_node = Self::get_node_mut_by_indices(nodes, &to_indices)
                    .ok_or_else(|| OverseerError::ValidationError(format!("To list not found: {}", to_path)))?;
                if to_node.node_type != "list" { return Err(OverseerError::ValidationError("move.to is not a list".to_string())); }
                let insert_at = at_index.unwrap_or(to_node.children.len());
                let idx = if insert_at > to_node.children.len() { to_node.children.len() } else { insert_at };
                to_node.children.insert(idx, item);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_document;

    #[test]
    fn test_inc_action_on_click() {
        let input = r#"
        div Root {
            int counter = 1
            button Increment {
                on click { inc(path="../counter", by=2) }
            }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolver::resolve_document(&mut nodes);

        let path = vec!["Root".to_string(), "Increment".to_string()];

        let res = ActionExecutor::execute_event(&mut nodes, &path, "click");
        assert!(res.is_ok());

        // Verify counter incremented to 3
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let counter = root.children.iter().find(|c| c.name == "counter").unwrap();
        assert_eq!(counter.parameters.get("value"), Some(&OverseerValue::Integer(3)));
    }

    #[test]
    fn test_toggle_action() {
        let input = r#"
        div Root {
            bool done = false
            button Toggle {
                on click { toggle(path="../done") }
            }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolver::resolve_document(&mut nodes);
        let path = vec!["Root".to_string(), "Toggle".to_string()];
        let res = ActionExecutor::execute_event(&mut nodes, &path, "click");
        assert!(res.is_ok());
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let done = root.children.iter().find(|c| c.name == "done").unwrap();
        assert_eq!(done.parameters.get("value"), Some(&OverseerValue::Boolean(true)));
    }

    #[test]
    fn test_ensure_in_list_creates_item() {
        let input = r#"
        div Root {
            div Task {
                string id = ""
                string title = ""
            }
            list Tasks (entry=<Task>, key="id") {
            }
            button Add {
                on click { ensure_in_list(list="/Root/Tasks", keyField="id", keyValue="a1", template="<Task>") }
            }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolver::resolve_document(&mut nodes);
        let path = vec!["Root".to_string(), "Add".to_string()];
        let res = ActionExecutor::execute_event(&mut nodes, &path, "click");
        assert!(res.is_ok());
        // Verify a child exists with id == "a1"
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let list = root.children.iter().find(|c| c.name == "Tasks").unwrap();
        assert_eq!(list.node_type, "list");
        assert!(list.children.iter().any(|it| it.children.iter().any(|f| f.name=="id" && f.parameters.get("value")==Some(&OverseerValue::String("a1".to_string())))));
    }

    #[test]
    fn test_remove_from_list_by_key() {
        let input = r#"
        div Root {
            div Task {
                string id = ""
                string title = ""
            }
            list Tasks (entry=<Task>, key="id") {
                - Task { id = "x1" title = "Old" }
            }
            button Remove {
                on click { remove(list="/Root/Tasks", keyField="id", keyValue="x1") }
            }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolver::resolve_document(&mut nodes);
        let path = vec!["Root".to_string(), "Remove".to_string()];
        let res = ActionExecutor::execute_event(&mut nodes, &path, "click");
        assert!(res.is_ok());
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let list = root.children.iter().find(|c| c.name == "Tasks").unwrap();
        assert!(list.children.iter().all(|it| it.children.iter().all(|f| !(f.name=="id" && f.parameters.get("value")==Some(&OverseerValue::String("x1".to_string()))))));
    }

    #[test]
    fn test_append_to_template_list() {
        let input = r#"
        div Root {
            div Task { string id = "" string title = "" }
            list Tasks (entry=<Task>, key="id") { }
            button Add { on click { append(list="/Root/Tasks", template="<Task>") } }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolver::resolve_document(&mut nodes);
        let path = vec!["Root".to_string(), "Add".to_string()];
        let res = ActionExecutor::execute_event(&mut nodes, &path, "click");
        assert!(res.is_ok());
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let list = root.children.iter().find(|c| c.name == "Tasks").unwrap();
        assert_eq!(list.node_type, "list");
        assert_eq!(list.children.len(), 1);
        assert_eq!(list.children[0].node_type, "Task");
    }

    #[test]
    fn test_append_to_simple_list() {
        let input = r#"
        div Root {
            list Names (entry=string) { }
            button Add { on click { append(list="/Root/Names", value="Alice") } }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolver::resolve_document(&mut nodes);
        let path = vec!["Root".to_string(), "Add".to_string()];
        let res = ActionExecutor::execute_event(&mut nodes, &path, "click");
        assert!(res.is_ok());
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let list = root.children.iter().find(|c| c.name == "Names").unwrap();
        assert_eq!(list.node_type, "list");
        assert_eq!(list.children.len(), 1);
        assert_eq!(list.children[0].node_type, "string");
        assert_eq!(list.children[0].parameters.get("value"), Some(&OverseerValue::String("Alice".to_string())));
    }

    #[test]
    fn test_move_within_list_by_key() {
        let input = r#"
        div Root {
            div Task { string id = "" }
            list Tasks (entry=<Task>, key="id") {
                - Task { id = "a" }
                - Task { id = "b" }
                - Task { id = "c" }
            }
            button Move { on click { move(from="/Root/Tasks", keyField="id", keyValue="b", at=0) } }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolver::resolve_document(&mut nodes);
        let path = vec!["Root".to_string(), "Move".to_string()];
        let res = ActionExecutor::execute_event(&mut nodes, &path, "click");
        assert!(res.is_ok());
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let list = root.children.iter().find(|c| c.name == "Tasks").unwrap();
        let ids: Vec<String> = list.children.iter().map(|it| {
            it.children.iter().find(|f| f.name=="id").and_then(|f| f.parameters.get("value")).and_then(|v| if let OverseerValue::String(s)=v { Some(s.clone()) } else { None }).unwrap_or_default()
        }).collect();
        assert_eq!(ids, vec!["b".to_string(), "a".to_string(), "c".to_string()]);
    }
}
