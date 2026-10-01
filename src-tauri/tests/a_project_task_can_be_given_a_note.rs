//! A task in a project can be given a note from the page, open or finished.
//!
//! The commentary under a task is hidden while it is empty, which is most of them - and a hidden
//! field has nowhere to tap, so a task nobody had said anything about could not be given a note
//! at all except by editing the file. Each task has a note button now that opens it, the way a
//! task's comment opens in the task manager.
//!
//! A finished task keeps its commentary out of sight for good, since a history is long and would
//! be mostly commentary. Its button opens it all the same, and giving it one meant putting the
//! row it sat in inside a column - so finishing a task is checked to still carry its note across.

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
    let root = std::env::temp_dir().join(format!("overseer_project_note_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("p.os");
    std::fs::write(&file, TEMPLATE).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let instant = chrono::DateTime::parse_from_rfc3339("2026-10-01T09:00:00Z").unwrap().with_timezone(&chrono::Utc);
    FormulaEvaluator::set_time_override(Some(instant));
    let nodes = app_api::load_document(TEMPLATE.to_string()).expect("open");
    (file.to_string_lossy().to_string(), nodes)
}

/// A field of an entry, wherever in it the template puts it - but not inside a button, whose
/// actions name the same fields: `finish` sets a `commentary` of its own on the way out.
fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    nodes.iter().filter(|n| n.node_type != "button").find_map(|n| {
        if n.name == name {
            Some(n)
        } else {
            find(&n.children, name)
        }
    })
}

fn text(node: &OverseerNode) -> String {
    match node.parameters.get("value") {
        Some(OverseerValue::String(s)) => s.clone(),
        _ => String::new(),
    }
}

/// The child indices of the entry of `list` whose handle reads `handle`.
fn entry_of(nodes: &[OverseerNode], list: &str, handle: &str) -> Vec<usize> {
    fn walk(level: &[OverseerNode], list: &str, handle: &str, at: &mut Vec<usize>) -> bool {
        for (i, node) in level.iter().enumerate() {
            at.push(i);
            if node.name == list && node.node_type == "list" {
                if let Some(j) = node
                    .children
                    .iter()
                    .position(|e| find(&e.children, "handle").map(text).as_deref() == Some(handle))
                {
                    at.push(j);
                    return true;
                }
            } else if walk(&node.children, list, handle, at) {
                return true;
            }
            at.pop();
        }
        false
    }
    let mut at = Vec::new();
    assert!(walk(nodes, list, handle, &mut at), "no `{}` in {}", handle, list);
    at
}

fn node_at<'a>(nodes: &'a [OverseerNode], path: &[usize]) -> &'a OverseerNode {
    path[1..].iter().fold(&nodes[path[0]], |node, i| &node.children[*i])
}

/// By name, the way the page names what it presses: the entry's, then down to the button.
fn press(file: &str, nodes: &[OverseerNode], entry: &[usize], button: &str) -> app_api::ResolvedUpdate {
    let mut names = Vec::new();
    let mut level = nodes;
    for i in entry {
        names.push(level[*i].name.clone());
        level = &level[*i].children;
    }
    fn down(level: &[OverseerNode], button: &str, names: &mut Vec<String>) -> bool {
        for node in level {
            names.push(node.name.clone());
            if node.name == button && node.node_type == "button" || down(&node.children, button, names) {
                return true;
            }
            names.pop();
        }
        false
    }
    assert!(down(level, button, &mut names), "no `{}` button in the entry", button);
    app_api::run_event_at(file, "p.os", names, "click".into(), "s", Vec::new()).expect("the press was refused")
}

#[test]
fn an_open_task_opens_its_commentary() {
    serialised(|| {
        let (file, nodes) = opened("open");
        // `editor` has nothing said about it, so its commentary is hidden: the case this is for.
        let entry = entry_of(&nodes, "Items", "editor");
        let answer = press(&file, &nodes, &entry, "note");

        let wanted = answer.start_editing.expect("the answer names no field to open");
        assert!(wanted.path.starts_with(&entry), "it opens a field of another task: {}", wanted.address);
        let field = node_at(&nodes, &wanted.path);
        assert_eq!(field.name, "commentary", "it opens {}", wanted.address);
        assert_eq!(field.parameters.get("_computed_hidden"), Some(&OverseerValue::Boolean(true)));
        assert!(!answer.wrote, "opening a field wrote the file");
    });
}

#[test]
fn a_finished_task_opens_its_commentary() {
    serialised(|| {
        let (file, nodes) = opened("finished");
        let entry = entry_of(&nodes, "History", "escape");
        let answer = press(&file, &nodes, &entry, "note");

        let wanted = answer.start_editing.expect("the answer names no field to open");
        assert!(wanted.path.starts_with(&entry), "it opens a field of another task: {}", wanted.address);
        assert_eq!(node_at(&nodes, &wanted.path).name, "commentary", "it opens {}", wanted.address);
        assert!(!answer.wrote, "opening a field wrote the file");
    });
}

#[test]
fn finishing_a_task_carries_its_commentary_into_the_history() {
    serialised(|| {
        let (file, nodes) = opened("carries");
        let entry = entry_of(&nodes, "Items", "brace");
        let said = text(find(&node_at(&nodes, &entry).children, "commentary").unwrap());
        assert!(!said.is_empty(), "the sample task has nothing to carry");

        assert!(press(&file, &nodes, &entry, "finish").wrote, "finishing wrote nothing");
        let written = std::fs::read_to_string(&file).unwrap();
        let now = app_api::load_document(written).expect("the file reopens");
        let finished = node_at(&now, &entry_of(&now, "History", "brace"));
        assert_eq!(text(find(&finished.children, "commentary").expect("no commentary")), said);
        // And it is the history's own, under the row - not one left over in the row beside it.
        assert!(
            finished.children.iter().any(|c| c.name == "commentary"),
            "the commentary is not where the template puts it: {:?}",
            finished.children.iter().map(|c| c.name.as_str()).collect::<Vec<_>>()
        );
    });
}
