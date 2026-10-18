//! A container carries copies only of the contexts it changes.
//!
//! A container is given a copy of each context it declares, and of each context whose formulas
//! read one it declares, however many steps away. Everything else it sees is the copy above it,
//! which a lookup climbs on to: a list whose entry template declares one context carries one copy
//! in each entry, not one of every context in view.

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
        Some(OverseerValue::Boolean(b)) => b.to_string(),
        other => format!("{:?}", other),
    }
}

/// The names of the copies among a node's own children.
fn copies_in(node: &OverseerNode) -> Vec<String> {
    node.children
        .iter()
        .filter(|child| overseer::contexts::is_copy(child))
        .map(|child| child.name.clone())
        .collect()
}

const THEMED: &str = r#"context mode = "light"
context theme {
    string surface = $(mode == "dark" ? "black" : "white")
}
context shade {
    string edge = $(theme/surface)
}
context accent = "blue"
"#;

#[test]
fn an_entry_whose_template_declares_one_context_carries_one_copy() {
    let source = format!(
        "{}{}",
        THEMED,
        r#"div (hidden=true) {
    div Item {
        context hover-text = true
        string seen = $(theme/surface)
        string tint = $(accent)
    }
}
tab t (label="T") {
    list items (entry=<Item>) {
        - first { }
        - second { }
    }
}
"#
    );
    let nodes = resolved(&source);
    let items = find(&nodes, "items").expect("the list");
    for entry in &items.children {
        assert_eq!(copies_in(entry), vec!["hover-text"]);
        let one = std::slice::from_ref(entry);
        assert_eq!(value_of(one, "seen"), "white", "the theme above is not seen");
        assert_eq!(value_of(one, "tint"), "blue");
    }
}

#[test]
fn a_container_that_declares_a_mode_carries_what_reads_it() {
    let source = format!(
        "{}{}",
        THEMED,
        r#"tab t (label="T") {
    div night {
        context mode = "dark"
        string seen = $(theme/surface)
        string outline = $(shade/edge)
    }
    string day = $(shade/edge)
}
"#
    );
    let nodes = resolved(&source);
    let night = find(&nodes, "night").expect("the div");
    assert_eq!(copies_in(night), vec!["mode", "theme", "shade"]);
    assert_eq!(value_of(&nodes, "seen"), "black");
    assert_eq!(value_of(&nodes, "outline"), "black", "a context reading the theme was not re-derived");
    assert_eq!(value_of(&nodes, "day"), "white");
}
