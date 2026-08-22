//! A keyed list must not take the same key twice.
//!
//! Two entries sharing a key share an address, and one of them is then unreachable. Worse, a
//! reader keying them into a map keeps only the last: two diary notes written in one turn -
//! both stamped with the time of the message rather than the times of the things they
//! described - became one note by the time the morning recap read them back. Nothing failed
//! and nothing said so; the day's account was simply missing half of what had been written.

use overseer::server::{DocumentRoot, RequestError};
use overseer::types::OverseerValue;
use std::collections::HashMap;

const DOCUMENT: &str = r#"tab diary (label="D", mutable=true) {
    div (hidden=true) {
        div Note (layout="horizontal", margin=0) {
            timestamp at = "2026-01-01T00:00:00Z"
            string note = ""
        }
        div Loose (layout="horizontal", margin=0) {
            string what = ""
        }
    }

    list Notes (entry=<Note>, key="at") { }
    list Anything (entry=<Loose>) { }
}
"#;

struct Sandbox {
    root: std::path::PathBuf,
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn sandbox(tag: &str) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_dupkey_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir");
    std::fs::write(root.join("diary.os"), DOCUMENT).expect("write");
    (Sandbox { root: root.clone() }, DocumentRoot::new(&root).expect("root"))
}

fn note(at: &str, text: &str) -> HashMap<String, OverseerValue> {
    HashMap::from([
        ("at".to_string(), OverseerValue::Timestamp(at.to_string())),
        ("note".to_string(), OverseerValue::String(text.to_string())),
    ])
}

#[test]
fn the_same_key_twice_is_refused() {
    const TAG: &str = "refused";
    let (_dir, documents) = sandbox(TAG);
    let when = "2026-08-18T19:13:00+04:00";
    documents
        .append_at("diary.os", "diary/Notes", &note(when, "went for a hike"))
        .expect("the first note should go in");

    match documents.append_at("diary.os", "diary/Notes", &note(when, "he went to a meeting")) {
        Err(RequestError::Rejected(said)) => {
            assert!(said.contains("at"), "it did not name the key: {said}");
            assert!(said.contains(when), "it did not say which value was taken: {said}");
        }
        Err(other) => panic!("refused for the wrong reason: {other:?}"),
        Ok(_) => panic!("a second entry took an address that was already in use"),
    }
}

#[test]
fn nothing_is_written_when_the_key_is_taken() {
    const TAG: &str = "nothing";
    let (_dir, documents) = sandbox(TAG);
    let when = "2026-08-18T19:13:00+04:00";
    documents.append_at("diary.os", "diary/Notes", &note(when, "first")).expect("first");
    let _ = documents.append_at("diary.os", "diary/Notes", &note(when, "second"));

    let view = documents.read_at("diary.os", "diary/Notes").expect("read");
    assert_eq!(view.child_addresses.len(), 1, "{:?}", view.child_addresses);
}

#[test]
fn a_minute_apart_is_enough() {
    const TAG: &str = "apart";
    let (_dir, documents) = sandbox(TAG);
    documents
        .append_at("diary.os", "diary/Notes", &note("2026-08-18T19:13:00+04:00", "hike"))
        .expect("first");
    documents
        .append_at("diary.os", "diary/Notes", &note("2026-08-18T19:14:00+04:00", "meeting"))
        .expect("a different key was refused");

    let view = documents.read_at("diary.os", "diary/Notes").expect("read");
    assert_eq!(view.child_addresses.len(), 2, "{:?}", view.child_addresses);
}

#[test]
fn a_list_with_no_key_still_takes_duplicates() {
    // Two of the same thing on a shopping list are two things to buy. Only a keyed list has
    // an address to collide over.
    const TAG: &str = "unkeyed";
    let (_dir, documents) = sandbox(TAG);
    let same = HashMap::from([("what".to_string(), OverseerValue::String("milk".into()))]);
    documents.append_at("diary.os", "diary/Anything", &same).expect("first");
    documents.append_at("diary.os", "diary/Anything", &same).expect("second");

    let view = documents.read_at("diary.os", "diary/Anything").expect("read");
    assert_eq!(view.child_addresses.len(), 2);
}
