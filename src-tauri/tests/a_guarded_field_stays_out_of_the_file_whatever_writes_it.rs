//! A field marked `mutable="guarded"` is the viewer's, whichever action writes it.
//!
//! Only `set` used to ask. A `toggle` on a fold flag, an `inc` on a counter the page keeps for
//! itself, a `clear` - each wrote its value into the file as the document's own, so one person
//! folding a card folded it for everybody, and the backup committed it.

use overseer::server::DocumentRoot;
use serde_json::json;

const DOCUMENT: &str = r#"tab view (label="View", mutable=true) {

    bool folded (mutable="guarded") = false
    int seen (mutable="guarded") = 0
    string filter (mutable="guarded") = "open"
    int recorded = 0

    button fold (label="fold") {
        on click {
            toggle (path="../folded")
        }
    }

    button up (label="up") {
        on click {
            inc (path="../seen", by=2)
        }
    }

    button down (label="down") {
        on click {
            dec (path="../seen")
        }
    }

    button wipe (label="wipe") {
        on click {
            clear (path="../filter")
        }
    }

    button record (label="record") {
        on click {
            inc (path="../recorded")
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
    let root = std::env::temp_dir().join(format!("overseer_guarded_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("d.os"), DOCUMENT).expect("write");
    overseer::viewstate::forget_all();
    root
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("d.os")).expect("read")
}

fn press(service: &DocumentRoot, button: &str) -> serde_json::Value {
    let nodes = service.open("d.os").expect("open");
    let path = overseer::addressing::name_path(&nodes, &format!("view/{}", button))
        .unwrap_or_else(|| panic!("no such button: {}", button));
    service
        .command_for(
            "alice",
            Some("d.os"),
            "run_overseer_event",
            &json!({ "node_path": path, "event_name": "click" }),
        )
        .expect("the press was refused")
}

fn the_file_is_untouched_by(button: &str) {
    serialised(|| {
        let root = a_root(button);
        let service = DocumentRoot::new(&root).expect("open the root");
        press(&service, button);
        assert_eq!(on_disk(&root), DOCUMENT, "pressing {} wrote the viewer's value", button);
    });
}

#[test]
fn a_toggle_leaves_the_file_alone() {
    the_file_is_untouched_by("fold");
}

#[test]
fn an_inc_leaves_the_file_alone() {
    the_file_is_untouched_by("up");
}

#[test]
fn a_dec_leaves_the_file_alone() {
    the_file_is_untouched_by("down");
}

#[test]
fn a_clear_leaves_the_file_alone() {
    the_file_is_untouched_by("wipe");
}

#[test]
fn a_later_real_write_does_not_carry_them_in() {
    serialised(|| {
        let root = a_root("later");
        let service = DocumentRoot::new(&root).expect("open the root");
        for button in ["fold", "up", "up", "down", "wipe"] {
            press(&service, button);
        }
        press(&service, "record");

        let text = on_disk(&root);
        assert!(text.contains("bool folded (mutable=\"guarded\") = false"), "fold reached the file:\n{}", text);
        assert!(text.contains("int seen (mutable=\"guarded\") = 0"), "the count reached the file:\n{}", text);
        assert!(text.contains("string filter (mutable=\"guarded\") = \"open\""), "the clear reached the file:\n{}", text);
        assert!(text.contains("int recorded = 1"), "the real change did not land:\n{}", text);
    });
}

#[test]
fn the_viewer_still_has_what_they_toggled() {
    // Kept out of the file, but not out of the viewer's hands: a second toggle has to start from
    // the first one's result, or a fold button could only ever fold.
    serialised(|| {
        let root = a_root("twice");
        let service = DocumentRoot::new(&root).expect("open the root");
        press(&service, "fold");
        assert_eq!(
            overseer::viewstate::overlay("alice", "d.os").get("view/folded"),
            Some(&overseer::types::OverseerValue::Boolean(true)),
            "the toggle was not kept for the viewer"
        );
        press(&service, "fold");
        assert_eq!(
            overseer::viewstate::overlay("alice", "d.os").get("view/folded"),
            Some(&overseer::types::OverseerValue::Boolean(false)),
            "the second toggle did not start from the first"
        );
        assert_eq!(on_disk(&root), DOCUMENT);
    });
}
