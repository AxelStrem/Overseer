//! An entry an action adds is worked out as a fresh open of the file would work it out.
//!
//! A template that declares a context has a worked-out copy of it put in front of each entry's
//! children - see `contexts`. A whole work-out puts it there; following the change of shape after
//! a press that made an entry did not, so the same text was held in two shapes: the new entry
//! without its copies where the press had left it, and with them wherever the document was worked
//! out again. The page applies a change by the index of the node it is about, so a change
//! described against one shape and applied to the other landed two places along. A project
//! item's first comment turned its Talk button into one labelled "moved", and a press on Talk
//! folded nothing.

use overseer::app_api;
use overseer::types::*;

/// The entries' template, in a file of its own as a project tracker's is in the library: a list
/// naming a template in another file is given it as that file was read, where nothing worked its
/// contexts out.
const ROWS: &str = r##"div (hidden=true) {
    div Row (layout="horizontal") {
        context hover-text = true
        context theme {
            - text = $(qty > 2 ? "#dc2626" : "inherit")
        }
        string id = ""
        int qty = 0
        int twice = $(qty * 2)
        button more (label="more") {
            on click {
                set (path="../qty") = 9
            }
        }
    }
}
"##;

const DOCUMENT: &str = r##"tab t (mutable=true) {
    list Rows (entry=<rows.os/Row>, key="id") {
        - {
            - id = "b"
            - qty = 1
        }
    }
    button first (label="first") {
        on click {
            prepend (list="/t/Rows") {
                - id = "a"
                - qty = 3
            }
        }
    }
    div Form (layout="horizontal") {
        string id = "f"
        int qty = 6
        button add (label="add") {
            on click {
                prepend (list="/t/Rows", from="..") {
                    - qty = 7
                }
            }
        }
    }
    button last (label="last") {
        on click {
            append (list="/t/Rows") {
                - id = "c"
                - qty = 4
            }
        }
    }
}
"##;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

/// A file holding the document, and the document as the page holds it once opened.
fn opened(tag: &str) -> (String, Vec<OverseerNode>) {
    let root = std::env::temp_dir().join(format!("overseer_entry_as_opened_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("d.os");
    std::fs::write(&file, DOCUMENT).unwrap();
    std::fs::write(root.join("rows.os"), ROWS).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let nodes = beside(&root, || app_api::load_document(DOCUMENT.to_string()).expect("open"));
    (file.to_string_lossy().to_string(), nodes)
}

/// Worked out as a document in this folder is, so the template's file is found beside it.
fn beside<T>(folder: &std::path::Path, body: impl FnOnce() -> T) -> T {
    overseer::docmgr::manager::DocumentManager::with_document(Some(folder.to_path_buf()), body)
}

/// Take an answer on, the way the page does.
fn take(page: &mut Vec<OverseerNode>, answer: app_api::ResolvedUpdate) {
    if let Some(whole) = answer.nodes {
        *page = whole;
        return;
    }
    overseer::delta::apply(page, answer.changes.expect("an answer describes something"));
}

fn name_path(page: &[OverseerNode], address: &str) -> Vec<String> {
    overseer::addressing::name_path(page, address).unwrap_or_else(|| panic!("nothing at {}", address))
}

fn press(path: &str, page: &mut Vec<OverseerNode>, address: &str) {
    let button = name_path(page, address);
    let folder = std::path::Path::new(path).parent().unwrap();
    let answer = beside(folder, || app_api::run_event_at(path, "d.os", button, "click".into(), "s", Vec::new(), Vec::new()));
    take(page, answer.expect("the press"));
}

/// An entry's children in order, each by type, name and whether it is a worked-out copy, with
/// what was worked out on it and on what is inside it: what the page goes by when it applies a
/// change by index.
fn shape(page: &[OverseerNode], entry: &str) -> Vec<String> {
    fn described(node: &OverseerNode, out: &mut Vec<String>, depth: usize) {
        let mut line = format!(
            "{}{} {}{}",
            "  ".repeat(depth),
            node.node_type,
            node.name,
            if overseer::contexts::is_copy(node) { " (copy)" } else { "" }
        );
        for (k, v) in &node.parameters {
            if k.starts_with("_computed_") {
                line.push_str(&format!(" {}={:?}", k, v));
            }
        }
        out.push(line);
        for child in &node.children {
            described(child, out, depth + 1);
        }
    }
    let node = overseer::addressing::find(page, &format!("t/Rows/[{}]", entry))
        .unwrap_or_else(|| panic!("no entry {}", entry));
    let mut out = Vec::new();
    for child in &node.children {
        described(child, &mut out, 0);
    }
    out
}

/// The entry as a fresh open of the file has it. Last in a case: it lets go of what is held.
fn as_a_fresh_open(path: &str, entry: &str) -> Vec<String> {
    app_api::forget_baseline();
    app_api::forget_dependencies();
    let folder = std::path::Path::new(path).parent().unwrap();
    let fresh = beside(folder, || app_api::load_document(std::fs::read_to_string(path).unwrap()).expect("a fresh open"));
    shape(&fresh, entry)
}

#[test]
fn an_entry_a_press_puts_in_front_carries_the_copies_of_its_contexts() {
    serialised(|| {
        let (path, mut page) = opened("first");
        press(&path, &mut page, "t/first");
        let held = shape(&page, "a");
        assert!(
            held.iter().any(|line| line.starts_with("context theme (copy)")),
            "the new entry has no copy of its theme:\n{}",
            held.join("\n")
        );
        assert_eq!(held, as_a_fresh_open(&path, "a"));
    });
}

#[test]
fn an_entry_a_press_puts_last_carries_the_copies_of_its_contexts() {
    serialised(|| {
        let (path, mut page) = opened("last");
        press(&path, &mut page, "t/last");
        assert_eq!(shape(&page, "c"), as_a_fresh_open(&path, "c"));
    });
}

#[test]
fn an_entry_a_press_fills_from_a_form_carries_the_copies_of_its_contexts() {
    serialised(|| {
        let (path, mut page) = opened("form");
        press(&path, &mut page, "t/Form/add");
        let held = shape(&page, "f");
        assert!(
            held.iter().any(|line| line.starts_with("context theme (copy)")),
            "the new entry has no copy of its theme:
{}",
            held.join("
")
        );
        assert_eq!(held, as_a_fresh_open(&path, "f"));
    });
}

#[test]
fn a_press_inside_a_new_entry_reaches_the_field_it_names() {
    serialised(|| {
        let (path, mut page) = opened("inside");
        press(&path, &mut page, "t/first");
        press(&path, &mut page, "t/Rows/[a]/more");
        let held = shape(&page, "a");
        assert_eq!(held, as_a_fresh_open(&path, "a"));
        // The change for `qty` landed on `qty`, not on a node two places along.
        assert!(held.iter().any(|line| line.starts_with("int twice") && line.contains("Integer(18)")), "{}", held.join("\n"));
    });
}

#[test]
fn an_entry_appended_from_outside_carries_the_copies_of_its_contexts() {
    serialised(|| {
        let (path, mut page) = opened("outside");
        let list = name_path(&page, "t/Rows");
        let mut fields = std::collections::HashMap::new();
        fields.insert("id".to_string(), OverseerValue::String("d".into()));
        fields.insert("qty".to_string(), OverseerValue::Integer(5));
        let folder = std::path::Path::new(&path).parent().unwrap().to_path_buf();
        let answer = beside(&folder, || app_api::append_entry_at(&path, "d.os", list, fields, "s"));
        take(&mut page, answer.expect("the append"));
        assert_eq!(shape(&page, "d"), as_a_fresh_open(&path, "d"));
    });
}
