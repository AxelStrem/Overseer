//! A task in a project can be taken off the list without being finished.
//!
//! `finish` records a task as done and takes its points. Some tasks are not done but dropped - no
//! longer wanted, or added by mistake - and recording those as finished would credit points for
//! nothing. `drop` takes the task off the list and records nothing. It is hidden while anything
//! under the task is still open, as `finish` is, so a child is never left naming a parent that has
//! gone.

use overseer::app_api;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::types::*;

const TEMPLATE: &str = include_str!("../../examples/projects/project_template.os");

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    FormulaEvaluator::set_time_override(None);
    out
}

/// The template in a file, opened the way the page opens it.
fn opened(tag: &str) -> (String, Vec<OverseerNode>) {
    let root = std::env::temp_dir().join(format!("overseer_project_drop_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("p.os");
    std::fs::write(&file, TEMPLATE).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let instant = chrono::DateTime::parse_from_rfc3339("2026-10-03T09:00:00Z").unwrap().with_timezone(&chrono::Utc);
    FormulaEvaluator::set_time_override(Some(instant));
    let nodes = app_api::load_document(TEMPLATE.to_string()).expect("open");
    (file.to_string_lossy().to_string(), nodes)
}

/// A field of an entry, wherever in it the template puts it - but not inside a button, whose
/// actions name the same fields.
fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    nodes.iter().filter(|n| n.node_type != "button").find_map(|n| {
        if n.name == name {
            Some(n)
        } else {
            find(&n.children, name)
        }
    })
}

fn handles(nodes: &[OverseerNode], list: &str) -> Vec<String> {
    fn list_named<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
        nodes.iter().find_map(|n| {
            if n.name == name && n.node_type == "list" {
                Some(n)
            } else {
                list_named(&n.children, name)
            }
        })
    }
    list_named(nodes, list)
        .unwrap_or_else(|| panic!("no {}", list))
        .children
        .iter()
        .filter_map(|e| match find(&e.children, "handle").and_then(|h| h.parameters.get("value")) {
            Some(OverseerValue::String(s)) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

fn the_drop_button<'a>(nodes: &'a [OverseerNode], handle: &str) -> &'a OverseerNode {
    let entry = nodes
        .iter()
        .find_map(|n| find_entry(n, handle))
        .unwrap_or_else(|| panic!("no task {}", handle));
    entry.children.iter().find(|c| c.name == "drop" && c.node_type == "button").expect("no drop button")
}

fn find_entry<'a>(node: &'a OverseerNode, handle: &str) -> Option<&'a OverseerNode> {
    if node.node_type == "list" && node.name == "Items" {
        return node.children.iter().find(|e| {
            matches!(find(&e.children, "handle").and_then(|h| h.parameters.get("value")),
                Some(OverseerValue::String(s)) if s == handle)
        });
    }
    node.children.iter().find_map(|c| find_entry(c, handle))
}

fn hidden(node: &OverseerNode) -> bool {
    matches!(
        node.parameters.get("_computed_hidden").or_else(|| node.parameters.get("hidden")),
        Some(OverseerValue::Boolean(true))
    )
}

#[test]
fn dropping_a_task_takes_it_off_the_list_and_records_nothing() {
    serialised(|| {
        let (file, nodes) = opened("drop");
        let finished_before = handles(&nodes, "History").len();
        assert!(handles(&nodes, "Items").contains(&"brace".to_string()));

        let button = overseer::addressing::name_path(&nodes, "project/Items/[brace]/drop").expect("the button");
        let answer = app_api::run_event_at(&file, "p.os", button, "click".into(), "s", Vec::new(), Vec::new()).expect("the press");
        assert!(answer.wrote, "dropping wrote nothing");

        let now = app_api::load_document(std::fs::read_to_string(&file).unwrap()).expect("reopen");
        assert!(!handles(&now, "Items").contains(&"brace".to_string()), "the task is still open");
        assert_eq!(handles(&now, "History").len(), finished_before, "dropping recorded it as finished");
        assert!(!handles(&now, "History").contains(&"brace".to_string()));
    });
}

#[test]
fn a_task_with_open_work_under_it_offers_no_drop() {
    serialised(|| {
        let (_, nodes) = opened("parent");
        // `editor` has open children; `brace` is a leaf.
        assert!(hidden(the_drop_button(&nodes, "editor")), "a parent with open children can be dropped");
        assert!(!hidden(the_drop_button(&nodes, "brace")), "a leaf cannot be dropped");
    });
}
