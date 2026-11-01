//! A list drawn as a tree by a field naming each entry's parent keeps its window by top-level
//! branches, each one whole.
//!
//! Counted by entries, a window would cut a branch in two: a child kept in view under a parent
//! left out would be drawn at the top, as if it had no parent at all.

use overseer::app_api;
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

fn value_of(entry: &OverseerNode, field: &str) -> String {
    let field = entry
        .children
        .iter()
        .find(|c| c.name == field)
        .expect("no such field");
    match field
        .parameters
        .get("_computed_value")
        .or_else(|| field.parameters.get("value"))
    {
        Some(OverseerValue::String(s)) => s.clone(),
        other => panic!("not a string: {other:?}"),
    }
}

/// Entries as `(name, parent)`, in the order written.
fn outline(window: usize, entries: &[(&str, &str)]) -> String {
    let mut text = String::from(
        "tab t (label=\"T\") {\n\
         \x20   div (hidden=true) {\n\
         \x20       div Item {\n\
         \x20           string name = \"\"\n\
         \x20           string parent = \"\"\n\
         \x20       }\n\
         \x20   }\n",
    );
    text.push_str(&format!(
        "\x20   list Items (entry=<Item>, key=\"name\", tree=\"parent\", window={window}) {{\n"
    ));
    for (name, parent) in entries {
        text.push_str(&format!("\x20       - {{\n\x20           - name = \"{name}\"\n"));
        if !parent.is_empty() {
            text.push_str(&format!("\x20           - parent = \"{parent}\"\n"));
        }
        text.push_str("\x20       }\n");
    }
    text.push_str("\x20   }\n}\n");
    text
}

fn in_view(text: String) -> (Vec<String>, Option<OverseerValue>) {
    let nodes = app_api::load_document(text).expect("load");
    let list = find(&nodes, "Items").expect("no Items list");
    let shown = list
        .children
        .iter()
        .filter(|c| c.node_type == "list_item" || c.node_type == "-" || c.node_type == "Item")
        .filter(|c| !matches!(c.parameters.get("_out_of_view"), Some(OverseerValue::Boolean(true))))
        .map(|c| value_of(c, "name"))
        .collect();
    (shown, list.parameters.get("_left_out_of_view").cloned())
}

#[test]
fn a_window_counts_top_level_branches_and_keeps_each_whole() {
    let (shown, left_out) = in_view(outline(
        2,
        &[
            ("a", ""),
            ("a1", "a"),
            ("a1x", "a1"),
            ("b", ""),
            ("c", ""),
            ("c1", "c"),
            ("b1", "b"),
        ],
    ));
    assert_eq!(shown, vec!["a", "a1", "a1x", "b", "b1"]);
    assert_eq!(
        left_out,
        Some(OverseerValue::Integer(1)),
        "the branches left out are counted, not their entries"
    );
}

#[test]
fn an_entry_whose_parent_is_missing_is_a_branch_of_its_own() {
    let (shown, _) = in_view(outline(2, &[("a", "gone"), ("b", ""), ("c", "")]));
    assert_eq!(shown, vec!["a", "b"]);
}

#[test]
fn a_cycle_is_a_branch_that_starts_at_its_first_entry() {
    let (shown, left_out) = in_view(outline(
        1,
        &[("x", "y"), ("y", "x"), ("z", "y"), ("w", "")],
    ));
    assert_eq!(shown, vec!["x", "y", "z"]);
    assert_eq!(left_out, Some(OverseerValue::Integer(1)));
}
