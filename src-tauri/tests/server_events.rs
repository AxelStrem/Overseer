//! Pressing what a document declares.
//!
//! The document decides what may happen: an event runs an `on <event>` block already written
//! there. That is the difference between a caller doing what the document describes and a
//! caller reaching in and rearranging it - and it means the button a person presses and the
//! button a program presses are the same button, with no second implementation to drift.

use overseer::server::{DocumentRoot, RequestError};
use overseer::types::*;

fn sandbox(tag: &str) -> (std::path::PathBuf, DocumentRoot) {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Secrebot/documents");
    let root = std::env::temp_dir().join(format!("overseer_events_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    if !source.join("exercise.os").exists() {
        panic!("expected a migrated exercise.os beside this repository");
    }
    std::fs::copy(source.join("exercise.os"), root.join("exercise.os")).unwrap();
    let documents = DocumentRoot::new(&root).unwrap();
    (root, documents)
}

fn field(node: &OverseerNode, name: &str) -> Option<OverseerValue> {
    let child = node.children.iter().find(|c| c.name == name)?;
    child
        .parameters
        .get("_computed_value")
        .or_else(|| child.parameters.get("value"))
        .cloned()
}

fn history_len(documents: &DocumentRoot) -> usize {
    documents
        .read_at("exercise.os", "exercise_tracker/History")
        .expect("history")
        .node
        .children
        .len()
}

#[test]
fn pressing_done_records_the_exercise() {
    let (root, documents) = sandbox("done");
    let before = history_len(&documents);
    let exercise = documents
        .read_at("exercise.os", "exercise_tracker/Exercises/[bicep_curls]")
        .expect("the exercise should be addressable by its handle");
    let last_done_before = field(&exercise.node, "last_done");

    let outcome = documents
        .run_event(
            "exercise.os",
            "exercise_tracker/Exercises/[bicep_curls]/done",
            "click",
        )
        .expect("Done should be pressable");
    assert!(!outcome.gone);

    assert_eq!(
        history_len(&documents),
        before + 1,
        "pressing Done did not add a record"
    );

    // The answer says the press took effect, without a second request.
    let after = outcome.node.expect("the exercise should come back");
    assert_ne!(
        field(&after, "last_done"),
        last_done_before,
        "last_done did not move, so the record was not attributed to this exercise"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_new_record_lands_at_the_end() {
    // Done appends now. If it prepended, every existing record's address would shift, and a
    // caller that read the history and then pressed would be acting on the wrong one.
    let (root, documents) = sandbox("append");
    let before = documents
        .read_at("exercise.os", "exercise_tracker/History")
        .unwrap();
    let first_before = before.child_addresses.first().cloned();
    let count = before.node.children.len();

    documents
        .run_event(
            "exercise.os",
            "exercise_tracker/Exercises/[push_ups]/done",
            "click",
        )
        .expect("press");

    let after = documents
        .read_at("exercise.os", "exercise_tracker/History")
        .unwrap();
    assert_eq!(after.node.children.len(), count + 1);
    assert_eq!(
        after.child_addresses.first().cloned(),
        first_before,
        "the first record moved, so every address shifted"
    );
    let newest = after.node.children.last().expect("the new record");
    assert_eq!(
        field(newest, "description"),
        Some(OverseerValue::String("Push Ups".into())),
        "the record at the end is not the one just added"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_records_own_remove_button_removes_it() {
    let (root, documents) = sandbox("remove");
    let history = documents
        .read_at("exercise.os", "exercise_tracker/History")
        .unwrap();
    let count = history.node.children.len();
    let last = history.child_addresses.last().cloned().expect("a record");

    let outcome = documents
        .run_event("exercise.os", &format!("{}/rem", last), "click")
        .expect("rem should be pressable");
    assert!(outcome.gone, "the record it was pressed on should be gone");
    assert_eq!(history_len(&documents), count - 1);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_event_nothing_declares_is_refused() {
    let (root, documents) = sandbox("undeclared");
    let outcome = documents.run_event(
        "exercise.os",
        "exercise_tracker/Exercises/[bicep_curls]",
        "explode",
    );
    // Refused, rather than quietly doing nothing. This used to answer Ok - the reasoning being
    // that no `on explode` block exists, so there is nothing to run - and the caller could not
    // tell that apart from a press that worked. It is how `.../bought/click` was pressed three
    // times on a shopping item, accepted each time, without the item ever moving to history.
    match outcome {
        Err(RequestError::Rejected(said)) => {
            assert!(said.contains("explode"), "it did not name the event: {said}");
        }
        Err(other) => panic!("refused for the wrong reason: {other:?}"),
        Ok(_) => panic!("an event nothing declares was not refused"),
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_address_that_names_nothing_is_reported() {
    let (root, documents) = sandbox("missing");
    assert!(matches!(
        documents.run_event("exercise.os", "exercise_tracker/Nowhere/done", "click"),
        Err(RequestError::NotFound(_))
    ));
    let _ = std::fs::remove_dir_all(&root);
}
