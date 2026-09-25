//! A form fills a new list entry in one press.
//!
//! A div of textboxes and a button whose `append (list=..., from="..")` copies the div onto a new
//! entry. The entry is made from the list's template, as any appended entry is; the form only
//! supplies values, matched by name and converted to whatever the template says each field is.
//!
//! What is typed into a textbox belongs to the page. It reaches the backend with the press and
//! nowhere else, and is gone again before anything is written: the file says what a box starts
//! with, and nothing more, however many times the form is used.
//!
//! Through the door the page uses, with the message it sends, because the last two commands
//! written for the page were each refused from their first deploy over the shape of theirs.

use overseer::server::DocumentRoot;
use serde_json::{json, Value};

const DOCUMENT: &str = r#"tab project (label="P", mutable=true) {

    div (hidden=true) {
        div Item (layout="horizontal") {
            timestamp added (hidden=true) = "2026-01-01T00:00:00Z"
            string title = ""
            tags labels = ""
            int points = 1
            date due = "2026-01-01"
            bool urgent = false
            string commentary = ""
            string note = "kept"
        }
    }

    div NewTask (layout="horizontal") {
        textbox title (placeholder="what") = ""

        // Arranging, not meaning anything: the fields in it still match by their own names.
        div (layout="horizontal") {
            textbox points = ""
            textbox due = ""
        }

        textbox urgent = ""
        textbox labels = ""
        textbox nonsense = ""
        textbox commentary = "starts like this"
        string note = "from the form"

        button add (label="add") {
            on click {
                append (list="/project/Items", from="..") {
                    - added = $(now())
                }
            }
        }

        button add_titled (label="add") {
            on click {
                append (list="/project/Items", from="..") {
                    - added = $(now())
                    - title = "said by the block"
                }
            }
        }
    }

    list Items (entry=<Item>, key="added") {
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
    let root = std::env::temp_dir().join(format!("overseer_form_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("p.os"), DOCUMENT).expect("write");
    overseer::viewstate::forget_all();
    root
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("p.os")).expect("read")
}

/// The one entry the list holds, as the file writes it.
fn the_entry(text: &str) -> String {
    let at = text.find("list Items").expect("no list");
    text[at..].to_string()
}

fn path(field: &str) -> Vec<&str> {
    vec!["project", "NewTask", field]
}

/// A press, as the page sends one: the button's path in both spellings at the top level, where
/// either is read, and each typed box's path in the one spelling a value takes.
fn press(service: &DocumentRoot, button: &str, typed: &[(&str, &str)]) -> Value {
    let typed: Vec<Value> = typed
        .iter()
        .map(|(field, text)| json!({ "node_path": path(field), "value": { "String": text } }))
        .collect();
    service
        .command_for(
            "alice",
            Some("p.os"),
            "run_overseer_event",
            &json!({
                "path": "p.os",
                "node_path": path(button),
                "nodePath": path(button),
                "event_name": "click",
                "eventName": "click",
                "typed": typed,
            }),
        )
        .expect("the press was refused")
}

#[test]
fn a_form_fills_a_new_entry() {
    serialised(|| {
        let root = a_root("fills");
        let service = DocumentRoot::new(&root).expect("open the root");
        press(
            &service,
            "add",
            &[("title", "Write the docs"), ("points", "3"), ("due", "2026-10-01"), ("urgent", "yes"), ("labels", "ui, dsl")],
        );
        let entry = the_entry(&on_disk(&root));
        assert!(entry.contains("- title = \"Write the docs\""), "{}", entry);
        assert!(entry.contains("- points = 3"), "the text was not made a number: {}", entry);
        assert!(entry.contains("- due = \"2026-10-01\""), "{}", entry);
        assert!(entry.contains("- urgent = true"), "the text was not made a flag: {}", entry);
        assert!(entry.contains("- labels = \"ui, dsl\""), "{}", entry);
        assert!(entry.contains("- added = "), "the block's own line was lost: {}", entry);
        assert_eq!(overseer::undo::depth(&root, "p.os"), 1, "one press, one step to take back");
    });
}

#[test]
fn what_is_typed_never_reaches_the_file() {
    serialised(|| {
        let root = a_root("never");
        let service = DocumentRoot::new(&root).expect("open the root");
        press(&service, "add", &[("title", "Write the docs"), ("commentary", "typed over")]);
        let text = on_disk(&root);
        let form = &text[text.find("div NewTask").unwrap()..text.find("list Items").unwrap()];
        assert!(form.contains("textbox title (placeholder=\"what\") = \"\""), "{}", form);
        assert!(form.contains("textbox commentary = \"starts like this\""), "{}", form);
        assert!(!text.contains("Write the docs\"\n        textbox"), "{}", text);
        assert!(!text.contains("_typed"), "the marker was written: {}", text);
    });
}

#[test]
fn a_box_nobody_typed_into_gives_what_it_starts_with() {
    serialised(|| {
        let root = a_root("starting");
        let service = DocumentRoot::new(&root).expect("open the root");
        press(&service, "add", &[("title", "x")]);
        let entry = the_entry(&on_disk(&root));
        assert!(entry.contains("- commentary = \"starts like this\""), "{}", entry);
    });
}

#[test]
fn what_cannot_be_converted_is_left_to_the_template() {
    // And so is an empty box: nothing typed is not a value to write over the default with.
    serialised(|| {
        let root = a_root("unconvertible");
        let service = DocumentRoot::new(&root).expect("open the root");
        press(&service, "add", &[("title", "x"), ("points", "three"), ("due", "soon"), ("urgent", "")]);
        let entry = the_entry(&on_disk(&root));
        assert!(!entry.contains("- points"), "{}", entry);
        assert!(!entry.contains("- due"), "{}", entry);
        assert!(!entry.contains("- urgent"), "{}", entry);
    });
}

#[test]
fn a_field_the_template_lacks_is_not_copied() {
    serialised(|| {
        let root = a_root("lacks");
        let service = DocumentRoot::new(&root).expect("open the root");
        press(&service, "add", &[("title", "x"), ("nonsense", "anything")]);
        let entry = the_entry(&on_disk(&root));
        assert!(!entry.contains("nonsense"), "{}", entry);
    });
}

#[test]
fn a_field_the_block_names_is_the_blocks() {
    serialised(|| {
        let root = a_root("block");
        let service = DocumentRoot::new(&root).expect("open the root");
        press(&service, "add_titled", &[("title", "typed")]);
        let entry = the_entry(&on_disk(&root));
        assert!(entry.contains("- title = \"said by the block\""), "{}", entry);
        assert!(!entry.contains("typed"), "{}", entry);
    });
}

#[test]
fn only_a_textbox_takes_typed_text() {
    // This is a way to say what was typed, not a way to write a field without writing it.
    serialised(|| {
        let root = a_root("only");
        let service = DocumentRoot::new(&root).expect("open the root");
        press(&service, "add", &[("title", "x"), ("note", "slipped in")]);
        let text = on_disk(&root);
        assert!(!text.contains("slipped in"), "{}", text);
        assert!(the_entry(&text).contains("- note = \"from the form\""), "{}", text);
    });
}

#[test]
fn the_answer_names_the_boxes_to_empty() {
    // The page holds what is typed, so it is the page that empties the form - and only the boxes
    // the copy read, with something in them. A press elsewhere must leave a half-typed form alone.
    serialised(|| {
        let root = a_root("emptied");
        let service = DocumentRoot::new(&root).expect("open the root");
        let answer = press(&service, "add", &[("title", "x"), ("points", "3")]);
        let mut emptied: Vec<Vec<String>> =
            serde_json::from_value(answer.get("emptied").cloned().unwrap_or(json!([]))).unwrap();
        emptied.sort();
        assert_eq!(
            emptied,
            vec![
                vec!["project".to_string(), "NewTask".into(), "points".into()],
                vec!["project".to_string(), "NewTask".into(), "title".into()],
            ]
        );
    });
}

#[test]
fn a_press_without_typing_is_the_press_it_always_was() {
    // Older pages send no `typed` at all.
    serialised(|| {
        let root = a_root("untyped");
        let service = DocumentRoot::new(&root).expect("open the root");
        service
            .command_for(
                "alice",
                Some("p.os"),
                "run_overseer_event",
                &json!({ "node_path": path("add"), "event_name": "click" }),
            )
            .expect("a press without typing was refused");
        assert!(the_entry(&on_disk(&root)).contains("- added = "));
    });
}
