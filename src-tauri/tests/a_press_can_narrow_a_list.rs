//! `narrow` puts a value in a textbox that narrows a list.
//!
//! What such a box holds is how somebody is looking at the list rather than anything the document
//! says, so it is the page's to keep - like a fold. The action finds the box by the same rules as
//! every other action's path, works out the value from where it is said, and the answer names
//! both. Nothing is written and nothing is worked out again.

use overseer::app_api;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::types::*;

const DOCUMENT: &str = r#"tab t (label="T", mutable=true) {

    // Read on every resolve, so whether the document was worked out again shows.
    timestamp checked (hidden=true) = $(now())

    textbox branch (placeholder="branch", vocabulary="/t/Tasks", narrows="/t/Tasks", by="handle", branch=true) = ""
    textbox plain (placeholder="find") = ""

    div (hidden=true) {
        div Task (layout="horizontal") {
            string handle = ""
            string parent (hidden=true) = ""
            string title = ""

            on click {
                narrow (path="/t/branch") = $(./handle)
            }

            button wrong (label="wrong") {
                on click {
                    narrow (path="/t/plain") = $(../handle)
                }
            }
        }
    }

    list Tasks (entry=<Task>, key="handle", tree="parent") {
        - {
            - handle = "a"
            - title = "first"
        }
        - {
            - handle = "b"
            - parent = "a"
            - title = "second"
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
    let root = std::env::temp_dir().join(format!("overseer_narrow_{}_{}", tag, std::process::id()));
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

fn press(path: &str, nodes: &[OverseerNode], what: &str) -> overseer::Result<app_api::ResolvedUpdate> {
    let node_path = overseer::addressing::name_path(nodes, what).expect("the node pressed");
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
fn the_answer_names_the_box_and_what_to_put_in_it() {
    serialised(|| {
        let (path, nodes) = opened("names");
        let answer = press(&path, &nodes, "t/Tasks/[b]").expect("the press was refused");
        FormulaEvaluator::set_time_override(None);

        assert_eq!(answer.narrowings.len(), 1, "{:?}", answer.narrowings);
        let narrowing = &answer.narrowings[0];
        assert_eq!(narrowing.address, "t/branch");
        assert_eq!(narrowing.value, "b");
        assert_eq!(node_at(&nodes, &narrowing.path).name, "branch", "the path reaches something else");
        assert!(!answer.wrote, "narrowing wrote the file");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), DOCUMENT);
    });
}

#[test]
fn a_press_that_only_narrows_works_nothing_out() {
    // Five minutes after the document was opened, a resolve would move `checked` - so an answer
    // that says nothing changed is one that was not worked out again.
    serialised(|| {
        let (path, nodes) = opened("nothing");
        at("2026-09-26T09:05:00Z");
        let answer = press(&path, &nodes, "t/Tasks/[a]").expect("the press was refused");
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
fn a_box_that_narrows_nothing_is_refused() {
    serialised(|| {
        let (path, nodes) = opened("wrong");
        let refused = press(&path, &nodes, "t/Tasks/[a]/wrong");
        FormulaEvaluator::set_time_override(None);
        assert!(refused.is_err(), "narrowing a plain textbox was accepted");
    });
}
