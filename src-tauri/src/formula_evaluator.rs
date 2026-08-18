use crate::types::{OverseerError, OverseerNode, OverseerValue};
use chrono::{DateTime, Local, Utc};
use nom::{
    branch::alt,
    bytes::complete::{tag, take_while, take_while1},
    character::complete::{char, multispace0},
    combinator::{map, opt, peek},
    multi::many0,
    number::complete::double,
    sequence::{delimited, pair, preceded, tuple},
    IResult,
};
use std::cell::RefCell;
use std::collections::HashSet;

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
    BooleanLiteral(bool),
    FieldReference(String),
    PathReference(Vec<String>), // e.g., ["..","field"] for ../field
    PathParam {
        path: Vec<String>,
        param: String,
    },
    // Lambda literal and method chaining for list pipelines
    Lambda {
        params: Vec<String>,
        body: Box<FormulaExpression>,
    },
    MethodChain {
        base: Box<FormulaExpression>,
        calls: Vec<MethodCall>,
    },
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
    // Thread-local time override used by time-based functions during evaluation
    thread_local! {
        static TIME_OVERRIDE_UTC: std::cell::RefCell<Option<DateTime<Utc>>> = const { std::cell::RefCell::new(None) };
    }

    // Thread-local evaluation guard to detect cycles; stores keys like "path1/path2|formula"
    thread_local! {
    static EVAL_GUARD: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    }

    pub fn set_time_override(dt: Option<DateTime<Utc>>) {
        Self::TIME_OVERRIDE_UTC.with(|cell| {
            *cell.borrow_mut() = dt;
        });
    }

    pub fn get_time_override() -> Option<DateTime<Utc>> {
        Self::TIME_OVERRIDE_UTC.with(|cell| cell.borrow().clone())
    }

    fn guard_key(path: &[String], formula: &str) -> String {
        let mut s = String::new();
        for (i, seg) in path.iter().enumerate() {
            if i > 0 {
                s.push('/');
            }
            s.push_str(seg);
        }
        s.push('|');
        s.push_str(formula);
        s
    }
    /// Evaluate a list-source expression used for plots, returning the base path and filtered items (nodes).
    /// Supported forms:
    /// - Path only: /A/B or Rel/Path -> returns children of the container
    /// - Method chain with filter/find on a container: /A/B.filter(|x| ...), /A/B.find(key)
    ///   Map/reduce/aggregates are not supported here (return error).
    #[allow(dead_code)]
    pub fn evaluate_list_source_nodes<'a>(
        expr_src: &str,
        context: &'a EvaluationContext,
    ) -> Result<(Vec<String>, Vec<&'a OverseerNode>), OverseerError> {
        let expr = match Self::parse_expression(expr_src) {
            Ok(e) => e,
            Err(_) => {
                return Err(OverseerError::FormulaError(
                    "invalid formula error".to_string(),
                ))
            }
        };

        // Resolve a base expression to (path, node)
        fn resolve_base<'a>(
            base: &FormulaExpression,
            ctx: &'a EvaluationContext,
        ) -> Option<(Vec<String>, &'a OverseerNode)> {
            match base {
                FormulaExpression::PathReference(path) => {
                    // Similar to resolve_path_to_node_any, but track the path vector as we go
                    if path.is_empty() {
                        return None;
                    }
                    if path[0] == "/" {
                        let segments = &path[1..];
                        if segments.is_empty() {
                            return None;
                        }
                        // Find nearest ancestor containing first seg, else treat as absolute root
                        let mut start = ctx.current_node;
                        let mut p: Vec<String> = ctx.node_path.clone();
                        let mut end = ctx.node_path.len();
                        let mut found = false;
                        while end > 0 {
                            if let Some(candidate) = FormulaEvaluator::resolve_path_to_node(
                                &ctx.node_path[..end].to_vec(),
                                ctx.document_root,
                            ) {
                                if candidate
                                    .get_accessible_children()
                                    .into_iter()
                                    .any(|c| &c.name == &segments[0])
                                {
                                    start = candidate;
                                    p = ctx.node_path[..end].to_vec();
                                    found = true;
                                    break;
                                }
                            }
                            end -= 1;
                        }
                        let mut skip_first = false;
                        if !found {
                            if let Some(root_match) =
                                ctx.document_root.iter().find(|n| &n.name == &segments[0])
                            {
                                start = root_match;
                                p = vec![segments[0].clone()];
                                skip_first = true;
                            }
                        }
                        let mut current = start;
                        let start_idx = if skip_first { 1 } else { 0 };
                        for seg in &segments[start_idx..] {
                            if let Some(next) = current
                                .get_accessible_children()
                                .into_iter()
                                .find(|c| &c.name == seg)
                            {
                                p.push(next.name.clone());
                                current = next;
                            } else {
                                return None;
                            }
                        }
                        Some((p, current))
                    } else if path[0] == ".." {
                        // Parent hops
                        let mut hops = 1usize;
                        let mut idx = 1usize;
                        while idx < path.len() && path[idx] == ".." {
                            hops += 1;
                            idx += 1;
                        }
                        if ctx.node_path.len() < hops {
                            return None;
                        }
                        let mut p = ctx.node_path[..ctx.node_path.len() - hops].to_vec();
                        let mut current =
                            FormulaEvaluator::resolve_path_to_node(&p, ctx.document_root)?;
                        for seg in &path[idx..] {
                            if let Some(next) = current
                                .get_accessible_children()
                                .into_iter()
                                .find(|c| &c.name == seg)
                            {
                                p.push(next.name.clone());
                                current = next;
                            } else {
                                return None;
                            }
                        }
                        Some((p, current))
                    } else {
                        // Identifier-based from nearest ancestor
                        let mut end = ctx.node_path.len();
                        while end > 0 {
                            let anc_path = &ctx.node_path[..end];
                            if let Some(mut node) =
                                FormulaEvaluator::resolve_path_to_node(anc_path, ctx.document_root)
                            {
                                let mut p = anc_path.to_vec();
                                let mut ok = true;
                                for seg in path {
                                    if let Some(next) = node
                                        .get_accessible_children()
                                        .into_iter()
                                        .find(|c| &c.name == seg)
                                    {
                                        p.push(next.name.clone());
                                        node = next;
                                    } else {
                                        ok = false;
                                        break;
                                    }
                                }
                                if ok {
                                    return Some((p, node));
                                }
                            }
                            end -= 1;
                        }
                        None
                    }
                }
                FormulaExpression::FieldReference(name) => {
                    // Try bound var first
                    if let Some(BoundValue::Node(n)) = ctx.var_bindings.get(name) {
                        return Some((ctx.node_path.clone(), *n));
                    }
                    // Try children of current
                    if let Some(child) = ctx
                        .current_node
                        .get_accessible_children()
                        .into_iter()
                        .find(|c| &c.name == name)
                    {
                        let mut p = ctx.node_path.clone();
                        p.push(child.name.clone());
                        return Some((p, child));
                    }
                    None
                }
                _ => None,
            }
        }

        match &expr {
            FormulaExpression::PathReference(_) => {
                if let Some((p, base)) = resolve_base(&expr, context) {
                    let items = base.get_accessible_children();
                    return Ok((p, items));
                }
                Err(OverseerError::FormulaError("Path not found".to_string()))
            }
            FormulaExpression::MethodChain { base, calls } => {
                let (base_path, base_node) = resolve_base(base, context).ok_or_else(|| {
                    OverseerError::FormulaError("Base path not found".to_string())
                })?;
                let mut list: Vec<&OverseerNode> = base_node.get_accessible_children();
                let current_container = base_node;
                for call in calls {
                    match call.name.as_str() {
                        "filter" => {
                            let lambda = call.args.get(0).ok_or_else(|| {
                                OverseerError::FormulaError("filter() requires 1 arg".to_string())
                            })?;
                            let mut out: Vec<&OverseerNode> = Vec::new();
                            for n in list.into_iter() {
                                let item_ctx = EvaluationContext::new_with_current_and_parent(
                                    n,
                                    Some(current_container),
                                    {
                                        let mut p = base_path.clone();
                                        p.push(n.name.clone());
                                        p
                                    },
                                    context.document_root,
                                );
                                let keep_bool =
                                    match Self::eval_lambda(lambda, None, Some(n), None, &item_ctx)
                                    {
                                        Ok(v) => Self::value_to_bool(&v).unwrap_or(false),
                                        Err(_) => false,
                                    };
                                if keep_bool {
                                    out.push(n);
                                }
                            }
                            list = out;
                        }
                        "find" => {
                            let key_val_expr = call.args.get(0).ok_or_else(|| {
                                OverseerError::FormulaError("find() requires 1 arg".to_string())
                            })?;
                            let key_field = if let Some(OverseerValue::String(k)) =
                                current_container.parameters.get("key")
                            {
                                k.clone()
                            } else {
                                return Err(OverseerError::ValidationError(
                                    "find() base does not declare a key parameter".to_string(),
                                ));
                            };
                            let mut found: Option<&OverseerNode> = None;
                            let target = Self::evaluate_expression(key_val_expr, context)?;
                            for n in list.into_iter() {
                                if let Some(ch) = n
                                    .get_accessible_children()
                                    .into_iter()
                                    .find(|c| c.name == key_field)
                                {
                                    if let Some(v) =
                                        Self::get_effective_param(&ch.parameters, "value")
                                    {
                                        if Self::compare_values(v, &target)
                                            .map(|o| o == std::cmp::Ordering::Equal)
                                            .unwrap_or(false)
                                        {
                                            found = Some(n);
                                            break;
                                        }
                                    }
                                }
                            }
                            list = match found {
                                Some(n) => vec![n],
                                None => vec![],
                            };
                        }
                        other => {
                            return Err(OverseerError::FormulaError(format!(
                                "Unsupported method in source: {}",
                                other
                            )));
                        }
                    }
                }
                Ok((base_path, list))
            }
            _ => Err(OverseerError::FormulaError(
                "Unsupported source expression".to_string(),
            )),
        }
    }
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
            Err(_) => {
                return Err(OverseerError::FormulaError(
                    "invalid formula error".to_string(),
                ))
            }
        };
        match &expr {
            FormulaExpression::Lambda { .. } => {
                Self::eval_lambda(&expr, None, Some(item_node), None, context)
            }
            _ => Self::eval_lambda(&expr, None, Some(item_node), None, context),
        }
    }
    /// Helper: get effective parameter value preferring computed shadow
    fn get_effective_param<'p>(
        params: &'p std::collections::HashMap<String, OverseerValue>,
        key: &str,
    ) -> Option<&'p OverseerValue> {
        // Semantics: for 'value', prefer raw when it's not a Formula; if it's a Formula and no computed shadow yet,
        // return None so callers can decide to evaluate the raw formula in the correct context (avoids stale ordering).
        if key == "value" {
            if let Some(raw) = params.get("value") {
                // If explicitly Null, consult fallback
                if matches!(raw, OverseerValue::Null) {
                    if let Some(fb) = params.get("_computed_fallback") {
                        return Some(fb);
                    }
                    if let Some(fb_raw) = params.get("fallback") {
                        if !matches!(fb_raw, OverseerValue::Formula(_)) {
                            return Some(fb_raw);
                        }
                    }
                    // No fallback provided; treat as an effective Null value
                    return Some(raw);
                } else if !matches!(raw, OverseerValue::Formula(_)) {
                    return Some(raw);
                }
            }
            if let Some(comp) = params.get("_computed_value") {
                return Some(comp);
            }
            // Do NOT return the raw Formula here; let callers evaluate it when needed
            return None;
        } else {
            let shadow = format!("_computed_{}", key);
            if let Some(v) = params.get(&shadow) {
                return Some(v);
            }
            return params.get(key);
        }
    }

    /// Compute a node's effective value, including evaluating fallback formulas when value is Null.
    pub fn get_effective_value_for_node(
        node: &OverseerNode,
        path: &[String],
        document_root: &[OverseerNode],
    ) -> Result<OverseerValue, OverseerError> {
        // 1) Computed shadow takes precedence
        if let Some(comp) = node.parameters.get("_computed_value") {
            return Ok(comp.clone());
        }
        // 2) Raw value handling
        if let Some(raw) = node.parameters.get("value") {
            match raw {
                OverseerValue::Formula(f) => {
                    let ctx =
                        EvaluationContext::new_with_current(node, path.to_vec(), document_root);
                    return FormulaEvaluator::evaluate_formula(f, &ctx);
                }
                OverseerValue::Null => {
                    // Use computed fallback if present; else evaluate fallback formula if any; else literal fallback; else Null
                    if let Some(fb) = node.parameters.get("_computed_fallback") {
                        return Ok(fb.clone());
                    }
                    if let Some(fb_raw) = node.parameters.get("fallback") {
                        match fb_raw {
                            OverseerValue::Formula(f) => {
                                let ctx = EvaluationContext::new_with_current(
                                    node,
                                    path.to_vec(),
                                    document_root,
                                );
                                return FormulaEvaluator::evaluate_formula(f, &ctx);
                            }
                            other => return Ok(other.clone()),
                        }
                    }
                    return Ok(OverseerValue::Null);
                }
                other => return Ok(other.clone()),
            }
        }
        // 3) No explicit value: if we have computed fallback, use it
        if let Some(fb) = node.parameters.get("_computed_fallback") {
            return Ok(fb.clone());
        }
        Ok(OverseerValue::Null)
    }

    /// Evaluate a formula expression string within the given context
    /// Results cached for the duration of one evaluation pass.
    ///
    /// Off by default. The resolver switches it on around a pass, during which every read
    /// goes through an immutable snapshot of the document; outside that window the document
    /// is being mutated and a cache would serve stale values. Thread-local because passes
    /// run on whichever thread the command arrived on.
    fn with_memo<R>(f: impl FnOnce(&mut Option<std::collections::HashMap<(String, String), OverseerValue>>) -> R) -> R {
        thread_local! {
            static MEMO: std::cell::RefCell<
                Option<std::collections::HashMap<(String, String), OverseerValue>>,
            > = const { std::cell::RefCell::new(None) };
        }
        MEMO.with(|cell| f(&mut cell.borrow_mut()))
    }

    /// Start caching. Call once per pass; the previous contents are discarded.
    pub fn begin_pass_memo() {
        Self::with_memo(|slot| *slot = Some(std::collections::HashMap::new()));
    }

    /// Stop caching. Anything evaluated after this is computed fresh.
    pub fn end_pass_memo() {
        Self::with_memo(|slot| *slot = None);
    }

    fn memo_lookup(key: &(String, String)) -> Option<OverseerValue> {
        Self::with_memo(|slot| slot.as_ref().and_then(|m| m.get(key).cloned()))
    }

    fn memo_store(key: &(String, String), value: OverseerValue) {
        Self::with_memo(|slot| {
            if let Some(m) = slot.as_mut() {
                m.insert(key.clone(), value);
            }
        });
    }

    pub fn evaluate_formula(
        formula: &str,
        context: &EvaluationContext,
    ) -> Result<OverseerValue, OverseerError> {
        // Within an evaluation pass the document is read from an immutable snapshot, so the
        // same formula at the same path cannot produce two answers. It does get asked
        // repeatedly though - once for the node that owns it, and again every time another
        // formula reads through that node - which is how a single aggregate over the history
        // list came to be evaluated hundreds of times per pass and dominate the cost of an
        // interaction.
        //
        // Lambda bindings are deliberately excluded: inside `filter(|x| ...)` the same text
        // means something different for each item, and the key would have to capture that.
        // The expensive formulas are the top-level aggregates, which have no bindings.
        let memo_key = if context.var_bindings.is_empty() {
            Some((context.node_path.join("/"), formula.to_string()))
        } else {
            None
        };
        if let Some(key) = memo_key.as_ref() {
            if let Some(hit) = Self::memo_lookup(key) {
                return Ok(hit);
            }
        }
        let outcome = Self::evaluate_formula_uncached(formula, context);
        if let (Some(key), Ok(value)) = (memo_key, outcome.as_ref()) {
            Self::memo_store(&key, value.clone());
        }
        outcome
    }

    fn evaluate_formula_uncached(
        formula: &str,
        context: &EvaluationContext,
    ) -> Result<OverseerValue, OverseerError> {
        debug_evaluator!(
            "[EVAL] Start evaluate_formula at path {:?}: {}",
            context.node_path,
            formula
        );
        // Parse the formula expression
        let expression = match Self::parse_expression(formula) {
            Ok(expr) => expr,
            Err(_err) => {
                debug_evaluator!(
                    "[EVAL] Parse error at {:?}: <hidden> => '{}'",
                    context.node_path,
                    formula
                );
                return Err(OverseerError::FormulaError(
                    "invalid formula error".to_string(),
                ));
            }
        };
        debug_evaluator!("[EVAL] Parsed AST: {:?}", expression);

        // Cycle guard: if this (path,formula) is already in progress, short-circuit to Null
        let key = Self::guard_key(&context.node_path, formula);
        let in_progress = Self::EVAL_GUARD.with(|set| {
            let mut set = set.borrow_mut();
            if set.contains(&key) {
                true
            } else {
                set.insert(key.clone());
                false
            }
        });
        if in_progress {
            debug_evaluator!("[EVAL] Cycle detected for key {}, returning Null", key);
            return Ok(OverseerValue::Null);
        }

        // Evaluate the parsed expression
        let result = Self::evaluate_expression(&expression, context);
        // Clear guard for this key
        Self::EVAL_GUARD.with(|set| {
            set.borrow_mut().remove(&key);
        });
        debug_evaluator!(
            "[EVAL] End evaluate_formula at path {:?}: result = {:?}",
            context.node_path,
            result
        );
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
            FormulaExpression::BooleanLiteral(b) => Ok(OverseerValue::Boolean(*b)),
            FormulaExpression::FieldReference(field_name) => {
                debug_evaluator!(
                    "[EVAL] Resolve field '{}' at {:?}",
                    field_name,
                    context.node_path
                );
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
                debug_evaluator!(
                    "[EVAL] Resolve path param {:?}.{} at {:?}",
                    path,
                    param,
                    context.node_path
                );
                let out = Self::resolve_path_param(path, param, context);
                debug_evaluator!(
                    "[EVAL] Resolved path param {:?}.{} => {:?}",
                    path,
                    param,
                    out
                );
                out
            }
            FormulaExpression::Lambda { .. } => Err(OverseerError::FormulaError(
                "Unexpected top-level lambda; use inside map/filter/reduce".to_string(),
            )),
            FormulaExpression::MethodChain { base, calls } => {
                debug_evaluator!(
                    "[EVAL] Method chain on base {:?} with calls {:?}",
                    base,
                    calls
                );
                let out = Self::evaluate_method_chain(base, calls, context);
                debug_evaluator!("[EVAL] Method chain result => {:?}", out);
                out
            }
            FormulaExpression::UnaryOp { operator, expr } => match operator {
                UnaryOperator::Not => {
                    let val = Self::evaluate_expression(expr, context)?;
                    let b = Self::value_to_bool(&val)?;
                    Ok(OverseerValue::Boolean(!b))
                }
            },
            FormulaExpression::BinaryOp {
                left,
                operator,
                right,
            } => {
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
                        debug_evaluator!(
                            "[EVAL] Binary {:?} {:?} {:?} => {:?}",
                            left_val,
                            operator,
                            right_val,
                            res
                        );
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
            FormulaExpression::Conditional {
                condition,
                then_branch,
                else_branch,
            } => {
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
                // Resolve base to a node, then follow child segments and return final node's effective value
                if let Some(mut cur) = Self::eval_expr_to_node(base, context) {
                    let mut p = context.node_path.clone();
                    for seg in segments {
                        if let Some(next) = cur
                            .get_accessible_children()
                            .into_iter()
                            .find(|c| &c.name == seg)
                        {
                            p.push(next.name.clone());
                            cur = next;
                        } else {
                            return Err(OverseerError::FormulaError(format!(
                                "Path segment not found: {}",
                                seg
                            )));
                        }
                    }
                    return FormulaEvaluator::get_effective_value_for_node(
                        cur,
                        &p,
                        context.document_root,
                    );
                }
                Err(OverseerError::FormulaError(
                    "Base of path does not resolve to a node".to_string(),
                ))
            }
        }
    }

    /// Resolve a simple field reference within the current node
    fn resolve_field_reference(
        field_name: &str,
        context: &EvaluationContext,
    ) -> Result<OverseerValue, OverseerError> {
        debug_evaluator!(
            "[EVAL] resolve_field_reference '{}' at {:?}",
            field_name,
            context.node_path
        );
        // Strategy:
        // 0) Lambda-bound variable lookup
        if let Some(bound) = context.var_bindings.get(field_name) {
            debug_evaluator!("[EVAL] '{}' bound in lambda: {:?}", field_name, bound);
            return match bound {
                BoundValue::Value(v) => Ok(v.clone()),
                BoundValue::Node(n) => {
                    if let Some(v) = Self::get_effective_param(&n.parameters, "value") {
                        Ok(v.clone())
                    } else {
                        Err(OverseerError::FormulaError(format!(
                            "Bound node '{}' has no value",
                            field_name
                        )))
                    }
                }
            };
        }
        // 1) Try current node's parameters by exact field name
        // Safety-first: use effective/computed value to avoid self-recursive evaluation.
        if let Some(val) = Self::get_effective_param(&context.current_node.parameters, field_name) {
            return Ok(val.clone());
        }

        // 2) Try to find a child node with this name and return its effective value (value or fallback)
        if let Some(child) = context
            .current_node
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == field_name)
        {
            debug_evaluator!("[EVAL] Found child '{}' under current node", field_name);
            let mut path = context.node_path.clone();
            path.push(child.name.clone());
            return FormulaEvaluator::get_effective_value_for_node(
                child,
                &path,
                context.document_root,
            );
        }

        // 3) Look up in parent scope: siblings or parent parameters
        if !context.node_path.is_empty() {
            // Walk up the ancestor chain from nearest parent to root
            for end in (1..=context.node_path.len() - 1).rev() {
                let ancestor_path = &context.node_path[..end];
                if let Some(ancestor) =
                    FormulaEvaluator::resolve_path_to_node(ancestor_path, context.document_root)
                {
                    debug_evaluator!(
                        "[EVAL] Searching ancestor {:?} for '{}'",
                        ancestor_path,
                        field_name
                    );
                    // a) Ancestor parameters by key: use effective/computed only (avoid evaluating raw formulas here)
                    if let Some(val) = Self::get_effective_param(&ancestor.parameters, field_name) {
                        return Ok(val.clone());
                    }
                    // b) Child of ancestor by name
                    if let Some(child) = ancestor
                        .get_accessible_children()
                        .into_iter()
                        .find(|c| c.name == field_name)
                    {
                        debug_evaluator!(
                            "[EVAL] Found ancestor child '{}' under {:?}",
                            field_name,
                            ancestor_path
                        );
                        let mut path = ancestor_path.to_vec();
                        path.push(child.name.clone());
                        return FormulaEvaluator::get_effective_value_for_node(
                            child,
                            &path,
                            context.document_root,
                        );
                    }
                    // c) Deep search under ancestor (first match)
                    if let Some(val) =
                        FormulaEvaluator::find_value_by_name_deep(ancestor, field_name)
                    {
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
            debug_evaluator!(
                "[EVAL] Global fallback found node '{}' at root search",
                field_name
            );
            let guess_path = vec![node.name.clone()];
            return FormulaEvaluator::get_effective_value_for_node(
                node,
                &guess_path,
                context.document_root,
            );
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
        debug_evaluator!(
            "[EVAL] resolve_path_reference {:?} at {:?}",
            path,
            context.node_path
        );
        if path.is_empty() {
            return Err(OverseerError::FormulaError(
                "Empty path reference".to_string(),
            ));
        }

        // Case 0: "/"-anchored paths (within current container/nearest ancestor containing first segment)
        if path[0] == "/" {
            let segments = &path[1..];
            if segments.is_empty() {
                return Err(OverseerError::FormulaError(
                    "Path missing target after /".to_string(),
                ));
            }
            // Find nearest ancestor that contains the first segment as a child; fallback to current
            let mut base_path = context.node_path.clone();
            let mut found_start = context.current_node;
            let first_seg = &segments[0];
            let mut end = context.node_path.len();
            let mut found = false;
            while end > 0 {
                if let Some(candidate) = Self::resolve_path_to_node(
                    &context.node_path[..end].to_vec(),
                    context.document_root,
                ) {
                    if candidate
                        .get_accessible_children()
                        .into_iter()
                        .any(|c| &c.name == first_seg)
                    {
                        found_start = candidate;
                        base_path = context.node_path[..end].to_vec();
                        debug_evaluator!(
                            "[EVAL] '/' anchored base {:?} chosen for first seg '{}'",
                            base_path,
                            first_seg
                        );
                        found = true;
                        break;
                    }
                }
                end -= 1;
            }
            let mut skip_first = false;
            if !found {
                // Fallback: treat as absolute root path if a top-level node matches first segment
                if let Some(root_match) =
                    context.document_root.iter().find(|n| &n.name == first_seg)
                {
                    found_start = root_match;
                    base_path = vec![first_seg.clone()];
                    debug_evaluator!(
                        "[EVAL] '/' absolute base {:?} chosen at root for first seg '{}'",
                        base_path,
                        first_seg
                    );
                    skip_first = true;
                }
            }
            // Traverse all but last via children names from found_start
            let mut current = found_start;
            let mut p = base_path.clone();
            let start_idx = if skip_first { 1 } else { 0 };
            for seg in &segments[start_idx..segments.len().saturating_sub(1)] {
                if let Some(next) = current
                    .get_accessible_children()
                    .into_iter()
                    .find(|c| &c.name == seg)
                {
                    p.push(next.name.clone());
                    current = next;
                } else {
                    return Err(OverseerError::FormulaError(format!(
                        "Path segment not found: {}",
                        seg
                    )));
                }
            }
            let last = segments.last().unwrap();
            // Final segment can be either a parameter on current or a child node's value
            if let Some(val) = Self::get_effective_param(&current.parameters, last) {
                if let OverseerValue::Formula(formula_expr) = val {
                    let child_ctx =
                        EvaluationContext::new_with_current(current, p, context.document_root);
                    return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
                }
                return Ok(val.clone());
            }
            if let Some(child) = current
                .get_accessible_children()
                .into_iter()
                .find(|c| &c.name == last)
            {
                // Return the child's effective value (handles computed, raw, and fallback-on-demand)
                p.push(child.name.clone());
                return FormulaEvaluator::get_effective_value_for_node(
                    child,
                    &p,
                    context.document_root,
                );
            }
            // Fallback: deep search within found_start subtree
            if let Some(v) = FormulaEvaluator::find_value_by_name_deep(found_start, last) {
                return Ok(v);
            }
            // NEW Fallback: search upwards across ancestors for a descendant with this name
            let mut up_end = context.node_path.len();
            while up_end > 0 {
                let anc = &context.node_path[..up_end];
                if let Some(node) = Self::resolve_path_to_node(anc, context.document_root) {
                    if let Some(v) = FormulaEvaluator::find_value_by_name_deep(node, last) {
                        return Ok(v);
                    }
                }
                up_end -= 1;
            }
            return Err(OverseerError::FormulaError(format!(
                "Unknown field '{}' at target path",
                last
            )));
        }

        // Case A: identifier-based relative path (e.g., GrandChild/Inner/x)
        if path[0] != ".." {
            // If first segment is a bound variable naming a node, resolve relative to it
            if let Some(BoundValue::Node(start)) = context.var_bindings.get(&path[0]) {
                // Traverse remaining segments from the bound node
                if path.len() == 1 {
                    // Just the variable name; return its value param if present
                    if let Some(v) = Self::get_effective_param(&start.parameters, "value") {
                        return Ok(v.clone());
                    }
                    return Err(OverseerError::FormulaError(format!(
                        "Bound node '{}' has no value",
                        path[0]
                    )));
                }
                let mut node = *start;
                let mut traversed_all = true;
                for (i, seg) in path[1..].iter().enumerate() {
                    if let Some(next) = node
                        .get_accessible_children()
                        .into_iter()
                        .find(|c| &c.name == seg)
                    {
                        node = next;
                    } else {
                        // Allow final segment to be a parameter on current node
                        if i == path.len() - 2 {
                            // since we skipped first, len-2 is last index here
                            if let Some(v) = Self::get_effective_param(&node.parameters, seg) {
                                return Ok(v.clone());
                            }
                            // If raw parameter exists as a Formula, evaluate it on-demand at this node
                            if let Some(OverseerValue::Formula(formula_expr)) =
                                node.parameters.get(seg)
                            {
                                let child_ctx = EvaluationContext::new_with_current(
                                    node,
                                    context.node_path.clone(),
                                    context.document_root,
                                );
                                return FormulaEvaluator::evaluate_formula(
                                    formula_expr,
                                    &child_ctx,
                                );
                            }
                            // Deep-search fallback for single-segment access like x/field
                            if path.len() == 2 {
                                if let Some(v) =
                                    FormulaEvaluator::find_value_by_name_deep(*start, seg)
                                {
                                    return Ok(v);
                                }
                            }
                        }
                        traversed_all = false;
                        break;
                    }
                }
                if traversed_all {
                    // Prefer node's value
                    if let Some(v) = Self::get_effective_param(&node.parameters, "value") {
                        return Ok(v.clone());
                    }
                    // If no computed value yet and raw value is a Formula, evaluate it now in the child's context
                    if let Some(OverseerValue::Formula(formula_expr)) = node.parameters.get("value")
                    {
                        // Build a best-effort path by appending this node's name to the current path
                        let mut p = context.node_path.clone();
                        p.push(node.name.clone());
                        let child_ctx =
                            EvaluationContext::new_with_current(node, p, context.document_root);
                        return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
                    }
                    // Or treat last as parameter on that node
                    if let Some(last) = path.last() {
                        if let Some(v) = Self::get_effective_param(&node.parameters, last) {
                            return Ok(v.clone());
                        }
                        // If raw parameter exists as a Formula, evaluate it on-demand at this node
                        if let Some(OverseerValue::Formula(formula_expr)) =
                            node.parameters.get(last)
                        {
                            let child_ctx = EvaluationContext::new_with_current(
                                node,
                                context.node_path.clone(),
                                context.document_root,
                            );
                            return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
                        }
                    }
                } else if path.len() == 2 {
                    // If we couldn't traverse and it's a simple x/field shape, deep-search under the bound node
                    let target = &path[1];
                    if let Some(v) = FormulaEvaluator::find_value_by_name_deep(*start, target) {
                        return Ok(v);
                    }
                }
                return Err(OverseerError::FormulaError(format!(
                    "Path not found from bound var: {}",
                    path.join("/")
                )));
            }
            // Try from current node upward until a matching child chain is found
            let mut end = context.node_path.len();
            while end > 0 {
                let anc_path = &context.node_path[..end];
                if let Some(mut node) =
                    FormulaEvaluator::resolve_path_to_node(anc_path, context.document_root)
                {
                    let mut traversed_all = true;
                    for (i, seg) in path.iter().enumerate() {
                        if let Some(next) = node
                            .get_accessible_children()
                            .into_iter()
                            .find(|c| &c.name == seg)
                        {
                            node = next;
                        } else {
                            // Allow final segment to be a parameter on current node
                            if i == path.len() - 1 {
                                if let Some(v) = Self::get_effective_param(&node.parameters, seg) {
                                    return Ok(v.clone());
                                }
                                // If raw parameter exists as a Formula, evaluate it on-demand at this node
                                if let Some(OverseerValue::Formula(formula_expr)) =
                                    node.parameters.get(seg)
                                {
                                    let p = anc_path.to_vec();
                                    let child_ctx = EvaluationContext::new_with_current(
                                        node,
                                        p,
                                        context.document_root,
                                    );
                                    return FormulaEvaluator::evaluate_formula(
                                        formula_expr,
                                        &child_ctx,
                                    );
                                }
                            }
                            traversed_all = false;
                            break;
                        }
                    }
                    if traversed_all {
                        // If we ended on a node (e.g., x), return its effective value (supports fallback)
                        let mut p = anc_path.to_vec();
                        p.extend(path.iter().cloned());
                        return FormulaEvaluator::get_effective_value_for_node(
                            node,
                            &p,
                            context.document_root,
                        );
                    }
                }
                end -= 1;
            }
            // Extra root-level fallback: if first segment matches a top-level node, start from there
            if let Some(first) = path.first() {
                if let Some(mut node) = context.document_root.iter().find(|n| &n.name == first) {
                    if path.len() == 1 {
                        return FormulaEvaluator::get_effective_value_for_node(
                            node,
                            &[node.name.clone()],
                            context.document_root,
                        );
                    }
                    let mut p = vec![node.name.clone()];
                    for (i, seg) in path.iter().enumerate().skip(1) {
                        if let Some(next) = node
                            .get_accessible_children()
                            .into_iter()
                            .find(|c| &c.name == seg)
                        {
                            p.push(next.name.clone());
                            node = next;
                        } else {
                            if i == path.len() - 1 {
                                if let Some(v) = Self::get_effective_param(&node.parameters, seg) {
                                    return Ok(v.clone());
                                }
                                if let Some(OverseerValue::Formula(formula_expr)) =
                                    node.parameters.get(seg)
                                {
                                    let child_ctx = EvaluationContext::new_with_current(
                                        node,
                                        p,
                                        context.document_root,
                                    );
                                    return FormulaEvaluator::evaluate_formula(
                                        formula_expr,
                                        &child_ctx,
                                    );
                                }
                            }
                            break;
                        }
                    }
                    // Ended on a node; return its effective value
                    return FormulaEvaluator::get_effective_value_for_node(
                        node,
                        &p,
                        context.document_root,
                    );
                }
            }
            return Err(OverseerError::FormulaError(format!(
                "Path not found: {}",
                path.join("/")
            )));
        }

        // Case B: ../-prefixed paths
        // Only ../-prefixed paths are supported in this branch
        if path[0] != ".." {
            return Err(OverseerError::FormulaError(
                "Path must start with '..'".to_string(),
            ));
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
            return Err(OverseerError::FormulaError(
                "Path missing target after ..".to_string(),
            ));
        }

        // Compute ancestor path by removing `hops` segments from the end of node_path
        if context.node_path.len() < hops {
            return Err(OverseerError::FormulaError(
                "Path climbs above root".to_string(),
            ));
        }
        let ancestor_segments = &context.node_path[..context.node_path.len() - hops];
        let ancestor = Self::resolve_path_to_node(ancestor_segments, context.document_root)
            .ok_or_else(|| OverseerError::FormulaError("Ancestor not found".to_string()))?;

        // Resolve remaining path against ancestor
        let mut current = ancestor;
        for seg in &remaining[..remaining.len().saturating_sub(1)] {
            // Navigate through named children
            if let Some(next) = current
                .get_accessible_children()
                .into_iter()
                .find(|c| &c.name == seg)
            {
                current = next;
            } else {
                return Err(OverseerError::FormulaError(format!(
                    "Path segment not found: {}",
                    seg
                )));
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
                    p.extend(remaining[..remaining.len() - 1].iter().cloned());
                }
                let child_ctx =
                    EvaluationContext::new_with_current(current, p, context.document_root);
                return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
            }
            return Ok(val.clone());
        }
        if let Some(child) = current
            .get_accessible_children()
            .into_iter()
            .find(|c| &c.name == last)
        {
            // Return the child's effective value (supports fallback)
            let mut p = ancestor_segments.to_vec();
            if !remaining.is_empty() {
                p.extend(remaining[..remaining.len() - 1].iter().cloned());
            }
            p.push(child.name.clone());
            return FormulaEvaluator::get_effective_value_for_node(
                child,
                &p,
                context.document_root,
            );
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

        Err(OverseerError::FormulaError(format!(
            "Unknown field '{}' at target path",
            last
        )))
    }

    /// Resolve a path reference with parameter extraction like ../field.color
    fn resolve_path_param(
        path: &[String],
        param: &str,
        context: &EvaluationContext,
    ) -> Result<OverseerValue, OverseerError> {
        debug_evaluator!(
            "[EVAL] resolve_path_param {:?}.{} at {:?}",
            path,
            param,
            context.node_path
        );
        if path.is_empty() {
            return Err(OverseerError::FormulaError(
                "Empty path for param extraction".to_string(),
            ));
        }
        // Current-container anchored path: ["/", seg1, seg2, ...] (nearest ancestor containing seg1)
        if path[0] == "/" {
            let segments = &path[1..];
            if segments.is_empty() {
                return Err(OverseerError::FormulaError(
                    "Path missing target after / for param".to_string(),
                ));
            }
            // Find nearest ancestor containing first segment
            let mut start = context.current_node;
            let mut p = context.node_path.clone();
            let first = &segments[0];
            let mut end = context.node_path.len();
            let mut found = false;
            while end > 0 {
                if let Some(candidate) = Self::resolve_path_to_node(
                    &context.node_path[..end].to_vec(),
                    context.document_root,
                ) {
                    if candidate
                        .get_accessible_children()
                        .into_iter()
                        .any(|c| &c.name == first)
                    {
                        start = candidate;
                        p = context.node_path[..end].to_vec();
                        found = true;
                        break;
                    }
                }
                end -= 1;
            }
            let mut skip_first = false;
            if !found {
                if let Some(root_match) = context.document_root.iter().find(|n| &n.name == first) {
                    start = root_match;
                    p = vec![first.clone()];
                    skip_first = true;
                    debug_evaluator!(
                        "[EVAL] '/' absolute base {:?} for param chosen at root for first seg '{}'",
                        p,
                        first
                    );
                }
            }
            let mut node = start;
            let start_idx = if skip_first { 1 } else { 0 };
            for seg in &segments[start_idx..] {
                if let Some(next) = node
                    .get_accessible_children()
                    .into_iter()
                    .find(|c| &c.name == seg)
                {
                    p.push(next.name.clone());
                    node = next;
                } else {
                    return Err(OverseerError::FormulaError(format!(
                        "Path segment not found: {}",
                        seg
                    )));
                }
            }
            if let Some(v) = Self::get_effective_param(&node.parameters, param) {
                if let OverseerValue::Formula(formula_expr) = v {
                    let child_ctx =
                        EvaluationContext::new_with_current(node, p, context.document_root);
                    return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
                }
                return Ok(Self::value_for_param_extraction(v));
            }
            return Err(OverseerError::FormulaError(format!(
                "Parameter '{}' not found on node",
                param
            )));
        }
        // Identifier-based path from nearest ancestor
        if path[0] != ".." {
            // Variable-anchored: start from bound node if available
            if let Some(BoundValue::Node(start)) = context.var_bindings.get(&path[0]) {
                let mut node = *start;
                let mut ok = true;
                for seg in path.iter().skip(1) {
                    if let Some(next) = node
                        .get_accessible_children()
                        .into_iter()
                        .find(|c| &c.name == seg)
                    {
                        node = next;
                    } else {
                        ok = false;
                        break;
                    }
                }
                if ok {
                    if let Some(v) = Self::get_effective_param(&node.parameters, param) {
                        if let OverseerValue::Formula(formula_expr) = v {
                            let child_ctx = EvaluationContext::new_with_current(
                                node,
                                context.node_path.clone(),
                                context.document_root,
                            );
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
                                let child_ctx = EvaluationContext::new_with_current(
                                    found_node,
                                    context.node_path.clone(),
                                    context.document_root,
                                );
                                return FormulaEvaluator::evaluate_formula(
                                    formula_expr,
                                    &child_ctx,
                                );
                            }
                            return Ok(Self::value_for_param_extraction(v));
                        }
                    }
                }
                return Err(OverseerError::FormulaError(format!(
                    "Parameter '{}' not found on node",
                    param
                )));
            }
            let mut end = context.node_path.len();
            while end > 0 {
                let anc_path = &context.node_path[..end];
                if let Some(mut node) =
                    FormulaEvaluator::resolve_path_to_node(anc_path, context.document_root)
                {
                    let mut ok = true;
                    for seg in path {
                        if let Some(next) = node
                            .get_accessible_children()
                            .into_iter()
                            .find(|c| &c.name == seg)
                        {
                            node = next;
                        } else {
                            ok = false;
                            break;
                        }
                    }
                    if ok {
                        if let Some(v) = Self::get_effective_param(&node.parameters, param) {
                            if let OverseerValue::Formula(formula_expr) = v {
                                let mut p = anc_path.to_vec();
                                p.extend(path.iter().cloned());
                                let child_ctx = EvaluationContext::new_with_current(
                                    node,
                                    p,
                                    context.document_root,
                                );
                                return FormulaEvaluator::evaluate_formula(
                                    formula_expr,
                                    &child_ctx,
                                );
                            }
                            return Ok(Self::value_for_param_extraction(v));
                        }
                        // If parameter not found, error
                        return Err(OverseerError::FormulaError(format!(
                            "Parameter '{}' not found on node",
                            param
                        )));
                    }
                }
                end -= 1;
            }
            return Err(OverseerError::FormulaError(format!(
                "Path not found: {}.{}",
                path.join("/"),
                param
            )));
        }

        // ../ path
        let mut hops = 1usize;
        let mut idx = 1usize;
        while idx < path.len() && path[idx] == ".." {
            hops += 1;
            idx += 1;
        }
        let remaining = &path[idx..];
        if context.node_path.len() < hops {
            return Err(OverseerError::FormulaError(
                "Path climbs above root".to_string(),
            ));
        }
        let ancestor_segments = &context.node_path[..context.node_path.len() - hops];
        let mut current = Self::resolve_path_to_node(ancestor_segments, context.document_root)
            .ok_or_else(|| OverseerError::FormulaError("Ancestor not found".to_string()))?;
        for seg in remaining {
            if let Some(next) = current
                .get_accessible_children()
                .into_iter()
                .find(|c| &c.name == seg)
            {
                current = next;
            } else {
                return Err(OverseerError::FormulaError(format!(
                    "Path segment not found: {}",
                    seg
                )));
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
        Err(OverseerError::FormulaError(format!(
            "Parameter '{}' not found on node",
            param
        )))
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
                BinaryOperator::LessThanOrEqual => {
                    OverseerValue::Boolean(ord == Ordering::Less || ord == Ordering::Equal)
                }
                BinaryOperator::GreaterThan => OverseerValue::Boolean(ord == Ordering::Greater),
                BinaryOperator::GreaterThanOrEqual => {
                    OverseerValue::Boolean(ord == Ordering::Greater || ord == Ordering::Equal)
                }
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
    fn compare_values(
        a: &OverseerValue,
        b: &OverseerValue,
    ) -> Result<std::cmp::Ordering, OverseerError> {
        use std::cmp::Ordering;
        Ok(match (a, b) {
            (OverseerValue::Integer(x), OverseerValue::Integer(y)) => x.cmp(y),
            (OverseerValue::Float(x), OverseerValue::Float(y)) => {
                x.partial_cmp(y).unwrap_or(Ordering::Equal)
            }
            (OverseerValue::Integer(x), OverseerValue::Float(y)) => {
                (*x as f64).partial_cmp(y).unwrap_or(Ordering::Equal)
            }
            (OverseerValue::Float(x), OverseerValue::Integer(y)) => {
                x.partial_cmp(&(*y as f64)).unwrap_or(Ordering::Equal)
            }
            (OverseerValue::String(x), OverseerValue::String(y)) => x.cmp(y),
            (OverseerValue::Boolean(x), OverseerValue::Boolean(y)) => x.cmp(y),
            (OverseerValue::Date(x), OverseerValue::Date(y)) => x.cmp(y),
            (OverseerValue::Timestamp(x), OverseerValue::Timestamp(y)) => x.cmp(y),
            // Mixed date/timestamp: coerce date to start-of-day timestamp
            (OverseerValue::Date(x), OverseerValue::Timestamp(y)) => {
                Self::date_to_timestamp(x).cmp(y)
            }
            (OverseerValue::Timestamp(x), OverseerValue::Date(y)) => {
                x.cmp(&Self::date_to_timestamp(y))
            }
            // Fallback to string representations
            _ => Self::value_to_string(a).cmp(&Self::value_to_string(b)),
        })
    }

    fn date_to_timestamp(date: &str) -> String {
        if date.contains('T') {
            return date.to_string();
        }
        format!("{}T00:00:00Z", date)
    }

    /// Convert an OverseerValue to a number for arithmetic/comparisons
    fn value_to_number(value: &OverseerValue) -> Result<f64, OverseerError> {
        match value {
            OverseerValue::Null => Err(OverseerError::FormulaError(
                "Cannot convert null to number".to_string(),
            )),
            OverseerValue::Integer(i) => Ok(*i as f64), // i64
            OverseerValue::Float(f) => Ok(*f),
            OverseerValue::String(s) => s.parse::<f64>().map_err(|_| {
                OverseerError::FormulaError(format!("Cannot convert '{}' to number", s))
            }),
            _ => Err(OverseerError::FormulaError(
                "Cannot convert value to number".to_string(),
            )),
        }
    }

    /// Convert an OverseerValue to boolean for logical operations
    fn value_to_bool(value: &OverseerValue) -> Result<bool, OverseerError> {
        match value {
            OverseerValue::Null => Ok(false),
            OverseerValue::Boolean(b) => Ok(*b),
            OverseerValue::Integer(i) => Ok(*i != 0),
            OverseerValue::Float(f) => Ok(*f != 0.0),
            OverseerValue::String(s) => {
                let sl = s.to_lowercase();
                if sl == "true" {
                    Ok(true)
                } else if sl == "false" {
                    Ok(false)
                } else if let Ok(n) = s.parse::<f64>() {
                    Ok(n != 0.0)
                } else {
                    Ok(!s.is_empty())
                } // treat non-empty strings as true, empty as false
            }
            OverseerValue::Date(d) => Ok(!d.is_empty()),
            OverseerValue::Timestamp(ts) => Ok(!ts.is_empty()),
            _ => Err(OverseerError::FormulaError(
                "Cannot convert value to bool".to_string(),
            )),
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
                    return Err(OverseerError::FormulaError(
                        "today() function takes no arguments".to_string(),
                    ));
                }
                let today = if let Some(ovr) = Self::get_time_override() {
                    let local_dt: DateTime<Local> = DateTime::<Local>::from(ovr);
                    local_dt.format("%Y-%m-%d").to_string()
                } else {
                    chrono::Local::now().format("%Y-%m-%d").to_string()
                };
                Ok(OverseerValue::Date(today))
            }
            "now" => {
                if !args.is_empty() {
                    return Err(OverseerError::FormulaError(
                        "now() takes no arguments".to_string(),
                    ));
                }
                let now = match Self::get_time_override() {
                    Some(dt) => dt.to_rfc3339(),
                    None => chrono::Utc::now().to_rfc3339(),
                };
                Ok(OverseerValue::Timestamp(now))
            }
            // days_since(ts): returns whole days between now() and the given timestamp/date/string
            "days_since" => {
                if args.len() != 1 {
                    return Err(OverseerError::FormulaError(
                        "days_since(x) takes exactly 1 argument".to_string(),
                    ));
                }
                let val = Self::evaluate_expression(&args[0], context)?;
                if let Some(ts) = Self::to_utc(&val) {
                    let now = match Self::get_time_override() {
                        Some(dt) => dt,
                        None => chrono::Utc::now(),
                    };
                    let dur = now.signed_duration_since(ts);
                    let days = dur.num_days();
                    Ok(OverseerValue::Integer(days))
                } else {
                    Err(OverseerError::FormulaError(
                        "days_since: unable to parse timestamp/date".to_string(),
                    ))
                }
            }
            // minutes_since(ts): the same as days_since, at the resolution a deadline needs.
            //
            // `days_since` truncates toward zero, so three hours late and three hours early
            // both come out 0 and neither can be told from the other. A deadline is exactly
            // the question that turns on which side of zero you are.
            //
            // Negative in the future, which is what makes `minutes_since(due) > 0` read as
            // "overdue" rather than needing a second function to say the opposite.
            "minutes_since" => {
                if args.len() != 1 {
                    return Err(OverseerError::FormulaError(
                        "minutes_since(x) takes exactly 1 argument".to_string(),
                    ));
                }
                let val = Self::evaluate_expression(&args[0], context)?;
                match Self::to_utc(&val) {
                    Some(ts) => {
                        let now = Self::get_time_override().unwrap_or_else(chrono::Utc::now);
                        Ok(OverseerValue::Integer(
                            now.signed_duration_since(ts).num_minutes(),
                        ))
                    }
                    None => Err(OverseerError::FormulaError(
                        "minutes_since: unable to parse timestamp/date".to_string(),
                    )),
                }
            }
            // date_add_hours(x, n): a deadline an interval after something, where the interval
            // is shorter than a day. `n` may be fractional, so half an hour is 0.5 and there is
            // no need for a third function to say it in minutes.
            "date_add_hours" => {
                if args.len() != 2 {
                    return Err(OverseerError::FormulaError(
                        "date_add_hours(x, n) takes exactly 2 arguments".to_string(),
                    ));
                }
                let val = Self::evaluate_expression(&args[0], context)?;
                let hours = match Self::evaluate_expression(&args[1], context)? {
                    OverseerValue::Integer(i) => i as f64,
                    OverseerValue::Float(f) => f,
                    OverseerValue::String(ref s) => s.parse::<f64>().map_err(|_| {
                        OverseerError::FormulaError("date_add_hours: n must be a number".into())
                    })?,
                    _ => {
                        return Err(OverseerError::FormulaError(
                            "date_add_hours: n must be a number".to_string(),
                        ))
                    }
                };
                match Self::to_utc(&val) {
                    Some(dt) => {
                        let minutes = (hours * 60.0).round() as i64;
                        Ok(OverseerValue::Timestamp(
                            (dt + chrono::Duration::minutes(minutes)).to_rfc3339(),
                        ))
                    }
                    None => Err(OverseerError::FormulaError(
                        "date_add_hours: unable to parse timestamp/date".to_string(),
                    )),
                }
            }
            // date_add_days(x, n): add n days to a Date (YYYY-MM-DD) or Timestamp (RFC3339)
            "date_add_days" => {
                if args.len() != 2 {
                    return Err(OverseerError::FormulaError(
                        "date_add_days(x, n) takes exactly 2 arguments".to_string(),
                    ));
                }
                let val = Self::evaluate_expression(&args[0], context)?;
                let days_val = Self::evaluate_expression(&args[1], context)?;
                let n_days: i64 = match days_val {
                    OverseerValue::Integer(i) => i,
                    OverseerValue::Float(f) => f as i64,
                    OverseerValue::String(ref s) => s.parse::<i64>().map_err(|_| {
                        OverseerError::FormulaError("date_add_days: n must be integer".to_string())
                    })?,
                    _ => {
                        return Err(OverseerError::FormulaError(
                            "date_add_days: n must be integer".to_string(),
                        ))
                    }
                };
                match val {
                    OverseerValue::Date(ref d) => {
                        let nd =
                            chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").map_err(|_| {
                                OverseerError::FormulaError(
                                    "date_add_days: invalid date".to_string(),
                                )
                            })?;
                        let nd2 = nd + chrono::Duration::days(n_days);
                        Ok(OverseerValue::Date(nd2.format("%Y-%m-%d").to_string()))
                    }
                    OverseerValue::Timestamp(ref ts) | OverseerValue::String(ref ts) => {
                        // A bare date stays a date: adding a day to "2026-08-12" should not
                        // hand back an instant with a midnight in it.
                        if let Ok(nd) = chrono::NaiveDate::parse_from_str(ts.trim(), "%Y-%m-%d") {
                            let nd2 = nd + chrono::Duration::days(n_days);
                            return Ok(OverseerValue::Date(nd2.format("%Y-%m-%d").to_string()));
                        }
                        match Self::to_utc(&val) {
                            Some(dt) => Ok(OverseerValue::Timestamp(
                                (dt + chrono::Duration::days(n_days)).to_rfc3339(),
                            )),
                            None => Err(OverseerError::FormulaError(
                                "date_add_days: unable to parse timestamp/date".to_string(),
                            )),
                        }
                    }
                    _ => Err(OverseerError::FormulaError(
                        "date_add_days: unsupported argument type".to_string(),
                    )),
                }
            }
            // same_day(a, b): true if both timestamps/dates fall on the same calendar day (local time for timestamps)
            "same_day" => {
                if args.len() != 2 {
                    return Err(OverseerError::FormulaError(
                        "same_day(a, b) takes exactly 2 arguments".to_string(),
                    ));
                }
                let a = Self::evaluate_expression(&args[0], context)?;
                let b = Self::evaluate_expression(&args[1], context)?;
                if let (Some(da), Some(db)) = (Self::to_local_date(&a), Self::to_local_date(&b)) {
                    Ok(OverseerValue::Boolean(da == db))
                } else {
                    Err(OverseerError::FormulaError(
                        "same_day: unable to parse arguments as date/timestamp".to_string(),
                    ))
                }
            }
            // Which day of the week, of the month, or which month a date falls on.
            //
            // Enough to say "every Tuesday" or "the 7th of each month" in a formula, which
            // durations cannot express: months and years are not a fixed number of days, and
            // there is no modulo operator to derive a weekday from one.
            //
            // Weekdays are 1-7 from Monday, as ISO numbers them and as people say them.
            // Months are 1-12. Both read the date the way `same_day` does, in local time, so
            // "is it Tuesday" and "is it still the same day" never disagree about where a
            // midnight falls.
            "weekday" | "day_of_month" | "month_of" => {
                if args.len() != 1 {
                    return Err(OverseerError::FormulaError(format!(
                        "{}(x) takes exactly 1 argument",
                        name
                    )));
                }
                let val = Self::evaluate_expression(&args[0], context)?;
                let date = Self::to_local_date(&val).ok_or_else(|| {
                    OverseerError::FormulaError(format!(
                        "{}: unable to parse argument as date/timestamp",
                        name
                    ))
                })?;
                use chrono::Datelike;
                let n = match name {
                    "weekday" => date.weekday().num_days_from_monday() as i64 + 1,
                    "day_of_month" => date.day() as i64,
                    _ => date.month() as i64,
                };
                Ok(OverseerValue::Integer(n))
            }
            _ => Err(OverseerError::FormulaError(format!(
                "Unknown function: {}",
                name
            ))),
        }
    }

    /// An instant, however it was written.
    ///
    /// One parser for every date function, because they were three and they disagreed.
    /// `same_day` read "2026-08-12T09:17:00" quite happily while `days_since` refused it, so a
    /// task written by the bot sorted by a date the document could not subtract - and reported
    /// "invalid formula error" in the one field a person actually looks at. Whether a string is
    /// a timestamp is not a question two functions in the same document may answer differently.
    ///
    /// Accepted: RFC3339; a space in place of the `T`; a trailing `Z`; no zone at all; and a
    /// bare date, which is midnight.
    ///
    /// With no zone the instant is taken as UTC. It is genuinely ambiguous - someone writing a
    /// wall-clock time usually means their own - but UTC is what `same_day` has always assumed,
    /// and a silent change of meaning is worse than a documented guess. Anything writing a
    /// timestamp should include the offset; the guides say so.
    fn to_utc(v: &OverseerValue) -> Option<chrono::DateTime<chrono::Utc>> {
        use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
        let text = match v {
            OverseerValue::Date(d) => d,
            OverseerValue::Timestamp(ts) | OverseerValue::String(ts) => ts,
            _ => return None,
        };
        let txt = text.trim();
        if let Ok(dt) = DateTime::parse_from_rfc3339(txt) {
            return Some(dt.with_timezone(&Utc));
        }
        let mut patched = txt.replace('T', " ");
        if patched.ends_with('Z') {
            patched = patched.trim_end_matches('Z').trim_end().to_string();
        }
        for fmt in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"] {
            if let Ok(ndt) = NaiveDateTime::parse_from_str(&patched, fmt) {
                return Some(DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc));
            }
        }
        let nd = NaiveDate::parse_from_str(txt, "%Y-%m-%d").ok()?;
        Some(DateTime::<Utc>::from_naive_utc_and_offset(
            nd.and_hms_opt(0, 0, 0)?,
            Utc,
        ))
    }

    /// A date value as a local calendar date, however it was written.
    ///
    /// Timestamps are instants and dates are not, so turning one into the other needs a
    /// timezone; local is the one the person reading the document lives in.
    ///
    /// A bare date is taken as written rather than run through UTC and back: "2026-08-12" is
    /// the 12th to whoever wrote it, and west of Greenwich the round trip would make it the
    /// 11th.
    fn to_local_date(v: &OverseerValue) -> Option<chrono::NaiveDate> {
        use chrono::{DateTime, Local, NaiveDate};
        if let OverseerValue::Date(d) = v {
            return NaiveDate::parse_from_str(d.trim(), "%Y-%m-%d").ok();
        }
        if let OverseerValue::Timestamp(ts) | OverseerValue::String(ts) = v {
            if let Ok(nd) = NaiveDate::parse_from_str(ts.trim(), "%Y-%m-%d") {
                return Some(nd);
            }
        }
        Self::to_utc(v).map(|dt| DateTime::<Local>::from(dt).date_naive())
    }
}

impl FormulaEvaluator {
    fn value_to_string(v: &OverseerValue) -> String {
        match v {
            OverseerValue::Null => "".to_string(),
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
        let resolved_current =
            FormulaEvaluator::resolve_path_to_node(&node_path, document_root).unwrap_or(current);
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
    pub fn new_with_current(
        current_node: &'a OverseerNode,
        node_path: Vec<String>,
        document_root: &'a [OverseerNode],
    ) -> Self {
        Self {
            current_node,
            parent_node: None,
            document_root,
            node_path,
            var_bindings: std::collections::HashMap::new(),
        }
    }

    /// Construct a context with explicit parent reference.
    pub fn new_with_current_and_parent(
        current_node: &'a OverseerNode,
        parent_node: Option<&'a OverseerNode>,
        node_path: Vec<String>,
        document_root: &'a [OverseerNode],
    ) -> Self {
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
    fn resolve_path_to_node<'a>(
        segments: &[String],
        root: &'a [OverseerNode],
    ) -> Option<&'a OverseerNode> {
        if segments.is_empty() {
            return None;
        }
        // Helper: split a segment like "name#k" into (name, Some(k)) or (name, None)
        fn split_seg(seg: &str) -> (&str, Option<usize>) {
            if let Some((base, idx_str)) = seg.rsplit_once('#') {
                if let Ok(k) = idx_str.parse::<usize>() {
                    return (base, Some(k));
                }
            }
            (seg, None)
        }
        // First segment maps to a top-level node name (with optional ordinal)
        let (root_name, root_ord) = split_seg(&segments[0]);
        let mut cur_iter = root.iter().filter(|n| n.name == root_name);
        let mut current = if let Some(ord) = root_ord {
            cur_iter.nth(ord)?
        } else {
            cur_iter.next()?
        };
        for seg in &segments[1..] {
            let (name, ord) = split_seg(seg);
            // IMPORTANT: follow RAW children; disambiguate by ordinal among siblings of same name when provided
            let mut it = current.children.iter().filter(|c| c.name == name);
            if let Some(k) = ord {
                if let Some(next) = it.nth(k) {
                    current = next;
                } else {
                    return None;
                }
            } else if let Some(next) = it.next() {
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
                // Prefer computed/effective value if present
                if let Some(v) = Self::get_effective_param(&child.parameters, "value") {
                    return Some(v.clone());
                }
                // If value is Null and a fallback exists, try literal fallback here (formulas evaluated elsewhere)
                if let Some(fb) = child.parameters.get("_computed_fallback") {
                    return Some(fb.clone());
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
    // Helper to skip ASCII whitespaces (consistent with multispace0 for our inputs)
    fn skip_ws(s: &str) -> &str {
        let mut i = 0usize;
        for ch in s.chars() {
            if ch.is_whitespace() {
                i += ch.len_utf8();
            } else {
                break;
            }
        }
        &s[i..]
    }

    let (rest, cond) = logical_or_expression(input)?;
    // Try to parse '?' following the condition
    let r1 = skip_ws(rest);
    if r1.starts_with('?') {
        let mut after_q = &r1[1..];
        after_q = skip_ws(after_q);
        if after_q.starts_with(':') {
            // Elvis form: A ?: C  => then = A, else = C
            let mut after_colon = &after_q[1..];
            after_colon = skip_ws(after_colon);
            let (rest_after, else_e) = ternary_expression(after_colon)?;
            return Ok((
                rest_after,
                FormulaExpression::Conditional {
                    condition: Box::new(cond.clone()),
                    then_branch: Box::new(cond),
                    else_branch: Box::new(else_e),
                },
            ));
        } else {
            // Standard ternary: parse then and else branches
            let (after_then, then_e) = ternary_expression(after_q)?;
            let r2 = skip_ws(after_then);
            if !r2.starts_with(':') {
                return Err(nom::Err::Error(nom::error::Error {
                    input: r2,
                    code: nom::error::ErrorKind::Char,
                }));
            }
            let mut after_colon = &r2[1..];
            after_colon = skip_ws(after_colon);
            let (rest_after, else_e) = ternary_expression(after_colon)?;
            return Ok((
                rest_after,
                FormulaExpression::Conditional {
                    condition: Box::new(cond),
                    then_branch: Box::new(then_e),
                    else_branch: Box::new(else_e),
                },
            ));
        }
    }
    Ok((rest, cond))
}

/// Logical OR (||) with short-circuit semantics at evaluation time
fn logical_or_expression(input: &str) -> IResult<&str, FormulaExpression> {
    let (input, first) = logical_and_expression(input)?;
    let (input, ops) = many0(pair(
        delimited(multispace0, tag("||"), multispace0),
        logical_and_expression,
    ))(input)?;
    Ok((
        input,
        ops.into_iter()
            .fold(first, |acc, (_, expr)| FormulaExpression::BinaryOp {
                left: Box::new(acc),
                operator: BinaryOperator::Or,
                right: Box::new(expr),
            }),
    ))
}

/// Logical AND (&&)
fn logical_and_expression(input: &str) -> IResult<&str, FormulaExpression> {
    let (input, first) = comparison_expression(input)?;
    let (input, ops) = many0(pair(
        delimited(multispace0, tag("&&"), multispace0),
        comparison_expression,
    ))(input)?;
    Ok((
        input,
        ops.into_iter()
            .fold(first, |acc, (_, expr)| FormulaExpression::BinaryOp {
                left: Box::new(acc),
                operator: BinaryOperator::And,
                right: Box::new(expr),
            }),
    ))
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

    Ok((
        input,
        ops.into_iter().fold(first, |acc, (op, expr)| {
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
        }),
    ))
}

/// Parse addition and subtraction (lowest precedence)
fn additive_expression(input: &str) -> IResult<&str, FormulaExpression> {
    let (input, first) = multiplicative_expression(input)?;
    let (input, operations) = many0(pair(
        delimited(multispace0, alt((char('+'), char('-'))), multispace0),
        multiplicative_expression,
    ))(input)?;

    Ok((
        input,
        operations.into_iter().fold(first, |acc, (op, expr)| {
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
        }),
    ))
}

/// Parse multiplication and division (higher precedence)
fn multiplicative_expression(input: &str) -> IResult<&str, FormulaExpression> {
    let (input, first) = unary_expression(input)?;
    let (input, operations) = many0(pair(
        delimited(multispace0, alt((char('*'), char('/'))), multispace0),
        unary_expression,
    ))(input)?;

    Ok((
        input,
        operations.into_iter().fold(first, |acc, (op, expr)| {
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
        }),
    ))
}

/// Parse unary expressions like !expr
fn unary_expression(input: &str) -> IResult<&str, FormulaExpression> {
    let (input, bangs) = many0(delimited(multispace0, char('!'), multispace0))(input)?;
    let (input, mut expr) = primary_expression(input)?;
    // Parse zero or more method calls for chaining
    let (input, calls) = many0(method_call)(input)?;
    if !calls.is_empty() {
        expr = FormulaExpression::MethodChain {
            base: Box::new(expr),
            calls,
        };
    }
    // Optionally parse a parameter suffix like .color AFTER method calls
    let (input, param_opt) = opt(preceded(
        char('.'),
        take_while1(|c: char| c.is_alphanumeric() || c == '_' || c == '-'),
    ))(input)?;
    if let Some(param) = param_opt {
        let param = param.to_string();
        expr = match expr {
            FormulaExpression::FieldReference(name) => FormulaExpression::PathParam {
                path: vec![name],
                param,
            },
            FormulaExpression::PathReference(path) => FormulaExpression::PathParam { path, param },
            other => other,
        };
    }
    // Parse zero or more path tail segments like /child/grand after a method chain or any expr
    let (input, tail_segments) = many0(preceded(
        char('/'),
        take_while1(|c: char| c.is_alphanumeric() || c == '_' || c == '-'),
    ))(input)?;
    if !tail_segments.is_empty() {
        let segs = tail_segments
            .into_iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        expr = FormulaExpression::PathFollow {
            base: Box::new(expr),
            segments: segs,
        };
    }
    for _ in 0..bangs.len() {
        expr = FormulaExpression::UnaryOp {
            operator: UnaryOperator::Not,
            expr: Box::new(expr),
        };
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
            bool_literal,
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
        map(
            take_while1(|c: char| c.is_alphanumeric() || c == '_'),
            |s: &str| s.to_string(),
        ),
    )(input)?;
    let (input, _) = char('|')(input)?;
    let (input, body) = expression(input)?;
    Ok((
        input,
        FormulaExpression::Lambda {
            params,
            body: Box::new(body),
        },
    ))
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
    Ok((
        input,
        MethodCall {
            name: name.to_string(),
            args,
        },
    ))
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

/// Parse boolean literals true/false ensuring word boundary (not followed by ident char)
fn bool_literal(input: &str) -> IResult<&str, FormulaExpression> {
    // Match 'true' or 'false'
    let (rest, word) = alt((tag("true"), tag("false")))(input)?;
    // Ensure next char is not an identifier character (avoid consuming prefix of a longer identifier)
    let (_rest2, next) = opt(peek(take_while1(|c: char| c.is_alphanumeric() || c == '_')))(rest)?;
    if next.is_some() {
        // Fail this alternative so other parsers (e.g., field_reference) can match
        return Err(nom::Err::Error(nom::error::Error::new(
            rest,
            nom::error::ErrorKind::Alpha,
        )));
    }
    Ok((rest, FormulaExpression::BooleanLiteral(word == "true")))
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
    let (input, _) = char('/')(input)?;
    let (input, segments) = separated_list1(
        char('/'),
        take_while1(|c: char| c.is_alphanumeric() || c == '_' || c == '-'),
    )(input)?;
    if segments.is_empty() {
        return Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::SeparatedList,
        )));
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
    if segments.is_empty()
        || !segments[0].chars().next().unwrap_or('0').is_alphabetic()
            && segments[0].chars().next().unwrap_or('_') != '_'
    {
        return Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Alpha,
        )));
    }
    // Require at least two segments (must contain a slash)
    if segments.len() < 2 {
        return Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::SeparatedList,
        )));
    }
    Ok((
        input,
        FormulaExpression::PathReference(segments.iter().map(|s| s.to_string()).collect()),
    ))
}

// ================= Method chain evaluation helpers =================

#[derive(Debug, Clone)]
enum ListItem<'a> {
    Node(&'a OverseerNode),
    Value(OverseerValue),
}

impl FormulaEvaluator {
    /// What a node offers to a method chain.
    ///
    /// Ordinarily its children - `intake.map(...)` walks the entries of a list. A `tags` node
    /// has no children: it holds `"lidl,edeka,ikea"`, and what it means is those three values.
    /// Handing them over here is what lets every method that already exists work on a set of
    /// tags, rather than each of them needing to learn what a tag is.
    fn items_of(node: &OverseerNode) -> Vec<ListItem<'_>> {
        if node.node_type == "tags" {
            let text = match node
                .parameters
                .get("_computed_value")
                .or_else(|| node.parameters.get("value"))
            {
                Some(OverseerValue::String(s)) => s.clone(),
                _ => String::new(),
            };
            return text
                .split(',')
                .map(|tag| tag.trim())
                .filter(|tag| !tag.is_empty())
                .map(|tag| ListItem::Value(OverseerValue::String(tag.to_string())))
                .collect();
        }
        node.get_accessible_children()
            .into_iter()
            .map(ListItem::Node)
            .collect()
    }

    fn evaluate_method_chain(
        base: &FormulaExpression,
        calls: &[MethodCall],
        context: &EvaluationContext,
    ) -> Result<OverseerValue, OverseerError> {
        debug_evaluator!(
            "[EVAL] evaluate_method_chain base {:?} calls {:?}",
            base,
            calls
        );
        // Phase 1: special-case files(pattern) to support reducers without loading anything yet
        let mut list: Vec<ListItem> = if let FormulaExpression::FunctionCall { name, args } = base {
            if name == "files" {
                // Evaluate pattern arg to ensure syntax is valid; ignore actual results (stub)
                if let Some(arg0) = args.get(0) {
                    let _ = Self::evaluate_expression(arg0, context).ok();
                }
                Vec::new() // empty list stub; reducers will operate on empty list
            } else {
                // Fallback to node-based chaining
                let base_node = match Self::eval_expr_to_node(base, context) {
                    Some(n) => n,
                    None => return Ok(OverseerValue::String("null".to_string())),
                };
                Self::items_of(base_node)
            }
        } else {
            let base_node = match Self::eval_expr_to_node(base, context) {
                Some(n) => n,
                None => return Ok(OverseerValue::String("null".to_string())),
            };
            Self::items_of(base_node)
        };
        debug_evaluator!("[EVAL] Initial list size: {}", list.len());

        for call in calls {
            match call.name.as_str() {
                "map" => {
                    let lambda = call.args.get(0).ok_or_else(|| {
                        OverseerError::FormulaError("map() requires 1 argument".to_string())
                    })?;
                    let mut out: Vec<ListItem> = Vec::with_capacity(list.len());
                    for item in list.into_iter() {
                        let v_res = match &item {
                            ListItem::Node(n) => {
                                Self::eval_lambda(lambda, None, Some(*n), None, context)
                            }
                            ListItem::Value(val) => {
                                Self::eval_lambda(lambda, None, None, Some(val.clone()), context)
                            }
                        };
                        match v_res {
                            Ok(v) => out.push(ListItem::Value(v)),
                            Err(_) => {
                                out.push(ListItem::Value(OverseerValue::String("null".to_string())))
                            }
                        }
                    }
                    list = out;
                    debug_evaluator!("[EVAL] After map: list size {}", list.len());
                }
                "find" => {
                    // find(keyValue) — requires current list base to be a container with parameter 'key'
                    let key_val_expr = call.args.get(0).ok_or_else(|| {
                        OverseerError::FormulaError("find() requires 1 argument".to_string())
                    })?;
                    let target_key_value = Self::evaluate_expression(key_val_expr, context)?;
                    // Determine key field name from base container
                    let key_field = if let Some(base_node) = Self::eval_expr_to_node(base, context)
                    {
                        if let Some(OverseerValue::String(k)) = base_node.parameters.get("key") {
                            k.clone()
                        } else {
                            return Err(OverseerError::ValidationError(
                                "find() base does not declare a key parameter".to_string(),
                            ));
                        }
                    } else {
                        return Err(OverseerError::ValidationError(
                            "find() base must be a node (list/container)".to_string(),
                        ));
                    };
                    // Scan list for the first item whose child named key_field has value == target_key_value
                    let mut found: Option<ListItem> = None;
                    for item in list.into_iter() {
                        match &item {
                            ListItem::Node(n) => {
                                if let Some(child) = n
                                    .get_accessible_children()
                                    .into_iter()
                                    .find(|c| c.name == key_field)
                                {
                                    if let Some(v) =
                                        Self::get_effective_param(&child.parameters, "value")
                                    {
                                        // Relaxed equality: use compare_values to allow int<->string numeric equality, etc.
                                        if Self::compare_values(v, &target_key_value)
                                            .map(|ord| ord == std::cmp::Ordering::Equal)
                                            .unwrap_or(false)
                                        {
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
                    let lambda = call.args.get(0).ok_or_else(|| {
                        OverseerError::FormulaError("filter() requires 1 argument".to_string())
                    })?;
                    let mut out: Vec<ListItem> = Vec::new();
                    for item in list.into_iter() {
                        let keep_val = match &item {
                            ListItem::Node(n) => {
                                Self::eval_lambda(lambda, None, Some(*n), None, context)?
                            }
                            ListItem::Value(val) => {
                                Self::eval_lambda(lambda, None, None, Some(val.clone()), context)?
                            }
                        };
                        if Self::value_to_bool(&keep_val).unwrap_or(false) {
                            out.push(item);
                        }
                    }
                    list = out;
                    debug_evaluator!("[EVAL] After filter: list size {}", list.len());
                }
                "reduce" => {
                    if call.args.len() != 2 {
                        return Err(OverseerError::FormulaError(
                            "reduce(init, lambda) requires 2 args".to_string(),
                        ));
                    }
                    let init = Self::evaluate_expression(&call.args[0], context)?;
                    let lambda = &call.args[1];
                    let mut acc = init;
                    for item in list.into_iter() {
                        acc = match item {
                            ListItem::Node(n) => {
                                Self::eval_lambda(lambda, Some(acc), Some(n), None, context)?
                            }
                            ListItem::Value(v) => {
                                Self::eval_lambda(lambda, Some(acc), None, Some(v), context)?
                            }
                        };
                    }
                    debug_evaluator!("[EVAL] After reduce: {:?}", acc);
                    return Ok(acc);
                }
                "sum" => {
                    // Optional projection: sum(expr) — evaluate expr in each item's context
                    if call.args.len() == 1 {
                        let proj = &call.args[0];
                        let mut total: f64 = 0.0;
                        for item in &list {
                            // If projection is a lambda, evaluate it with the item bound; otherwise, preserve existing behavior
                            let v_res = match proj {
                                FormulaExpression::Lambda { .. } => match item {
                                    ListItem::Node(n) => {
                                        Self::eval_lambda(proj, None, Some(*n), None, context)
                                    }
                                    ListItem::Value(val) => Self::eval_lambda(
                                        proj,
                                        None,
                                        None,
                                        Some(val.clone()),
                                        context,
                                    ),
                                },
                                _ => match item {
                                    ListItem::Node(n) => {
                                        // Evaluate expression with current node = item
                                        let ctx = EvaluationContext::new_with_current_and_parent(
                                            *n,
                                            Self::eval_expr_to_node(base, context),
                                            {
                                                // Best-effort path: start from current path, append base and item names
                                                let mut p = context.node_path.clone();
                                                // Append base name if resolvable
                                                if let Some(bn) =
                                                    Self::eval_expr_to_node(base, context)
                                                {
                                                    p.push(bn.name.clone());
                                                }
                                                p.push(n.name.clone());
                                                p
                                            },
                                            context.document_root,
                                        );
                                        Self::evaluate_expression(proj, &ctx)
                                    }
                                    ListItem::Value(val) => {
                                        // Bind x to the value and evaluate
                                        let mut ctx = context.clone();
                                        ctx = ctx.with_var("x", BoundValue::Value(val.clone()));
                                        Self::evaluate_expression(proj, &ctx)
                                    }
                                },
                            };
                            if let Ok(v) = v_res {
                                if let Ok(n) = Self::value_to_number(&v) {
                                    if n.is_finite() {
                                        total += n;
                                    }
                                }
                            }
                        }
                        let res = Self::normalize_number(total);
                        debug_evaluator!("[EVAL] sum(arg) => {:?}", res);
                        return Ok(res);
                    } else {
                        // sum() without args: sum numeric items directly
                        let mut total: f64 = 0.0;
                        for item in &list {
                            if let Some(num) = Self::item_to_number(item) {
                                if num.is_finite() {
                                    total += num;
                                }
                            }
                        }
                        let res = Self::normalize_number(total);
                        debug_evaluator!("[EVAL] sum => {:?}", res);
                        return Ok(res);
                    }
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
                                Some(cur) => match Self::compare_values(&v, &cur) {
                                    Ok(std::cmp::Ordering::Greater) => v,
                                    _ => cur,
                                },
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
                                Some(cur) => match Self::compare_values(&v, &cur) {
                                    Ok(std::cmp::Ordering::Less) => v,
                                    _ => cur,
                                },
                            });
                        }
                    }
                    let res = best.unwrap_or_else(|| OverseerValue::String("null".to_string()));
                    debug_evaluator!("[EVAL] min => {:?}", res);
                    return Ok(res);
                }
                "avg" | "average" => {
                    // Support optional projection arg: avg(expr)
                    if let Some(proj) = call.args.get(0) {
                        let mut sum: f64 = 0.0;
                        let mut cnt: usize = 0;
                        for item in &list {
                            // If projection is a lambda, evaluate it with the item bound; otherwise, preserve existing behavior
                            let v_res = match proj {
                                FormulaExpression::Lambda { .. } => match item {
                                    ListItem::Node(n) => {
                                        Self::eval_lambda(proj, None, Some(*n), None, context)
                                    }
                                    ListItem::Value(val) => Self::eval_lambda(
                                        proj,
                                        None,
                                        None,
                                        Some(val.clone()),
                                        context,
                                    ),
                                },
                                _ => match item {
                                    ListItem::Node(n) => {
                                        // Evaluate expression with current node = item
                                        let ctx = EvaluationContext::new_with_current_and_parent(
                                            *n,
                                            Self::eval_expr_to_node(base, context),
                                            {
                                                let mut p = context.node_path.clone();
                                                if let Some(bn) =
                                                    Self::eval_expr_to_node(base, context)
                                                {
                                                    p.push(bn.name.clone());
                                                }
                                                p.push(n.name.clone());
                                                p
                                            },
                                            context.document_root,
                                        );
                                        Self::evaluate_expression(proj, &ctx)
                                    }
                                    ListItem::Value(val) => {
                                        let mut ctx = context.clone();
                                        ctx = ctx.with_var("x", BoundValue::Value(val.clone()));
                                        Self::evaluate_expression(proj, &ctx)
                                    }
                                },
                            };
                            if let Ok(v) = v_res {
                                if let Ok(n) = Self::value_to_number(&v) {
                                    if n.is_finite() {
                                        sum += n;
                                        cnt += 1;
                                    }
                                }
                            }
                        }
                        let res = if cnt == 0 {
                            OverseerValue::String("null".to_string())
                        } else {
                            OverseerValue::Float(sum / cnt as f64)
                        };
                        debug_evaluator!("[EVAL] avg(arg) => {:?}", res);
                        return Ok(res);
                    } else {
                        // No-arg: average numeric items directly
                        let mut sum: f64 = 0.0;
                        let mut cnt: usize = 0;
                        for item in &list {
                            if let Some(num) = Self::item_to_number(item) {
                                if num.is_finite() {
                                    sum += num;
                                    cnt += 1;
                                }
                            }
                        }
                        let res = if cnt == 0 {
                            OverseerValue::String("null".to_string())
                        } else {
                            OverseerValue::Float(sum / cnt as f64)
                        };
                        debug_evaluator!("[EVAL] avg => {:?}", res);
                        return Ok(res);
                    }
                }
                "first" => {
                    // Returns the first item in the list as a value; if Node, returns its effective value
                    // Optional arg first(default) to return default when list is empty
                    let default = if let Some(arg0) = call.args.get(0) {
                        Some(Self::evaluate_expression(arg0, context)?)
                    } else {
                        None
                    };
                    if let Some(item) = list.first() {
                        let v = match item {
                            ListItem::Value(v) => v.clone(),
                            ListItem::Node(n) => {
                                if let Some(v) = Self::get_effective_param(&n.parameters, "value") {
                                    v.clone()
                                } else {
                                    OverseerValue::String("null".to_string())
                                }
                            }
                        };
                        debug_evaluator!("[EVAL] first => {:?}", v);
                        return Ok(v);
                    }
                    let out = default.unwrap_or_else(|| OverseerValue::String("null".to_string()));
                    debug_evaluator!("[EVAL] first(empty) => {:?}", out);
                    return Ok(out);
                }
                other => {
                    return Err(OverseerError::FormulaError(format!(
                        "Unknown method: {}",
                        other
                    )))
                }
            }
        }

        Err(OverseerError::FormulaError(
            "Method chain must end with an aggregate or reduce".to_string(),
        ))
    }

    fn item_to_number(item: &ListItem) -> Option<f64> {
        match item {
            ListItem::Value(v) => Self::value_to_number(v).ok(),
            ListItem::Node(n) => {
                if let Some(v) = Self::get_effective_param(&n.parameters, "value") {
                    Self::value_to_number(v).ok()
                } else {
                    None
                }
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
                if let Some(v) = Self::get_effective_param(&n.parameters, "value") {
                    Some(v.clone())
                } else {
                    None
                }
            }
        }
    }

    fn normalize_number(n: f64) -> OverseerValue {
        if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
            OverseerValue::Integer(n as i64)
        } else {
            OverseerValue::Float(n)
        }
    }

    fn eval_expr_to_node<'a>(
        expr: &FormulaExpression,
        context: &'a EvaluationContext,
    ) -> Option<&'a OverseerNode> {
        match expr {
            FormulaExpression::PathReference(path) => Self::resolve_path_to_node_any(path, context),
            FormulaExpression::FieldReference(name) => {
                if let Some(BoundValue::Node(n)) = context.var_bindings.get(name) {
                    return Some(*n);
                }
                // First, try child of the current node (rare but valid)
                if let Some(child) = context
                    .current_node
                    .get_accessible_children()
                    .into_iter()
                    .find(|c| &c.name == name)
                {
                    return Some(child);
                }
                // Next, try immediate parent for siblings
                if let Some(parent) = context.parent_node {
                    if let Some(sib) = parent
                        .get_accessible_children()
                        .into_iter()
                        .find(|c| &c.name == name)
                    {
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
                            // At top-level: search among document root nodes (siblings of current)
                            if let Some(found) =
                                context.document_root.iter().find(|n| &n.name == name)
                            {
                                return Some(found);
                            }
                            break;
                        }
                        if let Some(parent) = FormulaEvaluator::resolve_path_to_node(
                            &context.node_path[..parent_end],
                            context.document_root,
                        ) {
                            if let Some(sib) = parent
                                .get_accessible_children()
                                .into_iter()
                                .find(|c| &c.name == name)
                            {
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
                            let key_field = if let Some(OverseerValue::String(k)) =
                                current_container.parameters.get("key")
                            {
                                k
                            } else {
                                return None;
                            };
                            // Find first child whose key_field child value == target
                            let mut found: Option<&OverseerNode> = None;
                            for n in nodes.into_iter() {
                                if let Some(ch) = n
                                    .get_accessible_children()
                                    .into_iter()
                                    .find(|c| &c.name == key_field)
                                {
                                    if let Some(v) =
                                        Self::get_effective_param(&ch.parameters, "value")
                                    {
                                        // Relaxed equality using compare_values
                                        if Self::compare_values(v, &target)
                                            .map(|ord| ord == std::cmp::Ordering::Equal)
                                            .unwrap_or(false)
                                        {
                                            found = Some(n);
                                            break;
                                        }
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
                        "filter" => {
                            // Keep only nodes matching predicate; maintain current_container context
                            let lambda = call.args.get(0)?;
                            let mut out: Vec<&OverseerNode> = Vec::new();
                            for n in nodes.into_iter() {
                                let item_ctx = EvaluationContext::new_with_current_and_parent(
                                    n,
                                    Some(current_container),
                                    context.node_path.clone(),
                                    context.document_root,
                                );
                                if let Ok(keep_v) =
                                    Self::eval_lambda(lambda, None, Some(n), None, &item_ctx)
                                {
                                    if Self::value_to_bool(&keep_v).unwrap_or(false) {
                                        out.push(n);
                                    }
                                }
                            }
                            nodes = out;
                        }
                        "first" => {
                            // Narrow to the first node if available (ignore default arg for node resolution)
                            if let Some(n) = nodes.first().cloned() {
                                nodes = vec![n];
                                current_container = n;
                            } else {
                                nodes = vec![];
                            }
                        }
                        _ => {
                            // Unsupported for node resolution; bail
                            return None;
                        }
                    }
                }
                // If chain narrowed to a single node, return it
                if nodes.len() == 1 {
                    Some(nodes[0])
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn resolve_path_to_node_any<'a>(
        path: &[String],
        context: &'a EvaluationContext,
    ) -> Option<&'a OverseerNode> {
        if path.is_empty() {
            return None;
        }
        if path[0] == "/" {
            // Find nearest ancestor containing first segment
            let segments = &path[1..];
            if segments.is_empty() {
                return None;
            }
            let mut start = context.current_node;
            let mut p_end = context.node_path.len();
            let mut found = false;
            while p_end > 0 {
                if let Some(candidate) = Self::resolve_path_to_node(
                    &context.node_path[..p_end].to_vec(),
                    context.document_root,
                ) {
                    if candidate
                        .get_accessible_children()
                        .into_iter()
                        .any(|c| c.name == segments[0])
                    {
                        start = candidate;
                        found = true;
                        break;
                    }
                }
                p_end -= 1;
            }
            let mut skip_first = false;
            if !found {
                if let Some(root_match) =
                    context.document_root.iter().find(|n| n.name == segments[0])
                {
                    start = root_match;
                    skip_first = true;
                }
            }
            let mut current = start;
            let start_idx = if skip_first { 1 } else { 0 };
            for seg in &segments[start_idx..] {
                current = current
                    .get_accessible_children()
                    .into_iter()
                    .find(|c| &c.name == seg)?;
            }
            return Some(current);
        }
        if path[0] == ".." {
            let mut hops = 1usize;
            let mut idx = 1usize;
            while idx < path.len() && path[idx] == ".." {
                hops += 1;
                idx += 1;
            }
            if context.node_path.len() < hops {
                return None;
            }
            let ancestor_segments = &context.node_path[..context.node_path.len() - hops];
            let mut current = Self::resolve_path_to_node(ancestor_segments, context.document_root)?;
            for seg in &path[idx..] {
                current = current
                    .get_accessible_children()
                    .into_iter()
                    .find(|c| &c.name == seg)?;
            }
            return Some(current);
        }
        // Identifier-based starting at a bound variable node, if present
        if let Some(BoundValue::Node(start)) = context.var_bindings.get(&path[0]) {
            let mut current = *start;
            for seg in &path[1..] {
                current = current
                    .get_accessible_children()
                    .into_iter()
                    .find(|c| &c.name == seg)?;
            }
            return Some(current);
        }
        // Fallback: search from nearest ancestor following child chain
        let mut end = context.node_path.len();
        while end > 0 {
            if let Some(mut node) =
                Self::resolve_path_to_node(&context.node_path[..end], context.document_root)
            {
                let mut ok = true;
                for seg in path {
                    if let Some(next) = node
                        .get_accessible_children()
                        .into_iter()
                        .find(|c| &c.name == seg)
                    {
                        node = next;
                    } else {
                        ok = false;
                        break;
                    }
                }
                if ok {
                    return Some(node);
                }
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
                    if let Some(n) = item_node {
                        ctx = ctx.with_var(&params[0], BoundValue::Node(n));
                    } else if let Some(v) = item_value.clone() {
                        ctx = ctx.with_var(&params[0], BoundValue::Value(v));
                    }
                } else if params.len() == 2 {
                    if let Some(acc) = acc_value.clone() {
                        ctx = ctx.with_var(&params[0], BoundValue::Value(acc));
                    }
                    if let Some(n) = item_node {
                        ctx = ctx.with_var(&params[1], BoundValue::Node(n));
                    } else if let Some(v) = item_value.clone() {
                        ctx = ctx.with_var(&params[1], BoundValue::Value(v));
                    }
                }
                debug_evaluator!(
                    "[EVAL] eval_lambda with params {:?}, bindings {:?}",
                    params,
                    ctx.var_bindings.keys().collect::<Vec<_>>()
                );
                Self::evaluate_expression(body, &ctx)
            }
            other => {
                // Allow non-lambda expressions as trivial mappers/predicates with item bound to 'x'
                let mut ctx = context.clone();
                if let Some(n) = item_node {
                    ctx = ctx.with_var("x", BoundValue::Node(n));
                }
                if let Some(v) = item_value {
                    ctx = ctx.with_var("x", BoundValue::Value(v));
                }
                debug_evaluator!(
                    "[EVAL] eval_lambda (implicit) with x bound, expr {:?}",
                    other
                );
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
                char(')'),
            ),
        )),
        |(name, args_opt): (&str, Option<Vec<FormulaExpression>>)| {
            FormulaExpression::FunctionCall {
                name: name.to_string(),
                args: args_opt.unwrap_or_else(|| vec![]),
            }
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
        let total = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "total")
            .unwrap();
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
        let record = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "Record")
            .unwrap();
        let desc = record
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "description")
            .unwrap();
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
        let record = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "Record")
            .unwrap();
        let desc = record
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "desc")
            .unwrap();
        let computed = desc.parameters.get("_computed_value").cloned().unwrap();
        assert_eq!(computed, OverseerValue::String("Squats".to_string()));
    }

    #[test]
    fn test_sum_with_projection_over_template_list() {
        let input = r#"
        div Root {
            div MealRecord (hidden=true) {
                string description = ""
                int calories = 0
            }
            div WeightRecord (hidden=true) {
                list intake (entry=<MealRecord>)
                int total_calories = $(intake.sum(calories))
            }
            list History (entry=<WeightRecord>) {
                - {
                    list intake {
                        - { string description = "a" int calories = 100 }
                        - { string description = "b" int calories = 200 }
                    }
                }
            }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let root = &nodes[0];
        let history = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "History")
            .unwrap();
        let day = history
            .get_accessible_children()
            .into_iter()
            .find(|c| c.node_type == "WeightRecord")
            .unwrap();
        let total = day
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "total_calories")
            .unwrap();
        let computed = total.parameters.get("_computed_value").cloned().unwrap();
        assert_eq!(computed, OverseerValue::Integer(300));
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
        let record = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "Record")
            .unwrap();
        let desc = record
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "desc")
            .unwrap();
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
        let record = tab
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "ExerciseRecord")
            .unwrap();
        let desc = record
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "description")
            .unwrap();
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
        let ok_count = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "ok_count")
            .unwrap();
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
        let total = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "total")
            .unwrap();
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
        let cnt = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "cnt")
            .unwrap();
        let computed = cnt.parameters.get("_computed_value").cloned().unwrap();
        assert_eq!(computed, OverseerValue::Integer(3));
    }

    #[test]
    fn test_weight_tracker_total_calories_in_history_entries() {
        // Mirrors examples/weight_tracker/weight_tracker.os semantics for History entries
        let input = r#"
        tab weight {
            div (hidden=true) {
                div MealRecord {
                    string description = ""
                    float calories = 100
                }
                div WeightRecord {
                    timestamp date
                    float weight = 108.3
                    int total_calories = $(intake.sum(calories))
                    list intake (entry=<MealRecord>)
                    text commentary = ""
                }
            }
            list History (entry=<WeightRecord>) {
                - {
                    list intake {
                        - { string description = "a" float calories = 300 }
                        - { string description = "b" float calories = 150 }
                    }
                }
                - {
                    // no intake -> total_calories should be 0
                }
            }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let tab = &nodes[0];
        let history = tab
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "History")
            .unwrap();
        let mut items = history
            .get_accessible_children()
            .into_iter()
            .filter(|c| c.node_type == "WeightRecord");
        let first = items.next().unwrap();
        let first_total = first
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "total_calories")
            .unwrap();
        let v1 = first_total
            .parameters
            .get("_computed_value")
            .cloned()
            .unwrap();
        assert_eq!(v1, OverseerValue::Integer(450));
        let second = items.next().unwrap();
        let second_total = second
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "total_calories")
            .unwrap();
        let v2 = second_total
            .parameters
            .get("_computed_value")
            .cloned()
            .unwrap();
        assert_eq!(v2, OverseerValue::Integer(0));
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
        let steps = nodes
            .iter()
            .find(|n| n.name == "Steps")
            .expect("Steps list not found at root");
        assert_eq!(steps.node_type, "list");
        assert_eq!(steps.name, "Steps");
        assert_eq!(steps.children.len(), 2);

        // First step instance should have 2 StepTasks
        let step_a = &steps.children[0];
        let a_steptasks = step_a
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "StepTasks")
            .unwrap();
        assert_eq!(a_steptasks.children.len(), 2);
        assert_eq!(a_steptasks.get_accessible_children().len(), 2);
        let a_complete = step_a
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "complete_tasks")
            .unwrap();
        let a_total = step_a
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "total_tasks")
            .unwrap();
        assert_eq!(
            a_complete.parameters.get("_computed_value"),
            Some(&OverseerValue::Integer(1))
        );
        assert_eq!(
            a_total.parameters.get("_computed_value"),
            Some(&OverseerValue::Integer(2))
        );

        // Second step instance should have 3 StepTasks
        let step_b = &steps.children[1];
        let b_steptasks = step_b
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "StepTasks")
            .unwrap();
        assert_eq!(b_steptasks.children.len(), 3);
        assert_eq!(b_steptasks.get_accessible_children().len(), 3);
        let b_complete = step_b
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "complete_tasks")
            .unwrap();
        let b_total = step_b
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "total_tasks")
            .unwrap();
        assert_eq!(
            b_complete.parameters.get("_computed_value"),
            Some(&OverseerValue::Integer(2))
        );
        assert_eq!(
            b_total.parameters.get("_computed_value"),
            Some(&OverseerValue::Integer(3))
        );
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
        let done = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "done")
            .unwrap();
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
        let ex = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "Exercise")
            .unwrap();
        let last_done = ex
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "last_done")
            .unwrap();
        let computed = last_done
            .parameters
            .get("_computed_value")
            .cloned()
            .unwrap();
        assert_eq!(
            computed,
            OverseerValue::String("2025-08-12T10:53:02.760779800+00:00".to_string())
        );
    }

    #[test]
    fn test_circular_dependency_yields_null() {
        let input = r#"
        div Root {
            int a = $(b)
            int b = $(a)
        }
        "#;
        let mut nodes = crate::parser::parse_document(input).unwrap().1;
        crate::resolver::resolve_document(&mut nodes);
        let root = &nodes[0];
        let a = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "a")
            .unwrap();
        let b = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "b")
            .unwrap();
        // Both should resolve to Null due to cycle
        assert_eq!(
            a.parameters.get("_computed_value").cloned(),
            Some(OverseerValue::Null)
        );
        assert_eq!(
            b.parameters.get("_computed_value").cloned(),
            Some(OverseerValue::Null)
        );
    }

    #[test]
    fn test_fallback_used_when_value_null() {
        let input = r#"
        div Root {
            string name (fallback="Untitled") = null
            string shown = $(name)
        }
        "#;
        let mut nodes = crate::parser::parse_document(input).unwrap().1;
        crate::resolver::resolve_document(&mut nodes);
        let root = &nodes[0];
        let name = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "name")
            .unwrap();
        // Effective value should use fallback
        let eff = name
            .parameters
            .get("_computed_value")
            .or_else(|| name.parameters.get("_computed_fallback"))
            .cloned();
        assert_eq!(eff, Some(OverseerValue::String("Untitled".to_string())));
        // And the dependent should see the same string
        let shown = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "shown")
            .unwrap();
        assert_eq!(
            shown.parameters.get("_computed_value").cloned(),
            Some(OverseerValue::String("Untitled".to_string()))
        );
    }

    #[test]
    fn test_elvis_operator_shortcut() {
        let input = r#"
        div Root {
            string a = null
            string b = $(a ?: "x")
        }
        "#;
        let mut nodes = crate::parser::parse_document(input).unwrap().1;
        crate::resolver::resolve_document(&mut nodes);
        let root = &nodes[0];
        let b = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "b")
            .unwrap();
        assert_eq!(
            b.parameters.get("_computed_value").cloned(),
            Some(OverseerValue::String("x".to_string()))
        );
    }

    #[test]
    fn test_elvis_with_false_boolean_falls_back() {
        let content = r#"
        div Root {
            bool a = false
            string b = $(a ?: "x")
        }
        "#;
        let mut nodes = crate::parser::parse_document(content).unwrap().1;
        crate::resolver::resolve_document(&mut nodes);
        let root = &nodes[0];
        let b = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "b")
            .unwrap();
        let eff = b
            .parameters
            .get("_computed_value")
            .or_else(|| b.parameters.get("_computed_fallback"))
            .cloned();
        match eff {
            Some(OverseerValue::String(s)) => assert_eq!(s, "x"),
            other => panic!("unexpected value: {:?}", other),
        }
    }

    #[test]
    fn test_elvis_with_zero_integer_falls_back() {
        let content = r#"
        div Root {
            int a = 0
            int b = $(a ?: 42)
        }
        "#;
        let mut nodes = crate::parser::parse_document(content).unwrap().1;
        crate::resolver::resolve_document(&mut nodes);
        let root = &nodes[0];
        let b = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "b")
            .unwrap();
        let eff = b
            .parameters
            .get("_computed_value")
            .or_else(|| b.parameters.get("_computed_fallback"))
            .cloned();
        match eff {
            Some(OverseerValue::Integer(i)) => assert_eq!(i, 42),
            other => panic!("unexpected value: {:?}", other),
        }
    }

    #[test]
    fn test_elvis_with_empty_string_falls_back() {
        let content = r#"
        div Root {
            string a = ""
            string b = $(a ?: "fallback")
        }
        "#;
        let mut nodes = crate::parser::parse_document(content).unwrap().1;
        crate::resolver::resolve_document(&mut nodes);
        let root = &nodes[0];
        let b = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "b")
            .unwrap();
        let eff = b
            .parameters
            .get("_computed_value")
            .or_else(|| b.parameters.get("_computed_fallback"))
            .cloned();
        match eff {
            Some(OverseerValue::String(s)) => assert_eq!(s, "fallback"),
            other => panic!("unexpected value: {:?}", other),
        }
    }

    #[test]
    fn test_elvis_with_nonempty_string_keeps_value() {
        let content = r#"
        div Root {
            string a = "hello"
            string b = $(a ?: "fallback")
        }
        "#;
        let mut nodes = crate::parser::parse_document(content).unwrap().1;
        crate::resolver::resolve_document(&mut nodes);
        let root = &nodes[0];
        let b = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "b")
            .unwrap();
        let eff = b
            .parameters
            .get("_computed_value")
            .or_else(|| b.parameters.get("_computed_fallback"))
            .cloned();
        match eff {
            Some(OverseerValue::String(s)) => assert_eq!(s, "hello"),
            other => panic!("unexpected value: {:?}", other),
        }
    }

    #[test]
    fn test_top_level_reference_and_path_multiplication() {
        // Verifies: int a1 = $(b1*D/c1) resolves at the document root
        let input = r#"
        div Root {
            int a1 = $(b1*D/c1)
            int b1 = 2
            div D { int c1 = 10 }
        }
        "#;
        let mut nodes = crate::parser::parse_document(input).unwrap().1;
        crate::resolver::resolve_document(&mut nodes);
        let root = &nodes[0];
        let a1 = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "a1")
            .unwrap();
        let v = a1.parameters.get("_computed_value").cloned().unwrap();
        assert_eq!(v, OverseerValue::Integer(20));
    }

    #[test]
    fn test_mealcalc_calories_fallback_uses_nested_path() {
        // Minimal repro of weight_tracker MealCalc scenario
        let input = r#"
        div MealCalc {
            float calories (fallback=$(item*per_item/cal)) = null
            int item = 1
            div per_item {
                int cal (fallback=$(weight)) = null
                float weight = 100
            }
        }
        "#;
        let mut nodes = crate::parser::parse_document(input).unwrap().1;
        crate::resolver::resolve_document(&mut nodes);
        let meal = &nodes[0];
        let calories = meal
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "calories")
            .unwrap();
        // Effective value should come from fallback: item(1) * per_item/cal(100) = 100
        let eff = calories
            .parameters
            .get("_computed_value")
            .or_else(|| calories.parameters.get("_computed_fallback"))
            .cloned()
            .unwrap();
        match eff {
            OverseerValue::Integer(i) => assert_eq!(i, 100),
            OverseerValue::Float(f) => assert!((f - 100.0).abs() < 1e-9),
            other => panic!("unexpected value for calories: {:?}", other),
        }
    }

    #[test]
    fn test_top_level_roots_field_and_path_resolution() {
        // Reproduce examples/basic/fallback_formulas.os top-level case (no enclosing container)
        let input = r#"
        int a1 = $(b1*D/c1)
        int b1 = 2
        div D { int c1 = 10 }
        "#;
        let mut nodes = crate::parser::parse_document(input).unwrap().1;
        crate::resolver::resolve_document(&mut nodes);
        // Find the 'a1' node among top-level roots
        let a1 = nodes
            .iter()
            .find(|n| n.name == "a1")
            .expect("a1 not found at root");
        let v = a1.parameters.get("_computed_value").cloned().unwrap();
        assert_eq!(v, OverseerValue::Integer(20));
    }

    #[test]
    fn test_date_add_days_on_date_and_timestamp() {
        let input = r#"
        div Root {
            date d1 = $(date_add_days("2024-08-10", 2))
            timestamp t1 = $(date_add_days("2024-08-10T05:30:00Z", 1))
            date d2 = $(date_add_days("2024-08-10", -10))
        }
        "#;
        let mut nodes = crate::parser::parse_document(input).unwrap().1;
        crate::resolver::resolve_document(&mut nodes);
        let root = &nodes[0];
        let d1 = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "d1")
            .unwrap();
        let t1 = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "t1")
            .unwrap();
        let d2 = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "d2")
            .unwrap();
        assert_eq!(
            d1.parameters.get("_computed_value"),
            Some(&OverseerValue::Date("2024-08-12".to_string()))
        );
        match t1.parameters.get("_computed_value").cloned().unwrap() {
            OverseerValue::Timestamp(s) => assert!(s.starts_with("2024-08-11T")),
            other => panic!("unexpected: {:?}", other),
        }
        assert_eq!(
            d2.parameters.get("_computed_value"),
            Some(&OverseerValue::Date("2024-07-31".to_string()))
        );
    }
}
