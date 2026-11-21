//! A part parsed only to be summarised is let go of once its document is worked out, and the
//! snapshots its parse registered go with it: nothing of it was drawn, so the page can name none
//! of them. What is drawn keeps its snapshots, and is saved as it was written.

use crate::common::serial;
use overseer::server::DocumentRoot;
use overseer::source_registry::SourceRegistry;
use overseer::types::*;
use std::path::{Path, PathBuf};

const DOCUMENT: &str = r#"tab log (label="Log", mutable=true) {
    div (hidden=true) {
        div Done (layout="horizontal") {
            string handle = ""
            timestamp done_at = "2026-11-02T10:00:00Z"
            int points = 1
        }
    }
    int points (mutable=false) = $(History.sum(|x| x/points))

    list History (entry=<Done>, key="handle", window=1, parts=$(|x| week(x/done_at)),
                  sort_by=$(|x| 0 - millis_since_epoch(x/done_at))) { }
}
"#;

/// A week's part, written as a person might: a comment and spacing no writer would put there,
/// which a save that lost the part's snapshots would not keep. `tag` keeps each test's texts
/// apart, as parts of the same text share their snapshots.
fn week(tag: &str, handle: &str, day: &str, points: i64) -> String {
    format!(
        "list History {{\n    // {tag}, week of {day}\n    - {{\n        - handle   = \"{handle}\"\n        - done_at = \"{day}T08:00:00Z\"\n        - points  = {points}\n    }}\n}}\n"
    )
}

const OLD_WEEKS: [(&str, &str); 4] =
    [("2026-W41", "2026-10-06"), ("2026-W42", "2026-10-13"), ("2026-W43", "2026-10-20"), ("2026-W44", "2026-10-27")];


struct Sandbox(PathBuf);

impl Sandbox {
    fn documents(&self) -> PathBuf {
        self.0.join("documents")
    }

    fn part(&self, document: &str, week: &str) -> PathBuf {
        self.documents().join(format!("{document}.parts/History/{week}.os"))
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        overseer::document_disk::set_override(None);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// `log.os`, a history in five weekly parts of which the window draws the newest, with no disk
/// cache, so that a fresh open parses the four old parts to summarise them.
fn sandbox(tag: &str) -> Sandbox {
    let root = std::env::temp_dir().join(format!("overseer_regforget_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let sandbox = Sandbox(root);
    write(&sandbox.documents().join("log.os"), DOCUMENT);
    for (n, (part, day)) in OLD_WEEKS.iter().enumerate() {
        write(&sandbox.part("log", part), &week(tag, &format!("w{n}"), day, n as i64 + 1));
    }
    write(&sandbox.part("log", "2026-W45"), &week(tag, "now", "2026-11-03", 5));
    overseer::document_disk::set_override(Some(None));
    sandbox
}

/// A document opened as by a server that has just started: no summary and no document held.
fn open_fresh(sandbox: &Sandbox, name: &str) -> Vec<OverseerNode> {
    overseer::document_disk::settled();
    overseer::partsums::forget_in_memory();
    overseer::app_api::forget_baseline();
    overseer::app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let documents = DocumentRoot::new(sandbox.documents()).unwrap();
    documents.open(name).expect("open")
}

fn points(nodes: &[OverseerNode]) -> Option<OverseerValue> {
    let params = &overseer::addressing::find(nodes, "log/points")?.parameters;
    params.get("_computed_value").or_else(|| params.get("value")).cloned()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn the_snapshots_of_a_part_read_only_to_be_summarised_are_not_kept() {
    let _guard = serial();
    let sandbox = sandbox("summarised");
    let made = overseer::partsums::summaries_made();
    let nodes = open_fresh(&sandbox, "log.os");
    assert!(overseer::partsums::summaries_made() > made, "the old parts are summarised");
    assert_eq!(points(&nodes), Some(OverseerValue::Integer(1 + 2 + 3 + 4 + 5)));
    for (part, _) in OLD_WEEKS {
        let text = read(&sandbox.part("log", part));
        assert!(!SourceRegistry::holds_text(&text), "the snapshots of {part}, only summarised, are let go of");
    }
    let drawn = read(&sandbox.part("log", "2026-W45"));
    assert!(SourceRegistry::holds_text(&drawn), "the part the window draws keeps its snapshots");
}

#[test]
fn a_write_to_a_drawn_entry_after_the_summaries_are_let_go_of_is_saved_as_written() {
    let _guard = serial();
    let sandbox = sandbox("drawn");
    open_fresh(&sandbox, "log.os");
    let drawn = sandbox.part("log", "2026-W45");
    let before = read(&drawn);
    let documents = DocumentRoot::new(sandbox.documents()).unwrap();
    documents.set_at("log.os", "log/History/[now]/points", OverseerValue::Integer(8)).expect("the set");
    assert_eq!(read(&drawn), before.replace("- points  = 5", "- points  = 8"));
    assert_eq!(read(&sandbox.documents().join("log.os")), DOCUMENT);
    for (n, (part, day)) in OLD_WEEKS.iter().enumerate() {
        assert_eq!(read(&sandbox.part("log", part)), week("drawn", &format!("w{n}"), day, n as i64 + 1), "{part}");
    }
}

#[test]
fn a_write_to_a_part_that_was_only_summarised_is_saved_as_written() {
    let _guard = serial();
    let sandbox = sandbox("brought_in");
    open_fresh(&sandbox, "log.os");
    // With no summary held, the write's own resolve summarises the part again before it reads it
    // for the write.
    overseer::partsums::forget_in_memory();
    let old = sandbox.part("log", "2026-W41");
    let before = read(&old);
    let documents = DocumentRoot::new(sandbox.documents()).unwrap();
    documents.set_at("log.os", "log/History/[w0]/points", OverseerValue::Integer(9)).expect("the set");
    assert_eq!(read(&old), before.replace("- points  = 1", "- points  = 9"));
}

#[test]
fn a_drawn_part_keeps_its_snapshots_when_a_summarised_part_has_the_same_text() {
    let _guard = serial();
    let sandbox = sandbox("shared");
    // `other.os` draws, as its only part, a part with the same text as an old part of `log.os`,
    // which shares that part's snapshots.
    write(&sandbox.documents().join("other.os"), DOCUMENT);
    let shared = read(&sandbox.part("log", "2026-W41"));
    let drawn = sandbox.part("other", "2026-W41");
    write(&drawn, &shared);
    open_fresh(&sandbox, "other.os");
    open_fresh(&sandbox, "log.os");
    assert!(SourceRegistry::holds_text(&shared), "a drawn part of the same text keeps the snapshots");
    let documents = DocumentRoot::new(sandbox.documents()).unwrap();
    documents.set_at("other.os", "log/History/[w0]/points", OverseerValue::Integer(9)).expect("the set");
    assert_eq!(read(&drawn), shared.replace("- points  = 1", "- points  = 9"));
}
