//! A day that only a `layout="year"` draws goes to the page as the square it is drawn as: its
//! own parameters, its style among them, and nothing under it. The tree the server keeps holds
//! the day whole, so a write into it, a read of it by address and the saved file lose nothing.

use crate::common::serial;
use overseer::server::DocumentRoot;
use overseer::types::*;
use serde_json::{json, Value};

const DOC: &str = r##"tab t (label="T", mutable=true) {
    div (hidden=true) {
        div Day {
            int total = 0
            int doubled = $(total * 2)
            div meals {
                string breakfast = "porridge"
            }
        }
        <Day> HeatDay (background-color=$(total > 1000 ? "#e5534b" : "#30a14e")) { }
    }

    calendar Days (entry=<Day>, span="day", at="2026-11-05", layout="day") {
        - 2026-10-27 {
            - total = 1500
        }
    }

    calendar Year (of="Days", entry=<HeatDay>, from="2026-10-26", to="2026-11-08", layout="year")

    button eat (label="Eat") {
        on click {
            set (path="../Days/2026-10-28/total") = 1200
        }
    }
}
"##;

// Saving consults a process-wide registry of parsed sources, and the view state is
// process-wide too.

fn a_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_square_sent_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("d.os"), DOC).unwrap();
    overseer::app_api::forget_baseline();
    overseer::viewstate::forget_all();
    root
}

/// A node of the page's answer, found by the names on the way to it.
fn sent<'a>(nodes: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut here = nodes;
    for name in path {
        let children = here.as_array().or_else(|| here.get("children")?.as_array())?;
        here = children.iter().find(|c| c.get("name").and_then(Value::as_str) == Some(name))?;
    }
    Some(here)
}

fn open(service: &DocumentRoot) -> Value {
    service
        .command_for("someone", Some("d.os"), "parse_overseer_content", &json!({ "content": DOC, "path": "d.os" }))
        .expect("open")
}

#[test]
fn a_square_goes_to_the_page_without_its_children() {
    let _guard = serial();
    let root = a_root("open");
    let service = DocumentRoot::new(&root).expect("open the root");
    let answer = open(&service);

    let square = sent(&answer, &["t", "Days", "2026-10-27"]).expect("the square");
    assert_eq!(square["parameters"]["_drawn_by_style"], json!({ "Boolean": true }), "{}", square);
    assert_eq!(
        square["parameters"]["_computed__view@t/Year@background-color"],
        json!({ "String": "#e5534b" }),
        "the square lost its colour: {}",
        square
    );
    assert!(square.get("children").is_none(), "the square was sent with its children: {}", square);

    let day = sent(&answer, &["t", "Days", "2026-11-05"]).expect("the day in view");
    assert!(day["parameters"].get("_drawn_by_style").is_none());
    assert!(sent(&answer, &["t", "Days", "2026-11-05", "meals", "breakfast"]).is_some(), "{}", day);
}

#[test]
fn what_the_server_keeps_still_holds_the_square_whole() {
    let _guard = serial();
    let nodes = overseer::app_api::load_document(DOC.to_string()).unwrap();
    let square = overseer::addressing::find(&nodes, "t/Days/2026-10-27").expect("the square");
    assert!(overseer::resolver::drawn_by_style(square));
    assert!(!square.children.is_empty());

    let plain = serde_json::to_string(&nodes).unwrap();
    let back: Vec<OverseerNode> = serde_json::from_str(&plain).unwrap();
    let kept = overseer::addressing::find(&back, "t/Days/2026-10-27/meals/breakfast");
    assert!(kept.is_some(), "the ordinary form lost what is under the square");

    // And writing for the page leaves nothing switched on behind it.
    let slim = serde_json::to_string(&ForThePage(&nodes)).unwrap();
    assert!(slim.len() < plain.len());
    assert_eq!(serde_json::to_string(&nodes).unwrap(), plain);
}

#[test]
fn a_square_written_and_read_by_address_is_whole() {
    let _guard = serial();
    let root = a_root("write");
    let service = DocumentRoot::new(&root).expect("open the root");
    open(&service);
    let nodes = service.open_for("someone", "d.os").expect("open");
    let path = overseer::addressing::name_path(&nodes, "t/eat").expect("the button");
    let change = service
        .command_for("someone", Some("d.os"), "run_overseer_event", &json!({ "node_path": path, "event_name": "click" }))
        .expect("the press was refused");
    assert!(!change.to_string().contains("\"breakfast\""), "the change sent what is under a square: {}", change);

    let read = service.read_at("d.os", "t/Days/2026-10-28").expect("the read was refused");
    assert!(read.child_addresses.iter().any(|a| a.ends_with("/meals")), "{:?}", read.child_addresses);
    let total = service.read_at("d.os", "t/Days/2026-10-28/total").expect("the read was refused");
    assert_eq!(total.node.parameters.get("_computed_value").or(total.node.parameters.get("value")), Some(&OverseerValue::Integer(1200)));

    let written = std::fs::read_to_string(root.join("d.os")).unwrap();
    assert!(written.contains("- 2026-10-28 {"), "{}", written);
    assert!(written.contains("- total = 1200"), "{}", written);
    assert!(written.contains("- total = 1500"), "{}", written);
    assert!(written.contains("string breakfast = \"porridge\""), "{}", written);
}
