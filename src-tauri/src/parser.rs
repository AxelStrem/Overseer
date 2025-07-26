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
    // A node definition can be templated or regular
    let (input, (template_val, node_type)) = alt((
        // Templated: <path>
        map(parse_template_value, |p| (Some(p), None)),
        // Regular: type or -
        map(parse_node_type, |t| (None, Some(t.to_string()))),
    ))(input)?;

    // Then parse optional name and parameters
    let (input, _) = multispace0(input)?;
    let (input, node_name) = opt(parse_identifier)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, parameters) = opt(parse_parameters)(input)?;
    let (input, _) = multispace0(input)?;

    // Then parse body, which can be a block, a value assignment, or nothing
    let (input, body) = opt(alt((
        // Node with a value assignment = ...
        map(parse_value_assignment, |val| (Some(val), Vec::new())),
        // Node with a block body { ... }
        map(parse_block, |children| (None, children)),
    )))(input)?;

    let (value, children) = body.unwrap_or((None, Vec::new()));

    // Determine the template path string, if it exists
    let template_path = if let Some(OverseerValue::Template(t)) = &template_val {
        Some(t.clone())
    } else {
        None
    };

    // Construct the node
    let final_node_type = if let Some(nt) = node_type {
        nt
    } else if let Some(t) = &template_path {
        // If the type is a template, we can use the template path as a hint for the type
        t.split('/').last().unwrap_or_default().to_string()
    } else { "".to_string() };
    let mut node = OverseerNode::new_with_type(final_node_type, node_name.map(|s| s.to_string()));
    
    node.template = template_path;
    node.parameters = parameters.unwrap_or_default();
    node.children = children;

    if let Some(val) = value {
        node.parameters.insert("value".to_string(), val);
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
        // Try to parse a list item or node
        match alt((parse_list_item, parse_node))(next_input) {
            Ok((after, node)) => {
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

/// Parse a list item starting with -
fn parse_list_item(input: &str) -> IResult<&str, OverseerNode> {
    let (input, _) = preceded(multispace0, char('-'))(input)?;
    let (input, _) = multispace0(input)?;

    // A list item can be a simple value or a complex object in a block
    let (input, body) = alt((
        // Case 1: Complex object like - { - name = "..." }
        map(parse_block, |children| (None, children)),
        // Case 2: Simple value like - "a string"
        map(parse_value, |value| (Some(value), Vec::new())),
    ))(input)?;

    let (value, children) = body;

    let mut node = OverseerNode::new_with_type("list_item".to_string(), None);
    if let Some(val) = value {
        node.parameters.insert("value".to_string(), val);
    }
    node.children = children;
    Ok((input, node))
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
            many0(alt((alphanumeric1, tag("_"), tag("."), tag("/")))),
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
