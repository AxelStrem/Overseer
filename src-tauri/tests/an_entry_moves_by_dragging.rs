//! An entry moves by dragging, within its list or into another.
//!
//! What a drag writes is one change: the entry taken out of where it was and put where it was
//! dropped, in one write and one step to take back, so it is never in both lists or in neither.
//! The document says where it is allowed - both lists say `movable=true`, and two lists only
//! trade entries made from the same template. Where it lands is the entry it goes before, which
//! is what the page knows even when the list is drawn filtered.

use overseer::server::DocumentRoot;
use serde_json::json;

const DOCUMENT: &str = r#"tab board (mutable=true) {

    div (hidden=true) {

        div Card {
            string name = ""
            int size = 1
        }

        div Note {
            string name = ""
        }
    }

    list todo (entry=<Card>, key="name", movable=true) {
        - {
            - name = "alpha"
        }
        - {
            - name = "beta"
            - size = 3
        }
        - {
            - name = "gamma"
        }
    }

    list done (entry=<Card>, key="name", movable=true) {
        - {
            - name = "delta"
        }
    }

    list notes (entry=<Note>, movable=true) {
        - {
            - name = "alpha"
        }
    }

    list fixed (entry=<Card>, key="name") {
        - {
            - name = "epsilon"
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
    let root = std::env::temp_dir().join(format!("overseer_drag_{}_{}", tag, std::process::id()));
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

fn drag(
    service: &DocumentRoot,
    entry: &str,
    to_list: &str,
    before: Option<&str>,
) -> Result<serde_json::Value, String> {
    let entry = names(service, entry);
    let to_list = names(service, to_list);
    let before = before.map(|b| names(service, b));
    service
        .command_for(
            "alice",
            Some("d.os"),
            "move_overseer_entry",
            &json!({ "entry_path": entry, "to_list_path": to_list, "before": before }),
        )
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
fn an_entry_moves_up_and_down_its_list() {
    serialised(|| {
        let root = a_root("reorder");
        let service = DocumentRoot::new(&root).expect("open the root");

        drag(&service, "board/todo/[gamma]", "board/todo", Some("board/todo/[alpha]")).expect("up");
        assert_eq!(order(&on_disk(&root), "todo"), ["gamma", "alpha", "beta"]);

        drag(&service, "board/todo/[gamma]", "board/todo", None).expect("to the end");
        assert_eq!(order(&on_disk(&root), "todo"), ["alpha", "beta", "gamma"]);

        drag(&service, "board/todo/[alpha]", "board/todo", Some("board/todo/[gamma]")).expect("down");
        let text = on_disk(&root);
        assert_eq!(order(&text, "todo"), ["beta", "alpha", "gamma"]);
        // Its fields go with it, and a default stays unsaid.
        assert!(text.contains("- name = \"beta\"\n            - size = 3\n"), "{}", text);
        assert_eq!(text.matches("- size = ").count(), 1, "{}", text);
    });
}

#[test]
fn an_entry_moves_into_another_list_made_from_its_template() {
    serialised(|| {
        let root = a_root("across");
        let service = DocumentRoot::new(&root).expect("open the root");

        drag(&service, "board/todo/[beta]", "board/done", Some("board/done/[delta]")).expect("across");
        let text = on_disk(&root);
        assert_eq!(order(&text, "todo"), ["alpha", "gamma"]);
        assert_eq!(order(&text, "done"), ["beta", "delta"]);
        assert!(text.contains("- name = \"beta\"\n            - size = 3\n"), "{}", text);

        drag(&service, "board/todo/[alpha]", "board/done", None).expect("to the end");
        let text = on_disk(&root);
        assert_eq!(order(&text, "todo"), ["gamma"]);
        assert_eq!(order(&text, "done"), ["beta", "delta", "alpha"]);
    });
}

#[test]
fn one_undo_puts_both_lists_back() {
    serialised(|| {
        let root = a_root("undo");
        let service = DocumentRoot::new(&root).expect("open the root");
        let before = on_disk(&root);
        drag(&service, "board/todo/[beta]", "board/done", None).expect("across");
        assert_ne!(on_disk(&root), before);
        service.undo("d.os").expect("undo");
        assert_eq!(on_disk(&root), before, "one undo did not restore both lists");
    });
}

#[test]
fn what_the_document_does_not_allow_is_refused_and_nothing_moves() {
    serialised(|| {
        let root = a_root("refused");
        let service = DocumentRoot::new(&root).expect("open the root");
        let before = on_disk(&root);

        assert!(drag(&service, "board/todo/[alpha]", "board/notes", None).is_err(), "a different template");
        assert!(drag(&service, "board/fixed/[epsilon]", "board/todo", None).is_err(), "out of a list that is not movable");
        assert!(drag(&service, "board/todo/[alpha]", "board/fixed", None).is_err(), "into a list that is not movable");
        std::fs::write(
            root.join("d.os"),
            before.replace("- name = \"delta\"", "- name = \"alpha\""),
        )
        .expect("write");
        let clashing = on_disk(&root);
        assert!(drag(&service, "board/todo/[alpha]", "board/done", None).is_err(), "a key the list holds already");
        assert_eq!(on_disk(&root), clashing, "a refused move wrote something");
    });
}

#[test]
fn a_drop_where_it_already_is_changes_nothing() {
    serialised(|| {
        let root = a_root("inplace");
        let service = DocumentRoot::new(&root).expect("open the root");
        let before = on_disk(&root);
        drag(&service, "board/todo/[alpha]", "board/todo", Some("board/todo/[beta]")).expect("in place");
        drag(&service, "board/todo/[beta]", "board/todo", Some("board/todo/[beta]")).expect("onto itself");
        assert_eq!(order(&on_disk(&root), "todo"), order(&before, "todo"));
    });
}

#[test]
fn entries_of_lists_without_a_key_move_by_where_they_are() {
    serialised(|| {
        let root = a_root("unkeyed");
        std::fs::write(
            root.join("d.os"),
            DOCUMENT.replace(
                "    list notes (entry=<Note>, movable=true) {\n",
                "    list notes (entry=<Note>, movable=true) {\n        - {\n            - name = \"first\"\n        }\n",
            )
            .replace("    list fixed (entry=<Card>, key=\"name\") {", "    list more (entry=<Note>, movable=true) {\n    }\n\n    list fixed (entry=<Card>, key=\"name\") {"),
        )
        .expect("write");
        let service = DocumentRoot::new(&root).expect("open the root");
        let nodes = service.open("d.os").expect("open");
        let in_notes: Vec<String> = overseer::addressing::addresses(&nodes)
            .into_iter()
            .filter(|a| a.starts_with("board/notes/") && a.matches('/').count() == 2)
            .collect();
        assert_eq!(in_notes.len(), 2, "{:?}", in_notes);

        drag(&service, &in_notes[1], "board/notes", Some(&in_notes[0])).expect("up");
        assert_eq!(order(&on_disk(&root), "notes"), ["alpha", "first"]);

        let nodes = service.open("d.os").expect("open");
        let first_now = overseer::addressing::addresses(&nodes)
            .into_iter()
            .find(|a| a.starts_with("board/notes/") && a.matches('/').count() == 2)
            .expect("an entry");
        drag(&service, &first_now, "board/more", None).expect("into an empty list");
        let text = on_disk(&root);
        assert_eq!(order(&text, "notes"), ["first"]);
        assert_eq!(order(&text, "more"), ["alpha"]);
    });
}
