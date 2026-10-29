//! `each` runs a block once for every item of a list, and `press` runs another node's `on click`
//! inside the same press - one change to the file, one step to take back.
//!
//! A press that presses back into what it came from is the flood a minesweeper board needs: a
//! cell with nothing around it opens, and presses its closed neighbours. A node already pressed in
//! the press is not pressed again, which is what ends it, and a press nested past the limit fails
//! rather than running out of stack.

use overseer::server::DocumentRoot;
use serde_json::json;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn a_root(tag: &str, document: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_eachpress_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("d.os"), document).expect("write");
    root
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("d.os")).expect("read")
}

fn press(service: &DocumentRoot, address: &str) -> Result<serde_json::Value, String> {
    let nodes = service.open("d.os").expect("open");
    let path = overseer::addressing::name_path(&nodes, address)
        .unwrap_or_else(|| panic!("no such button: {}", address));
    service
        .command_for(
            "someone",
            Some("d.os"),
            "run_overseer_event",
            &json!({ "node_path": path, "event_name": "click" }),
        )
        .map_err(|e| format!("{:?}", e))
}

fn steps_back(root: &std::path::Path) -> usize {
    let (beside, named) = overseer::undo::beside(&root.join("d.os")).expect("an undo folder");
    overseer::undo::depth(&beside, &named)
}

const TASKS: &str = r#"div (hidden=true) {
    div Task {
        string title = ""
        bool urgent = false
        bool done = false
    }
    div Line {
        string said = ""
    }
}

tab tasks (mutable=true) {
    tags names = "ann, bob, cy"

    list Tasks (entry=<Task>, key="title") {
        - {
            - title = "rent"
            - urgent = true
        }
        - {
            - title = "plants"
        }
        - {
            - title = "taxes"
            - urgent = true
        }
    }

    list Log (entry=<Line>) {
    }

    button tick_urgent (label="Tick urgent") {
        on click {
            each (in=$(../Tasks.filter(|x| x/urgent)), as="t") {
                set (path="t/done") = true
            }
        }
    }

    button greet (label="Greet") {
        on click {
            each (in=$(../names), as="n") {
                append (list="../Log") {
                    - said = $("hello " + n)
                }
            }
        }
    }
}
"#;

#[test]
fn each_sets_a_field_of_every_entry_the_filter_keeps() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("entries", TASKS);
    let service = DocumentRoot::new(&root).expect("open the root");
    press(&service, "tasks/tick_urgent").expect("the press was refused");

    let nodes = service.open("d.os").expect("open");
    let done = |title: &str| {
        let node = overseer::addressing::find(&nodes, &format!("tasks/Tasks/[{}]/done", title)).expect("done");
        node.parameters.get("value").cloned()
    };
    assert_eq!(done("rent"), Some(overseer::types::OverseerValue::Boolean(true)));
    assert_eq!(done("taxes"), Some(overseer::types::OverseerValue::Boolean(true)));
    assert_ne!(done("plants"), Some(overseer::types::OverseerValue::Boolean(true)), "an entry the filter left out was ticked");
    assert_eq!(steps_back(&root), 1, "one press, one step to take back");
}

#[test]
fn each_goes_through_plain_values_in_order() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("values", TASKS);
    let service = DocumentRoot::new(&root).expect("open the root");
    press(&service, "tasks/greet").expect("the press was refused");

    let text = on_disk(&root);
    let ann = text.find("hello ann").expect("no line for ann");
    let bob = text.find("hello bob").expect("no line for bob");
    let cy = text.find("hello cy").expect("no line for cy");
    assert!(ann < bob && bob < cy, "the values were gone through out of order:\n{}", text);
}

const DONE: &str = r#"div (hidden=true) {
    div Task {
        string title = ""
        bool done = false
    }
}

tab tasks (mutable=true) {
    list Tasks (entry=<Task>, key="title") {
        - {
            - title = "rent"
            - done = true
        }
        - {
            - title = "plants"
        }
        - {
            - title = "taxes"
            - done = true
        }
        - {
            - title = "post"
            - done = true
        }
    }

    button clear_done (label="Clear done") {
        on click {
            each (in=$(../Tasks.filter(|x| x/done)), as="t") {
                remove (from="../Tasks", keyValue=$(t/title))
            }
        }
    }
}
"#;

#[test]
fn each_finds_its_entries_again_after_a_round_takes_one_away() {
    // Every round moves the entries after the one it removes; the next round still reaches the
    // entry it was given, by its key.
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("remove", DONE);
    let service = DocumentRoot::new(&root).expect("open the root");
    press(&service, "tasks/clear_done").expect("the press was refused");

    let text = on_disk(&root);
    assert!(text.contains("\"plants\""), "the entry that was not done went:
{}", text);
    for gone in ["rent", "taxes", "post"] {
        assert!(!text.contains(&format!("\"{}\"", gone)), "{} was left:
{}", gone, text);
    }
    assert_eq!(steps_back(&root), 1);
}

const BUTTONS: &str = r#"tab b (mutable=true) {
    int count = 0
    int seen = 0

    button add (label="Add") {
        on click {
            inc (path="../count", by=1)
        }
    }

    button twice (label="Twice") {
        on click {
            press (path="../add")
            set (path="../seen") = $(../count)
        }
    }

    button leaky (label="Leaky") {
        on click {
            each (in=$(../names), as="add") {
                press (path="../add")
            }
        }
    }

    tags names = "one"

    button nowhere (label="Nowhere") {
        on click {
            press (path="../count")
        }
    }
}
"#;

#[test]
fn press_runs_another_buttons_click_as_one_step() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("press", BUTTONS);
    let service = DocumentRoot::new(&root).expect("open the root");
    press(&service, "b/twice").expect("the press was refused");

    let text = on_disk(&root);
    assert!(text.contains("int count = 1"), "the pressed button's handler did not run:\n{}", text);
    assert!(text.contains("int seen = 1"), "what the pressed handler wrote was not there to read after:\n{}", text);
    assert_eq!(steps_back(&root), 1, "the press and what it pressed were more than one step");

    service.undo("d.os").expect("undo");
    assert_eq!(on_disk(&root), BUTTONS, "one undo did not take back all of it");
}

#[test]
fn a_bound_name_is_a_value_when_the_item_is_one() {
    // `add` names a value here, not a node, so `../add` is the button beside it as always.
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("value_name", BUTTONS);
    let service = DocumentRoot::new(&root).expect("open the root");
    press(&service, "b/leaky").expect("the press was refused");
    assert!(on_disk(&root).contains("int count = 1"));
}

#[test]
fn pressing_what_has_no_click_is_refused() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("nowhere", BUTTONS);
    let service = DocumentRoot::new(&root).expect("open the root");
    let refused = press(&service, "b/nowhere").expect_err("a press of a field was run");
    assert!(refused.contains("no on click"), "the refusal does not say why: {}", refused);
    assert_eq!(on_disk(&root), BUTTONS);
}

/// A row of cells, each of which opens and presses the cells beside it - the flood.
fn a_row(cells: usize) -> String {
    let mut entries = String::new();
    for at in 0..cells {
        entries.push_str(&format!("        - {{\n            - id = \"c{}\"\n            - pos = {}\n        }}\n", at, at));
    }
    format!(
        r#"div (hidden=true) {{
    div Cell {{
        string id = ""
        int pos = 0
        bool opened = false

        button open (label="Open") {{
            on click {{
                set (path="../opened") = true
                inc (path="/board/presses", by=1)
                each (in=$(/board/Cells.filter(|x| x/pos == ../pos + 1 || x/pos == ../pos - 1)), as="c") {{
                    press (path="c/open")
                }}
            }}
        }}
    }}
}}

tab board (mutable=true) {{
    int presses = 0

    list Cells (entry=<Cell>, key="id") {{
{}    }}
}}
"#,
        entries
    )
}

#[test]
fn a_flood_presses_each_cell_once_and_stops() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let row = a_row(12);
    let root = a_root("flood", &row);
    let service = DocumentRoot::new(&root).expect("open the root");
    press(&service, "board/Cells/[c5]/open").expect("the flood was refused");

    let nodes = service.open("d.os").expect("open");
    for at in 0..12 {
        let opened = overseer::addressing::find(&nodes, &format!("board/Cells/[c{}]/opened", at))
            .and_then(|n| n.parameters.get("value").cloned());
        assert_eq!(opened, Some(overseer::types::OverseerValue::Boolean(true)), "cell {} was not opened", at);
    }
    assert!(on_disk(&root).contains("int presses = 12"), "a cell was pressed more than once:\n{}", on_disk(&root));
    assert_eq!(steps_back(&root), 1, "the flood was more than one step");
}

#[test]
fn a_press_too_deep_fails_and_writes_nothing() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let row = a_row(overseer::actions::PRESS_DEPTH_LIMIT + 20);
    let root = a_root("deep", &row);
    let service = DocumentRoot::new(&root).expect("open the root");
    // Run on a thread with the stack a server's request thread has, which is what has to hold it.
    let refused = std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || press(&service, "board/Cells/[c0]/open"))
        .expect("a thread")
        .join()
        .expect("the press ran out of stack")
        .expect_err("a press past the limit was run");
    assert!(refused.contains("deep"), "the refusal does not say why: {}", refused);
    assert_eq!(on_disk(&root), row, "a refused flood wrote the file");
}

const LEAK: &str = r#"div (hidden=true) {
    div Task {
        string title = ""
        bool done = false
    }
}

tab tasks (mutable=true) {
    list Tasks (entry=<Task>, key="title") {
        - {
            - title = "rent"
        }
    }

    button mark (label="Mark") {
        on click {
            set (path="t/done") = true
        }
    }

    button mark_all (label="Mark all") {
        on click {
            each (in=$(../Tasks), as="t") {
                press (path="../mark")
            }
        }
    }
}
"#;

#[test]
fn a_name_each_gives_does_not_reach_a_pressed_nodes_handler() {
    // `mark` says nothing of `t`: it is read where it is written, not where it was pressed from.
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("leak", LEAK);
    let service = DocumentRoot::new(&root).expect("open the root");
    let refused = press(&service, "tasks/mark_all").expect_err("the pressed handler found t");
    assert!(refused.contains("t/done"), "refused for another reason: {}", refused);
    assert_eq!(on_disk(&root), LEAK);
}
