//! A sorted list keeps its order after an edit.
//!
//! The order is presentation: `sort_by` gives each entry a `_ui_sort_key`, and the page sorts by
//! it, falling back to the order the file keeps for an entry without one. Only the full resolve
//! worked the keys out. An edit is answered by a selective one, and those left the keys alone - so
//! an edit to what a list sorts by left the entry where it was - and the one that starts from
//! freshly parsed text, which the page falls back to, had none at all to leave. Editing one day of
//! the diary put every day in file order, oldest first.
//!
//! The page reached that fallback on every edit for a while, because the write it sent first
//! named each value's path twice over and was refused - see the last test.

use overseer::app_api;
use overseer::delta::DocumentChange;
use overseer::types::*;

const DOCUMENT: &str = r#"tab diary (label="Diary", mutable=true) {

    div (hidden=true) {
        div Day (layout="vertical") {
            date day (label="") = "2026-01-01"
            text recap (markdown=true) = ""
        }
        div Task (layout="horizontal") {
            string id = ""
            int priority = 0
        }
    }

    list Tasks (entry=<Task>, key="id", sort_by=$(|x| 0 - x/priority)) {
        - {
            - id = "a"
            - priority = 1
        }
        - {
            - id = "b"
            - priority = 2
        }
        - {
            - id = "c"
            - priority = 3
        }
    }

    list Days (entry=<Day>, key="day", layout="vertical", sort_by=$(|x| 0 - millis_since_epoch(x/day))) {
        - {
            - day = "2026-09-10"
            - recap = "first"
        }
        - {
            - day = "2026-09-11"
            - recap = "second"
        }
        - {
            - day = "2026-09-12"
            - recap = "third"
        }
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
    let root = std::env::temp_dir().join(format!("overseer_sortedit_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("d.os");
    std::fs::write(&file, DOCUMENT).unwrap();
    app_api::forget_baseline();
    overseer::viewstate::forget_all();
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    (file.to_string_lossy().to_string(), nodes)
}

fn entry(nodes: &[OverseerNode], day: &str) -> String {
    overseer::addressing::name_path(nodes, &format!("diary/Days/[{}]", day))
        .unwrap_or_else(|| panic!("no day {}", day))
        .last()
        .unwrap()
        .clone()
}

fn days(nodes: &[OverseerNode]) -> &OverseerNode {
    nodes[0].children.iter().find(|c| c.name == "Days").expect("Days")
}

fn key(value: Option<&OverseerValue>) -> Option<i64> {
    match value {
        Some(OverseerValue::Integer(n)) => Some(*n),
        Some(OverseerValue::Float(f)) => Some(*f as i64),
        _ => None,
    }
}

/// The days in the order the page would show them: by key, and by position where there is none.
fn shown(nodes: &[OverseerNode]) -> Vec<String> {
    let mut entries: Vec<(i64, usize, String)> = days(nodes)
        .children
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let day = e.children.iter().find(|c| c.name == "day").and_then(|d| match d.parameters.get("value") {
                Some(OverseerValue::Date(s)) | Some(OverseerValue::String(s)) => Some(s.clone()),
                _ => None,
            });
            (key(e.parameters.get("_ui_sort_key")).unwrap_or(i as i64), i, day.unwrap_or_default())
        })
        .collect();
    entries.sort();
    entries.into_iter().map(|(_, _, day)| day).collect()
}

fn write(path: &str, field: Vec<String>, value: OverseerValue) -> app_api::ResolvedUpdate {
    let w: app_api::ValueWrite = serde_json::from_value(serde_json::json!({
        "node_path": field, "value": value
    }))
    .unwrap();
    app_api::write_values_at(path, "d.os", vec![w], "s").expect("the write was refused")
}

#[test]
fn opening_shows_the_newest_first() {
    // What the rest compare against.
    serialised(|| {
        let (_, nodes) = opened("open");
        assert_eq!(shown(&nodes), vec!["2026-09-12", "2026-09-11", "2026-09-10"]);
    });
}

#[test]
fn the_fallback_keeps_the_order_of_every_entry() {
    // The page's other way of asking: the text it holds and the value, worked out afresh.
    serialised(|| {
        let (_, nodes) = opened("fallback");
        let recap = format!("diary/Days/{}/recap", entry(&nodes, "2026-09-11"));
        let mut values = std::collections::HashMap::new();
        values.insert(recap.clone(), OverseerValue::String("edited".into()));
        let answer = app_api::resolve_selective(DOCUMENT.to_string(), vec![recap], Some(values))
            .expect("resolve");
        for e in &days(&answer).children {
            assert!(
                e.parameters.contains_key("_ui_sort_key"),
                "{} came back without its place in the order",
                e.name
            );
        }
        assert_eq!(shown(&answer), vec!["2026-09-12", "2026-09-11", "2026-09-10"]);
    });
}

#[test]
fn the_fallback_moves_an_entry_whose_sort_field_was_edited() {
    serialised(|| {
        let (_, nodes) = opened("fallback_moves");
        let day = format!("diary/Days/{}/day", entry(&nodes, "2026-09-11"));
        let mut values = std::collections::HashMap::new();
        values.insert(day.clone(), OverseerValue::Date("2026-09-20".into()));
        let answer = app_api::resolve_selective(DOCUMENT.to_string(), vec![day], Some(values))
            .expect("resolve");
        assert_eq!(shown(&answer), vec!["2026-09-20", "2026-09-12", "2026-09-10"]);
    });
}

#[test]
fn a_write_moves_an_entry_whose_sort_field_was_edited() {
    // The ordinary way: a write, answered with what changed. The new key has to be among it, or
    // the page goes on showing the day where it used to sort.
    serialised(|| {
        let (path, nodes) = opened("write_moves");
        let moved = entry(&nodes, "2026-09-11");

        let answer = write(
            &path,
            vec!["diary".into(), "Days".into(), moved.clone(), "day".into()],
            OverseerValue::Date("2026-09-20".into()),
        );

        // Taken on the way the page takes it, and read the way the page reads it. The day is the
        // list's key too, so changing it renames the entry and the list may come back whole; what
        // matters is where the day ends up, not which shape of answer put it there.
        let mut page = nodes.clone();
        apply(&mut page, answer.changes.expect("answered with what changed"));
        assert_eq!(shown(&page), vec!["2026-09-20", "2026-09-12", "2026-09-10"]);
    });
}

#[test]
fn a_write_moves_an_entry_whose_sort_field_is_not_its_key() {
    // The case the date cannot show: a field that is not the key moves no address, so nothing
    // comes back whole, and the new place has to be among the described changes or it is nowhere.
    serialised(|| {
        let (path, nodes) = opened("write_priority");
        let b = overseer::addressing::name_path(&nodes, "diary/Tasks/[b]").expect("b").last().unwrap().clone();
        let answer = write(
            &path,
            vec!["diary".into(), "Tasks".into(), b, "priority".into()],
            OverseerValue::Integer(9),
        );
        let mut page = nodes.clone();
        apply(&mut page, answer.changes.expect("answered with what changed"));
        assert_eq!(tasks_shown(&page), vec!["b", "c", "a"]);
    });
}

/// The tasks in the order the page would show them.
fn tasks_shown(nodes: &[OverseerNode]) -> Vec<String> {
    let tasks = nodes[0].children.iter().find(|c| c.name == "Tasks").expect("Tasks");
    let mut entries: Vec<(i64, usize, String)> = tasks
        .children
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let id = e.children.iter().find(|c| c.name == "id").and_then(|d| match d.parameters.get("value") {
                Some(OverseerValue::String(s)) => Some(s.clone()),
                _ => None,
            });
            (key(e.parameters.get("_ui_sort_key")).unwrap_or(i as i64), i, id.unwrap_or_default())
        })
        .collect();
    entries.sort();
    entries.into_iter().map(|(_, _, id)| id).collect()
}

/// What `applyDocumentChanges` does on the page.
fn apply(nodes: &mut Vec<OverseerNode>, changes: Vec<DocumentChange>) {
    fn at<'a>(nodes: &'a mut Vec<OverseerNode>, path: &[usize]) -> &'a mut OverseerNode {
        let mut node = &mut nodes[path[0]];
        for i in &path[1..] {
            node = &mut node.children[*i];
        }
        node
    }
    let mut removals = Vec::new();
    for change in changes {
        match change {
            DocumentChange::Parameters { path, parameters, .. } => at(nodes, &path).parameters = parameters,
            DocumentChange::Subtree { path, node, .. } => *at(nodes, &path) = node,
            DocumentChange::Removed { path, .. } => removals.push(path),
        }
    }
    removals.sort();
    for path in removals.into_iter().rev() {
        let (last, parent) = path.split_last().unwrap();
        if parent.is_empty() {
            nodes.remove(*last);
        } else {
            at(nodes, parent).children.remove(*last);
        }
    }
}

#[test]
fn a_write_to_anything_else_leaves_every_key_in_place() {
    serialised(|| {
        let (path, nodes) = opened("write_keeps");
        let answer = write(
            &path,
            vec!["diary".into(), "Days".into(), entry(&nodes, "2026-09-11"), "recap".into()],
            OverseerValue::String("edited".into()),
        );
        for c in answer.changes.expect("answered with what changed") {
            if let DocumentChange::Parameters { address, parameters, .. } = c {
                let is_an_entry = address.starts_with("diary/Days/[") && address.matches('/').count() == 2;
                if is_an_entry {
                    assert!(parameters.contains_key("_ui_sort_key"), "{} lost its place in the order", address);
                }
            }
        }
    });
}

#[test]
fn a_value_names_its_path_once() {
    // The door takes either spelling and refuses both at once as a duplicate field. The page sent
    // both, every field edit was refused, and nothing said so - it fell back to sending the whole
    // text, which is the path that lost the order.
    let once: std::result::Result<app_api::ValueWrite, _> = serde_json::from_value(serde_json::json!({
        "node_path": ["diary", "Days", "Day__2", "recap"], "value": { "String": "x" }
    }));
    assert!(once.is_ok(), "the spelling the page sends is not accepted: {:?}", once.err());

    let twice: std::result::Result<app_api::ValueWrite, _> = serde_json::from_value(serde_json::json!({
        "node_path": ["diary"], "nodePath": ["diary"], "value": { "String": "x" }
    }));
    let refused = twice.expect_err("both spellings at once are accepted after all - the page may send them again");
    assert!(refused.to_string().contains("duplicate field"), "{}", refused);
}
