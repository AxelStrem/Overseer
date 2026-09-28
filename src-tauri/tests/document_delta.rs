//! A change should be described, not re-sent.
//!
//! An interaction answers with the whole resolved document today: ~9 MB for exercise.os, moved
//! over an IPC that manages a couple of MB per second, and then rebuilt element by element by
//! the client. A field edit changes a handful of nodes, so the answer should be a handful of
//! nodes.

use overseer::app_api;
use overseer::delta::{self, DocumentChange};
use overseer::docmgr::manager::DocumentManager;
use overseer::types::*;

fn exercise() -> (String, Vec<OverseerNode>) {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/exercise_tracker/exercise.os");
    DocumentManager::set_current_document(Some(p.to_string_lossy().as_ref()));
    let text = std::fs::read_to_string(&p).unwrap();
    let nodes = app_api::load_document(text.clone()).unwrap();
    (text, nodes)
}

#[test]
fn an_unchanged_document_produces_no_changes() {
    let (text, before) = exercise();
    let again = app_api::load_document(text).unwrap();
    let changes = delta::diff(&before, &again);
    assert!(
        changes.is_empty(),
        "resolving the same document twice reported {} changes, first at {}",
        changes.len(),
        changes.first().map(|c| c.address()).unwrap_or("?")
    );
    DocumentManager::set_current_document(None);
}

#[test]
fn a_field_edit_is_a_handful_of_changes_not_a_document() {
    let (text, before) = exercise();
    let field = "exercise_tracker/Exercises/Exercise__1/plates/p4";
    let mut m = std::collections::HashMap::new();
    m.insert(field.to_string(), OverseerValue::Integer(3));
    let after = app_api::resolve_selective(text, vec![field.to_string()], Some(m)).unwrap();

    let changes = delta::diff(&before, &after);
    assert!(!changes.is_empty(), "the edit produced no changes at all");

    let full = serde_json::to_string(&after).unwrap().len();
    let delta_size = serde_json::to_string(&changes).unwrap().len();
    println!(
        "full document {:.1} MB, delta {:.1} KB ({:.3}%), {} changes",
        full as f64 / 1048576.0,
        delta_size as f64 / 1024.0,
        100.0 * delta_size as f64 / full as f64,
        changes.len()
    );
    for c in changes.iter().take(6) {
        println!("  {}", c.address());
    }

    // The point of the exercise: an edit must not cost anything like the document.
    assert!(
        delta_size * 20 < full,
        "the delta is {} bytes against a {} byte document - not worth sending",
        delta_size,
        full
    );

    // The edited field must actually be in there.
    assert!(
        changes.iter().any(|c| c.address().ends_with("/plates/p4")),
        "the edited field is not among the changes"
    );
    DocumentManager::set_current_document(None);
}


/// Follow child indices, as the recipient does.
fn at<'a>(nodes: &'a [OverseerNode], path: &[usize]) -> Option<&'a OverseerNode> {
    let (first, rest) = path.split_first()?;
    let mut node = nodes.get(*first)?;
    for i in rest {
        node = node.children.get(*i)?;
    }
    Some(node)
}

#[test]
fn an_index_path_reaches_the_same_node_in_the_document_being_updated() {
    // The recipient holds the old document and applies paths computed from the new one. That
    // only works because a change is never reported below an ancestor whose shape moved, so
    // every level above it holds the same nodes in the same order in both.
    let (text, before) = exercise();
    let field = "exercise_tracker/Exercises/Exercise__1/plates/p4";
    let mut m = std::collections::HashMap::new();
    m.insert(field.to_string(), OverseerValue::Integer(3));
    let after = app_api::resolve_selective(text, vec![field.to_string()], Some(m)).unwrap();

    let changes = delta::diff(&before, &after);
    assert!(!changes.is_empty());
    for change in &changes {
        let target = at(&before, change.path()).unwrap_or_else(|| {
            panic!(
                "the path for {} does not reach anything in the document being updated",
                change.address()
            )
        });
        let expected = overseer::addressing::find(&before, change.address()).unwrap();
        assert_eq!(
            target.name, expected.name,
            "the path for {} reaches '{}', but that address holds '{}'",
            change.address(), target.name, expected.name
        );
        assert_eq!(
            at(&after, change.path()).map(|n| n.name.clone()),
            Some(target.name.clone()),
            "the path for {} names different nodes in the two documents",
            change.address()
        );
    }
    DocumentManager::set_current_document(None);
}

const KEYED: &str = r#"tab t (mutable=true) {
    div (hidden=true) {
        div Row (layout="vertical") {
            string id = ""
            int qty = 0
        }
    }
    list Rows (entry=<Row>, key="id") {
        - {
            - id = "a"
            - qty = 1
        }
        - {
            - id = "b"
            - qty = 2
        }
    }
}
"#;

#[test]
fn an_inserted_entry_reports_the_list_and_leaves_its_neighbours_alone() {
    DocumentManager::set_current_document(None);
    let before = app_api::load_document(KEYED.to_string()).unwrap();
    let inserted = KEYED.replace(
        "    list Rows (entry=<Row>, key=\"id\") {\n",
        "    list Rows (entry=<Row>, key=\"id\") {\n        - {\n            - id = \"z\"\n            - qty = 9\n        }\n",
    );
    let after = app_api::load_document(inserted).unwrap();

    let changes = delta::diff(&before, &after);
    let entries = changes
        .iter()
        .find_map(|c| match c {
            DocumentChange::Entries { address, entries, .. } if address == "t/Rows" => Some(entries),
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!(
                "the list whose entries changed was not reported by them, got {:?}",
                changes.iter().map(|c| c.address()).collect::<Vec<_>>()
            )
        });
    assert!(matches!(&entries[0], delta::Entry::New { node } if node.name == "Row__1"), "{:?}", entries[0]);
    assert!(
        matches!(&entries[1..], [delta::Entry::Kept { kept: 0, name: a }, delta::Entry::Kept { kept: 1, name: b }]
            if a == "Row__2" && b == "Row__3"),
        "the neighbours were not kept, under the names their places give them now: {:?}",
        &entries[1..]
    );
    // The entries that did not move must not be reported separately - they were kept as they were.
    assert!(
        !changes.iter().any(|c| c.address().starts_with("t/Rows/[")),
        "entries that did not change were reported again: {:?}",
        changes.iter().map(|c| c.address()).collect::<Vec<_>>()
    );
}

/// What the page shows of a document: every node's name, type and parameters, and its children.
fn shown(nodes: &[OverseerNode]) -> serde_json::Value {
    serde_json::Value::Array(
        nodes
            .iter()
            .map(|n| {
                serde_json::json!({
                    "name": n.name,
                    "type": n.node_type,
                    "parameters": serde_json::to_value(&n.parameters).unwrap(),
                    "children": shown(&n.children),
                })
            })
            .collect(),
    )
}

/// The changes between two texts, having checked that applied to the first they give the second.
fn applied(before: &str, after: &str) -> Vec<DocumentChange> {
    DocumentManager::set_current_document(None);
    let before = app_api::load_document(before.to_string()).unwrap();
    let after = app_api::load_document(after.to_string()).unwrap();
    let changes = delta::diff(&before, &after);
    let mut held = before.clone();
    delta::apply(&mut held, changes.clone());
    assert_eq!(shown(&held), shown(&after), "the changes applied do not give the document: {:?}",
        changes.iter().map(|c| c.address()).collect::<Vec<_>>());
    changes
}

const PLACED: &str = r#"tab t (mutable=true) {
    div (hidden=true) {
        div Meal (layout="vertical") {
            float grams = 0
            float calories = $(grams * 2)
        }
    }
    list intake (entry=<Meal>) {
        - {
            - grams = 1
        }
        - {
            - grams = 2
        }
        - {
            - grams = 3
        }
    }
    float total = $(intake.map(|x| x/calories).sum())
}
"#;

#[test]
fn applied_to_the_document_before_the_changes_give_the_document_after() {
    let b = "        - {\n            - id = \"b\"\n            - qty = 2\n        }\n";
    let z = "        - {\n            - id = \"z\"\n            - qty = 9\n        }\n";
    let rows = "    list Rows (entry=<Row>, key=\"id\") {\n";
    // Keyed: one put in front, one taken out, one added at the end, and one put in front while
    // another changes - what changed inside that one is located in the new order.
    applied(KEYED, &KEYED.replace(rows, &format!("{}{}", rows, z)));
    applied(KEYED, &KEYED.replace(b, ""));
    applied(KEYED, &KEYED.replace(b, &format!("{}{}", b, z)));
    let changes = applied(KEYED, &KEYED.replace(rows, &format!("{}{}", rows, z)).replace("- qty = 2", "- qty = 5"));
    assert!(changes.iter().any(|c| matches!(c, DocumentChange::Parameters { address, .. } if address.starts_with("t/Rows/[b]"))));

    // Named by place: taking out the middle one renames the last, and what it holds differs from
    // what was held under that name.
    let two = "        - {\n            - grams = 2\n        }\n";
    applied(PLACED, &PLACED.replace(two, ""));
    applied(PLACED, &PLACED.replace(two, &format!("{}        - {{\n            - grams = 7\n        }}\n", two)));
    applied(PLACED, &PLACED.replace("    list intake (entry=<Meal>) {\n", "    list intake (entry=<Meal>) {\n        - {\n            - grams = 7\n        }\n"));
}

#[test]
fn an_entry_added_to_a_long_list_costs_the_entry_not_the_list() {
    let many: String = (0..200)
        .map(|i| format!("        - {{\n            - id = \"r{:03}\"\n            - qty = {}\n        }}\n", i, i))
        .collect();
    let rows = "    list Rows (entry=<Row>, key=\"id\") {\n";
    let long = KEYED.replace(rows, &format!("{}{}", rows, many));
    let one_more = long.replace(rows, &format!("{}        - {{\n            - id = \"new\"\n            - qty = 1\n        }}\n", rows));
    let changes = applied(&long, &one_more);
    DocumentManager::set_current_document(None);
    let list = serde_json::to_string(overseer::addressing::find(&app_api::load_document(one_more).unwrap(), "t/Rows").unwrap()).unwrap().len();
    let sent = serde_json::to_string(&changes).unwrap().len();
    assert!(sent * 10 < list, "one entry added sent {} bytes of a {} byte list", sent, list);
}

#[test]
fn a_removed_entry_is_reported_as_removed() {
    DocumentManager::set_current_document(None);
    let before = app_api::load_document(KEYED.to_string()).unwrap();
    let without = KEYED.replace("        - {\n            - id = \"b\"\n            - qty = 2\n        }\n", "");
    assert_ne!(without, KEYED, "the fixture did not change");
    let after = app_api::load_document(without).unwrap();

    let changes = delta::diff(&before, &after);
    assert!(
        changes.iter().any(|c| c.address() == "t/Rows"),
        "the list that lost an entry was not reported: {:?}",
        changes.iter().map(|c| c.address()).collect::<Vec<_>>()
    );
}
