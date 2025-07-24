use crate::types::{OverseerNode, OverseerValue, OverseerError};
use nom::{
    IResult,
    bytes::complete::{tag, take_until},
    character::complete::{alpha1, alphanumeric1, char, multispace0},
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
    preceded(
        multispace0,
        many0(preceded(multispace0, parse_node))
    )(input)
}

/// Parse a single node
fn parse_node(input: &str) -> IResult<&str, OverseerNode> {
    let (input, node_type) = parse_identifier(input)?;
    let (input, _) = multispace0(input)?;
    let (input, name) = opt(parse_identifier)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, parameters) = opt(parse_parameters)(input)?;
    let (input, _) = multispace0(input)?;
    
    // Check if this is a simple value assignment or a block
    let (input, (value, children)) = alt((
        parse_value_assignment,
        parse_block
    ))(input)?;

    let mut node = OverseerNode::new(node_type.to_string());
    if let Some(name) = name {
        node.name = name.to_string();
    }
    if let Some(params) = parameters {
        // Convert HashMap<String, String> to HashMap<String, OverseerValue>
        node.parameters = params.into_iter()
            .map(|(k, v)| (k, OverseerValue::String(v)))
            .collect();
    }
    // Note: There's no value field in OverseerNode, so we'll skip this
    node.children = children;

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
    delimited(
        char('{'),
        preceded(
            multispace0,
            many0(preceded(multispace0, parse_node))
        ),
        preceded(multispace0, char('}'))
    )(input)
    .map(|(input, children)| (input, (None, children)))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_node() {
        let input = r#"string Name = "Test""#;
        let result = parse_node(input);
        assert!(result.is_ok());
        
        let (_, node) = result.unwrap();
        assert_eq!(node.node_type, "string");
        assert_eq!(node.name, Some("Name".to_string()));
        assert!(matches!(node.value, Some(OverseerValue::String(_))));
    }

    #[test]
    fn test_parse_node_with_parameters() {
        let input = r#"div Task (background=Red) { }"#;
        let result = parse_node(input);
        assert!(result.is_ok());
        
        let (_, node) = result.unwrap();
        assert_eq!(node.node_type, "div");
        assert_eq!(node.name, Some("Task".to_string()));
        assert_eq!(node.parameters.get("background"), Some(&"Red".to_string()));
    }

    #[test]
    fn test_parse_formula() {
        let input = r#"int Priority = $(5 + 3)"#;
        let result = parse_node(input);
        assert!(result.is_ok());
        
        let (_, node) = result.unwrap();
        assert!(matches!(node.value, Some(OverseerValue::Formula(_))));
    }
}
