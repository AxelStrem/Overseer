//! An entry moved to another list takes the colour of where it goes, not of where it was.
//!
//! A style said on a container is handed to everything inside it, and that used to include what a
//! button runs. So the line `- title = $(../title)` in a done button's `append` held the colour of
//! the task the button sat in, worked out there - red, for a task done while overdue - and the
//! append gave it to the new entry's title along with the value. A field that already holds a style
//! keeps it, so the history entry stayed red field by field, while its own box took the history's
//! colour: in tasks.os, in exercise.os and wherever else a coloured entry is moved to another list.
//!
//! Only the document held in memory had it - the file never did, since a style handed down is not
//! written - so it lasted until the server worked the document out again from its text.

use overseer::app_api;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::types::*;

const DOCUMENT: &str = r##"tab t (label="T", mutable=true) {
    div (hidden=true) {
        div Task (layout="vertical", background-color=$(late ? "#5a1414" : "inherit")) {
            string key (hidden=true) = ""
            bool late (hidden=true) = false
            div (layout="horizontal") {
                string title = ""
                button done (label="done") {
                    on click {
                        append (list="/t/Done") {
                            - title = $(../title)
                        }
                        remove (from="/t/Open", keyField="key", keyValue=$(../key))
                    }
                }
            }
        }
        div Record (layout="vertical") {
            div (layout="horizontal") {
                string title = ""
            }
        }
    }

    list Open (entry=<Task>, key="key") {
        - {
            - key = "a"
            - late = true
            - title = "Clean the sink"
        }
    }

    list Done (entry=<Record>) {
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

fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    nodes.iter().find_map(|n| if n.name == name { Some(n) } else { find(&n.children, name) })
}

/// Every node under `node` holding a background, by name.
fn coloured(node: &OverseerNode) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(node: &OverseerNode, out: &mut Vec<String>) {
        if let Some(colour) = node.parameters.get("background-color") {
            out.push(format!("{} = {:?}", node.name, colour));
        }
        for child in &node.children {
            walk(child, out);
        }
    }
    for child in &node.children {
        walk(child, &mut out);
    }
    out
}

/// Opened the way the page opens it, then `done` pressed on the late task.
fn moved(tag: &str) -> (String, Vec<OverseerNode>) {
    let root = std::env::temp_dir().join(format!("overseer_moved_colour_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("d.os");
    std::fs::write(&file, DOCUMENT).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    FormulaEvaluator::set_time_override(None);
    let path = file.to_string_lossy().to_string();
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");

    let open = find(&nodes, "Open").expect("no Open list");
    let task = &open.children[0];
    assert!(
        !coloured(task).is_empty(),
        "the late task is not coloured to begin with, so this shows nothing"
    );
    let button = overseer::addressing::name_path(&nodes, "t/Open/[a]/done").expect("the button");
    app_api::run_event_at(&path, "d.os", button, "click".into(), "s", Vec::new()).expect("the press");
    let written = std::fs::read_to_string(&file).unwrap();
    // The document as the server holds it after the press, which is what the page is shown - not
    // worked out again from the file, which never held the colour.
    let held = app_api::load_document(written.clone()).expect("reopen");
    (written, held)
}

#[test]
fn the_moved_entry_holds_no_colour_of_the_list_it_left() {
    serialised(|| {
        let (written, held) = moved("held");
        let done = find(&held, "Done").expect("no Done list");
        assert_eq!(done.children.len(), 1, "nothing was moved:\n{}", written);
        let entry = &done.children[0];
        assert_eq!(
            find(&entry.children, "title").and_then(|t| t.parameters.get("value")),
            Some(&OverseerValue::String("Clean the sink".into())),
            "the value did not move with it"
        );
        assert_eq!(coloured(entry), Vec::<String>::new(), "the history entry took the late task's colour");
    });
}

#[test]
fn what_a_press_runs_is_handed_no_style() {
    // The cause rather than the symptom: nothing inside an `on` block is drawn, so nothing there
    // is given a style to carry somewhere else.
    serialised(|| {
        app_api::forget_baseline();
        let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
        let open = find(&nodes, "Open").expect("no Open list");
        let click = find(&open.children[0].children, "click").expect("no on click");
        let mut handed = Vec::new();
        fn walk(node: &OverseerNode, handed: &mut Vec<String>) {
            for key in overseer::resolver::INHERITABLE_PARAMS {
                if node.parameters.contains_key(key) {
                    handed.push(format!("{} {}", node.name, key));
                }
            }
            for child in &node.children {
                walk(child, handed);
            }
        }
        walk(click, &mut handed);
        assert_eq!(handed, Vec::<String>::new());
        // And a field beside the button is still handed the task's colour, as it should be.
        let title = find(&open.children[0].children, "title").expect("no title");
        assert!(title.parameters.contains_key("background-color"), "inheritance stopped altogether");
    });
}
