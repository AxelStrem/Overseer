//! An answer to an instruction names the document's text rather than carrying it.
//!
//! The text was nearly all of every answer: a press on tasks.os came back with 267 KB, 222 of them
//! the text, and a field write with 242 KB for half a kilobyte of change. Compressed, 31 KB of the
//! 36 every tap cost on a phone. The page reads that text only when something goes the long way -
//! an instruction refused and worked out from the text instead, or a save - so the answer names it
//! by version, and the page asks for it by that version when it is wanted.
//!
//! What has to hold: the text is left out rather than sent empty, since an older page would build
//! on an empty text; a version gives back exactly the text it names, even once the file has moved
//! on; a save that names its baseline by version is refused when the file has moved on, as one
//! naming it by text is; and a whole document still comes with its text.

use overseer::server::{DocumentRoot, RequestError};
use serde_json::{json, Value};

const DOCUMENT: &str = "\
tab day (label=\"Day\", mutable=true) {
    timestamp showing (mutable=\"guarded\") = $(today())
    int recorded = 0

    button record (label=\"record\") {
        on click {
            inc (path=\"/day/recorded\", by=1)
        }
    }

    button back (label=\"back\") {
        on click {
            set (path=\"/day/showing\") = $(date_add_days(../showing, -1))
        }
    }
}
";

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

/// A root holding the document, opened the way the page opens it.
fn opened(tag: &str) -> (std::path::PathBuf, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_answer_text_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("day.os"), DOCUMENT).expect("write");
    overseer::viewstate::forget_all();
    overseer::app_api::forget_baseline();
    let service = DocumentRoot::new(&root).expect("open the root");
    service
        .command_for("alice", Some("day.os"), "parse_overseer_content", &json!({ "content": DOCUMENT }))
        .expect("open");
    (root, service)
}

fn press(service: &DocumentRoot, button: &str) -> Value {
    service
        .command_for(
            "alice",
            Some("day.os"),
            "run_overseer_event",
            &json!({ "path": "day.os", "node_path": ["day", button], "event_name": "click" }),
        )
        .expect("press")
}

fn text_of(service: &DocumentRoot, version: &str) -> Value {
    service
        .command_for("alice", Some("day.os"), "load_overseer_text", &json!({ "path": "day.os", "version": version }))
        .expect("load_overseer_text")
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("day.os")).expect("read")
}

fn version(answer: &Value, key: &str) -> String {
    answer
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("the answer names no {}: {}", key, answer))
        .to_string()
}

#[test]
fn a_press_names_the_text_and_does_not_send_it() {
    serialised(|| {
        let (root, service) = opened("press");
        let answer = press(&service, "record");
        assert!(answer.get("changes").is_some_and(Value::is_array), "not a described change: {}", answer);
        // Left out, not sent empty: a page from before this takes an empty string for the document.
        assert!(answer.get("text").is_none(), "the text came back: {} bytes", answer["text"].to_string().len());
        let named = version(&answer, "version");
        assert_eq!(text_of(&service, &named), Value::String(on_disk(&root)), "the version does not give back the file");
    });
}

#[test]
fn a_field_write_names_it_too() {
    serialised(|| {
        let (root, service) = opened("write");
        let answer = service
            .command_for(
                "alice",
                Some("day.os"),
                "write_overseer_values",
                &json!({ "path": "day.os", "values": [{ "node_path": ["day", "recorded"], "value": { "Integer": 7 } }] }),
            )
            .expect("write");
        assert!(answer.get("text").is_none());
        let named = version(&answer, "version");
        let given = text_of(&service, &named);
        assert!(given.as_str().is_some_and(|t| t.contains("int recorded = 7")), "{}", given);
        assert_eq!(given, Value::String(on_disk(&root)));
    });
}

#[test]
fn a_version_still_gives_back_its_text_once_the_file_has_moved_on() {
    // What a page asks for after a refusal is the text it was last told about, and that is the
    // text the next thing it does has to be built from - not whatever the file says now.
    serialised(|| {
        let (root, service) = opened("moved");
        let named = version(&press(&service, "record"), "version");
        let told = on_disk(&root);
        std::fs::write(root.join("day.os"), told.replace("int recorded = 1", "int recorded = 40")).unwrap();
        assert_eq!(text_of(&service, &named), Value::String(told));
    });
}

#[test]
fn a_version_nothing_holds_gives_back_nothing() {
    serialised(|| {
        let (_root, service) = opened("unknown");
        assert_eq!(text_of(&service, "0000000000000000-1"), Value::Null);
    });
}

#[test]
fn a_save_naming_its_baseline_by_version_is_refused_once_the_file_has_moved_on() {
    serialised(|| {
        let (root, service) = opened("save");
        let named = version(&press(&service, "record"), "version");
        let save = |content: &str| {
            service.command(
                Some("day.os"),
                "save_overseer_file_from_text",
                &json!({ "path": "day.os", "content": content, "original_version": named }),
            )
        };
        // The file is still what the version names, so the save goes through.
        let mine = on_disk(&root).replace("int recorded = 1", "int recorded = 2");
        save(&mine).expect("a save on an unmoved file was refused");
        assert!(on_disk(&root).contains("int recorded = 2"));

        // Something else writes: the version no longer names the file, and the save is refused.
        std::fs::write(root.join("day.os"), on_disk(&root).replace("int recorded = 2", "int recorded = 9")).unwrap();
        let refused = save(&mine.replace("int recorded = 2", "int recorded = 3"));
        assert!(matches!(refused, Err(RequestError::Rejected(_))), "{:?}", refused);
        assert!(on_disk(&root).contains("int recorded = 9"), "the other write was thrown away");
    });
}

#[test]
fn a_viewers_file_is_named_rather_than_sent() {
    // With a viewer's own value in play the page is shown one text and the file holds another,
    // and both came back. The page only checks a later save against the file's, which its
    // version does as well - so that one is named, whatever shape the rest of the answer takes.
    serialised(|| {
        let (root, service) = opened("viewer");
        press(&service, "back");
        let answer = press(&service, "record");
        assert!(answer.get("file_text").is_none(), "the file's text came back: {}", answer);
        let file = version(&answer, "file_version");
        assert_eq!(text_of(&service, &file), Value::String(on_disk(&root)));
        // What they were shown is their own: the file's, with the day they moved to.
        let shown = match answer.get("text").and_then(Value::as_str) {
            Some(text) => text.to_string(),
            None => text_of(&service, &version(&answer, "version")).as_str().expect("shown").to_string(),
        };
        assert!(shown.contains("int recorded = 1"), "{}", shown);
        assert_ne!(shown, on_disk(&root), "the viewer's day was not in what they were shown");
    });
}

#[test]
fn a_whole_document_still_comes_with_its_text() {
    // Nothing held to describe a change against, so the answer is the document - and the page
    // works from that answer's text next, so it is sent.
    serialised(|| {
        let (_root, service) = opened("whole");
        overseer::app_api::forget_baseline();
        let answer = press(&service, "record");
        if answer.get("nodes").is_some_and(Value::is_array) {
            assert!(answer.get("text").and_then(Value::as_str).is_some_and(|t| !t.is_empty()), "{}", answer);
        } else {
            // Worked out again from the file and described as a change after all - which is
            // also fine, as long as it is named.
            version(&answer, "version");
        }
    });
}
