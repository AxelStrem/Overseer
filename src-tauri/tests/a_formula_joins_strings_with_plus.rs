//! `+` joins text: `$("the id " + ../id + " is taken")`.
//!
//! The spec listed `+` as string concatenation, and the evaluator turned both sides of every
//! arithmetic operator into numbers, so any sentence built from a field failed with "Cannot
//! convert 'the id ' to number". It came up writing a refusal that names what was typed.
//!
//! The rule is that `+` joins when either side is text that does not read as a number. Values
//! often reach a formula as strings holding numbers, and those still add.

use overseer::app_api;
use overseer::types::*;

const DOCUMENT: &str = r#"tab t (label="T") {
    string id = "x7"
    int seven = 7
    string five = "5"
    string three = "3"
    string blank = ""
    string word = "stage"

    div Out {
        string sentence = $("the id " + ../id + " is taken")
        string counted = $("the id " + ../seven)
        string counted_after = $(../seven + " items")
        int added = $(../five + ../three)
        string with_blank = $(../word + ../blank)
        string a_minus = $(../word - "s")
        string a_times = $(../word * 2)
    }
}
"#;

fn value<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerValue> {
    let tab = nodes.iter().find(|n| n.name == "t").expect("the tab");
    let out = tab.children.iter().find(|c| c.name == "Out").expect("Out");
    let field = out.children.iter().find(|c| c.name == name).unwrap_or_else(|| panic!("no {}", name));
    field.parameters.get("_computed_value")
}

fn text(s: &str) -> Option<OverseerValue> {
    Some(OverseerValue::String(s.to_string()))
}

#[test]
fn text_and_a_field_join_into_a_sentence() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    assert_eq!(value(&nodes, "sentence").cloned(), text("the id x7 is taken"));
}

#[test]
fn a_number_joins_as_it_reads() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    assert_eq!(value(&nodes, "counted").cloned(), text("the id 7"));
    assert_eq!(value(&nodes, "counted_after").cloned(), text("7 items"));
}

#[test]
fn strings_that_are_numbers_still_add() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    let added = value(&nodes, "added");
    assert!(
        matches!(added, Some(OverseerValue::Integer(8))) || matches!(added, Some(OverseerValue::Float(f)) if *f == 8.0),
        "\"5\" + \"3\" is no longer 8: {:?}",
        added
    );
}

#[test]
fn an_empty_field_joins_as_nothing() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    assert_eq!(value(&nodes, "with_blank").cloned(), text("stage"));
}

#[test]
fn only_plus_joins() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    for name in ["a_minus", "a_times"] {
        let v = value(&nodes, name).cloned();
        assert!(
            !matches!(v, Some(OverseerValue::String(ref s)) if s.contains("stage")),
            "{} joined text: {:?}",
            name,
            v
        );
    }
}
