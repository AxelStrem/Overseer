//! A `mark` draws something - a grip an entry is dragged by - and is nothing else.
//!
//! It is unnamed and holds no value, so like an unnamed div it is no step of any address: the
//! field beside it is reached as though the mark were not there, a formula reads past it, and two
//! marks in one div do not stand for the same thing. An entry made from a template that holds one
//! inherits it, and its own text says nothing about it.

use overseer::app_api;
use overseer::file_ops::OverseerFileHandler;
use overseer::types::*;

const DOCUMENT: &str = r##"tab board (label="B", mutable=true) {

    div Card (hidden=true) {
        div (layout="horizontal") {
            mark (type="grip")
            string title = ""
            string shout = $(title + "!")
            mark (type="grip")
        }
    }

    list Cards (entry=<Card>, key="title", movable=true) {
        - {
            - title = "first"
        }
    }

    button add {
        on click {
            append (list="/board/Cards") {
                - title = "second"
            }
        }
    }
}
"##;

fn a_document() -> String {
    let root = std::env::temp_dir().join(format!("overseer_mark_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("b.os");
    std::fs::write(&file, DOCUMENT).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    file.to_string_lossy().to_string()
}

fn value_at(nodes: &[OverseerNode], address: &str) -> Option<OverseerValue> {
    let node = overseer::addressing::find(nodes, address)?;
    node.parameters.get("_computed_value").or_else(|| node.parameters.get("value")).cloned()
}

#[test]
fn a_mark_is_read_and_written_back_as_it_was() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("load");
    let written = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
    assert_eq!(written, DOCUMENT);
}

#[test]
fn the_fields_beside_a_mark_are_reached_as_if_it_were_not_there() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("load");
    assert_eq!(value_at(&nodes, "board/Cards/[first]/title"), Some(OverseerValue::String("first".into())));
    assert_eq!(value_at(&nodes, "board/Cards/[first]/shout"), Some(OverseerValue::String("first!".into())));
    assert!(overseer::addressing::find(&nodes, "board/Cards/[first]/mark").is_none());
    assert!(overseer::addressing::find(&nodes, "board/Card/mark").is_none());
}

#[test]
fn an_entry_made_from_the_template_writes_no_mark_of_its_own() {
    let file = a_document();
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("load");
    let path = overseer::addressing::name_path(&nodes, "board/add").expect("no button");
    let done = app_api::run_event_at(&file, "b.os", path, "click".into(), "s", Vec::new(), Vec::new())
        .expect("the press was refused");
    assert!(done.wrote, "the press wrote nothing");

    let after = std::fs::read_to_string(&file).unwrap();
    assert_eq!(after.matches("mark (").count(), 2, "{}", after);
    assert!(after.contains("- title = \"second\""), "{}", after);
    let nodes = app_api::load_document(after).expect("reload");
    assert_eq!(value_at(&nodes, "board/Cards/[second]/shout"), Some(OverseerValue::String("second!".into())));
}
