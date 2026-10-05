//! A project item moves through stages, and finishing is one of them.
//!
//! Every project has the same stages - filed, discuss, asked, ready, testing, later - and
//! `finished`, which is not a place an item stays: picking it records the item in History and
//! takes it off the list, which is what a finish button used to do. Checked against the template
//! every project is a copy of.
//!
//! The page sends the value and the field's `on change` as one change. As two, taking back the
//! second left the first standing, and a task picked as finished came back on the list saying
//! finished - so one pick is one step to take back, and taking it back puts the stage back too.

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

/// The template in a file of its own, and the folder it is in.
fn a_project(tag: &str) -> (std::path::PathBuf, String) {
    let root = std::env::temp_dir().join(format!("overseer_stages_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("p.os");
    std::fs::write(&file, TEMPLATE).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let instant = chrono::DateTime::parse_from_rfc3339("2026-10-05T09:00:00Z").unwrap().with_timezone(&chrono::Utc);
    FormulaEvaluator::set_time_override(Some(instant));
    (root, file.to_string_lossy().to_string())
}

fn on_disk(file: &str) -> String {
    std::fs::read_to_string(file).unwrap()
}

/// The handles in a list, in the order the file has them.
fn handles_in(text: &str, list: &str) -> Vec<String> {
    let start = text.find(&format!("\n    list {} (", list)).expect(list);
    let body = &text[start + 1..];
    let end = body.find("\n    }").unwrap_or(body.len());
    body[..end]
        .lines()
        .filter_map(|l| l.trim().strip_prefix("- handle = \"").map(|h| h.trim_end_matches('"').to_string()))
        .collect()
}

/// The lines of one entry, by its handle.
fn entry_lines(text: &str, handle: &str) -> Vec<String> {
    let at = text.find(&format!("- handle = \"{}\"", handle)).expect(handle);
    let open = text[..at].rfind("- {").expect("the entry's opening");
    let close = at + text[at..].find("\n        }").expect("the entry's end");
    text[open + 3..close].lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()
}

/// Pick a stage for an open item, as the page does.
fn pick(file: &str, handle: &str, stage: &str) -> app_api::ResolvedUpdate {
    let nodes = app_api::load_document(on_disk(file)).expect("open");
    let path = overseer::addressing::name_path(&nodes, &format!("project/Items/[{}]/stage", handle))
        .unwrap_or_else(|| panic!("no stage on {}", handle));
    let value = app_api::ValueWrite { node_path: path.clone(), value: OverseerValue::String(stage.into()) };
    app_api::run_event_at(file, "p.os", path, "change".into(), "s", Vec::new(), vec![value])
        .expect("the pick was refused")
}

#[test]
fn picking_finished_records_the_item_and_takes_it_off_the_list() {
    serialised(|| {
        let (_root, file) = a_project("finish");
        assert!(handles_in(&on_disk(&file), "Items").contains(&"brace".to_string()));
        assert!(pick(&file, "brace", "finished").wrote, "finishing wrote nothing");

        let text = on_disk(&file);
        assert!(!handles_in(&text, "Items").contains(&"brace".to_string()), "still open:\n{}", text);
        assert!(handles_in(&text, "History").contains(&"brace".to_string()), "not recorded:\n{}", text);
        let record = entry_lines(&text, "brace");
        for field in ["- finished_at = ", "- title = ", "- parent = ", "- points = ", "- commentary = "] {
            assert!(record.iter().any(|l| l.starts_with(field)), "the record has no {}: {:?}", field, record);
        }
        assert!(!record.iter().any(|l| l.starts_with("- stage = ")), "the record kept a stage: {:?}", record);
    });
}

#[test]
fn one_undo_takes_a_finish_back_whole() {
    serialised(|| {
        let (root, file) = a_project("undo");
        let before = entry_lines(&on_disk(&file), "brace");
        let steps = overseer::undo::depth(&root, "p.os");
        pick(&file, "brace", "finished");
        assert_eq!(overseer::undo::depth(&root, "p.os"), steps + 1, "a pick was more than one step");

        app_api::undo_document(&file).expect("undo");
        let text = on_disk(&file);
        assert!(handles_in(&text, "Items").contains(&"brace".to_string()), "not back on the list:\n{}", text);
        assert!(!handles_in(&text, "History").contains(&"brace".to_string()), "still recorded:\n{}", text);
        assert_eq!(entry_lines(&text, "brace"), before, "it came back other than it was");
    });
}

#[test]
fn a_task_with_children_open_is_not_finished() {
    // The picker does not offer it - see project_rollup - and the handler asks the same, for
    // anything that sets the stage some other way.
    serialised(|| {
        let (_root, file) = a_project("parent");
        let open = handles_in(&on_disk(&file), "Items");
        pick(&file, "editor", "finished");
        let text = on_disk(&file);
        assert_eq!(handles_in(&text, "Items"), open, "finished with children open:\n{}", text);
        assert!(!handles_in(&text, "History").contains(&"editor".to_string()));
    });
}

#[test]
fn any_other_stage_is_written_where_the_template_has_it() {
    serialised(|| {
        let (_root, file) = a_project("ready");
        pick(&file, "brace", "ready");
        let text = on_disk(&file);
        assert!(handles_in(&text, "Items").contains(&"brace".to_string()));
        let lines = entry_lines(&text, "brace");
        let title = lines.iter().position(|l| l.starts_with("- title = ")).expect("a title");
        assert_eq!(lines.get(title + 1).map(String::as_str), Some("- stage = \"ready\""), "{:?}", lines);
    });
}

#[test]
fn every_project_offers_the_same_stages_in_order() {
    let nodes = app_api::load_document(TEMPLATE.to_string()).expect("open");
    let stages = overseer::addressing::find(&nodes, "project/Stages").expect("a Stages list");
    let tags: Vec<String> = stages
        .children
        .iter()
        .filter_map(|e| e.children.iter().find(|c| c.name == "tag"))
        .filter_map(|c| match c.parameters.get("value") {
            Some(OverseerValue::String(s)) => Some(s.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(tags, ["filed", "discuss", "asked", "ready", "testing", "later", "finished"]);
}
