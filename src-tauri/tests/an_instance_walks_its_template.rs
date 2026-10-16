//! An instance's block is merged into its template by walking the two together.
//!
//! A named element finds its field by name, in any order and through unnamed nodes. Unnamed
//! elements and `+` elements follow a cursor through the template: an unnamed one lands on the next
//! unnamed node of its type, a `+` one goes in where the cursor is, and a match moves the cursor to
//! just after what it matched. An element that matches nothing is kept, last in its own block.

use overseer::parser::parse_document;
use overseer::resolver::resolve_document;
use overseer::types::{OverseerNode, OverseerValue};

fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    for n in nodes {
        if n.name == name {
            return Some(n);
        }
        if let Some(f) = find(&n.children, name) {
            return Some(f);
        }
    }
    None
}

fn resolved(text: &str) -> Vec<OverseerNode> {
    let mut nodes = parse_document(text).expect("parses").1;
    resolve_document(&mut nodes);
    nodes
}

/// A node's children as a line: each by its name, with what is inside it in braces.
fn outline(node: &OverseerNode) -> String {
    node.children
        .iter()
        .map(|c| {
            if c.children.is_empty() {
                c.name.clone()
            } else {
                format!("{} {{ {} }}", c.name, outline(c))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn text(node: &OverseerNode, key: &str) -> Option<String> {
    match node.parameters.get(key) {
        Some(OverseerValue::String(s)) => Some(s.clone()),
        _ => None,
    }
}

fn instance<'a>(nodes: &'a [OverseerNode], name: &str) -> &'a OverseerNode {
    find(nodes, name).unwrap_or_else(|| panic!("no node named {}", name))
}

const CARD: &str = r##"tab t (label="T") {
    div (hidden=true) {
        div Card {
            string name = "n"
            div {
                string description = "d"
            }
            div (background="#fff") {
                string notes = ""
            }
        }
    }
"##;

fn with_card(body: &str) -> Vec<OverseerNode> {
    resolved(&format!("{}{}\n}}\n", CARD, body))
}

#[test]
fn named_overrides_in_any_order_keep_the_templates_order() {
    let nodes = with_card(
        r##"    <Card> c {
        - description = "D"
        - name = "N"
    }"##,
    );
    let c = instance(&nodes, "c");
    assert_eq!(outline(c), "name div { description } div { notes }");
    assert_eq!(text(find(&c.children, "name").unwrap(), "value").as_deref(), Some("N"));
    assert_eq!(text(find(&c.children, "description").unwrap(), "value").as_deref(), Some("D"));
}

#[test]
fn a_plus_element_lands_right_after_the_field_matched_before_it_at_its_depth() {
    let nodes = with_card(
        r##"    <Card> c {
        - description = "D"
        +string subtitle = "s"
    }"##,
    );
    let c = instance(&nodes, "c");
    assert_eq!(outline(c), "name div { description subtitle } div { notes }");
    let subtitle = find(&c.children, "subtitle").unwrap();
    assert_eq!(subtitle.node_type, "string");
    assert!(subtitle.authored_plus);
    assert!(!subtitle.authored_dash);
}

#[test]
fn a_plus_element_written_first_lands_before_everything() {
    let nodes = with_card(
        r##"    <Card> c {
        + heading = "h"
        - name = "N"
    }"##,
    );
    let c = instance(&nodes, "c");
    assert_eq!(outline(c), "heading name div { description } div { notes }");
    assert_eq!(text(&c.children[0], "value").as_deref(), Some("h"));
}

#[test]
fn an_element_that_matches_nothing_is_kept_last_in_its_own_block() {
    let nodes = with_card(
        r##"    <Card> c {
        - extra = "e"
        div {
            - deeper = "x"
        }
    }"##,
    );
    let c = instance(&nodes, "c");
    assert_eq!(outline(c), "name div { description deeper } div { notes } extra");
}

#[test]
fn an_unnamed_element_reaches_the_unnamed_node_the_walk_comes_to() {
    let nodes = resolved(
        r##"tab t (label="T") {
    div (hidden=true) {
        div Board {
            tab (label="A") {
                div {
                    string a = ""
                }
            }
            tab (label="B") {
                div {
                    string b = ""
                }
                div {
                    string c = ""
                }
            }
        }
    }
    <Board> board {
        tab { }
        tab {
            div { }
            div (background="#eee") { }
        }
    }
}
"##,
    );
    let board = instance(&nodes, "board");
    assert_eq!(outline(board), "tab { div { a } } tab { div { b } div { c } }");
    let backgrounds: Vec<Option<String>> = [
        &board.children[0].children[0],
        &board.children[1].children[0],
        &board.children[1].children[1],
    ]
    .iter()
    .map(|n| text(n, "background"))
    .collect();
    assert_eq!(backgrounds, vec![None, None, Some("#eee".to_string())]);
}

#[test]
fn an_unnamed_element_never_overrides_a_named_one() {
    let nodes = resolved(
        r##"tab t (label="T") {
    div (hidden=true) {
        div Panel {
            div box (background="#fff") {
                string x = ""
            }
        }
    }
    <Panel> p {
        div (background="#000") { }
    }
}
"##,
    );
    let p = instance(&nodes, "p");
    assert_eq!(text(find(&p.children, "box").unwrap(), "background").as_deref(), Some("#fff"));
    assert_eq!(p.children.len(), 2);
    assert_eq!(text(&p.children[1], "background").as_deref(), Some("#000"));
}

#[test]
fn a_nested_overrides_own_parameters_reach_the_template_node() {
    let nodes = resolved(
        r##"tab t (label="T") {
    div (hidden=true) {
        div Panel {
            div box (label="Box", hidden=false) {
                string x = ""
            }
        }
    }
    <Panel> p {
        div box (label="Mine") { }
    }
}
"##,
    );
    let p = instance(&nodes, "p");
    let b = find(&p.children, "box").unwrap();
    assert_eq!(text(b, "label").as_deref(), Some("Mine"));
    assert!(!b.parameters.contains_key("_template_label"));
    assert!(matches!(b.parameters.get("hidden"), Some(OverseerValue::Boolean(false))));
    assert_eq!(outline(p), "box { x }");
}

#[test]
fn the_same_walk_merges_a_list_entry() {
    let nodes = with_card(
        r##"    list Cards (entry=<Card>) {
        - {
            - description = "D"
            + subtitle = "s"
            - extra = "e"
        }
    }"##,
    );
    let cards = instance(&nodes, "Cards");
    let entry = &cards.children[0];
    assert_eq!(outline(entry), "name div { description subtitle } div { notes } extra");
}

#[test]
fn a_plus_never_combines_with_a_dash() {
    // A block steps over what it cannot read a character at a time, so `+-` is read as the `-`
    // after it: an ordinary element, not a new one.
    let nodes = resolved("tab t {\n    +- x = 1\n}\n");
    let x = find(&nodes, "x").unwrap();
    assert!(!x.authored_plus);
    assert!(x.authored_dash);
}

#[test]
fn a_plus_outside_any_template_is_an_ordinary_element() {
    let nodes = resolved("tab t {\n    +int count = 3\n}\n");
    let count = find(&nodes, "count").unwrap();
    assert_eq!(count.node_type, "int");
    assert!(count.authored_plus);
    assert!(!count.authored_dash);
}
