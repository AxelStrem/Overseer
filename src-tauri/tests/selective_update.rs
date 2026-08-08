//! An interaction should be answered with what changed, not with the document.
//!
//! The backend keeps the document it last produced, so it has the state a change is described
//! against without the caller sending anything back - which is the point, since sending the
//! document back is what costs seconds.

use overseer::app_api;
use overseer::docmgr::manager::DocumentManager;
use overseer::types::*;

/// The remembered document is process-wide, as it is in the app - one document is open at a
/// time. Tests in a file share a process and run in parallel, so they take turns.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn start() -> std::sync::MutexGuard<'static, ()> {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    app_api::forget_baseline();
    guard
}

fn open() -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/exercise_tracker/exercise.os");
    DocumentManager::set_current_document(Some(p.to_string_lossy().as_ref()));
    std::fs::read_to_string(&p).unwrap()
}

fn edit(text: String, field: &str, value: OverseerValue) -> app_api::ResolvedUpdate {
    let mut m = std::collections::HashMap::new();
    m.insert(field.to_string(), value);
    app_api::resolve_selective_update(text, vec![field.to_string()], Some(m)).unwrap()
}

#[test]
fn an_edit_after_a_load_is_answered_with_changes() {
    let _turn = start();
    let text = open();
    // Loading establishes what the caller holds.
    let loaded = app_api::load_document(text.clone()).unwrap();
    assert!(!loaded.is_empty());

    let update = edit(
        text,
        "exercise_tracker/Exercises/Exercise__1/plates/p4",
        OverseerValue::Integer(3),
    );

    let changes = update
        .changes
        .expect("the edit was answered with a whole document despite a known baseline");
    assert!(!changes.is_empty(), "the edit reported no changes");
    assert!(
        update.nodes.is_none(),
        "the document was sent as well as the changes"
    );

    let described = serde_json::to_string(&changes).unwrap().len();
    println!("{} changes, {:.1} KB", changes.len(), described as f64 / 1024.0);
    assert!(
        described < 100 * 1024,
        "the change came to {} bytes, which is not a change",
        described
    );
    DocumentManager::set_current_document(None);
}

#[test]
fn an_edit_against_an_unknown_document_is_answered_in_full() {
    let _turn = start();
    let text = open();
    // Deliberately no load first: nothing pairs with this text, so there is no baseline and
    // the caller has to be given something it can adopt outright.

    let update = edit(
        text,
        "exercise_tracker/Exercises/Exercise__1/plates/p4",
        OverseerValue::Integer(3),
    );
    assert!(
        update.nodes.is_some(),
        "no document was sent even though there was no baseline to describe a change against"
    );
    assert!(update.changes.is_none());
    DocumentManager::set_current_document(None);
}

#[test]
fn the_baseline_follows_the_document_across_successive_edits() {
    let _turn = start();
    let text = open();
    let _ = app_api::load_document(text.clone()).unwrap();

    let first = edit(
        text,
        "exercise_tracker/Exercises/Exercise__1/plates/p4",
        OverseerValue::Integer(3),
    );
    assert!(first.changes.is_some(), "the first edit had no baseline");

    // The second edit starts from the text the first returned, as the caller does.
    let second = edit(
        first.text,
        "exercise_tracker/Exercises/Exercise__1/plates/p3",
        OverseerValue::Integer(2),
    );
    assert!(
        second.changes.is_some(),
        "the baseline did not follow the document, so the second edit sent everything"
    );
    DocumentManager::set_current_document(None);
}
