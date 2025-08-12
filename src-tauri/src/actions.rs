use crate::resolver;
use crate::formula_evaluator::{FormulaEvaluator, EvaluationContext, BoundValue};
use crate::types::{OverseerError, OverseerNode, OverseerValue};
use chrono::{Local, Utc, Duration, NaiveDateTime, NaiveDate};

// Debug logging macro for actions
macro_rules! debug_actions {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-actions")]
        println!($($arg)*);
    };
}

pub struct ActionExecutor;

impl ActionExecutor {
    /// Parse a variety of timestamp string forms into a UTC DateTime
    fn parse_timestamp_utc(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
        // Prefer RFC3339 first
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) { return Some(dt.with_timezone(&chrono::Utc)); }
        // Fallback: "YYYY-MM-DD HH:MM:SS" (assume UTC)
        if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
            return Some(chrono::DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc));
        }
        // Fallback: "YYYY-MM-DDTHH:MM:SS" (no zone, assume UTC)
        if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
            return Some(chrono::DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc));
        }
        // Fallback: date only -> start of day UTC
        if let Ok(nd) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
            let ndt = nd.and_hms_opt(0,0,0)?;
            return Some(chrono::DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc));
        }
        None
    }

    /// Parse an offset value to chrono::Duration. Supported:
    /// - Integer/Float => seconds
    /// - String with suffix: "ms", "s", "m", "h", "d" (e.g., "1500ms", "10s", "5m", "2h", "1d")
    fn parse_offset_duration(val: &OverseerValue) -> Option<Duration> {
        match val {
            OverseerValue::Integer(i) => Some(Duration::seconds(*i)),
            OverseerValue::Float(f) => Some(Duration::seconds(*f as i64)),
            OverseerValue::String(s) => {
                let txt = s.trim().to_lowercase();
                if txt.ends_with("ms") {
                    let num = txt.trim_end_matches("ms").trim().parse::<i64>().ok()?;
                    Some(Duration::milliseconds(num))
                } else if txt.ends_with('s') {
                    let num = txt.trim_end_matches('s').trim().parse::<i64>().ok()?;
                    Some(Duration::seconds(num))
                } else if txt.ends_with('m') {
                    let num = txt.trim_end_matches('m').trim().parse::<i64>().ok()?;
                    Some(Duration::minutes(num))
                } else if txt.ends_with('h') {
                    let num = txt.trim_end_matches('h').trim().parse::<i64>().ok()?;
                    Some(Duration::hours(num))
                } else if txt.ends_with('d') {
                    let num = txt.trim_end_matches('d').trim().parse::<i64>().ok()?;
                    Some(Duration::days(num))
                } else if let Ok(num) = txt.parse::<i64>() {
                    Some(Duration::seconds(num))
                } else { None }
            }
            _ => None,
        }
    }
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
                #[cfg(feature = "debug-actions")] eprintln!("[ACTIONS] Owner node not found at path {:?}", node_path);
                return Err(OverseerError::ValidationError(format!(
                    "Owner node not found at path {:?}",
                    node_path
                )));
            }
        };

        // SAFETY: we use raw pointer to allow nested borrows during traversal of action children
        let owner: &mut OverseerNode = unsafe { &mut *owner_ptr };

        // 2) Find matching on block(s)
    #[cfg(feature = "debug-actions")] eprintln!("[ACTIONS] Owner: {} (type={}) children: {:?}", owner.name, owner.node_type, owner.children.iter().map(|c| format!("{}/{}", c.node_type, c.name)).collect::<Vec<_>>() );
    for child in owner.children.clone() {
            if child.node_type == "on" && child.name == event_name {
                // Execute each action child in order
                for action in child.children {
                    #[cfg(feature = "debug-actions")] eprintln!("[ACTIONS] Action node: type='{}' name='{}' params={:?}", action.node_type, action.name, action.parameters);
                    Self::execute_action(nodes, &owner_indices, &node_path, &action)?;
                    // Re-resolve after each action (per-action transaction)
                    resolver::resolve_document(nodes);
                }
            }
        }

        // Note: do not run timers here; scheduling handles timer firing.
        Ok(())
    }

    /// Scan the document for timer nodes and fire any whose condition is met.
    /// Semantics: A timer node is any node with original_type 'timer' or name 'timer' containing
    /// parameters: active=true and at=<timestamp or formula producing Timestamp/Date>.
    /// On fire: execute its on timeout { ... } block, then set active=false (one-shot) and re-resolve once.
    fn run_timers(nodes: &mut Vec<OverseerNode>) -> Result<(), OverseerError> {
    // Snapshot of now (UTC)
    let now_dt = chrono::Utc::now();
        // Collect paths to timers to avoid borrow issues
        let mut timer_paths: Vec<Vec<String>> = Vec::new();
        fn collect(paths: &mut Vec<Vec<String>>, cur: &OverseerNode, path: &mut Vec<String>) {
            path.push(cur.name.clone());
            // Identify timer by node_type or original type param
            let is_timer = cur.node_type == "timer"
                || cur
                    .parameters
                    .get("_original_type")
                    .map(|v| matches!(v, OverseerValue::String(s) if s == "timer"))
                    .unwrap_or(false);
            if is_timer {
                paths.push(path.clone());
            }
            for child in &cur.children { collect(paths, child, path); }
            path.pop();
        }
        for root in nodes.iter() {
            let mut p: Vec<String> = Vec::new();
            collect(&mut timer_paths, root, &mut p);
        }

    let mut any_fired = false;
    // Use snapshot for safe evaluation contexts
    let snapshot = nodes.clone();
        // Evaluate and fire timers
        for tpath in timer_paths {
            // Resolve node by path
            if let Some((ptr, _idx)) = Self::get_node_mut_by_path(nodes, &tpath) {
                let timer_node: &mut OverseerNode = unsafe { &mut *ptr };
                // Check active
                let active = match Self::get_effective(&timer_node.parameters, "active").or_else(|| timer_node.parameters.get("active")) {
                    Some(OverseerValue::Boolean(b)) => *b,
                    Some(OverseerValue::String(s)) => s.eq_ignore_ascii_case("true"),
                    _ => false,
                };
                if !active { continue; }
                // Evaluate 'at' (base timestamp)
                let at_val = Self::get_effective(&timer_node.parameters, "at").or_else(|| timer_node.parameters.get("at"));
                let at_str: Option<String> = match at_val {
                    Some(OverseerValue::Timestamp(ts)) => Some(ts.clone()),
                    Some(OverseerValue::Date(d)) => Some(format!("{}T00:00:00Z", d)),
                    Some(OverseerValue::String(s)) => Some(s.clone()),
                    Some(OverseerValue::Formula(expr)) => {
                        // Evaluate formula in context of this timer
                        // Build a minimal context using document snapshot and path
                        let ctx = EvaluationContext::new(tpath.clone(), &snapshot);
                        match FormulaEvaluator::evaluate_formula(expr, &ctx) {
                            Ok(OverseerValue::Timestamp(ts)) => Some(ts),
                            Ok(OverseerValue::Date(d)) => Some(format!("{}T00:00:00Z", d)),
                            Ok(OverseerValue::String(s)) => Some(s),
                            _ => None,
                        }
                    }
                    _ => None,
                };
                if let Some(at) = at_str {
                    // Compute due instant = at + offset (if any)
                    let base = Self::parse_timestamp_utc(&at);
                    // offset may come from computed/raw 'offset' param
                    let off_val = Self::get_effective(&timer_node.parameters, "offset").or_else(|| timer_node.parameters.get("offset"));
                    let offset = off_val.and_then(|v| Self::parse_offset_duration(v));
                    let due = match base {
                        Some(b) => {
                            let inst = if let Some(off) = offset { b + off } else { b };
                            inst <= now_dt
                        }
                        None => false,
                    };
                    if due {
                        // Fire: execute its on timeout { ... } actions
                        let actions: Vec<OverseerNode> = timer_node
                            .children
                            .iter()
                            .filter(|c| c.node_type == "on" && c.name == "timeout")
                            .flat_map(|on| on.children.clone())
                            .collect();
                        if !actions.is_empty() {
                            for action in actions {
                                Self::execute_action(nodes, &[], &tpath, &action)?;
                            }
                            // Deactivate timer
                            timer_node.parameters.insert("active".to_string(), OverseerValue::Boolean(false));
                            any_fired = true;
                        }
                    }
                }
            }
        }
        if any_fired {
            resolver::resolve_document(nodes);
        }
        Ok(())
    }

    /// Public tick entry: run timers sweep once. Returns Ok when done.
    pub fn tick(nodes: &mut Vec<OverseerNode>) -> Result<(), OverseerError> {
        Self::run_timers(nodes)
    }

    /// Compute the next due timestamp (UTC, ms since epoch) across all active timers, if any.
    pub fn next_due_ms(nodes: &Vec<OverseerNode>) -> Option<i64> {
        let now = chrono::Utc::now();
        // Snapshot for evaluation context
        let snapshot = nodes.clone();
        // Collect timer paths
        let mut timer_paths: Vec<Vec<String>> = Vec::new();
        fn collect(paths: &mut Vec<Vec<String>>, cur: &OverseerNode, path: &mut Vec<String>) {
            path.push(cur.name.clone());
            let is_timer = cur.node_type == "timer"
                || cur
                    .parameters
                    .get("_original_type")
                    .map(|v| matches!(v, OverseerValue::String(s) if s == "timer"))
                    .unwrap_or(false);
            if is_timer { paths.push(path.clone()); }
            for child in &cur.children { collect(paths, child, path); }
            path.pop();
        }
        for root in nodes.iter() {
            let mut p: Vec<String> = Vec::new();
            collect(&mut timer_paths, root, &mut p);
        }
        let mut next_due: Option<i64> = None;
        for tpath in timer_paths {
            // Find timer node in snapshot for safe read/eval
            // Walk by name path
            let mut cur_opt: Option<&OverseerNode> = None;
            let mut _idx = 0usize;
            for root in snapshot.iter() {
                if root.name == tpath[0] { cur_opt = Some(root); break; }
            }
            if cur_opt.is_none() { continue; }
            let mut cur = cur_opt.unwrap();
            for seg in tpath.iter().skip(1) {
                if let Some(next) = cur.children.iter().find(|c| &c.name == seg) { cur = next; } else { break; }
                _idx += 1;
            }
            let timer_node = cur;
            // Active?
            let active = match Self::get_effective(&timer_node.parameters, "active").or_else(|| timer_node.parameters.get("active")) {
                Some(OverseerValue::Boolean(b)) => *b,
                Some(OverseerValue::String(s)) => s.eq_ignore_ascii_case("true"),
                _ => false,
            };
            if !active { continue; }
            // Evaluate 'at' using same logic as run_timers
            let at_val = Self::get_effective(&timer_node.parameters, "at").or_else(|| timer_node.parameters.get("at"));
            let at_str: Option<String> = match at_val {
                Some(OverseerValue::Timestamp(ts)) => Some(ts.clone()),
                Some(OverseerValue::Date(d)) => Some(format!("{}T00:00:00Z", d)),
                Some(OverseerValue::String(s)) => Some(s.clone()),
                Some(OverseerValue::Formula(expr)) => {
                    let ctx = EvaluationContext::new(tpath.clone(), &snapshot);
                    match FormulaEvaluator::evaluate_formula(expr, &ctx) {
                        Ok(OverseerValue::Timestamp(ts)) => Some(ts),
                        Ok(OverseerValue::Date(d)) => Some(format!("{}T00:00:00Z", d)),
                        Ok(OverseerValue::String(s)) => Some(s),
                        _ => None,
                    }
                }
                _ => None,
            };
            if let Some(at) = at_str {
                if let Some(base) = Self::parse_timestamp_utc(&at) {
                    // include offset when computing due time
                    let off_val = Self::get_effective(&timer_node.parameters, "offset").or_else(|| timer_node.parameters.get("offset"));
                    let offset = off_val.and_then(|v| Self::parse_offset_duration(v));
                    let utc = if let Some(off) = offset { base + off } else { base };
                    if utc > now {
                        let ms = utc.timestamp_millis();
                        next_due = match next_due { Some(prev) => Some(prev.min(ms)), None => Some(ms) };
                    }
                }
            }
        }
        next_due
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
                // Optional mode: "value" (default) evaluates and writes the result; "formula" copies raw formula
                let mode = match action.parameters.get("mode") {
                    Some(OverseerValue::String(s)) => s.to_lowercase(),
                    _ => "value".to_string(),
                };
                let value = if mode == "formula" {
                    // Copy raw value exactly as provided
                    Self::require_value(&action.parameters, "value")?
                } else {
                    // Always evaluate a Formula now in the owner's context to avoid stale _computed_value
                    if let Some(OverseerValue::Formula(expr)) = action.parameters.get("value") {
                        let snapshot = nodes.clone();
                        let ctx = EvaluationContext::new(owner_path.to_vec(), &snapshot);
                        FormulaEvaluator::evaluate_formula(expr, &ctx)?
                    } else if let Some(v) = action.parameters.get("value") {
                        v.clone()
                    } else if let Some(v) = Self::get_effective(&action.parameters, "value") {
                        // Fallback to any precomputed value if present
                        v.clone()
                    } else {
                        return Err(OverseerError::ValidationError("Missing parameter 'value'".to_string()));
                    }
                };
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
            "set_now_ts" => {
                // set_now_ts(path=..., offset=seconds?) -> sets RFC3339 Timestamp
                // path can be either a node field (sets its value) or a specific parameter via trailing .param (e.g., ../after_10s.at)
                let target = Self::require_string(&action.parameters, "path")?;
                let offset_secs: i64 = match action.parameters.get("offset").or(action.parameters.get("offsetSeconds")) {
                    Some(OverseerValue::Integer(i)) => *i,
                    Some(OverseerValue::Float(f)) => *f as i64,
                    Some(OverseerValue::String(s)) => s.parse::<i64>().unwrap_or(0),
                    _ => 0,
                };
                let ts = (Utc::now() + Duration::seconds(offset_secs)).to_rfc3339();
                let val = OverseerValue::Timestamp(ts);
                // If target specifies a parameter explicitly (../node.param), set that parameter; else set 'value'
                let (_segments, explicit_param, _anchored) = Self::split_path_and_param(&target);
                if explicit_param.is_some() {
                    Self::set_value(nodes, owner_indices, owner_path, &target, val)
                } else {
                    // force write to value by ensuring no explicit param
                    Self::set_value(nodes, owner_indices, owner_path, &target, val)
                }
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
                // append(list=/path, template=<...>?){ overrides... } for template lists
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
                // Pass action block overrides to append semantics
                let overrides = action.children.clone();
                Self::append_to_list(nodes, owner_path, &list_path, template_name_opt.as_deref(), value_opt, &overrides)
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
            "sort" => {
                // sort(list=/path, by=$(...), order=asc|desc, stable=true)
                let list_path = Self::require_string(&action.parameters, "list")?;
                let mut by_expr = match action.parameters.get("by") {
                    Some(OverseerValue::Formula(s)) => s.clone(),
                    Some(OverseerValue::String(s)) => s.clone(),
                    _ => return Err(OverseerError::ValidationError("sort.by must be a formula string".to_string())),
                };
                // Allow passing with $(...) wrapper; strip it if present
                let trimmed = by_expr.trim();
                if trimmed.starts_with("$(") && trimmed.ends_with(')') {
                    let inner = &trimmed[2..trimmed.len()-1];
                    by_expr = inner.trim().to_string();
                }
                let order = match action.parameters.get("order") {
                    Some(OverseerValue::String(s)) => s.to_lowercase(),
                    _ => "asc".to_string(),
                };
                let stable = match action.parameters.get("stable") {
                    Some(OverseerValue::Boolean(b)) => *b,
                    _ => true,
                };
                Self::sort_list(nodes, owner_path, &list_path, &by_expr, &order, stable)
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
            #[cfg(feature = "debug-actions")] eprintln!("[ACTIONS] resolve_target_indices: base={:?} segs={:?} => abs={:?}", base, segments, abs);
            if let Some(indices) = Self::find_indices_by_name_path(nodes, &abs) {
                return Some(indices);
            }
        }
        None
    }

    fn find_indices_by_name_path(nodes: &Vec<OverseerNode>, path: &[String]) -> Option<Vec<usize>> {
        if path.is_empty() { return None; }
        // Helper: DFS to find a descendant by name through transparent nodes, returning index chain from 'cur'
        fn find_child_chain(cur: &OverseerNode, target: &str) -> Option<Vec<usize>> {
            for (i, ch) in cur.children.iter().enumerate() {
                if ch.name == target {
                    return Some(vec![i]);
                }
                if ch.is_hierarchy_transparent {
                    if let Some(mut sub) = find_child_chain(ch, target) {
                        let mut out = vec![i];
                        out.append(&mut sub);
                        return Some(out);
                    }
                }
            }
            None
        }
        // Helper: search from roots for first segment, allowing transparent wrappers
        fn find_root_chain(nodes: &Vec<OverseerNode>, target: &str) -> Option<Vec<usize>> {
            for (i, n) in nodes.iter().enumerate() {
                if n.name == target { return Some(vec![i]); }
                if let Some(mut sub) = find_child_chain(n, target) {
                    let mut out = vec![i];
                    out.append(&mut sub);
                    return Some(out);
                }
            }
            None
        }

        let mut indices: Vec<usize> = Vec::new();
        // Find first segment anywhere in the (transparent-flattened) roots
        let mut chain = find_root_chain(nodes, &path[0])?;
        indices.append(&mut chain);
        // Walk remaining segments, allowing transparent traversal at each step
        let mut cur: &OverseerNode = {
            let mut node_ref: &OverseerNode = &nodes[indices[0]];
            for idx in indices.iter().skip(1) { node_ref = &node_ref.children[*idx]; }
            node_ref
        };
        for name in &path[1..] {
            if cur.name == *name {
                // Path segment refers to current node; continue
                continue;
            }
            if let Some(mut sub) = find_child_chain(cur, name) {
                indices.append(&mut sub);
                // advance cur to new node
                let mut node_ref: &OverseerNode = &nodes[indices[0]];
                for idx in indices.iter().skip(1) { node_ref = &node_ref.children[*idx]; }
                cur = node_ref;
            } else {
                return None;
            }
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
    let mut cur: *mut OverseerNode;
        // Reuse transparent-aware name path resolution to compute indices, then fetch pointer
    let indices = Self::find_indices_by_name_path(nodes, path)?;
        // Walk indices to yield a mutable pointer
        cur = &mut nodes[indices[0]] as *mut _;
        for idx in indices.iter().skip(1) {
            unsafe {
                let cur_ref = &mut *cur;
                if *idx >= cur_ref.children.len() { return None; }
                cur = &mut cur_ref.children[*idx] as *mut _;
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
                #[cfg(feature = "debug-actions")] eprintln!("[ACTIONS] set: Target not found: {} (owner_path={:?}, anchored={}, segments={:?})", target, owner_path, anchored, segments);
                return Err(OverseerError::ValidationError(format!("Target not found: {}", target)));
            }
        };
        let node = Self::get_node_mut_by_indices(nodes, &indices)
            .ok_or_else(|| OverseerError::ValidationError(format!("Target not found: {}", target)))?;
        let key = explicit_param.unwrap_or_else(|| "value".to_string());
        // Equality-aware override: don't mark as override if value is unchanged
        let same = node.parameters.get(&key).map_or(false, |v| v == &value);
        node.parameters.insert(key.clone(), value.clone());
        // If overriding a parameter that had a template marker, remove the marker so it persists
        let marker = format!("_template_{}", key);
        if !same { node.parameters.remove(&marker); }
        // If overriding the 'value' of a template-derived child, mark explicit override for serializer
        if key == "value" {
            if !same {
                node.parameters.insert("_override_present".to_string(), OverseerValue::Boolean(true));
                node.parameters.remove("_template_value");
            }
            // Clear any stale computed value
            node.parameters.remove("_computed_value");
        }
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
                #[cfg(feature = "debug-actions")] eprintln!("[ACTIONS] inc: Target not found: {} (owner_path={:?}, anchored={}, segments={:?})", target, owner_path, anchored, segments);
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
                #[cfg(feature = "debug-actions")] eprintln!("[ACTIONS] toggle: Target not found: {} (owner_path={:?}, anchored={}, segments={:?})", target, owner_path, anchored, segments);
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
                #[cfg(feature = "debug-actions")] eprintln!("[ACTIONS] clear: Target not found: {} (owner_path={:?}, anchored={}, segments={:?})", target, owner_path, anchored, segments);
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

    // Create a list item by cloning template definition and marking template-derived fields
    fn clone_from_template(template: &OverseerNode) -> OverseerNode {
        // Start parameters with template defaults and add _template_ markers so serializer can skip them
        let mut params = template.parameters.clone();
        // Preserve original container type if present on the template (e.g., an instance carries _original_type="div").
        // Fallback to template.node_type when no explicit original type is recorded.
        let orig_ty = match template.parameters.get("_original_type") {
            Some(OverseerValue::String(s)) => s.clone(),
            _ => template.node_type.clone(),
        };
        params.insert("_original_type".to_string(), OverseerValue::String(orig_ty));
    // Mark that this node originates from a template so serializer can suppress inherited children
    params.insert("_from_template".to_string(), OverseerValue::Boolean(true));
        // Add per-parameter template markers (skip internal keys)
        let keys: Vec<String> = params
            .keys()
            .filter(|k| !k.starts_with('_'))
            .cloned()
            .collect();
        for k in keys {
            if let Some(v) = params.get(&k).cloned() {
                params.insert(format!("_template_{}", k), v);
            }
        }

        // Clone children and mark entire subtree as template-derived so we don't save inherited fields
        let mut children = template.children.clone();
        for ch in children.iter_mut() {
            Self::mark_template_child_recursive_action(ch);
            Self::clear_computed_recursive(ch);
        }

        // Preserve the instantiated type semantics: if this template node represents an instance of a component
        // (node_type == template.name) and carries _original_type indicating the container kind (e.g., div),
        // keep node_type equal to the component name (like list/template instantiation does) so fields are found
        // by name, but also record _original_type for layout/rendering. This mirrors resolver behavior for instances.
        let mut node = OverseerNode {
            name: template.name.clone(),
            node_type: template.name.clone(),
            template: None,
            parameters: params,
            children,
            is_hierarchy_transparent: template.is_hierarchy_transparent,
        };
        // Also clear computed params at the root clone
        Self::clear_computed_recursive(&mut node);
        node
    }

    

    // Mark a node and its subtree as template-derived for serializer filtering
    fn mark_template_child_recursive_action(node: &mut OverseerNode) {
        node
            .parameters
            .insert("_template_node".to_string(), OverseerValue::Boolean(true));
        let keys: Vec<String> = node
            .parameters
            .keys()
            .filter(|k| !k.starts_with('_'))
            .cloned()
            .collect();
        for k in keys {
            if let Some(v) = node.parameters.get(&k).cloned() {
                node.parameters.insert(format!("_template_{}", k), v);
            }
        }
        for ch in node.children.iter_mut() {
            Self::mark_template_child_recursive_action(ch);
        }
    }

    fn set_field_value_on_item(item: &mut OverseerNode, field: &str, value: OverseerValue) {
        if let Some(child) = item.children.iter_mut().find(|c| c.name == field) {
            child.parameters.insert("value".to_string(), value);
            // Mark explicit override so serializer will persist this child even if template-derived
            child.parameters.insert("_override_present".to_string(), OverseerValue::Boolean(true));
            child.parameters.insert("_explicit_child_override".to_string(), OverseerValue::Boolean(true));
            // Remove template marker for value on this field if present
            child.parameters.remove("_template_value");
            // Clear any stale computed value on this field
            child.parameters.remove("_computed_value");
            // Track override at parent level for completeness
            let entry = item
                .parameters
                .entry("_explicit_overrides".to_string())
                .or_insert(OverseerValue::String(String::new()));
            if let OverseerValue::String(s) = entry {
                if !s.split(',').any(|n| n == field) {
                    if !s.is_empty() { s.push(','); }
                    s.push_str(field);
                }
            }
        } else {
            // Add simple string field if missing
            let new_field = OverseerNode {
                name: field.to_string(),
                node_type: "string".to_string(),
                template: None,
                parameters: {
                    let mut m = std::collections::HashMap::new();
                    m.insert("value".to_string(), value);
                    m.insert("_override_present".to_string(), OverseerValue::Boolean(true));
                    m.insert("_explicit_child_override".to_string(), OverseerValue::Boolean(true));
                    m
                },
                children: Vec::new(),
                is_hierarchy_transparent: false,
            };
            item.children.push(new_field);
            // Track override at parent level
            let entry = item
                .parameters
                .entry("_explicit_overrides".to_string())
                .or_insert(OverseerValue::String(String::new()));
            if let OverseerValue::String(s) = entry {
                if !s.split(',').any(|n| n == field) {
                    if !s.is_empty() { s.push(','); }
                    s.push_str(field);
                }
            }
        }
    }

    // Remove any computed shadow parameters so formulas recompute in the new instance context
    fn clear_computed_recursive(node: &mut OverseerNode) {
        let keys: Vec<String> = node
            .parameters
            .keys()
            .filter(|k| k.starts_with("_computed_"))
            .cloned()
            .collect();
        for k in keys { node.parameters.remove(&k); }
        // Special-case top-level computed value
        node.parameters.remove("_computed_value");
        for ch in node.children.iter_mut() { Self::clear_computed_recursive(ch); }
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
    // Assign a unique instance name mirroring resolver reload semantics (e.g., T__1, T__2)
    // This prevents duplicate sibling names that break name-based path resolution during formula evaluation.
    let ordinal = list_node.children.len() + 1; // 1-based index after append
    new_item.name = format!("{}__{}", template_def.name, ordinal);
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
    overrides: &Vec<OverseerNode>,
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
                    // Assign a unique instance name (e.g., T__1, T__2) to avoid duplicate sibling names.
                    // Using current length+1 reflects the creation order and matches resolver's reload naming convention.
                    let ordinal = list_node.children.len() + 1;
                    new_item.name = format!("{}__{}", template_def.name, ordinal);
                    // Apply evaluated overrides from action block
                    Self::apply_overrides_evaluated(&mut new_item, overrides, owner_path, &snapshot)?;
                    list_node.children.push(new_item);
                }
                OverseerValue::String(type_name) => {
                    // Simple type list requires a value
                    let val = value_opt.ok_or_else(|| OverseerError::ValidationError("append.value required for simple list".to_string()))?;
                    let mut item = OverseerNode {
                        name: format!("{}__{}", type_name, list_node.children.len() + 1),
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

    // Evaluate formulas in value/params against owner_path context and apply into target
    fn apply_overrides_evaluated(
        target: &mut OverseerNode,
        overrides: &Vec<OverseerNode>,
        owner_path: &[String],
        snapshot: &Vec<OverseerNode>,
    ) -> Result<(), OverseerError> {
        for ov in overrides {
            let name = ov.name.clone();
            // Find or create corresponding child in target
            let idx_opt = target.children.iter().position(|c| c.name == name);
            if let Some(idx) = idx_opt {
                // Merge parameters (evaluate any Formula)
                let child = target.children.get_mut(idx).unwrap();
                // Apply parameters
                for (k, v) in ov.parameters.iter() {
                    let eval = Self::evaluate_in_context(v, owner_path, snapshot)?;
                    child.parameters.insert(k.clone(), eval);
                }
                // Mark explicit override and clear template marker for value, if present
                child.parameters.insert("_override_present".to_string(), OverseerValue::Boolean(true));
                child.parameters.insert("_explicit_child_override".to_string(), OverseerValue::Boolean(true));
                child.parameters.remove("_template_value");
                // Track at parent level
                let entry = target
                    .parameters
                    .entry("_explicit_overrides".to_string())
                    .or_insert(OverseerValue::String(String::new()));
                if let OverseerValue::String(s) = entry {
                    if !s.split(',').any(|n| n == name) {
                        if !s.is_empty() { s.push(','); }
                        s.push_str(&name);
                    }
                }
                // Recurse into children overrides
                if !ov.children.is_empty() {
                    Self::apply_overrides_evaluated(child, &ov.children, owner_path, snapshot)?;
                }
            } else {
                // Create new child with inferred type from override/value
                let mut new_child = OverseerNode {
                    name: name.clone(),
                    node_type: ov.node_type.clone(),
                    template: None,
                    parameters: Default::default(),
                    children: Vec::new(),
                    is_hierarchy_transparent: false,
                };
                // Copy/evaluate params
                for (k, v) in ov.parameters.iter() {
                    let eval = Self::evaluate_in_context(v, owner_path, snapshot)?;
                    new_child.parameters.insert(k.clone(), eval);
                }
                new_child.parameters.insert("_override_present".to_string(), OverseerValue::Boolean(true));
                new_child.parameters.insert("_explicit_child_override".to_string(), OverseerValue::Boolean(true));
                new_child.parameters.remove("_template_value");
                // Recurse
                if !ov.children.is_empty() {
                    Self::apply_overrides_evaluated(&mut new_child, &ov.children, owner_path, snapshot)?;
                }
                // Infer node type from evaluated value if empty
                if new_child.node_type.is_empty() {
                    if let Some(val) = new_child.parameters.get("value") {
                        new_child.node_type = match val {
                            OverseerValue::Integer(_) => "int".to_string(),
                            OverseerValue::Float(_) => "float".to_string(),
                            OverseerValue::Boolean(_) => "bool".to_string(),
                            OverseerValue::String(_) => "string".to_string(),
                            OverseerValue::Date(_) => "date".to_string(),
                            OverseerValue::Timestamp(_) => "timestamp".to_string(),
                            _ => new_child.node_type.clone(),
                        };
                    }
                }
                target.children.push(new_child);
                // Track at parent level
                let entry = target
                    .parameters
                    .entry("_explicit_overrides".to_string())
                    .or_insert(OverseerValue::String(String::new()));
                if let OverseerValue::String(s) = entry {
                    if !s.split(',').any(|n| n == name) {
                        if !s.is_empty() { s.push(','); }
                        s.push_str(&name);
                    }
                }
            }
        }
        Ok(())
    }

    fn evaluate_in_context(
        val: &OverseerValue,
        owner_path: &[String],
        snapshot: &Vec<OverseerNode>,
    ) -> Result<OverseerValue, OverseerError> {
        match val {
            OverseerValue::Formula(expr) => {
                let ctx = EvaluationContext::new(owner_path.to_vec(), snapshot);
                let evaluated = FormulaEvaluator::evaluate_formula(expr, &ctx)?;
                Ok(evaluated)
            }
            other => Ok(other.clone()),
        }
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

    fn sort_list(
        nodes: &mut Vec<OverseerNode>,
        owner_path: &[String],
        list_path: &str,
        by_expr: &str,
        order: &str,
        stable: bool,
    ) -> Result<(), OverseerError> {
        let (segments, _explicit_param, anchored) = Self::split_path_and_param(list_path);
        let snapshot = nodes.clone();
        let indices = Self::resolve_target_indices(&snapshot, owner_path, anchored, &segments)
            .ok_or_else(|| OverseerError::ValidationError(format!("List not found: {}", list_path)))?;
        let list_node = Self::get_node_mut_by_indices(nodes, &indices)
            .ok_or_else(|| OverseerError::ValidationError(format!("List not found: {}", list_path)))?;
        if list_node.node_type != "list" { return Err(OverseerError::ValidationError("sort.target is not a list".to_string())); }

    // Prepare evaluation context base for by_expr; we'll bind 'x' to each item
    // For current_node in base context, use the list node snapshot for relative paths
    let list_snapshot = &snapshot[indices[0]]; // root of path's first segment
    // Re-traverse to get the exact list snapshot node reference for context
    let mut cur: &OverseerNode = list_snapshot;
        for idx in &indices[1..] {
            cur = &cur.children[*idx];
        }

        // Take children to reorder
        let children_snapshot = cur.children.clone();
        let taken = std::mem::take(&mut list_node.children);
        let mut entries: Vec<(OverseerValue, OverseerNode, usize)> = Vec::with_capacity(taken.len());

        for (i, item) in taken.into_iter().enumerate() {
            // Build context with x bound to snapshot item
            let base_ctx = EvaluationContext {
                current_node: cur,
                parent_node: None,
                document_root: &snapshot,
                node_path: owner_path.to_vec(),
                var_bindings: std::collections::HashMap::new(),
            };
            let ctx = base_ctx.with_var("x", BoundValue::Node(&children_snapshot[i]));
            let key = match FormulaEvaluator::evaluate_formula(by_expr, &ctx) {
                Ok(v) => v,
                Err(_) => OverseerValue::String(String::new()),
            };
            entries.push((key, item, i));
        }

        // Choose comparator
        let cmp = |a: &OverseerValue, b: &OverseerValue| Self::compare_overseer_values(a, b);
        if stable {
            if order == "desc" {
                entries.sort_by(|(ka, _ia, _xa), (kb, _ib, _xb)| cmp(kb, ka));
            } else {
                entries.sort_by(|(ka, _ia, _xa), (kb, _ib, _xb)| cmp(ka, kb));
            }
        } else {
            if order == "desc" {
                entries.sort_unstable_by(|(ka, _ia, _xa), (kb, _ib, _xb)| cmp(kb, ka));
            } else {
                entries.sort_unstable_by(|(ka, _ia, _xa), (kb, _ib, _xb)| cmp(ka, kb));
            }
        }

        list_node.children = entries.into_iter().map(|(_k, item, _i)| item).collect();
        Ok(())
    }

    fn compare_overseer_values(a: &OverseerValue, b: &OverseerValue) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (a, b) {
            (OverseerValue::Integer(x), OverseerValue::Integer(y)) => x.cmp(y),
            (OverseerValue::Float(x), OverseerValue::Float(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
            (OverseerValue::Integer(x), OverseerValue::Float(y)) => (*x as f64).partial_cmp(y).unwrap_or(Ordering::Equal),
            (OverseerValue::Float(x), OverseerValue::Integer(y)) => x.partial_cmp(&(*y as f64)).unwrap_or(Ordering::Equal),
            (OverseerValue::String(x), OverseerValue::String(y)) => x.cmp(y),
            (OverseerValue::Boolean(x), OverseerValue::Boolean(y)) => x.cmp(y),
            (OverseerValue::Date(x), OverseerValue::Date(y)) => x.cmp(y), // ISO-8601 strings are lex comparable
            // Mixed types: compare their string representations
            _ => Self::value_to_string(a).cmp(&Self::value_to_string(b)),
        }
    }

    fn value_to_string(v: &OverseerValue) -> String {
        match v {
            OverseerValue::Integer(i) => i.to_string(),
            OverseerValue::Float(f) => f.to_string(),
            OverseerValue::String(s) => s.clone(),
            OverseerValue::Boolean(b) => b.to_string(),
            OverseerValue::Date(d) => d.clone(),
            OverseerValue::Timestamp(ts) => ts.clone(),
            OverseerValue::Color(c) => format!("{:?}", c),
            OverseerValue::CssSize(s) => format!("{:?}", s),
            OverseerValue::BorderStyle(s) => format!("{:?}", s),
            OverseerValue::Formula(s) => s.clone(),
            OverseerValue::Template(s) => s.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_document;
    use crate::resolver::resolve_document;

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

    #[test]
    fn test_sort_list_by_number_desc() {
        let input = r#"
        div Root {
            div Task { string id = "" int priority = 0 }
            list Tasks (entry=<Task>, key="id") {
                - Task { id = "a" priority = 1 }
                - Task { id = "b" priority = 3 }
                - Task { id = "c" priority = 2 }
            }
            button Sort { on click { sort(list="/Root/Tasks", by="$(x/priority)", order="desc") } }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolver::resolve_document(&mut nodes);
        let path = vec!["Root".to_string(), "Sort".to_string()];
        let res = ActionExecutor::execute_event(&mut nodes, &path, "click");
        assert!(res.is_ok());
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let list = root.children.iter().find(|c| c.name == "Tasks").unwrap();
        let ids: Vec<String> = list.children.iter().map(|it| {
            it.children.iter().find(|f| f.name=="id").and_then(|f| f.parameters.get("value")).and_then(|v| if let OverseerValue::String(s)=v { Some(s.clone()) } else { None }).unwrap_or_default()
        }).collect();
        assert_eq!(ids, vec!["b".to_string(), "c".to_string(), "a".to_string()]);
    }

    #[test]
    fn test_timer_inactive_does_not_fire() {
        let input = r#"
        div Root {
            int A = 0
            string T = $(now())
            timer t1 (active=false, at=$(../T)) { on timeout { inc(path="/Root/A", by=1) } }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        // Tick should not change A since timer is inactive
        let _ = ActionExecutor::tick(&mut nodes);
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let a = root.children.iter().find(|c| c.name == "A").unwrap();
        assert_eq!(a.parameters.get("value"), Some(&OverseerValue::Integer(0)));
    }

    #[test]
    fn test_timer_fires_and_deactivates() {
        let input = r#"
        div Root {
            int A = 0
            string T = "2000-01-01T00:00:00Z"
            timer t1 (active=true, at=$(../T)) { on timeout { inc(path="/Root/A", by=1) } }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let _ = ActionExecutor::tick(&mut nodes);
        // After tick, A should be 1 and timer inactive
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let a = root.children.iter().find(|c| c.name == "A").unwrap();
        assert_eq!(a.parameters.get("value"), Some(&OverseerValue::Integer(1)));
    let timer = root.children.iter().find(|c| c.node_type == "timer").unwrap();
        assert_eq!(timer.parameters.get("active"), Some(&OverseerValue::Boolean(false)));
    }

    #[test]
    fn test_timer_future_does_not_fire_until_due() {
        let input = r#"
        div Root {
            int A = 0
            string T = "2999-01-01T00:00:00Z"
            timer t1 (active=true, at=$(../T)) { on timeout { inc(path="/Root/A", by=1) } }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let _ = ActionExecutor::tick(&mut nodes);
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let a = root.children.iter().find(|c| c.name == "A").unwrap();
        assert_eq!(a.parameters.get("value"), Some(&OverseerValue::Integer(0)));
    }
}

// Additional tests for helpers
#[cfg(test)]
mod tests_clone_from_template {
    use super::*;
    use crate::parser::parse_document;
    use crate::resolver::resolve_document;
    use crate::file_ops::OverseerFileHandler;

    #[test]
    fn preserves_original_type_when_cloning_instance_template() {
        // Create a pseudo instance-like template with node_type "ComponentName" but _original_type "div"
        let mut t = OverseerNode {
            name: "ComponentName".to_string(),
            node_type: "ComponentName".to_string(),
            template: None,
            parameters: Default::default(),
            children: vec![],
            is_hierarchy_transparent: false,
        };
        t.parameters.insert("_original_type".to_string(), OverseerValue::String("div".to_string()));
        let cloned = ActionExecutor::clone_from_template(&t);
        assert_eq!(cloned.parameters.get("_original_type"), Some(&OverseerValue::String("div".to_string())));
        // node_type should remain the component name for consistency with instances
        assert_eq!(cloned.node_type, "ComponentName");
    }

    #[test]
    fn append_with_overrides_serializes_as_object_not_primitive() {
        let input = r#"
        div Root {
            div T { int i = 10 int ii = $(2*i) }
            button B { on click { append(list="/Root/L") { - i = 20 } } }
            button C { on click { append(list="/Root/L") { - i = 30 } } }
            int x = 50
            button D (label="button 3") {
                on click { append(list="/Root/L") { - i = $(x) } }
            }
            list L (entry=<T>, layout="horizontal") { }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        // Click B, C, D to append three items
        assert!(ActionExecutor::execute_event(&mut nodes, &vec!["Root".into(), "B".into()], "click").is_ok());
        assert!(ActionExecutor::execute_event(&mut nodes, &vec!["Root".into(), "C".into()], "click").is_ok());
        assert!(ActionExecutor::execute_event(&mut nodes, &vec!["Root".into(), "D".into()], "click").is_ok());
        // Serialize and verify list entries are objects with named field overrides
        let s = OverseerFileHandler::serialize_nodes(&nodes).unwrap();
    assert!(s.contains("list L ("));
    assert!(s.contains("entry=<T>"));
    assert!(s.contains("layout=\"horizontal\""));
        // Ensure we do not emit primitive entries like "- 20"/"- 30"/"- 50"
        assert!(!s.contains("\n    - 20\n"), "Should not serialize primitive '- 20' entries.\n{}", s);
        assert!(!s.contains("\n    - 30\n"), "Should not serialize primitive '- 30' entries.\n{}", s);
        assert!(!s.contains("\n    - 50\n"), "Should not serialize primitive '- 50' entries.\n{}", s);
        // Should contain object block entries with named override lines
    // Check entries have object blocks and named overrides without relying on exact whitespace
    let dash_block_count = s.matches("\n        - {").count() + s.matches("\n    - {").count();
    assert!(dash_block_count >= 3, "Expected at least three '- {{ ... }}' blocks.\n{}", s);
    assert!(s.contains("- i = 20"), "Expected an override '- i = 20'.\n{}", s);
    assert!(s.contains("- i = 30"), "Expected an override '- i = 30'.\n{}", s);
    assert!(s.contains("- i = 50"), "Expected an override '- i = 50'.\n{}", s);
    }
}

#[cfg(test)]
mod tests_append_naming_and_transparency {
    use super::*;
    use crate::parser::parse_document;

    #[test]
    fn append_assigns_unique_names() {
        let input = r#"div T { int i = 10 int ii = $(2*i) }
list L (entry=<T>) { }
button B { on click { append (template="<T>", list="/L") { - i = 20 } } }
button B2 { on click { append (template="<T>", list="/L") { - i = 25 } } }
"#;
        let mut nodes = parse_document(input).unwrap().1;
        // Click B then B2
        assert!(ActionExecutor::execute_event(&mut nodes, &vec!["B".into()], "click").is_ok());
        assert!(ActionExecutor::execute_event(&mut nodes, &vec!["B2".into()], "click").is_ok());
        // Find list L
        let l = nodes.iter().find(|n| n.name=="L").unwrap();
        assert_eq!(l.children.len(), 2);
        assert_eq!(l.children[0].name, "T__1");
        assert_eq!(l.children[1].name, "T__2");
        // Values should be set on their own children
        let i1 = l.children[0].children.iter().find(|c| c.name=="i").unwrap();
        let i2 = l.children[1].children.iter().find(|c| c.name=="i").unwrap();
        assert_eq!(i1.parameters.get("value"), Some(&OverseerValue::Integer(20)));
        assert_eq!(i2.parameters.get("value"), Some(&OverseerValue::Integer(25)));
    }

    #[test]
    fn transparent_unnamed_nodes_in_paths() {
        // Unnamed div should be transparent for path resolution when targeting L and buttons
        let input = r#"div (hidden=true) {
    div ExerciseRecord { int x = 1 }
    div ExerciseRecordSample { int y = 2 }
}
list L (entry=<ExerciseRecord>) { }
button Add { on click { append (template="<ExerciseRecord>", list="/L") { - x = 3 } } }
"#;
        let mut nodes = parse_document(input).unwrap().1;
        // Should be able to click Add even though templates are under unnamed div
        assert!(ActionExecutor::execute_event(&mut nodes, &vec!["Add".into()], "click").is_ok());
        let l = nodes.iter().find(|n| n.name=="L").unwrap();
        assert_eq!(l.children.len(), 1);
        let x = l.children[0].children.iter().find(|c| c.name=="x").unwrap();
        assert_eq!(x.parameters.get("value"), Some(&OverseerValue::Integer(3)));
    }
}
