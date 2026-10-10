//! A press the document says no to says why.
//!
//! A form that checks what it is given used to refuse by doing nothing, which looks the same as a
//! press that never registered. `refuse = "..."` stops the press with a sentence: nothing it did
//! is written, and whoever pressed - the page or the bot - is told the sentence, apart from any
//! failure, so it can be shown as the document's own answer.

use overseer::server::{DocumentRoot, RequestError};
use overseer::types::OverseerError;
use serde_json::json;

const DOCUMENT: &str = r#"tab t (label="T", mutable=true) {
    div (hidden=true) {
        div Row (layout="horizontal") {
            string id = ""
        }
    }

    int presses = 0
    string taken_says (hidden=true) = "this id is taken, say another"

    list Rows (entry=<Row>, key="id") {
        - {
            - id = "taken"
        }
    }

    div Form (layout="horizontal") {
        textbox id (placeholder="id") = ""
        button add (label="add") {
            on click {
                if (cond=$(../id == "")) {
                    refuse = "say an id"
                }
                if (cond=$(/t/Rows.filter(|x| x/id == ../id).count() > 0)) {
                    refuse = $(/t/taken_says)
                }
                append (list="/t/Rows", from="..")
            }
        }
    }

    button counted_then_refused (label="c") {
        on click {
            set (path="../presses") = 1
            refuse = "not now"
        }
    }

    button let_through (label="l") {
        on click {
            if (cond=$(false)) {
                refuse = "never said"
            }
            set (path="../presses") = 2
        }
    }
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn a_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_refused_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("d.os"), DOCUMENT).unwrap();
    root
}

/// The bot's way in.
fn press(button: &str) -> (Result<(), RequestError>, String) {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root(button);
    overseer::app_api::forget_baseline();
    let service = DocumentRoot::new(&root).unwrap();
    let outcome = service.run_event("d.os", &format!("t/{}", button), "click").map(|_| ());
    let after = std::fs::read_to_string(root.join("d.os")).unwrap();
    let _ = std::fs::remove_dir_all(&root);
    (outcome, after)
}

/// The page's way in, through the server, with what is typed into the form's box.
fn page_presses_add(typed: &str) -> (Result<serde_json::Value, RequestError>, String) {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root(&format!("page_{}", typed.len()));
    overseer::app_api::forget_baseline();
    let service = DocumentRoot::new(&root).unwrap();
    let outcome = service.command_for(
        "s",
        Some("d.os"),
        "run_overseer_event",
        &json!({
            "node_path": ["t", "Form", "add"],
            "event_name": "click",
            "typed": [{ "node_path": ["t", "Form", "id"], "value": { "String": typed } }],
        }),
    );
    let after = std::fs::read_to_string(root.join("d.os")).unwrap();
    let _ = std::fs::remove_dir_all(&root);
    (outcome, after)
}

#[test]
fn a_refused_press_answers_its_sentence_and_writes_nothing() {
    let (outcome, after) = press("counted_then_refused");
    assert_eq!(outcome.err(), Some(RequestError::Refused("not now".into())));
    assert_eq!(after, DOCUMENT, "what the press did before refusing was written");
}

#[test]
fn a_refusal_left_unsaid_lets_the_press_through() {
    let (outcome, after) = press("let_through");
    assert!(outcome.is_ok(), "{:?}", outcome);
    assert!(after.contains("int presses = 2"), "{}", after);
}

#[test]
fn an_empty_box_is_refused_with_its_own_sentence() {
    let (outcome, after) = page_presses_add("");
    assert_eq!(outcome.err(), Some(RequestError::Refused("say an id".into())));
    assert_eq!(after, DOCUMENT);
}

#[test]
fn a_sentence_can_be_worked_out() {
    let (outcome, after) = page_presses_add("taken");
    assert_eq!(
        outcome.err(),
        Some(RequestError::Refused("this id is taken, say another".into()))
    );
    assert_eq!(after, DOCUMENT);
}

#[test]
fn a_form_that_passes_its_checks_still_adds() {
    let (outcome, after) = page_presses_add("fresh");
    assert!(outcome.is_ok(), "{:?}", outcome.err());
    assert!(after.contains(r#"- id = "fresh""#), "{}", after);
}

#[test]
fn the_desktop_page_is_told_the_refusal_by_name() {
    // The desktop door hands the page the error as serde writes it, and the page looks for
    // `Refused` to tell the document's sentence from a failure.
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("desktop");
    overseer::app_api::forget_baseline();
    let file = root.join("d.os");
    let file = file.to_string_lossy();
    let refused = overseer::app_api::run_event_at(
        &file,
        &file,
        vec!["t".into(), "counted_then_refused".into()],
        "click".into(),
        "w",
        vec![],
        vec![],
    )
    .err()
    .expect("the press should be refused");
    assert!(matches!(refused, OverseerError::Refused(_)), "{:?}", refused);
    assert_eq!(serde_json::to_value(&refused).unwrap(), json!({ "Refused": "not now" }));
    assert_eq!(std::fs::read_to_string(root.join("d.os")).unwrap(), DOCUMENT);
    let _ = std::fs::remove_dir_all(&root);
}
