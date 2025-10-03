use crate::source_registry::SourceRegistry;
use crate::types::{
    OverseerNode,
    OverseerValue,
    Color,
    CssSize,
    BorderStyle,
    NodeSourceSnapshot,
    NodeHeaderSnapshot,
    NodeBodySnapshot,
    SourceSlice,
    SnapshotOrigin,
};
use nom::{
    branch::alt,
    bytes::complete::{tag, take_until},
    character::complete::{alpha1, alphanumeric1, char, multispace0, multispace1},
    combinator::{map, opt, recognize},
    multi::{many0, separated_list0},
    sequence::{delimited, pair, preceded, tuple},
    IResult,
};

// Debug logging macro for parser
macro_rules! debug_parser {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-parser")]
        println!($($arg)*);
    };
}
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hasher;
use twox_hash::XxHash64;

#[derive(Clone, Copy, Debug)]
struct ParserInputContext {
    base_ptr: usize,
    len: usize,
}

thread_local! {
    static PARSER_CONTEXT_STACK: RefCell<Vec<ParserInputContext>> = RefCell::new(Vec::new());
}

#[derive(Debug)]
struct ParsedNode {
    node: OverseerNode,
    snapshot: ParsedNodeSnapshotData,
}

#[derive(Debug)]
struct ParsedNodeSnapshotData {
    template_span: Option<(usize, usize)>,
    type_span: Option<(usize, usize)>,
    name_span: Option<(usize, usize)>,
    params_span: Option<(usize, usize)>,
    header_gap_span: Option<(usize, usize)>,
    value_span: Option<(usize, usize)>,
    assignment_span: Option<(usize, usize)>,
    block_open_span: Option<(usize, usize)>,
    block_close_span: Option<(usize, usize)>,
    header_end_ptr: usize,
    body_start_ptr: usize,
    body_end_ptr: usize,
    trailing_start_ptr: usize,
    trailing_end_ptr: usize,
    trailing_whitespace_bytes: usize,
}

fn count_blank_lines_in_trivia(trivia: &str) -> u8 {
    let mut count: u8 = 0;
    let ends_with_newline = trivia.ends_with('\n');
    let mut iter = trivia.split_terminator('\n').peekable();
    while let Some(segment) = iter.next() {
        let is_last_segment_without_newline = !ends_with_newline && iter.peek().is_none();
        if is_last_segment_without_newline {
            break;
        }
        if segment.trim().is_empty() {
            count = count.saturating_add(1);
        }
    }
    count
}

#[derive(Debug)]
struct ValueAssignmentCapture {
    value: OverseerValue,
    raw_literal: Option<String>,
    assignment_span: (usize, usize),
    value_span: (usize, usize),
}

#[derive(Debug)]
struct DirectValueCapture {
    value: OverseerValue,
    span: (usize, usize),
}

#[derive(Debug)]
struct BlockCapture {
    children: Vec<OverseerNode>,
    open_span: (usize, usize),
    close_span: (usize, usize),
}

enum BodyKind {
    None,
    ValueAssignment(ValueAssignmentCapture),
    DirectValue(DirectValueCapture),
    Block(BlockCapture),
}

struct ParserContextGuard;

impl ParserContextGuard {
    fn push(input: &str) -> Self {
        let ctx = ParserInputContext {
            base_ptr: input.as_ptr() as usize,
            len: input.len(),
        };
        PARSER_CONTEXT_STACK.with(|stack| stack.borrow_mut().push(ctx));
        ParserContextGuard
    }
}

impl Drop for ParserContextGuard {
    fn drop(&mut self) {
        PARSER_CONTEXT_STACK.with(|stack| {
            let mut stack = stack.borrow_mut();
            stack.pop();
        });
    }
}

fn current_parser_context() -> Option<ParserInputContext> {
    PARSER_CONTEXT_STACK.with(|stack| stack.borrow().last().copied())
}

fn slice_from_offsets(ctx: ParserInputContext, start: usize, end: usize) -> String {
    if end <= start || end > ctx.len {
        return String::new();
    }
    unsafe {
        let ptr = (ctx.base_ptr + start) as *const u8;
        let len = end - start;
        let bytes = std::slice::from_raw_parts(ptr, len);
        std::str::from_utf8(bytes).unwrap().to_string()
    }
}

fn assign_snapshot(
    node: &mut OverseerNode,
    leading_ptr: usize,
    start_ptr: usize,
    meta: &ParsedNodeSnapshotData,
) {
    let end_ptr = meta.trailing_end_ptr;
    if end_ptr < start_ptr {
        return;
    }
    let Some(ctx) = current_parser_context() else { return; };
    if start_ptr < ctx.base_ptr || end_ptr > ctx.base_ptr + ctx.len {
        return;
    }
    let span_start = start_ptr - ctx.base_ptr;
    let span_end = end_ptr - ctx.base_ptr;
    let leading_start = if leading_ptr < start_ptr { leading_ptr - ctx.base_ptr } else { span_start };
    let leading_end = if leading_ptr < start_ptr { span_start } else { span_start };
    let leading_span = if leading_start < leading_end {
        Some((leading_start, leading_end))
    } else {
        None
    };

    let to_offset = |ptr: usize| -> usize {
        if ptr <= ctx.base_ptr {
            0
        } else {
            (ptr - ctx.base_ptr).min(ctx.len)
        }
    };

    let header_start = span_start;
    let mut header_end = to_offset(meta.header_end_ptr);
    if header_end < header_start { header_end = header_start; }
    if header_end > span_end { header_end = span_end; }

    let body_span = if meta.body_end_ptr > meta.body_start_ptr {
        let mut start = to_offset(meta.body_start_ptr);
        let mut end = to_offset(meta.body_end_ptr);
        if start < span_start { start = span_start; }
        if end > span_end { end = span_end; }
        if end > start { Some((start, end)) } else { None }
    } else {
        None
    };

    let trailing_span = if end_ptr > meta.trailing_start_ptr {
        let mut start = to_offset(meta.trailing_start_ptr);
        if start < span_start { start = span_start; }
        let end = span_end;
        if end > start { Some((start, end)) } else { None }
    } else {
        None
    };

    let full_text = slice_from_offsets(ctx, span_start, span_end);
    let leading_trivia = if let Some((ls, le)) = leading_span {
        slice_from_offsets(ctx, ls, le)
    } else {
        String::new()
    };
    let trailing_trivia = if let Some((ts, te)) = trailing_span {
        slice_from_offsets(ctx, ts, te)
    } else {
        String::new()
    };
    let newline = if full_text.contains("\r\n") {
        Some("\r\n".to_string())
    } else if full_text.contains('\n') {
        Some("\n".to_string())
    } else if leading_trivia.contains("\r\n") {
        Some("\r\n".to_string())
    } else if leading_trivia.contains('\n') {
        Some("\n".to_string())
    } else {
        None
    };
    let indent_from_full_text = full_text
        .lines()
        .next()
        .and_then(|line| {
            let indent: String = line
                .chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .collect();
            if indent.is_empty() { None } else { Some(indent) }
        });
    let indent_from_leading = {
        let tail = leading_trivia
            .rsplit_once('\n')
            .map(|(_, tail)| tail)
            .unwrap_or_else(|| leading_trivia.as_str());
        let indent: String = tail
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        if indent.is_empty() { None } else { Some(indent) }
    };
    let indent_unit = indent_from_full_text.or(indent_from_leading);

    let mut hasher = XxHash64::with_seed(0);
    hasher.write(leading_trivia.as_bytes());
    hasher.write(full_text.as_bytes());
    let fingerprint = hasher.finish();

        let make_slice = |span: Option<(usize, usize)>| -> Option<SourceSlice> {
            span.and_then(|(s_ptr, e_ptr)| {
                if e_ptr <= s_ptr { return None; }
                let start = to_offset(s_ptr);
                let end = to_offset(e_ptr);
                if end <= start { return None; }
                let text = slice_from_offsets(ctx, start, end);
                Some(SourceSlice { span: (start, end), text })
            })
        };

        let header_trailing = meta
            .header_gap_span
            .and_then(|(s_ptr, e_ptr)| {
                if e_ptr <= s_ptr { return None; }
                let start = to_offset(s_ptr);
                let end = to_offset(e_ptr);
                if end <= start { return None; }
                Some(slice_from_offsets(ctx, start, end))
            })
            .unwrap_or_default();

        let mut header_snapshot = NodeHeaderSnapshot::default();
        header_snapshot.template = make_slice(meta.template_span);
        header_snapshot.type_token = make_slice(meta.type_span);
        header_snapshot.name = make_slice(meta.name_span);
        header_snapshot.parameters = make_slice(meta.params_span);
        header_snapshot.assignment_operator = make_slice(meta.assignment_span);
        header_snapshot.trailing = header_trailing;

        let mut body_snapshot = NodeBodySnapshot::default();
        body_snapshot.value = make_slice(meta.value_span);
        body_snapshot.child_envelope.open = make_slice(meta.block_open_span);
        body_snapshot.child_envelope.close = make_slice(meta.block_close_span);

    let snapshot = NodeSourceSnapshot {
        span: (span_start, span_end),
        leading_span,
        header_span: (header_start, header_end),
        body_span,
        trailing_span,
        full_text,
        leading_trivia,
            header: header_snapshot,
            body: body_snapshot,
        trailing_trivia,
        indent_unit,
        newline,
        fingerprint,
            origin: SnapshotOrigin::Parsed,
    };
    node.source_id = Some(SourceRegistry::register(&snapshot));
    node.source_snapshot = Some(snapshot);
    node.source_fingerprint = Some(fingerprint);
}

fn finalize_parsed_node(mut parsed: ParsedNode, leading_ptr: usize, start_ptr: usize) -> OverseerNode {
    assign_snapshot(&mut parsed.node, leading_ptr, start_ptr, &parsed.snapshot);
    parsed.node
}

/// Parse the entire document (top-level nodes)
pub fn parse_document(input: &str) -> IResult<&str, Vec<OverseerNode>> {
    let _ctx_guard = ParserContextGuard::push(input);
    #[cfg(test)]
    let _registry_guard = crate::source_registry::REGISTRY_TEST_MUTEX.lock();
    SourceRegistry::reset();
    // Loop similar to previous many0(parse_node) but augmented to capture the count of
    // contiguous blank (whitespace-only) lines immediately preceding each parsed node.
    // We continue to ignore comment lines for blank-line counting so that stylistic
    // vertical spacing authored purely with empty lines is preserved while comments are
    // replayed directly from stored trivia during serialization.
    let mut nodes: Vec<OverseerNode> = Vec::new();
    let mut cur = input;
    let mut pending_whitespace_bytes: usize = 0;
    loop {
        // Skip EOF / pure whitespace remainder
        if cur.trim().is_empty() { break; }

    let (after_comments, _) = match skip_comments_and_whitespace(cur) { Ok(t) => t, Err(_) => (cur, ()) };
    let consumed_len = cur.len().saturating_sub(after_comments.len());
    let consumed_trivia = &cur[..consumed_len];
        let cur_ptr = cur.as_ptr() as usize;
        let leading_ptr = cur_ptr.saturating_sub(pending_whitespace_bytes);
        pending_whitespace_bytes = 0;
        // Attempt parse at after_comments
        match parse_node_with_meta(after_comments) {
            Ok((rest, parsed)) => {
                    let trailing_whitespace_bytes = parsed.snapshot.trailing_whitespace_bytes;
                let node_start_ptr = after_comments.as_ptr() as usize;
                let mut node = finalize_parsed_node(parsed, leading_ptr, node_start_ptr);
                let mut leading_trivia_blanks = node
                    .source_snapshot
                    .as_ref()
                    .map(|snap| count_blank_lines_in_trivia(&snap.leading_trivia))
                    .unwrap_or(0);
                if let Some(snap) = node.source_snapshot.as_ref() {
                    let trimmed_start = snap.leading_trivia.trim_start_matches(|c| c == '\r' || c == '\n');
                    if trimmed_start.trim_start().starts_with("//") && leading_trivia_blanks > 0 {
                        leading_trivia_blanks = leading_trivia_blanks.saturating_sub(1);
                    }
                }
                node.leading_blank_lines = leading_trivia_blanks;
                nodes.push(node);
                    pending_whitespace_bytes = trailing_whitespace_bytes;
                cur = rest;
            }
            Err(_) => {
                if !consumed_trivia.is_empty() {
                    SourceRegistry::append_document_trailing(consumed_trivia);
                }
                // Failed to parse a node; consume one physical line from original cursor to avoid infinite loop
                if let Some(pos) = cur.find('\n') { cur = &cur[pos+1..]; } else { break; }
            }
        }
    }
    // Post-parse enhancement: heuristic pass to improve authored_dash detection for nested dash override lines.
    // Rationale: The original source may contain blocks like:
    //   div per_item {\n        - calories = 410\n        - weight = 300\n   }
    // Parser already marks nodes whose explicit type token was '-' (list items / anonymous blocks) as authored_dash.
    // However, value override lines inside template/list instance override sections that were authored with dash syntax
    // but later resolved/serialized differently (e.g., due to template inference) might lose dash provenance if their
    // node_type was inferred rather than a raw '-'. To preserve round-trip textual fidelity we attempt to detect
    // additional candidates: nodes that (a) have only a 'value' parameter (plus internal markers), (b) no children,
    // (c) siblings in the same block include at least one already dash-authored node, and (d) the parent itself is
    // not a plain list body (where '-' would denote list items instead). We then mark them authored_dash=true so the
    // serializer can consider concise emission when other gates pass.
    fn enhance_authored_dash(nodes: &mut [OverseerNode]) {
        for n in nodes.iter_mut() {
            // Recurse first so child context is available
            if !n.children.is_empty() { enhance_authored_dash(&mut n.children); }
            // Nothing special at this node level beyond recursion; heuristic operates at each block among siblings
            if n.children.len() > 0 { continue; }
        }
        // Second pass per sibling group: we need sibling context, so operate on the slice passed in.
        let mut any_dash = false;
        for c in nodes.iter() { if c.authored_dash { any_dash = true; break; } }
        if any_dash {
            for c in nodes.iter_mut() {
                if c.authored_dash { continue; }
                if !c.children.is_empty() { continue; }
                // Count non-internal, non-value params
                let mut extra_params = 0usize;
                for (k, _v) in c.parameters.iter() {
                    let ks = k.as_str();
                    if ks == "value" { continue; }
                    if ks.starts_with('_') { continue; }
                    extra_params += 1;
                    if extra_params > 0 { break; }
                }
                if extra_params == 0 && c.parameters.contains_key("value") {
                    // Candidate simple value override lacking explicit dash token; mark it to allow concise emission later
                    c.authored_dash = true;
                }
            }
        }
    }
    enhance_authored_dash(&mut nodes);
    Ok((cur, nodes))
}

/// Skip comments and whitespace
fn skip_comments_and_whitespace(input: &str) -> IResult<&str, ()> {
    map(
        many0(alt((
            skip_single_line_comment,
            skip_multi_line_comment,
            skip_whitespace,
        ))),
        |_| (),
    )(input)
}

/// Skip single line comment // comment
fn skip_single_line_comment(input: &str) -> IResult<&str, ()> {
    use nom::bytes::complete::is_not;
    map(
        preceded(tag("//"), is_not("\r\n")),
        |_| (),
    )(input)
}

/// Skip multi-line comment /* comment */
fn skip_multi_line_comment(input: &str) -> IResult<&str, ()> {
    map(delimited(tag("/*"), take_until("*/"), tag("*/")), |_| ())(input)
}

/// Skip whitespace
fn skip_whitespace(input: &str) -> IResult<&str, ()> {
    map(multispace1, |_| ())(input)
}

/// Parse a single node
fn parse_node_with_meta(input: &str) -> IResult<&str, ParsedNode> {
    // Only skip lines that start with '=' and are not part of a value assignment after a type or identifier
    let trimmed = input.trim_start();
    if trimmed.starts_with('=') {
        debug_parser!("[PARSER] Skipping invalid node start: {}", trimmed.chars().take(40).collect::<String>());
        return Err(nom::Err::Failure(nom::error::Error::new(input, nom::error::ErrorKind::Tag)));
    }
    // Debug: print the input being parsed (disabled by default)
    debug_parser!("[PARSER] input: {}", input.chars().take(80).collect::<String>());

    let mut cur = input;
    let mut template_span = None;
    let mut type_span = None;
    let mut name_span = None;
    let mut params_span = None;
    let mut header_gap_span = None;
    let mut value_span = None;
    let mut assignment_span = None;
    let mut block_open_span = None;
    let mut block_close_span = None;

    // Parse template or type token
    let token_start_ptr = cur.as_ptr() as usize;
    let (next, (template_val_opt, node_type_opt)) = match alt((
        map(parse_template_value, |p| (Some(p), None)),
        map(parse_node_type, |t| (None, Some(t.to_string()))),
    ))(cur) {
        Ok(res) => res,
        Err(e) => {
            if !cur.trim().is_empty() {
                debug_parser!("[PARSER] Failed to parse node type: {:?}", e);
            }
            return Err(e);
        }
    };
    let token_end_ptr = next.as_ptr() as usize;
    let mut template_value: Option<OverseerValue> = None;
    let mut node_type_value: Option<String> = None;
    if let Some(template_val) = template_val_opt {
        template_span = Some((token_start_ptr, token_end_ptr));
        template_value = Some(template_val);
    }
    if let Some(node_type_str) = node_type_opt {
        type_span = Some((token_start_ptr, token_end_ptr));
        node_type_value = Some(node_type_str);
    }
    cur = next;
    let (next, _) = multispace0(cur)?;
    cur = next;

    // Parse optional name
    let name_start_ptr = cur.as_ptr() as usize;
    let (after_name, name_opt) = opt(parse_identifier)(cur)?;
    let mut node_name_value: Option<String> = None;
    cur = after_name;
    if let Some(name_token) = name_opt {
        node_name_value = Some(name_token.to_string());
        let name_end_ptr = cur.as_ptr() as usize;
        name_span = Some((name_start_ptr, name_end_ptr));
    }
    let (next, _) = multispace0(cur)?;
    cur = next;

    debug_parser!("[PARSER] Before parsing parameters, input: {}", cur.chars().take(50).collect::<String>());
    let params_start_ptr = cur.as_ptr() as usize;
    let (after_params, parameters_with_order) = opt(parse_parameters)(cur)?;
    cur = after_params;
    if parameters_with_order.is_some() {
        let params_end_ptr = cur.as_ptr() as usize;
        params_span = Some((params_start_ptr, params_end_ptr));
    }
    debug_parser!("[PARSER] After parsing parameters, input: {}", cur.chars().take(50).collect::<String>());

    let header_end_ptr = cur.as_ptr() as usize;
    let (next, _) = multispace0(cur)?;
    let header_gap_end_ptr = next.as_ptr() as usize;
    if header_gap_end_ptr > header_end_ptr {
        header_gap_span = Some((header_end_ptr, header_gap_end_ptr));
    }
    cur = next;

    // Parse body
    debug_parser!("[PARSER] Before parsing body, input: {}", cur.chars().take(50).collect::<String>());
    let body_start_ptr = cur.as_ptr() as usize;
    let body_kind = if let Ok((after_assign, capture)) = parse_value_assignment_capture(cur) {
        cur = after_assign;
        BodyKind::ValueAssignment(capture)
    } else if let Ok((after_block, capture)) = parse_block_capture(cur) {
        cur = after_block;
        BodyKind::Block(capture)
    } else if let Ok((after_value, capture)) = parse_direct_value_capture(cur) {
        cur = after_value;
        BodyKind::DirectValue(capture)
    } else {
        BodyKind::None
    };
    let body_end_ptr = cur.as_ptr() as usize;
    let trailing_start_ptr = body_end_ptr;
    debug_parser!("[PARSER] After parsing body, input: {}", cur.chars().take(50).collect::<String>());
    let (next, _) = multispace0(cur)?;
    let trailing_end_ptr = next.as_ptr() as usize;
    let consumed_len = cur.len() - next.len();
    let trailing_whitespace_bytes = consumed_len;
    cur = next;

    let mut children: Vec<OverseerNode> = Vec::new();
    let mut raw_value_literal: Option<String> = None;
    let mut value_parameter: Option<OverseerValue> = None;

    match body_kind {
        BodyKind::ValueAssignment(capture) => {
            assignment_span = Some(capture.assignment_span);
            value_span = Some(capture.value_span);
            raw_value_literal = capture.raw_literal;
            value_parameter = Some(capture.value);
        }
        BodyKind::DirectValue(capture) => {
            value_span = Some(capture.span);
            value_parameter = Some(capture.value);
        }
        BodyKind::Block(capture) => {
            block_open_span = Some(capture.open_span);
            block_close_span = Some(capture.close_span);
            children = capture.children;
        }
        BodyKind::None => {}
    }

    let template_path = template_value.as_ref().and_then(|val| {
        if let OverseerValue::Template(t) = val { Some(t.clone()) } else { None }
    });

    let final_node_type = if let Some(nt) = node_type_value.clone() {
        nt
    } else if let Some(t) = &template_path {
        t.split('/').last().unwrap_or_default().to_string()
    } else {
        "".to_string()
    };

    let mut node = OverseerNode::new_with_type(final_node_type.clone(), node_name_value);
    node.template = template_path;
    if let Some((param_map, order)) = parameters_with_order {
        node.param_order = order;
        node.parameters = param_map;
    }
    node.children = children;

    if final_node_type == "-" {
        node.authored_dash = true;
    }

    if let Some(val) = value_parameter {
        node.parameters.insert("value".to_string(), val);
    }
    if let Some(raw) = raw_value_literal {
        node.raw_value_literal = Some(raw);
    }

    debug_parser!("[PARSER] Parsed node: type='{}', name='{}'", node.node_type, node.name);
    if !node.parameters.is_empty() {
        debug_parser!("[PARSER]   Parameters: {:?}", node.parameters);
    }

    Ok((cur, ParsedNode {
        node,
        snapshot: ParsedNodeSnapshotData {
            template_span,
            type_span,
            name_span,
            params_span,
            header_gap_span,
            value_span,
            assignment_span,
            block_open_span,
            block_close_span,
            header_end_ptr,
            body_start_ptr,
            body_end_ptr,
            trailing_start_ptr,
            trailing_end_ptr,
            trailing_whitespace_bytes,
        },
    }))
}

#[cfg(test)]
fn parse_node(input: &str) -> IResult<&str, OverseerNode> {
    map(parse_node_with_meta, |parsed| parsed.node)(input)
}

/// Parse a node type, which is an identifier or a hyphen for inference
fn parse_node_type(input: &str) -> IResult<&str, &str> {
    alt((parse_identifier, tag("-")))(input)
}

/// Parse a template path like <../Task>
fn parse_template_value(input: &str) -> IResult<&str, OverseerValue> {
    map(delimited(char('<'), take_until(">"), char('>')), |s: &str| OverseerValue::Template(s.to_string()))(input)
}

/// Parse node parameters like (param=value, param2=value2) returning (map, order)
fn parse_parameters(input: &str) -> IResult<&str, (HashMap<String, OverseerValue>, Vec<String>)> {
    map(
        delimited(
            char('('),
            separated_list0(
                preceded(multispace0, char(',')),
                preceded(multispace0, parse_parameter),
            ),
            preceded(multispace0, char(')')),
        ),
        |params: Vec<(String, OverseerValue)>| {
            let mut map = HashMap::new();
            let mut order = Vec::new();
            for (k,v) in params { order.push(k.clone()); map.insert(k,v); }
            (map, order)
        },
    )(input)
}

/// Parse a single parameter (key=value)
fn parse_parameter(input: &str) -> IResult<&str, (String, OverseerValue)> {
    map(
        pair(
            parse_identifier,
            preceded(pair(multispace0, char('=')), preceded(multispace0, parse_value)),
        ),
        |(key, value)| (key.to_string(), value),
    )(input)
}

/// Parse value assignment (= value) capturing source spans
fn parse_value_assignment_capture(input: &str) -> IResult<&str, ValueAssignmentCapture> {
    let (after_ws, _) = multispace0(input)?;
    let equals_token_start = after_ws.as_ptr() as usize;
    let (after_eq, _) = char('=')(after_ws)?;
    let (after_gap, _) = multispace0(after_eq)?;
    let value_start_ptr = after_gap.as_ptr() as usize;

    if let Ok((after_value, (value, raw_literal))) = parse_number_value_with_raw(after_gap) {
        let value_end_ptr = after_value.as_ptr() as usize;
        return Ok((after_value, ValueAssignmentCapture {
            value,
            raw_literal,
            assignment_span: (equals_token_start, value_start_ptr),
            value_span: (value_start_ptr, value_end_ptr),
        }));
    }

    let (after_value, value) = parse_value(after_gap)?;
    let value_end_ptr = after_value.as_ptr() as usize;
    Ok((after_value, ValueAssignmentCapture {
        value,
        raw_literal: None,
        assignment_span: (equals_token_start, value_start_ptr),
        value_span: (value_start_ptr, value_end_ptr),
    }))
}

fn parse_number_value_with_raw(input: &str) -> IResult<&str, (OverseerValue, Option<String>)> {
    let start_len = input.len();
    let (remaining, val) = parse_number_value(input)?; // reuse existing logic
    let consumed = &input[..start_len - remaining.len()];
    // Only keep raw if float with trailing zeros preserved in source but lost by f64 to_string
    let raw = match &val {
        OverseerValue::Float(_f) => {
            if consumed.contains('.') && consumed.ends_with('0') {
                Some(consumed.to_string())
            } else { None }
        },
        _ => None,
    };
    Ok((remaining, (val, raw)))
}

/// Parse a value directly (without =) - only for quoted strings, numbers, booleans, etc.
fn parse_direct_value_capture(input: &str) -> IResult<&str, DirectValueCapture> {
    let start_ptr = input.as_ptr() as usize;
    let (remaining, value) = parse_value(input)?;
    let trimmed_remaining = remaining.trim_start();
    if let Some(next) = trimmed_remaining.chars().next() {
        let is_comment = next == '/' && (trimmed_remaining.starts_with("//") || trimmed_remaining.starts_with("/*"));
        if !is_comment {
            return Err(nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Tag)));
        }
    }
    let end_ptr = remaining.as_ptr() as usize;
    Ok((remaining, DirectValueCapture { value, span: (start_ptr, end_ptr) }))
}

/// Parse a block { ... } capturing brace spans
fn parse_block_capture(input: &str) -> IResult<&str, BlockCapture> {
    let (after_ws, _) = multispace0(input)?;
    let open_start_ptr = after_ws.as_ptr() as usize;
    let (mut cursor, _) = char('{')(after_ws)?;
    let open_end_ptr = cursor.as_ptr() as usize;
    let mut children: Vec<OverseerNode> = Vec::new();
    let mut pending_whitespace_bytes: usize = 0;

    loop {
    let (after_comments, _) = match skip_comments_and_whitespace(cursor) { Ok(t) => t, Err(_) => (cursor, ()) };
        let cursor_ptr = cursor.as_ptr() as usize;
        let leading_ptr = cursor_ptr.saturating_sub(pending_whitespace_bytes);
        pending_whitespace_bytes = 0;
        let node_start_ptr = after_comments.as_ptr() as usize;

        if let Some(rest) = after_comments.strip_prefix('}') {
            for (idx, ch) in children.iter_mut().enumerate() {
                ch.child_original_index = Some(idx);
            }
            let close_start_ptr = after_comments.as_ptr() as usize;
            let close_end_ptr = rest.as_ptr() as usize;
            return Ok((rest, BlockCapture {
                children,
                open_span: (open_start_ptr, open_end_ptr),
                close_span: (close_start_ptr, close_end_ptr),
            }));
        }

        match parse_node_with_meta(after_comments) {
            Ok((rest, parsed)) => {
                let trailing_whitespace_bytes = parsed.snapshot.trailing_whitespace_bytes;
                let mut node = finalize_parsed_node(parsed, leading_ptr, node_start_ptr);
                let mut leading_trivia_blanks = node
                    .source_snapshot
                    .as_ref()
                    .map(|snap| count_blank_lines_in_trivia(&snap.leading_trivia))
                    .unwrap_or(0);
                if let Some(snap) = node.source_snapshot.as_ref() {
                    let trimmed_start = snap.leading_trivia.trim_start_matches(|c| c == '\r' || c == '\n');
                    if trimmed_start.trim_start().starts_with("//") && leading_trivia_blanks > 0 {
                        leading_trivia_blanks = leading_trivia_blanks.saturating_sub(1);
                    }
                }
                node.leading_blank_lines = leading_trivia_blanks;
                cursor = rest;
                children.push(node);
                pending_whitespace_bytes = trailing_whitespace_bytes;
            }
            Err(_) => {
                if cursor.is_empty() {
                    break;
                }
                cursor = &cursor[1..];
            }
        }
    }

    Err(nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Char)))
}

/// Parse different types of values
fn parse_value(input: &str) -> IResult<&str, OverseerValue> {
    alt((
        parse_template_value,
        parse_null_value,
        parse_formula_value,
        parse_boolean_value,
        parse_border_style_value, // Parse border styles
        parse_color_value,
        parse_css_size_value, // Must come before number parsing to handle "10px" correctly
        parse_number_value,
        parse_quoted_string_value,
        parse_unquoted_string_value, // Must be last as it's a fallback
    ))(input)
}

/// Parse null literal
fn parse_null_value(input: &str) -> IResult<&str, OverseerValue> {
    map(tag("null"), |_| OverseerValue::Null)(input)
}

/// Parse formula like $(expression) with support for nested parentheses
fn parse_formula_value(input: &str) -> IResult<&str, OverseerValue> {
    // Expect the opening '$( '
    let (after_open, _) = tag("$(")(input)?;

    // Scan until matching closing ')' accounting for nested parentheses
    let bytes = after_open.as_bytes();
    let mut depth: i32 = 1; // we've consumed one '('
    let mut idx: usize = 0;
    while idx < bytes.len() {
        let c = bytes[idx] as char;
        if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
            if depth == 0 {
                // content is everything before this ')'
                let content = &after_open[..idx];
                let remaining = &after_open[idx + 1..];
                return Ok((remaining, OverseerValue::Formula(content.to_string())));
            }
        }
        idx += 1;
    }

    // If we reach here, no matching ')'
    Err(nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Char)))
}

/// Parse quoted strings
fn parse_quoted_string_value(input: &str) -> IResult<&str, OverseerValue> {
    map(
        delimited(char('"'), take_until("\""), char('"')),
        |s: &str| OverseerValue::String(s.to_string()),
    )(input)
}

/// Parse numbers (int or float)
fn parse_number_value(input: &str) -> IResult<&str, OverseerValue> {
    // Recognize a number pattern first to avoid ambiguity between int and float.
    // This is more robust than alt((double, i64)) because i64 could partially
    // parse a float, and double could parse an integer as a float.
    let (remaining, number_str) = recognize(
        pair(
            opt(char('-')),
            pair(
                nom::character::complete::digit1,
                opt(preceded(char('.'), nom::character::complete::digit1))
            )
        )
    )(input)?;

    // If the recognized string contains a '.', it's a float. Otherwise, it's an integer.
    if number_str.contains('.') {
        match number_str.parse::<f64>() {
            Ok(f) => Ok((remaining, OverseerValue::Float(f))),
            Err(_) => Err(nom::Err::Failure(nom::error::Error::new(input, nom::error::ErrorKind::Float)))
        }
    } else {
        match number_str.parse::<i64>() {
            Ok(i) => Ok((remaining, OverseerValue::Integer(i))),
            Err(_) => Err(nom::Err::Failure(nom::error::Error::new(input, nom::error::ErrorKind::Digit)))
        }
    }
}

/// Parse boolean values
fn parse_boolean_value(input: &str) -> IResult<&str, OverseerValue> {
    alt((
        map(tag("true"), |_| OverseerValue::Boolean(true)),
        map(tag("false"), |_| OverseerValue::Boolean(false)),
    ))(input)
}

/// Parse unquoted strings (identifiers, paths, etc.)
fn parse_unquoted_string_value(input: &str) -> IResult<&str, OverseerValue> {
    map(parse_identifier, |s| OverseerValue::String(s.to_string()))(input)
}

/// Parse color values in various formats
fn parse_color_value(input: &str) -> IResult<&str, OverseerValue> {
    alt((
        parse_hex_color,
        parse_rgb_color,
        parse_named_color,
    ))(input)
}

/// Parse hex color like #FF0000 or #ff0000
fn parse_hex_color(input: &str) -> IResult<&str, OverseerValue> {
    use nom::character::complete::hex_digit1;
    map(
        preceded(
            char('#'), 
            recognize(hex_digit1)
        ),
        |hex: &str| {
            if hex.len() == 3 || hex.len() == 6 {
                OverseerValue::Color(Color::Hex(format!("#{}", hex)))
            } else {
                // Invalid hex color, treat as string
                OverseerValue::String(format!("#{}", hex))
            }
        }
    )(input)
}

/// Parse RGB color like rgb(0.2, 0.8, 0.5)
fn parse_rgb_color(input: &str) -> IResult<&str, OverseerValue> {
    use nom::number::complete::float;
    map(
        delimited(
            tag("rgb("),
            tuple((
                preceded(multispace0, float),
                preceded(preceded(multispace0, char(',')), preceded(multispace0, float)),
                preceded(preceded(multispace0, char(',')), preceded(multispace0, float)),
            )),
            preceded(multispace0, char(')')),
        ),
        |(r, g, b)| OverseerValue::Color(Color::Rgb(r, g, b))
    )(input)
}

/// Parse named colors like red, blue, green
fn parse_named_color(input: &str) -> IResult<&str, OverseerValue> {
    map(
        alt((
            // Primary colors
            alt((tag("red"), tag("green"), tag("blue"), tag("yellow"), tag("orange"))),
            // Secondary colors  
            alt((tag("purple"), tag("pink"), tag("brown"), tag("black"), tag("white"))),
            // Tertiary colors
            alt((tag("gray"), tag("grey"), tag("cyan"), tag("magenta"), tag("lime"))),
            // Additional colors
            alt((tag("maroon"), tag("navy"), tag("olive"), tag("teal"), tag("silver"))),
            // Final colors
            alt((tag("aqua"), tag("fuchsia"), tag("transparent")))
        )),
        |color: &str| OverseerValue::Color(Color::Named(color.to_string()))
    )(input)
}

/// Parse CSS size values with units
fn parse_css_size_value(input: &str) -> IResult<&str, OverseerValue> {
    alt((
        parse_css_size_with_unit,
        parse_css_size_keywords,
    ))(input)
}

/// Parse CSS size with unit like 16px, 1.2em, 50%
fn parse_css_size_with_unit(input: &str) -> IResult<&str, OverseerValue> {
    // Use our own number parsing that matches the existing behavior
    map(
        pair(
            recognize(
                pair(
                    opt(char('-')),
                    pair(
                        nom::character::complete::digit1,
                        opt(preceded(char('.'), nom::character::complete::digit1))
                    )
                )
            ),
            parse_css_unit
        ),
        |(number_str, unit)| {
            let value = number_str.parse::<f32>().unwrap_or(0.0);
            let size = match unit {
                "px" => CssSize::Pixels(value),
                "%" => CssSize::Percentage(value),
                "em" => CssSize::Em(value),
                "rem" => CssSize::Rem(value),
                "vw" => CssSize::ViewportWidth(value),
                "vh" => CssSize::ViewportHeight(value),
                _ => return OverseerValue::String(format!("{}{}", value, unit)), // Invalid unit, treat as string
            };
            OverseerValue::CssSize(size)
        }
    )(input)
}

/// Parse CSS unit suffixes
fn parse_css_unit(input: &str) -> IResult<&str, &str> {
    alt((
        tag("px"), tag("em"), tag("rem"), tag("vh"), tag("vw"), tag("%")
    ))(input)
}

/// Parse CSS size keywords like auto, fit-content
fn parse_css_size_keywords(input: &str) -> IResult<&str, OverseerValue> {
    map(
        alt((
            tag("auto"),
            tag("fit-content"),
        )),
        |keyword: &str| {
            let size = match keyword {
                "auto" => CssSize::Auto,
                "fit-content" => CssSize::FitContent,
                _ => return OverseerValue::String(keyword.to_string()),
            };
            OverseerValue::CssSize(size)
        }
    )(input)
}

/// Parse border style values
fn parse_border_style_value(input: &str) -> IResult<&str, OverseerValue> {
    alt((
        // Parse "none"
        map(tag("none"), |_| OverseerValue::BorderStyle(BorderStyle::None)),
        // Parse "default"
        map(tag("default"), |_| OverseerValue::BorderStyle(BorderStyle::Default)),
        // Parse "solid thickness color" format
        map(
            tuple((
                tag("solid"),
                preceded(multispace1, parse_css_size_value),
                preceded(multispace1, parse_color_value),
            )),
            |(_, thickness, color)| {
                let thickness_size = match thickness {
                    OverseerValue::CssSize(size) => size,
                    _ => CssSize::Pixels(1.0), // fallback
                };
                let border_color = match color {
                    OverseerValue::Color(color) => color,
                    _ => Color::Named("black".to_string()), // fallback
                };
                OverseerValue::BorderStyle(BorderStyle::Solid(thickness_size, border_color))
            }
        ),
        // Parse "dashed thickness color" format
        map(
            tuple((
                tag("dashed"),
                preceded(multispace1, parse_css_size_value),
                preceded(multispace1, parse_color_value),
            )),
            |(_, thickness, color)| {
                let thickness_size = match thickness {
                    OverseerValue::CssSize(size) => size,
                    _ => CssSize::Pixels(1.0), // fallback
                };
                let border_color = match color {
                    OverseerValue::Color(color) => color,
                    _ => Color::Named("black".to_string()), // fallback
                };
                OverseerValue::BorderStyle(BorderStyle::Dashed(thickness_size, border_color))
            }
        ),
        // Parse "dotted thickness color" format
        map(
            tuple((
                tag("dotted"),
                preceded(multispace1, parse_css_size_value),
                preceded(multispace1, parse_color_value),
            )),
            |(_, thickness, color)| {
                let thickness_size = match thickness {
                    OverseerValue::CssSize(size) => size,
                    _ => CssSize::Pixels(1.0), // fallback
                };
                let border_color = match color {
                    OverseerValue::Color(color) => color,
                    _ => Color::Named("black".to_string()), // fallback
                };
                OverseerValue::BorderStyle(BorderStyle::Dotted(thickness_size, border_color))
            }
        ),
    ))(input)
}

/// Parse identifiers (variable names, node types, etc.)
fn parse_identifier(input: &str) -> IResult<&str, &str> {
    recognize(
        pair(
            alt((alpha1, tag("_"))),
            many0(alt((alphanumeric1, tag("_"), tag("."), tag("/"), tag("-")))),
        ),
    )(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::SourceRegistry;

    #[test]
    fn test_parse_simple_node() {
        let input = r#"string Name = "TestValue""#;
        let result = parse_node(input);
        assert!(result.is_ok());
        
        let (remaining, node) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(node.node_type, "string");
        assert_eq!(node.name, "Name");
        assert_eq!(
            node.parameters.get("value"),
            Some(&OverseerValue::String("TestValue".to_string()))
        );
    }
    
    #[test]
    fn test_parse_node_with_block() {
        let input = r#"div Task { string name = "My Task" }"#;
        let result = parse_node(input);
        assert!(result.is_ok());
        let (remaining, node) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(node.node_type, "div");
        assert_eq!(node.name, "Task");
        assert_eq!(node.children.len(), 1);
        assert_eq!(node.children[0].node_type, "string");
        assert_eq!(node.children[0].name, "name");
    }

    #[test]
    fn test_parse_list_with_complex_items() {
        let input = r#"list Steps { - { - name = "Step 1" } }"#;
        let result = parse_node(input);
        assert!(result.is_ok());
        let (remaining, node) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(node.node_type, "list");
        assert_eq!(node.children.len(), 1);
        let list_item = &node.children[0];
        assert_eq!(list_item.node_type, "-"); // Changed: now uses "-" instead of "list_item"
        assert_eq!(list_item.children.len(), 1);
        let item_field = &list_item.children[0];
        assert_eq!(item_field.node_type, "-"); // Inferred type
        assert_eq!(item_field.name, "name");
        assert_eq!(
            item_field.parameters.get("value"),
            Some(&OverseerValue::String("Step 1".to_string()))
        );
    }

    #[test]
    fn test_parse_templated_node() {
        let input = r#"<../TaskTemplate> my_task (priority=5) {}"#;
        let result = parse_node(input);
        assert!(result.is_ok());
        let (_, node) = result.unwrap();
        assert_eq!(node.node_type, "TaskTemplate");
        assert_eq!(node.name, "my_task");
        assert_eq!(node.template, Some("../TaskTemplate".to_string()));
        assert_eq!(node.parameters.get("priority"), Some(&OverseerValue::Integer(5)));
    }

    #[test]
    fn test_parse_node_with_hyphenated_parameters() {
        let input = r#"string title (margin-top=10, margin-left=5) = "Test Value""#;
        let result = parse_node(input);
        assert!(result.is_ok());
        let (remaining, node) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(node.node_type, "string");
        assert_eq!(node.name, "title");
        assert_eq!(node.parameters.get("margin-top"), Some(&OverseerValue::Integer(10)));
        assert_eq!(node.parameters.get("margin-left"), Some(&OverseerValue::Integer(5)));
        assert_eq!(node.parameters.get("value"), Some(&OverseerValue::String("Test Value".to_string())));
    }

    #[test]
    fn test_parse_layout_parameters() {
        let input = r#"div Container (layout=horizontal, spacing=15) { string field = "test" }"#;
        let result = parse_node(input);
        assert!(result.is_ok());
        let (remaining, node) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(node.node_type, "div");
        assert_eq!(node.name, "Container");
        assert_eq!(node.parameters.get("layout"), Some(&OverseerValue::String("horizontal".to_string())));
        assert_eq!(node.parameters.get("spacing"), Some(&OverseerValue::Integer(15)));
        assert_eq!(node.children.len(), 1);
        assert_eq!(node.children[0].node_type, "string");
        assert_eq!(node.children[0].name, "field");
    }

    #[test]
    fn test_parse_complex_parameters() {
        let input = r#"div Card (layout=vertical, spacing=8, margin-bottom=12) {
            string title (margin-top=4) = "Card Title"
        }"#;
        let result = parse_node(input);
        assert!(result.is_ok());
        let (remaining, node) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(node.node_type, "div");
        assert_eq!(node.name, "Card");
        assert_eq!(node.parameters.get("layout"), Some(&OverseerValue::String("vertical".to_string())));
        assert_eq!(node.parameters.get("spacing"), Some(&OverseerValue::Integer(8)));
        assert_eq!(node.parameters.get("margin-bottom"), Some(&OverseerValue::Integer(12)));
        assert_eq!(node.children.len(), 1);
        let child = &node.children[0];
        assert_eq!(child.node_type, "string");
        assert_eq!(child.name, "title");
        assert_eq!(child.parameters.get("margin-top"), Some(&OverseerValue::Integer(4)));
        assert_eq!(child.parameters.get("value"), Some(&OverseerValue::String("Card Title".to_string())));
    }

    #[test]
    fn parse_document_captures_source_snapshots() {
    let _registry_guard = crate::source_registry::REGISTRY_TEST_MUTEX.lock();
        let src = concat!(
            "div Root {\n",
            "    string child = \"Value\"\n",
            "}\n",
            "\n",
            "// comment about next\n",
            "\n",
            "string next = \"Other\"\n",
        );
        let (remaining, mut nodes) = parse_document(src).expect("document should parse");
        assert!(remaining.trim().is_empty(), "expected all input consumed, got '{remaining}'");
        assert_eq!(nodes.len(), 2, "expected two top-level nodes");

        let root = nodes.remove(0);
        let root_snapshot = root.source_snapshot.as_ref().expect("root should have snapshot");
        assert_eq!(root_snapshot.span.0, 0, "root span should begin at start of document");
        assert!(root_snapshot.full_text.starts_with("div Root {"));
        assert!(root_snapshot.full_text.contains("string child = \"Value\""));
        assert!(root_snapshot.leading_trivia.is_empty());
        assert!(root_snapshot.indent_unit.is_none());
        assert!(root_snapshot.leading_span.is_none());
    assert_eq!(root_snapshot.header_span.0, root_snapshot.span.0);
    assert!(root_snapshot.header_span.1 <= root_snapshot.span.1);
    assert!(root_snapshot.body_span.is_some());
    assert!(root_snapshot.trailing_span.is_some());
    assert!(root_snapshot.trailing_trivia.contains('\n'));
        let root_id = root.source_id.as_ref().expect("root should have source id");
        let fetched_root = SourceRegistry::get(root_id).expect("root id should resolve in registry");
        assert_eq!(fetched_root.full_text, root_snapshot.full_text);

        let child = root.children.get(0).expect("root should have child");
        let child_snapshot = child.source_snapshot.as_ref().expect("child should have snapshot");
        assert_eq!(child_snapshot.leading_trivia, "\n    ");
        assert_eq!(child_snapshot.indent_unit.as_deref(), Some("    "));
        assert_eq!(child_snapshot.full_text.trim_end(), "string child = \"Value\"");
        assert_eq!(child_snapshot.span.0, src.find("string child").expect("child text present"));
        let child_leading_span = child_snapshot.leading_span.expect("child should record leading span");
        assert_eq!(child_leading_span.1 - child_leading_span.0, child_snapshot.leading_trivia.len());
    let child_body_span = child_snapshot.body_span.expect("child should have body span recorded");
    assert!(child_body_span.1 > child_body_span.0);
    assert!(child_snapshot.trailing_span.is_some());
        let child_id = child.source_id.as_ref().expect("child should have source id");
        assert_ne!(child_id, root_id, "child and root should have distinct ids");
        let fetched_child = SourceRegistry::get(child_id).expect("child id should resolve in registry");
        assert_eq!(fetched_child.full_text, child_snapshot.full_text);

        let next = nodes.into_iter().next().expect("expected second node");
    assert_eq!(next.leading_blank_lines, 2, "blank lines between nodes should be recorded");
        let next_snapshot = next.source_snapshot.as_ref().expect("second node should have snapshot");
        assert!(next_snapshot.full_text.starts_with("string next = \"Other\""));
        assert!(next_snapshot.leading_trivia.starts_with("\n"));
        assert!(next_snapshot.leading_trivia.contains("// comment about next"));
        assert!(next_snapshot.indent_unit.is_none());
        assert_eq!(next_snapshot.span.0, src.find("string next").expect("second node text present"));
        let next_leading_span = next_snapshot.leading_span.expect("second node should record leading span");
        assert_eq!(next_leading_span.1 - next_leading_span.0, next_snapshot.leading_trivia.len());
    assert_eq!(next_snapshot.header_span.0, next_snapshot.span.0);
    assert!(next_snapshot.body_span.is_some());
    assert!(next_snapshot.trailing_trivia.ends_with('\n'));
        let next_id = next.source_id.as_ref().expect("second node should have source id");
        let fetched_next = SourceRegistry::get(next_id).expect("second node id should resolve in registry");
        assert_eq!(fetched_next.full_text, next_snapshot.full_text);


        assert_eq!(SourceRegistry::len(), 3, "registry should track all parsed nodes in sample");

        SourceRegistry::reset();
    }

    #[test]
    fn test_parse_boolean_and_float_values() {
        let input = r#"div Settings (visible=true, opacity=0.8, count=42) {}"#;
        let result = parse_node(input);
        assert!(result.is_ok());
        let (remaining, node) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(node.parameters.get("visible"), Some(&OverseerValue::Boolean(true)));
        assert_eq!(node.parameters.get("opacity"), Some(&OverseerValue::Float(0.8)));
        assert_eq!(node.parameters.get("count"), Some(&OverseerValue::Integer(42)));
    }

    #[test]
    fn test_parse_color_values() {
        // Test hex colors
        let result = parse_value("#FF0000");
        assert_eq!(result, Ok(("", OverseerValue::Color(Color::Hex("#FF0000".to_string())))));
        
        let result = parse_value("#abc");
        assert_eq!(result, Ok(("", OverseerValue::Color(Color::Hex("#abc".to_string())))));
        
        // Test named colors
        let result = parse_value("red");
        assert_eq!(result, Ok(("", OverseerValue::Color(Color::Named("red".to_string())))));
        
        let result = parse_value("transparent");
        assert_eq!(result, Ok(("", OverseerValue::Color(Color::Named("transparent".to_string())))));
        
        // Test RGB colors
        let result = parse_value("rgb(0.2, 0.8, 0.5)");
        assert_eq!(result, Ok(("", OverseerValue::Color(Color::Rgb(0.2, 0.8, 0.5)))));
        
        let result = parse_value("rgb(1.0, 0.0, 0.5)");
        assert_eq!(result, Ok(("", OverseerValue::Color(Color::Rgb(1.0, 0.0, 0.5)))));
    }
    
    #[test]
    fn test_parse_css_size_values() {
        // Test pixels
        let result = parse_value("16px");
        assert_eq!(result, Ok(("", OverseerValue::CssSize(CssSize::Pixels(16.0)))));
        
        let result = parse_value("10.5px");
        assert_eq!(result, Ok(("", OverseerValue::CssSize(CssSize::Pixels(10.5)))));
        
        // Test percentages
        let result = parse_value("120%");
        assert_eq!(result, Ok(("", OverseerValue::CssSize(CssSize::Percentage(120.0)))));
        
        // Test em/rem
        let result = parse_value("1.2em");
        assert_eq!(result, Ok(("", OverseerValue::CssSize(CssSize::Em(1.2)))));
        
        let result = parse_value("2rem");
        assert_eq!(result, Ok(("", OverseerValue::CssSize(CssSize::Rem(2.0)))));
        
        // Test viewport units
        let result = parse_value("50vw");
        assert_eq!(result, Ok(("", OverseerValue::CssSize(CssSize::ViewportWidth(50.0)))));
        
        let result = parse_value("100vh");
        assert_eq!(result, Ok(("", OverseerValue::CssSize(CssSize::ViewportHeight(100.0)))));
        
        // Test keywords
        let result = parse_value("auto");
        assert_eq!(result, Ok(("", OverseerValue::CssSize(CssSize::Auto))));
        
        let result = parse_value("fit-content");
        assert_eq!(result, Ok(("", OverseerValue::CssSize(CssSize::FitContent))));
    }

    #[test]
    fn test_parse_styling_parameters() {
        let input = r#"div Container (background-color=#FF0000, font-color=blue, font-size=16px, width=50%) {
            string field = "Value"
        }"#;
        let result = parse_node(input);
        assert!(result.is_ok());
        
        let (_, node) = result.unwrap();
        assert_eq!(node.node_type, "div");
        assert_eq!(node.name, "Container");
        assert_eq!(node.parameters.get("background-color"), Some(&OverseerValue::Color(Color::Hex("#FF0000".to_string()))));
        assert_eq!(node.parameters.get("font-color"), Some(&OverseerValue::Color(Color::Named("blue".to_string()))));
        assert_eq!(node.parameters.get("font-size"), Some(&OverseerValue::CssSize(CssSize::Pixels(16.0))));
        assert_eq!(node.parameters.get("width"), Some(&OverseerValue::CssSize(CssSize::Percentage(50.0))));
    }

    #[test]
    fn test_markdown_parameter() {
        // Test with parameter syntax matching existing working tests
        let input = r#"div Container (markdown=true, font-size=18px) {
        }"#;
        
        let result = parse_node(input);
        assert!(result.is_ok(), "Failed to parse node with markdown parameter: {:?}", result.err());
        
        let (_, node) = result.unwrap();
        assert_eq!(node.node_type, "div");
        assert_eq!(node.name, "Container");
        assert_eq!(node.parameters.get("markdown"), Some(&OverseerValue::Boolean(true)));
        assert_eq!(node.parameters.get("font-size"), Some(&OverseerValue::CssSize(CssSize::Pixels(18.0))));
    }

}
