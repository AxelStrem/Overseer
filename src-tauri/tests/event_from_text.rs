//! Running an event from the document's text must match running it from the document.
//!
//! The frontend used to upload the whole document to trigger an event - seconds on a large
//! one, since the IPC moves a couple of MB per second. It now sends the text it already holds
//! and Rust reconstitutes the document. That only works if the two routes agree, so this
//! pins them together: the same click on the same node, once each way.

use overseer::app_api;
use overseer::docmgr::manager::DocumentManager;
use overseer::types::*;

/// Provenance ids are minted per parse and always differ; compare what the document says.
fn shape(n: &OverseerNode) -> serde_json::Value {
    let mut params: Vec<String> = n
        .parameters
        .iter()
        .map(|(k, v)| format!("{}={:?}", k, v))
        .collect();
    params.sort();
    serde_json::json!({
        "name": n.name,
        "type": format!("{:?}", n.node_type),
        "params": params,
        "children": n.children.iter().map(shape).collect::<Vec<_>>(),
    })
}

fn document() -> (String, Vec<OverseerNode>) {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/exercise_tracker/exercise.os");
    DocumentManager::set_current_document(Some(p.to_string_lossy().as_ref()));
    let text = std::fs::read_to_string(&p).unwrap();
    let nodes = app_api::load_document(text.clone()).unwrap();
    (text, nodes)
}

/// Address of a node, as `execute_event` resolves one.
///
/// Segments carry an ordinal when siblings share a name: a document with several unnamed
/// `div`s would otherwise be addressed ambiguously and the first one would always win.
fn segment(siblings: &[OverseerNode], index: usize) -> String {
    let name = &siblings[index].name;
    let ordinal = siblings[..index].iter().filter(|s| &s.name == name).count();
    if ordinal == 0 {
        name.clone()
    } else {
        format!("{}#{}", name, ordinal)
    }
}

/// Path to the first node with this name. `require_instance` restricts the search to nodes
/// inside a hydrated template instance, since the same names also appear in the template
/// definitions, where clicking them means nothing.
fn path_to(
    nodes: &[OverseerNode],
    name: &str,
    prefix: Vec<String>,
    in_instance: bool,
    require_instance: bool,
) -> Option<Vec<String>> {
    for (i, n) in nodes.iter().enumerate() {
        let mut here = prefix.clone();
        here.push(segment(nodes, i));
        let inside = in_instance || n.name.contains("__");
        if n.name == name && (inside || !require_instance) {
            return Some(here);
        }
        if let Some(found) = path_to(&n.children, name, here, inside, require_instance) {
            return Some(found);
        }
    }
    None
}

#[test]
fn event_driven_by_text_matches_event_driven_by_document() {
    let (text, mut nodes) = document();
    // Deliberately not the Done button: its action records the current time, so two runs of
    // it differ by construction and would say nothing about the two routes. Removing a
    // history entry is structural and has no such dependence.
    let path = path_to(&nodes, "rem", Vec::new(), false, false)
        .expect("no remove button in the fixture");

    // The old route: the caller hands over the document it holds.
    app_api::execute_event(&mut nodes, &path, "click").expect("event on document");

    // The new route: the caller hands over the text and Rust rebuilds the document.
    let from_text = app_api::execute_event_on_text(text, path, "click".to_string())
        .expect("event on text");

    let a: Vec<_> = nodes.iter().map(shape).collect();
    let b: Vec<_> = from_text.nodes.iter().map(shape).collect();
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap(),
        "the two routes produced different documents"
    );
    assert!(!from_text.text.is_empty(), "no text came back for the next interaction");
    DocumentManager::set_current_document(None);
}

#[test]
fn the_returned_text_can_drive_the_next_event() {
    let (text, _) = document();
    let nodes = app_api::load_document(text.clone()).unwrap();
    let path = path_to(&nodes, "done", Vec::new(), false, true)
        .expect("no done button on a hydrated exercise in the fixture");

    let first = app_api::execute_event_on_text(text, path.clone(), "click".to_string()).unwrap();
    // A second click starts from what the first one returned, as the frontend does.
    let second = app_api::execute_event_on_text(first.text, path, "click".to_string())
        .expect("the text returned by an event could not drive the next one");
    assert!(!second.text.is_empty());
    DocumentManager::set_current_document(None);
}
