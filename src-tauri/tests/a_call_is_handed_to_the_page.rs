//! `call` asks whoever shows the document for an operation - open a document, sign out, copy.
//!
//! The operation is the host's to run, so the press only names it: the answer carries its name,
//! its arguments worked out where the press is, and the guarded field its answer goes into. The
//! file is not written and nothing joins the undo.

use overseer::app_api;
use overseer::types::*;

const DOCUMENT: &str = r#"tab t (label="T", mutable=true) {
    string target = "tasks.os"
    int count = 2
    string said (mutable="guarded") = ""
    string kept = ""

    button go (label="Go") {
        on click {
            set (path="../count") = $(../count + 1)
            call (name="open", into="../said") {
                - path = $(../target)
                - times = $(../count)
                - quietly = true
            }
            call (name="copy") {
                - text = "hello"
            }
        }
    }

    button only (label="Only") {
        on click {
            call (name="sign_out")
        }
    }

    button unknown (label="Unknown") {
        on click {
            call (name="launch_rockets")
        }
    }

    button kept_into (label="Into a kept field") {
        on click {
            call (name="copy", into="../kept") {
                - text = "x"
            }
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

/// A file holding the document, opened the way the page opens it.
fn opened(tag: &str) -> (String, Vec<OverseerNode>) {
    let root = std::env::temp_dir().join(format!("overseer_call_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("d.os");
    std::fs::write(&file, DOCUMENT).unwrap();
    app_api::forget_baseline();
    overseer::viewstate::forget_all();
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
fn the_answer_names_each_operation_with_its_arguments_in_order() {
    serialised(|| {
        let (path, nodes) = opened("names");
        let answer = press(&path, &nodes, "t/go").expect("the press was refused");

        assert_eq!(answer.calls.len(), 2, "{:?}", answer.calls);
        let open = &answer.calls[0];
        assert_eq!(open.name, "open");
        assert_eq!(open.args["path"], serde_json::json!("tasks.os"));
        // Worked out after the `set` before it.
        assert_eq!(open.args["times"], serde_json::json!(3));
        assert_eq!(open.args["quietly"], serde_json::json!(true));
        let into = open.into.as_ref().expect("the field for its answer");
        assert_eq!(into.address, "t/said");
        assert_eq!(node_at(&nodes, &into.path).name, "said", "the path reaches something else");

        let copy = &answer.calls[1];
        assert_eq!(copy.name, "copy");
        assert_eq!(copy.args["text"], serde_json::json!("hello"));
        assert!(copy.into.is_none());
    });
}

#[test]
fn a_press_that_only_calls_writes_nothing() {
    serialised(|| {
        let (path, nodes) = opened("nothing");
        let answer = press(&path, &nodes, "t/only").expect("the press was refused");
        assert_eq!(answer.calls.len(), 1);
        assert_eq!(answer.calls[0].name, "sign_out");
        assert!(answer.calls[0].args.is_empty());
        assert!(!answer.wrote, "calling wrote the file");
        assert_eq!(answer.changes.as_deref().map(|c| c.len()), Some(0), "something was worked out again");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), DOCUMENT);
    });
}

#[test]
fn an_operation_nobody_has_is_refused() {
    serialised(|| {
        let (path, nodes) = opened("unknown");
        let Err(said) = press(&path, &nodes, "t/unknown") else { panic!("an unknown operation was let through") };
        assert!(said.to_string().contains("launch_rockets"), "{}", said);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), DOCUMENT);
    });
}

#[test]
fn an_answer_goes_only_into_a_guarded_field() {
    serialised(|| {
        let (path, nodes) = opened("kept");
        let Err(said) = press(&path, &nodes, "t/kept_into") else { panic!("an answer was let into the document") };
        assert!(said.to_string().contains("guarded"), "{}", said);
    });
}

#[test]
fn a_call_is_never_a_step_to_take_back() {
    serialised(|| {
        // One undo after a press that wrote and one that only called takes back the write.
        let (path, nodes) = opened("undo");
        let wrote = press(&path, &nodes, "t/go").expect("the press was refused");
        assert!(wrote.wrote);
        let nodes = app_api::load_document(std::fs::read_to_string(&path).unwrap()).expect("open");
        press(&path, &nodes, "t/only").expect("the press was refused");
        app_api::undo_document(&path).expect("nothing to take back");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), DOCUMENT);
    });
}
