//! An entry appended to a list lines up with where that list actually is.
//!
//! A new entry takes its indentation from the entries already around it. The first entry of a
//! list has none to take it from, and what it fell back on was the list node's own indentation -
//! which is where the list sits, not where its entries sit. For a list at the top of a document
//! that is one level short and the serializer quietly corrected it afterwards. For a list nested
//! inside a template entry - a day's notes, a reading of a day's readings - it was the *parent*
//! list's depth, and nothing corrected that.
//!
//! The damage was invisible for as long as nothing rebuilt the document. The snapshot path
//! replays each node from the text it was read from, so a malformed block was replayed malformed
//! and the file looked stable. The desktop app hands its nodes across the IPC boundary, which
//! drops those snapshots, so the first save from the app reflowed every entry the bot had ever
//! written - turning a one-line edit into a diff of dozens.

use overseer::server::DocumentRoot;
use overseer::types::{OverseerNode, OverseerValue};

const DOCUMENT: &str = r#"tab diary (label="Diary", mutable=true) {

    div (hidden=true) {

        div Note (layout="horizontal") {
            timestamp at = "2026-01-01T00:00:00Z"
            string note = ""
        }

        div Day (layout="vertical") {
            timestamp day (precision="day") = "2026-01-01T00:00:00Z"
            string mine = ""
            list Notes (entry=<Note>, layout="vertical") { }
        }
    }

    list Days (entry=<Day>, key="day", keyPrecision="day") {
        - {
            - day = "2026-08-17T00:00:00Z"
        }
    }
}
"#;

fn a_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_lineup_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("d.os"), DOCUMENT).expect("write");
    root
}

fn note(service: &DocumentRoot, at: &str, text: &str) {
    let mut fields = std::collections::HashMap::new();
    fields.insert("at".to_string(), OverseerValue::String(at.to_string()));
    fields.insert("note".to_string(), OverseerValue::String(text.to_string()));
    service
        .append_at("d.os", "diary/Days/[2026-08-17T00:00:00Z]/Notes", &fields)
        .expect("append");
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("d.os")).expect("read")
}

/// How far in a line sits, for the lines that open and close an entry.
fn indents_of(text: &str, needle: &str) -> Vec<usize> {
    text.lines()
        .filter(|l| l.trim() == needle)
        .map(|l| l.len() - l.trim_start().len())
        .collect()
}

/// What the desktop app does on the way to writing: the nodes cross the IPC boundary, which
/// drops the snapshots, so every node is written from itself rather than replayed.
fn rebuilt(text: &str) -> String {
    let nodes = overseer::app_api::load_document(text.to_string()).expect("resolve");
    let across: Vec<OverseerNode> =
        serde_json::from_str(&serde_json::to_string(&nodes).expect("to json")).expect("from json");
    overseer::file_ops::OverseerFileHandler::serialize_nodes(&across).expect("serialize")
}

#[test]
fn the_first_entry_of_a_nested_list_sits_inside_that_list() {
    let root = a_root("first");
    let service = DocumentRoot::new(&root).expect("open the root");
    note(&service, "2026-08-17T15:40:00Z", "first");

    let text = on_disk(&root);
    let list_at = indents_of(&text, "list Notes {");
    assert_eq!(list_at.len(), 1, "the nested list was not written:{}{}", "
", text);
    assert_eq!(
        indents_of(&text, "- {"),
        vec![8, list_at[0] + 4],
        "the entry did not line up one level inside its list:{}{}",
        "
",
        text
    );
}

#[test]
fn a_second_entry_lines_up_with_the_first() {
    let root = a_root("second");
    let service = DocumentRoot::new(&root).expect("open the root");
    note(&service, "2026-08-17T15:40:00Z", "first");
    note(&service, "2026-08-17T18:00:00Z", "second");

    let text = on_disk(&root);
    let opens = indents_of(&text, "- {");
    assert_eq!(opens, vec![8, 16, 16], "the entries did not agree:{}{}", "
", text);
    assert_eq!(
        indents_of(&text, "}").iter().filter(|i| **i == 16).count(),
        2,
        "an entry opened at one depth and closed at another:{}{}",
        "
",
        text
    );
}

#[test]
fn what_was_appended_survives_the_desktops_own_save() {
    // The fault as it was actually met: nothing had gone wrong until the app saved, and then a
    // document nobody had edited came back different.
    let root = a_root("rebuild");
    let service = DocumentRoot::new(&root).expect("open the root");
    note(&service, "2026-08-17T15:40:00Z", "first");
    note(&service, "2026-08-17T18:00:00Z", "second");

    let text = on_disk(&root);
    let again = rebuilt(&text);
    let moved: Vec<(usize, &str, &str)> = text
        .lines()
        .zip(again.lines())
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(n, (a, b))| (n + 1, a, b))
        .collect();
    assert!(moved.is_empty(), "the document was reflowed by a save: {:?}", moved);
    assert_eq!(text.lines().count(), again.lines().count());
}

#[test]
fn an_entry_appended_to_a_list_at_the_top_still_lines_up() {
    // The shape that always worked, so a fix to the nested one that moved this is not a pass.
    let root = a_root("toplevel");
    let service = DocumentRoot::new(&root).expect("open the root");
    let mut fields = std::collections::HashMap::new();
    fields.insert("day".to_string(), OverseerValue::String("2026-08-18T00:00:00Z".to_string()));
    service.append_at("d.os", "diary/Days", &fields).expect("append a day");

    let text = on_disk(&root);
    assert_eq!(
        indents_of(&text, "- {"),
        vec![8, 8],
        "a day stopped lining up with the days around it:{}{}",
        "
",
        text
    );
    assert!(rebuilt(&text).lines().eq(text.lines()), "the document was reflowed by a save");
}
