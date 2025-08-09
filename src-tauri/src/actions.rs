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
        // Build candidate base paths for anchored search; else use owner_path as base
        let mut bases: Vec<Vec<String>> = Vec::new();
        if anchored {
            for end in (1..=owner_path.len()).rev() {
                bases.push(owner_path[..end].to_vec());
            }
        } else {
            bases.push(owner_path.to_vec());
        }
        for base in bases {
            // Build absolute name path by applying segments (support "..")
            let mut abs = base.clone();
            let mut valid = true;
            for seg in segments {
                if seg == ".." {
                    if abs.len() > 1 { abs.pop(); } else { valid = false; break; }
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
}
