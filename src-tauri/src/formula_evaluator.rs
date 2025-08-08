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

/// Context for formula evaluation, tracking current position in document tree
#[derive(Debug, Clone)]
pub struct EvaluationContext<'a> {
    pub current_node: &'a OverseerNode,
    #[allow(dead_code)]
    pub parent_node: Option<&'a OverseerNode>,
    pub document_root: &'a [OverseerNode],
    pub node_path: Vec<String>, // Path from root to current node for debugging
}

/// Represents a parsed formula expression
#[derive(Debug, Clone)]
pub enum FormulaExpression {
    Number(f64),
    StringLiteral(String),
    FieldReference(String),
    PathReference(Vec<String>), // e.g., ["..","field"] for ../field
    PathParam { path: Vec<String>, param: String },
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
    /// Helper: get effective parameter value preferring computed shadow
    fn get_effective_param<'p>(params: &'p std::collections::HashMap<String, OverseerValue>, key: &str) -> Option<&'p OverseerValue> {
        if key == "value" {
            if let Some(v) = params.get("_computed_value") { return Some(v); }
        } else {
            let shadow = format!("_computed_{}", key);
            if let Some(v) = params.get(&shadow) { return Some(v); }
        }
        params.get(key)
    }
    /// Evaluate a formula expression string within the given context
    pub fn evaluate_formula(
        formula: &str,
        context: &EvaluationContext,
    ) -> Result<OverseerValue, OverseerError> {
        // Parse the formula expression
        let expression = Self::parse_expression(formula)
            .map_err(|_| OverseerError::FormulaError("invalid formula error".to_string()))?;

        // Evaluate the parsed expression
        Self::evaluate_expression(&expression, context)
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
                Self::resolve_field_reference(field_name, context)
            }
            FormulaExpression::PathReference(path) => {
                Self::resolve_path_reference(path, context)
            }
            FormulaExpression::PathParam { path, param } => {
                Self::resolve_path_param(path, param, context)
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
                        Self::apply_binary_operator(&left_val, operator, &right_val)
                    }
                }
            }
            FormulaExpression::FunctionCall { name, args } => {
                Self::evaluate_function_call(name, args, context)
            }
            FormulaExpression::Conditional { condition, then_branch, else_branch } => {
                let cond_val = Self::evaluate_expression(condition, context)?;
                let cond_bool = Self::value_to_bool(&cond_val)?;
                if cond_bool {
                    Self::evaluate_expression(then_branch, context)
                } else {
                    Self::evaluate_expression(else_branch, context)
                }
            }
        }
    }

    /// Resolve a simple field reference within the current node
    fn resolve_field_reference(
        field_name: &str,
        context: &EvaluationContext,
    ) -> Result<OverseerValue, OverseerError> {
        // Strategy:
        // 1) Try current node's parameters by exact field name
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
            if let Some(val) = Self::get_effective_param(&child.parameters, "value") {
                if let OverseerValue::Formula(formula_expr) = val {
                    // On-demand evaluate this child's formula
                    let mut path = context.node_path.clone();
                    path.push(child.name.clone());
                    let child_ctx = EvaluationContext::new_with_current(child, path, context.document_root);
                    return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
                }
                return Ok(val.clone());
            }
            // If no value found but there's a formula stored directly
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
                    // a) Ancestor parameters by key
                    if let Some(val) = Self::get_effective_param(&ancestor.parameters, field_name) {
                        if let OverseerValue::Formula(formula_expr) = val {
                            // Evaluate formula for ancestor parameter on-demand
                            let child_ctx = EvaluationContext::new(context.node_path.clone(), context.document_root);
                            return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
                        }
                        return Ok(val.clone());
                    }
                    // b) Child of ancestor by name
                    if let Some(child) = ancestor.get_accessible_children().into_iter().find(|c| c.name == field_name) {
                        if let Some(val) = Self::get_effective_param(&child.parameters, "value") {
                            if let OverseerValue::Formula(formula_expr) = val {
                                let mut path = ancestor_path.to_vec();
                                path.push(child.name.clone());
                                let child_ctx = EvaluationContext::new_with_current(child, path, context.document_root);
                                return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
                            }
                            return Ok(val.clone());
                        }
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
        if path.is_empty() {
            return Err(OverseerError::FormulaError("Empty path reference".to_string()));
        }

        // Case 0: "/"-anchored paths (within current node)
        if path[0] == "/" {
            let segments = &path[1..];
            if segments.is_empty() {
                return Err(OverseerError::FormulaError("Path missing target after /".to_string()));
            }

            // Start from current node
            let mut current = context.current_node;
            // Traverse all but last via children names
            for seg in &segments[..segments.len().saturating_sub(1)] {
                if let Some(next) = current.get_accessible_children().into_iter().find(|c| &c.name == seg) {
                    current = next;
                } else {
                    return Err(OverseerError::FormulaError(format!("Path segment not found: {}", seg)));
                }
            }
            let last = segments.last().unwrap();
            // Final segment can be either a parameter on current or a child node's value
            if let Some(val) = Self::get_effective_param(&current.parameters, last) {
                if let OverseerValue::Formula(formula_expr) = val {
                    // Evaluate parameter formula on-demand using exact node reference
                    let child_ctx = EvaluationContext::new_with_current(current, context.node_path.clone(), context.document_root);
                    return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
                }
                return Ok(val.clone());
            }
            if let Some(child) = current.get_accessible_children().into_iter().find(|c| &c.name == last) {
                if let Some(v) = Self::get_effective_param(&child.parameters, "value") {
                    if let OverseerValue::Formula(formula_expr) = v {
                        let mut path = context.node_path.clone();
                        path.push(child.name.clone());
                        let child_ctx = EvaluationContext::new_with_current(child, path, context.document_root);
                        return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
                    }
                    return Ok(v.clone());
                }
                if let Some(OverseerValue::Formula(formula_expr)) = child.parameters.get("value") {
                    let mut path = context.node_path.clone();
                    path.push(child.name.clone());
                    let child_ctx = EvaluationContext::new_with_current(child, path, context.document_root);
                    return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
                }
            }
            // Fallback: deep search within current subtree
            if let Some(v) = FormulaEvaluator::find_value_by_name_deep(current, last) {
                return Ok(v);
            }
            return Err(OverseerError::FormulaError(format!("Unknown field '{}' at current path", last)));
        }

        // Case A: identifier-based relative path (e.g., GrandChild/Inner/x)
        if path[0] != ".." {
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
        if path.is_empty() {
            return Err(OverseerError::FormulaError("Empty path for param extraction".to_string()));
        }
        // Current-node anchored path: ["/", seg1, seg2, ...]
        if path[0] == "/" {
            let segments = &path[1..];
            if segments.is_empty() {
                return Err(OverseerError::FormulaError("Path missing target after / for param".to_string()));
            }
            // Traverse from current node
            let mut node = context.current_node;
            for seg in segments {
                if let Some(next) = node.get_accessible_children().into_iter().find(|c| &c.name == seg) {
                    node = next;
                } else {
                    return Err(OverseerError::FormulaError(format!("Path segment not found: {}", seg)));
                }
            }
            if let Some(v) = Self::get_effective_param(&node.parameters, param) {
                if let OverseerValue::Formula(formula_expr) = v {
                    let mut p = context.node_path.clone();
                    p.extend(segments.iter().cloned());
                    let child_ctx = EvaluationContext::new(p, context.document_root);
                    return FormulaEvaluator::evaluate_formula(formula_expr, &child_ctx);
                }
                return Ok(Self::value_for_param_extraction(v));
            }
            return Err(OverseerError::FormulaError(format!("Parameter '{}' not found on node", param)));
        }
        // Identifier-based path from nearest ancestor
        if path[0] != ".." {
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
        // For comparisons and arithmetic, convert both to numbers for now (step 4.1 scope)
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
            _ => Err(OverseerError::FormulaError("Cannot convert value to bool".to_string())),
        }
    }

    /// Evaluate a function call
    fn evaluate_function_call(
        name: &str,
        args: &[FormulaExpression],
        _context: &EvaluationContext,
    ) -> Result<OverseerValue, OverseerError> {
        match name {
            "today" => {
                if !args.is_empty() {
                    return Err(OverseerError::FormulaError("today() function takes no arguments".to_string()));
                }
                let today = chrono::Local::now().format("%Y-%m-%d").to_string();
                Ok(OverseerValue::Date(today))
            }
            _ => Err(OverseerError::FormulaError(format!("Unknown function: {}", name))),
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

    // Optionally parse a parameter suffix like .color
    let (input, param_opt) = opt(preceded(char('.'), take_while1(|c: char| c.is_alphanumeric() || c == '_' || c == '-')))(input)?;
    if let Some(param) = param_opt {
        let param = param.to_string();
        return match atom {
            FormulaExpression::FieldReference(name) => Ok((input, FormulaExpression::PathParam { path: vec![name], param })),
            FormulaExpression::PathReference(path) => Ok((input, FormulaExpression::PathParam { path, param })),
            other => Ok((input, other)),
        };
    }
    Ok((input, atom))
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
                    take_while1(|c: char| c.is_alphanumeric() || c == '_'),
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
        take_while1(|c: char| c.is_alphanumeric() || c == '_'),
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
        take_while1(|c: char| c.is_alphanumeric() || c == '_'),
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

/// Parse function calls like today()
fn function_call(input: &str) -> IResult<&str, FormulaExpression> {
    map(
        tuple((

            take_while1(|c: char| c.is_alphabetic() || c == '_'),
            delimited(char('('), multispace0, char(')')),

        )),
        |(name, _): (&str, &str)| FormulaExpression::FunctionCall {
            name: name.to_string(),
            args: vec![], // No arguments for now
        },
    )(input)
}
