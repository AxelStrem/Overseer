//! A path can start from the node it belongs to: `./field`.
//!
//! The spec listed `./Data/5/Header` among the path references and the parser had no such form,
//! so `$(./handle)` was refused as an invalid formula. It came up writing a pressable shopping
//! entry, whose actions run from the entry itself and wanted to name the entry's own field.
//!
//! A bare `handle` already finds that field when it is there - a name is looked for among the
//! node's own children first. What `./` adds is that it stops there: a bare name that is not
//! found goes on looking through every ancestor, and `../` starts a level too high. `./x` is this
//! node's `x` or nothing, which is what an action on one entry of a list wants to be sure of.

use overseer::app_api;
use overseer::server::DocumentRoot;
use overseer::types::*;
use serde_json::json;

const DOCUMENT: &str = r#"tab t (label="T", mutable=true) {

    // Found by a bare name from Box below, which has no `shared` of its own - and not by `./`.
    string shared = "the tab's"

    div Box (hidden=$(./flag)) {
        bool flag = false
        int n = 2
        div inner {
            int deep = 7
        }
        list Items (entry=string) {
            - "a"
            - "b"
            - "c"
        }
    }

    div Readings {
        int own_sum = $(../../Box/n)
        string from_the_tab = $(../Box/n)
    }

    div Probe (label=$(./inner/deep + ./n), title=$(./Items.count()), bare=$(Items.count()), note=$(./shared), heard=$(shared)) {
        int n = 3
        div inner {
            int deep = 7
        }
        list Items (entry=<Thing>) {
            - {
                - v = "a"
            }
            - {
                - v = "b"
            }
        }
    }

    div (hidden=true) {
        div Thing {
            string v = ""
        }
        div Item (layout="horizontal") {
            string handle = ""
            bool bought = false
            on click {
                append (list="/t/History") {
                    - handle = $(./handle)
                }
                set (path="./bought") = true
            }
        }
    }

    list List (entry=<Item>, key="handle") {
        - {
            - handle = "milk"
        }
    }

    list History (entry=<Item>) {
    }
}
"#;

fn node<'a>(nodes: &'a [OverseerNode], path: &[&str]) -> &'a OverseerNode {
    let mut here = nodes.iter().find(|n| n.name == path[0]).expect("root");
    for seg in &path[1..] {
        here = here.children.iter().find(|c| c.name == *seg).unwrap_or_else(|| panic!("no {}", seg));
    }
    here
}

fn computed<'a>(n: &'a OverseerNode, param: &str) -> Option<&'a OverseerValue> {
    n.parameters.get(&format!("_computed_{}", param))
}

fn number(v: Option<&OverseerValue>) -> Option<f64> {
    match v {
        Some(OverseerValue::Integer(i)) => Some(*i as f64),
        Some(OverseerValue::Float(f)) => Some(*f),
        _ => None,
    }
}

#[test]
fn a_div_reads_its_own_fields() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    let probe = node(&nodes, &["t", "Probe"]);
    assert_eq!(number(computed(probe, "label")), Some(10.0), "./inner/deep + ./n: {:?}", probe.parameters);
    assert_eq!(number(computed(probe, "title")), Some(2.0), "./Items.count(): {:?}", probe.parameters);
    assert_eq!(computed(probe, "title"), computed(probe, "bare"), "./ and a bare name disagree about a list");
}

#[test]
fn a_flag_of_its_own_hides_a_div() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    let shown = node(&nodes, &["t", "Box"]);
    assert_eq!(computed(shown, "hidden"), Some(&OverseerValue::Boolean(false)), "{:?}", shown.parameters);
}

#[test]
fn it_looks_no_further_than_its_own_node() {
    // The difference from a bare name, and the reason to write it: `shared` is the tab's, and a
    // bare name finds it by climbing, while `./shared` has nothing of its own to find.
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    let probe = node(&nodes, &["t", "Probe"]);
    assert_eq!(computed(probe, "heard"), Some(&OverseerValue::String("the tab's".into())));
    assert_ne!(
        computed(probe, "note"),
        Some(&OverseerValue::String("the tab's".into())),
        "./ climbed to an ancestor: {:?}",
        probe.parameters
    );
}

#[test]
fn a_parent_path_still_reads_as_it_did() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    let readings = node(&nodes, &["t", "Readings", "own_sum"]);
    assert_eq!(number(computed(readings, "value")), Some(2.0), "{:?}", readings.parameters);
}

#[test]
fn an_entrys_actions_name_its_own_fields() {
    // The case that asked for it: a pressable entry, its action run from the entry itself.
    let root = std::env::temp_dir().join(format!("overseer_ownpath_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("t.os"), DOCUMENT).unwrap();
    overseer::viewstate::forget_all();
    let service = DocumentRoot::new(&root).expect("root");
    let nodes = service.open("t.os").expect("open");
    let path = overseer::addressing::name_path(&nodes, "t/List/[milk]").expect("the entry");
    service
        .command_for("alice", Some("t.os"), "run_overseer_event", &json!({ "node_path": path, "event_name": "click" }))
        .expect("the press was refused");
    let text = std::fs::read_to_string(root.join("t.os")).unwrap();
    let history = &text[text.find("list History").unwrap()..];
    assert!(history.contains("- handle = \"milk\""), "./handle did not read the entry's own: {}", text);
    let list = &text[text.find("list List").unwrap()..text.find("list History").unwrap()];
    assert!(list.contains("- bought = true"), "set (path=\"./bought\") did not reach the entry's own field: {}", text);
}

