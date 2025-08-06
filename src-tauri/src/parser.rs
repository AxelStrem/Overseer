use crate::types::{OverseerNode, OverseerValue, Color, CssSize};
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
        parse_color_value,
        parse_css_size_value, // Must come before number parsing to handle "10px" correctly
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
