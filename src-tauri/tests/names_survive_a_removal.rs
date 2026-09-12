//! What an event answers with must be what its own text produces.
//!
//! List entries are named by position, and that name is given at parse time to an item that
//! carries no name of its own. Removing one in memory leaves the survivors with the names they
//! already had, so the list reads `Task__4, Task__6, Task__7` - a hole where the removed one
//! was. Parse the same document's text and they come out contiguous.
//!
//! The caller addresses events by name and the server answers them by parsing text, so those
//! two disagreeing is not a cosmetic difference. After one task was closed the page went on
//! offering the old names, and the next press named an entry the text resolves to a different
//! task: closing two in a row closed the wrong second one, every time.

use overseer::app_api;
use overseer::types::OverseerNode;

const DOCUMENT: &str = r#"tab tasks (label="T", mutable=true) {
    div (hidden=true) {
        div Task (layout="horizontal", margin=0) {
            timestamp added (hidden=true) = "2026-01-01T00:00:00Z"
            string title = ""
            button done (label="done") {
                on click {
                    remove (from="/tasks/Open", keyField="added", keyValue=$(../added))
                }
            }
        }
    }

    list Open (entry=<Task>, key="added") {
        - { - added = "2026-08-01T09:00:00Z"
            - title = "first" }
        - { - added = "2026-08-02T09:00:00Z"
            - title = "second" }
        - { - added = "2026-08-03T09:00:00Z"
            - title = "third" }
        - { - added = "2026-08-04T09:00:00Z"
            - title = "fourth" }
    }
}
"#;

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

/// The entry names, and the title each one carries.
fn entries(nodes: &[OverseerNode]) -> Vec<(String, String)> {
    find(nodes, "Open")
        .expect("no Open list")
        .children
        .iter()
        .map(|c| {
            let title = find(std::slice::from_ref(c), "title")
                .and_then(|t| {
                    t.parameters
                        .get("_computed_value")
                        .or(t.parameters.get("value"))
                        .cloned()
                })
                .map(|v| format!("{v:?}"))
                .unwrap_or_default();
            (c.name.clone(), title)
        })
        .collect()
}

/// Press a button, the way the app does, and answer with what the caller now holds.
fn press(text: String, entry: &str) -> (String, Vec<OverseerNode>) {
    let path: Vec<String> = ["tasks", "Open", entry, "done"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let update = app_api::execute_event_update(text, path, "click".into()).expect("event");
    let nodes = update
        .nodes
        .clone()
        .or_else(|| {
            update.changes.as_ref().and_then(|changes| {
                changes.iter().find_map(|c| match c {
                    overseer::delta::DocumentChange::Subtree { address, node, .. }
                        if address == "tasks/Open" =>
                    {
                        Some(vec![node.clone()])
                    }
                    _ => None,
                })
            })
        })
        .expect("the answer described neither the document nor the list");
    (update.text, nodes)
}

#[test]
fn the_answer_reads_the_same_as_its_own_text() {
    app_api::forget_baseline();
    let (text, _) = press(DOCUMENT.to_string(), "Task__2");
    let fresh = app_api::load_document(text).expect("reparse");
    assert_eq!(
        entries(&fresh)
            .iter()
            .map(|(n, _)| n.as_str())
            .collect::<Vec<_>>(),
        vec!["Task__1", "Task__2", "Task__3"],
        "a fresh parse should number what is left from one"
    );
}

#[test]
fn what_the_caller_holds_names_the_same_tasks_the_text_does() {
    // The invariant. Both sides may be self-consistent and still disagree, and it is the
    // disagreement that closes the wrong task.
    app_api::forget_baseline();
    let (text, held) = press(DOCUMENT.to_string(), "Task__2");
    let fresh = app_api::load_document(text).expect("reparse");
    assert_eq!(
        entries(&held),
        entries(&fresh),
        "what the caller was handed does not name the same tasks as the text it was handed"
    );
}

#[test]
fn a_second_press_closes_the_task_it_names() {
    // The bug as it was met: close one task, then press the name the answer gave for another.
    app_api::forget_baseline();
    let (text, held) = press(DOCUMENT.to_string(), "Task__2");

    // Whatever the caller now calls "third" is what it would press next.
    let third = entries(&held)
        .into_iter()
        .find(|(_, title)| title.contains("third"))
        .expect("third should still be open")
        .0;

    let (_, after) = press(text, &third);
    let left: Vec<String> = entries(&after).into_iter().map(|(_, t)| t).collect();
    assert!(
        !left.iter().any(|t| t.contains("third")),
        "pressing the entry named for \"third\" left it open; it closed {left:?}"
    );
    assert!(
        left.iter().any(|t| t.contains("fourth")),
        "it closed the wrong task: {left:?}"
    );
}
