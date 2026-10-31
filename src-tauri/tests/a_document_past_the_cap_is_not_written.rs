//! A write that would take a document past the most a document may hold is refused.
//!
//! An append that doubled an indent each time took a document to 1.2 GB through ordinary writes,
//! and nothing said so until the page could no longer open it. The figure is a stop for a fault
//! like that one: whatever makes a write grow the text, it ends in a refusal in a sentence, with
//! every file as it was and no step to undo. A document already past the figure can still be made
//! smaller, and an undo is never refused.

use overseer::app_api::{self, DOCUMENT_MOST_BYTES};
use overseer::server::{DocumentRoot, RequestError};
use overseer::types::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const SIDE: &str = r#"tab s (mutable=true) {
    div (hidden=true) {
        div Thing {
            string handle = ""
        }
    }
    list Things (entry=<Thing>, key="handle") {
        - {
            - handle = "a"
        }
    }
}
"#;

/// A document of exactly `size` bytes, nearly all of it one string nothing reads, so working it
/// out stays cheap however large it is.
fn document_of(size: u64) -> String {
    let around = |filler: &str| {
        format!(
            concat!(
                "tab t (mutable=true) {{
",
                "    string filler = \"{}\"
",
                "    string note = \"\"
",
                "    div (hidden=true) {{
",
                "        div Row {{
",
                "            string word = \"\"
",
                "        }}
",
                "    }}
",
                "    list Rows (entry=<Row>) {{
",
                "    }}
",
                "    mount SIDE (lazy=false, mutable=true, source=\"side.os/s/Things\") {{ }}
",
                "    button grow (label=\"Grow\") {{
",
                "        on click {{
",
                "            append (list=\"../SIDE/Things\") {{
",
                "                - handle = \"b\"
",
                "            }}
",
                "            set (path=\"../note\") = \"{}\"
",
                "        }}
",
                "    }}
",
                "}}
",
            ),
            filler,
            "y".repeat(500)
        )
    };
    around(&"x".repeat(size as usize - around("").len()))
}

/// Within the figure as it stands, and past it once either write below is made.
fn just_within() -> String {
    document_of(DOCUMENT_MOST_BYTES - 100)
}

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Sandbox(PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sandbox(tag: &str, text: &str) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_cap_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("big.os"), text).unwrap();
    std::fs::write(root.join("side.os"), SIDE).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let documents = DocumentRoot::new(&root).unwrap();
    (Sandbox(root), documents)
}

/// That the write was refused for its size, that the sentence names no folder, and that nothing
/// it touched changed.
fn nothing_was_written(sandbox: &Sandbox, documents: &DocumentRoot, was: &str, refused: RequestError) {
    match &refused {
        RequestError::Refused(said) => {
            assert!(said.contains("at most 95 MB"), "{}", said);
            assert!(!said.contains(&*sandbox.0.to_string_lossy()), "{}", said);
        }
        other => panic!("refused as something else: {:?}", other),
    }
    assert!(on_disk(&sandbox.0, "big.os") == was, "the document changed");
    assert_eq!(on_disk(&sandbox.0, "side.os"), SIDE);
    documents.undo("big.os").expect_err("no step to undo");
}

fn on_disk(root: &Path, name: &str) -> String {
    std::fs::read_to_string(root.join(name)).unwrap()
}

#[test]
fn a_save_past_the_figure_is_refused() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = sandbox("save", SIDE);
    let refused = documents
        .command(
            None,
            "save_overseer_file_from_text",
            &serde_json::json!({ "path": "big.os", "content": document_of(DOCUMENT_MOST_BYTES + 1024) }),
        )
        .expect_err("refused");
    nothing_was_written(&sandbox, &documents, SIDE, refused);
}

#[test]
fn an_append_that_crosses_the_figure_is_refused() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let was = just_within();
    let (sandbox, documents) = sandbox("append", &was);
    let row: HashMap<String, OverseerValue> =
        [("word".to_string(), OverseerValue::String("w".repeat(1000)))].into_iter().collect();
    let refused = documents.append_at("big.os", "t/Rows", &row).err().expect("refused");
    nothing_was_written(&sandbox, &documents, &was, refused);
}

#[test]
fn a_page_instruction_that_crosses_the_figure_is_refused_in_a_sentence() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let was = just_within();
    let (sandbox, documents) = sandbox("instruction", &was);
    let refused = documents
        .command(
            Some("big.os"),
            "append_overseer_entry",
            &serde_json::json!({
                "list_path": ["t", "Rows"],
                "fields": { "word": { "String": "w".repeat(1000) } },
            }),
        )
        .expect_err("refused");
    nothing_was_written(&sandbox, &documents, &was, refused);
}

/// A press writes its mounts first, so what it wrote there would stay if the refusal came from
/// the document's own write.
#[test]
fn a_press_too_large_for_the_document_leaves_what_it_mounts_alone() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let was = just_within();
    let (sandbox, documents) = sandbox("press", &was);
    let refused = documents.run_event("big.os", "t/grow", "click").err().expect("refused");
    nothing_was_written(&sandbox, &documents, &was, refused);
}

#[test]
fn a_page_press_too_large_for_the_document_leaves_what_it_mounts_alone() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let was = just_within();
    let (sandbox, documents) = sandbox("pagepress", &was);
    let refused = documents
        .command(
            Some("big.os"),
            "run_overseer_event",
            &serde_json::json!({ "node_path": ["t", "grow"], "event_name": "click" }),
        )
        .expect_err("refused");
    nothing_was_written(&sandbox, &documents, &was, refused);
}

#[test]
fn a_mounted_file_is_held_to_the_figure_too() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = sandbox("mounted", SIDE);
    let host = "tab t (mutable=true) {\n    mount SIDE (lazy=false, mutable=true, source=\"side.os/s/Things\") { }\n}\n";
    std::fs::write(sandbox.0.join("host.os"), host).unwrap();
    let padded = SIDE.replace(
        "    list Things",
        &format!("    string filler = \"{}\"\n    list Things", "x".repeat(DOCUMENT_MOST_BYTES as usize - SIDE.len() - 30)),
    );
    std::fs::write(sandbox.0.join("side.os"), &padded).unwrap();
    let thing: HashMap<String, OverseerValue> =
        [("handle".to_string(), OverseerValue::String("h".repeat(1000)))].into_iter().collect();
    let refused = documents.append_at("host.os", "t/SIDE/Things", &thing).err().expect("refused");
    assert!(matches!(&refused, RequestError::Refused(said) if said.starts_with("'side.os'")), "{:?}", refused);
    assert!(on_disk(&sandbox.0, "side.os") == padded, "the mounted file changed");
}

/// A document past the figure is repaired by writing it smaller, and only that.
#[test]
fn a_document_already_past_the_figure_may_shrink_but_not_grow() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let huge = document_of(DOCUMENT_MOST_BYTES + 4096);
    assert!(huge.len() as u64 > DOCUMENT_MOST_BYTES);
    let (sandbox, documents) = sandbox("past", &huge);

    let refused = documents
        .command(
            None,
            "save_overseer_file_from_text",
            &serde_json::json!({ "path": "big.os", "content": document_of(DOCUMENT_MOST_BYTES + 8192) }),
        )
        .expect_err("growing refused");
    nothing_was_written(&sandbox, &documents, &huge, refused);

    let smaller = document_of(DOCUMENT_MOST_BYTES + 2048);
    documents
        .command(
            None,
            "save_overseer_file_from_text",
            &serde_json::json!({ "path": "big.os", "content": smaller }),
        )
        .expect("still past the figure, but smaller");
    assert!(on_disk(&sandbox.0, "big.os").len() < huge.len());
}

#[test]
fn an_undo_is_not_held_to_the_figure() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let huge = document_of(DOCUMENT_MOST_BYTES + 4096);
    let (sandbox, documents) = sandbox("undo", &huge);
    documents
        .command(None, "save_overseer_file_from_text", &serde_json::json!({ "path": "big.os", "content": SIDE }))
        .expect("the repair");
    assert!(on_disk(&sandbox.0, "big.os").len() < 1024);
    documents.undo("big.os").expect("the undo");
    assert!(on_disk(&sandbox.0, "big.os") == huge, "the document as it was");
}

#[test]
fn the_desktops_writer_is_held_to_the_figure_too() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, _documents) = sandbox("desktop", SIDE);
    let file = sandbox.0.join("big.os");
    let too_large = "x".repeat(DOCUMENT_MOST_BYTES as usize + 1);
    let refused = app_api::write_file_keeping_a_step_back(file.to_str().unwrap(), &too_large)
        .expect_err("refused");
    assert!(refused.to_string().contains("at most 95 MB"), "{}", refused);
    assert_eq!(on_disk(&sandbox.0, "big.os"), SIDE);
}
