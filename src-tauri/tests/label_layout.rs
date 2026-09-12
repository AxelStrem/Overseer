//! Where a field's label goes, decided the same way a container's layout is.
//!
//! A div alternates with its nesting - horizontal inside vertical, vertical inside that - and a
//! field arranges two things of its own, its label and its value. Those are the same question, so
//! they get the same answer: a field in a horizontal row wears its label above the value, because
//! a row is already using the horizontal axis; a field in a vertical column wears it beside,
//! because a column of "name: value" lines is half the height of a column of pairs.
//!
//! `layout` on the field overrides it, in the vocabulary a div already uses.

use overseer::parser;
use overseer::resolver;
use overseer::types::{OverseerNode, OverseerValue};

fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    for node in nodes {
        if node.name == name {
            return Some(node);
        }
        if let Some(hit) = find(&node.children, name) {
            return Some(hit);
        }
    }
    None
}

fn resolved(source: &str) -> Vec<OverseerNode> {
    let (_rest, mut nodes) = parser::parse_document(source).expect("parse");
    resolver::resolve_document(&mut nodes);
    nodes
}

/// Where this field puts its label.
fn label_layout(nodes: &[OverseerNode], name: &str) -> String {
    let node = find(nodes, name).unwrap_or_else(|| panic!("no node named `{}`", name));
    match node.parameters.get("_label_layout") {
        Some(OverseerValue::String(s)) => s.clone(),
        other => panic!("`{}` has no label layout: {:?}", name, other),
    }
}

const DOC: &str = r#"
tab t (label="T") {
    string at_the_top (label="top") = ""

    div column (layout="vertical") {
        string in_a_column (label="a") = ""

        div row (layout="horizontal") {
            string in_a_row (label="b") = ""

            div deeper (layout="vertical") {
                string deeper_still (label="c") = ""
            }
        }
    }
}
"#;

#[test]
fn a_label_alternates_with_the_nesting() {
    let nodes = resolved(DOC);

    // A column arranges its children down the page, so a field in it has the width to spare and
    // spends it on the label.
    assert_eq!(label_layout(&nodes, "in_a_column"), "horizontal");

    // A row is already spending the horizontal axis, so the label goes above.
    assert_eq!(label_layout(&nodes, "in_a_row"), "vertical");

    // And it keeps alternating, as a div's does.
    assert_eq!(label_layout(&nodes, "deeper_still"), "horizontal");
}

#[test]
fn a_field_under_a_tab_reads_as_being_in_a_column() {
    // A tab is not a layout container, so nothing gives it a layout to be the opposite of. It
    // stacks its children down the page, so the answer that matches what is on screen is the one
    // a vertical container would give.
    let nodes = resolved(DOC);
    assert_eq!(label_layout(&nodes, "at_the_top"), "horizontal");
}

#[test]
fn the_field_can_say_where_its_label_goes() {
    let source = r#"
tab t (label="T") {
    div row (layout="horizontal") {
        string pinned (label="a", layout="horizontal") = ""
        string same_as_the_row (label="b", layout="inherit") = ""
        string flipped (label="c", layout="opposite") = ""
        string left_alone (label="d") = ""
    }
}
"#;
    let nodes = resolved(source);

    assert_eq!(label_layout(&nodes, "pinned"), "horizontal");
    assert_eq!(label_layout(&nodes, "same_as_the_row"), "horizontal");
    assert_eq!(label_layout(&nodes, "flipped"), "vertical");
    // Which is what `opposite` already means, and what the default already does.
    assert_eq!(label_layout(&nodes, "left_alone"), "vertical");
}

#[test]
fn a_field_is_not_a_layout_parent_for_what_is_under_it() {
    // The hazard in stamping a layout onto something that is not a container: a field can have
    // children - a handler, an entry override - and if the field became their parent for layout
    // purposes, everything below would flip. They inherit from the row, as they did before.
    let source = r#"
tab t (label="T") {
    div row (layout="horizontal") {
        int counter (label="n") = 0 {
            on change {
                set (path="../counter") = 1
            }
        }

        div inside_the_row (layout="vertical") {
            string nested (label="x") = ""
        }
    }
}
"#;
    let nodes = resolved(source);

    // The field itself: in a row, so its label goes above it.
    assert_eq!(label_layout(&nodes, "counter"), "vertical");

    // Its sibling div is the control: still the opposite of the row. If the field had become a
    // layout parent, a div under it would have read from `vertical` instead and come out wrong.
    let inside = find(&nodes, "inside_the_row").expect("no div");
    assert_eq!(
        inside.parameters.get("_effective_layout"),
        Some(&OverseerValue::String("vertical".to_string())),
        "the explicit layout on the div stopped being respected",
    );
    assert_eq!(label_layout(&nodes, "nested"), "horizontal");
}

#[test]
fn it_is_not_written_back_to_the_document() {
    // Resolved parameters are the renderer's business. An underscore key is skipped by the
    // serializer, which is what keeps a document from growing a layout field it never declared -
    // the round-trip tests would catch it, but the reason belongs here.
    let source = "tab t (label=\"T\") {\n    string a (label=\"a\") = \"\"\n}\n";
    let nodes = resolved(source);
    let written = overseer::file_ops::OverseerFileHandler::serialize_nodes(&nodes)
        .expect("serialize");
    assert!(
        !written.contains("_label_layout"),
        "the layout reached the file:\n{}",
        written,
    );
}
