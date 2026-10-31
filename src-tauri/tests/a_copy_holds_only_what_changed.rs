//! A container's copy of a context holds only the keys the container changes.
//!
//! A theme is dozens of keys, and a template that says one of them is copied into every entry. A
//! copy keeps what its container declares, what reads that, and what those read; a key it leaves
//! out has the value of the copy above, which a read climbs on to.

use overseer::parser::parse_document;
use overseer::resolver;
use overseer::types::{OverseerNode, OverseerValue};

fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    for node in nodes {
        if node.name == name && node.node_type != "context" {
            return Some(node);
        }
        if let Some(hit) = find(&node.children, name) {
            return Some(hit);
        }
    }
    None
}

fn resolved(source: &str) -> Vec<OverseerNode> {
    overseer::source_registry::SourceRegistry::reset();
    let mut nodes = parse_document(source).expect("parse").1;
    resolver::resolve_document(&mut nodes);
    nodes
}

fn value_of(nodes: &[OverseerNode], name: &str) -> String {
    let node = find(nodes, name).unwrap_or_else(|| panic!("no node named `{}`", name));
    match node
        .parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
    {
        Some(OverseerValue::String(s)) => s.clone(),
        other => format!("{:?}", other),
    }
}

/// The keys of the copy of `theme` a node holds, in order.
fn keys_in(nodes: &[OverseerNode], name: &str) -> Vec<String> {
    let node = find(nodes, name).unwrap_or_else(|| panic!("no node named `{}`", name));
    node.children
        .iter()
        .find(|child| overseer::contexts::is_copy(child) && child.name == "theme")
        .unwrap_or_else(|| panic!("`{}` holds no copy of the theme", name))
        .children
        .iter()
        .map(|key| key.name.clone())
        .collect()
}

const THEME: &str = r#"context mode = "light"
context theme {
    - light {
        - text = "ink"
        - surface = "paper"
    }
    - dark {
        - text = "chalk"
        - surface = "slate"
    }
    - text = $(mode == "dark" ? dark/text : light/text)
    - surface = $(mode == "dark" ? dark/surface : light/surface)
    - line = $(text + " line")
    - radius = "8px"
    - checkbox {
        - style = "box"
    }
}
"#;

fn with_theme(rest: &str) -> Vec<OverseerNode> {
    resolved(&format!("{}{}", THEME, rest))
}

#[test]
fn a_copy_holds_the_key_said_and_a_read_of_another_climbs() {
    let nodes = with_theme(
        r#"div box {
    context theme {
        - radius = "2px"
    }
    string corner = $(theme/radius)
    string paper = $(theme/surface)
    string kind = $(theme/checkbox/style)
}
"#,
    );
    assert_eq!(keys_in(&nodes, "box"), ["radius"]);
    assert_eq!(value_of(&nodes, "corner"), "2px");
    assert_eq!(value_of(&nodes, "paper"), "paper");
    assert_eq!(value_of(&nodes, "kind"), "box");
}

#[test]
fn a_nearer_key_reworks_what_is_derived_from_it() {
    let nodes = with_theme(
        r#"div box {
    context theme {
        - text = "red"
    }
    string edge = $(theme/line)
}
"#,
    );
    assert_eq!(value_of(&nodes, "edge"), "red line");
    let keys = keys_in(&nodes, "box");
    assert!(keys.contains(&"line".to_string()), "{:?}", keys);
    assert!(!keys.contains(&"surface".to_string()), "{:?}", keys);
    assert!(!keys.contains(&"radius".to_string()), "{:?}", keys);
}

#[test]
fn a_nearer_mode_reworks_every_key_that_reads_it_and_only_those() {
    let nodes = with_theme(
        r#"div box {
    context mode = "dark"
    string ink = $(theme/text)
    string paper = $(theme/surface)
    string edge = $(theme/line)
    string corner = $(theme/radius)
}
"#,
    );
    assert_eq!(value_of(&nodes, "ink"), "chalk");
    assert_eq!(value_of(&nodes, "paper"), "slate");
    assert_eq!(value_of(&nodes, "edge"), "chalk line");
    assert_eq!(value_of(&nodes, "corner"), "8px");
    let keys = keys_in(&nodes, "box");
    assert!(!keys.contains(&"radius".to_string()), "{:?}", keys);
    assert!(!keys.contains(&"checkbox".to_string()), "{:?}", keys);
}

#[test]
fn a_colour_changed_in_one_mode_reaches_what_picks_from_it() {
    let nodes = resolved(&format!(
        "{}{}",
        THEME.replace(r#"context mode = "light""#, r#"context mode = "dark""#),
        r#"div box {
    context theme {
        - dark {
            - surface = "black"
        }
    }
    string paper = $(theme/surface)
    string ink = $(theme/text)
}
"#,
    ));
    assert_eq!(value_of(&nodes, "paper"), "black");
    assert_eq!(value_of(&nodes, "ink"), "chalk");
}

#[test]
fn a_key_reading_the_document_is_worked_out_where_each_copy_is() {
    let nodes = resolved(
        r#"string colour = "outer"
context theme {
    - tint = $(colour)
    - radius = "8px"
}
div box {
    string colour = "inner"
    context theme {
        - radius = "2px"
    }
    string seen = $(theme/tint)
}
"#,
    );
    assert_eq!(value_of(&nodes, "seen"), "inner");
    assert_eq!(keys_in(&nodes, "box"), ["tint", "radius"]);
}

#[test]
fn a_copy_that_starts_over_stays_whole() {
    let nodes = with_theme(
        r#"div (hidden=true) {
    context other {
        - radius = "0"
        - surface = "glass"
    }
}
div box {
    context theme (base=<other>) {
        - surface = "stone"
    }
    string paper = $(theme/surface)
}
"#,
    );
    assert_eq!(keys_in(&nodes, "box"), ["radius", "surface"]);
    assert_eq!(value_of(&nodes, "paper"), "stone");
}

#[test]
fn a_path_from_the_top_or_from_a_sibling_reads_past_the_copy() {
    let nodes = with_theme(
        r#"div box {
    context theme {
        - radius = "2px"
    }
    string anchored = $(/theme/surface)
    string sibling = $(../theme/surface)
    string nested = $(../theme/checkbox/style)
    string own = $(../theme/radius)
}
"#,
    );
    assert_eq!(value_of(&nodes, "anchored"), "paper");
    assert_eq!(value_of(&nodes, "sibling"), "paper");
    assert_eq!(value_of(&nodes, "nested"), "box");
    assert_eq!(value_of(&nodes, "own"), "2px");
}

#[test]
fn entries_of_a_list_carry_the_one_key_their_template_says() {
    let nodes = with_theme(
        r#"div (hidden=true) {
    div Item {
        context theme {
            - radius = $(../size)
        }
        string size = "1px"
        string paper = $(theme/surface)
    }
}
tab t (label="T") {
    list items (entry=<Item>) {
        - first {
            - size = "3px"
        }
    }
}
"#,
    );
    assert_eq!(keys_in(&nodes, "first"), ["radius"]);
    assert_eq!(value_of(&nodes, "paper"), "paper");
}

#[test]
fn resolving_twice_comes_to_the_same() {
    let mut nodes = with_theme(
        r#"div box {
    context theme {
        - radius = "2px"
    }
}
"#,
    );
    overseer::contexts::resolve_contexts(&mut nodes);
    assert_eq!(keys_in(&nodes, "box"), ["radius"]);
}
