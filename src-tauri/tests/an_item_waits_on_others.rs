//! A project item can say which items it waits on, and reads as waiting while any of them is open.
//!
//! `after` holds handles, as a tags field holds tags. Nothing is ever taken out of it: whether an
//! item still waits is worked out - how many of those handles are still on the open list - so
//! finishing or dropping what it waited on frees it with nothing edited, and `after` keeps the
//! record of what it waited on. Each item also works out its colour from its stage, which is what
//! a handle in someone's `after` is drawn in. Checked against the template every project copies.

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

fn a_project(tag: &str) -> String {
    let root = std::env::temp_dir().join(format!("overseer_after_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("p.os");
    std::fs::write(&file, TEMPLATE).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let instant = chrono::DateTime::parse_from_rfc3339("2026-10-05T09:00:00Z").unwrap().with_timezone(&chrono::Utc);
    FormulaEvaluator::set_time_override(Some(instant));
    file.to_string_lossy().to_string()
}

/// A worked-out field of the entry with this handle, in Items or History.
fn read(nodes: &[OverseerNode], list: &str, handle: &str, field: &str) -> Option<OverseerValue> {
    fn seek<'a>(node: &'a OverseerNode, name: &str) -> Option<&'a OverseerNode> {
        node.children.iter().filter(|c| c.node_type != "on" && c.node_type != "button").find_map(|c| {
            if c.name == name { Some(c) } else { seek(c, name) }
        })
    }
    let list = overseer::addressing::find(nodes, &format!("project/{}", list)).expect(list);
    let entry = list
        .children
        .iter()
        .find(|e| seek(e, "handle").and_then(|h| h.parameters.get("value")) == Some(&OverseerValue::String(handle.into())))
        .unwrap_or_else(|| panic!("no {} in {}", handle, list.name));
    let node = seek(entry, field).unwrap_or_else(|| panic!("no {} on {}", field, handle));
    node.parameters.get("_computed_value").or_else(|| node.parameters.get("value")).cloned()
}

fn opened(text: String) -> Vec<OverseerNode> {
    app_api::load_document(text).expect("open")
}

#[test]
fn an_item_waits_while_what_it_names_is_open() {
    serialised(|| {
        let nodes = opened(TEMPLATE.to_string());
        // `favicon` waits on `rows`, which is open.
        assert_eq!(read(&nodes, "Items", "favicon", "waiting"), Some(OverseerValue::Integer(1)));
        // `rows` waits on `escape`, which is finished.
        assert_eq!(read(&nodes, "Items", "rows", "waiting"), Some(OverseerValue::Integer(0)));
        // And one naming nothing waits on nothing.
        assert_eq!(read(&nodes, "Items", "docs", "waiting"), Some(OverseerValue::Integer(0)));
    });
}

#[test]
fn an_item_takes_its_stages_colour() {
    serialised(|| {
        let nodes = opened(TEMPLATE.to_string());
        let colour = |list: &str, handle: &str| read(&nodes, list, handle, "colour");
        assert_eq!(colour("Items", "rows"), Some(OverseerValue::String("#6b7280".into())), "filed, by default");
        assert_eq!(colour("Items", "undo"), Some(OverseerValue::String("#475569".into())), "later");
        assert_eq!(colour("History", "escape"), Some(OverseerValue::String("#15803d".into())), "finished");
    });
}

#[test]
fn finishing_what_an_item_waits_on_frees_it_and_keeps_the_record() {
    serialised(|| {
        let file = a_project("frees");
        let nodes = opened(std::fs::read_to_string(&file).unwrap());
        let path = overseer::addressing::name_path(&nodes, "project/Items/[rows]/stage").expect("rows");
        let value = app_api::ValueWrite { node_path: path.clone(), value: OverseerValue::String("finished".into()) };
        app_api::run_event_at(&file, "p.os", path, "change".into(), "s", Vec::new(), vec![value]).expect("the pick");

        let text = std::fs::read_to_string(&file).unwrap();
        let now = opened(text.clone());
        assert_eq!(
            read(&now, "Items", "favicon", "waiting"),
            Some(OverseerValue::Integer(0)),
            "still waiting on something finished"
        );
        assert!(text.contains("- after = \"rows\""), "the record of what favicon waited on went:\n{}", text);
        assert_eq!(
            read(&now, "History", "rows", "after"),
            Some(OverseerValue::String("escape".into())),
            "the finished record forgot what it waited on"
        );
    });
}
