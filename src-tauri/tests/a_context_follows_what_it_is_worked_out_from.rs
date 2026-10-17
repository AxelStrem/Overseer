//! A context worked out from a field of the document follows that field, through the server.
//!
//! `context accent = $(colour)` makes a project's colour what everything inside it reads as its
//! accent. A change of the colour reaches every reader, as a change of any field reaches what reads
//! it; a write elsewhere gives the file back as it was written, declarations and all; and a mounted
//! document is worked out with the contexts of the one that mounts it.

use overseer::server::DocumentRoot;
use overseer::types::*;

const DOCUMENT: &str = "tab t (label=\"T\", mutable=true) {\r\n    string colour = \"red\"\r\n    context accent = $(colour)\r\n\r\n    div side {\r\n        context mode = \"dark\"\r\n        string near = $(accent)\r\n    }\r\n    string far = $(accent)\r\n    string other = \"\"\r\n}\r\n";

const HOST: &str = r#"context accent = "host blue"
tab t (label="T") {
    mount M (lazy=false, mutable=false, source="side.os/s") { }
}
"#;

const SIDE: &str = r#"tab s (label="S") {
    string seen = $(accent)
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Sandbox(std::path::PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sandbox(tag: &str, files: &[(&str, &str)]) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_contexts_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for (name, text) in files {
        std::fs::write(root.join(name), text).unwrap();
    }
    overseer::app_api::forget_baseline();
    overseer::app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let documents = DocumentRoot::new(&root).unwrap();
    (Sandbox(root), documents)
}

fn text_at(nodes: &[OverseerNode], address: &str) -> Option<String> {
    let node = overseer::addressing::find(nodes, address)?;
    match node
        .parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
    {
        Some(OverseerValue::String(s)) => Some(s.clone()),
        _ => None,
    }
}

#[test]
fn a_change_of_what_a_context_reads_reaches_its_readers() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = sandbox("follows", &[("p.os", DOCUMENT)]);
    let before = documents.open("p.os").expect("open");
    assert_eq!(text_at(&before, "t/far").as_deref(), Some("red"));
    assert_eq!(text_at(&before, "t/side/near").as_deref(), Some("red"));

    documents
        .set_at("p.os", "t/colour", OverseerValue::String("green".to_string()))
        .expect("the write");
    let after = documents.open("p.os").expect("reopen");
    assert_eq!(text_at(&after, "t/far").as_deref(), Some("green"));
    assert_eq!(
        text_at(&after, "t/side/near").as_deref(),
        Some("green"),
        "the side's own copy of the accent did not follow"
    );
    let written = std::fs::read_to_string(sandbox.0.join("p.os")).unwrap();
    assert_eq!(written, DOCUMENT.replace("\"red\"", "\"green\""));
}

#[test]
fn a_write_elsewhere_gives_the_file_back_as_it_was_written() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = sandbox("bytes", &[("p.os", DOCUMENT)]);
    documents.open("p.os").expect("open");
    documents
        .set_at("p.os", "t/other", OverseerValue::String("x".to_string()))
        .expect("the write");
    documents
        .set_at("p.os", "t/other", OverseerValue::String(String::new()))
        .expect("the write back");
    let written = std::fs::read_to_string(sandbox.0.join("p.os")).unwrap();
    assert_eq!(written, DOCUMENT);
}

#[test]
fn a_mounted_document_sees_the_contexts_of_the_one_that_mounts_it() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_sandbox, documents) = sandbox("mount", &[("host.os", HOST), ("side.os", SIDE)]);
    let host = documents.open("host.os").expect("open");
    assert_eq!(text_at(&host, "t/M/s/seen").as_deref(), Some("host blue"));
}
