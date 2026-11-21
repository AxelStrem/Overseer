//! A list can start over from its template's entries.
//!
//! A template's list entries come first in every instance, and are the template's there. An
//! instance that wants none of them says `base=none` on the list, as on any element it overrides:
//! what the template had inside it goes, and the entries its own block writes are all the list
//! holds. They are the instance's own, so they can be edited, moved, sorted and removed.

use crate::common::{serial, shared};
use overseer::app_api;
use overseer::docmgr::manager::DocumentManager;
use overseer::file_ops::OverseerFileHandler;
use overseer::server::DocumentRoot;
use overseer::types::*;

const TEMPLATE: &str = r#"    div (hidden=true) {
        div Item {
            string name = ""
            int rank = 0
        }
        div Project {
            string title = ""
            list Items (entry=<Item>, key="name", movable=true) {
                - {
                    - name = "a"
                    - rank = 1
                }
                - {
                    - name = "b"
                    - rank = 2
                }
            }
        }
    }
"#;

const BARE: &str = r#"    <Project> p {
        - title = "None"
        - Items (base=none)
    }
"#;

const OWN: &str = r#"    <Project> p {
        - title = "Mine"
        - Items (base=none) {
            - {
                - name = "x"
                - rank = 5
            }
            - {
                - name = "y"
                - rank = 9
            }
        }
    }
"#;

const MERGED: &str = r#"    <Project> p {
        - Items {
            - {
                - name = "c"
            }
        }
    }
"#;

const BUTTONS: &str = r#"
    button first (label="f") {
        on click {
            prepend (list="/t/p/Items") {
                - name = "front"
            }
        }
    }

    button last (label="l") {
        on click {
            append (list="/t/p/Items") {
                - name = "back"
            }
        }
    }

    button sorted (label="s") {
        on click {
            sort (list="/t/p/Items", by=$(x/rank), order="desc")
        }
    }

    button up (label="u") {
        on click {
            move (from="/t/p/Items", to="/t/p/Items", keyField="name", keyValue="y", at=0)
        }
    }

    button drop (label="d") {
        on click {
            remove (from="/t/p/Items", keyField="name", keyValue="x")
        }
    }

    button rename (label="r") {
        on click {
            set (path="/t/p/Items[0]/name") = "z"
        }
    }
"#;

fn document(instance: &str) -> String {
    format!("tab t (mutable=true) {{\n{}\n{}}}\n", TEMPLATE, instance)
}

fn with_buttons(instance: &str) -> String {
    format!("tab t (mutable=true) {{\n{}\n{}{}}}\n", TEMPLATE, instance, BUTTONS)
}

fn list(nodes: &[OverseerNode]) -> &OverseerNode {
    overseer::addressing::node_at_path(nodes, "t/p/Items").expect("the list")
}

fn names(nodes: &[OverseerNode]) -> Vec<String> {
    list(nodes)
        .children
        .iter()
        .map(|entry| {
            match entry
                .children
                .iter()
                .find(|c| c.name == "name")
                .and_then(|c| c.parameters.get("value"))
            {
                Some(OverseerValue::String(s)) => s.clone(),
                other => format!("{:?}", other),
            }
        })
        .collect()
}

fn read(text: &str) -> Vec<String> {
    names(&app_api::load_document(text.to_string()).unwrap())
}

/// Resolves a document, and checks that saving it gives back the same bytes - as it was read,
/// and rebuilt from nothing but the nodes.
fn resolved(text: &str) -> Vec<OverseerNode> {
    let mut nodes = app_api::load_document(text.to_string()).unwrap();
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), text);
    fn drop_fingerprints(nodes: &mut [OverseerNode]) {
        for n in nodes.iter_mut() {
            n.source_fingerprint = None;
            drop_fingerprints(&mut n.children);
        }
    }
    let kept = nodes.clone();
    drop_fingerprints(&mut nodes);
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), text, "rebuilt");
    kept
}


fn opened(tag: &str, text: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_list_starts_over_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("d.os"), text).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    root
}

/// Presses a button of a document holding `instance`, and gives back the file as it is
/// afterwards.
fn press(instance: &str, button: &str) -> String {
    let _guard = serial();
    let root = opened(button, &with_buttons(instance));
    let service = DocumentRoot::new(&root).unwrap();
    service
        .run_event("d.os", &format!("t/{}", button), "click")
        .unwrap_or_else(|e| panic!("{} was refused: {}", button, e));
    let after = std::fs::read_to_string(root.join("d.os")).unwrap();
    // What was written reads back as itself.
    let nodes = DocumentManager::with_document(Some(root.clone()), || app_api::load_document(after.clone())).unwrap();
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), after, "a second save of {} changed the file", button);
    assert!(after.contains("- Items (base=none)"), "the list no longer starts over:\n{}", after);
    // The template's entries are written once, in the template.
    assert_eq!(after.matches("- name = \"a\"").count(), 1, "{}", after);
    after
}

#[test]
fn a_list_that_starts_over_has_none_of_the_templates_entries() {
    let _shared = shared();
    let nodes = resolved(&document(BARE));
    assert!(names(&nodes).is_empty(), "entries left: {:?}", names(&nodes));
}

#[test]
fn a_list_that_starts_over_holds_only_the_entries_it_writes() {
    let _shared = shared();
    let nodes = resolved(&document(OWN));
    assert_eq!(names(&nodes), ["x", "y"]);
    for entry in &list(&nodes).children {
        assert!(!overseer::resolver::is_a_given_entry(entry), "an entry the instance wrote is the template's");
    }
}

#[test]
fn a_list_that_does_not_say_so_still_has_the_templates_entries_first() {
    let _shared = shared();
    let nodes = resolved(&document(MERGED));
    assert_eq!(names(&nodes), ["a", "b", "c"]);
    let given: Vec<bool> = list(&nodes).children.iter().map(overseer::resolver::is_a_given_entry).collect();
    assert_eq!(given, [true, true, false]);
}

#[test]
fn the_rest_of_the_instance_is_as_the_template_has_it() {
    let _shared = shared();
    let nodes = resolved(&document(BARE));
    let items = list(&nodes);
    assert!(matches!(items.parameters.get("key"), Some(OverseerValue::String(k)) if k == "name"));
    assert!(items.parameters.contains_key("entry"), "the list lost its template");
}

/// A list an instance names without a block keeps its dash when the file is rebuilt from the
/// nodes, whatever it says on that line.
#[test]
fn a_list_named_only_for_a_parameter_is_written_with_its_dash() {
    let _shared = shared();
    let nodes = resolved(&document("    <Project> p {
        - Items (label=\"Mine\")
    }
"));
    assert_eq!(names(&nodes), ["a", "b"]);
}

#[test]
fn an_entry_appended_to_an_emptied_list_is_its_only_one() {
    let after = press(BARE, "last");
    assert_eq!(read(&after), ["back"], "{}", after);
}

#[test]
fn an_entry_prepended_to_an_emptied_list_is_its_only_one() {
    let after = press(BARE, "first");
    assert_eq!(read(&after), ["front"], "{}", after);
}

#[test]
fn a_prepended_entry_goes_first() {
    let after = press(OWN, "first");
    assert_eq!(read(&after), ["front", "x", "y"], "{}", after);
}

#[test]
fn an_appended_entry_goes_last() {
    let after = press(OWN, "last");
    assert_eq!(read(&after), ["x", "y", "back"], "{}", after);
}

#[test]
fn a_sort_orders_every_entry() {
    let after = press(OWN, "sorted");
    assert_eq!(read(&after), ["y", "x"], "{}", after);
}

#[test]
fn an_entry_moves_to_the_top() {
    let after = press(OWN, "up");
    assert_eq!(read(&after), ["y", "x"], "{}", after);
}

#[test]
fn its_first_entry_is_written_to() {
    let after = press(OWN, "rename");
    assert_eq!(read(&after), ["z", "y"], "{}", after);
}

#[test]
fn removing_an_entry_leaves_the_rest() {
    let after = press(OWN, "drop");
    assert_eq!(read(&after), ["y"], "{}", after);
}

#[test]
fn removing_its_last_entry_leaves_it_empty() {
    let _guard = serial();
    let text = document("    <Project> p {\n        - Items (base=none) {\n            - {\n                - name = \"mine\"\n            }\n        }\n    }\n");
    let root = opened("only", &text);
    let path = root.join("d.os").to_string_lossy().to_string();
    let nodes = app_api::load_document(text.clone()).unwrap();
    let entry = overseer::addressing::name_path(&nodes, "t/p/Items/[mine]").expect("the entry");
    app_api::remove_entry_at(&path, "d.os", entry, "s").expect("remove");
    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.contains("(base=none)"), "the list no longer starts over:\n{}", after);
    assert!(read(&after).is_empty(), "{}", after);
}
