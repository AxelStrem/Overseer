//! A project item holds a conversation and a record of the agents' work on it.
//!
//! The one-line note an item had became a list of comments, each dated and signed, with a box at
//! the bottom to add one; and beside it a list of the work agents did on the item, one record a
//! session. Both sit under the row folded away, each opened by a button of its own in the row -
//! the work button only where there is work to show - and both go to History when the item closes
//! and come back with it, which `a_closed_item_can_be_reopened` checks the other way round.

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

/// This text in a file of its own, the clock stopped.
fn a_project(tag: &str, text: &str) -> String {
    let root = std::env::temp_dir().join(format!("overseer_project_talk_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("p.os");
    std::fs::write(&file, text).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let instant = chrono::DateTime::parse_from_rfc3339("2026-10-01T09:00:00Z").unwrap().with_timezone(&chrono::Utc);
    FormulaEvaluator::set_time_override(Some(instant));
    file.to_string_lossy().to_string()
}

fn opened(file: &str) -> Vec<OverseerNode> {
    app_api::load_document(std::fs::read_to_string(file).unwrap()).expect("open")
}

fn names_of(file: &str, address: &str) -> Vec<String> {
    overseer::addressing::name_path(&opened(file), address).unwrap_or_else(|| panic!("nothing at {}", address))
}

fn press(file: &str, address: &str, typed: &[(&str, &str)]) -> app_api::ResolvedUpdate {
    let typed = typed
        .iter()
        .map(|(at, text)| app_api::ValueWrite { node_path: names_of(file, at), value: OverseerValue::String(text.to_string()) })
        .collect();
    app_api::run_event_at(file, "p.os", names_of(file, address), "click".into(), "s", typed, Vec::new())
        .expect("the press was refused")
}

fn pick(file: &str, handle: &str, stage: &str) {
    let path = names_of(file, &format!("project/Items/[{}]/stage", handle));
    let value = app_api::ValueWrite { node_path: path.clone(), value: OverseerValue::String(stage.into()) };
    let answer = app_api::run_event_at(file, "p.os", path, "change".into(), "s", Vec::new(), vec![value]).expect("the pick");
    assert!(answer.wrote, "picking {} on {} wrote nothing", stage, handle);
}

/// The text of the field with this name somewhere under a node, not inside a button or a handler.
fn field_text(node: &OverseerNode, name: &str) -> Option<String> {
    for child in &node.children {
        if child.name == name {
            return match child.parameters.get("_computed_value").or_else(|| child.parameters.get("value")) {
                Some(OverseerValue::String(s)) | Some(OverseerValue::Timestamp(s)) => Some(s.clone()),
                Some(OverseerValue::Integer(i)) => Some(i.to_string()),
                Some(OverseerValue::Float(f)) => Some(f.to_string()),
                Some(OverseerValue::Boolean(b)) => Some(b.to_string()),
                other => Some(format!("{:?}", other)),
            };
        }
        if child.node_type != "on" && child.node_type != "button" {
            if let Some(found) = field_text(child, name) {
                return Some(found);
            }
        }
    }
    None
}

fn entries(file: &str, list: &str, fields: &[&str]) -> Vec<Vec<String>> {
    let nodes = opened(file);
    let list = overseer::addressing::find(&nodes, list).unwrap_or_else(|| panic!("no list at {}", list));
    list.children
        .iter()
        .map(|entry| fields.iter().map(|f| field_text(entry, f).unwrap_or_default()).collect())
        .collect()
}

fn said_on(file: &str, item: &str) -> Vec<Vec<String>> {
    entries(file, &format!("{}/Talk/Comments", item), &["at", "author", "body"])
}

fn param(file: &str, address: &str, name: &str) -> Option<OverseerValue> {
    let nodes = opened(file);
    let node = overseer::addressing::find(&nodes, address).unwrap_or_else(|| panic!("nothing at {}", address));
    node.parameters.get(&format!("_computed_{}", name)).or_else(|| node.parameters.get(name)).cloned()
}

#[test]
fn each_button_folds_its_own_part_of_the_item() {
    serialised(|| {
        let file = a_project("buttons", TEMPLATE);
        for (item, button, part) in [
            ("project/Items/[brace]", "talk", "Talk"),
            ("project/Items/[brace]", "work", "Agents"),
            ("project/History/[roundtrip]", "talk", "Talk"),
            ("project/History/[roundtrip]", "work", "Agents"),
        ] {
            let answer = press(&file, &format!("{}/{}", item, button), &[]);
            assert!(!answer.wrote, "{} on {} wrote the file", button, item);
            assert_eq!(answer.folds.len(), 1, "{} on {}: {:?}", button, item, answer.folds);
            assert!(
                answer.folds[0].address.ends_with(&format!("/{}", part)),
                "{} on {} folds {}",
                button,
                item,
                answer.folds[0].address
            );
            assert_eq!(answer.folds[0].how, overseer::actions::FoldHow::Toggle);
        }
    });
}

#[test]
fn the_buttons_say_whether_there_is_anything_behind_them() {
    serialised(|| {
        let file = a_project("shown", TEMPLATE);
        let grey = Some(OverseerValue::String("#6b7280".into()));
        assert_eq!(param(&file, "project/Items/[editor]/talk", "font-color"), grey, "nothing said reads grey");
        assert_ne!(param(&file, "project/Items/[brace]/talk", "font-color"), grey, "something said reads grey");
        let hidden = Some(OverseerValue::Boolean(true));
        assert_eq!(param(&file, "project/Items/[brace]/work", "hidden"), hidden, "a work button with no work");
        assert_ne!(param(&file, "project/History/[roundtrip]/work", "hidden"), hidden, "the work is out of reach");
    });
}

#[test]
fn a_comment_is_added_dated_and_signed_under_the_ones_before() {
    serialised(|| {
        let file = a_project("add", TEMPLATE);
        // editor has nothing said about it, so its whole conversation is new to the file.
        let answer = press(
            &file,
            "project/Items/[editor]/Talk/NewComment/add",
            &[("project/Items/[editor]/Talk/NewComment/body", "split it in **two**\nfirst the model")],
        );
        assert!(answer.wrote, "adding a comment wrote nothing");
        let said = said_on(&file, "project/Items/[editor]");
        assert_eq!(said.len(), 1, "{:?}", said);
        assert!(said[0][0].starts_with("2026-10-01T09:00:00"), "dated {}", said[0][0]);
        assert_eq!(said[0][1], "owner");
        assert_eq!(said[0][2], "split it in **two**\nfirst the model");

        press(
            &file,
            "project/Items/[brace]/Talk/NewComment/add",
            &[("project/Items/[brace]/Talk/NewComment/body", "seen again today")],
        );
        let said = said_on(&file, "project/Items/[brace]");
        assert_eq!(said.iter().map(|c| c[2].as_str()).collect::<Vec<_>>(), [
            "only when the value comes first; a body on its own is fine",
            "seen again today"
        ]);
    });
}

#[test]
fn an_empty_box_adds_nothing() {
    serialised(|| {
        let file = a_project("empty", TEMPLATE);
        let before = std::fs::read_to_string(&file).unwrap();
        let answer = press(&file, "project/Items/[editor]/Talk/NewComment/add", &[]);
        assert!(!answer.wrote, "an empty comment was written");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
    });
}

#[test]
fn finishing_carries_the_conversation_and_the_work_into_history() {
    serialised(|| {
        // brace, with a session of work recorded on it the way the loop records one.
        let worked = TEMPLATE.replacen(
            "                - points = 2\r\n                div Talk {",
            "                - points = 2\r\n                div Agents {\r\n                    list Work {\r\n                        - {\r\n                            - at = \"2026-09-30T12:00:00+00:00\"\r\n                            - kind = \"build\"\r\n                            - model = \"claude-sonnet (high)\"\r\n                            - moved = \"ready > testing\"\r\n                            - cost = 0.75\r\n                            - lines = 12\r\n                        }\r\n                    }\r\n                }\r\n                div Talk {",
            1,
        );
        assert_ne!(worked, TEMPLATE, "the template's brace is not where this expects it");
        let file = a_project("finish", &worked);
        let work = |item: &str| entries(&file, &format!("{}/Agents/Work", item), &["kind", "model", "moved", "cost", "lines", "turns"]);
        let did = work("project/Items/[brace]");
        assert_eq!(did, [["build", "claude-sonnet (high)", "ready > testing", "0.75", "12", "0"]]);
        let said = said_on(&file, "project/Items/[brace]");

        pick(&file, "brace", "finished");
        assert_eq!(said_on(&file, "project/History/[brace]"), said, "the conversation changed");
        assert_eq!(work("project/History/[brace]"), did, "the work changed");
        let written = std::fs::read_to_string(&file).unwrap();
        let history = &written[written.find("list History").unwrap()..];
        let record = &history[history.find("- handle = \"brace\"").expect("no record of brace")..];
        assert!(!record.contains("- turns = ") && !record.contains("- read = "), "a default was restated:\n{}", record);
    });
}
