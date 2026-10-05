//! `start_editing` opens a field for editing, as though it had been double-tapped.
//!
//! For a field with nowhere to tap: a comment on a task takes no room while it is empty, so it is
//! hidden, and a button beside the task opens it. Opening a field is the page's business, so the
//! action finds the field - by the same rules as every other action's path - and the answer names
//! it, by the child indices the page finds it with.
//!
//! A press that only opens a field changes nothing, and used to cost a whole resolve anyway,
//! which on tasks.os is most of a second between the tap and the cursor. It is answered without
//! one now - and a press or a write that does change something is still worked out.

use overseer::app_api;
use overseer::delta::DocumentChange;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::types::*;

const DOCUMENT: &str = r#"tab t (label="T", mutable=true) {

    // Read on every resolve, so whether the document was worked out again shows.
    timestamp checked (hidden=true) = $(now())

    int n = 2
    int doubled = $(n * 2)

    div (hidden=true) {
        div Task (layout="vertical") {
            string key (hidden=true) = ""
            bool seen (hidden=true) = false
            div (layout="horizontal") {
                string title = ""
                button note (icon="note") {
                    on click {
                        start_editing (path="../comment")
                    }
                }
                button look (label="look") {
                    on click {
                        set (path="../seen") = true
                        start_editing (path="../comment")
                    }
                }
                button nowhere (label="nowhere") {
                    on click {
                        start_editing (path="../no_such_field")
                    }
                }
            }
            string comment (hidden=$(comment == "")) = ""
        }
    }

    list Open (entry=<Task>, key="key") {
        - {
            - key = "a"
            - title = "first"
        }
        - {
            - key = "b"
            - title = "second"
            - comment = "already said"
        }
    }
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

fn at(when: &str) {
    let instant = chrono::DateTime::parse_from_rfc3339(when)
        .expect("bad instant")
        .with_timezone(&chrono::Utc);
    FormulaEvaluator::set_time_override(Some(instant));
}

/// A file holding the document, opened the way the page opens it, at nine o'clock.
fn opened(tag: &str) -> (String, Vec<OverseerNode>) {
    let root = std::env::temp_dir().join(format!("overseer_start_editing_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("d.os");
    std::fs::write(&file, DOCUMENT).unwrap();
    app_api::forget_baseline();
    overseer::viewstate::forget_all();
    at("2026-09-26T09:00:00Z");
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    (file.to_string_lossy().to_string(), nodes)
}

fn press(path: &str, nodes: &[OverseerNode], button: &str) -> overseer::Result<app_api::ResolvedUpdate> {
    let node_path = overseer::addressing::name_path(nodes, button).expect("the button");
    app_api::run_event_at(path, "d.os", node_path, "click".into(), "s", Vec::new(), Vec::new())
}

fn node_at<'a>(nodes: &'a [OverseerNode], path: &[usize]) -> &'a OverseerNode {
    let mut node = &nodes[path[0]];
    for i in &path[1..] {
        node = &node.children[*i];
    }
    node
}

#[test]
fn the_answer_names_the_field_to_open() {
    serialised(|| {
        let (path, nodes) = opened("names");
        at("2026-09-26T09:05:00Z");
        let answer = press(&path, &nodes, "t/Open/[a]/note").expect("the press was refused");
        FormulaEvaluator::set_time_override(None);

        let wanted = answer.start_editing.expect("the answer names no field to open");
        assert_eq!(wanted.address, "t/Open/[a]/comment");
        // By the indices the page finds it with, in the document the page is about to hold.
        let comment = node_at(&nodes, &wanted.path);
        assert_eq!(comment.name, "comment", "the path reaches {:?}", comment.name);
        assert_eq!(
            comment.parameters.get("_computed_hidden"),
            Some(&OverseerValue::Boolean(true)),
            "the field is not the hidden one it was meant to be"
        );
        assert!(!answer.wrote, "opening a field wrote the file");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), DOCUMENT);
    });
}

#[test]
fn a_press_that_only_opens_a_field_works_nothing_out() {
    // Five minutes after the document was opened, a resolve would move `checked` - so an answer
    // that says nothing changed is one that was not worked out again.
    serialised(|| {
        let (path, nodes) = opened("nothing");
        at("2026-09-26T09:05:00Z");
        let answer = press(&path, &nodes, "t/Open/[a]/note").expect("the press was refused");
        FormulaEvaluator::set_time_override(None);
        let changes = answer.changes.expect("answered with what changed");
        assert!(
            changes.is_empty(),
            "the document was worked out again: {:?}",
            changes.iter().map(|c| c.address().to_string()).collect::<Vec<_>>()
        );
    });
}

#[test]
fn a_field_that_is_not_there_is_refused() {
    serialised(|| {
        let (path, nodes) = opened("nowhere");
        let refused = press(&path, &nodes, "t/Open/[a]/nowhere");
        FormulaEvaluator::set_time_override(None);
        assert!(refused.is_err(), "opening a field that does not exist was accepted");
    });
}

#[test]
fn a_press_that_also_writes_is_worked_out() {
    serialised(|| {
        let (path, nodes) = opened("writes");
        at("2026-09-26T09:05:00Z");
        let answer = press(&path, &nodes, "t/Open/[b]/look").expect("the press was refused");
        FormulaEvaluator::set_time_override(None);
        assert_eq!(answer.start_editing.expect("no field to open").address, "t/Open/[b]/comment");
        assert!(answer.wrote, "the set was not written");
        assert!(std::fs::read_to_string(&path).unwrap().contains("- seen = true"));
    });
}

#[test]
fn a_write_without_a_graph_is_still_worked_out() {
    // A write reaches the document without any action, and it is exactly what must not be
    // mistaken for a press that did nothing. Without a graph there is no other way to settle it.
    serialised(|| {
        let (path, nodes) = opened("write");
        app_api::forget_dependencies();
        let field = overseer::addressing::name_path(&nodes, "t/n").expect("the field");
        let w: app_api::ValueWrite = serde_json::from_value(serde_json::json!({
            "node_path": field, "value": OverseerValue::Integer(5)
        }))
        .unwrap();
        let answer = app_api::write_values_at(&path, "d.os", vec![w], "s").expect("the write was refused");
        FormulaEvaluator::set_time_override(None);
        let doubled = answer.changes.expect("answered with what changed").into_iter().find_map(|c| match c {
            DocumentChange::Parameters { address, parameters, .. } if address == "t/doubled" => {
                parameters.get("_computed_value").cloned()
            }
            _ => None,
        });
        assert!(
            matches!(doubled, Some(OverseerValue::Integer(10))) || matches!(doubled, Some(OverseerValue::Float(f)) if f == 10.0),
            "what reads the written field was not worked out: {:?}",
            doubled
        );
    });
}
