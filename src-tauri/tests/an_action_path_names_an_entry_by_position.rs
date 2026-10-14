//! An action path can name an entry of a list by its position: `Comments[0]` is the first,
//! `Comments[-1]` the last.
//!
//! A list without a key has nothing else to name an entry by, and a press that appends to it
//! then wants the entry it has just made - to open its text for typing, so adding a comment is
//! one press instead of a form.

use overseer::app_api;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::types::*;

const DOCUMENT: &str = r#"tab t (label="T", mutable=true) {

    div (hidden=true) {
        div Comment (layout="vertical") {
            string author = ""
            text body = ""
        }
    }

    div Talk (layout="vertical") {
        list Comments (entry=<Comment>) {
            - {
                - author = "first"
                - body = "one"
            }
            - {
                - author = "second"
                - body = "two"
            }
        }
        button add (label="add comment") {
            on click {
                append (list="../Comments") {
                    - author = "owner"
                }
                start_editing (path="../Comments[-1]/body")
            }
        }
        button mark (label="mark") {
            on click {
                set (path="../Comments[0]/author") = "marked"
            }
        }
        button past (label="past") {
            on click {
                set (path="../Comments[5]/author") = "nobody"
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
    let root = std::env::temp_dir().join(format!("overseer_position_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("d.os");
    std::fs::write(&file, DOCUMENT).unwrap();
    app_api::forget_baseline();
    overseer::viewstate::forget_all();
    FormulaEvaluator::set_time_override(None);
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    (file.to_string_lossy().to_string(), nodes)
}

fn press(path: &str, nodes: &[OverseerNode], button: &str) -> overseer::Result<app_api::ResolvedUpdate> {
    let node_path = overseer::addressing::name_path(nodes, button).expect("the button");
    app_api::run_event_at(path, "d.os", node_path, "click".into(), "s", Vec::new(), Vec::new())
}

#[test]
fn a_press_opens_the_entry_it_has_just_appended() {
    serialised(|| {
        let (path, nodes) = opened("append");
        let answer = press(&path, &nodes, "t/Talk/add").expect("the press was refused");
        let wanted = answer.start_editing.expect("the answer names no field to open");
        // A body no entry had before the press: the new entry's.
        assert!(wanted.address.ends_with("/body"), "opened {}", wanted.address);
        assert!(
            !overseer::addressing::addresses(&nodes).contains(&wanted.address),
            "opened an entry that was already there: {}",
            wanted.address
        );
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("- author = \"owner\""), "the comment was not appended:\n{}", written);
    });
}

#[test]
fn a_position_from_the_start_names_the_first_entry() {
    serialised(|| {
        let (path, nodes) = opened("first");
        press(&path, &nodes, "t/Talk/mark").expect("the press was refused");
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("- author = \"marked\""), "nothing was marked:\n{}", written);
        assert!(written.contains("- author = \"second\""), "the wrong entry was marked:\n{}", written);
        assert!(!written.contains("- author = \"first\""), "the first entry was not the one marked:\n{}", written);
    });
}

#[test]
fn a_position_past_the_end_is_refused() {
    serialised(|| {
        let (path, nodes) = opened("past");
        assert!(press(&path, &nodes, "t/Talk/past").is_err(), "a position past the end was accepted");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), DOCUMENT);
    });
}
