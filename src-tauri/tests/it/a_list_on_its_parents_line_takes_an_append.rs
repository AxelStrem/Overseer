//! A list written on the line its parent's block opens - a whole document on one line, as agents
//! write short examples - is given its new entries like any other. The writer used to copy that
//! line as it was read, since the list's own text had not changed, and so lost every entry
//! appended to it, and wrote the parent's closing brace a second time.

use crate::common::serialised;
use overseer::server::DocumentRoot;
use overseer::types::*;
use std::path::PathBuf;

const ONE_LINE: &str =
    "div bench (mutable=true) { div (hidden=true) { div Rec { string a = \"\" } } list Rows (entry=<Rec>) { } }\n";

const ON_THE_OPENERS_LINE: &str = "div (hidden=true) {
    div Rec { string a = \"\" }
}
div bench (mutable=true) { list Rows (entry=<Rec>) { } }
";

const WITH_AN_ENTRY: &str =
    "div bench (mutable=true) { div (hidden=true) { div Rec { string a = \"\" } } list Rows (entry=<Rec>) { - { - a = \"y\" } } }\n";

struct Sandbox(PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sandbox(tag: &str, files: &[(&str, &str)]) -> Sandbox {
    let root = std::env::temp_dir().join(format!("overseer_parents_line_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for (name, text) in files {
        std::fs::write(root.join(name), text).unwrap();
    }
    overseer::app_api::forget_baseline();
    overseer::app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    Sandbox(root)
}

fn append(documents: &DocumentRoot, document: &str, a: &str) {
    append_at(documents, document, "bench/Rows", a);
}

fn append_at(documents: &DocumentRoot, document: &str, address: &str, a: &str) {
    let fields = [("a".to_string(), OverseerValue::String(a.to_string()))].into_iter().collect();
    documents.append_at(document, address, &fields).expect("the append");
}

/// What `bench/Rows` holds when the file is read afresh, each entry's `a` in order.
fn rows_read_afresh(text: &str) -> Vec<String> {
    let nodes = overseer::app_api::load_document(text.to_string()).expect("the file reads back");
    let rows = overseer::addressing::find(&nodes, "bench/Rows").expect("the list is there");
    rows.children
        .iter()
        .map(|entry| {
            let a = entry.children.iter().find(|c| c.name == "a").expect("a field a");
            match a.parameters.get("value") {
                Some(OverseerValue::String(s)) => s.clone(),
                other => panic!("a is {:?}", other),
            }
        })
        .collect()
}

fn braces_balance(text: &str) {
    assert_eq!(text.matches('{').count(), text.matches('}').count(), "unbalanced braces in:\n{}", text);
}

fn appended_twice(tag: &str, document: &str, before: &[&str]) {
    serialised(|| {
        let sandbox = sandbox(tag, &[("r.os", document)]);
        let documents = DocumentRoot::new(&sandbox.0).unwrap();
        documents.open("r.os").expect("open");
        append(&documents, "r.os", "x");
        let text = std::fs::read_to_string(sandbox.0.join("r.os")).unwrap();
        braces_balance(&text);
        let mut expected: Vec<String> = before.iter().map(|s| s.to_string()).collect();
        expected.push("x".to_string());
        assert_eq!(rows_read_afresh(&text), expected, "after one append the file is:\n{}", text);

        append(&documents, "r.os", "z");
        let text = std::fs::read_to_string(sandbox.0.join("r.os")).unwrap();
        braces_balance(&text);
        expected.push("z".to_string());
        assert_eq!(rows_read_afresh(&text), expected, "after two appends the file is:\n{}", text);
    });
}

#[test]
fn a_document_written_on_one_line_takes_an_append() {
    appended_twice("one_line", ONE_LINE, &[]);
}

#[test]
fn a_list_on_the_line_its_parent_opens_takes_an_append() {
    appended_twice("openers_line", ON_THE_OPENERS_LINE, &[]);
}

#[test]
fn a_one_line_list_with_an_entry_takes_another() {
    appended_twice("with_entry", WITH_AN_ENTRY, &["y"]);
}

#[test]
fn a_one_line_document_takes_an_append_through_a_mount() {
    serialised(|| {
        let sandbox = sandbox(
            "mount",
            &[
                ("r.os", ONE_LINE),
                ("host.os", "tab t (mutable=true) {\n    mount M (lazy=false, mutable=true, source=\"r.os/bench\") { }\n}\n"),
            ],
        );
        let documents = DocumentRoot::new(&sandbox.0).unwrap();
        documents.open("host.os").expect("open");
        append_at(&documents, "host.os", "t/M/bench/Rows", "x");
        let text = std::fs::read_to_string(sandbox.0.join("r.os")).unwrap();
        braces_balance(&text);
        assert_eq!(rows_read_afresh(&text), vec!["x".to_string()], "the mounted file is:\n{}", text);
    });
}

#[test]
fn a_one_line_block_nothing_changed_in_is_given_back_as_it_was() {
    serialised(|| {
        let document = "div keep (mutable=true) { string a = \"\" list Rows (entry=<Rec>) { - { - a = \"y\" } } }\n\
div (hidden=true) {\n    div Rec { string a = \"\" }\n}\n\
div bench (mutable=true) {\n    list Rows (entry=<Rec>) { }\n}\n";
        let sandbox = sandbox("untouched", &[("r.os", document)]);
        let documents = DocumentRoot::new(&sandbox.0).unwrap();
        documents.open("r.os").expect("open");
        append(&documents, "r.os", "x");
        let text = std::fs::read_to_string(sandbox.0.join("r.os")).unwrap();
        let first = text.lines().next().unwrap();
        assert_eq!(first, document.lines().next().unwrap(), "the file is:\n{}", text);
        assert_eq!(rows_read_afresh(&text), vec!["x".to_string()]);
    });
}
