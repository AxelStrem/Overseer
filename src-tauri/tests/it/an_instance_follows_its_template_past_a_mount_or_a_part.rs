//! What is made from a template follows a change to the template at once, also when the same
//! press wrote through a mount or into a list that keeps its entries in parts.
//!
//! A change inside a template is settled by reading the document again from its text - see
//! `an_instance_follows_its_template`. What a mount brought in and what a part holds are no part
//! of that text: they are written to their own files after the settle, from the tree, so reading
//! the text before then would have brought them back as the files still said. Such a press was
//! settled through the graph alone, and every instance kept the template's old value, in the
//! answer and in the document held, until the document was read afresh.

use crate::common::serial;
use overseer::app_api;
use overseer::docmgr::manager::DocumentManager;
use overseer::server::DocumentRoot;
use overseer::types::*;
use std::path::{Path, PathBuf};

const SIDE: &str = r#"tab s (label="S", mutable=true) {
    div (hidden=true) {
        div Thing (layout="horizontal") {
            string handle = ""
            int worth = 0
        }
    }

    list Things (entry=<Thing>, key="handle") {
        - {
            - handle = "a"
            - worth = 1
        }
    }
}
"#;

const THIS_WEEK: &str = r#"list History {
    - {
        - handle = "b"
        - points = 5
    }
}
"#;

fn host(side: &str) -> String {
    r##"tab t (label="T", mutable=true) {
    div (hidden=true) {
        div Card {
            string title = ""
            string note = "plain"
        }
        div Done (layout="horizontal") {
            string handle = ""
            timestamp done_at = "2026-11-02T10:00:00Z"
            int points = 1
        }
    }

    mount CATALOG (lazy=false, mutable=true, source="SIDE/s/Things") { }

    list History (entry=<Done>, key="handle", parts=$(|x| week(x/done_at))) { }

    <Card> one {
        - title = "One"
    }
    string shown = $(one/note)
    int total (mutable=false) = $(History.sum(|x| x/points))
    int worth (mutable=false) = $(CATALOG/Things.sum(|x| x/worth))

    button catalogue (label="Catalogue") {
        on click {
            set (path="/t/Card/note") = "catalogued"
            append (list="../CATALOG/Things") {
                - handle = "c"
                - worth = 3
            }
        }
    }

    button log (label="Log") {
        on click {
            set (path="/t/Card/note") = "logged"
            append (list="../History") {
                - handle = "c"
                - points = 2
            }
        }
    }
}
"##
    .replace("SIDE", side)
}


struct Sandbox(PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sandbox(tag: &str, side: Option<&str>) -> Sandbox {
    let root = std::env::temp_dir().join(format!("overseer_follow_past_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("d.parts/History")).unwrap();
    // The app names a mount's file whole; the server reads it beside the document.
    let whole = root.join("side.os").to_string_lossy().replace('\\', "/");
    std::fs::write(root.join("d.os"), host(side.unwrap_or(&whole))).unwrap();
    std::fs::write(root.join("side.os"), SIDE).unwrap();
    std::fs::write(root.join("d.parts/History/2026-W45.os"), THIS_WEEK).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    Sandbox(root)
}

fn on_disk(root: &Path, name: &str) -> String {
    std::fs::read_to_string(root.join(name)).unwrap()
}

fn value_in(nodes: &[OverseerNode], address: &str) -> OverseerValue {
    let node = overseer::addressing::find(nodes, address).unwrap_or_else(|| panic!("nothing at {address}"));
    node.parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
        .cloned()
        .unwrap_or_else(|| panic!("no value at {address}"))
}

fn text(value: &str) -> OverseerValue {
    OverseerValue::String(value.to_string())
}

/// The instance and what reads it say what the template now says, and what the press wrote
/// outside the document's text is still there, with what reads that.
fn follows(nodes: &[OverseerNode], note: &str, worth: i64, total: i64, when: &str) {
    assert_eq!(value_in(nodes, "t/one/note"), text(note), "the instance, {when}");
    assert_eq!(value_in(nodes, "t/shown"), text(note), "what reads the instance, {when}");
    assert_eq!(value_in(nodes, "t/worth"), OverseerValue::Integer(worth), "what reads the mount, {when}");
    assert_eq!(value_in(nodes, "t/total"), OverseerValue::Integer(total), "what reads the parts, {when}");
    if worth == 4 {
        assert_eq!(value_in(nodes, "t/CATALOG/Things/[c]/worth"), OverseerValue::Integer(3), "{when}");
    }
    if total == 7 {
        assert_eq!(value_in(nodes, "t/History/[c]/points"), OverseerValue::Integer(2), "{when}");
    }
}

fn whole(pressed: overseer::server::EventOutcome) -> Vec<OverseerNode> {
    vec![pressed.node.expect("the tab the button is in")]
}

#[test]
fn a_press_that_also_writes_through_a_mount_shows_the_instances_changed() {
    let _guard = serial();
    let sandbox = sandbox("mount", Some("side.os"));
    let service = DocumentRoot::new(&sandbox.0).unwrap();
    // Read first, so the document is held worked out and the press starts from it.
    follows(&service.open("d.os").unwrap(), "plain", 1, 5, "to begin with");

    let pressed = service.run_event("d.os", "t/catalogue", "click").unwrap();
    follows(&whole(pressed), "catalogued", 4, 5, "in the answer");
    follows(&service.open("d.os").unwrap(), "catalogued", 4, 5, "in the document held");
    assert!(on_disk(&sandbox.0, "side.os").contains("- handle = \"c\""), "the mounted file has the entry");
    assert_eq!(
        on_disk(&sandbox.0, "d.os"),
        host("side.os").replace("string note = \"plain\"", "string note = \"catalogued\""),
        "one line of the document moved"
    );

    // And the next write starts from a document that has both.
    service.set_at("d.os", "t/one/title", text("Uno")).unwrap();
    follows(&service.open("d.os").unwrap(), "catalogued", 4, 5, "after a write that followed");
}

#[test]
fn a_press_that_also_writes_into_a_part_shows_the_instances_changed() {
    let _guard = serial();
    let sandbox = sandbox("part", Some("side.os"));
    let service = DocumentRoot::new(&sandbox.0).unwrap();
    follows(&service.open("d.os").unwrap(), "plain", 1, 5, "to begin with");

    let pressed = service.run_event("d.os", "t/log", "click").unwrap();
    follows(&whole(pressed), "logged", 1, 7, "in the answer");
    follows(&service.open("d.os").unwrap(), "logged", 1, 7, "in the document held");
    assert!(on_disk(&sandbox.0, "d.parts/History/2026-W45.os").contains("- handle = \"c\""), "the part has the entry");
    assert_eq!(
        on_disk(&sandbox.0, "d.os"),
        host("side.os").replace("string note = \"plain\"", "string note = \"logged\""),
        "one line of the document moved"
    );

    service.set_at("d.os", "t/one/title", text("Uno")).unwrap();
    follows(&service.open("d.os").unwrap(), "logged", 1, 7, "after a write that followed");
}

#[test]
fn the_pages_presses_show_them_changed_too() {
    let _guard = serial();
    let sandbox = sandbox("page", None);
    let file = sandbox.0.join("d.os");
    let path = file.to_string_lossy().into_owned();
    let within = |f: &mut dyn FnMut()| DocumentManager::with_document_file(Some(file.clone()), f);

    // The document as the page holds it once opened, and each answer taken on as the page does.
    let mut page = Vec::new();
    within(&mut || page = app_api::load_document(on_disk(&sandbox.0, "d.os")).expect("open"));
    follows(&page, "plain", 1, 5, "to begin with");

    for (button, note, worth, total) in [("catalogue", "catalogued", 4, 5), ("log", "logged", 4, 7)] {
        let mut answer = None;
        within(&mut || {
            answer = Some(
                app_api::run_event_at(&path, "d.os", vec!["t".into(), button.into()], "click".into(), "", Vec::new(), Vec::new())
                    .expect("the press"),
            )
        });
        let answer = answer.unwrap();
        assert!(answer.wrote, "{button} wrote the document");
        match answer.nodes {
            Some(whole) => page = whole,
            None => overseer::delta::apply(&mut page, answer.changes.expect("an answer describes something")),
        }
        follows(&page, note, worth, total, &format!("on the page after {button}"));
        // What is held for the text the page now holds is the same document.
        let mut held = Vec::new();
        within(&mut || held = app_api::load_document(answer.text.clone()).expect("the document held"));
        follows(&held, note, worth, total, &format!("in the document held after {button}"));
    }
    assert!(on_disk(&sandbox.0, "side.os").contains("- handle = \"c\""));
    assert!(on_disk(&sandbox.0, "d.parts/History/2026-W45.os").contains("- handle = \"c\""));
}
