//! A step you can take back is a change that happened.
//!
//! Undoing sometimes did nothing, and sometimes appeared to take back two changes at once while
//! still claiming another was waiting. Both are the same fault: a save that wrote exactly what
//! the file already said still recorded a step, so the history filled with snapshots identical
//! to the state around them. Taking one of those back changes nothing on screen, and the step
//! that really did move something is then one further back than the count suggests.
//!
//! Saving now happens on every change rather than when somebody presses Save, so this stopped
//! being rare. The document it was found on has an Update button that writes a field marked
//! `mutable="guarded"` - the button does something visible and the file correctly does not
//! move, which is exactly the case that produced an empty step every time it was pressed.

use overseer::server::DocumentRoot;
use serde_json::json;

const DOCUMENT: &str = "\
tab t (label=\"T\", mutable=true) {
    int a = 1
    int looking (mutable=\"guarded\") = 10

    button update (label=\"Update\") {
        on click {
            set (path=\"/t/looking\") = 20
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
    let root = std::env::temp_dir().join(format!("overseer_steps_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("t.os"), DOCUMENT).expect("write");
    overseer::viewstate::forget_all();
    root
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("t.os")).expect("read")
}

/// A save the way the page makes one.
fn save(service: &DocumentRoot, text: &str) {
    service
        .command(
            Some("t.os"),
            "save_overseer_file_from_text",
            &json!({ "path": "t.os", "content": text, "guarded": [] }),
        )
        .expect("save");
}

#[test]
fn saving_what_the_file_already_says_is_not_a_step() {
    serialised(|| {
        let root = a_root("nochange");
        let service = DocumentRoot::new(&root).expect("open the root");
        let held = on_disk(&root);

        save(&service, &held);
        save(&service, &held);
        save(&service, &held);

        assert_eq!(
            overseer::undo::depth(&root, "t.os"),
            0,
            "saving an unchanged document left steps that take nothing back"
        );
    });
}

#[test]
fn a_press_that_only_moves_the_viewer_is_not_a_step() {
    // The case from the real document: the button writes a guarded field, so the file correctly
    // does not move - and a step recorded for it is one that undoes nothing.
    serialised(|| {
        let root = a_root("guarded");
        let service = DocumentRoot::new(&root).expect("open the root");

        let pressed = service
            .command(
                Some("t.os"),
                "execute_overseer_event_update",
                &json!({
                    "content": on_disk(&root),
                    "node_path": ["t", "update"],
                    "event_name": "click"
                }),
            )
            .expect("press");
        let held = pressed.get("text").and_then(|t| t.as_str()).expect("no text").to_string();

        // The page saves straight after, naming the guarded field so its authored value goes back.
        service
            .command(
                Some("t.os"),
                "save_overseer_file_from_text",
                &json!({
                    "path": "t.os",
                    "content": held,
                    "guarded": [{ "path": "t/looking", "value": { "Integer": 10 } }]
                }),
            )
            .expect("save");

        assert_eq!(
            overseer::undo::depth(&root, "t.os"),
            0,
            "pressing a button that moves only the viewer left a step to take back"
        );
    });
}

#[test]
fn every_step_taken_back_changes_something() {
    // The property the count has to mean. Two real changes and a press in between that moves
    // only the viewer: two steps, and each one moves the document when it is taken back.
    serialised(|| {
        let root = a_root("real");
        let service = DocumentRoot::new(&root).expect("open the root");

        let first = on_disk(&root).replace("int a = 1", "int a = 2");
        save(&service, &first);
        let second = on_disk(&root).replace("int a = 2", "int a = 3");
        save(&service, &second);

        assert_eq!(overseer::undo::depth(&root, "t.os"), 2, "two changes were not two steps");

        let before_first_undo = on_disk(&root);
        service.undo("t.os").expect("undo");
        let after = on_disk(&root);
        assert_ne!(after, before_first_undo, "the first undo took nothing back");
        assert!(after.contains("int a = 2"), "the first undo skipped a change");

        service.undo("t.os").expect("undo again");
        assert!(on_disk(&root).contains("int a = 1"), "the second undo did not reach the start");
        assert_eq!(overseer::undo::depth(&root, "t.os"), 0);
    });
}

#[test]
fn the_count_says_how_many_undos_will_do_something() {
    // What made the fault confusing: it said one was left when taking that one back changed
    // nothing. The number has to be the number of times pressing undo moves the document.
    serialised(|| {
        let root = a_root("count");
        let service = DocumentRoot::new(&root).expect("open the root");

        save(&service, &on_disk(&root).replace("int a = 1", "int a = 2"));
        save(&service, &on_disk(&root)); // nothing moved
        save(&service, &on_disk(&root).replace("int a = 2", "int a = 3"));
        save(&service, &on_disk(&root)); // nothing moved again

        let steps = overseer::undo::depth(&root, "t.os");
        let mut moved = 0;
        for _ in 0..steps {
            let before = on_disk(&root);
            service.undo("t.os").expect("undo");
            if on_disk(&root) != before {
                moved += 1;
            }
        }
        assert_eq!(moved, steps, "{} steps were offered and {} of them did anything", steps, moved);
    });
}

// ---------------------------------------------------------------------------------------------
// The desktop app writes through its own function, and the same property has to hold there.
//
// This is also where the counting was reported as wrong a second time: several changes appearing
// to revert together. They were not several changes. A document that marks a field as the
// viewer's has fields that move on screen and correctly never reach the file, so they are not
// steps to take back - and undo reading the file back is what put them all on screen at once.
// The page carries them across that reload now; what is pinned here is the counting.

/// One field the file holds, one the viewer holds, and a list that is writable.
const WITH_A_VIEWERS_FIELD: &str = r#"int A (mutable=true) = 66
int C (mutable="guarded") = 11

list L (mutable=true, entry=int) {
    - 10
}
"#;

fn a_desktop_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_desktop_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("m.os"), WITH_A_VIEWERS_FIELD).expect("write");
    root
}

/// A save the way the desktop app makes one: the text the page holds, with the viewer's fields
/// named alongside so what the document authored goes back before anything is written.
fn desktop_save(root: &std::path::Path, held: &str) {
    let guarded = vec![overseer::app_api::GuardedRevert {
        path: "C".into(),
        value: Some(overseer::types::OverseerValue::Integer(11)),
    }];
    let text = overseer::app_api::save_document_from_text(held.to_string(), guarded)
        .expect("restore what the document authored");
    overseer::app_api::write_file_keeping_a_step_back(
        root.join("m.os").to_str().expect("a path"),
        &text,
    )
    .expect("write");
}

#[test]
fn moving_the_viewers_field_is_not_a_step_on_the_desktop_either() {
    let root = a_desktop_root("guarded");
    let held = WITH_A_VIEWERS_FIELD.replace("= 66", "= 67");
    desktop_save(&root, &held);
    assert_eq!(overseer::undo::depth(&root, "m.os"), 1, "a change the file holds was not a step");

    // The viewer now moves their own field, twice. Neither reaches the file, so neither is a
    // step - and the count has to keep saying one.
    let held = held.replace(r#"int C (mutable="guarded") = 11"#, r#"int C (mutable="guarded") = 99"#);
    desktop_save(&root, &held);
    let held = held.replace(r#"int C (mutable="guarded") = 99"#, r#"int C (mutable="guarded") = 42"#);
    desktop_save(&root, &held);

    assert_eq!(
        overseer::undo::depth(&root, "m.os"),
        1,
        "moving the viewer's own field left steps that take nothing back"
    );
    let on_disk = std::fs::read_to_string(root.join("m.os")).expect("read");
    assert!(
        on_disk.contains(r#"int C (mutable="guarded") = 11"#),
        "the viewer's field reached the file:\n{}",
        on_disk
    );
}

#[test]
fn two_fields_the_file_holds_are_two_steps() {
    // The other half of the same question: changes that are real are counted separately, whether
    // they are to one field or to two different ones.
    let root = a_desktop_root("two");
    let held = WITH_A_VIEWERS_FIELD.replace("= 66", "= 67");
    desktop_save(&root, &held);
    let held = held.replace("    - 10", "    - 15");
    desktop_save(&root, &held);
    assert_eq!(
        overseer::undo::depth(&root, "m.os"),
        2,
        "two changes the file holds were collapsed into one step"
    );
}
