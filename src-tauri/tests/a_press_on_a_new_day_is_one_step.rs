//! A press on a day the history has not got is one change, and one step to take back.
//!
//! The day has to be made before anything in it can be pressed, and making it was one
//! instruction and the press another: two file writes, so two presses of Undo for a meal logged
//! once. A field edited on such a day was already one, because the instruction that makes the
//! entry carries the value that asked for it. Now it can carry a press the same way - `then`, run
//! on the entry once it is there, in the same change.

use overseer::server::DocumentRoot;
use serde_json::{json, Value};

const DOCUMENT: &str = r#"tab tracker (label="Tracker", mutable=true) {

    div (hidden=true) {
        div Meal (layout="horizontal") {
            string what = ""
        }
        div Day (layout="vertical") {
            date day = "2026-01-01"
            int logged = 0
            list Meals (entry=<Meal>) { }
            div (layout="horizontal") {
                button add (label="add an apple") {
                    on click {
                        append (list="../Meals") {
                            - what = "apple"
                        }
                        inc (path="../logged", by=1)
                    }
                }
            }
        }
    }

    list Days (entry=<Day>, key="day") {
        - {
            - day = "2026-09-01"
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
    let root = std::env::temp_dir().join(format!("overseer_newday_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("t.os"), DOCUMENT).expect("write");
    overseer::viewstate::forget_all();
    root
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("t.os")).expect("read")
}

/// The page's message for a press inside a day the list may not have: one spelling of each name,
/// the press named from the entry down.
fn press_on(service: &DocumentRoot, day: &str) -> Value {
    service
        .command_for(
            "alice",
            Some("t.os"),
            "ensure_overseer_entry",
            &json!({
                "path": "t.os",
                "wanted": {
                    "list_path": ["tracker", "Days"],
                    "key_field": "day",
                    "key_value": { "String": day },
                    "template": "Day",
                    "position": "append",
                    "fields": {},
                    "then": { "within": ["add"], "event": "click" },
                },
            }),
        )
        .expect("the press was refused")
}

/// The body of one day, as the file writes it.
fn day_in(text: &str, day: &str) -> String {
    let at = text.find(&format!("- day = \"{}\"", day)).unwrap_or_else(|| panic!("no day {}:\n{}", day, text));
    let start = text[..at].rfind("        - {").unwrap();
    let end = text[at..].find("\n        }").map(|e| at + e).unwrap();
    text[start..end].to_string()
}

#[test]
fn the_day_is_made_and_the_press_lands_on_it() {
    serialised(|| {
        let root = a_root("lands");
        let service = DocumentRoot::new(&root).expect("open the root");
        press_on(&service, "2026-09-25");
        let day = day_in(&on_disk(&root), "2026-09-25");
        assert!(day.contains("- what = \"apple\""), "the press did not run on the new day:\n{}", day);
        assert!(day.contains("- logged = 1"), "{}", day);
    });
}

#[test]
fn it_is_one_step_to_take_back() {
    serialised(|| {
        let root = a_root("one_step");
        let service = DocumentRoot::new(&root).expect("open the root");
        press_on(&service, "2026-09-25");
        assert_eq!(overseer::undo::depth(&root, "t.os"), 1, "a press on a new day left two steps");
        let before = overseer::undo::take(&root, "t.os").expect("a step to take back");
        assert!(!before.contains("2026-09-25"), "one step back still has the day in it:\n{}", before);
    });
}

#[test]
fn a_day_already_there_is_pressed_as_it_is() {
    // Making sure of the entry when it exists is doing nothing, so this is simply the press.
    serialised(|| {
        let root = a_root("there");
        let service = DocumentRoot::new(&root).expect("open the root");
        press_on(&service, "2026-09-01");
        press_on(&service, "2026-09-01");
        let text = on_disk(&root);
        assert_eq!(text.matches("- day = \"2026-09-01\"").count(), 1, "the day was made twice:\n{}", text);
        assert!(day_in(&text, "2026-09-01").contains("- logged = 2"), "{}", text);
        assert_eq!(overseer::undo::depth(&root, "t.os"), 2, "two presses, two steps");
    });
}
