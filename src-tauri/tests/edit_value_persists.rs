//! An edit delivered through `changed_field_values` must survive serialization.
//!
//! The frontend currently gets away without this: it applies the edit to its own document and
//! uploads that, so the value reaches the backend inside the document text and the changed
//! values are only belt-and-braces. Uploading a large document costs seconds, so the value has
//! to be able to travel on its own - which means the write must be recorded as the user's
//! override, not left looking like something inherited from a template.

use overseer::app_api;
use overseer::docmgr::manager::DocumentManager;
use overseer::file_ops::OverseerFileHandler;
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

#[test]
fn value_sent_without_the_document_survives_a_save() {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/exercise_tracker/exercise.os");
    DocumentManager::set_current_document(Some(p.to_string_lossy().as_ref()));
    let text = std::fs::read_to_string(&p).unwrap();

    // Deliberately unedited text: the new value arrives only in changed_field_values.
    let path = "exercise_tracker/Exercises/Exercise__1/reps";
    let mut m = std::collections::HashMap::new();
    m.insert(path.to_string(), OverseerValue::Integer(7));
    let edited = app_api::resolve_selective(text, vec![path.to_string()], Some(m)).unwrap();
    assert_eq!(
        value_of(&edited, "Exercise__1", "reps"),
        Some(OverseerValue::Integer(7)),
        "the edit was not applied"
    );

    let saved = OverseerFileHandler::serialize_nodes(&edited).unwrap();
    let reopened = app_api::load_document(saved).unwrap();
    assert_eq!(
        value_of(&reopened, "Exercise__1", "reps"),
        Some(OverseerValue::Integer(7)),
        "the edit was lost when the document was written out"
    );
    DocumentManager::set_current_document(None);
}
