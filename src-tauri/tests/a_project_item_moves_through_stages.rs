//! A project item moves through stages, and finishing is one of them.
//!
//! Every project has the same stages - filed, planning, ready, testing, later - and
//! `finished`, which is not a place an item stays: picking it records the item in History and
//! takes it off the list, which is what a finish button used to do. `cancelled` closes it the
//! same way, with the record's status saying so, and a cancelled part counts towards nothing.
//! How hard an item is to think through - its complexity - goes with it onto the record.
//! Checked against the template every project is a copy of.
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

/// The handles in a list, in the order the file has them. Found at whatever depth the template
/// puts the list - Items and History each sit in a tab of their own - and ended by the brace at
/// the list's own indent.
fn handles_in(text: &str, list: &str) -> Vec<String> {
    let start = text.find(&format!("list {} (", list)).expect(list);
    let line = text[..start].rfind('\n').map_or(0, |n| n + 1);
    let close = format!("\n{}}}", &text[line..start]);
    let body = &text[start..];
    let end = body.find(&close).unwrap_or(body.len());
    body[..end]
        .lines()
        .filter_map(|l| l.trim().strip_prefix("- handle = \"").map(|h| h.trim_end_matches('"').to_string()))
        .collect()
}

/// The lines of one entry, by its handle: an entry's fields hold no braces, so the first line
/// that is only one closes it.
fn entry_lines(text: &str, handle: &str) -> Vec<String> {
    let at = text.find(&format!("- handle = \"{}\"", handle)).expect(handle);
    let open = text[..at].rfind("- {").expect("the entry's opening");
    text[open + 3..]
        .lines()
        .map(|l| l.trim().to_string())
        .take_while(|l| l != "}")
        .filter(|l| !l.is_empty())
        .collect()
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

/// What a field of the document reads once it is worked out, by its address.
fn value_at(file: &str, address: &str) -> OverseerValue {
    let nodes = app_api::load_document(on_disk(file)).expect("open");
    let node = overseer::addressing::find(&nodes, address).unwrap_or_else(|| panic!("nothing at {}", address));
    node.parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
        .cloned()
        .unwrap_or_else(|| panic!("no value at {}", address))
}

fn number_at(file: &str, address: &str) -> f64 {
    match value_at(file, address) {
        OverseerValue::Integer(i) => i as f64,
        OverseerValue::Float(f) => f,
        other => panic!("{} reads {:?}", address, other),
    }
}

#[test]
fn picking_cancelled_closes_the_item_marked_cancelled() {
    serialised(|| {
        let (_root, file) = a_project("cancel");
        assert!(pick(&file, "brace", "cancelled").wrote, "cancelling wrote nothing");

        let text = on_disk(&file);
        assert!(!handles_in(&text, "Items").contains(&"brace".to_string()), "still open:\n{}", text);
        assert!(handles_in(&text, "History").contains(&"brace".to_string()), "not recorded:\n{}", text);
        let record = entry_lines(&text, "brace");
        assert!(record.contains(&"- status = \"cancelled\"".to_string()), "not marked cancelled: {:?}", record);
        assert!(!record.iter().any(|l| l.starts_with("- stage = ")), "the record kept a stage: {:?}", record);
    });
}

#[test]
fn a_record_that_does_not_say_reads_finished() {
    // Everything closed before cancelled existed has no status, and it was all finished.
    serialised(|| {
        let (_root, file) = a_project("old");
        assert_eq!(value_at(&file, "project/History/[escape]/status"), OverseerValue::String("finished".into()));
        pick(&file, "brace", "finished");
        assert_eq!(value_at(&file, "project/History/[brace]/status"), OverseerValue::String("finished".into()));
    });
}

#[test]
fn a_cancelled_part_counts_as_neither_work_done_nor_work_left() {
    // parser holds brace, open, and roundtrip, finished. Cancelling brace leaves roundtrip the
    // whole of it: one part, all done, and no points won that were not won before.
    serialised(|| {
        let (_root, file) = a_project("rollup");
        let won = number_at(&file, "project/points_won");
        assert_eq!(number_at(&file, "project/Items/[parser]/kids"), 2.0);
        pick(&file, "brace", "cancelled");
        assert_eq!(number_at(&file, "project/Items/[parser]/kids"), 1.0);
        assert_eq!(number_at(&file, "project/Items/[parser]/open_kids"), 0.0);
        assert_eq!(number_at(&file, "project/Items/[parser]/done"), 100.0);
        assert_eq!(number_at(&file, "project/points_won"), won, "cancelled work counted as won");
    });
}

#[test]
fn a_task_with_children_open_is_not_cancelled() {
    serialised(|| {
        let (_root, file) = a_project("cancelparent");
        let open = handles_in(&on_disk(&file), "Items");
        pick(&file, "editor", "cancelled");
        let text = on_disk(&file);
        assert_eq!(handles_in(&text, "Items"), open, "cancelled with children open:\n{}", text);
        assert!(!handles_in(&text, "History").contains(&"editor".to_string()));
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
    assert_eq!(tags, ["filed", "planning", "ready", "testing", "later", "finished", "cancelled"]);
}

/// Set a field of an open item, as the page does when a chip is picked.
fn set(file: &str, handle: &str, field: &str, value: &str) {
    let nodes = app_api::load_document(on_disk(file)).expect("open");
    let path = overseer::addressing::name_path(&nodes, &format!("project/Items/[{}]/{}", handle, field))
        .unwrap_or_else(|| panic!("no {} on {}", field, handle));
    let write = app_api::ValueWrite { node_path: path.clone(), value: OverseerValue::String(value.into()) };
    app_api::run_event_at(file, "p.os", path, "change".into(), "s", Vec::new(), vec![write])
        .expect("the change was refused");
}

#[test]
fn an_item_not_yet_judged_reads_unassigned() {
    serialised(|| {
        let (_root, file) = a_project("unjudged");
        assert_eq!(value_at(&file, "project/Items/[brace]/complexity"), OverseerValue::String("unassigned".into()));
        assert_eq!(value_at(&file, "project/History/[escape]/complexity"), OverseerValue::String("unassigned".into()));
    });
}

#[test]
fn an_items_complexity_is_written_after_its_points_and_kept_on_the_record() {
    serialised(|| {
        let (_root, file) = a_project("complexity");
        set(&file, "brace", "complexity", "medium");
        let lines = entry_lines(&on_disk(&file), "brace");
        let points = lines.iter().position(|l| l.starts_with("- points = ")).expect("points");
        assert_eq!(lines.get(points + 1).map(String::as_str), Some("- complexity = \"medium\""), "{:?}", lines);

        pick(&file, "brace", "finished");
        let record = entry_lines(&on_disk(&file), "brace");
        assert!(record.contains(&"- complexity = \"medium\"".to_string()), "the record lost it: {:?}", record);
        assert_eq!(value_at(&file, "project/History/[brace]/complexity"), OverseerValue::String("medium".into()));
    });
}

#[test]
fn every_project_has_the_same_complexity_scale() {
    let nodes = app_api::load_document(TEMPLATE.to_string()).expect("open");
    let scale = overseer::addressing::find(&nodes, "project/Complexities").expect("a Complexities list");
    let tags: Vec<String> = scale
        .children
        .iter()
        .filter_map(|e| e.children.iter().find(|c| c.name == "tag"))
        .filter_map(|c| match c.parameters.get("value") {
            Some(OverseerValue::String(s)) => Some(s.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(tags, ["unassigned", "trivial", "low", "medium", "high"]);
}
