use crate::types::{OverseerValue, OverseerError, OverseerNode};
use nom::{
    IResult,
    branch::alt,
    bytes::complete::{tag, take_while1, take_while},
    character::complete::{char, multispace0},
    combinator::{map, opt},
    multi::many0,
    number::complete::double,
    sequence::{delimited, pair, preceded, tuple},
};

// Debug logging macro for evaluator
macro_rules! debug_evaluator {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-evaluator")]
        println!($($arg)*);
    };
}

/// A value bound in lambda evaluation: either a node reference or a value
#[derive(Debug, Clone)]
pub enum BoundValue<'a> {
    Node(&'a OverseerNode),
    Value(OverseerValue),
}

/// Context for formula evaluation, tracking current position in document tree
#[derive(Debug, Clone)]
pub struct EvaluationContext<'a> {
    pub current_node: &'a OverseerNode,
    #[allow(dead_code)]
    pub parent_node: Option<&'a OverseerNode>,
    pub document_root: &'a [OverseerNode],
    pub node_path: Vec<String>, // Path from root to current node for debugging
    pub var_bindings: std::collections::HashMap<String, BoundValue<'a>>, // lambda variables
}

/// Represents a parsed formula expression
#[derive(Debug, Clone)]
pub enum FormulaExpression {
    Number(f64),
    StringLiteral(String),
    FieldReference(String),
    PathReference(Vec<String>), // e.g., ["..","field"] for ../field
    PathParam { path: Vec<String>, param: String },
    // Lambda literal and method chaining for list pipelines
    Lambda { params: Vec<String>, body: Box<FormulaExpression> },
    MethodChain { base: Box<FormulaExpression>, calls: Vec<MethodCall> },
    UnaryOp {
        operator: UnaryOperator,
        expr: Box<FormulaExpression>,
    },
    BinaryOp {
        left: Box<FormulaExpression>,
        operator: BinaryOperator,
        right: Box<FormulaExpression>,
    },
    FunctionCall {
        name: String,
        args: Vec<FormulaExpression>,
    },
    Conditional {
        condition: Box<FormulaExpression>,
        then_branch: Box<FormulaExpression>,
        else_branch: Box<FormulaExpression>,
    },
    // Follow a child path from an expression that resolves to a node, e.g., expr/child/grand
    PathFollow {
        base: Box<FormulaExpression>,
        segments: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub struct MethodCall {
    pub name: String,
    pub args: Vec<FormulaExpression>,
}

#[derive(Debug, Clone)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    // Added comparisons (step 4.1)
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    // Boolean logic
    And,
    Or,
}

#[derive(Debug, Clone)]
pub enum UnaryOperator {
    Not,
}

/// Formula evaluator - handles parsing and evaluation of $(expression) formulas
pub struct FormulaEvaluator;

impl FormulaEvaluator {
    /// Evaluate a lambda (or general expression) against a list item by binding the item as `x`.
    /// If `lambda_src` parses as a Lambda, it will be invoked; otherwise the expression is evaluated
    /// with implicit `x` bound to the item (consistent with map/filter implicit forms).
    pub fn evaluate_lambda_on_item(
        lambda_src: &str,
        context: &EvaluationContext,
        item_node: &OverseerNode,
    ) -> Result<OverseerValue, OverseerError> {
        let expr = match Self::parse_expression(lambda_src) {
            Ok(e) => e,
            Err(_) => return Err(OverseerError::FormulaError("invalid formula error".to_string())),
        };
        match &expr {
            FormulaExpression::Lambda { .. } => Self::eval_lambda(&expr, None, Some(item_node), None, context),
            _ => Self::eval_lambda(&expr, None, Some(item_node), None, context),
        }
    }
    /// Helper: get effective parameter value preferring computed shadow
    fn get_effective_param<'p>(params: &'p std::collections::HashMap<String, OverseerValue>, key: &str) -> Option<&'p OverseerValue> {
        // Semantics: for 'value', prefer raw when it's not a Formula; if it's a Formula and no computed shadow yet,
        // return None so callers can decide to evaluate the raw formula in the correct context (avoids stale ordering).
        if key == "value" {
            if let Some(raw) = params.get("value") {
                if !matches!(raw, OverseerValue::Formula(_)) {
                    return Some(raw);
                }
            }
            if let Some(comp) = params.get("_computed_value") { return Some(comp); }
            // Do NOT return the raw Formula here; let callers evaluate it when needed
            return None;
        } else {
            let shadow = format!("_computed_{}", key);
            if let Some(v) = params.get(&shadow) { return Some(v); }
            return params.get(key);
        }
    }
    /// Evaluate a formula expression string within the given context
    pub fn evaluate_formula(
        formula: &str,
        context: &EvaluationContext,
    ) -> Result<OverseerValue, OverseerError> {
    debug_evaluator!("[EVAL] Start evaluate_formula at path {:?}: {}", context.node_path, formula);
        // Parse the formula expression
        let expression = match Self::parse_expression(formula) {
            Ok(expr) => expr,
            Err(_err) => {
                debug_evaluator!("[EVAL] Parse error at {:?}: <hidden> => '{}'", context.node_path, formula);
                return Err(OverseerError::FormulaError("invalid formula error".to_string()));
            }
        };
    debug_evaluator!("[EVAL] Parsed AST: {:?}", expression);

        // Evaluate the parsed expression
    let result = Self::evaluate_expression(&expression, context);
    debug_evaluator!("[EVAL] End evaluate_formula at path {:?}: result = {:?}", context.node_path, result);
    result
    }

    /// Parse a formula string into a FormulaExpression
    fn parse_expression(input: &str) -> Result<FormulaExpression, String> {
        let trimmed = input.trim();
    match expression(trimmed) {
            Ok((remaining, expr)) => {
                if remaining.trim().is_empty() {
                    Ok(expr)
                } else {
                    Err(format!("Unexpected characters: {}", remaining))
                }
            }
            Err(_) => Err("Failed to parse expression".to_string()),
        }
    }

    /// Evaluate a parsed FormulaExpression within the given context
    fn evaluate_expression(
        expr: &FormulaExpression,
        context: &EvaluationContext,
    ) -> Result<OverseerValue, OverseerError> {
    debug_evaluator!("[EVAL] Eval expr at {:?}: {:?}", context.node_path, expr);
        match expr {
            FormulaExpression::Number(n) => {
                // Return as Integer (i64) if whole number, otherwise Float
                if n.fract() == 0.0 && *n >= i64::MIN as f64 && *n <= i64::MAX as f64 {
                    Ok(OverseerValue::Integer(*n as i64))
                } else {
                    Ok(OverseerValue::Float(*n))
                }
            }
            FormulaExpression::StringLiteral(s) => Ok(OverseerValue::String(s.clone())),
            FormulaExpression::FieldReference(field_name) => {
                debug_evaluator!("[EVAL] Resolve field '{}' at {:?}", field_name, context.node_path);
                let out = Self::resolve_field_reference(field_name, context);
                debug_evaluator!("[EVAL] Resolved field '{}' => {:?}", field_name, out);
                out
            }
            FormulaExpression::PathReference(path) => {
                debug_evaluator!("[EVAL] Resolve path {:?} at {:?}", path, context.node_path);
                let out = Self::resolve_path_reference(path, context);
                debug_evaluator!("[EVAL] Resolved path {:?} => {:?}", path, out);
                out
            }
            FormulaExpression::PathParam { path, param } => {
                debug_evaluator!("[EVAL] Resolve path param {:?}.{} at {:?}", path, param, context.node_path);
                let out = Self::resolve_path_param(path, param, context);
                debug_evaluator!("[EVAL] Resolved path param {:?}.{} => {:?}", path, param, out);
                out
            }
            FormulaExpression::Lambda { .. } => {
                Err(OverseerError::FormulaError("Unexpected top-level lambda; use inside map/filter/reduce".to_string()))
            }
            FormulaExpression::MethodChain { base, calls } => {
                debug_evaluator!("[EVAL] Method chain on base {:?} with calls {:?}", base, calls);
                let out = Self::evaluate_method_chain(base, calls, context);
                debug_evaluator!("[EVAL] Method chain result => {:?}", out);
                out
            }
            FormulaExpression::UnaryOp { operator, expr } => {
                match operator {
                    UnaryOperator::Not => {
                        let val = Self::evaluate_expression(expr, context)?;
                        let b = Self::value_to_bool(&val)?;
                        Ok(OverseerValue::Boolean(!b))
                    }
                }
            }
            FormulaExpression::BinaryOp { left, operator, right } => {
                // Short-circuit for boolean ops
                match operator {
                    BinaryOperator::And => {
                        let left_val = Self::evaluate_expression(left, context)?;
                        let lb = Self::value_to_bool(&left_val)?;
                        if !lb {
                            return Ok(OverseerValue::Boolean(false));
                        }
                        let right_val = Self::evaluate_expression(right, context)?;
                        let rb = Self::value_to_bool(&right_val)?;
                        Ok(OverseerValue::Boolean(rb))
                    }
                    BinaryOperator::Or => {
                        let left_val = Self::evaluate_expression(left, context)?;
                        let lb = Self::value_to_bool(&left_val)?;
                        if lb {
                            return Ok(OverseerValue::Boolean(true));
                        }
                        let right_val = Self::evaluate_expression(right, context)?;
                        let rb = Self::value_to_bool(&right_val)?;
                        Ok(OverseerValue::Boolean(rb))
                    }
                    _ => {
                        let left_val = Self::evaluate_expression(left, context)?;
            let right_val = Self::evaluate_expression(right, context)?;
            let res = Self::apply_binary_operator(&left_val, operator, &right_val);
            debug_evaluator!("[EVAL] Binary {:?} {:?} {:?} => {:?}", left_val, operator, right_val, res);
            res
                    }
                }
            }
            FormulaExpression::FunctionCall { name, args } => {
        debug_evaluator!("[EVAL] Function call {} with {} args", name, args.len());
        let out = Self::evaluate_function_call(name, args, context);
        debug_evaluator!("[EVAL] Function {} => {:?}", name, out);
        out
            }
            FormulaExpression::Conditional { condition, then_branch, else_branch } => {
                let cond_val = Self::evaluate_expression(condition, context)?;
                let cond_bool = Self::value_to_bool(&cond_val)?;
        debug_evaluator!("[EVAL] Ternary condition {:?} => {}", cond_val, cond_bool);
                if cond_bool {
                    Self::evaluate_expression(then_branch, context)
                } else {
                    Self::evaluate_expression(else_branch, context)
                }
            }
            FormulaExpression::PathFollow { base, segments } => {
                // Resolve base to a node, then follow child segments and return final node's value
                if let Some(mut cur) = Self::eval_expr_to_node(base, context) {
                    let mut p = context.node_path.clone();
                    for seg in segments {
                        if let Some(next) = cur.get_accessible_children().into_iter().find(|c| &c.name == seg) {
                            p.push(next.name.clone());
                            cur = next;
                        } else {
                            return Err(OverseerError::FormulaError(format!("Path segment not found: {}", seg)));
                        }
                    }
                    // Prefer evaluating a raw value formula if present
                    if let Some(OverseerValue::Formula(f)) = cur.parameters.get("value") {
                        let child_ctx = EvaluationContext::new_with_current(cur, p, context.document_root);
                        return FormulaEvaluator::evaluate_formula(f, &child_ctx);
                    }
                    if let Some(v) = Self::get_effective_param(&cur.parameters, "value") { return Ok(v.clone()); }
                    // If no value, return a null-ish string for now
                    return Ok(OverseerValue::String("null".to_string()));
                }
                Err(OverseerError::FormulaError("Base of path does not resolve to a node".to_string()))
            }
        }
    }

    /// Resolve a simple field reference within the current node
    fn resolve_field_reference(
        field_name: &str,
        context: &EvaluationContext,
    ) -> Result<OverseerValue, OverseerError> {
        debug_evaluator!("[EVAL] resolve_field_reference '{}' at {:?}", field_name, context.node_path);
        // Strategy:
        // 0) Lambda-bound variable lookup
        if let Some(bound) = context.var_bindings.get(field_name) {
            debug_evaluator!("[EVAL] '{}' bound in lambda: {:?}", field_name, bound);
            return match bound {
                BoundValue::Value(v) => Ok(v.clone()),
                BoundValue::Node(n) => {
                    if let Some(v) = Self::get_effective_param(&n.parameters, "value") { Ok(v.clone()) }
                    else { Err(OverseerError::FormulaError(format!("Bound node '{}' has no value", field_name))) }
                }
            };
        }
    // 1) Try current node's parameters by exact field name
    // Safety-first: use effective/computed value to avoid self-recursive evaluation.
    if let Some(val) = Self::get_effective_param(&context.current_node.parameters, field_name) {
            return Ok(val.clone());
        }

        // 2) Try to find a child node with this name and return its "value" parameter if present
        if let Some(child) = context
            .current_node
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == field_name)
        {
            debug_evaluator!("[EVAL] Found child '{}' under current node", field_name);
            // Prefer effective/computed value first to avoid triggering recursive dependencies
            if let Some(val) = Self::get_effective_param(&child.parameters, "value") {
                return Ok(val.clone());
            }
            // If no effective/computed value exists (e.g., child hasn't been resolved yet), evaluate raw value formula
            if let Some(OverseerValue::Formula(formula_expr)) = child.parameters.get("value") {
                let mut path = context.node_path.clone();
                path.push(child.name.clone());
                let child_ctx = EvaluationContext::new_with_current(child, path, context.document_root);
                return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
            }
        }

        // 3) Look up in parent scope: siblings or parent parameters
    if !context.node_path.is_empty() {
            // Walk up the ancestor chain from nearest parent to root
            for end in (1..=context.node_path.len() - 1).rev() {
                let ancestor_path = &context.node_path[..end];
                if let Some(ancestor) = FormulaEvaluator::resolve_path_to_node(ancestor_path, context.document_root) {
            debug_evaluator!("[EVAL] Searching ancestor {:?} for '{}'", ancestor_path, field_name);
                    // a) Ancestor parameters by key: use effective/computed only (avoid evaluating raw formulas here)
                    if let Some(val) = Self::get_effective_param(&ancestor.parameters, field_name) {
                        return Ok(val.clone());
                    }
                    // b) Child of ancestor by name
                    if let Some(child) = ancestor.get_accessible_children().into_iter().find(|c| c.name == field_name) {
                        debug_evaluator!("[EVAL] Found ancestor child '{}' under {:?}", field_name, ancestor_path);
                        if let Some(val) = Self::get_effective_param(&child.parameters, "value") {
                            return Ok(val.clone());
                        }
                        // Only if no effective/computed value exists, evaluate raw value formula for ancestor child
                        if let Some(OverseerValue::Formula(formula_expr)) = child.parameters.get("value") {
                            let mut path = ancestor_path.to_vec();
                            path.push(child.name.clone());
                            let child_ctx = EvaluationContext::new_with_current(child, path, context.document_root);
                            return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
                        }
                    }
                    // c) Deep search under ancestor (first match)
                    if let Some(val) = FormulaEvaluator::find_value_by_name_deep(ancestor, field_name) {
                        return Ok(val);
                    }
                }
            }
        }

        // 3) If current node is a simple field node (int/string/etc.), allow shorthand resolving its own value
        if field_name == "value" {
            if let Some(v) = Self::get_effective_param(&context.current_node.parameters, "value") {
                return Ok(v.clone());
            }
        }

        // 4) Fallback: search the entire document for a node with this name and return its value
        if let Some(node) = FormulaEvaluator::find_node_by_name(context.document_root, field_name) {
            debug_evaluator!("[EVAL] Global fallback found node '{}' at root search", field_name);
            if let Some(v) = Self::get_effective_param(&node.parameters, "value") {
                if let OverseerValue::Formula(formula_expr) = v {
                    // Best-effort: evaluate with current path (unknown exact path)
                    let child_ctx = EvaluationContext::new(context.node_path.clone(), context.document_root);
                    return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
                }
                return Ok(v.clone());
            }
        }

        Err(OverseerError::FormulaError(format!(
            "Unknown field '{}' in current scope",
            field_name
        )))
    }

    /// Resolve a path reference like ../field or ../../other
    fn resolve_path_reference(
        path: &[String],
        context: &EvaluationContext,
    ) -> Result<OverseerValue, OverseerError> {
    debug_evaluator!("[EVAL] resolve_path_reference {:?} at {:?}", path, context.node_path);
        if path.is_empty() {
            return Err(OverseerError::FormulaError("Empty path reference".to_string()));
        }

        // Case 0: "/"-anchored paths (within current container/nearest ancestor containing first segment)
        if path[0] == "/" {
            let segments = &path[1..];
            if segments.is_empty() {
                return Err(OverseerError::FormulaError("Path missing target after /".to_string()));
            }
            // Find nearest ancestor that contains the first segment as a child; fallback to current
            let mut base_path = context.node_path.clone();
            let mut found_start = context.current_node;
            let first_seg = &segments[0];
            let mut end = context.node_path.len();
            let mut found = false;
            while end > 0 {
                if let Some(candidate) = Self::resolve_path_to_node(&context.node_path[..end].to_vec(), context.document_root) {
                    if candidate.get_accessible_children().into_iter().any(|c| &c.name == first_seg) {
                        found_start = candidate;
                        base_path = context.node_path[..end].to_vec();
                        debug_evaluator!("[EVAL] '/' anchored base {:?} chosen for first seg '{}'", base_path, first_seg);
                        found = true;
                        break;
                    }
                }
                end -= 1;
            }
        let mut skip_first = false;
        if !found {
                // Fallback: treat as absolute root path if a top-level node matches first segment
                if let Some(root_match) = context.document_root.iter().find(|n| &n.name == first_seg) {
                    found_start = root_match;
                    base_path = vec![first_seg.clone()];
                    debug_evaluator!("[EVAL] '/' absolute base {:?} chosen at root for first seg '{}'", base_path, first_seg);
            skip_first = true;
                }
            }
            // Traverse all but last via children names from found_start
            let mut current = found_start;
            let mut p = base_path.clone();
        let start_idx = if skip_first { 1 } else { 0 };
        for seg in &segments[start_idx..segments.len().saturating_sub(1)] {
                if let Some(next) = current.get_accessible_children().into_iter().find(|c| &c.name == seg) {
                    p.push(next.name.clone());
                    current = next;
                } else {
                    return Err(OverseerError::FormulaError(format!("Path segment not found: {}", seg)));
                }
            }
            let last = segments.last().unwrap();
            // Final segment can be either a parameter on current or a child node's value
            if let Some(val) = Self::get_effective_param(&current.parameters, last) {
                if let OverseerValue::Formula(formula_expr) = val {
                    let child_ctx = EvaluationContext::new_with_current(current, p, context.document_root);
                    return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
                }
                return Ok(val.clone());
            }
            if let Some(child) = current.get_accessible_children().into_iter().find(|c| &c.name == last) {
                if let Some(v) = Self::get_effective_param(&child.parameters, "value") {
                    if let OverseerValue::Formula(formula_expr) = v {
                        let mut cp = p.clone();
                        cp.push(child.name.clone());
                        let child_ctx = EvaluationContext::new_with_current(child, cp, context.document_root);
                        return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
                    }
                    return Ok(v.clone());
                }
                if let Some(OverseerValue::Formula(formula_expr)) = child.parameters.get("value") {
                    let mut cp = p.clone();
                    cp.push(child.name.clone());
                    let child_ctx = EvaluationContext::new_with_current(child, cp, context.document_root);
                    return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
                }
            }
            // Fallback: deep search within found_start subtree
            if let Some(v) = FormulaEvaluator::find_value_by_name_deep(found_start, last) { return Ok(v); }
            // NEW Fallback: search upwards across ancestors for a descendant with this name
            let mut up_end = context.node_path.len();
            while up_end > 0 {
                let anc = &context.node_path[..up_end];
                if let Some(node) = Self::resolve_path_to_node(anc, context.document_root) {
                    if let Some(v) = FormulaEvaluator::find_value_by_name_deep(node, last) { return Ok(v); }
                }
                up_end -= 1;
            }
            return Err(OverseerError::FormulaError(format!("Unknown field '{}' at target path", last)));
        }

        // Case A: identifier-based relative path (e.g., GrandChild/Inner/x)
        if path[0] != ".." {
            // If first segment is a bound variable naming a node, resolve relative to it
            if let Some(BoundValue::Node(start)) = context.var_bindings.get(&path[0]) {
                // Traverse remaining segments from the bound node
                if path.len() == 1 {
                    // Just the variable name; return its value param if present
                    if let Some(v) = Self::get_effective_param(&start.parameters, "value") { return Ok(v.clone()); }
                    return Err(OverseerError::FormulaError(format!("Bound node '{}' has no value", path[0])));
                }
                let mut node = *start;
                let mut traversed_all = true;
                for (i, seg) in path[1..].iter().enumerate() {
                    if let Some(next) = node.get_accessible_children().into_iter().find(|c| &c.name == seg) {
                        node = next;
                    } else {
                        // Allow final segment to be a parameter on current node
                        if i == path.len() - 2 { // since we skipped first, len-2 is last index here
                            if let Some(v) = Self::get_effective_param(&node.parameters, seg) { return Ok(v.clone()); }
                            // Deep-search fallback for single-segment access like x/field
                            if path.len() == 2 {
                                if let Some(v) = FormulaEvaluator::find_value_by_name_deep(*start, seg) { return Ok(v); }
                            }
                        }
                        traversed_all = false;
                        break;
                    }
                }
                if traversed_all {
                    // Prefer node's value
                    if let Some(v) = Self::get_effective_param(&node.parameters, "value") { return Ok(v.clone()); }
                    // Or treat last as parameter on that node
                    if let Some(last) = path.last() {
                        if let Some(v) = Self::get_effective_param(&node.parameters, last) { return Ok(v.clone()); }
                    }
                } else if path.len() == 2 {
                    // If we couldn't traverse and it's a simple x/field shape, deep-search under the bound node
                    let target = &path[1];
                    if let Some(v) = FormulaEvaluator::find_value_by_name_deep(*start, target) { return Ok(v); }
                }
                return Err(OverseerError::FormulaError(format!("Path not found from bound var: {}", path.join("/"))));
            }
            // Try from current node upward until a matching child chain is found
            let mut end = context.node_path.len();
            while end > 0 {
                let anc_path = &context.node_path[..end];
            if let Some(mut node) = FormulaEvaluator::resolve_path_to_node(anc_path, context.document_root) {
                    let mut traversed_all = true;
                    for (i, seg) in path.iter().enumerate() {
                        if let Some(next) = node.get_accessible_children().into_iter().find(|c| &c.name == seg) {
                            node = next;
                        } else {
                            // Allow final segment to be a parameter on current node
                            if i == path.len() - 1 {
                    if let Some(v) = Self::get_effective_param(&node.parameters, seg) {
                                    return Ok(v.clone());
                                }
                            }
                            traversed_all = false;
                            break;
                        }
                    }
                    if traversed_all {
                        // If we ended on a node (e.g., x), prefer its value
                if let Some(v) = Self::get_effective_param(&node.parameters, "value") {
                            return Ok(v.clone());
                        }
                        // Or last segment as parameter on that final node
                        if let Some(last) = path.last() {
                    if let Some(v) = Self::get_effective_param(&node.parameters, last) {
                                return Ok(v.clone());
                            }
                        }
                    }
                }
                end -= 1;
            }
            return Err(OverseerError::FormulaError(format!("Path not found: {}", path.join("/"))));
        }

        // Case B: ../-prefixed paths
        // Only ../-prefixed paths are supported in this branch
        if path[0] != ".." {
            return Err(OverseerError::FormulaError("Path must start with '..'".to_string()));
        }

        // Count how many parent hops ('..') we have
        let mut hops = 1usize;
        let mut idx = 1usize;
    while idx < path.len() && path[idx] == ".." {
            hops += 1;
            idx += 1;
        }
        // Remaining path after hopping up
        let remaining = &path[idx..];
        if remaining.is_empty() {
            return Err(OverseerError::FormulaError("Path missing target after ..".to_string()));
        }

        // Compute ancestor path by removing `hops` segments from the end of node_path
        if context.node_path.len() < hops {
            return Err(OverseerError::FormulaError("Path climbs above root".to_string()));
        }
        let ancestor_segments = &context.node_path[..context.node_path.len() - hops];
        let ancestor = Self::resolve_path_to_node(ancestor_segments, context.document_root)
            .ok_or_else(|| OverseerError::FormulaError("Ancestor not found".to_string()))?;

        // Resolve remaining path against ancestor
        let mut current = ancestor;
        for seg in &remaining[..remaining.len().saturating_sub(1)] {
            // Navigate through named children
            if let Some(next) = current.get_accessible_children().into_iter().find(|c| &c.name == seg) {
                current = next;
            } else {
                return Err(OverseerError::FormulaError(format!("Path segment not found: {}", seg)));
            }
        }
        // Final segment can be either a parameter key or a child node with value
    let last = remaining.last().unwrap();
        if let Some(val) = Self::get_effective_param(&current.parameters, last) {
            if let OverseerValue::Formula(formula_expr) = val {
                // Evaluate parameter formula on-demand at this node
                let mut p = ancestor_segments.to_vec();
                // include traversed segments to reach 'current'
                if !remaining.is_empty() {
                    p.extend(remaining[..remaining.len()-1].iter().cloned());
                }
                let child_ctx = EvaluationContext::new_with_current(current, p, context.document_root);
                return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
            }
            return Ok(val.clone());
        }
        if let Some(child) = current.get_accessible_children().into_iter().find(|c| &c.name == last) {
            if let Some(v) = Self::get_effective_param(&child.parameters, "value") {
                if let OverseerValue::Formula(formula_expr) = v {
                    let mut p = ancestor_segments.to_vec();
                    // include any traversed segments leading to 'current'
                    if !remaining.is_empty() {
                        p.extend(remaining[..remaining.len()-1].iter().cloned());
                    }
                    p.push(child.name.clone());
                    let child_ctx = EvaluationContext::new_with_current(child, p, context.document_root);
                    return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
                }
                return Ok(v.clone());
            }
            if let Some(OverseerValue::Formula(formula_expr)) = child.parameters.get("value") {
                let mut p = ancestor_segments.to_vec();
                if !remaining.is_empty() {
                    p.extend(remaining[..remaining.len()-1].iter().cloned());
                }
                p.push(child.name.clone());
                let child_ctx = EvaluationContext::new_with_current(child, p, context.document_root);
                return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
            }
        }

        // Fallback: search upwards from ancestor for a descendant with this name
        let mut end = ancestor_segments.len();
        while end > 0 {
            let anc = &context.node_path[..end];
            if let Some(node) = Self::resolve_path_to_node(anc, context.document_root) {
                if let Some(v) = Self::find_value_by_name_deep(node, last) {
                    return Ok(v);
                }
            }
            end -= 1;
        }

    Err(OverseerError::FormulaError(format!("Unknown field '{}' at target path", last)))
    }

    /// Resolve a path reference with parameter extraction like ../field.color
    fn resolve_path_param(
        path: &[String],
        param: &str,
        context: &EvaluationContext,
    ) -> Result<OverseerValue, OverseerError> {
    debug_evaluator!("[EVAL] resolve_path_param {:?}.{} at {:?}", path, param, context.node_path);
        if path.is_empty() {
            return Err(OverseerError::FormulaError("Empty path for param extraction".to_string()));
        }
        // Current-container anchored path: ["/", seg1, seg2, ...] (nearest ancestor containing seg1)
        if path[0] == "/" {
            let segments = &path[1..];
            if segments.is_empty() {
                return Err(OverseerError::FormulaError("Path missing target after / for param".to_string()));
            }
            // Find nearest ancestor containing first segment
            let mut start = context.current_node;
            let mut p = context.node_path.clone();
            let first = &segments[0];
            let mut end = context.node_path.len();
            let mut found = false;
            while end > 0 {
                if let Some(candidate) = Self::resolve_path_to_node(&context.node_path[..end].to_vec(), context.document_root) {
                    if candidate.get_accessible_children().into_iter().any(|c| &c.name == first) {
                        start = candidate; p = context.node_path[..end].to_vec(); found = true; break;
                    }
                }
                end -= 1;
            }
        let mut skip_first = false;
        if !found {
                if let Some(root_match) = context.document_root.iter().find(|n| &n.name == first) {
            start = root_match; p = vec![first.clone()]; skip_first = true;
                    debug_evaluator!("[EVAL] '/' absolute base {:?} for param chosen at root for first seg '{}'", p, first);
                }
            }
            let mut node = start;
        let start_idx = if skip_first { 1 } else { 0 };
        for seg in &segments[start_idx..] {
                if let Some(next) = node.get_accessible_children().into_iter().find(|c| &c.name == seg) { p.push(next.name.clone()); node = next; }
                else { return Err(OverseerError::FormulaError(format!("Path segment not found: {}", seg))); }
            }
            if let Some(v) = Self::get_effective_param(&node.parameters, param) {
                if let OverseerValue::Formula(formula_expr) = v {
                    let child_ctx = EvaluationContext::new_with_current(node, p, context.document_root);
                    return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
                }
                return Ok(Self::value_for_param_extraction(v));
            }
            return Err(OverseerError::FormulaError(format!("Parameter '{}' not found on node", param)));
        }
        // Identifier-based path from nearest ancestor
        if path[0] != ".." {
            // Variable-anchored: start from bound node if available
            if let Some(BoundValue::Node(start)) = context.var_bindings.get(&path[0]) {
                let mut node = *start;
                let mut ok = true;
                for seg in path.iter().skip(1) {
                    if let Some(next) = node.get_accessible_children().into_iter().find(|c| &c.name == seg) {
                        node = next;
                    } else {
                        ok = false; break;
                    }
                }
                if ok {
                    if let Some(v) = Self::get_effective_param(&node.parameters, param) {
                        if let OverseerValue::Formula(formula_expr) = v {
                            let child_ctx = EvaluationContext::new_with_current(node, context.node_path.clone(), context.document_root);
                            return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
                        }
                        return Ok(Self::value_for_param_extraction(v));
                    }
                } else if path.len() == 2 {
                    // Deep-search under the bound node for the single target segment
                    let target = &path[1];
                    if let Some(found_node) = Self::find_node_by_name_deep(*start, target) {
                        if let Some(v) = Self::get_effective_param(&found_node.parameters, param) {
                            if let OverseerValue::Formula(formula_expr) = v {
                                let child_ctx = EvaluationContext::new_with_current(found_node, context.node_path.clone(), context.document_root);
                                return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
                            }
                            return Ok(Self::value_for_param_extraction(v));
                        }
                    }
                }
                return Err(OverseerError::FormulaError(format!("Parameter '{}' not found on node", param)));
            }
            let mut end = context.node_path.len();
            while end > 0 {
                let anc_path = &context.node_path[..end];
                if let Some(mut node) = FormulaEvaluator::resolve_path_to_node(anc_path, context.document_root) {
                    let mut ok = true;
                    for seg in path {
                        if let Some(next) = node.get_accessible_children().into_iter().find(|c| &c.name == seg) {
                            node = next;
                        } else {
                            ok = false; break;
                        }
                    }
                    if ok {
                        if let Some(v) = Self::get_effective_param(&node.parameters, param) {
                            if let OverseerValue::Formula(formula_expr) = v {
                                let mut p = anc_path.to_vec();
                                p.extend(path.iter().cloned());
                                let child_ctx = EvaluationContext::new_with_current(node, p, context.document_root);
                                return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
                            }
                            return Ok(Self::value_for_param_extraction(v));
                        }
                        // If parameter not found, error
                        return Err(OverseerError::FormulaError(format!("Parameter '{}' not found on node", param)));
                    }
                }
                end -= 1;
            }
            return Err(OverseerError::FormulaError(format!("Path not found: {}.{}", path.join("/"), param)));
        }

        // ../ path
        let mut hops = 1usize;
        let mut idx = 1usize;
    while idx < path.len() && path[idx] == ".." { hops += 1; idx += 1; }
        let remaining = &path[idx..];
        if context.node_path.len() < hops {
            return Err(OverseerError::FormulaError("Path climbs above root".to_string()));
        }
        let ancestor_segments = &context.node_path[..context.node_path.len() - hops];
        let mut current = Self::resolve_path_to_node(ancestor_segments, context.document_root)
            .ok_or_else(|| OverseerError::FormulaError("Ancestor not found".to_string()))?;
        for seg in remaining {
            if let Some(next) = current.get_accessible_children().into_iter().find(|c| &c.name == seg) {
                current = next;
            } else {
                return Err(OverseerError::FormulaError(format!("Path segment not found: {}", seg)));
            }
        }
        if let Some(v) = Self::get_effective_param(&current.parameters, param) {
            if let OverseerValue::Formula(formula_expr) = v {
                let mut p = ancestor_segments.to_vec();
                p.extend(remaining.iter().cloned());
                let child_ctx = EvaluationContext::new(p, context.document_root);
                return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
            }
            return Ok(Self::value_for_param_extraction(v));
        }
        Err(OverseerError::FormulaError(format!("Parameter '{}' not found on node", param)))
    }

    /// For parameter extraction, convert complex visual types to String for text fields
    fn value_for_param_extraction(v: &OverseerValue) -> OverseerValue {
        match v {
            OverseerValue::Color(color) => OverseerValue::String(Self::color_to_string(color)),
            OverseerValue::CssSize(size) => OverseerValue::String(Self::css_size_to_string(size)),
            _ => v.clone(),
        }
    }

    fn color_to_string(color: &crate::types::Color) -> String {
        match color {
            crate::types::Color::Hex(hex) => hex.clone(),
            crate::types::Color::Named(name) => name.clone(),
            crate::types::Color::Rgb(r, g, b) => format!("rgb({}, {}, {})", r, g, b),
        }
    }

    fn css_size_to_string(size: &crate::types::CssSize) -> String {
        match size {
            crate::types::CssSize::Pixels(px) => format!("{}px", px),
            crate::types::CssSize::Percentage(p) => format!("{}%", p),
            crate::types::CssSize::Em(em) => format!("{}em", em),
            crate::types::CssSize::Rem(rem) => format!("{}rem", rem),
            crate::types::CssSize::ViewportWidth(vw) => format!("{}vw", vw),
            crate::types::CssSize::ViewportHeight(vh) => format!("{}vh", vh),
            crate::types::CssSize::Auto => "auto".to_string(),
            crate::types::CssSize::FitContent => "fit-content".to_string(),
        }
    }

    /// Apply a binary operator to two values
    fn apply_binary_operator(
        left: &OverseerValue,
        operator: &BinaryOperator,
        right: &OverseerValue,
    ) -> Result<OverseerValue, OverseerError> {
        // Boolean operators handled in evaluate_expression for short-circuit
        if matches!(operator, BinaryOperator::And | BinaryOperator::Or) {
            unreachable!("Boolean ops handled earlier")
        }
        // For comparisons, use rich comparison semantics
        if matches!(
            operator,
            BinaryOperator::Equal
                | BinaryOperator::NotEqual
                | BinaryOperator::LessThan
                | BinaryOperator::LessThanOrEqual
                | BinaryOperator::GreaterThan
                | BinaryOperator::GreaterThanOrEqual
        ) {
            use std::cmp::Ordering;
            let ord = Self::compare_values(left, right)?;
            let res = match operator {
                BinaryOperator::Equal => OverseerValue::Boolean(ord == Ordering::Equal),
                BinaryOperator::NotEqual => OverseerValue::Boolean(ord != Ordering::Equal),
                BinaryOperator::LessThan => OverseerValue::Boolean(ord == Ordering::Less),
                BinaryOperator::LessThanOrEqual => OverseerValue::Boolean(ord == Ordering::Less || ord == Ordering::Equal),
                BinaryOperator::GreaterThan => OverseerValue::Boolean(ord == Ordering::Greater),
                BinaryOperator::GreaterThanOrEqual => OverseerValue::Boolean(ord == Ordering::Greater || ord == Ordering::Equal),
                _ => unreachable!(),
            };
            return Ok(res);
        }

        // For arithmetic, convert both to numbers
        let left_num = Self::value_to_number(left)?;
        let right_num = Self::value_to_number(right)?;

        let result = match operator {
            BinaryOperator::Add => OverseerValue::Float(left_num + right_num),
            BinaryOperator::Subtract => OverseerValue::Float(left_num - right_num),
            BinaryOperator::Multiply => OverseerValue::Float(left_num * right_num),
            BinaryOperator::Divide => {
                if right_num == 0.0 {
                    return Err(OverseerError::FormulaError("Division by zero".to_string()));
                }
                OverseerValue::Float(left_num / right_num)
            }
            BinaryOperator::Equal => OverseerValue::Boolean(left_num == right_num),
            BinaryOperator::NotEqual => OverseerValue::Boolean(left_num != right_num),
            BinaryOperator::LessThan => OverseerValue::Boolean(left_num < right_num),
            BinaryOperator::LessThanOrEqual => OverseerValue::Boolean(left_num <= right_num),
            BinaryOperator::GreaterThan => OverseerValue::Boolean(left_num > right_num),
            BinaryOperator::GreaterThanOrEqual => OverseerValue::Boolean(left_num >= right_num),
            BinaryOperator::And => {
                // Should have been handled earlier; fallback without short-circuit
                let lb = Self::value_to_bool(left)?;
                let rb = Self::value_to_bool(right)?;
                OverseerValue::Boolean(lb && rb)
            }
            BinaryOperator::Or => {
                let lb = Self::value_to_bool(left)?;
                let rb = Self::value_to_bool(right)?;
                OverseerValue::Boolean(lb || rb)
            }
        };

        // For arithmetic results, normalize integers if whole
        Ok(match result {
            OverseerValue::Float(f) => {
                if f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
                    OverseerValue::Integer(f as i64)
                } else {
                    OverseerValue::Float(f)
                }
            }
            other => other,
        })
    }

    /// Compare values (numbers/strings/bools/dates/timestamps) with sensible defaults.
    fn compare_values(a: &OverseerValue, b: &OverseerValue) -> Result<std::cmp::Ordering, OverseerError> {
        use std::cmp::Ordering;
        Ok(match (a, b) {
            (OverseerValue::Integer(x), OverseerValue::Integer(y)) => x.cmp(y),
            (OverseerValue::Float(x), OverseerValue::Float(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
            (OverseerValue::Integer(x), OverseerValue::Float(y)) => (*x as f64).partial_cmp(y).unwrap_or(Ordering::Equal),
            (OverseerValue::Float(x), OverseerValue::Integer(y)) => x.partial_cmp(&(*y as f64)).unwrap_or(Ordering::Equal),
            (OverseerValue::String(x), OverseerValue::String(y)) => x.cmp(y),
            (OverseerValue::Boolean(x), OverseerValue::Boolean(y)) => x.cmp(y),
            (OverseerValue::Date(x), OverseerValue::Date(y)) => x.cmp(y),
            (OverseerValue::Timestamp(x), OverseerValue::Timestamp(y)) => x.cmp(y),
            // Mixed date/timestamp: coerce date to start-of-day timestamp
            (OverseerValue::Date(x), OverseerValue::Timestamp(y)) => Self::date_to_timestamp(x).cmp(y),
            (OverseerValue::Timestamp(x), OverseerValue::Date(y)) => x.cmp(&Self::date_to_timestamp(y)),
            // Fallback to string representations
            _ => Self::value_to_string(a).cmp(&Self::value_to_string(b)),
        })
    }

    fn date_to_timestamp(date: &str) -> String {
        if date.contains('T') { return date.to_string(); }
        format!("{}T00:00:00Z", date)
    }

    /// Convert an OverseerValue to a number for arithmetic/comparisons
    fn value_to_number(value: &OverseerValue) -> Result<f64, OverseerError> {
        match value {
            OverseerValue::Integer(i) => Ok(*i as f64), // i64
            OverseerValue::Float(f) => Ok(*f),
            OverseerValue::String(s) => {
                s.parse::<f64>().map_err(|_| 
                    OverseerError::FormulaError(format!("Cannot convert '{}' to number", s))
                )
            }
            _ => Err(OverseerError::FormulaError("Cannot convert value to number".to_string())),
        }
    }

    /// Convert an OverseerValue to boolean for logical operations
    fn value_to_bool(value: &OverseerValue) -> Result<bool, OverseerError> {
        match value {
            OverseerValue::Boolean(b) => Ok(*b),
            OverseerValue::Integer(i) => Ok(*i != 0),
            OverseerValue::Float(f) => Ok(*f != 0.0),
            OverseerValue::String(s) => {
                let sl = s.to_lowercase();
                if sl == "true" { Ok(true) }
                else if sl == "false" { Ok(false) }
                else if let Ok(n) = s.parse::<f64>() { Ok(n != 0.0) }
                else { Err(OverseerError::FormulaError(format!("Cannot convert '{}' to bool", s))) }
            }
            OverseerValue::Date(d) => Ok(!d.is_empty()),
            OverseerValue::Timestamp(ts) => Ok(!ts.is_empty()),
            _ => Err(OverseerError::FormulaError("Cannot convert value to bool".to_string())),
        }
    }

    /// Evaluate a function call
    fn evaluate_function_call(
        name: &str,
        args: &[FormulaExpression],
        context: &EvaluationContext,
    ) -> Result<OverseerValue, OverseerError> {
        match name {
            "today" => {
                if !args.is_empty() {
                    return Err(OverseerError::FormulaError("today() function takes no arguments".to_string()));
                }
                let today = chrono::Local::now().format("%Y-%m-%d").to_string();
                Ok(OverseerValue::Date(today))
            }
            "now" => {
                if !args.is_empty() {
                    return Err(OverseerError::FormulaError("now() takes no arguments".to_string()));
                }
                let now = chrono::Utc::now().to_rfc3339();
                Ok(OverseerValue::Timestamp(now))
            }
            // days_since(ts): returns whole days between now() and the given timestamp/date/string
            "days_since" => {
                if args.len() != 1 {
                    return Err(OverseerError::FormulaError("days_since(x) takes exactly 1 argument".to_string()));
                }
                let val = Self::evaluate_expression(&args[0], context)?;
                // Helper: parse timestamp or date into chrono::DateTime<Utc>
                fn parse_to_utc(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
                    // Try RFC3339 first
                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
                        return Some(dt.with_timezone(&chrono::Utc));
                    }
                    // Try date-only
                    if let Ok(nd) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
                        let ndt = nd.and_hms_opt(0, 0, 0)?;
                        let dt = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(ndt, chrono::Utc);
                        return Some(dt);
                    }
                    None
                }
                let ts_opt: Option<chrono::DateTime<chrono::Utc>> = match val {
                    OverseerValue::Timestamp(ref s) => parse_to_utc(s),
                    OverseerValue::Date(ref d) => parse_to_utc(d),
                    OverseerValue::String(ref s) => parse_to_utc(s),
                    _ => None,
                };
                if let Some(ts) = ts_opt {
                    let now = chrono::Utc::now();
                    let dur = now.signed_duration_since(ts);
                    let days = dur.num_days();
                    Ok(OverseerValue::Integer(days))
                } else {
                    Err(OverseerError::FormulaError("days_since: unable to parse timestamp/date".to_string()))
                }
            }
            _ => Err(OverseerError::FormulaError(format!("Unknown function: {}", name))),
        }
    }
}

impl FormulaEvaluator {
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

impl<'a> EvaluationContext<'a> {
    /// Construct a basic context from the document root and a logical path.
    /// Note: current_node defaults to the first root node; parent_node is None.
    /// Resolver sets the correct current/parent when recursing.
    pub fn new(node_path: Vec<String>, document_root: &'a [OverseerNode]) -> Self {
        let current = document_root
            .first()
            .expect("document_root must not be empty when building EvaluationContext");
        // Resolve current node by following node_path from root when possible
        let resolved_current = FormulaEvaluator::resolve_path_to_node(&node_path, document_root)
            .unwrap_or(current);
        Self {
            current_node: resolved_current,
            parent_node: None,
            document_root,
            node_path,
            var_bindings: std::collections::HashMap::new(),
        }
    }

    /// Construct a context using a direct reference to the current node.
    /// This avoids ambiguity when there are duplicate names under the same parent (e.g., template instances in lists).
    pub fn new_with_current(current_node: &'a OverseerNode, node_path: Vec<String>, document_root: &'a [OverseerNode]) -> Self {
        Self {
            current_node,
            parent_node: None,
            document_root,
            node_path,
            var_bindings: std::collections::HashMap::new(),
        }
    }

    /// Construct a context with explicit parent reference.
    pub fn new_with_current_and_parent(current_node: &'a OverseerNode, parent_node: Option<&'a OverseerNode>, node_path: Vec<String>, document_root: &'a [OverseerNode]) -> Self {
        Self {
            current_node,
            parent_node,
            document_root,
            node_path,
            var_bindings: std::collections::HashMap::new(),
        }
    }

    /// Clone with added variable binding
    pub fn with_var(&self, name: &str, value: BoundValue<'a>) -> Self {
        let mut vars = self.var_bindings.clone();
        vars.insert(name.to_string(), value);
        Self {
            current_node: self.current_node,
            parent_node: self.parent_node,
            document_root: self.document_root,
            node_path: self.node_path.clone(),
            var_bindings: vars,
        }
    }
}

impl FormulaEvaluator {
    /// Helper: resolve a path like [root, child, grandchild] to a node reference in document_root
    fn resolve_path_to_node<'a>(segments: &[String], root: &'a [OverseerNode]) -> Option<&'a OverseerNode> {
        if segments.is_empty() {
            return None;
        }
        // First segment maps to a top-level node name
        let mut current = root.iter().find(|n| n.name == segments[0])?;
        for seg in &segments[1..] {
            // Use accessible children to honor transparency
            let children = current.get_accessible_children();
            if let Some(next) = children.into_iter().find(|c| c.name == *seg) {
                current = next;
            } else {
                return None;
            }
        }
        Some(current)
    }

    fn find_value_by_name_deep(node: &OverseerNode, name: &str) -> Option<OverseerValue> {
        for child in &node.children {
            if child.name == name {
                if let Some(v) = Self::get_effective_param(&child.parameters, "value") {
                    return Some(v.clone());
                }
            }
            if let Some(v) = Self::find_value_by_name_deep(child, name) {
                return Some(v);
            }
        }
        None
    }

    fn find_node_by_name<'a>(roots: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
        for n in roots {
            if n.name == name {
                return Some(n);
            }
            if let Some(found) = Self::find_node_by_name(&n.children, name) {
                return Some(found);
            }
        }
        None
    }

    fn find_node_by_name_deep<'a>(node: &'a OverseerNode, name: &str) -> Option<&'a OverseerNode> {
        for child in &node.children {
            if child.name == name {
                return Some(child);
            }
            if let Some(found) = Self::find_node_by_name_deep(child, name) {
                return Some(found);
            }
        }
        None
    }
}

// Parser combinators for formula expressions

/// Parse a complete expression with operator precedence
fn expression(input: &str) -> IResult<&str, FormulaExpression> {
    // Highest level: ternary operator
    ternary_expression(input)
}

/// Ternary operator: condition ? then : else
fn ternary_expression(input: &str) -> IResult<&str, FormulaExpression> {
    use nom::combinator::opt as nopt;
    let (input, cond) = logical_or_expression(input)?;
    let (input, maybe) = nopt(tuple((
        delimited(multispace0, char('?'), multispace0),
        // then branch can be any expression
        ternary_expression,
        delimited(multispace0, char(':'), multispace0),
        ternary_expression,
    )))(input)?;
    if let Some((_, then_e, _, else_e)) = maybe {
        Ok((input, FormulaExpression::Conditional {
            condition: Box::new(cond),
            then_branch: Box::new(then_e),
            else_branch: Box::new(else_e),
        }))
    } else {
        Ok((input, cond))
    }
}

/// Logical OR (||) with short-circuit semantics at evaluation time
fn logical_or_expression(input: &str) -> IResult<&str, FormulaExpression> {
    let (input, first) = logical_and_expression(input)?;
    let (input, ops) = many0(pair(
        delimited(multispace0, tag("||"), multispace0),
        logical_and_expression,
    ))(input)?;
    Ok((input, ops.into_iter().fold(first, |acc, (_, expr)| {
        FormulaExpression::BinaryOp {
            left: Box::new(acc),
            operator: BinaryOperator::Or,
            right: Box::new(expr),
        }
    })))
}

/// Logical AND (&&)
fn logical_and_expression(input: &str) -> IResult<&str, FormulaExpression> {
    let (input, first) = comparison_expression(input)?;
    let (input, ops) = many0(pair(
        delimited(multispace0, tag("&&"), multispace0),
        comparison_expression,
    ))(input)?;
    Ok((input, ops.into_iter().fold(first, |acc, (_, expr)| {
        FormulaExpression::BinaryOp {
            left: Box::new(acc),
            operator: BinaryOperator::And,
            right: Box::new(expr),
        }
    })))
}

/// New: comparison layer (==, !=, <, <=, >, >=) with lower precedence than +,-
fn comparison_expression(input: &str) -> IResult<&str, FormulaExpression> {
    let (input, first) = additive_expression(input)?;
    let (input, ops) = many0(pair(
        delimited(
            multispace0,
            alt((
                tag("=="),
                tag("!="),
                tag("<="),
                tag(">="),
                tag("<"),
                tag(">"),
            )),
            multispace0,
        ),
        additive_expression,
    ))(input)?;

    Ok((input, ops.into_iter().fold(first, |acc, (op, expr)| {
        let operator = match op {
            "==" => BinaryOperator::Equal,
            "!=" => BinaryOperator::NotEqual,
            "<" => BinaryOperator::LessThan,
            "<=" => BinaryOperator::LessThanOrEqual,
            ">" => BinaryOperator::GreaterThan,
            ">=" => BinaryOperator::GreaterThanOrEqual,
            _ => unreachable!(),
        };
        FormulaExpression::BinaryOp {
            left: Box::new(acc),
            operator,
            right: Box::new(expr),
        }
    })))
}

/// Parse addition and subtraction (lowest precedence)
fn additive_expression(input: &str) -> IResult<&str, FormulaExpression> {
    let (input, first) = multiplicative_expression(input)?;
    let (input, operations) = many0(pair(
        delimited(multispace0, alt((char('+'), char('-'))), multispace0),
        multiplicative_expression,
    ))(input)?;

    Ok((input, operations.into_iter().fold(first, |acc, (op, expr)| {
        let operator = match op {
            '+' => BinaryOperator::Add,
            '-' => BinaryOperator::Subtract,
            _ => unreachable!(),
        };
        FormulaExpression::BinaryOp {
            left: Box::new(acc),
            operator,
            right: Box::new(expr),
        }
    })))
}

/// Parse multiplication and division (higher precedence)
fn multiplicative_expression(input: &str) -> IResult<&str, FormulaExpression> {
    let (input, first) = unary_expression(input)?;
    let (input, operations) = many0(pair(
        delimited(multispace0, alt((char('*'), char('/'))), multispace0),
        unary_expression,
    ))(input)?;

    Ok((input, operations.into_iter().fold(first, |acc, (op, expr)| {
        let operator = match op {
            '*' => BinaryOperator::Multiply,
            '/' => BinaryOperator::Divide,
            _ => unreachable!(),
        };
        FormulaExpression::BinaryOp {
            left: Box::new(acc),
            operator,
            right: Box::new(expr),
        }
    })))
}

/// Parse unary expressions like !expr
fn unary_expression(input: &str) -> IResult<&str, FormulaExpression> {
    let (input, bangs) = many0(delimited(multispace0, char('!'), multispace0))(input)?;
    let (input, mut expr) = primary_expression(input)?;
    // Parse zero or more method calls for chaining
    let (input, calls) = many0(method_call)(input)?;
    if !calls.is_empty() {
        expr = FormulaExpression::MethodChain { base: Box::new(expr), calls };
    }
    // Optionally parse a parameter suffix like .color AFTER method calls
    let (input, param_opt) = opt(preceded(char('.'), take_while1(|c: char| c.is_alphanumeric() || c == '_' || c == '-')))(input)?;
    if let Some(param) = param_opt {
        let param = param.to_string();
        expr = match expr {
            FormulaExpression::FieldReference(name) => FormulaExpression::PathParam { path: vec![name], param },
            FormulaExpression::PathReference(path) => FormulaExpression::PathParam { path, param },
            other => other,
        };
    }
    // Parse zero or more path tail segments like /child/grand after a method chain or any expr
    let (input, tail_segments) = many0(preceded(char('/'), take_while1(|c: char| c.is_alphanumeric() || c == '_' || c == '-')))(input)?;
    if !tail_segments.is_empty() {
        let segs = tail_segments.into_iter().map(|s| s.to_string()).collect::<Vec<_>>();
        expr = FormulaExpression::PathFollow { base: Box::new(expr), segments: segs };
    }
    for _ in 0..bangs.len() {
        expr = FormulaExpression::UnaryOp { operator: UnaryOperator::Not, expr: Box::new(expr) };
    }
    Ok((input, expr))
}

/// Parse primary expressions (numbers, identifiers, function calls, parentheses)
fn primary_expression(input: &str) -> IResult<&str, FormulaExpression> {
    // Parse an atom first
    let (input, atom) = delimited(
        multispace0,
        alt((
            lambda_literal,
            parenthesized_expression,
            function_call,
            string_literal,
            current_path_reference,
            relative_path_reference,
            path_reference,
            number,
            field_reference,
        )),
        multispace0,
    )(input)?;
    Ok((input, atom))
}

/// Parse lambda literals: |x| expr or |acc, x| expr
fn lambda_literal(input: &str) -> IResult<&str, FormulaExpression> {
    use nom::multi::separated_list1;
    let (input, _) = char('|')(input)?;
    let (input, params) = separated_list1(
        delimited(multispace0, char(','), multispace0),
        map(take_while1(|c: char| c.is_alphanumeric() || c == '_'), |s: &str| s.to_string()),
    )(input)?;
    let (input, _) = char('|')(input)?;
    let (input, body) = expression(input)?;
    Ok((input, FormulaExpression::Lambda { params, body: Box::new(body) }))
}

/// Parse a method call: .name(args?)
fn method_call(input: &str) -> IResult<&str, MethodCall> {
    use nom::multi::separated_list0;
    let (input, _) = delimited(multispace0, char('.'), multispace0)(input)?;
    let (input, name) = take_while1(|c: char| c.is_alphabetic() || c == '_')(input)?;
    // parse arguments within parentheses (optional content)
    let (input, args_opt) = delimited(
        delimited(multispace0, char('('), multispace0),
        opt(separated_list0(
            delimited(multispace0, char(','), multispace0),
            expression,
        )),
        delimited(multispace0, char(')'), multispace0),
    )(input)?;
    let args = args_opt.unwrap_or_else(|| vec![]);
    Ok((input, MethodCall { name: name.to_string(), args }))
}

/// Parse string literals: quoted strings or #HEX-like values used for colors
fn string_literal(input: &str) -> IResult<&str, FormulaExpression> {
    // Quoted string
    let quoted = map(
        delimited(char('"'), take_while(|c| c != '"'), char('"')),
        |s: &str| FormulaExpression::StringLiteral(s.to_string()),
    );
    // #hex literal (letters/numbers)
    let hexish = map(
        preceded(char('#'), take_while1(|c: char| c.is_ascii_hexdigit())),
        |s: &str| FormulaExpression::StringLiteral(format!("#{s}")),
    );
    alt((quoted, hexish))(input)
}

/// Parse parenthesized expressions
fn parenthesized_expression(input: &str) -> IResult<&str, FormulaExpression> {
    delimited(char('('), expression, char(')'))(input)
}

/// Parse numbers (integers and floats)
fn number(input: &str) -> IResult<&str, FormulaExpression> {
    map(double, FormulaExpression::Number)(input)
}

/// Parse field references (simple identifiers)
fn field_reference(input: &str) -> IResult<&str, FormulaExpression> {
    map(
        take_while1(|c: char| c.is_alphanumeric() || c == '_'),
        |s: &str| FormulaExpression::FieldReference(s.to_string()),
    )(input)
}

/// Parse path references like ../field or ../../field
fn path_reference(input: &str) -> IResult<&str, FormulaExpression> {
    map(
        pair(
            tag(".."),
            many0(preceded(
                char('/'),
                alt((
                    tag(".."),
                    take_while1(|c: char| c.is_alphanumeric() || c == '_' || c == '-'),
                )),
            )),
        ),
        |(_, parts): (&str, Vec<&str>)| {
            let mut path = vec!["..".to_string()];
            path.extend(parts.into_iter().map(|s| s.to_string()));
            FormulaExpression::PathReference(path)
        },
    )(input)
}

/// Parse current-node anchored paths like /child or /child/grandchild
fn current_path_reference(input: &str) -> IResult<&str, FormulaExpression> {
    use nom::multi::separated_list1;
    let (input, _) = char('/') (input)?;
    let (input, segments) = separated_list1(
        char('/'),
    take_while1(|c: char| c.is_alphanumeric() || c == '_' || c == '-'),
    )(input)?;
    if segments.is_empty() {
        return Err(nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::SeparatedList)));
    }
    let mut path = vec!["/".to_string()];
    path.extend(segments.iter().map(|s| s.to_string()));
    Ok((input, FormulaExpression::PathReference(path)))
}

/// Parse identifier-based relative paths like GrandChild/Inner/x (no whitespace around '/')
fn relative_path_reference(input: &str) -> IResult<&str, FormulaExpression> {
    use nom::multi::separated_list1;
    let (input, segments) = separated_list1(
        char('/'),
        take_while1(|c: char| c.is_alphanumeric() || c == '_' || c == '-'),
    )(input)?;
    // Ensure first segment starts with alpha or underscore to avoid consuming numbers
    if segments.is_empty() || !segments[0].chars().next().unwrap_or('0').is_alphabetic() && segments[0].chars().next().unwrap_or('_') != '_' {
        return Err(nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Alpha)));
    }
    // Require at least two segments (must contain a slash)
    if segments.len() < 2 {
        return Err(nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::SeparatedList)));
    }
    Ok((input, FormulaExpression::PathReference(segments.iter().map(|s| s.to_string()).collect())))
}

// ================= Method chain evaluation helpers =================

#[derive(Debug, Clone)]
enum ListItem<'a> {
    Node(&'a OverseerNode),
    Value(OverseerValue),
}

impl FormulaEvaluator {
    fn evaluate_method_chain(
        base: &FormulaExpression,
        calls: &[MethodCall],
        context: &EvaluationContext,
    ) -> Result<OverseerValue, OverseerError> {
    debug_evaluator!("[EVAL] evaluate_method_chain base {:?} calls {:?}", base, calls);
    let base_node = match Self::eval_expr_to_node(base, context) {
            Some(n) => n,
            None => return Ok(OverseerValue::String("null".to_string())),
        };
    let mut list: Vec<ListItem> = base_node.get_accessible_children().into_iter().map(|n| ListItem::Node(n)).collect();
    debug_evaluator!("[EVAL] Initial list size: {}", list.len());

        for call in calls {
            match call.name.as_str() {
                "map" => {
                    let lambda = call.args.get(0).ok_or_else(|| OverseerError::FormulaError("map() requires 1 argument".to_string()))?;
                    let mut out: Vec<ListItem> = Vec::with_capacity(list.len());
                    for item in list.into_iter() {
                        let v_res = match &item {
                            ListItem::Node(n) => Self::eval_lambda(lambda, None, Some(*n), None, context),
                            ListItem::Value(val) => Self::eval_lambda(lambda, None, None, Some(val.clone()), context),
                        };
                        match v_res {
                            Ok(v) => out.push(ListItem::Value(v)),
                            Err(_) => out.push(ListItem::Value(OverseerValue::String("null".to_string()))),
                        }
                    }
                    list = out;
                    debug_evaluator!("[EVAL] After map: list size {}", list.len());
                }
                "find" => {
                    // find(keyValue) — requires current list base to be a container with parameter 'key'
                    let key_val_expr = call.args.get(0).ok_or_else(|| OverseerError::FormulaError("find() requires 1 argument".to_string()))?;
                    let target_key_value = Self::evaluate_expression(key_val_expr, context)?;
                    // Determine key field name from base container
                    let key_field = if let Some(base_node) = Self::eval_expr_to_node(base, context) {
                        if let Some(OverseerValue::String(k)) = base_node.parameters.get("key") { k.clone() } else { return Err(OverseerError::ValidationError("find() base does not declare a key parameter".to_string())); }
                    } else { return Err(OverseerError::ValidationError("find() base must be a node (list/container)".to_string())); };
                    // Scan list for the first item whose child named key_field has value == target_key_value
                    let mut found: Option<ListItem> = None;
                    for item in list.into_iter() {
                        match &item {
                            ListItem::Node(n) => {
                                if let Some(child) = n.get_accessible_children().into_iter().find(|c| c.name == key_field) {
                                    if let Some(v) = Self::get_effective_param(&child.parameters, "value") {
                                        // Relaxed equality: use compare_values to allow int<->string numeric equality, etc.
                                        if Self::compare_values(v, &target_key_value).map(|ord| ord == std::cmp::Ordering::Equal).unwrap_or(false) {
                                            found = Some(ListItem::Node(n));
                                            break;
                                        }
                                    }
                                }
                            }
                            ListItem::Value(_) => { /* skip values for find over nodes */ }
                        }
                    }
                    list = match found {
                        Some(x) => vec![x],
                        None => vec![],
                    };
                    debug_evaluator!("[EVAL] find => {}", list.len());
                }
                "filter" => {
                    let lambda = call.args.get(0).ok_or_else(|| OverseerError::FormulaError("filter() requires 1 argument".to_string()))?;
                    let mut out: Vec<ListItem> = Vec::new();
                    for item in list.into_iter() {
                        let keep_val = match &item {
                            ListItem::Node(n) => Self::eval_lambda(lambda, None, Some(*n), None, context)?,
                            ListItem::Value(val) => Self::eval_lambda(lambda, None, None, Some(val.clone()), context)?,
                        };
                        if Self::value_to_bool(&keep_val).unwrap_or(false) { out.push(item); }
                    }
                    list = out;
                    debug_evaluator!("[EVAL] After filter: list size {}", list.len());
                }
                "reduce" => {
                    if call.args.len() != 2 { return Err(OverseerError::FormulaError("reduce(init, lambda) requires 2 args".to_string())); }
                    let init = Self::evaluate_expression(&call.args[0], context)?;
                    let lambda = &call.args[1];
                    let mut acc = init;
                    for item in list.into_iter() {
                        acc = match item {
                            ListItem::Node(n) => Self::eval_lambda(lambda, Some(acc), Some(n), None, context)?,
                            ListItem::Value(v) => Self::eval_lambda(lambda, Some(acc), None, Some(v), context)?,
                        };
                    }
                    debug_evaluator!("[EVAL] After reduce: {:?}", acc);
                    return Ok(acc);
                }
                "sum" => {
                    let mut total: f64 = 0.0;
                    for item in &list { if let Some(num) = Self::item_to_number(item) { total += num; } }
                    let res = Self::normalize_number(total);
                    debug_evaluator!("[EVAL] sum => {:?}", res);
                    return Ok(res);
                }
                "count" => {
                    let res = OverseerValue::Integer(list.len() as i64);
                    debug_evaluator!("[EVAL] count => {:?}", res);
                    return Ok(res);
                }
                "max" => {
                    // Prefer generic comparison using compare_values so timestamps/dates/strings work.
                    // If items are numeric-only, result will still be numeric.
                    let mut best: Option<OverseerValue> = None;
                    for item in &list {
                        if let Some(v) = Self::item_to_value(item) {
                            best = Some(match best {
                                None => v,
                                Some(cur) => {
                                    match Self::compare_values(&v, &cur) {
                                        Ok(std::cmp::Ordering::Greater) => v,
                                        _ => cur,
                                    }
                                }
                            });
                        }
                    }
                    let res = best.unwrap_or_else(|| OverseerValue::String("null".to_string()));
                    debug_evaluator!("[EVAL] max => {:?}", res);
                    return Ok(res);
                }
                "min" => {
                    let mut best: Option<OverseerValue> = None;
                    for item in &list {
                        if let Some(v) = Self::item_to_value(item) {
                            best = Some(match best {
                                None => v,
                                Some(cur) => {
                                    match Self::compare_values(&v, &cur) {
                                        Ok(std::cmp::Ordering::Less) => v,
                                        _ => cur,
                                    }
                                }
                            });
                        }
                    }
                    let res = best.unwrap_or_else(|| OverseerValue::String("null".to_string()));
                    debug_evaluator!("[EVAL] min => {:?}", res);
                    return Ok(res);
                }
                "avg" => {
                    let mut sum: f64 = 0.0; let mut cnt: usize = 0;
                    for item in &list { if let Some(num) = Self::item_to_number(item) { sum += num; cnt += 1; } }
                    let res = if cnt == 0 { OverseerValue::String("null".to_string()) } else { OverseerValue::Float(sum / cnt as f64) };
                    debug_evaluator!("[EVAL] avg => {:?}", res);
                    return Ok(res);
                }
                "first" => {
                    // Returns the first item in the list as a value; if Node, returns its effective value
                    // Optional arg first(default) to return default when list is empty
                    let default = if let Some(arg0) = call.args.get(0) { Some(Self::evaluate_expression(arg0, context)?) } else { None };
                    if let Some(item) = list.first() {
                        let v = match item {
                            ListItem::Value(v) => v.clone(),
                            ListItem::Node(n) => {
                                if let Some(v) = Self::get_effective_param(&n.parameters, "value") { v.clone() } else { OverseerValue::String("null".to_string()) }
                            }
                        };
                        debug_evaluator!("[EVAL] first => {:?}", v);
                        return Ok(v);
                    }
                    let out = default.unwrap_or_else(|| OverseerValue::String("null".to_string()));
                    debug_evaluator!("[EVAL] first(empty) => {:?}", out);
                    return Ok(out);
                }
                other => return Err(OverseerError::FormulaError(format!("Unknown method: {}", other))),
            }
        }

        Err(OverseerError::FormulaError("Method chain must end with an aggregate or reduce".to_string()))
    }

    fn item_to_number(item: &ListItem) -> Option<f64> {
        match item {
            ListItem::Value(v) => Self::value_to_number(v).ok(),
            ListItem::Node(n) => {
                if let Some(v) = Self::get_effective_param(&n.parameters, "value") { Self::value_to_number(v).ok() } else { None }
            }
        }
    }

    /// Extract a value suitable for generic comparison from a list item.
    /// - For Value items, use as-is.
    /// - For Node items, use the effective "value" parameter if present; otherwise None.
    fn item_to_value(item: &ListItem) -> Option<OverseerValue> {
        match item {
            ListItem::Value(v) => Some(v.clone()),
            ListItem::Node(n) => {
                if let Some(v) = Self::get_effective_param(&n.parameters, "value") { Some(v.clone()) } else { None }
            }
        }
    }

    fn normalize_number(n: f64) -> OverseerValue {
        if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 { OverseerValue::Integer(n as i64) } else { OverseerValue::Float(n) }
    }

    fn eval_expr_to_node<'a>(expr: &FormulaExpression, context: &'a EvaluationContext) -> Option<&'a OverseerNode> {
        match expr {
            FormulaExpression::PathReference(path) => Self::resolve_path_to_node_any(path, context),
            FormulaExpression::FieldReference(name) => {
                if let Some(BoundValue::Node(n)) = context.var_bindings.get(name) { return Some(*n); }
                // First, try child of the current node (rare but valid)
                if let Some(child) = context.current_node.get_accessible_children().into_iter().find(|c| &c.name == name) {
                    return Some(child);
                }
                // Next, try immediate parent for siblings
                if let Some(parent) = context.parent_node {
                    if let Some(sib) = parent.get_accessible_children().into_iter().find(|c| &c.name == name) {
                        return Some(sib);
                    }
                }
                // Then, walk ancestors using node_path in the document snapshot and search each ancestor's children
                if !context.node_path.is_empty() {
                    // Start from the parent of current and go up to root
                    let mut end = context.node_path.len();
                    while end > 0 {
                        let parent_end = end - 1;
                        if parent_end == 0 {
                            // root has no parent to search siblings under; but we can still check root's children directly
                            if let Some(root_node) = FormulaEvaluator::resolve_path_to_node(&context.node_path[..end], context.document_root) {
                                if let Some(found) = root_node.get_accessible_children().into_iter().find(|c| &c.name == name) { return Some(found); }
                            }
                            break;
                        }
                        if let Some(parent) = FormulaEvaluator::resolve_path_to_node(&context.node_path[..parent_end], context.document_root) {
                            if let Some(sib) = parent.get_accessible_children().into_iter().find(|c| &c.name == name) {
                                return Some(sib);
                            }
                        }
                        end -= 1;
                    }
                }
                None
            }
            FormulaExpression::MethodChain { base, calls } => {
                // Resolve base container node
                let base_node = Self::eval_expr_to_node(base, context)?;
                // Start from base's accessible children
                let mut nodes: Vec<&OverseerNode> = base_node.get_accessible_children();
                let mut current_container = base_node;
                for call in calls {
                    match call.name.as_str() {
                        "find" => {
                            // Evaluate key value to match
                            let key_val_expr = call.args.get(0)?;
                            let target = Self::evaluate_expression(key_val_expr, context).ok()?;
                            // Determine key field name from the base container (or most recent container)
                            let key_field = if let Some(OverseerValue::String(k)) = current_container.parameters.get("key") { k } else { return None; };
                            // Find first child whose key_field child value == target
                            let mut found: Option<&OverseerNode> = None;
                            for n in nodes.into_iter() {
                                if let Some(ch) = n.get_accessible_children().into_iter().find(|c| &c.name == key_field) {
                                    if let Some(v) = Self::get_effective_param(&ch.parameters, "value") {
                                        // Relaxed equality using compare_values
                                        if Self::compare_values(v, &target).map(|ord| ord == std::cmp::Ordering::Equal).unwrap_or(false) { found = Some(n); break; }
                                    }
                                }
                            }
                            if let Some(f) = found {
                                // After find, treat result as a singleton list and container becomes the found node
                                nodes = vec![f];
                                current_container = f;
                            } else {
                                nodes = vec![]; // no match
                            }
                        }
                        _ => {
                            // Unsupported for node resolution; bail
                            return None;
                        }
                    }
                }
                // If chain narrowed to a single node, return it
                if nodes.len() == 1 { Some(nodes[0]) } else { None }
            }
            _ => None,
        }
    }

    fn resolve_path_to_node_any<'a>(path: &[String], context: &'a EvaluationContext) -> Option<&'a OverseerNode> {
        if path.is_empty() { return None; }
        if path[0] == "/" {
            // Find nearest ancestor containing first segment
        let segments = &path[1..];
            if segments.is_empty() { return None; }
            let mut start = context.current_node;
            let mut p_end = context.node_path.len();
            let mut found = false;
            while p_end > 0 {
                if let Some(candidate) = Self::resolve_path_to_node(&context.node_path[..p_end].to_vec(), context.document_root) {
                    if candidate.get_accessible_children().into_iter().any(|c| c.name == segments[0]) {
                        start = candidate; found = true; break;
                    }
                }
                p_end -= 1;
            }
        let mut skip_first = false;
        if !found {
                if let Some(root_match) = context.document_root.iter().find(|n| n.name == segments[0]) {
            start = root_match; skip_first = true;
                }
            }
            let mut current = start;
        let start_idx = if skip_first { 1 } else { 0 };
        for seg in &segments[start_idx..] {
                current = current.get_accessible_children().into_iter().find(|c| &c.name == seg)?;
            }
            return Some(current);
        }
        if path[0] == ".." {
            let mut hops = 1usize; let mut idx = 1usize; while idx < path.len() && path[idx] == ".." { hops += 1; idx += 1; }
            if context.node_path.len() < hops { return None; }
            let ancestor_segments = &context.node_path[..context.node_path.len()-hops];
            let mut current = Self::resolve_path_to_node(ancestor_segments, context.document_root)?;
            for seg in &path[idx..] {
                current = current.get_accessible_children().into_iter().find(|c| &c.name == seg)?;
            }
            return Some(current);
        }
        // Identifier-based starting at a bound variable node, if present
        if let Some(BoundValue::Node(start)) = context.var_bindings.get(&path[0]) {
            let mut current = *start;
            for seg in &path[1..] {
                current = current.get_accessible_children().into_iter().find(|c| &c.name == seg)?;
            }
            return Some(current);
        }
        // Fallback: search from nearest ancestor following child chain
        let mut end = context.node_path.len();
        while end > 0 {
            if let Some(mut node) = Self::resolve_path_to_node(&context.node_path[..end], context.document_root) {
                let mut ok = true;
                for seg in path {
                    if let Some(next) = node.get_accessible_children().into_iter().find(|c| &c.name == seg) { node = next; } else { ok = false; break; }
                }
                if ok { return Some(node); }
            }
            end -= 1;
        }
        None
    }

    fn eval_lambda(
        lambda_expr: &FormulaExpression,
        acc_value: Option<OverseerValue>,
        item_node: Option<&OverseerNode>,
        item_value: Option<OverseerValue>,
        context: &EvaluationContext,
    ) -> Result<OverseerValue, OverseerError> {
        match lambda_expr {
            FormulaExpression::Lambda { params, body } => {
                let mut ctx = context.clone();
                if params.len() == 1 {
                    if let Some(n) = item_node { ctx = ctx.with_var(&params[0], BoundValue::Node(n)); }
                    else if let Some(v) = item_value.clone() { ctx = ctx.with_var(&params[0], BoundValue::Value(v)); }
                } else if params.len() == 2 {
                    if let Some(acc) = acc_value.clone() { ctx = ctx.with_var(&params[0], BoundValue::Value(acc)); }
                    if let Some(n) = item_node { ctx = ctx.with_var(&params[1], BoundValue::Node(n)); }
                    else if let Some(v) = item_value.clone() { ctx = ctx.with_var(&params[1], BoundValue::Value(v)); }
                }
                debug_evaluator!("[EVAL] eval_lambda with params {:?}, bindings {:?}", params, ctx.var_bindings.keys().collect::<Vec<_>>());
                Self::evaluate_expression(body, &ctx)
            }
            other => {
                // Allow non-lambda expressions as trivial mappers/predicates with item bound to 'x'
                let mut ctx = context.clone();
                if let Some(n) = item_node { ctx = ctx.with_var("x", BoundValue::Node(n)); }
                if let Some(v) = item_value { ctx = ctx.with_var("x", BoundValue::Value(v)); }
                debug_evaluator!("[EVAL] eval_lambda (implicit) with x bound, expr {:?}", other);
                Self::evaluate_expression(other, &ctx)
            }
        }
    }
}

/// Parse function calls like today() or days_since(arg1, arg2)
fn function_call(input: &str) -> IResult<&str, FormulaExpression> {
    use nom::multi::separated_list0;
    map(
        tuple((
            take_while1(|c: char| c.is_alphabetic() || c == '_'),
            delimited(
                char('('),
                opt(separated_list0(
                    delimited(multispace0, char(','), multispace0),
                    expression,
                )),
                char(')')
            ),
        )),
        |(name, args_opt): (&str, Option<Vec<FormulaExpression>>)| FormulaExpression::FunctionCall {
            name: name.to_string(),
            args: args_opt.unwrap_or_else(|| vec![]),
        },
    )(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_document;
    use crate::resolver::resolve_document;

    #[test]
    fn test_pipeline_map_sum() {
        let input = r#"
        div Root {
            div Items {
                string a = 10
                string b = 20
                string c = 5
            }
            string total = $(Items.map(|x| x/value).sum())
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let root = &nodes[0];
        let total = root.get_accessible_children().into_iter().find(|c| c.name == "total").unwrap();
        let computed = total.parameters.get("_computed_value").cloned().unwrap();
        assert_eq!(computed, OverseerValue::Integer(35));
    }

    #[test]
    fn test_pipeline_filter_map_first_lookup_by_key() {
        let input = r#"
        div Root {
            list Exercises (entry=<Exercise>) {
                - { string id = "a" string description = "Push Ups" }
                - { string id = "b" string description = "Squats" }
            }
            div Record {
                string exercise_id = "b"
                string description = $(/Exercises.filter(|x| x/id == ../exercise_id).map(|x| x/description).first(""))
            }
            div Exercise (hidden=true) {
                string id = ""
                string description = ""
            }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let root = &nodes[0];
        // Navigate to Record/description
        let record = root.get_accessible_children().into_iter().find(|c| c.name == "Record").unwrap();
        let desc = record.get_accessible_children().into_iter().find(|c| c.name == "description").unwrap();
        let computed = desc.parameters.get("_computed_value").cloned().unwrap();
        assert_eq!(computed, OverseerValue::String("Squats".to_string()));
    }

    #[test]
    fn test_find_method_uses_list_key_and_path_follow() {
        let input = r#"
        div Root {
            div Exercise (hidden=true) {
                string id = ""
                string description = ""
            }
            list Exercises (entry=<Exercise>, key="id") {
                - { string id = "a" string description = "Push Ups" }
                - { string id = "b" string description = "Squats" }
            }
            div Record {
                string id = "b"
                string desc = $(/Exercises.find(../id)/description)
            }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let root = &nodes[0];
        let record = root.get_accessible_children().into_iter().find(|c| c.name == "Record").unwrap();
        let desc = record.get_accessible_children().into_iter().find(|c| c.name == "desc").unwrap();
        let computed = desc.parameters.get("_computed_value").cloned().unwrap();
        assert_eq!(computed, OverseerValue::String("Squats".to_string()));
    }

    #[test]
    fn test_find_with_integer_keys_and_string_lookup() {
        let input = r#"
        div Root {
            div Exercise (hidden=true) {
                int id = 0
                string description = ""
            }
            list Exercises (entry=<Exercise>, key="id") {
                - { int id = 1 string description = "One" }
                - { int id = 2 string description = "Two" }
            }
            div Record {
                // Lookup id provided as a string; should match integer id via relaxed equality
                string id = "2"
                string desc = $(/Exercises.find(../id)/description)
            }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let root = &nodes[0];
        let record = root.get_accessible_children().into_iter().find(|c| c.name == "Record").unwrap();
        let desc = record.get_accessible_children().into_iter().find(|c| c.name == "desc").unwrap();
        let computed = desc.parameters.get("_computed_value").cloned().unwrap();
        assert_eq!(computed, OverseerValue::String("Two".to_string()));
    }

    #[test]
    fn test_absolute_path_with_tab_and_find() {
        let input = r#"
        tab exercise_tracker {
            div ExerciseRecord {
                int eid = 2
                string description = $(/exercise_tracker/Exercises.find(eid)/description)
            }
            div (hidden=true) {
                div Exercise {
                    int id = 0
                    string description = ""
                }
            }
            list Exercises (entry=<Exercise>, key="id") {
                - { int id = 1 string description = "A" }
                - { int id = 2 string description = "B" }
            }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let tab = &nodes[0];
        let record = tab.get_accessible_children().into_iter().find(|c| c.name == "ExerciseRecord").unwrap();
        let desc = record.get_accessible_children().into_iter().find(|c| c.name == "description").unwrap();
        let computed = desc.parameters.get("_computed_value").cloned().unwrap();
        assert_eq!(computed, OverseerValue::String("B".to_string()));
    }

    #[test]
    fn test_pipeline_filter_count() {
        let input = r#"
        div Root {
            div Items {
                string a (ok=true) = 1
                string b (ok=false) = 2
                string c (ok=true) = 3
            }
            string ok_count = $(Items.filter(|x| x/ok).count())
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let root = &nodes[0];
        let ok_count = root.get_accessible_children().into_iter().find(|c| c.name == "ok_count").unwrap();
        let computed = ok_count.parameters.get("_computed_value").cloned().unwrap();
        assert_eq!(computed, OverseerValue::Integer(2));
    }

    #[test]
    fn test_pipeline_reduce_sum() {
        let input = r#"
        div Root {
            div Items {
                string a = 4
                string b = 6
            }
            string total = $(Items.reduce(0, |acc, x| acc + x/value))
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let root = &nodes[0];
        let total = root.get_accessible_children().into_iter().find(|c| c.name == "total").unwrap();
        let computed = total.parameters.get("_computed_value").cloned().unwrap();
        assert_eq!(computed, OverseerValue::Integer(10));
    }

    #[test]
    fn test_pipeline_count_only() {
        let input = r#"
        div Root {
            div Items {
                string a = 1
                string b = 2
                string c = 3
            }
            string cnt = $(Items.count())
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let root = &nodes[0];
        let cnt = root.get_accessible_children().into_iter().find(|c| c.name == "cnt").unwrap();
        let computed = cnt.parameters.get("_computed_value").cloned().unwrap();
        assert_eq!(computed, OverseerValue::Integer(3));
    }

    #[test]
    fn test_template_instance_formulas_per_instance() {
        // Verify that formulas defined inside a template (Step) are evaluated per instance
        // after instantiation, so each Step gets its own complete/total task counts.
        let input = r#"
        div Task (hidden=true) {
            string task_description = ""
            checkbox complete = false
        }

        div Step (hidden=true) {
            string name = ""
            list StepTasks (entry=<../Task>)
            int complete_tasks = $(StepTasks.filter(|x| x/complete).count())
            int total_tasks = $(StepTasks.count())
        }

        list Steps (entry=<Step>) {
            - {
                string name = "Step A"
                list StepTasks (entry=<../Task>) {
                    - { string task_description = "A1" checkbox complete = true }
                    - { string task_description = "A2" checkbox complete = false }
                }
            }
            - {
                string name = "Step B"
                list StepTasks (entry=<../Task>) {
                    - { string task_description = "B1" checkbox complete = true }
                    - { string task_description = "B2" checkbox complete = true }
                    - { string task_description = "B3" checkbox complete = false }
                }
            }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);

    // Find the Steps list among top-level nodes
    let steps = nodes.iter().find(|n| n.name == "Steps").expect("Steps list not found at root");
    assert_eq!(steps.node_type, "list");
    assert_eq!(steps.name, "Steps");
        assert_eq!(steps.children.len(), 2);

    // First step instance should have 2 StepTasks
    let step_a = &steps.children[0];
    let a_steptasks = step_a.get_accessible_children().into_iter().find(|c| c.name == "StepTasks").unwrap();
    assert_eq!(a_steptasks.children.len(), 2);
    assert_eq!(a_steptasks.get_accessible_children().len(), 2);
        let a_complete = step_a.get_accessible_children().into_iter().find(|c| c.name == "complete_tasks").unwrap();
        let a_total = step_a.get_accessible_children().into_iter().find(|c| c.name == "total_tasks").unwrap();
        assert_eq!(a_complete.parameters.get("_computed_value"), Some(&OverseerValue::Integer(1)));
        assert_eq!(a_total.parameters.get("_computed_value"), Some(&OverseerValue::Integer(2)));

    // Second step instance should have 3 StepTasks
    let step_b = &steps.children[1];
    let b_steptasks = step_b.get_accessible_children().into_iter().find(|c| c.name == "StepTasks").unwrap();
    assert_eq!(b_steptasks.children.len(), 3);
    assert_eq!(b_steptasks.get_accessible_children().len(), 3);
        let b_complete = step_b.get_accessible_children().into_iter().find(|c| c.name == "complete_tasks").unwrap();
        let b_total = step_b.get_accessible_children().into_iter().find(|c| c.name == "total_tasks").unwrap();
        assert_eq!(b_complete.parameters.get("_computed_value"), Some(&OverseerValue::Integer(2)));
        assert_eq!(b_total.parameters.get("_computed_value"), Some(&OverseerValue::Integer(3)));
    }

    #[test]
    fn test_pipeline_filter_count_with_wrapper_div() {
        // Ensure variable-anchored path x/complete works when 'complete' is nested under a wrapper
        let input = r#"
        div Root {
            list Items {
                - { div card { checkbox complete = true } }
                - { div card { checkbox complete = false } }
                - { div card { checkbox complete = true } }
            }
            int done = $(Items.filter(|x| x/complete).count())
        }
        "#;
        let mut nodes = crate::parser::parse_document(input).unwrap().1;
        crate::resolver::resolve_document(&mut nodes);
        let root = &nodes[0];
        let done = root.get_accessible_children().into_iter().find(|c| c.name == "done").unwrap();
        let computed = done.parameters.get("_computed_value").cloned().unwrap();
        assert_eq!(computed, OverseerValue::Integer(2));
    }

    #[test]
    fn test_latest_timestamp_via_history_filter_map_max() {
        // Verify that max() works on timestamps to compute latest record time for an exercise id
        let input = r#"
        div Root {
            div Exercise (hidden=true) {
                int id = 2
                timestamp last_done = $(/History.filter(|x| x/eid == ../id).map(|x| x/time).max())
            }
            div ExerciseRecord (hidden=true) {
                int eid = 0
                timestamp time = now()
            }
            list History (entry=<ExerciseRecord>) {
                - { int eid = 1 timestamp time = "2025-08-10T19:33:55.706634+00:00" }
                - { int eid = 2 timestamp time = "2025-08-12T09:20:47.374048300+00:00" }
                - { int eid = 2 timestamp time = "2025-08-12T10:53:02.760779800+00:00" }
            }
        }
        "#;
        let mut nodes = crate::parser::parse_document(input).unwrap().1;
        crate::resolver::resolve_document(&mut nodes);
        let root = &nodes[0];
        let ex = root.get_accessible_children().into_iter().find(|c| c.name == "Exercise").unwrap();
        let last_done = ex.get_accessible_children().into_iter().find(|c| c.name == "last_done").unwrap();
    let computed = last_done.parameters.get("_computed_value").cloned().unwrap();
    assert_eq!(computed, OverseerValue::String("2025-08-12T10:53:02.760779800+00:00".to_string()));
    }
}
