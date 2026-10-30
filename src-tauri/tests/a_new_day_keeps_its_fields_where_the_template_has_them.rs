//! A day made for a key the list lacks, from a template that lays its fields out in unnamed divs.
//!
//! The key field sat in an unnamed row beside the day's buttons, and making the entry looked for
//! it among the day's own fields only: it found none and added a second `date` beside the row,
//! which was written last. And a list two unnamed divs deep was written a space short of where
//! the entry's other fields are. A day made this way should read like any other.

use overseer::server::DocumentRoot;
use serde_json::{json, Value};

const DOCUMENT: &str = r#"tab tracker (label="Tracker", mutable=true) {

    div (hidden=true) {
        div Meal (layout="horizontal") {
            string what = ""
        }
        div Day (layout="vertical") {
            div (layout="horizontal") {
                date day = "2026-01-01"
                button add (label="add an apple") {
                    on click {
                        append (list="../Meals") {
                            - what = "apple"
                        }
                    }
                }
            }
            div (layout="flow") {
                div (layout="vertical") {
                    int logged = 0
                }
                list Meals (entry=<Meal>)
            }
        }
    }

    list Days (entry=<Day>, key="day") {
        - {
            - day = "2026-09-01"
            list Meals {
                - {
                    - what = "pear"
                }
            }
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
    let root = std::env::temp_dir().join(format!("overseer_newday_rows_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("t.os"), DOCUMENT).expect("write");
    overseer::viewstate::forget_all();
    root
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("t.os")).expect("read")
}

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

#[test]
fn the_new_day_is_written_as_the_days_before_it() {
    serialised(|| {
        let root = a_root("written");
        let service = DocumentRoot::new(&root).expect("open the root");
        press_on(&service, "2026-09-25");
        let text = on_disk(&root);
        let expected = r#"        - {
            - day = "2026-09-25"
            list Meals {
                - {
                    - what = "apple"
                }
            }
        }
"#;
        assert!(text.contains(expected), "the new day is not written like the one before it:\n{}", text);
        assert_eq!(text.matches("- day = ").count(), 2, "a day holds its key twice:\n{}", text);
    });
}
