//! Adding and removing an entry are instructions too.
//!
//! A value the page changes is named and written. A change of shape could not be: adding an
//! entry meant building it in the page and letting a save of the whole document carry it over
//! the file - which is the pattern that threw away whatever else had written in the meantime,
//! and the reason `instructions` was filed in the first place. It survived that item because
//! the two writes that happen constantly are a field edit and a press, and these are rarer.
//!
//! Most shape changes already went as instructions without anybody arranging it: an Add button
//! and a Remove button are presses, and a press has been an instruction since `instructions`.
//! What was left was the page doing it directly - materialising a row for a day that is not in
//! the history yet, so a field on it can be edited.
//!
//! An entry added or taken out renames every entry after it, so the names the page is holding
//! no longer mean what they did. What it is told is therefore not "this value moved" but "here
//! is that list again" - a subtree, which it adopts whole. That was already how a change of
//! shape was described; worth a test of its own, because describing one of these field by field
//! would be describing entries by names that have shifted under them.

use overseer::server::DocumentRoot;
use serde_json::json;

const DOCUMENT: &str = r#"tab day (label="Day", mutable=true) {

    div (hidden=true) {

        div Meal (layout="horizontal") {
            string food = ""
            int portions = 1
        }
    }

    int recorded = 0

    list intake (entry=<Meal>, layout="vertical") {
        - {
            - food = "porridge"
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
    let root = std::env::temp_dir().join(format!("overseer_shape_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("d.os"), DOCUMENT).expect("write");
    overseer::viewstate::forget_all();
    root
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("d.os")).expect("read")
}

/// The path of names the page sends for the node at an address.
fn names(service: &DocumentRoot, address: &str) -> Vec<String> {
    let nodes = service.open("d.os").expect("open");
    overseer::addressing::name_path(&nodes, address)
        .unwrap_or_else(|| panic!("no such address: {}", address))
}

fn add(service: &DocumentRoot, fields: serde_json::Value) -> serde_json::Value {
    let list = names(service, "day/intake");
    service
        .command_for(
            "alice",
            Some("d.os"),
            "append_overseer_entry",
            &json!({ "list_path": list, "fields": fields }),
        )
        .expect("the append was refused")
}

fn take_out(service: &DocumentRoot, entry: &str) -> serde_json::Value {
    let path = names(service, &format!("day/intake/{}", entry));
    service
        .command_for(
            "alice",
            Some("d.os"),
            "remove_overseer_entry",
            &json!({ "entry_path": path }),
        )
        .expect("the removal was refused")
}

fn entries(root: &std::path::Path) -> usize {
    on_disk(root).matches("- food = ").count()
}

#[test]
fn an_entry_can_be_added_by_naming_the_list() {
    serialised(|| {
        let root = a_root("add");
        let service = DocumentRoot::new(&root).expect("open the root");
        add(&service, json!({ "food": { "String": "toast" }, "portions": { "Integer": 2 } }));

        let text = on_disk(&root);
        assert!(text.contains(r#"- food = "toast""#), "the entry did not land:\n{}", text);
        assert!(text.contains("- portions = 2"), "its fields did not land:\n{}", text);
        assert_eq!(entries(&root), 2, "the list should hold two:\n{}", text);
    });
}

#[test]
fn the_document_it_was_added_to_is_not_disturbed() {
    // The whole point of an instruction: it is applied to what the file says, so nothing else
    // in the document is rewritten and nothing anybody else wrote is lost.
    serialised(|| {
        let root = a_root("quiet");
        let service = DocumentRoot::new(&root).expect("open the root");
        let before = on_disk(&root);
        add(&service, json!({ "food": { "String": "toast" } }));
        let after = on_disk(&root);

        let gone: Vec<&str> = before.lines().filter(|l| !after.lines().any(|a| a == *l)).collect();
        assert!(gone.is_empty(), "the append rewrote lines it did not add: {:?}", gone);
        assert!(after.contains(r#"- food = "porridge""#), "the entry already there was lost");
    });
}

#[test]
fn a_write_made_while_the_page_was_open_survives_an_append() {
    serialised(|| {
        let root = a_root("nolose");
        let service = DocumentRoot::new(&root).expect("open the root");
        // Something else writes - the bot recording against the same document.
        service
            .set_at("d.os", "day/recorded", overseer::types::OverseerValue::Integer(7))
            .expect("the other write");
        add(&service, json!({ "food": { "String": "toast" } }));

        let text = on_disk(&root);
        assert!(text.contains("int recorded = 7"), "the other write was thrown away:\n{}", text);
        assert!(text.contains(r#"- food = "toast""#), "the append did not land:\n{}", text);
    });
}

#[test]
fn an_entry_can_be_taken_out_again() {
    serialised(|| {
        let root = a_root("remove");
        let service = DocumentRoot::new(&root).expect("open the root");
        add(&service, json!({ "food": { "String": "toast" } }));
        assert_eq!(entries(&root), 2);

        take_out(&service, "Meal__1");
        let text = on_disk(&root);
        assert_eq!(entries(&root), 1, "the removal did not take:\n{}", text);
        assert!(
            !text.contains(r#"- food = "porridge""#),
            "the wrong entry was taken out:\n{}",
            text
        );
    });
}

#[test]
fn the_list_is_described_whole_rather_than_field_by_field() {
    // Because the entries after the new one have been renamed. A description that named them
    // individually would be naming things that have moved; one that hands back the list says
    // exactly as much and cannot be misapplied.
    serialised(|| {
        let root = a_root("whole");
        let service = DocumentRoot::new(&root).expect("open the root");
        service.open_for("alice", "d.os").expect("the page opens it");

        let answer = add(&service, json!({ "food": { "String": "toast" } }));
        let changes = answer
            .get("changes")
            .and_then(|c| c.as_array())
            .unwrap_or_else(|| panic!("no changes to apply: {}", answer));
        let describes_the_list = changes.iter().any(|c| {
            c.get("kind").and_then(|k| k.as_str()) == Some("subtree")
                && c.get("address").and_then(|a| a.as_str()) == Some("day/intake")
        });
        assert!(
            describes_the_list,
            "the list was not handed back whole: {:?}",
            changes
        );
        assert_eq!(answer.get("wrote").and_then(|w| w.as_bool()), Some(true));
    });
}

#[test]
fn a_page_that_has_not_opened_it_is_given_the_document() {
    // Nothing to describe a change against, so the answer is the document itself.
    serialised(|| {
        let root = a_root("cold");
        let service = DocumentRoot::new(&root).expect("open the root");
        // The path first: asking for it works the document out, which is the very thing being
        // taken away here.
        let list = names(&service, "day/intake");
        overseer::app_api::forget_baseline();
        let answer = service
            .command_for(
                "alice",
                Some("d.os"),
                "append_overseer_entry",
                &json!({ "list_path": list, "fields": { "food": { "String": "toast" } } }),
            )
            .expect("the append was refused");
        assert!(
            answer.get("nodes").and_then(|n| n.as_array()).is_some_and(|n| !n.is_empty()),
            "neither changes nor a document came back: {}",
            answer
        );
    });
}

#[test]
fn each_one_is_a_step_to_take_back() {
    serialised(|| {
        let root = a_root("undo");
        let service = DocumentRoot::new(&root).expect("open the root");
        add(&service, json!({ "food": { "String": "toast" } }));
        assert_eq!(overseer::undo::depth(&root, "d.os"), 1, "no step to take it back");

        service.undo("d.os").expect("undo");
        assert_eq!(entries(&root), 1, "undo did not take the entry out again");
        assert!(on_disk(&root).contains(r#"- food = "porridge""#));
    });
}

#[test]
fn appending_to_something_that_is_not_a_list_is_refused() {
    // Loudly, rather than writing somewhere unexpected. The page builds these paths itself.
    serialised(|| {
        let root = a_root("refuse");
        let service = DocumentRoot::new(&root).expect("open the root");
        let not_a_list = names(&service, "day/recorded");
        let answer = service.command_for(
            "alice",
            Some("d.os"),
            "append_overseer_entry",
            &json!({ "list_path": not_a_list, "fields": {} }),
        );
        assert!(answer.is_err(), "appending to a field was accepted");
        assert_eq!(on_disk(&root), DOCUMENT, "a refused append still wrote");
    });
}
