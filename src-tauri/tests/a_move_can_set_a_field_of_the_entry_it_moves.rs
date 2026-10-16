//! A move can set a field of the entry it moves - a drop on a board's lane.
//!
//! The entry goes where it was dropped and takes the lane's value, and the field's `on change`
//! runs, all as one change: one write, one step to take back. The field is named from the entry,
//! since the page cannot know what the entry is called once it has landed.

use overseer::server::DocumentRoot;
use serde_json::json;

const DOCUMENT: &str = r#"tab t (mutable=true) {

    div (hidden=true) {

        div Tier {
            string tag = ""
        }

        div Food {
            string name = ""
            string said = ""
            enum tier (vocabulary="/t/Tiers") = "" {
                on change {
                    set (path="../said") = $(../tier)
                }
            }
        }
    }

    list Tiers (entry=<Tier>, key="tag", hidden=true) {
        - {
            - tag = "S"
        }
        - {
            - tag = "A"
        }
    }

    list Ranked (entry=<Food>, key="name", view="board", by="tier", movable=true) {
        - {
            - name = "pizza"
            - tier = "S"
        }
        - {
            - name = "ramen"
            - tier = "A"
        }
    }

    list Unranked (entry=<Food>, key="name", movable=true) {
        - {
            - name = "waffles"
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
    let root = std::env::temp_dir().join(format!("overseer_lane_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("d.os"), DOCUMENT).expect("write");
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

fn drop_in(
    service: &DocumentRoot,
    entry: &str,
    to_list: &str,
    before: Option<&str>,
    field: &str,
    value: &str,
) -> Result<serde_json::Value, String> {
    let entry = names(service, entry);
    let to_list = names(service, to_list);
    let before = before.map(|b| names(service, b));
    service
        .command_for(
            "alice",
            Some("d.os"),
            "move_overseer_entry",
            &json!({
                "entry_path": entry,
                "to_list_path": to_list,
                "before": before,
                "setting": { "node_path": [field], "value": { "String": value } },
            }),
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

/// What an entry says, as the lines of the file between its name and the next entry.
fn entry_text(text: &str, name: &str) -> String {
    let start = text.find(&format!("- name = \"{}\"", name)).expect("the entry");
    let body = &text[start..];
    let end = body.find("\n        }").unwrap_or(body.len());
    body[..end].to_string()
}

#[test]
fn an_entry_dropped_in_a_lane_moves_and_takes_its_value_and_runs_its_handler() {
    serialised(|| {
        let root = a_root("across");
        let service = DocumentRoot::new(&root).expect("open the root");

        drop_in(&service, "t/Unranked/[waffles]", "t/Ranked", Some("t/Ranked/[ramen]"), "tier", "S")
            .expect("the drop");
        let text = on_disk(&root);
        assert_eq!(order(&text, "Ranked"), ["pizza", "waffles", "ramen"]);
        assert!(order(&text, "Unranked").is_empty(), "{}", text);
        let waffles = entry_text(&text, "waffles");
        assert!(waffles.contains("- tier = \"S\""), "{}", text);
        assert!(waffles.contains("- said = \"S\""), "the handler did not run: {}", text);
    });
}

#[test]
fn one_undo_takes_back_the_move_the_value_and_the_handler() {
    serialised(|| {
        let root = a_root("undo");
        let service = DocumentRoot::new(&root).expect("open the root");
        let before = on_disk(&root);
        drop_in(&service, "t/Unranked/[waffles]", "t/Ranked", None, "tier", "A").expect("the drop");
        assert_ne!(on_disk(&root), before);
        service.undo("d.os").expect("undo");
        assert_eq!(on_disk(&root), before, "one undo did not take it all back");
    });
}

#[test]
fn a_drop_in_its_own_place_writes_only_the_value() {
    serialised(|| {
        let root = a_root("inplace");
        let service = DocumentRoot::new(&root).expect("open the root");
        drop_in(&service, "t/Ranked/[ramen]", "t/Ranked", None, "tier", "S").expect("the drop");
        let text = on_disk(&root);
        assert_eq!(order(&text, "Ranked"), ["pizza", "ramen"]);
        let ramen = entry_text(&text, "ramen");
        assert!(ramen.contains("- tier = \"S\""), "{}", text);
        assert!(ramen.contains("- said = \"S\""), "{}", text);
    });
}

#[test]
fn a_move_within_the_board_reorders_and_sets() {
    serialised(|| {
        let root = a_root("within");
        let service = DocumentRoot::new(&root).expect("open the root");
        drop_in(&service, "t/Ranked/[ramen]", "t/Ranked", Some("t/Ranked/[pizza]"), "tier", "S")
            .expect("the drop");
        let text = on_disk(&root);
        assert_eq!(order(&text, "Ranked"), ["ramen", "pizza"]);
        assert!(entry_text(&text, "ramen").contains("- tier = \"S\""), "{}", text);
    });
}

#[test]
fn a_field_that_is_not_there_refuses_the_whole_drop() {
    serialised(|| {
        let root = a_root("refused");
        let service = DocumentRoot::new(&root).expect("open the root");
        let before = on_disk(&root);
        assert!(drop_in(&service, "t/Unranked/[waffles]", "t/Ranked", None, "nothing", "S").is_err());
        assert_eq!(on_disk(&root), before, "a refused drop moved something");
    });
}

#[test]
fn a_value_it_holds_already_runs_no_handler() {
    serialised(|| {
        let root = a_root("same");
        let service = DocumentRoot::new(&root).expect("open the root");
        drop_in(&service, "t/Ranked/[ramen]", "t/Ranked", Some("t/Ranked/[pizza]"), "tier", "A")
            .expect("the drop");
        let text = on_disk(&root);
        assert_eq!(order(&text, "Ranked"), ["ramen", "pizza"]);
        assert!(!entry_text(&text, "ramen").contains("- said = "), "{}", text);
    });
}
