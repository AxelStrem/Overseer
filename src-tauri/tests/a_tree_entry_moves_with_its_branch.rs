//! An entry of a tree moves with its branch: among its siblings when the tree says
//! `movable=true`, and to any level of it when it says `movable="anywhere"`.
//!
//! In a tree whose entries hold their children, the move is one between the tree's lists, which
//! say nothing of moving themselves: the root list speaks for them. In a tree by a parent field,
//! the move writes the parent and the entry's place in the file as one change. Either way an
//! entry never goes under itself.

use overseer::server::DocumentRoot;
use serde_json::json;

fn document(movable: &str) -> String {
    format!(
        r#"div (hidden=true) {{
    div Part {{
        string name = ""
        list Parts (entry=<Part>, key="name") {{ }}
    }}

    div Task {{
        string handle = ""
        string said = ""
        string parent = "" {{
            on change {{
                set (path="../said") = $(../parent)
            }}
        }}
    }}
}}

tab t (mutable=true) {{
    list Parts (entry=<Part>, key="name", tree="Parts", movable={m}) {{
        - {{
            - name = "Car"
            list Parts {{
                - {{
                    - name = "Engine"
                    list Parts {{
                        - {{
                            - name = "Pistons"
                        }}
                    }}
                }}
                - {{
                    - name = "Wheels"
                }}
            }}
        }}
        - {{
            - name = "Bicycle"
        }}
    }}

    list Tasks (entry=<Task>, key="handle", tree="parent", movable={m}) {{
        - {{
            - handle = "site"
        }}
        - {{
            - handle = "pages"
            - parent = "site"
        }}
        - {{
            - handle = "about"
            - parent = "pages"
        }}
        - {{
            - handle = "deploy"
            - parent = "site"
        }}
        - {{
            - handle = "launch"
        }}
    }}
}}
"#,
        m = movable
    )
}

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

fn a_root(tag: &str, movable: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_tree_move_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("d.os"), document(movable)).expect("write");
    overseer::viewstate::forget_all();
    root
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("d.os")).expect("read")
}

fn names(service: &DocumentRoot, address: &str) -> Vec<String> {
    let nodes = service.open("d.os").expect("open");
    overseer::addressing::name_path(&nodes, address)
        .unwrap_or_else(|| panic!("no such address: {}", address))
}

fn move_to(
    service: &DocumentRoot,
    entry: &str,
    to_list: &str,
    before: Option<&str>,
    parent: Option<&str>,
) -> Result<serde_json::Value, String> {
    let entry = names(service, entry);
    let to_list = names(service, to_list);
    let before = before.map(|b| names(service, b));
    let mut args = json!({
        "entry_path": entry,
        "to_list_path": to_list,
        "before": before,
    });
    if let Some(parent) = parent {
        args["setting"] = json!({ "node_path": ["parent"], "value": { "String": parent } });
    }
    service
        .command_for("alice", Some("d.os"), "move_overseer_entry", &args)
        .map_err(|e| format!("{:?}", e))
}

/// The handles of the tasks, in the order the file has them.
fn tasks(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| l.trim().strip_prefix("- handle = \""))
        .map(|n| n.trim_end_matches('"').to_string())
        .collect()
}

/// What a task says, as the lines of the file between its handle and the end of its entry.
fn task_text(text: &str, handle: &str) -> String {
    let start = text.find(&format!("- handle = \"{}\"", handle)).expect("the task");
    let body = &text[start..];
    let end = body.find("\n        }").unwrap_or(body.len());
    body[..end].to_string()
}

#[test]
fn a_nested_entry_goes_to_another_level_with_its_children_when_the_tree_says_anywhere() {
    serialised(|| {
        let root = a_root("nested", "\"anywhere\"");
        let service = DocumentRoot::new(&root).expect("open the root");
        move_to(&service, "t/Parts/[Car]/Parts/[Engine]", "t/Parts/[Bicycle]/Parts", None, None)
            .expect("the move");
        let nodes = service.open("d.os").expect("open");
        assert!(
            overseer::addressing::name_path(&nodes, "t/Parts/[Bicycle]/Parts/[Engine]/Parts/[Pistons]").is_some(),
            "the branch did not go with it: {}",
            on_disk(&root)
        );
        assert!(overseer::addressing::name_path(&nodes, "t/Parts/[Car]/Parts/[Engine]").is_none());
        // And back up to the top, before the first.
        move_to(&service, "t/Parts/[Bicycle]/Parts/[Engine]", "t/Parts", Some("t/Parts/[Car]"), None)
            .expect("the move up");
        let nodes = service.open("d.os").expect("open");
        assert!(overseer::addressing::name_path(&nodes, "t/Parts/[Engine]/Parts/[Pistons]").is_some());
        let text = on_disk(&root);
        assert!(text.find("- name = \"Engine\"").unwrap() < text.find("- name = \"Car\"").unwrap(), "{}", text);
    });
}

#[test]
fn a_nested_entry_cannot_go_under_itself() {
    serialised(|| {
        let root = a_root("nested_self", "\"anywhere\"");
        let service = DocumentRoot::new(&root).expect("open the root");
        let before = on_disk(&root);
        let refused = move_to(&service, "t/Parts/[Car]", "t/Parts/[Car]/Parts/[Engine]/Parts", None, None);
        assert!(refused.is_err(), "it went under itself");
        assert_eq!(on_disk(&root), before);
    });
}

#[test]
fn a_nested_entry_moves_only_among_its_siblings_when_the_tree_says_true() {
    serialised(|| {
        let root = a_root("nested_true", "true");
        let service = DocumentRoot::new(&root).expect("open the root");
        let before = on_disk(&root);
        let refused = move_to(&service, "t/Parts/[Car]/Parts/[Engine]", "t/Parts/[Bicycle]/Parts", None, None);
        assert!(refused.is_err(), "it changed level under movable=true");
        assert_eq!(on_disk(&root), before);
        move_to(&service, "t/Parts/[Car]/Parts/[Wheels]", "t/Parts/[Car]/Parts", Some("t/Parts/[Car]/Parts/[Engine]"), None)
            .expect("a move among its siblings");
        let text = on_disk(&root);
        assert!(text.find("- name = \"Wheels\"").unwrap() < text.find("- name = \"Engine\"").unwrap(), "{}", text);
    });
}

#[test]
fn a_flat_entry_takes_its_new_parent_and_place_as_one_change() {
    serialised(|| {
        let root = a_root("flat", "\"anywhere\"");
        let service = DocumentRoot::new(&root).expect("open the root");
        let before = on_disk(&root);
        move_to(&service, "t/Tasks/[pages]", "t/Tasks", Some("t/Tasks/[launch]"), Some(""))
            .expect("the move");
        let text = on_disk(&root);
        assert_eq!(tasks(&text), ["site", "about", "deploy", "pages", "launch"]);
        let pages = task_text(&text, "pages");
        assert!(!pages.contains("- parent = \"site\""), "the parent was not cleared: {}", text);
        // Its child stays where it is in the file, and is still its child.
        assert!(task_text(&text, "about").contains("- parent = \"pages\""), "{}", text);
        service.undo("d.os").expect("undo");
        assert_eq!(on_disk(&root), before, "one undo did not take it all back");
    });
}

#[test]
fn a_flat_move_runs_the_parent_fields_handler() {
    serialised(|| {
        let root = a_root("flat_handler", "\"anywhere\"");
        let service = DocumentRoot::new(&root).expect("open the root");
        move_to(&service, "t/Tasks/[launch]", "t/Tasks", None, Some("deploy")).expect("the move");
        let launch = task_text(&on_disk(&root), "launch");
        assert!(launch.contains("- parent = \"deploy\""), "{}", launch);
        assert!(launch.contains("- said = \"deploy\""), "the handler did not run: {}", launch);
    });
}

#[test]
fn a_flat_entry_cannot_go_under_itself_or_its_branch() {
    serialised(|| {
        let root = a_root("flat_self", "\"anywhere\"");
        let service = DocumentRoot::new(&root).expect("open the root");
        let before = on_disk(&root);
        for parent in ["site", "pages", "about"] {
            let refused = move_to(&service, "t/Tasks/[site]", "t/Tasks", None, Some(parent));
            assert!(refused.is_err(), "site went under {}", parent);
        }
        assert!(move_to(&service, "t/Tasks/[launch]", "t/Tasks", None, Some("nobody")).is_err());
        assert_eq!(on_disk(&root), before, "a refused move wrote something");
    });
}

#[test]
fn a_flat_entry_keeps_its_parent_when_the_tree_says_true() {
    serialised(|| {
        let root = a_root("flat_true", "true");
        let service = DocumentRoot::new(&root).expect("open the root");
        let before = on_disk(&root);
        assert!(move_to(&service, "t/Tasks/[deploy]", "t/Tasks", None, Some("")).is_err());
        assert_eq!(on_disk(&root), before);
        move_to(&service, "t/Tasks/[deploy]", "t/Tasks", Some("t/Tasks/[pages]"), Some("site"))
            .expect("a move among its siblings");
        assert_eq!(tasks(&on_disk(&root)), ["site", "deploy", "pages", "about", "launch"]);
    });
}
