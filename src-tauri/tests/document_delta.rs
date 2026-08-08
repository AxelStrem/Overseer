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
    assert!(
        changes
            .iter()
            .any(|c| matches!(c, DocumentChange::Subtree { address, .. } if address == "t/Rows")),
        "the list whose shape changed was not sent, got {:?}",
        changes.iter().map(|c| c.address()).collect::<Vec<_>>()
    );
    // The entries that did not move must not be reported separately - they came with the list.
    assert!(
        !changes.iter().any(|c| c.address().starts_with("t/Rows/[")),
        "entries inside the replaced list were reported again: {:?}",
        changes.iter().map(|c| c.address()).collect::<Vec<_>>()
    );
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
