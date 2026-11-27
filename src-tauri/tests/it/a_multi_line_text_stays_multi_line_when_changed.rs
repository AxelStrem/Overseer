//! A string written in the document over several lines is written back over several lines when
//! it changes, escaping only what has to be: a hand-written note stays readable in the file after
//! an edit. A string the document never held as a multi-line literal is written on one line.

use crate::common::serial;
use overseer::server::DocumentRoot;
use overseer::types::*;
use std::path::PathBuf;

const DOC: &str = r#"tab notes (mutable=true) {
    text plain (markdown=true) = "one

two"
    text single = "one line"

    div (hidden=true) {
        div Note {
            string title = ""
            text body = ""
        }
    }

    list Notes (entry=<Note>, key="title") {
        - {
            - title = "a"
            - body = "first
second"
        }
    }

    button add (label="Add") {
        on click {
            append (list="../Notes") {
                - title = "n"
                - body = "x\ny"
            }
        }
    }
}
"#;

struct Sandbox(PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sandbox(tag: &str) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_multiline_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("notes.os"), DOC).unwrap();
    overseer::app_api::forget_baseline();
    overseer::app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let documents = DocumentRoot::new(&root).unwrap();
    (Sandbox(root), documents)
}

fn on_disk(sandbox: &Sandbox) -> String {
    std::fs::read_to_string(sandbox.0.join("notes.os")).unwrap().replace("\r\n", "\n")
}

fn text(s: &str) -> OverseerValue {
    OverseerValue::String(s.to_string())
}

#[test]
fn a_changed_multi_line_field_keeps_its_lines() {
    let _guard = serial();
    let (sandbox, documents) = sandbox("field");
    documents.open("notes.os").expect("open");
    documents.set_at("notes.os", "notes/plain", text("one\n\ntwo more")).expect("the set");
    assert_eq!(on_disk(&sandbox), DOC.replace("\n\ntwo\"", "\n\ntwo more\""));
}

#[test]
fn a_changed_multi_line_entry_field_keeps_its_lines() {
    let _guard = serial();
    let (sandbox, documents) = sandbox("entry");
    documents.open("notes.os").expect("open");
    documents.set_at("notes.os", "notes/Notes/[a]/body", text("first\nsecond\nthird")).expect("the set");
    assert_eq!(on_disk(&sandbox), DOC.replace("first\nsecond\"", "first\nsecond\nthird\""));
}

#[test]
fn a_multi_line_value_escapes_all_but_its_line_breaks() {
    let _guard = serial();
    let (sandbox, documents) = sandbox("escapes");
    documents.open("notes.os").expect("open");
    documents
        .set_at("notes.os", "notes/plain", text("say \"hi\"\nC:\\dir\ttab"))
        .expect("the set");
    let written = on_disk(&sandbox);
    assert!(
        written.contains("text plain (markdown=true) = \"say \\\"hi\\\"\nC:\\\\dir\\ttab\"\n"),
        "{written}"
    );
    let nodes = overseer::parser::parse_document(&written).expect("parse").1;
    let plain = nodes[0].children.iter().find(|c| c.name == "plain").expect("plain");
    assert_eq!(plain.parameters.get("value"), Some(&text("say \"hi\"\nC:\\dir\ttab")));
}

#[test]
fn a_one_line_field_given_lines_is_written_on_one_line() {
    let _guard = serial();
    let (sandbox, documents) = sandbox("single");
    documents.open("notes.os").expect("open");
    documents.set_at("notes.os", "notes/single", text("now\ntwo")).expect("the set");
    assert_eq!(on_disk(&sandbox), DOC.replace("\"one line\"", "\"now\\ntwo\""));
}

#[test]
fn an_appended_entry_writes_its_lines_escaped() {
    let _guard = serial();
    let (sandbox, documents) = sandbox("append");
    documents.open("notes.os").expect("open");
    documents.run_event("notes.os", "notes/add", "click").expect("the press");
    let written = on_disk(&sandbox);
    assert!(written.contains("- body = \"x\\ny\"\n"), "{written}");
    assert!(written.contains("- body = \"first\nsecond\"\n"), "{written}");
}
