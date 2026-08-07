//! The text returned with a resolve is the basis for the next edit, so edits must accumulate
//! through it.
//!
//! The frontend no longer uploads the document to have it serialized - that costs seconds on a
//! large one - and instead keeps the text the backend returns. If an edit did not survive into
//! that text, the next edit would silently undo it.

use overseer::app_api;
use overseer::docmgr::manager::DocumentManager;
use overseer::types::*;

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

fn value_of(nodes: &[OverseerNode], instance: &str, field: &str) -> Option<OverseerValue> {
    find(nodes, instance)?
        .children
        .iter()
        .find(|c| c.name == field)?
        .parameters
        .get("value")
        .cloned()
}

fn edit(text: String, path: &str, value: OverseerValue) -> app_api::ResolvedDocument {
    let mut m = std::collections::HashMap::new();
    m.insert(path.to_string(), value);
    app_api::resolve_selective_with_text(text, vec![path.to_string()], Some(m)).unwrap()
}

#[test]
fn successive_edits_accumulate_through_the_returned_text() {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/exercise_tracker/exercise.os");
    DocumentManager::set_current_document(Some(p.to_string_lossy().as_ref()));
    let text = std::fs::read_to_string(&p).unwrap();

    let first = edit(text, "exercise_tracker/Exercises/Exercise__1/reps", OverseerValue::Integer(7));
    assert_eq!(value_of(&first.nodes, "Exercise__1", "reps"), Some(OverseerValue::Integer(7)));

    // The second edit starts from the text the first one returned, as the frontend does.
    let second = edit(first.text, "exercise_tracker/Exercises/Exercise__1/sets", OverseerValue::Integer(4));
    assert_eq!(
        value_of(&second.nodes, "Exercise__1", "sets"),
        Some(OverseerValue::Integer(4)),
        "the second edit was not applied"
    );
    assert_eq!(
        value_of(&second.nodes, "Exercise__1", "reps"),
        Some(OverseerValue::Integer(7)),
        "the first edit was lost when the second one was made"
    );

    // A third, to catch anything that degrades only after repeated round trips.
    let third = edit(second.text, "exercise_tracker/Exercises/Exercise__1/reps", OverseerValue::Integer(11));
    assert_eq!(value_of(&third.nodes, "Exercise__1", "reps"), Some(OverseerValue::Integer(11)));
    assert_eq!(
        value_of(&third.nodes, "Exercise__1", "sets"),
        Some(OverseerValue::Integer(4)),
        "an earlier edit was lost after repeated round trips"
    );
    DocumentManager::set_current_document(None);
}
