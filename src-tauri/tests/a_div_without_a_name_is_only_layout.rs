//! A div with no name is layout and nothing else.
//!
//! It puts what it holds in a row, or hides a block of templates, and means nothing to any logic.
//! Every path the resolver builds names a node the way its address does, with the wrapper looked
//! through: the path a formula is worked out against, the key its answer is remembered under,
//! what the dependency graph records, what a write says it changed and what an action is run
//! from.
//!
//! Wrappers used to be a step in all of those and not in addresses. A `..` written inside one
//! landed on the wrapper, a level short of where the document meant, and every such read fell
//! through to a search. The food tracker had learned to say `../../food` for a field of its own
//! entry, and putting two fields in a row for the look of it changed what their formulas read.
//!
//! A wrapper's own parameters are its container's business: `./show` on one reads the field its
//! container holds, as a bare name does.

use overseer::app_api;
use overseer::delta::DocumentChange;
use overseer::dependencies::{self, Graph};
use overseer::resolver;
use overseer::types::*;

const DOCUMENT: &str = r#"tab t (label="T", mutable=true) {
    int n = 1

    div Box {
        int n = 2
        bool show = true
        int count = 0

        // The same two formulas three times: beside the fields, in a row, and in a row in a row.
        int beside = $(../n)
        int beside_far = $(../../n)
        div (layout="horizontal") {
            int in_a_row = $(../n)
            int in_a_row_far = $(../../n)
            div (layout="vertical") {
                int deeper = $(../n)
                int deeper_far = $(../../n)
            }
        }

        div (layout="horizontal") {
            int a = 3
            int b = 4
        }
        int total = $(../a + ../b)

        div (layout="horizontal", hidden=$(./show == false)) {
            button Add (label="add") {
                on click {
                    set (path="../count") = $(../count + 1)
                }
            }
        }
        int doubled = $(../count * 2)
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

/// A file holding the document, opened the way the page opens it.
fn opened(tag: &str) -> (String, Vec<OverseerNode>) {
    let root = std::env::temp_dir().join(format!("overseer_wrapper_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("d.os");
    std::fs::write(&file, DOCUMENT).unwrap();
    app_api::forget_baseline();
    overseer::viewstate::forget_all();
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    (file.to_string_lossy().to_string(), nodes)
}

fn at<'a>(nodes: &'a [OverseerNode], address: &str) -> &'a OverseerNode {
    overseer::addressing::find(nodes, address).unwrap_or_else(|| panic!("nothing at {}", address))
}

fn number(nodes: &[OverseerNode], address: &str) -> Option<f64> {
    match at(nodes, address).parameters.get("_computed_value") {
        Some(OverseerValue::Integer(i)) => Some(*i as f64),
        Some(OverseerValue::Float(f)) => Some(*f),
        _ => None,
    }
}

/// The value a change hands the page for this address, if it hands one.
fn answered(changes: &[DocumentChange], address: &str) -> Option<OverseerValue> {
    changes.iter().find_map(|change| match change {
        DocumentChange::Parameters { address: a, parameters, .. } if a == address => {
            parameters.get("_computed_value").cloned()
        }
        _ => None,
    })
}

#[test]
fn a_row_is_no_step_up() {
    // What moving fields into a row must not change: the same formula reads the same thing.
    serialised(|| {
        let (_, nodes) = opened("climb");
        for (near, far) in [
            ("t/Box/beside", "t/Box/beside_far"),
            ("t/Box/in_a_row", "t/Box/in_a_row_far"),
            ("t/Box/deeper", "t/Box/deeper_far"),
        ] {
            assert_eq!(number(&nodes, near), Some(2.0), "{} did not read Box's n", near);
            assert_eq!(number(&nodes, far), Some(1.0), "{} did not read the tab's n", far);
        }
    });
}

#[test]
fn the_graph_names_nodes_by_their_addresses() {
    // One spelling for a node, everywhere. A key the page's addresses cannot produce is a
    // dependency no edit will ever reach.
    let (_rest, mut nodes) = overseer::parser::parse_document(DOCUMENT).expect("parse");
    dependencies::start_recording();
    resolver::resolve_document(&mut nodes);
    let graph = dependencies::take_recording();
    let addresses: std::collections::HashSet<String> =
        overseer::addressing::addresses(&nodes).into_iter().collect();
    for value in graph.values() {
        let node = Graph::node_of(value);
        assert!(addresses.contains(node), "the graph names {} and no address does: {:?}", node, addresses);
    }
    let reached = graph.nodes_to_work_out_again(&["t/Box/n".to_string()]);
    for reader in ["t/Box/in_a_row", "t/Box/deeper", "t/Box/beside"] {
        assert!(reached.iter().any(|r| r == reader), "an edit to Box's n misses {}: {:?}", reader, reached);
    }
}

#[test]
fn an_edit_in_a_row_reaches_what_reads_it() {
    // The page names the field by the path an action takes to it, and the answer has to carry
    // the total that sums it - worked out from the graph, not by opening the document again.
    serialised(|| {
        let (path, nodes) = opened("edit");
        let field = overseer::addressing::name_path(&nodes, "t/Box/a").expect("the field");
        let w: app_api::ValueWrite = serde_json::from_value(serde_json::json!({
            "node_path": field, "value": OverseerValue::Integer(10)
        }))
        .unwrap();
        let answer = app_api::write_values_at(&path, "d.os", vec![w], "s").expect("the write was refused");
        let changes = answer.changes.expect("answered with what changed");
        let total = answered(&changes, "t/Box/total");
        assert!(
            matches!(total, Some(OverseerValue::Integer(14))) || matches!(total, Some(OverseerValue::Float(f)) if f == 14.0),
            "the total was not worked out again: {:?}",
            changes.iter().map(|c| c.address().to_string()).collect::<Vec<_>>()
        );
    });
}

#[test]
fn a_button_in_a_row_writes_beside_it() {
    // `..` from the button is the div that holds the row, and so is what the button's formula
    // reads - and what reads the count hears about it.
    serialised(|| {
        let (path, nodes) = opened("press");
        let button = overseer::addressing::name_path(&nodes, "t/Box/Add").expect("the button");
        let answer = app_api::run_event_at(&path, "d.os", button, "click".into(), "s", Vec::new())
            .expect("the press was refused");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("int count = 1"), "the press did not count: {}", text);
        let changes = answer.changes.expect("answered with what changed");
        let doubled = answered(&changes, "t/Box/doubled");
        assert!(
            matches!(doubled, Some(OverseerValue::Integer(2))) || matches!(doubled, Some(OverseerValue::Float(f)) if f == 2.0),
            "what reads the count was not worked out again: {:?}",
            doubled
        );
    });
}

#[test]
fn a_wrappers_own_formula_reads_its_container() {
    serialised(|| {
        let (_, nodes) = opened("own");
        let row = &at(&nodes, "t/Box").children;
        let with_button = row
            .iter()
            .find(|c| overseer::addressing::is_wrapper(c) && c.children.iter().any(|g| g.name == "Add"))
            .expect("the row holding the button");
        assert_eq!(
            with_button.parameters.get("_computed_hidden"),
            Some(&OverseerValue::Boolean(false)),
            "{:?}",
            with_button.parameters
        );
    });
}
