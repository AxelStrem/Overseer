//! A view onto a key the list lacks, made real by being edited - through the door the page uses.
//!
//! The page used to do this itself: find the template in its own copy of the document, clone it,
//! set the key, insert it, and leave the whole document to be saved as text over the file. That
//! was the last page write able to discard a write made in between, and nothing about it reached
//! the file except through that save.
//!
//! One instruction now says the whole sentence - make sure of the entry at this key, then apply
//! the edit that asked for it - against whatever the file holds at that moment. Both halves
//! together because a person edited one field once: one file write, one step to take back.

use overseer::server::DocumentRoot;
use serde_json::json;

const DOCUMENT: &str = r#"tab tracker (label="Tracker", mutable=true) {

    div (hidden=true) {
        div DayRecord (layout="vertical") {
            timestamp date (precision="day") = $(today())
            float weight = null

            div targets {
                float target (freeze=true, fallback=$(/tracker/standing)) = null
            }
        }
    }

    float standing = 1800

    timestamp showing (precision="day") = $(today())

    div SelectedDay (link="/tracker/History[key=$(../showing)]", phantom-materialize="append-on-edit") { }

    list History (entry=<DayRecord>, key="date", keyPrecision="day") {
        - {
            - date = "2026-08-03"
            - weight = 92.3
        }
        - {
            - date = "2026-08-04"
            - weight = 93.7
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
    let root = std::env::temp_dir().join(format!("overseer_preview_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("day.os"), DOCUMENT).expect("write");
    overseer::viewstate::forget_all();
    root
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("day.os")).expect("read")
}

/// The page's side of a field edited in a view whose key the list does not have.
fn edit_through_the_view(
    service: &DocumentRoot,
    session: &str,
    key: &str,
    field: &str,
    value: serde_json::Value,
) -> std::result::Result<serde_json::Value, overseer::server::RequestError> {
    service.command_for(
        session,
        Some("day.os"),
        "ensure_overseer_entry",
        &json!({ "wanted": {
            "list_path": ["tracker", "History"],
            "key_field": "date",
            "key_value": { "String": key },
            "template": "DayRecord",
            "position": "append-on-edit",
            "fields": { field: value },
        }}),
    )
}

fn expect_edit(service: &DocumentRoot, session: &str, key: &str, field: &str, value: f64) {
    edit_through_the_view(service, session, key, field, json!({ "Float": value }))
        .expect("the edit was refused");
}

#[test]
fn the_entry_and_the_edit_both_reach_the_file() {
    serialised(|| {
        let root = a_root("lands");
        let service = DocumentRoot::new(&root).expect("open the root");
        expect_edit(&service, "alice", "2026-08-05", "weight", 91.0);

        let text = on_disk(&root);
        assert!(text.contains("2026-08-05"), "the day was not written:\n{}", text);
        let day = text.rsplit("2026-08-05").next().unwrap();
        assert!(day.contains("91"), "the edit did not reach the day:\n{}", text);
    });
}

#[test]
fn it_is_one_step_to_take_back() {
    // A person edited one field once. Making the entry and writing the value as two changes
    // would be two file writes and two presses of Undo to get back to where they started.
    serialised(|| {
        let root = a_root("undo");
        let service = DocumentRoot::new(&root).expect("open the root");
        let before = on_disk(&root);
        expect_edit(&service, "alice", "2026-08-05", "weight", 91.0);

        assert_eq!(overseer::undo::depth(&root, "day.os"), 1);
        service.undo("day.os").expect("undo");
        assert_eq!(
            overseer::app_api::canonicalize_document(&on_disk(&root)),
            overseer::app_api::canonicalize_document(&before),
            "one press did not put it back"
        );
    });
}

#[test]
fn the_new_day_freezes_what_it_was_aiming_at() {
    // The reason this is a correction rather than a risk. `freeze=true` writes the standing
    // figure into a day when the day is created, and it works off a mark only the backend sets.
    // A day the page made in its own copy and saved as text was an ordinary entry by the time
    // the backend saw it, so it never froze - while a day the bot made did.
    serialised(|| {
        let root = a_root("freeze");
        let service = DocumentRoot::new(&root).expect("open the root");
        expect_edit(&service, "alice", "2026-08-05", "weight", 91.0);

        let text = on_disk(&root);
        let day = text.rsplit("2026-08-05").next().unwrap();
        assert!(
            day.contains("target = 1800"),
            "the new day did not freeze its target:\n{}",
            text
        );
    });
}

#[test]
fn editing_a_day_that_is_already_there_writes_to_it_and_makes_nothing() {
    // The same sentence either way, which is what lets a view stop caring whether what it shows
    // was a preview a moment ago.
    serialised(|| {
        let root = a_root("existing");
        let service = DocumentRoot::new(&root).expect("open the root");
        expect_edit(&service, "alice", "2026-08-04", "weight", 90.0);

        let text = on_disk(&root);
        assert_eq!(text.matches("- date = \"2026-08-04\"").count(), 1, "{}", text);
        let day = text.rsplit("2026-08-04").next().unwrap();
        assert!(day.contains("90"), "the edit did not land:\n{}", text);
    });
}

#[test]
fn a_second_edit_through_the_same_view_makes_no_second_entry() {
    serialised(|| {
        let root = a_root("twice");
        let service = DocumentRoot::new(&root).expect("open the root");
        expect_edit(&service, "alice", "2026-08-05", "weight", 91.0);
        expect_edit(&service, "alice", "2026-08-05", "weight", 91.5);

        let text = on_disk(&root);
        assert_eq!(text.matches("- date = \"2026-08-05\"").count(), 1, "{}", text);
    });
}

#[test]
fn a_write_made_in_the_meantime_survives_it() {
    // The whole reason for moving this off the page. The instruction is applied to what the file
    // says when it is applied, so a meal the bot logged while the view sat open is still there.
    serialised(|| {
        let root = a_root("meanwhile");
        let service = DocumentRoot::new(&root).expect("open the root");

        // Something else writes to a day that is already in the file.
        expect_edit(&service, "bot", "2026-08-03", "weight", 88.8);
        // And the person, who has been looking at a day that does not exist, types into it.
        expect_edit(&service, "alice", "2026-08-05", "weight", 91.0);

        let text = on_disk(&root);
        assert!(text.contains("88.8"), "the other write was lost:\n{}", text);
        assert!(text.contains("2026-08-05"), "the new day was lost:\n{}", text);
    });
}

#[test]
fn a_key_in_something_that_is_not_a_list_is_refused() {
    serialised(|| {
        let root = a_root("notalist");
        let service = DocumentRoot::new(&root).expect("open the root");
        let before = on_disk(&root);
        let refused = service.command_for(
            "alice",
            Some("day.os"),
            "ensure_overseer_entry",
            &json!({ "wanted": {
                "list_path": ["tracker", "standing"],
                "key_field": "date",
                "key_value": { "String": "2026-08-05" },
                "template": "DayRecord",
                "fields": {},
            }}),
        );
        assert!(refused.is_err(), "a number was treated as a list");
        assert_eq!(on_disk(&root), before, "the file was written anyway");
    });
}

#[test]
fn a_template_the_document_does_not_have_is_refused() {
    serialised(|| {
        let root = a_root("notemplate");
        let service = DocumentRoot::new(&root).expect("open the root");
        let before = on_disk(&root);
        let refused = edit_through_the_view(
            &service,
            "alice",
            "2026-08-05",
            "weight",
            json!({ "Float": 91.0 }),
        );
        assert!(refused.is_ok(), "the ordinary case stopped working");

        let root = a_root("notemplate2");
        let service = DocumentRoot::new(&root).expect("open the root");
        let refused = service.command_for(
            "alice",
            Some("day.os"),
            "ensure_overseer_entry",
            &json!({ "wanted": {
                "list_path": ["tracker", "History"],
                "key_field": "date",
                "key_value": { "String": "2026-08-05" },
                "template": "NoSuchThing",
                "fields": {},
            }}),
        );
        assert!(refused.is_err(), "an entry was made from a template that is not there");
        assert_eq!(on_disk(&root), before, "the file was written anyway");
    });
}
