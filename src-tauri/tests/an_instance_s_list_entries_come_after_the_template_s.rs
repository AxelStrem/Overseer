//! An instance's list entries come after the template's.
//!
//! A list's entries inherit as a div's children do: the template's stay, first, and the ones the
//! instance writes come after them. The template's are the template's: they are not written into
//! the instance's file, and nothing done in the instance removes, moves or writes into them. An
//! instance that wrote a list used to replace the template's entries with its own, so a template
//! holding entries could not give them to a document that added any of its own.

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

const OWN: &str = r#"    <Project> p {
        - title = "Mine"
        - Items {
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

const BUTTONS: &str = r#"
    button first (label="f") {
        on click {
            prepend (list="/t/p/Items") {
                - name = "front"
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

    button given_up (label="u") {
        on click {
            move (from="/t/p/Items", to="/t/p/Items", keyField="name", keyValue="b", at=0)
        }
    }

    button drop_given (label="d") {
        on click {
            remove (from="/t/p/Items", keyField="name", keyValue="a")
        }
    }

    button drop_own (label="d") {
        on click {
            remove (from="/t/p/Items", keyField="name", keyValue="x")
        }
    }

    button rename_given (label="r") {
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

fn names(nodes: &[OverseerNode]) -> Vec<String> {
    let node = overseer::addressing::node_at_path(nodes, "t/p/Items").expect("the list");
    node.children
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

/// Resolves a document, and checks that saving it gives back the same bytes.
fn resolved(text: &str) -> Vec<String> {
    let nodes = app_api::load_document(text.to_string()).unwrap();
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), text);
    names(&nodes)
}

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn opened(tag: &str, text: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_given_entries_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("d.os"), text).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    root
}

/// Presses a button and gives back what it answered, with the file as it is afterwards.
fn press(button: &str) -> (std::result::Result<(), String>, String) {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = opened(button, &with_buttons(OWN));
    let service = DocumentRoot::new(&root).unwrap();
    let outcome = service
        .run_event("d.os", &format!("t/{}", button), "click")
        .map(|_| ())
        .map_err(|e| e.to_string());
    let after = std::fs::read_to_string(root.join("d.os")).unwrap();
    // What was written reads back as itself.
    let nodes = DocumentManager::with_document(Some(root.clone()), || app_api::load_document(after.clone())).unwrap();
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), after, "a second save of {} changed the file", button);
    (outcome, after)
}

fn refused(button: &str) {
    let (outcome, after) = press(button);
    let refusal = outcome.expect_err(&format!("{} changed an entry the template gives", button));
    assert!(refusal.contains("the template gives"), "{}", refusal);
    assert_eq!(after, with_buttons(OWN));
}

#[test]
fn an_empty_list_an_instance_writes_has_the_templates_entries() {
    let text = document("    <Project> p {\n        - title = \"Empty\"\n        - Items {\n        }\n    }\n");
    assert_eq!(resolved(&text), ["a", "b"]);
}

#[test]
fn an_instances_entries_come_after_the_templates() {
    assert_eq!(resolved(&document(OWN)), ["a", "b", "x", "y"]);
}

#[test]
fn an_instance_that_does_not_write_the_list_has_the_templates_entries() {
    assert_eq!(resolved(&document("    <Project> p {\n        - title = \"Full\"\n    }\n")), ["a", "b"]);
    assert_eq!(resolved(&document("    <Project> p {\n        - Items (label=\"Mine\")\n    }\n")), ["a", "b"]);
}

#[test]
fn a_prepended_entry_goes_first_after_the_templates() {
    let (outcome, after) = press("first");
    outcome.unwrap();
    assert_eq!(read(&after), ["a", "b", "front", "x", "y"]);
    assert_eq!(after.matches("- name = \"a\"").count(), 1, "{}", after);
}

#[test]
fn an_appended_entry_is_the_only_one_written() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let text = document("    <Project> p {\n        - title = \"Full\"\n    }\n");
    let root = opened("append", &text);
    let path = root.join("d.os").to_string_lossy().to_string();
    let mut fields = std::collections::HashMap::new();
    fields.insert("name".to_string(), OverseerValue::String("mine".to_string()));
    app_api::append_entry_at(&path, "d.os", vec!["t".into(), "p".into(), "Items".into()], fields, "s").expect("append");
    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(read(&after), ["a", "b", "mine"], "{}", after);
    // Once in the template, and nowhere else.
    assert_eq!(after.matches("- name = \"a\"").count(), 1, "{}", after);
}

#[test]
fn a_sort_orders_only_the_instances_entries() {
    let (outcome, after) = press("sorted");
    outcome.unwrap();
    assert_eq!(read(&after), ["a", "b", "y", "x"]);
}

#[test]
fn an_instances_entry_is_not_moved_above_the_templates() {
    let (outcome, after) = press("up");
    outcome.unwrap();
    assert_eq!(read(&after), ["a", "b", "y", "x"]);
}

#[test]
fn an_entry_the_template_gives_is_not_moved() {
    refused("given_up");
}

#[test]
fn an_entry_the_template_gives_is_not_removed() {
    refused("drop_given");
}

#[test]
fn an_entry_the_template_gives_is_not_written_to() {
    refused("rename_given");
}

#[test]
fn a_drag_cannot_take_an_entry_the_template_gives() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = opened("drag", &document(OWN));
    let path = root.join("d.os").to_string_lossy().to_string();
    let nodes = app_api::load_document(document(OWN)).unwrap();
    let given = overseer::addressing::name_path(&nodes, "t/p/Items/[a]").expect("the entry");
    let own = overseer::addressing::name_path(&nodes, "t/p/Items/[y]").expect("the entry");
    let list = overseer::addressing::name_path(&nodes, "t/p/Items").expect("the list");
    let refusal = app_api::move_entry_at(&path, "d.os", given.clone(), list.clone(), None, None, "s")
        .err()
        .expect("an entry the template gives was dragged");
    assert!(format!("{:?}", refusal).contains("the template gives"), "{:?}", refusal);
    // Dropped above the template's: it lands after them instead.
    app_api::move_entry_at(&path, "d.os", own, list, Some(given), None, "s").expect("the move");
    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(read(&after), ["a", "b", "y", "x"], "{}", after);
}

#[test]
fn removing_the_instances_last_entry_leaves_the_templates() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let text = document("    <Project> p {\n        - Items {\n            - {\n                - name = \"mine\"\n            }\n        }\n    }\n");
    let root = opened("last", &text);
    let path = root.join("d.os").to_string_lossy().to_string();
    let nodes = app_api::load_document(text.clone()).unwrap();
    let entry = overseer::addressing::name_path(&nodes, "t/p/Items/[mine]").expect("the entry");
    app_api::remove_entry_at(&path, "d.os", entry, "s").expect("remove");
    let after = std::fs::read_to_string(&path).unwrap();
    assert!(!after.contains("\"mine\""), "the entry came back:\n{}", after);
    assert_eq!(read(&after), ["a", "b"], "{}", after);
}

#[test]
fn removing_an_instances_entry_by_key_leaves_the_rest() {
    let (outcome, after) = press("drop_own");
    outcome.unwrap();
    assert_eq!(read(&after), ["a", "b", "y"]);
}
