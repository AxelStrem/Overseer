//! A field may carry a value and a body: `int done (...) = 0 { on change { ... } }`.
//!
//! These were exclusive, in both halves. The parser read a body after a value as the next
//! sibling, so its closing brace closed the enclosing block early - which is what
//! `tracker_v2.os` records as the reason it cannot clear the counterpart of an amount. And the
//! serializer, given a node with both, wrote the value and dropped the children on the floor
//! without saying so.
//!
//! The consequence was that no field could react to being edited. A project item could not
//! stamp when its progress last moved, so the stamp had to be a button that could be forgotten.
//!
//! Round-tripping is most of what is tested here. A parser change that reads a document
//! correctly and writes it back differently is worse than one that fails outright, because the
//! damage lands in the file.

use overseer::app_api;
use overseer::file_ops::OverseerFileHandler;
use overseer::types::OverseerNode;

const DOCUMENT: &str = r#"tab t (label="T", mutable=true) {

    int done (label="", format="trim", suffix="%") = 40 {
        on change {
            set (path="../moved_at", mode="value") = $(now())
        }
    }

    timestamp moved_at = ""

    text after (markdown=true) = "still inside the tab"
}
"#;

fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    for node in nodes {
        if node.name == name {
            return Some(node);
        }
        if let Some(found) = find(&node.children, name) {
            return Some(found);
        }
    }
    None
}

fn value_of(nodes: &[OverseerNode], name: &str) -> String {
    find(nodes, name)
        .and_then(|field| {
            field
                .parameters
                .get("_computed_value")
                .or(field.parameters.get("value"))
                .cloned()
        })
        .map(|value| format!("{value:?}"))
        .unwrap_or_else(|| "(none)".to_string())
}

#[test]
fn the_body_does_not_close_the_block_it_is_in() {
    // The whole failure, in one assertion. `moved_at` and `after` belong to the tab; if the
    // handler's brace closes the tab, they become siblings of it instead.
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("load");
    assert_eq!(nodes.len(), 1, "the document should have one root, the tab");

    let tab = &nodes[0];
    let named: Vec<&str> = tab.children.iter().map(|c| c.name.as_str()).collect();
    assert!(named.contains(&"done"), "no done field: {named:?}");
    assert!(named.contains(&"moved_at"), "moved_at fell out of the tab: {named:?}");
    assert!(named.contains(&"after"), "after fell out of the tab: {named:?}");
}

#[test]
fn the_field_keeps_its_value_and_its_handler() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("load");
    let done = find(&nodes, "done").expect("no done field");

    assert_eq!(value_of(&nodes, "done"), "Integer(40)", "the value was lost");
    assert!(
        done.children.iter().any(|c| c.node_type == "on" && c.name == "change"),
        "the handler was lost: {:?}",
        done.children.iter().map(|c| (&c.node_type, &c.name)).collect::<Vec<_>>()
    );
    // And its parameters survived the body, which sits after them.
    assert!(done.parameters.contains_key("suffix"), "the parameters were lost");
}

#[test]
fn it_writes_back_exactly_as_it_was_written() {
    // The one that matters. Read the document, serialize it, and require the same bytes:
    // anything else is damage to the file rather than a bug on screen.
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("load");
    let written = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
    assert_eq!(written, DOCUMENT, "the document did not survive a round trip");
}

#[test]
fn it_survives_being_round_tripped_twice() {
    // A serializer that is stable only on the first pass is not stable.
    let once = OverseerFileHandler::serialize_nodes(
        &app_api::load_document(DOCUMENT.to_string()).expect("load"),
    )
    .expect("serialize");
    let twice = OverseerFileHandler::serialize_nodes(
        &app_api::load_document(once.clone()).expect("reload"),
    )
    .expect("serialize again");
    assert_eq!(once, twice, "the second pass wrote something different");
}

#[test]
fn the_handler_runs_and_writes_what_it_was_told_to() {
    let path: Vec<String> = ["t", "done"].iter().map(|s| s.to_string()).collect();
    let update = app_api::execute_event_update(DOCUMENT.to_string(), path, "change".into())
        .expect("the change should run");
    let after = app_api::load_document(update.text).expect("reload");

    assert_ne!(value_of(&after, "moved_at"), "String(\"\")", "the handler did not fire");
    assert_eq!(
        value_of(&after, "done"),
        "Integer(40)",
        "running the handler disturbed the value it was attached to"
    );
}

#[test]
fn a_value_edited_after_the_handler_ran_is_kept() {
    // The pairing the project tracker needs: type a percentage, and the stamp follows.
    let path: Vec<String> = ["t", "done"].iter().map(|s| s.to_string()).collect();
    let edited = DOCUMENT.replace("= 40 {", "= 65 {");
    let update = app_api::execute_event_update(edited, path, "change".into()).expect("event");
    let after = app_api::load_document(update.text.clone()).expect("reload");

    assert_eq!(value_of(&after, "done"), "Integer(65)");
    assert_ne!(value_of(&after, "moved_at"), "String(\"\")");
    // And what was written is still a document that reads the same way.
    assert!(update.text.contains("= 65 {"), "the value and body came apart: {}", update.text);
}

#[test]
fn a_field_with_no_body_is_untouched_by_any_of_this() {
    // Every document that exists is of this shape, so it is the regression that would matter.
    const PLAIN: &str = r#"tab t (label="T", mutable=true) {
    int a (label="a") = 5
    string b = "text"

    div grouped (layout="horizontal") {
        int c = 1
    }
}
"#;
    let nodes = app_api::load_document(PLAIN.to_string()).expect("load");
    let written = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
    assert_eq!(written, PLAIN, "an ordinary document stopped round-tripping");
}
