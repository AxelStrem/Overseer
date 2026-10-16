//! An entry can say it stays where it is: `movable=false` on the entry itself, usually from its
//! template and often a formula, such as a pinned item's. Its list may be movable; that entry is
//! not, while its neighbours still are.

use overseer::app_api;

const DOCUMENT: &str = r##"tab t (label="T", mutable=true) {

    div Item (hidden=true, movable=$(pinned == false)) {
        string name = ""
        bool pinned = false
    }

    list Items (entry=<Item>, key="name", movable=true) {
        - {
            - name = "kept"
            - pinned = true
        }
        - {
            - name = "free"
        }
        - {
            - name = "last"
        }
    }
}
"##;

fn opened(tag: &str) -> String {
    let root = std::env::temp_dir().join(format!("overseer_entry_stays_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("t.os");
    std::fs::write(&file, DOCUMENT).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    file.to_string_lossy().to_string()
}

fn order(path: &str) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter_map(|l| l.trim().strip_prefix("- name = \"").map(|r| r.trim_end_matches('"').to_string()))
        .collect()
}

fn moved(path: &str, entry: &str) -> overseer::types::Result<app_api::ResolvedUpdate> {
    let nodes = app_api::load_document(std::fs::read_to_string(path).unwrap()).unwrap();
    let entry = overseer::addressing::name_path(&nodes, &format!("t/Items/[{}]", entry)).expect("the entry");
    let list = overseer::addressing::name_path(&nodes, "t/Items").expect("the list");
    app_api::move_entry_at(path, "t.os", entry, list, None, None, "s")
}

#[test]
fn an_entry_that_says_movable_false_is_not_moved() {
    let path = opened("kept");
    let refused = moved(&path, "kept").err().expect("a pinned entry was moved");
    assert!(format!("{:?}", refused).contains("movable=false"), "{:?}", refused);
    assert_eq!(order(&path), ["kept", "free", "last"]);
}

#[test]
fn its_neighbours_still_move() {
    let path = opened("free");
    moved(&path, "free").expect("the move");
    assert_eq!(order(&path), ["kept", "last", "free"]);
}
