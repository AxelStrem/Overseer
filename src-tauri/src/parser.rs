use crate::types::{OverseerNode, OverseerValue};
use nom::{
    branch::alt,
    bytes::complete::{tag, take_until},
    character::complete::{alpha1, alphanumeric1, char, multispace0, multispace1, none_of},
    combinator::{map, opt, recognize},
    multi::{many0, separated_list0},
    sequence::{delimited, pair, preceded},
    IResult,
};

// Debug logging macro for parser
macro_rules! debug_parser {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-parser")]
        println!($($arg)*);
    };
}
use std::collections::HashMap;

/// Parse the entire document (top-level nodes)
pub fn parse_document(input: &str) -> IResult<&str, Vec<OverseerNode>> {
    preceded(
        skip_comments_and_whitespace,
        many0(preceded(skip_comments_and_whitespace, parse_node)),
    )(input)
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
fn parse_node(input: &str) -> IResult<&str, OverseerNode> {
    // Only skip lines that start with '=' and are not part of a value assignment after a type or identifier
    let trimmed = input.trim_start();
    if trimmed.starts_with('=') {
        debug_parser!("[PARSER] Skipping invalid node start: {}", trimmed.chars().take(40).collect::<String>());
        return Err(nom::Err::Failure(nom::error::Error::new(input, nom::error::ErrorKind::Tag)));
    }
    // Debug: print the input being parsed
    debug_parser!("[PARSER] input: {}", input.chars().take(80).collect::<String>());

    // A node definition can be templated or regular
    let (input, (template_val, node_type)) = match alt((
        map(parse_template_value, |p| (Some(p), None)),
        map(parse_node_type, |t| (None, Some(t.to_string()))),
    ))(input) {
        Ok(res) => res,
        Err(e) => {
            if !input.trim().is_empty() {
                debug_parser!("[PARSER] Failed to parse node type: {:?}", e);
            }
            return Err(e);
        }
    };

    // Then parse optional name and parameters
    let (input, _) = multispace0(input)?;
    let (input, node_name) = opt(parse_identifier)(input)?;
    let (input, _) = multispace0(input)?;
    debug_parser!("[PARSER] Before parsing parameters, input: {}", input.chars().take(50).collect::<String>());
    let (input, parameters) = opt(parse_parameters)(input)?;
    debug_parser!("[PARSER] After parsing parameters, input: {}", input.chars().take(50).collect::<String>());
    let (input, _) = multispace0(input)?;

    // Then parse body, which can be a block, a value assignment, or nothing
    debug_parser!("[PARSER] Before parsing body, input: {}", input.chars().take(50).collect::<String>());
    let (input, body) = match opt(alt((
        map(parse_value_assignment, |val| (Some(val), Vec::new())),
        map(parse_direct_value, |val| (Some(val), Vec::new())),
        map(parse_block, |children| (None, children)),
    )))(input) {
        Ok(res) => res,
        Err(e) => {
            debug_parser!("[PARSER] Failed to parse body: {:?}", e);
            return Err(e);
        }
    };
    debug_parser!("[PARSER] After parsing body, input: {}", input.chars().take(50).collect::<String>());

    let (value, children) = body.unwrap_or((None, Vec::new()));

    // Determine the template path string, if it exists
    let template_path = if let Some(OverseerValue::Template(t)) = &template_val {
        Some(t.clone())
    } else {
        None
    };

    // Construct the node
    // Keep '-' as the type - type inference will happen in the resolver stage
    let final_node_type = if let Some(nt) = node_type {
        nt
    } else if let Some(t) = &template_path {
        // If the type is a template, we can use the template path as a hint for the type
        t.split('/').last().unwrap_or_default().to_string()
    } else { 
        "".to_string() 
    };
    let mut node = OverseerNode::new_with_type(final_node_type, node_name.map(|s| s.to_string()));

    node.template = template_path;
    node.parameters = parameters.unwrap_or_default();
    node.children = children;

    if let Some(val) = value {
        node.parameters.insert("value".to_string(), val);
    }

    debug_parser!("[PARSER] Parsed node: type='{}', name='{}'", node.node_type, node.name);
    if !node.parameters.is_empty() {
        debug_parser!("[PARSER]   Parameters: {:?}", node.parameters);
    }

    Ok((input, node))
}

/// Parse a node type, which is an identifier or a hyphen for inference
fn parse_node_type(input: &str) -> IResult<&str, &str> {
    alt((parse_identifier, tag("-")))(input)
}

/// Parse a template path like <../Task>
fn parse_template_value(input: &str) -> IResult<&str, OverseerValue> {
    map(delimited(char('<'), take_until(">"), char('>')), |s: &str| OverseerValue::Template(s.to_string()))(input)
}

/// Parse node parameters like (param=value, param2=value2)
fn parse_parameters(input: &str) -> IResult<&str, HashMap<String, OverseerValue>> {
    map(
        delimited(
            char('('),
            separated_list0(
                preceded(multispace0, char(',')),
                preceded(multispace0, parse_parameter),
            ),
            preceded(multispace0, char(')')),
        ),
        |params| params.into_iter().collect(),
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

/// Parse value assignment (= value)
fn parse_value_assignment(input: &str) -> IResult<&str, OverseerValue> {
    preceded(pair(multispace0, char('=')), preceded(multispace0, parse_value))(input)
}

/// Parse a value directly (without =) - only for quoted strings, numbers, booleans, etc.
fn parse_direct_value(input: &str) -> IResult<&str, OverseerValue> {
    preceded(multispace0, alt((
        parse_quoted_string_value,
        parse_template_value,
        parse_number_value,
        parse_boolean_value,
        // Note: We don't include parse_unquoted_string_value here to avoid conflicts
    )))(input)
}

/// Parse a block { ... }
fn parse_block(input: &str) -> IResult<&str, Vec<OverseerNode>> {
    let (mut input, _) = preceded(multispace0, char('{'))(input)?;
    let mut children = Vec::new();

    loop {
        let (next_input, _) = skip_comments_and_whitespace(input)?;
        // Check for end of block
        if let Ok((after, _)) = preceded(multispace0::<&str, ()>, char('}'))(next_input) {
            input = after;
            break;
        }
        match parse_node(next_input) {
            Ok((after, node)) => {
                // Always add the parsed node to children
                children.push(node);
                input = after;
            }
            Err(_) => {
                // Could not parse, skip one character and continue (to avoid infinite loop)
                input = &next_input[1..];
            }
        }
    }
    Ok((input, children))
}

/// Parse different types of values
fn parse_value(input: &str) -> IResult<&str, OverseerValue> {
    alt((
        parse_template_value,
        parse_formula_value,
        parse_boolean_value,
        parse_number_value,
        parse_quoted_string_value,
        parse_unquoted_string_value, // Must be last as it's a fallback
    ))(input)
}

/// Parse formula like $(expression)
fn parse_formula_value(input: &str) -> IResult<&str, OverseerValue> {
    map(
        delimited(tag("$("), take_until(")"), char(')')),
        |s: &str| OverseerValue::Formula(s.to_string()),
    )(input)
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
        assert_eq!(list_item.node_type, "list_item");
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
}
