//! What one viewer is looking at never reaches the document, whatever else they do.
//!
//! A field the document marks `mutable="guarded"` belongs to whoever is looking. The document is
//! worked out with their value applied - that is the whole point, it is the day they are on - so
//! their value is in the text every time, whatever the action was. It is taken back out again
//! before the text is written.
//!
//! It used to be taken out only at the addresses the action itself had reported moving. So a
//! press that moved the viewer wrote nothing, correctly; and then the very next write of anything
//! at all carried the viewer's value into the file with it. Two steps back through the days and
//! one meal recorded, and `showing (mutable="guarded") = $(today())` became the literal day that
//! one person happened to be looking at. The formula was gone from the document, and with it
//! everybody else's idea of today - including the bot's, which reads the same field to decide
//! which day it is writing to.
//!
//! Nothing reported it, because from the acting viewer's side everything looked right.

use overseer::server::DocumentRoot;
use serde_json::json;

const DOCUMENT: &str = r#"tab day (label="Day", mutable=true) {

    timestamp showing (mutable="guarded") = $(today())
    int recorded = 0
    string note = ""

    button back (label="back") {
        on click {
            set (path="/day/showing") = $(date_add_days(../showing, -1))
        }
    }

    button record (label="record") {
        on click {
            inc (path="/day/recorded", by=1)
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

fn a_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_stays_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("d.os"), DOCUMENT).expect("write");
    overseer::viewstate::forget_all();
    root
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("d.os")).expect("read")
}

fn press(service: &DocumentRoot, session: &str, button: &str) -> serde_json::Value {
    let nodes = service.open("d.os").expect("open");
    let path = overseer::addressing::name_path(&nodes, &format!("day/{}", button))
        .unwrap_or_else(|| panic!("no such button: {}", button));
    service
        .command_for(
            session,
            Some("d.os"),
            "run_overseer_event",
            &json!({ "node_path": path, "event_name": "click" }),
        )
        .expect("the press was refused")
}

fn set(service: &DocumentRoot, session: &str, field: &str, value: &str) {
    let nodes = service.open("d.os").expect("open");
    let path = overseer::addressing::name_path(&nodes, &format!("day/{}", field)).expect("field");
    service
        .command_for(
            session,
            Some("d.os"),
            "write_overseer_value",
            &json!({ "node_path": path, "value": { "String": value } }),
        )
        .expect("the write was refused");
}

#[test]
fn a_later_press_does_not_carry_it_in() {
    serialised(|| {
        let root = a_root("press");
        let service = DocumentRoot::new(&root).expect("open the root");
        press(&service, "alice", "back");
        press(&service, "alice", "back");
        press(&service, "alice", "record");

        let text = on_disk(&root);
        assert!(
            text.contains("= $(today())"),
            "the viewer's day replaced what the document authored:\n{}",
            text
        );
        assert!(text.contains("int recorded = 1"), "the real change did not land:\n{}", text);
    });
}

#[test]
fn neither_does_a_later_field_edit() {
    // The same question asked of the other way a page writes.
    serialised(|| {
        let root = a_root("edit");
        let service = DocumentRoot::new(&root).expect("open the root");
        press(&service, "alice", "back");
        set(&service, "alice", "note", "something");

        let text = on_disk(&root);
        assert!(
            text.contains("= $(today())"),
            "the viewer's day reached the file through an edit:\n{}",
            text
        );
        assert!(text.contains("string note = \"something\""), "the edit did not land:\n{}", text);
    });
}

#[test]
fn the_viewer_still_sees_the_day_they_moved_to() {
    // Keeping it out of the file must not take it off the screen: the answer the acting viewer
    // gets still has their day in it, which is why it is in the text at all.
    serialised(|| {
        let root = a_root("sees");
        let service = DocumentRoot::new(&root).expect("open the root");
        press(&service, "alice", "back");
        let answer = press(&service, "alice", "record");
        let shown = answer.get("text").and_then(|t| t.as_str()).expect("no text");
        assert!(
            !shown.contains("= $(today())"),
            "the viewer was put back to today by their own write"
        );
    });
}

#[test]
fn one_viewers_day_does_not_reach_another_through_the_file() {
    // What the leak cost in practice. Alice steps back and records something; Bob, who has moved
    // nothing, must still be looking at today.
    serialised(|| {
        let root = a_root("two");
        let service = DocumentRoot::new(&root).expect("open the root");
        press(&service, "alice", "back");
        press(&service, "alice", "record");

        let bob = press(&service, "bob", "record");
        let shown = bob.get("text").and_then(|t| t.as_str()).expect("no text");
        assert!(
            shown.contains("= $(today())"),
            "Bob was moved to the day Alice was looking at:\n{}",
            shown
        );
    });
}

#[test]
fn the_bot_writing_through_its_own_door_is_unaffected() {
    // `/v1` has no viewer and no session, so there is nothing to keep out - but it shares the
    // code that decides, so it is worth saying it still writes plainly.
    serialised(|| {
        let root = a_root("bot");
        let service = DocumentRoot::new(&root).expect("open the root");
        service.run_event("d.os", "day/record", "click").expect("the bot's press");
        let text = on_disk(&root);
        assert!(text.contains("int recorded = 1"));
        assert!(text.contains("= $(today())"));
    });
}
