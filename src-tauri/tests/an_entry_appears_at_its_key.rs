//! A list entry created for a key the list did not have.
//!
//! A view onto a list is parameterised by a key. Where no entry has that key the view shows the
//! template as a preview - it looks like an entry, but the document has none. The moment a field
//! in that view is edited the entry becomes real, at that key, and the edit lands on it; from
//! then on it is an ordinary view onto an ordinary entry.
//!
//! `ensure_in_list` is that sentence in the backend: find by key, do nothing if it is there,
//! otherwise clone the template with the key set. The page does not call it - it clones the
//! template in its own copy of the document and leaves the whole text to be saved, which is the
//! last page write able to discard somebody else's. Before moving the page onto it, this pins
//! down what it does today, including where it differs from the two paths that also make entries.

use overseer::actions::ActionExecutor;
use overseer::file_ops::OverseerFileHandler;
use overseer::parser::parse_document;
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

/// The entries of a list, in the order the file holds them.
fn entries<'a>(nodes: &'a [OverseerNode], list: &str) -> Vec<&'a OverseerNode> {
    find(nodes, list)
        .unwrap_or_else(|| panic!("no list named `{}`", list))
        .children
        .iter()
        .collect()
}

fn field_of(entry: &OverseerNode, name: &str) -> Option<String> {
    entry
        .children
        .iter()
        .find(|c| c.name == name)
        .and_then(|f| {
            f.parameters
                .get("value")
                .or_else(|| f.parameters.get("_computed_value"))
        })
        .map(|v| match v {
            OverseerValue::String(s) => s.clone(),
            other => format!("{:?}", other),
        })
}

/// A history keyed by date, with two days in it and a button that asks for a third.
///
/// Shaped after the one document in use that does this: a day record holding a target the
/// document asks to be frozen when the day is created, and a nested list inside it.
const DOC: &str = r#"tab t (label="T", mutable=true) {
    div (hidden=true) {
        div DayRecord (layout="vertical") {
            timestamp date (precision="day") = $(today())
            float weight = null

            div targets {
                float target_calories (freeze=true, fallback=$(/t/limits/standing)) = null
            }

            list intake (entry=<Meal>)
        }

        div Meal (layout="horizontal") {
            string food = ""
        }
    }

    div limits {
        float standing = 1800
    }

    button make_a_day (label="make a day") {
        on click {
            ensure_in_list (list="/t/History", keyField="date", keyValue="2026-08-05", template="<DayRecord>")
        }
    }

    button make_it_first (label="make it first") {
        on click {
            ensure_in_list (list="/t/History", keyField="date", keyValue="2026-08-05",
                            template="<DayRecord>", position="first")
        }
    }

    list History (entry=<DayRecord>, key="date", keyPrecision="day") {
        - {
            - date = "2026-08-03"
            - weight = 92.3
        }
        - {
            - date = "2026-08-04"
            - weight = 93.7
        }
    }
}
"#;

fn a_document() -> Vec<OverseerNode> {
    overseer::source_registry::SourceRegistry::reset();
    let mut nodes = parse_document(DOC).expect("parse").1;
    resolver::resolve_document(&mut nodes);
    nodes
}

fn ask_for_the_day(nodes: &mut Vec<OverseerNode>) {
    ActionExecutor::execute_event(nodes, &["t".to_string(), "make_a_day".to_string()], "click")
        .expect("the day was refused");
}

fn ask_for_the_day_at_the_front(nodes: &mut Vec<OverseerNode>) {
    ActionExecutor::execute_event(nodes, &["t".to_string(), "make_it_first".to_string()], "click")
        .expect("the day was refused");
}

fn dates(nodes: &[OverseerNode]) -> Vec<String> {
    entries(nodes, "History")
        .iter()
        .filter_map(|e| field_of(e, "date"))
        .collect()
}

#[test]
fn the_entry_is_created_with_the_key_it_was_asked_for() {
    let mut nodes = a_document();
    ask_for_the_day(&mut nodes);
    let made = dates(&nodes);
    assert!(
        made.iter().any(|d| d == "2026-08-05"),
        "no entry carries the key that was asked for: {:?}",
        made
    );
}

#[test]
fn asking_twice_makes_one_entry() {
    // The page may commit several fields in quick succession on a view that was a preview a
    // moment ago, and each of them asks. Only the first can create anything.
    let mut nodes = a_document();
    ask_for_the_day(&mut nodes);
    ask_for_the_day(&mut nodes);
    assert_eq!(entries(&nodes, "History").len(), 3, "{:?}", dates(&nodes));
}

#[test]
fn the_entry_gets_the_templates_fields() {
    let mut nodes = a_document();
    ask_for_the_day(&mut nodes);
    let made = entries(&nodes, "History")
        .into_iter()
        .find(|e| field_of(e, "date").as_deref() == Some("2026-08-05"))
        .expect("the entry was not made");
    for wanted in ["date", "weight", "targets", "intake"] {
        assert!(
            made.children.iter().any(|c| c.name == wanted),
            "the new entry has no `{}`: {:?}",
            wanted,
            made.children.iter().map(|c| &c.name).collect::<Vec<_>>()
        );
    }
}

#[test]
fn the_entry_is_frozen_the_way_a_day_made_any_other_way_is() {
    // `freeze=true` writes the standing figure into the entry once, at creation, so the history
    // does not later claim every past day was aiming at whatever today's figure is. It works off
    // a mark set when the entry is made, and the two paths that append or prepend one set it.
    // This one did not, so a day created through a view's key quietly kept reading the standing
    // figure for ever - which is how every day in the tracker is created.
    let mut nodes = a_document();
    ask_for_the_day(&mut nodes);
    resolver::resolve_document(&mut nodes);
    let text = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
    let after_the_key = text
        .rsplit("2026-08-05")
        .next()
        .expect("the new day is not in the file");
    assert!(
        after_the_key.contains("target_calories = 1800"),
        "the new day did not freeze its target:\n{}",
        text
    );
}

#[test]
fn the_entry_is_written_with_the_same_indentation_as_the_others() {
    // The other two paths ask the list how its entries are laid out and follow it. An entry that
    // does not is a diff on lines nobody touched.
    let mut nodes = a_document();
    ask_for_the_day(&mut nodes);
    resolver::resolve_document(&mut nodes);
    let text = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| l.trim_start().starts_with("- date = "))
        .collect();
    let indents: Vec<usize> = lines
        .iter()
        .map(|l| l.len() - l.trim_start().len())
        .collect();
    assert!(
        !indents.is_empty() && indents.windows(2).all(|w| w[0] == w[1]),
        "the entries are not indented alike: {:?}\n{}",
        lines,
        text
    );
}

#[test]
fn the_entry_goes_last_when_nothing_says_otherwise() {
    let mut nodes = a_document();
    ask_for_the_day(&mut nodes);
    assert_eq!(dates(&nodes), ["2026-08-03", "2026-08-04", "2026-08-05"]);
}

#[test]
fn or_first_where_the_document_asks_for_that() {
    // A keyed history is read in whatever order `sort_by` says and written at either end, so
    // which end is a choice the document makes - `phantom-materialize` says `append-on-edit` or
    // `prepend-on-edit`, and this is where that arrives.
    let mut nodes = a_document();
    ask_for_the_day_at_the_front(&mut nodes);
    assert_eq!(dates(&nodes), ["2026-08-05", "2026-08-03", "2026-08-04"]);
}

#[test]
fn an_entry_made_first_is_indented_like_the_rest() {
    let mut nodes = a_document();
    ask_for_the_day_at_the_front(&mut nodes);
    resolver::resolve_document(&mut nodes);
    let text = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
    let indents: Vec<usize> = text
        .lines()
        .filter(|l| l.trim_start().starts_with("- date = "))
        .map(|l| l.len() - l.trim_start().len())
        .collect();
    assert!(
        !indents.is_empty() && indents.windows(2).all(|w| w[0] == w[1]),
        "the entries are not indented alike:\n{}",
        text
    );
}
