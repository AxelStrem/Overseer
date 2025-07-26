use crate::types::{OverseerNode, OverseerValue, OverseerError};
use nom::{
    IResult,
    bytes::complete::{tag, take_until, take},
    character::complete::{alpha1, alphanumeric1, char, multispace0, multispace1, none_of},
    combinator::{opt, recognize},
    multi::{many0, separated_list0},
    sequence::{delimited, pair, preceded, tuple},
    branch::alt,
    Parser,
};
use std::collections::HashMap;

/// Parse an entire Overseer file
pub fn parse_overseer_file(input: &str) -> Result<serde_json::Value, OverseerError> {
    match parse_document(input) {
        Ok((_, document)) => {
            // Convert to JSON for now (later we'll return the proper AST)
            Ok(serde_json::to_value(document).unwrap())
        }
        Err(e) => Err(OverseerError::ParseError(format!("Failed to parse: {:?}", e))),
    }
}

/// Parse the entire document (top-level nodes)
pub fn parse_document(input: &str) -> IResult<&str, Vec<OverseerNode>> {
    println!("DEBUG parse_document: input start: {:?}", &input[..input.len().min(100)]);
    let result = preceded(
        skip_comments_and_whitespace,
        many0(preceded(skip_comments_and_whitespace, parse_node))
    )(input);
    println!("DEBUG parse_document: result: {:?}", result.as_ref().map(|(remaining, nodes)| (remaining.len(), nodes.len())));
    result
}

/// Skip comments and whitespace
fn skip_comments_and_whitespace(input: &str) -> IResult<&str, ()> {
    println!("DEBUG skip_comments_and_whitespace: input start: {:?}", &input[..input.len().min(100)]);
    let (input, _) = many0(alt((
        skip_single_line_comment,
        skip_multi_line_comment,
        skip_whitespace
    )))(input)?;
    println!("DEBUG skip_comments_and_whitespace: output start: {:?}", &input[..input.len().min(100)]);
    Ok((input, ()))
}

/// Skip single line comment // comment
fn skip_single_line_comment(input: &str) -> IResult<&str, ()> {
    let (input, _) = tag("//")(input)?;
    let (input, _) = many0(none_of("\r\n"))(input)?; // consume until line ending
    let (input, _) = alt((tag("\r\n"), tag("\n"), tag("\r")))(input)?; // handle any line ending
    Ok((input, ()))
}

/// Skip multi-line comment /* comment */
fn skip_multi_line_comment(input: &str) -> IResult<&str, ()> {
    let (input, _) = tag("/*")(input)?;
    let (input, _) = take_until("*/")(input)?;
    let (input, _) = tag("*/")(input)?;
    Ok((input, ()))
}

/// Skip whitespace
fn skip_whitespace(input: &str) -> IResult<&str, ()> {
    let (input, _) = multispace1(input)?;
    Ok((input, ()))
}

/// Parse a single node
fn parse_node(input: &str) -> IResult<&str, OverseerNode> {
    println!("DEBUG parse_node: input start: {:?}", &input[..input.len().min(50)]);
    let (input, node_type) = parse_identifier(input)?;
    println!("DEBUG parse_node: parsed node_type: {:?}", node_type);
    let (input, _) = skip_comments_and_whitespace(input)?;
    
    // Try to parse name and parameters in different orders
    // Pattern 1: node_type name (params) 
    // Pattern 2: node_type (params)
    // Pattern 3: node_type name
    // Pattern 4: node_type
    
    let (input, (node_name, parameters)) = 
        if let Ok((input, name)) = parse_identifier(input) {
            let (input, _) = skip_comments_and_whitespace(input)?;
            if let Ok((input, params)) = parse_parameters(input) {
                println!("DEBUG: Pattern 1 success: name={:?}, params=true", name);
                (input, (Some(name), Some(params)))
            } else {
                println!("DEBUG: Pattern 3 success: name={:?}, params=false", name);
                (input, (Some(name), None))
            }
        } else if let Ok((input, params)) = parse_parameters(input) {
            println!("DEBUG: Pattern 2 success: params={}", params.len());
            (input, (None, Some(params)))
        } else {
            println!("DEBUG: Pattern 4 success: no name, no params");
            (input, (None, None))
        };
    
    let (input, _) = skip_comments_and_whitespace(input)?;
    
    println!("DEBUG: After pattern matching, about to parse value/block. Input: {:?}", &input[..input.len().min(50)]);
    
    // Check if this is a simple value assignment or a block
    println!("DEBUG: About to parse value assignment or block, input: {:?}", &input[..input.len().min(50)]);
    let (input, (value, children)) = alt((
        parse_value_assignment,
        parse_block
    ))(input)?;
    println!("DEBUG: Parsed value/block successfully, remaining input: {:?}", &input[..input.len().min(50)]);

    // Create node with hierarchy transparency logic
    let mut node = OverseerNode::new_with_type(
        node_type.to_string(), 
        node_name.map(|s| s.to_string())
    );
    
    if let Some(params) = parameters {
        // Convert HashMap<String, String> to HashMap<String, OverseerValue>
        node.parameters = params.into_iter()
            .map(|(k, v)| (k, OverseerValue::String(v)))
            .collect();
    }
    
    // Set the value if we got one from value assignment
    if let Some(val) = value {
        node.parameters.insert("_value".to_string(), val);
    }
    
    node.children = children;

    println!("DEBUG: Created node with type={:?}, name={:?}, children={}", 
             node.node_type, node.name, node.children.len());

    Ok((input, node))
}

/// Parse node parameters like (param=value, param2=value2)
fn parse_parameters(input: &str) -> IResult<&str, HashMap<String, String>> {
    delimited(
        char('('),
        preceded(
            multispace0,
            separated_list0(
                preceded(multispace0, char(',')),
                preceded(multispace0, parse_parameter)
            )
        ),
        preceded(multispace0, char(')'))
    )(input)
    .map(|(input, params)| {
        let mut map = HashMap::new();
        for (key, value) in params {
            map.insert(key, value);
        }
        (input, map)
    })
}

/// Parse a single parameter (key=value)
fn parse_parameter(input: &str) -> IResult<&str, (String, String)> {
    let (input, key) = parse_identifier(input)?;
    let (input, _) = preceded(multispace0, char('='))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, value) = parse_parameter_value(input)?;
    Ok((input, (key.to_string(), value)))
}

/// Parse parameter values (strings, formulas, identifiers)
fn parse_parameter_value(input: &str) -> IResult<&str, String> {
    alt((
        parse_quoted_string,
        parse_formula,
        parse_identifier
    ))(input).map(|(input, value)| (input, value.to_string()))
}

/// Parse value assignment (= value)
fn parse_value_assignment(input: &str) -> IResult<&str, (Option<OverseerValue>, Vec<OverseerNode>)> {
    let (input, _) = char('=')(input)?;
    let (input, _) = multispace0(input)?;
    let (input, value) = parse_value(input)?;
    Ok((input, (Some(value), Vec::new())))
}

/// Parse a block { ... }
fn parse_block(input: &str) -> IResult<&str, (Option<OverseerValue>, Vec<OverseerNode>)> {
    let (input, _) = char('{')(input)?;
    let (input, _) = skip_comments_and_whitespace(input)?;
    
    let (input, children) = many0(
        alt((
            preceded(skip_comments_and_whitespace, parse_list_item),
            preceded(skip_comments_and_whitespace, parse_node)
        ))
    )(input)?;
    
    let (input, _) = skip_comments_and_whitespace(input)?;
    let (input, _) = char('}')(input)?;
    
    Ok((input, (None, children)))
}

/// Parse a list item starting with -
fn parse_list_item(input: &str) -> IResult<&str, OverseerNode> {
    let (input, _) = char('-')(input)?;
    let (input, _) = multispace0(input)?;
    let (input, value) = parse_value(input)?;
    
    let mut node = OverseerNode::new_with_type("list_item".to_string(), None);
    node.parameters.insert("_value".to_string(), value);
    
    println!("DEBUG: Created list item node with value: {:?}", node.parameters.get("_value"));
    Ok((input, node))
}

/// Parse different types of values
fn parse_value(input: &str) -> IResult<&str, OverseerValue> {
    alt((
        parse_formula_value,
        parse_quoted_string_value,
        parse_number_value,
        parse_boolean_value,
        parse_function_call_value,
        parse_identifier_value
    ))(input)
}

/// Parse formula like $(expression)
fn parse_formula(input: &str) -> IResult<&str, &str> {
    delimited(
        tag("$("),
        take_until(")"),
        char(')')
    )(input)
}

fn parse_formula_value(input: &str) -> IResult<&str, OverseerValue> {
    let (input, _) = tag("$(")(input)?;
    let (input, formula) = take_until(")")(input)?;
    let (input, _) = char(')')(input)?;
    Ok((input, OverseerValue::Formula(format!("$({})", formula))))
}

/// Parse quoted strings
fn parse_quoted_string(input: &str) -> IResult<&str, &str> {
    delimited(
        char('"'),
        take_until("\""),
        char('"')
    )(input)
}

fn parse_quoted_string_value(input: &str) -> IResult<&str, OverseerValue> {
    parse_quoted_string(input)
        .map(|(input, s)| (input, OverseerValue::String(s.to_string())))
}

/// Parse numbers (int or float)
fn parse_number_value(input: &str) -> IResult<&str, OverseerValue> {
    let (input, number_str) = recognize(
        tuple((
            opt(char('-')),
            nom::character::complete::digit1,
            opt(preceded(char('.'), nom::character::complete::digit1))
        ))
    )(input)?;

    if number_str.contains('.') {
        if let Ok(f) = number_str.parse::<f64>() {
            Ok((input, OverseerValue::Float(f)))
        } else {
            Err(nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Float)))
        }
    } else {
        if let Ok(i) = number_str.parse::<i64>() {
            Ok((input, OverseerValue::Integer(i)))
        } else {
            Err(nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Digit)))
        }
    }
}

/// Parse boolean values
fn parse_boolean_value(input: &str) -> IResult<&str, OverseerValue> {
    alt((
        tag("true").map(|_| OverseerValue::Boolean(true)),
        tag("false").map(|_| OverseerValue::Boolean(false))
    ))(input)
}

/// Parse function calls like today()
fn parse_function_call_value(input: &str) -> IResult<&str, OverseerValue> {
    let (input, func_name) = parse_identifier(input)?;
    let (input, _) = tag("()")(input)?;
    Ok((input, OverseerValue::Formula(format!("{}()", func_name))))
}

/// Parse identifier as a value
fn parse_identifier_value(input: &str) -> IResult<&str, OverseerValue> {
    parse_identifier(input)
        .map(|(input, id)| (input, OverseerValue::String(id.to_string())))
}

/// Parse identifiers (variable names, node types, etc.)
fn parse_identifier(input: &str) -> IResult<&str, &str> {
    recognize(
        pair(
            alt((alpha1, tag("_"))),
            many0(alt((alphanumeric1, tag("_"))))
        )
    )(input)
}

// Helper functions for parsing node patterns
fn parse_name_and_params(input: &str) -> IResult<&str, (Option<&str>, Option<HashMap<String, String>>)> {
    println!("DEBUG: Trying Pattern 1 (name + params): {:?}", &input[..input.len().min(50)]);
    let (input, name) = parse_identifier(input)?;
    let (input, _) = skip_comments_and_whitespace(input)?;
    let (input, params) = opt(parse_parameters)(input)?;
    println!("DEBUG: Pattern 1 success: name={:?}, params={:?}", name, params.is_some());
    Ok((input, (Some(name), params)))
}

fn parse_params_only(input: &str) -> IResult<&str, (Option<&str>, Option<HashMap<String, String>>)> {
    println!("DEBUG: Trying Pattern 2 (params only): {:?}", &input[..input.len().min(50)]);
    let (input, params) = parse_parameters(input)?;
    println!("DEBUG: Pattern 2 success: params={:?}", params.len());
    Ok((input, (None, Some(params))))
}

fn parse_name_only(input: &str) -> IResult<&str, (Option<&str>, Option<HashMap<String, String>>)> {
    println!("DEBUG: Trying Pattern 3 (name only): {:?}", &input[..input.len().min(50)]);
    let (input, name) = parse_identifier(input)?;
    println!("DEBUG: Pattern 3 success: name={:?}", name);
    Ok((input, (Some(name), None)))
}

fn parse_neither(input: &str) -> IResult<&str, (Option<&str>, Option<HashMap<String, String>>)> {
    println!("DEBUG: Trying Pattern 4 (neither): {:?}", &input[..input.len().min(50)]);
    Ok((input, (None, None)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_node() {
        let input = r#"string Name = "Test""#;
        let result = parse_node(input);
        assert!(result.is_ok());
        
        let (_, node) = result.unwrap();
        assert_eq!(node.name, "string_Name");
        assert_eq!(node.node_type, "string");
        assert!(!node.is_hierarchy_transparent);
    }

    #[test]
    fn test_parse_node_with_parameters() {
        let input = r#"div Task (background=Red) { }"#;
        let result = parse_node(input);
        assert!(result.is_ok());
        
        let (_, node) = result.unwrap();
        assert_eq!(node.name, "div_Task");
        assert_eq!(node.node_type, "div");
        assert_eq!(node.parameters.get("background"), Some(&OverseerValue::String("Red".to_string())));
        assert!(!node.is_hierarchy_transparent);
    }

    #[test]
    fn test_parse_transparent_node() {
        let input = r#"tab (title="Test") { }"#;
        let result = parse_node(input);
        assert!(result.is_ok());
        
        let (_, node) = result.unwrap();
        assert_eq!(node.name, "tab");
        assert_eq!(node.node_type, "tab");
        assert_eq!(node.parameters.get("title"), Some(&OverseerValue::String("Test".to_string())));
        assert!(node.is_hierarchy_transparent); // Should be transparent since no name
    }
}
