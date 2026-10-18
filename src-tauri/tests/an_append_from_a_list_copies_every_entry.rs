//! `append` or `prepend` with `from` naming a list copies every entry of it.
//!
//! Each entry is copied as a copy from that one entry would be - made from the target list's
//! template, filled by name, its own lists coming along - and the action's block applies to every
//! one. Putting the copies in place of what the list held is `clear_list` before the append, in
//! the same press, which one undo takes back.

use overseer::app_api;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::types::*;

const DOCUMENT: &str = r##"tab shelf (label="Shelf", mutable=true) {

    div (hidden=true) {
        div Book (layout="horizontal") {
            string title = ""
            int pages = 0
            int left = $(pages - 10)
            list Notes (entry=string) { }
        }

        div Copy (layout="horizontal") {
            string title = ""
            int pages = 0
            int left = 0
            string from = ""
            list Notes (entry=string) { }
        }
    }

    list Books (entry=<Book>, key="title") {
        - {
            - title = "Dune"
            - pages = 412
            list Notes {
                - "sand"
                - "spice"
            }
        }
        - {
            - title = "Emma"
            - pages = 300
        }
    }

    list Archive (entry=<Copy>, key="title") {
        - {
            - title = "Old"
        }
    }

    list Words (entry=string) {
        - "one"
        - "two"
    }

    list Echo (entry=string) {
        - "zero"
    }

    button append_all {
        on click {
            append (list="../Archive", from="../Books") {
                - from = "shelf"
            }
        }
    }

    button prepend_all {
        on click {
            prepend (list="../Archive", from="../Books")
        }
    }

    button replace_all {
        on click {
            clear_list (path="../Archive")
            append (list="../Archive", from="../Books")
        }
    }

    button again {
        on click {
            append (list="../Books", from="../Books")
        }
    }

    button words {
        on click {
            append (list="../Echo", from="../Words")
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

fn a_document(tag: &str) -> String {
    let root = std::env::temp_dir().join(format!("overseer_fromalist_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("s.os");
    std::fs::write(&file, DOCUMENT).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    FormulaEvaluator::set_time_override(None);
    file.to_string_lossy().to_string()
}

fn on_disk(file: &str) -> String {
    std::fs::read_to_string(file).unwrap()
}

fn press(file: &str, button: &str) -> std::result::Result<app_api::ResolvedUpdate, String> {
    let nodes = app_api::load_document(on_disk(file)).expect("open");
    let path = overseer::addressing::name_path(&nodes, &format!("shelf/{}", button))
        .unwrap_or_else(|| panic!("no button {}", button));
    app_api::run_event_at(file, "s.os", path, "click".into(), "s", Vec::new(), Vec::new())
        .map_err(|e| format!("{:?}", e))
}

fn read(entry: &OverseerNode, name: &str) -> Option<OverseerValue> {
    let field = entry.children.iter().find(|c| c.name == name)?;
    field.parameters.get("_computed_value").or_else(|| field.parameters.get("value")).cloned()
}

fn text_of(value: Option<OverseerValue>) -> String {
    match value {
        Some(OverseerValue::String(s)) => s,
        other => panic!("reads {:?}", other),
    }
}

fn titles(file: &str, list: &str) -> Vec<String> {
    let nodes = app_api::load_document(on_disk(file)).expect("open");
    let list = overseer::addressing::find(&nodes, list).unwrap_or_else(|| panic!("no {}", list));
    list.children.iter().map(|e| text_of(read(e, "title"))).collect()
}

fn value_at(file: &str, address: &str) -> Option<OverseerValue> {
    let nodes = app_api::load_document(on_disk(file)).expect("open");
    let node = overseer::addressing::find(&nodes, address).unwrap_or_else(|| panic!("nothing at {}", address));
    node.parameters.get("_computed_value").or_else(|| node.parameters.get("value")).cloned()
}

fn plain_values(file: &str, list: &str) -> Vec<String> {
    let nodes = app_api::load_document(on_disk(file)).expect("open");
    let list = overseer::addressing::find(&nodes, list).unwrap_or_else(|| panic!("no {}", list));
    list.children.iter().map(|e| text_of(e.parameters.get("value").cloned())).collect()
}

#[test]
fn an_append_from_a_list_puts_every_entry_after_what_was_there() {
    serialised(|| {
        let file = a_document("append");
        assert!(press(&file, "append_all").unwrap().wrote);
        assert_eq!(titles(&file, "shelf/Archive"), ["Old", "Dune", "Emma"], "{}", on_disk(&file));
        assert_eq!(value_at(&file, "shelf/Archive/[Dune]/pages"), Some(OverseerValue::Integer(412)));
        // Worked out on the source, a plain number on the copy.
        assert_eq!(value_at(&file, "shelf/Archive/[Dune]/left"), Some(OverseerValue::Integer(402)));
        assert_eq!(text_of(value_at(&file, "shelf/Archive/[Emma]/from")), "shelf", "the block applies to every copy");
        assert_eq!(text_of(value_at(&file, "shelf/Archive/[Dune]/from")), "shelf");
        assert_eq!(plain_values(&file, "shelf/Archive/[Dune]/Notes"), ["sand", "spice"], "{}", on_disk(&file));
        assert_eq!(titles(&file, "shelf/Books"), ["Dune", "Emma"], "the source is left as it was");
    });
}

#[test]
fn a_prepend_from_a_list_keeps_its_order_at_the_front() {
    serialised(|| {
        let file = a_document("prepend");
        assert!(press(&file, "prepend_all").unwrap().wrote);
        assert_eq!(titles(&file, "shelf/Archive"), ["Dune", "Emma", "Old"], "{}", on_disk(&file));
    });
}

#[test]
fn a_clear_before_the_append_replaces_the_list_and_one_undo_takes_both_back() {
    serialised(|| {
        let file = a_document("replace");
        let before = on_disk(&file);
        assert!(press(&file, "replace_all").unwrap().wrote);
        assert_eq!(titles(&file, "shelf/Archive"), ["Dune", "Emma"], "{}", on_disk(&file));
        app_api::undo_document(&file).expect("undo");
        assert_eq!(on_disk(&file), before);
    });
}

#[test]
fn a_key_the_target_already_holds_refuses_the_press() {
    serialised(|| {
        let file = a_document("taken");
        let before = on_disk(&file);
        assert!(press(&file, "again").is_err(), "copying a keyed list into itself took its keys twice");
        assert_eq!(on_disk(&file), before, "a refused press wrote something");
    });
}

#[test]
fn a_list_of_plain_values_copies_the_values() {
    serialised(|| {
        let file = a_document("words");
        assert!(press(&file, "words").unwrap().wrote);
        assert_eq!(plain_values(&file, "shelf/Echo"), ["zero", "one", "two"], "{}", on_disk(&file));
    });
}
