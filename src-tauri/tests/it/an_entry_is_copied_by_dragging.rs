//! An entry is copied by dragging it with the copy key held, into its own list or another.
//!
//! What the drag writes is one change: a copy of the entry put where it was dropped, the entry
//! itself left where it was, in one write and one step to take back. The document says where it
//! is allowed - the entry's list says `copyable=true`, and a target other than that list says
//! `copyable=true` or `movable=true`; two lists only share entries made from the same template; and a keyed
//! list never takes a key it holds already, so a keyed list takes no copy of its own entries.

use crate::common::serialised;
use overseer::server::DocumentRoot;
use serde_json::json;

const DOCUMENT: &str = r#"tab board (mutable=true) {

    div (hidden=true) {

        div Card {
            string name = ""
            int size = 1
            string lane = ""
        }

        div Note {
            string name = ""
        }
    }

    list palette (entry=<Card>, key="name", copyable=true) {
        - {
            - name = "alpha"
            - size = 3
        }
        - {
            - name = "beta"
        }
    }

    list todo (entry=<Card>, key="name", movable=true) {
        - {
            - name = "gamma"
        }
    }

    list notes (entry=<Note>, copyable=true) {
        - {
            - name = "first"
        }
        - {
            - name = "second"
        }
    }

    list fixed (entry=<Card>, key="name") {
        - {
            - name = "delta"
        }
    }
}
"#;



fn a_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_copy_{}_{}", tag, std::process::id()));
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

fn copy(
    service: &DocumentRoot,
    entry: &str,
    to_list: &str,
    before: Option<&str>,
    setting: Option<(&str, &str)>,
) -> Result<serde_json::Value, String> {
    let entry = names(service, entry);
    let to_list = names(service, to_list);
    let before = before.map(|b| names(service, b));
    let mut args = json!({ "entry_path": entry, "to_list_path": to_list, "before": before });
    if let Some((field, value)) = setting {
        args["setting"] = json!({ "node_path": [field], "value": { "String": value } });
    }
    service
        .command_for("alice", Some("d.os"), "copy_overseer_entry", &args)
        .map_err(|e| format!("{:?}", e))
}

/// The names in a list, in the order the file has them.
fn order(text: &str, list: &str) -> Vec<String> {
    let start = text.find(&format!("list {} ", list)).expect("the list");
    let body = &text[start..];
    let end = body.find("\n    }\n").expect("the list's end");
    body[..end]
        .lines()
        .filter_map(|l| l.trim().strip_prefix("- name = \""))
        .map(|n| n.trim_end_matches('"').to_string())
        .collect()
}

#[test]
fn a_copy_lands_before_the_entry_it_is_dropped_on_and_the_entry_stays() {
    serialised(|| {
        let root = a_root("across");
        let service = DocumentRoot::new(&root).expect("open the root");

        copy(&service, "board/palette/[alpha]", "board/todo", Some("board/todo/[gamma]"), None).expect("copy");
        let text = on_disk(&root);
        assert_eq!(order(&text, "palette"), ["alpha", "beta"]);
        assert_eq!(order(&text, "todo"), ["alpha", "gamma"]);
        // Its fields come with it, and a default stays unsaid.
        assert_eq!(text.matches("- name = \"alpha\"\n            - size = 3\n").count(), 2, "{}", text);
        assert_eq!(text.matches("- size = ").count(), 2, "{}", text);

        copy(&service, "board/palette/[beta]", "board/todo", None, None).expect("to the end");
        assert_eq!(order(&on_disk(&root), "todo"), ["alpha", "gamma", "beta"]);
    });
}

#[test]
fn one_undo_takes_the_copy_back() {
    serialised(|| {
        let root = a_root("undo");
        let service = DocumentRoot::new(&root).expect("open the root");
        let before = on_disk(&root);
        copy(&service, "board/palette/[beta]", "board/todo", None, None).expect("copy");
        assert_ne!(on_disk(&root), before);
        service.undo("d.os").expect("undo");
        assert_eq!(on_disk(&root), before, "one undo did not take the copy back");
    });
}

#[test]
fn what_the_document_does_not_allow_is_refused_and_nothing_is_written() {
    serialised(|| {
        let root = a_root("refused");
        let service = DocumentRoot::new(&root).expect("open the root");
        let before = on_disk(&root);

        assert!(copy(&service, "board/palette/[alpha]", "board/notes", None, None).is_err(), "a different template");
        assert!(copy(&service, "board/fixed/[delta]", "board/todo", None, None).is_err(), "out of a list that says neither");
        assert!(copy(&service, "board/todo/[gamma]", "board/palette", None, None).is_err(), "out of a list that only moves");
        assert!(copy(&service, "board/palette/[alpha]", "board/fixed", None, None).is_err(), "into a list that says neither");
        assert!(copy(&service, "board/palette/[alpha]", "board/palette", None, None).is_err(), "its own keyed list");
        assert_eq!(on_disk(&root), before, "a refused copy wrote something");

        copy(&service, "board/palette/[alpha]", "board/todo", None, None).expect("the first copy");
        let once = on_disk(&root);
        assert!(copy(&service, "board/palette/[alpha]", "board/todo", None, None).is_err(), "a key the list holds already");
        assert_eq!(on_disk(&root), once, "a refused copy wrote something");
    });
}

#[test]
fn a_list_without_a_key_takes_copies_of_its_own_entries() {
    serialised(|| {
        let root = a_root("unkeyed");
        let service = DocumentRoot::new(&root).expect("open the root");
        let nodes = service.open("d.os").expect("open");
        let in_notes: Vec<String> = overseer::addressing::addresses(&nodes)
            .into_iter()
            .filter(|a| a.starts_with("board/notes/") && a.matches('/').count() == 2)
            .collect();
        assert_eq!(in_notes.len(), 2, "{:?}", in_notes);

        copy(&service, &in_notes[1], "board/notes", Some(&in_notes[0]), None).expect("copy");
        assert_eq!(order(&on_disk(&root), "notes"), ["second", "first", "second"]);
    });
}

#[test]
fn a_copy_can_take_a_value_where_it_lands() {
    serialised(|| {
        let root = a_root("setting");
        let service = DocumentRoot::new(&root).expect("open the root");
        copy(&service, "board/palette/[beta]", "board/todo", None, Some(("lane", "doing"))).expect("copy");
        let text = on_disk(&root);
        assert_eq!(text.matches("- lane = \"doing\"").count(), 1, "{}", text);
        let todo = &text[text.find("list todo ").unwrap()..text.find("list notes ").unwrap()];
        assert!(todo.contains("- name = \"beta\"\n            - lane = \"doing\"\n"), "{}", text);
    });
}
