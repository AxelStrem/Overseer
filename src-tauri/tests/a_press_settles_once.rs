//! A press works the document out once, and the right number of times when it needs more.
//!
//! An action that changes the shape of the document - appending an entry, removing one - is
//! followed by a full resolve in place, so that a later action in the same handler traverses a
//! tree that exists. That is right between actions and waste after the last one: whoever asked
//! for the press works the document out afterwards anyway.
//!
//! It used to be decided by asking whether the shape had moved, which answered the wrong
//! question - a caller that settles does so whether it moved or not. So a button holding a single
//! `append`, which is what the Add buttons on a day are, worked the whole document out at the end
//! of the event and again in the caller. On the food tracker that was 800 ms of the 1,600 a
//! logged meal cost.
//!
//! What has to stay true is that the document is settled by the time anybody reads it, by
//! whichever of the two did it.

use overseer::app_api;
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

/// What this field was worked out to be, or what it says if nothing worked it out.
fn value_of(nodes: &[OverseerNode], name: &str) -> String {
    let node = find(nodes, name).unwrap_or_else(|| panic!("no node named `{}`", name));
    match node
        .parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
    {
        Some(OverseerValue::Integer(i)) => i.to_string(),
        Some(OverseerValue::Float(f)) => f.to_string(),
        Some(OverseerValue::String(s)) => s.clone(),
        other => format!("{:?}", other),
    }
}

fn entries(nodes: &[OverseerNode], list: &str) -> usize {
    find(nodes, list).map(|l| l.children.len()).unwrap_or(0)
}

/// A list with a running total over it, and buttons that change its shape.
///
/// `add_one` holds a single append - the shape of the Add buttons on a day, and the case that was
/// being worked out twice. `add_then_count` appends and then writes a value that depends on the
/// list having grown, which is the case that genuinely needs settling between the two.
const DOC: &str = r#"tab t (label="T", mutable=true) {
    div (hidden=true) {
        div Row (layout="horizontal") {
            int amount = 1
            int doubled = $(amount * 2)
        }
    }

    int total (label="Total") = $(Rows.sum(amount))
    int noted (label="Noted") = 0

    button add_one (label="add") {
        on click {
            append (list="/t/Rows") {
                - amount = 5
            }
        }
    }

    button add_then_count (label="add and note") {
        on click {
            append (list="/t/Rows") {
                - amount = 7
            }
            set (path="/t/noted") = $(/t/Rows.map(|x| x/doubled).sum())
        }
    }

    list Rows (entry=<Row>) {
        - {
            - amount = 1
        }
        - {
            - amount = 2
        }
    }
}
"#;

fn press(button: &str) -> Vec<OverseerNode> {
    overseer::source_registry::SourceRegistry::reset();
    let mut nodes = app_api::load_document(DOC.to_string()).expect("load");
    overseer::actions::start_reporting_and_settling();
    app_api::execute_event(&mut nodes, &["t".to_string(), button.to_string()], "click")
        .expect("the press");
    let _ = overseer::actions::take_report();
    // What every caller that says it settles then does.
    overseer::resolver::resolve_document(&mut nodes);
    nodes
}

#[test]
fn a_press_that_only_appends_leaves_a_settled_document() {
    let nodes = press("add_one");
    assert_eq!(entries(&nodes, "Rows"), 3);
    // 1 + 2 + 5, worked out by whoever settled it rather than inside the event.
    assert_eq!(value_of(&nodes, "total"), "8");
}

#[test]
fn a_press_that_appends_and_then_reads_the_list_sees_it_grown() {
    // A handler where a later action reads what an earlier one added. This is what the resolve
    // between actions exists for - and worth saying plainly: it passes with that resolve taken
    // out as well, because a read of a formula that has no worked-out value evaluates it there
    // and then. So this pins the answer rather than the mechanism. The resolve between actions
    // is left exactly as it was; only the one after the *last* action was taken away, and
    // whether the in-between one earns its cost is a separate question from this change.
    let nodes = press("add_then_count");
    assert_eq!(entries(&nodes, "Rows"), 3);
    // (1 + 2 + 7) doubled.
    assert_eq!(value_of(&nodes, "noted"), "20");
    assert_eq!(value_of(&nodes, "total"), "10");
}

#[test]
fn a_caller_that_does_not_settle_still_gets_a_settled_document() {
    // `execute_event_on_text` collects a report and serializes what it is handed without working
    // it out again. It has to be told apart from the three callers that do settle, or the event
    // skips the resolve at its end and that caller is handed an unsettled document.
    overseer::source_registry::SourceRegistry::reset();
    let mut nodes = app_api::load_document(DOC.to_string()).expect("load");
    overseer::actions::start_reporting();
    app_api::execute_event(&mut nodes, &["t".to_string(), "add_one".to_string()], "click")
        .expect("the press");
    let _ = overseer::actions::take_report();

    assert_eq!(entries(&nodes, "Rows"), 3);
    assert_eq!(
        value_of(&nodes, "total"),
        "8",
        "the event did not settle the document for a caller that will not"
    );
}

#[test]
fn a_press_with_no_report_settles_itself() {
    // Nobody collecting a report at all - the plainest caller there is.
    overseer::source_registry::SourceRegistry::reset();
    let mut nodes = app_api::load_document(DOC.to_string()).expect("load");
    app_api::execute_event(&mut nodes, &["t".to_string(), "add_one".to_string()], "click")
        .expect("the press");

    assert_eq!(entries(&nodes, "Rows"), 3);
    assert_eq!(value_of(&nodes, "total"), "8");
}
