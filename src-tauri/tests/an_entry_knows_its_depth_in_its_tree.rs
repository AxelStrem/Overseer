//! `depth()`, in a formula inside an entry of a list drawn with `tree=`, gives how deep the entry
//! stands in its tree, 0 at the top. In a tree whose entries hold their children, that is how
//! many lists of the tree the entry is nested in; in a tree by a field naming each entry's parent,
//! how many parents up the chain goes before it meets one not in the list, or a cycle.

use overseer::app_api;
use overseer::dependencies;
use overseer::file_ops::OverseerFileHandler;
use overseer::parser;
use overseer::resolver;
use overseer::types::*;

const DOC: &str = r#"div (hidden=true) {
    div Part {
        string name = ""
        int d = $(depth())
        list Parts (entry=<Part>, key="name") { }
    }

    div Task (margin-left=$(depth() * 12)) {
        string handle = ""
        string parent = ""
        int d = $(depth())
    }
}

tab t (mutable=true) {
    list Parts (entry=<Part>, key="name", tree="Parts") {
        - {
            - name = "Car"
            list Parts {
                - {
                    - name = "Engine"
                    list Parts {
                        - {
                            - name = "Pistons"
                        }
                    }
                }
            }
        }
    }

    list Tasks (entry=<Task>, key="handle", tree="parent") {
        - {
            - handle = "site"
        }
        - {
            - handle = "pages"
            - parent = "site"
        }
        - {
            - handle = "about"
            - parent = "pages"
        }
        - {
            - handle = "lost"
            - parent = "nowhere"
        }
        - {
            - handle = "ping"
            - parent = "pong"
        }
        - {
            - handle = "pong"
            - parent = "ping"
        }
        - {
            - handle = "below"
            - parent = "ping"
        }
    }

    int outside = 0
}
"#;

/// Saving consults a process-wide registry of parsed sources, so two of these running at once
/// see each other's document.
static SAVING: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn int_at(nodes: &[OverseerNode], address: &str) -> Option<i64> {
    let node = overseer::addressing::find(nodes, address)?;
    match node.parameters.get("_computed_value").or_else(|| node.parameters.get("value")) {
        Some(OverseerValue::Integer(i)) => Some(*i),
        Some(OverseerValue::Float(f)) if f.fract() == 0.0 => Some(*f as i64),
        _ => None,
    }
}

#[test]
fn a_nested_entry_counts_the_lists_it_is_in() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = app_api::load_document(DOC.to_string()).unwrap();
    assert_eq!(int_at(&nodes, "t/Parts/[Car]/d"), Some(0));
    assert_eq!(int_at(&nodes, "t/Parts/[Car]/Parts/[Engine]/d"), Some(1));
    assert_eq!(int_at(&nodes, "t/Parts/[Car]/Parts/[Engine]/Parts/[Pistons]/d"), Some(2));
}

#[test]
fn an_entry_by_parent_counts_its_parents() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = app_api::load_document(DOC.to_string()).unwrap();
    assert_eq!(int_at(&nodes, "t/Tasks/[site]/d"), Some(0));
    assert_eq!(int_at(&nodes, "t/Tasks/[pages]/d"), Some(1));
    assert_eq!(int_at(&nodes, "t/Tasks/[about]/d"), Some(2));
}

#[test]
fn a_parent_not_in_the_list_or_a_cycle_is_the_top() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = app_api::load_document(DOC.to_string()).unwrap();
    assert_eq!(int_at(&nodes, "t/Tasks/[lost]/d"), Some(0));
    assert_eq!(int_at(&nodes, "t/Tasks/[ping]/d"), Some(0));
    assert_eq!(int_at(&nodes, "t/Tasks/[pong]/d"), Some(0));
    assert_eq!(int_at(&nodes, "t/Tasks/[below]/d"), Some(1));
}

#[test]
fn a_parameter_of_the_entry_reads_its_depth() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = app_api::load_document(DOC.to_string()).unwrap();
    let about = overseer::addressing::find(&nodes, "t/Tasks/[about]").expect("the entry");
    let margin = about
        .parameters
        .get("_computed_margin-left")
        .or_else(|| about.parameters.get("margin-left"));
    assert!(
        matches!(margin, Some(OverseerValue::Integer(24)))
            || matches!(margin, Some(OverseerValue::Float(f)) if *f == 24.0),
        "{:?}",
        about.parameters
    );
}

#[test]
fn a_changed_parent_moves_the_depth_below_it() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let (_rest, mut nodes) = parser::parse_document(DOC).expect("parse");
    dependencies::start_recording();
    resolver::resolve_document(&mut nodes);
    let graph = dependencies::take_recording();
    let reached = graph.nodes_to_work_out_again(&["t/Tasks/Task__2/parent".to_string()]);
    assert!(reached.contains(&"t/Tasks/Task__3/d".to_string()), "{:?}", reached);

    let moved = DOC.replace("- parent = \"site\"", "- parent = \"below\"");
    let nodes = app_api::load_document(moved).unwrap();
    assert_eq!(int_at(&nodes, "t/Tasks/[pages]/d"), Some(2));
    assert_eq!(int_at(&nodes, "t/Tasks/[about]/d"), Some(3));
}

#[test]
fn depth_outside_an_entry_of_a_tree_is_an_error() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let doc = DOC.replace("int outside = 0", "int outside = $(depth())");
    let nodes = app_api::load_document(doc).unwrap();
    assert_eq!(int_at(&nodes, "t/outside"), None);
}

#[test]
fn a_tree_with_depths_saves_as_it_was_written() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = app_api::load_document(DOC.to_string()).unwrap();
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), DOC);
}
