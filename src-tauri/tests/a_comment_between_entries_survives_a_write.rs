//! What lies between a list's entries - a comment, a blank line - is the document's text, and a
//! write to one entry gives it back as it was read.

use overseer::app_api;
use overseer::server::DocumentRoot;
use overseer::types::*;
use std::path::PathBuf;

const DOC: &str = r#"tab log (label="Log", mutable=true) {
    div (hidden=true) {
        div Done (layout="horizontal") {
            string handle = ""
            int points = 1
        }
    }

    list History (entry=<Done>, key="handle") {
        // the first of the week
        - {
            - handle = "b"
            - points = 5
        }

        - { - handle = "h"
            - points = 2 }
    }

    button add (label="Add") {
        on click {
            append (list="../History") {
                - handle = "n"
            }
        }
    }
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Sandbox(PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sandbox(tag: &str) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_listtrivia_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("log.os"), DOC).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let documents = DocumentRoot::new(&root).unwrap();
    (Sandbox(root), documents)
}

fn on_disk(sandbox: &Sandbox) -> String {
    std::fs::read_to_string(sandbox.0.join("log.os")).unwrap().replace("\r\n", "\n")
}

#[test]
fn a_write_to_the_last_entry_keeps_the_comment_and_the_gap_before_it() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = sandbox("last");
    documents.open("log.os").expect("open");
    documents.set_at("log.os", "log/History/[h]/points", OverseerValue::Integer(4)).expect("the first set");
    documents.set_at("log.os", "log/History/[h]/points", OverseerValue::Integer(3)).expect("the second set");
    assert_eq!(on_disk(&sandbox), DOC.replace("- points = 2 }", "- points = 3 }"));
}

#[test]
fn a_write_to_the_commented_entry_keeps_its_comment() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = sandbox("first");
    documents.open("log.os").expect("open");
    documents.set_at("log.os", "log/History/[b]/points", OverseerValue::Integer(6)).expect("the set");
    assert_eq!(on_disk(&sandbox), DOC.replace("- points = 5", "- points = 6"));
}

#[test]
fn an_appended_entry_takes_no_comment_from_the_entries_before_it() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = sandbox("append");
    documents.open("log.os").expect("open");
    documents.run_event("log.os", "log/add", "click").expect("the press");
    let text = on_disk(&sandbox);
    assert_eq!(text.matches("// the first of the week").count(), 1, "{text}");
    assert!(
        text.contains("    list History (entry=<Done>, key=\"handle\") {\n        // the first of the week\n        - {\n            - handle = \"b\""),
        "{text}"
    );
    assert!(text.contains("- { - handle = \"h\"\n            - points = 2 }"), "{text}");
    assert!(text.contains("- handle = \"n\""), "{text}");
}
