//! An `enum` field holds one value out of a list of them, and says what happens when it changes.
//!
//! A tags field holding exactly one tag, in all but name: the same vocabulary list, read the same
//! way, and stored as the plain text of the value. So nothing about it is new to the backend - it
//! is written as a string, read by a formula as a string, and kept in the file as it was typed.
//! What is pinned here is that each of those holds, and that a field's `on change` runs against
//! the value just chosen: picking "finished" is meant to be able to do what a finish button does.

use overseer::server::DocumentRoot;
use overseer::types::OverseerValue;
use serde_json::json;

const DOCUMENT: &str = r##"tab project (label="Project", mutable=true) {

    div (hidden=true) {

        div Stage (layout="horizontal") {
            string tag = ""
            string name = ""
            string colour = ""
        }

        div Item (layout="horizontal") {
            string handle = ""
            enum stage (vocabulary="/project/Stages") = "filed" {
                on change {
                    if (cond=$(../stage == "finished")) {
                        append (list="/project/History") {
                            - handle = $(../handle)
                        }
                        remove (from="/project/Items", keyField="handle", keyValue=$(../handle))
                    }
                }
            }
        }

        div Finished (layout="horizontal") {
            string handle = ""
        }
    }

    list Stages (entry=<Stage>, hidden=true) {
        - {
            - tag = "filed"
            - name = "filed"
        }
        - {
            - tag = "ready"
            - name = "ready"
            - colour = "#1d4ed8"
        }
        - {
            - tag = "finished"
            - name = "finished"
        }
    }

    int ready (hidden=true) = $(Items.filter(|x| x/stage == "ready").count())

    list Items (entry=<Item>, key="handle") {
        - {
            - handle = "alpha"
        }
        - {
            - handle = "beta"
            - stage = "ready"
        }
    }

    list History (entry=<Finished>) {
    }
}
"##;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

fn a_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_enum_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("p.os"), DOCUMENT).expect("write");
    overseer::viewstate::forget_all();
    root
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("p.os")).expect("read")
}

/// The path of names the page sends for the node at an address.
fn names(service: &DocumentRoot, address: &str) -> Vec<String> {
    let nodes = service.open("p.os").expect("open");
    overseer::addressing::name_path(&nodes, address)
        .unwrap_or_else(|| panic!("no such address: {}", address))
}

/// What the page does when a value is picked: write it, then say the field changed.
fn choose(service: &DocumentRoot, handle: &str, value: &str) {
    let path = names(service, &format!("project/Items/[{}]/stage", handle));
    service
        .command_for(
            "alice",
            Some("p.os"),
            "write_overseer_values",
            &json!({ "values": [{ "node_path": path, "value": { "String": value } }] }),
        )
        .expect("the write was refused");
    let path = names(service, &format!("project/Items/[{}]/stage", handle));
    service
        .command_for(
            "alice",
            Some("p.os"),
            "run_overseer_event",
            &json!({ "node_path": path, "event_name": "change" }),
        )
        .expect("the change was refused");
}

fn handles_in(text: &str, list: &str) -> Vec<String> {
    let start = text.find(&format!("list {} ", list)).expect(list);
    let body = &text[start..];
    let end = body.find("\n    }").unwrap_or(body.len());
    body[..end]
        .lines()
        .filter_map(|l| l.trim().strip_prefix("- handle = \"").map(|h| h.trim_end_matches('"').to_string()))
        .collect()
}

/// The lines of one entry of Items, by its handle.
fn entry_of(text: &str, handle: &str) -> Vec<String> {
    let at = text.find(&format!("- handle = \"{}\"", handle)).expect(handle);
    let open = text[..at].rfind("- {").expect("the entry's opening");
    let close = at + text[at..].find("\n        }").expect("the entry's end");
    text[open + 3..close].lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()
}

#[test]
fn a_chosen_value_reaches_the_file_as_text() {
    serialised(|| {
        let root = a_root("written");
        let service = DocumentRoot::new(&root).expect("open the root");
        choose(&service, "alpha", "ready");
        let text = on_disk(&root);
        assert_eq!(
            entry_of(&text, "alpha"),
            vec!["- handle = \"alpha\"", "- stage = \"ready\""],
            "the stage was not written as an entry writes a value:\n{}",
            text
        );
        assert!(
            text.contains("enum stage (vocabulary=\"/project/Stages\") = \"filed\" {"),
            "the field's own line did not survive the write:\n{}",
            text
        );
        assert_eq!(handles_in(&text, "Items"), vec!["alpha", "beta"]);
    });
}

#[test]
fn a_value_the_entry_states_is_changed_where_it_stands() {
    serialised(|| {
        let root = a_root("stated");
        let service = DocumentRoot::new(&root).expect("open the root");
        choose(&service, "beta", "filed");
        let text = on_disk(&root);
        assert_eq!(
            entry_of(&text, "beta"),
            vec!["- handle = \"beta\"", "- stage = \"filed\""],
            "{}",
            text
        );
    });
}

#[test]
fn an_entry_that_was_given_a_value_still_does_what_the_template_says() {
    // Written as the value alone, the entry relies on the template for its `on change` - so
    // it has to still be there for a stage picked later, on a document read back from the file.
    serialised(|| {
        let root = a_root("later");
        let service = DocumentRoot::new(&root).expect("open the root");
        choose(&service, "alpha", "ready");
        let service = DocumentRoot::new(&root).expect("open the root again");
        choose(&service, "alpha", "finished");
        let text = on_disk(&root);
        assert_eq!(handles_in(&text, "Items"), vec!["beta"], "{}", text);
        assert_eq!(handles_in(&text, "History"), vec!["alpha"], "{}", text);
    });
}

#[test]
fn a_formula_compares_it_as_the_string_it_is() {
    serialised(|| {
        let root = a_root("compared");
        let service = DocumentRoot::new(&root).expect("open the root");
        let read = |service: &DocumentRoot| {
            let view = service.read_at("p.os", "project/ready").expect("read");
            view.node.parameters.get("_computed_value").cloned()
        };
        assert_eq!(read(&service), Some(OverseerValue::Integer(1)), "one task was ready");
        choose(&service, "alpha", "ready");
        assert_eq!(read(&service), Some(OverseerValue::Integer(2)), "two tasks are ready now");
    });
}

#[test]
fn choosing_finished_does_what_the_field_says() {
    serialised(|| {
        let root = a_root("finished");
        let service = DocumentRoot::new(&root).expect("open the root");
        choose(&service, "beta", "finished");
        let text = on_disk(&root);
        assert_eq!(handles_in(&text, "Items"), vec!["alpha"], "still open:\n{}", text);
        assert_eq!(handles_in(&text, "History"), vec!["beta"], "not recorded:\n{}", text);
    });
}

#[test]
fn a_value_sent_with_its_handler_is_one_step() {
    // How the page sends it when there is a file: the value travels with the `on change`, and
    // the two are written as one change - so one Undo takes back the pick and what it did.
    serialised(|| {
        let root = a_root("together");
        let service = DocumentRoot::new(&root).expect("open the root");
        let path = names(&service, "project/Items/[beta]/stage");
        service
            .command_for(
                "alice",
                Some("p.os"),
                "run_overseer_event",
                &json!({
                    "node_path": path,
                    "event_name": "change",
                    "writing": [{ "node_path": path, "value": { "String": "finished" } }],
                }),
            )
            .expect("the change was refused");
        let text = on_disk(&root);
        assert_eq!(handles_in(&text, "Items"), vec!["alpha"], "still open:\n{}", text);
        assert_eq!(handles_in(&text, "History"), vec!["beta"], "not recorded:\n{}", text);
        assert_eq!(overseer::undo::depth(&root, "p.os"), 1, "more than one step to take back");
    });
}

#[test]
fn choosing_anything_else_leaves_the_entry_where_it_is() {
    serialised(|| {
        let root = a_root("ready");
        let service = DocumentRoot::new(&root).expect("open the root");
        choose(&service, "alpha", "ready");
        let text = on_disk(&root);
        assert_eq!(handles_in(&text, "Items"), vec!["alpha", "beta"]);
        assert!(handles_in(&text, "History").is_empty(), "recorded as finished:\n{}", text);
    });
}
