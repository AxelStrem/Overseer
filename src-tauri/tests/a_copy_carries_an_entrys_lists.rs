//! A copy made with `from` carries an entry's lists too.
//!
//! Closing a project item and reopening it are each one `append (from="..")`: the item and its
//! record share their fields by name, so a field or a list added to one later is carried both
//! ways without being named in each handler. A list nested in the entry comes along entry by
//! entry, each one made from the target list's own template and filled by name, the way the
//! fields are.
//!
//! What either side works out by formula stays its own. Item and Finished both keep a hidden
//! colour - the stage's on one, the status's on the other - and a copy by name would otherwise
//! write one over the other's formula.

use overseer::app_api;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::types::*;

const DOCUMENT: &str = r##"tab project (label="P", mutable=true) {

    div (hidden=true) {
        div Record (layout="horizontal") {
            string kind = ""
            int tokens = 0
            list Notes (entry=string) { }
        }

        div Kept (layout="horizontal") {
            string kind = ""
            int tokens = 0
            string colour = $(kind == "build" ? "blue" : "grey")
            list Notes (entry=string) { }
        }

        div Item (layout="vertical") {
            string handle = ""
            string title = ""
            enum stage (vocabulary="filed, finished") = "filed" {
                on change {
                    if (cond=$(../stage == "finished")) {
                        append (list="/project/History", from="..") {
                            - finished_at = $(now())
                            - status = $(../stage)
                        }
                        remove (from="/project/Items", keyField="handle", keyValue=$(../handle))
                    }
                }
            }
            int points = 1
            string colour = $(stage == "filed" ? "#6b7280" : "#1d4ed8")
            int kids = $(../points * 2)
            list Work (entry=<Record>) { }
        }

        div Finished (layout="vertical") {
            timestamp finished_at = "2026-01-01T00:00:00Z"
            string handle = ""
            string title = ""
            string status = "finished"
            int points = 0
            int kids = 0
            string colour = $(status == "finished" ? "#15803d" : "#9f1239")
            list Work (entry=<Kept>) { }

            button reopen (icon="arrow-up") {
                on click {
                    append (list="/project/Items", from="..")
                    remove (from="/project/History", keyField="handle", keyValue=$(../handle))
                }
            }
        }
    }

    list Items (entry=<Item>, key="handle") {
        - {
            - handle = "carry"
            - title = "Carry the lists"
            - points = 3
            list Work {
                - {
                    - kind = "triage"
                    - tokens = 120
                }
                - {
                    - kind = "build"
                    - tokens = 900
                    list Notes {
                        - "first"
                        - "second"
                    }
                }
            }
        }
        - {
            - handle = "bare"
            - title = "Nothing nested"
        }
    }

    list History (entry=<Finished>, key="handle") {
    }
}
"##;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    FormulaEvaluator::set_time_override(None);
    out
}

fn a_document(tag: &str) -> String {
    let root = std::env::temp_dir().join(format!("overseer_fromlists_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("p.os");
    std::fs::write(&file, DOCUMENT).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let instant = chrono::DateTime::parse_from_rfc3339("2026-10-10T09:00:00Z").unwrap().with_timezone(&chrono::Utc);
    FormulaEvaluator::set_time_override(Some(instant));
    file.to_string_lossy().to_string()
}

fn on_disk(file: &str) -> String {
    std::fs::read_to_string(file).unwrap()
}

fn close(file: &str, handle: &str) {
    let nodes = app_api::load_document(on_disk(file)).expect("open");
    let path = overseer::addressing::name_path(&nodes, &format!("project/Items/[{}]/stage", handle))
        .unwrap_or_else(|| panic!("no stage on {}", handle));
    let value = app_api::ValueWrite { node_path: path.clone(), value: OverseerValue::String("finished".into()) };
    let done = app_api::run_event_at(file, "p.os", path, "change".into(), "s", Vec::new(), vec![value])
        .expect("the pick was refused");
    assert!(done.wrote, "closing wrote nothing");
}

fn reopen(file: &str, handle: &str) {
    let nodes = app_api::load_document(on_disk(file)).expect("open");
    let path = overseer::addressing::name_path(&nodes, &format!("project/History/[{}]/reopen", handle))
        .unwrap_or_else(|| panic!("no reopen on {}", handle));
    let done = app_api::run_event_at(file, "p.os", path, "click".into(), "s", Vec::new(), Vec::new())
        .expect("the press was refused");
    assert!(done.wrote, "reopening wrote nothing");
}

fn value_at(file: &str, address: &str) -> OverseerValue {
    let nodes = app_api::load_document(on_disk(file)).expect("open");
    let node = overseer::addressing::find(&nodes, address).unwrap_or_else(|| panic!("nothing at {}", address));
    node.parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
        .cloned()
        .unwrap_or_else(|| panic!("no value at {}", address))
}

/// What the entries of a nested list hold, field by field, in the order the list has them.
fn records(file: &str, list: &str) -> Vec<(String, i64)> {
    let nodes = app_api::load_document(on_disk(file)).expect("open");
    let list = overseer::addressing::find(&nodes, list).unwrap_or_else(|| panic!("no {}", list));
    list.children
        .iter()
        .map(|entry| {
            let read = |name: &str| {
                let field = entry.children.iter().find(|c| c.name == name).expect(name);
                field.parameters.get("_computed_value").or_else(|| field.parameters.get("value")).cloned()
            };
            let kind = match read("kind") {
                Some(OverseerValue::String(s)) => s,
                other => panic!("kind reads {:?}", other),
            };
            let tokens = match read("tokens") {
                Some(OverseerValue::Integer(i)) => i,
                other => panic!("tokens reads {:?}", other),
            };
            (kind, tokens)
        })
        .collect()
}

fn notes(file: &str, list: &str) -> Vec<String> {
    let nodes = app_api::load_document(on_disk(file)).expect("open");
    let list = overseer::addressing::find(&nodes, list).unwrap_or_else(|| panic!("no {}", list));
    list.children
        .iter()
        .map(|e| match e.parameters.get("value") {
            Some(OverseerValue::String(s)) => s.clone(),
            other => panic!("a note reads {:?}", other),
        })
        .collect()
}

fn the_work() -> Vec<(String, i64)> {
    vec![("triage".to_string(), 120), ("build".to_string(), 900)]
}

#[test]
fn a_close_keeps_a_nested_lists_entries_in_order() {
    serialised(|| {
        let file = a_document("close");
        close(&file, "carry");
        assert_eq!(records(&file, "project/History/[carry]/Work"), the_work(), "{}", on_disk(&file));
        assert_eq!(notes(&file, "project/History/[carry]/Work/Kept__2/Notes"), ["first", "second"], "{}", on_disk(&file));
        assert_eq!(value_at(&file, "project/History/[carry]/points"), OverseerValue::Integer(3));
        assert_eq!(value_at(&file, "project/History/[carry]/status"), OverseerValue::String("finished".into()));
    });
}

#[test]
fn a_reopen_brings_the_entries_back_in_order() {
    serialised(|| {
        let file = a_document("reopen");
        close(&file, "carry");
        reopen(&file, "carry");
        assert_eq!(records(&file, "project/Items/[carry]/Work"), the_work(), "{}", on_disk(&file));
        assert_eq!(notes(&file, "project/Items/[carry]/Work/Record__2/Notes"), ["first", "second"], "{}", on_disk(&file));
        assert_eq!(value_at(&file, "project/Items/[carry]/stage"), OverseerValue::String("filed".into()));
    });
}

#[test]
fn a_computed_field_keeps_its_formula_on_either_side() {
    serialised(|| {
        let file = a_document("formula");
        close(&file, "carry");
        let text = on_disk(&file);
        let history = &text[text.find("list History (").unwrap()..];
        assert!(!history.contains("colour"), "a colour was copied over a formula:\n{}", history);
        assert_eq!(value_at(&file, "project/History/[carry]/colour"), OverseerValue::String("#15803d".into()));
        // The record's own kept colour, worked out from what was copied into it.
        assert_eq!(value_at(&file, "project/History/[carry]/Work/Kept__2/colour"), OverseerValue::String("blue".into()));

        reopen(&file, "carry");
        let text = on_disk(&file);
        let items = &text[text.find("list Items (").unwrap()..text.find("list History (").unwrap()];
        assert!(!items.contains("colour"), "a colour was copied over a formula:\n{}", items);
        assert!(!items.contains("- kids"), "kids was copied over a formula:\n{}", items);
        assert_eq!(value_at(&file, "project/Items/[carry]/colour"), OverseerValue::String("#6b7280".into()));
        assert_eq!(value_at(&file, "project/Items/[carry]/kids"), OverseerValue::Integer(6));
    });
}

#[test]
fn a_value_the_source_works_out_is_copied_into_a_plain_field() {
    // Item works kids out; Finished keeps it as a plain number - a snapshot of it at closing.
    serialised(|| {
        let file = a_document("worked");
        close(&file, "carry");
        assert_eq!(value_at(&file, "project/History/[carry]/kids"), OverseerValue::Integer(6));
    });
}

#[test]
fn an_entry_with_nothing_nested_writes_no_list() {
    serialised(|| {
        let file = a_document("bare");
        close(&file, "bare");
        let text = on_disk(&file);
        let history = &text[text.find("list History (").unwrap()..];
        assert!(!history.contains("Work"), "an empty list was written:\n{}", history);
        reopen(&file, "bare");
        let text = on_disk(&file);
        let items = &text[text.find("list Items (").unwrap()..text.find("list History (").unwrap()];
        assert_eq!(items.matches("Work").count(), 1, "{}", items);
    });
}

#[test]
fn a_close_and_a_reopen_give_back_the_entry_as_it_was() {
    serialised(|| {
        let file = a_document("roundtrip");
        let before = on_disk(&file);
        close(&file, "carry");
        reopen(&file, "carry");
        let after = on_disk(&file);
        let entry = |text: &str| {
            let at = text.find("- handle = \"carry\"").unwrap();
            let open = text[..at].rfind("- {").unwrap();
            let end = text[open..].find("\n        }").unwrap();
            text[open..open + end].to_string()
        };
        assert_eq!(entry(&after), entry(&before));
    });
}
