//! The columns a table draws, worked out from the entry template.
//!
//! A row only shows the fields it currently has, and which those are differs from row to row: a
//! task with children shows a percentage, a leaf shows a finish button. So no row knows the whole
//! set, and a list with nothing in it has no rows to ask at all - which is when a heading is
//! worth most. The template knows, and the resolver reads it there.
//!
//! The distinction that makes it work is between a field hidden by its declaration and one hidden
//! by a formula. `hidden=true` is bookkeeping - a number other formulas read and nobody sees - and
//! gets no column. `hidden=$(kids == 0)` is a field that appears on some rows and not others, and
//! must get one: without a column of its own the rows that lack it slide left, and the whole table
//! below stops lining up.

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

/// The column set the renderer is handed, as it is handed it.
fn columns(nodes: &[OverseerNode], list: &str) -> Vec<serde_json::Value> {
    let node = find(nodes, list).unwrap_or_else(|| panic!("no list named `{}`", list));
    match node.parameters.get("_columns") {
        Some(OverseerValue::String(json)) => serde_json::from_str(json).expect("column json"),
        other => panic!("`{}` has no columns: {:?}", list, other),
    }
}

fn names(nodes: &[OverseerNode], list: &str) -> Vec<String> {
    columns(nodes, list)
        .iter()
        .map(|c| c["name"].as_str().unwrap_or("").to_string())
        .collect()
}

const DOC: &str = r#"
tab t (label="T") {
    div (hidden=true) {
        div Row (layout="horizontal") {
            timestamp added (hidden=true) = "2026-01-01T00:00:00Z"
            string handle (label="id", width=10%) = ""
            string title (label="title", width=40%) = ""
            int done (label="done", width=10%, hidden=$(kids == 0)) = 0
            int kids (hidden=true) = 0
            button act (label="do", width=10%) {
                on click {
                    set (path="../done", mode="value") = 1
                }
            }
            string note (label="", span="row", hidden=$(note == "")) = ""
        }
    }

    list Rows (entry=<Row>, view="table", header=true) {
        - {
            - handle = "one"
        }
    }

    list Plain (entry=<Row>) {
        - {
            - handle = "two"
        }
    }
}
"#;

#[test]
fn a_field_hidden_by_a_formula_keeps_its_column() {
    // The one that matters. `done` is not on every row, and if it had no column the rows without
    // it would pull everything after them one place to the left.
    assert!(names(&resolved(DOC), "Rows").contains(&"done".to_string()));
}

#[test]
fn a_field_hidden_by_its_declaration_gets_none() {
    let names = names(&resolved(DOC), "Rows");
    for bookkeeping in ["added", "kids"] {
        assert!(
            !names.contains(&bookkeeping.to_string()),
            "`{}` took a column; it is never drawn, so the table would carry an empty one",
            bookkeeping,
        );
    }
}

#[test]
fn the_columns_are_the_template_in_order() {
    assert_eq!(
        names(&resolved(DOC), "Rows"),
        vec!["handle", "title", "done", "act", "note"],
        "a table reads left to right in the order the template declares",
    );
}

#[test]
fn each_column_carries_its_heading_and_its_width() {
    let columns = columns(&resolved(DOC), "Rows");
    let handle = &columns[0];
    assert_eq!(handle["label"], "id");
    assert_eq!(handle["width"], "10%");

    // The width has to travel, not stay on the field: a percentage left on a grid item is read
    // against its own column rather than the row, and every cell would be a sliver of its share.
    let title = &columns[1];
    assert_eq!(title["width"], "40%");
}

#[test]
fn a_field_can_ask_for_a_line_of_its_own() {
    let columns = columns(&resolved(DOC), "Rows");
    let spanning: Vec<&serde_json::Value> = columns
        .iter()
        .filter(|c| c["span"].as_bool().unwrap_or(false))
        .collect();
    assert_eq!(spanning.len(), 1);
    assert_eq!(spanning[0]["name"], "note");
}

#[test]
fn a_list_that_did_not_ask_gets_nothing() {
    // Every list is templated; only the ones drawn as tables should carry the extra parameter,
    // which travels in every delta for as long as it exists.
    let nodes = resolved(DOC);
    let plain = find(&nodes, "Plain").expect("no Plain list");
    assert!(plain.parameters.get("_columns").is_none());
}

#[test]
fn the_columns_never_reach_the_file() {
    let nodes = resolved(DOC);
    let written = overseer::file_ops::OverseerFileHandler::serialize_nodes(&nodes)
        .expect("serialize");
    assert!(
        !written.contains("_columns"),
        "the column set was written back into the document:\n{}",
        written,
    );
}

#[test]
fn the_project_tracker_draws_the_columns_it_means_to() {
    // The real one, so a field added to the template without a thought for the table shows up
    // here rather than as a column with no heading.
    let nodes = resolved(include_str!("../../examples/projects/project_template.os"));
    assert_eq!(
        names(&nodes, "Items"),
        vec![
            "handle",
            "title",
            "parent",
            "labels",
            "done",
            "points",
            "finish",
            "moved_at",
            "commentary",
        ],
    );

    let columns = columns(&nodes, "Items");
    let headings: Vec<&str> = columns
        .iter()
        .filter(|c| !c["span"].as_bool().unwrap_or(false))
        .map(|c| c["label"].as_str().unwrap_or(""))
        .collect();
    assert!(
        headings.iter().all(|h| !h.is_empty()),
        "a column with no heading reads as an unexplained strip: {:?}",
        headings,
    );

    // The note breaks the table on purpose, which is what it was already doing by accident - the
    // widths came to more than a hundred, so the last cell wrapped.
    let commentary = columns.last().expect("no columns");
    assert_eq!(commentary["name"], "commentary");
    assert_eq!(commentary["span"], true);
}
