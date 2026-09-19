//! Pressing something writes it, and the backend's half of that promise.
//!
//! Open, edit, save was right when a document was a local file and the desktop app was the only
//! way in. The server owns them now, they are reached from a browser and from a bot, and a
//! change that is not written yet is one that is going to be lost - to a forgotten Save, or to
//! the bot writing in between.
//!
//! The asking is the page's: it saves as soon as anything changes, which is checked in
//! `tests/a_change_writes_itself.spec.js`. It has to be the page's, because there are two
//! backends - the desktop app's commands and the server's dispatcher - and the first version of
//! this put the writing in the second, so the app behaved exactly as it always had. That is also
//! what `both_hosts_answer_what_the_page_asks` exists to catch.
//!
//! What is checked here is the backend's half: that a save leaves a step to take back, that a
//! field the document marks as the viewer's does not reach the file however often it is pressed,
//! and that a press still costs no write when nothing but the viewer moved.

use overseer::server::DocumentRoot;
use serde_json::json;

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

fn a_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_press_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("day.os"), DOCUMENT).expect("write");
    overseer::viewstate::forget_all();
    root
}

/// A press the way the page makes one: the document as text, and which button.
fn press(service: &DocumentRoot, session: &str, held: &str, button: &str) -> String {
    let answer = service
        .command_for(
            session,
            Some("day.os"),
            "execute_overseer_event_update",
            &json!({ "content": held, "node_path": ["day", button], "event_name": "click" }),
        )
        .expect("press");
    answer
        .get("text")
        .and_then(|t| t.as_str())
        .expect("the press answered with no text")
        .to_string()
}

/// And the save the page asks for straight after, with the viewer's fields named as it names
/// them - the page knows which are guarded and hands back what the document authored.
fn save(service: &DocumentRoot, text: &str, guarded: serde_json::Value) {
    service
        .command(
            Some("day.os"),
            "save_overseer_file_from_text",
            &json!({ "path": "day.os", "content": text, "guarded": guarded }),
        )
        .expect("save");
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("day.os")).expect("read")
}

#[test]
fn a_save_leaves_a_step_to_take_back() {
    // What makes writing on every change survivable: each one can be undone.
    serialised(|| {
        let root = a_root("undo");
        let service = DocumentRoot::new(&root).expect("open the root");

        let after = press(&service, "alice", &on_disk(&root), "record");
        save(&service, &after, json!([]));
        assert!(on_disk(&root).contains("int recorded = 1"));

        service.undo("day.os").expect("undo");
        assert!(on_disk(&root).contains("int recorded = 0"), "the step did not go back");
    });
}

#[test]
fn the_viewers_field_does_not_reach_the_file_even_when_saved() {
    // The page sends the document it holds, which has the day it moved to in it, and names the
    // guarded fields alongside so the authored value goes back.
    serialised(|| {
        let root = a_root("guarded");
        let service = DocumentRoot::new(&root).expect("open the root");
        let moved = press(&service, "alice", &on_disk(&root), "back");
        assert!(moved.contains("showing"), "the press answered with something unexpected");

        // With the authored value, which is what the page sends: it remembers what the field
        // held the first time a guarded change moved it. Sending nothing means "there was no
        // authored value", which takes the formula out and leaves the next press nothing to
        // subtract a day from.
        save(&service, &moved, json!([{ "path": "day/showing", "value": { "Formula": "today()" } }]));
        assert!(
            on_disk(&root).contains("= $(today())"),
            "the day was written instead of what the document authored:\n{}",
            on_disk(&root)
        );
    });
}

#[test]
fn a_press_alone_still_writes_nothing() {
    // The press asks the backend what the document becomes; it is the save that writes. Worth
    // pinning because the first version of this wrote here, behind the page's back, and the two
    // between them wrote twice.
    serialised(|| {
        let root = a_root("nowrite");
        let service = DocumentRoot::new(&root).expect("open the root");
        let before = on_disk(&root);
        press(&service, "alice", &before, "record");
        assert_eq!(on_disk(&root), before, "the press wrote without being asked to");
    });
}

#[test]
fn the_bots_own_door_still_keeps_the_viewers_day_out_of_the_file() {
    // Nothing above changes what `/v1` does: there the write *is* the action, and a guarded
    // field is kept against the session instead of being written.
    serialised(|| {
        let root = a_root("bot");
        let service = DocumentRoot::new(&root).expect("open the root");
        let before = on_disk(&root);

        service.run_event("day.os", "day/back", "click").expect("press");
        service.run_event("day.os", "day/back", "click").expect("press again");

        assert_eq!(on_disk(&root), before, "the day reached the file");
        assert_eq!(overseer::undo::depth(&root, "day.os"), 0, "looking left a step to undo");
    });
}

#[test]
fn a_real_write_through_the_bots_door_still_lands() {
    serialised(|| {
        let root = a_root("botwrite");
        let service = DocumentRoot::new(&root).expect("open the root");
        service.run_event("day.os", "day/record", "click").expect("press");
        assert!(on_disk(&root).contains("int recorded = 1"), "the bot's write did not land");
        assert_eq!(overseer::undo::depth(&root, "day.os"), 1, "no step to take it back");
    });
}
