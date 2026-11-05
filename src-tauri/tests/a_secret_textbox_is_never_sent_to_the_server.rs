//! A `textbox (kind="secret")` holds a password. A `call` names it, `- current = <../current>`,
//! and the page hands what is typed straight to the operation: the press's answer says which box,
//! never what it holds, and the server never puts its text on the document, even for one press.

use overseer::app_api;
use overseer::types::*;

const DOCUMENT: &str = r#"tab t (label="T", mutable="guarded") {
    textbox current (kind="secret") = ""
    textbox password (kind="secret") = ""
    textbox plain = ""
    string said = ""
    string seen = ""

    button change (label="Change") {
        on click {
            call (name="change_password", into="../said") {
                - current = <../current>
                - password = <../password>
            }
        }
    }

    button peek (label="Peek") {
        on click {
            set (path="../seen") = $(../current)
        }
    }

    button not_secret (label="Not secret") {
        on click {
            call (name="copy") {
                - text = <../plain>
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

fn opened(tag: &str) -> (String, Vec<OverseerNode>) {
    let root = std::env::temp_dir().join(format!("overseer_secret_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("d.os");
    std::fs::write(&file, DOCUMENT).unwrap();
    app_api::forget_baseline();
    overseer::viewstate::forget_all();
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    (file.to_string_lossy().to_string(), nodes)
}

/// A press with the textboxes holding what is given, as a page that sent them would.
fn press(path: &str, nodes: &[OverseerNode], what: &str, typed: &[(&str, &str)]) -> overseer::Result<app_api::ResolvedUpdate> {
    let node_path = overseer::addressing::name_path(nodes, what).expect("the node pressed");
    let typed = typed
        .iter()
        .map(|(field, text)| app_api::ValueWrite {
            node_path: overseer::addressing::name_path(nodes, field).expect("the box"),
            value: OverseerValue::String(text.to_string()),
        })
        .collect();
    app_api::run_event_at(path, "d.os", node_path, "click".into(), "s", typed, Vec::new())
}

fn node_at<'a>(nodes: &'a [OverseerNode], path: &[usize]) -> &'a OverseerNode {
    let mut node = &nodes[path[0]];
    for i in &path[1..] {
        node = &node.children[*i];
    }
    node
}

#[test]
fn a_call_names_the_secret_box_and_never_what_it_holds() {
    serialised(|| {
        let (path, nodes) = opened("names");
        let answer = press(&path, &nodes, "t/change", &[("t/current", "hunter2"), ("t/password", "s3cret")])
            .expect("the press was refused");

        assert_eq!(answer.calls.len(), 1, "{:?}", answer.calls);
        let call = &answer.calls[0];
        assert_eq!(call.name, "change_password");
        for (arg, name) in [("current", "current"), ("password", "password")] {
            let secret = &call.args[arg]["secret"];
            assert_eq!(secret["address"], serde_json::json!(format!("t/{}", name)), "{:?}", call.args);
            let at: Vec<usize> = serde_json::from_value(secret["path"].clone()).expect("the box's path");
            assert_eq!(node_at(&nodes, &at).name, name, "the path reaches something else");
        }
        let said = serde_json::to_string(&answer).unwrap();
        assert!(!said.contains("hunter2") && !said.contains("s3cret"), "the answer carries a password: {}", said);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), DOCUMENT);
    });
}

#[test]
fn a_formula_never_reads_what_is_typed_into_a_secret_box() {
    serialised(|| {
        // Even when a page sends it along with the press.
        let (path, nodes) = opened("peek");
        let answer = press(&path, &nodes, "t/peek", &[("t/current", "hunter2")]).expect("the press was refused");
        assert!(!serde_json::to_string(&answer).unwrap().contains("hunter2"));
        assert!(!std::fs::read_to_string(&path).unwrap().contains("hunter2"));
        let seen = overseer::viewstate::overlay("s", "d.os");
        assert!(!format!("{:?}", seen).contains("hunter2"), "the viewer's state holds a password: {:?}", seen);
    });
}

#[test]
fn only_a_secret_box_is_passed_by_naming_it() {
    serialised(|| {
        let (path, nodes) = opened("plain");
        let Err(said) = press(&path, &nodes, "t/not_secret", &[("t/plain", "x")]) else {
            panic!("a box that is not secret was passed by naming it")
        };
        assert!(said.to_string().contains("secret"), "{}", said);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), DOCUMENT);
    });
}
