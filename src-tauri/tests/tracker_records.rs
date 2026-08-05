//! Handle-based consumed-food records: `examples/weight_tracker/tracker_v2.os`.
//!
//! A record stores a food handle and one of portions/grams. Everything shown - the food's
//! name, its nine macros, and the day's totals - is derived by looking the handle up in
//! `foods.os` through a preloaded mount. These tests check the arithmetic end to end, since
//! a lookup that silently resolves to nothing would show up as plausible-looking zeroes.

use overseer::actions::ActionExecutor;
use overseer::docmgr::manager::DocumentManager;
use overseer::file_ops::OverseerFileHandler;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::parser;
use overseer::resolver;
use overseer::types::{OverseerNode, OverseerValue};

/// These tests set a process-wide document directory and reset the global SourceRegistry.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

fn examples_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/weight_tracker")
}

fn source() -> String {
    let p = examples_dir().join("tracker_v2.os");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("failed to read {:?}: {}", p, e))
}

/// Mirrors opening the document in the app: the document's directory is registered, then
/// parse -> resolve -> preload mounts -> resolve again.
fn open() -> Vec<OverseerNode> {
    let host = examples_dir().join("tracker_v2.os");
    DocumentManager::set_current_document(Some(host.to_string_lossy().as_ref()));
    overseer::source_registry::SourceRegistry::reset();
    let (_rest, mut nodes) = parser::parse_document(&source()).expect("tracker_v2.os should parse");
    resolver::resolve_document(&mut nodes);
    ActionExecutor::preload_mounts(&mut nodes);
    resolver::resolve_document(&mut nodes);
    nodes
}

fn find<'a>(nodes: &'a [OverseerNode], path: &[&str]) -> Option<&'a OverseerNode> {
    let mut cur = nodes.iter().find(|n| n.name == path[0])?;
    for seg in &path[1..] {
        cur = cur.children.iter().find(|c| &c.name == seg)?;
    }
    Some(cur)
}

fn value(nodes: &[OverseerNode], path: &[&str]) -> OverseerValue {
    let n = find(nodes, path).unwrap_or_else(|| panic!("missing node {}", path.join("/")));
    let owned: Vec<String> = path.iter().map(|s| s.to_string()).collect();
    FormulaEvaluator::get_effective_value_for_node(n, &owned, nodes)
        .unwrap_or_else(|e| panic!("{} failed to evaluate: {:?}", path.join("/"), e))
}

fn number(nodes: &[OverseerNode], path: &[&str]) -> f64 {
    match value(nodes, path) {
        OverseerValue::Float(f) => f,
        OverseerValue::Integer(i) => i as f64,
        other => panic!("{} is not a number: {:?}", path.join("/"), other),
    }
}

fn close(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() < 0.05,
        "{}: expected {}, got {}",
        what,
        expected,
        actual
    );
}

/// The day whose records exercise both amount styles.
fn day_with(nodes: &[OverseerNode], date: &str) -> String {
    let history = find(nodes, &["tracker_v2", "History"]).expect("History");
    for child in &history.children {
        if let Some(d) = child.children.iter().find(|c| c.name == "date") {
            if let Some(OverseerValue::String(s)) = d.parameters.get("value") {
                if s == date {
                    return child.name.clone();
                }
            }
        }
    }
    panic!("no history entry for {}", date);
}

fn meal<'a>(day: &'a str, index: usize) -> Vec<String> {
    vec![
        "tracker_v2".to_string(),
        "History".to_string(),
        day.to_string(),
        "intake".to_string(),
        format!("MealRecord__{}", index),
    ]
}

fn as_refs(v: &[String]) -> Vec<&str> {
    v.iter().map(|s| s.as_str()).collect()
}

#[test]
fn document_parses_and_round_trips() {
    serialised(|| {
        let original = source();
        let nodes = open();
        let out = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        assert_eq!(out, original, "tracker_v2.os did not round-trip byte-for-byte");
    });
}

#[test]
fn a_record_resolves_its_food_through_the_mount() {
    serialised(|| {
        let nodes = open();
        let day = day_with(&nodes, "2026-08-04");
        let mut p = meal(&day, 1);
        p.push("name".to_string());
        assert_eq!(
            value(&nodes, &as_refs(&p)),
            OverseerValue::String("Chicken Wrap".to_string()),
            "the record's handle should resolve to the catalogued food name"
        );
    });
}

#[test]
fn stating_portions_derives_grams_and_macros() {
    serialised(|| {
        let nodes = open();
        let day = day_with(&nodes, "2026-08-04");

        // Chicken wrap: 1 portion of 300 g at 137 kcal/100 g.
        let base = meal(&day, 1);
        let mut g = base.clone();
        g.push("grams".to_string());
        close(number(&nodes, &as_refs(&g)), 300.0, "wrap grams from portions");

        let mut c = base.clone();
        c.extend(["macros".to_string(), "calories".to_string()]);
        close(number(&nodes, &as_refs(&c)), 411.0, "wrap calories");

        let mut pr = base;
        pr.extend(["macros".to_string(), "protein".to_string()]);
        close(number(&nodes, &as_refs(&pr)), 27.0, "wrap protein");
    });
}

#[test]
fn stating_grams_derives_portions_and_macros() {
    serialised(|| {
        let nodes = open();
        let day = day_with(&nodes, "2026-08-04");

        // Coffee latte: 250 g stated, portion weight 250 g, so exactly one portion.
        let base = meal(&day, 2);
        let mut p = base.clone();
        p.push("portions".to_string());
        close(number(&nodes, &as_refs(&p)), 1.0, "latte portions from grams");

        // 250 g at 80 kcal/100 g.
        let mut c = base;
        c.extend(["macros".to_string(), "calories".to_string()]);
        close(number(&nodes, &as_refs(&c)), 200.0, "latte calories");
    });
}

#[test]
fn day_totals_sum_the_intake() {
    serialised(|| {
        let nodes = open();
        let day = day_with(&nodes, "2026-08-04");

        // wrap 411 + latte 200 + apple (1 portion of 180 g at 52 kcal/100 g = 93.6)
        let p = [
            "tracker_v2",
            "History",
            day.as_str(),
            "totals",
            "calories",
        ];
        close(number(&nodes, &p), 704.6, "day total calories");

        let p = ["tracker_v2", "History", day.as_str(), "totals", "sugar"];
        // wrap 3.9 + latte 12.5 + apple 18.72
        close(number(&nodes, &p), 35.12, "day total sugar");
    });
}

/// A record is only a handle plus one amount - that is the whole point of the shape.
#[test]
fn records_stay_minimal_on_disk() {
    serialised(|| {
        let text = source();
        let start = text.find("- food = \"chicken_wrap\"").expect("wrap record");
        let after = &text[start..];
        let end = after.find('}').expect("record close");
        let body = &after[..end];
        let stored: Vec<&str> = body
            .lines()
            .map(|l| l.trim())
            .filter(|l| l.starts_with('-'))
            .collect();
        assert_eq!(
            stored.len(),
            2,
            "a consumed-food record should store exactly a handle and one amount, found {:?}",
            stored
        );
    });
}

/// Logging must append to the day whose button was pressed, with the drafted values.
#[test]
fn the_log_button_appends_to_its_own_day() {
    serialised(|| {
        let mut nodes = open();
        let day = day_with(&nodes, "2026-08-03");
        let other = day_with(&nodes, "2026-08-04");

        let count = |nodes: &[OverseerNode], d: &str| {
            find(nodes, &["tracker_v2", "History", d, "intake"])
                .expect("intake list")
                .children
                .len()
        };
        let before_target = count(&nodes, &day);
        let before_other = count(&nodes, &other);

        let path = vec![
            "tracker_v2".to_string(),
            "History".to_string(),
            day.clone(),
            "log".to_string(),
        ];
        ActionExecutor::execute_event(&mut nodes, &path, "click").expect("log click");

        assert_eq!(
            count(&nodes, &day),
            before_target + 1,
            "the pressed day should gain one record"
        );
        assert_eq!(
            count(&nodes, &other),
            before_other,
            "no other day should be touched"
        );

        // And the appended record must carry the drafted handle, not a template default.
        let added = find(&nodes, &["tracker_v2", "History", day.as_str(), "intake"])
            .unwrap()
            .children
            .last()
            .expect("appended record")
            .clone();
        let handle = added
            .children
            .iter()
            .find(|c| c.name == "food")
            .and_then(|f| f.parameters.get("value").cloned());
        assert_eq!(
            handle,
            Some(OverseerValue::String("apple".to_string())),
            "appended record should carry the drafted food handle, got {:?}",
            handle
        );
    });
}

/// The scenario that corrupted the document: open it, click Prev day, Next day, log a food,
/// then save. Every step crosses the IPC boundary, so the serializer works from registry
/// lookups rather than in-process snapshots - which is where the mounted catalog used to
/// overwrite the host.
#[test]
fn opening_acting_and_saving_leaves_the_document_intact() {
    serialised(|| {
        let original = source();

        let mut nodes = open();
        // Adopting the backend's response drops snapshots and keeps ids, as Tauri does.
        let across_ipc = |nodes: &Vec<OverseerNode>| -> Vec<OverseerNode> {
            let json = serde_json::to_string(nodes).expect("to json");
            serde_json::from_str(&json).expect("from json")
        };
        nodes = across_ipc(&nodes);

        for (path, event) in [
            (vec!["tracker_v2", "Selected", "Prev"], "click"),
            (vec!["tracker_v2", "Selected", "Next"], "click"),
        ] {
            let owned: Vec<String> = path.iter().map(|s| s.to_string()).collect();
            ActionExecutor::execute_event(&mut nodes, &owned, event).expect("day navigation");
            nodes = across_ipc(&nodes);
        }

        let day = day_with(&nodes, "2026-08-04");
        let log = vec![
            "tracker_v2".to_string(),
            "History".to_string(),
            day,
            "log".to_string(),
        ];
        ActionExecutor::execute_event(&mut nodes, &log, "click").expect("log a food");
        nodes = across_ipc(&nodes);

        let saved = OverseerFileHandler::serialize_nodes(&nodes).expect("save");

        // None of the mounted catalog may appear in the tracker.
        for marker in [
            "Food catalog: the single source",
            "list Catalog",
            "- handle = \"potato_salad\"",
            "div per_100g",
        ] {
            assert!(
                !saved.contains(marker),
                "the mounted catalog leaked into the tracker ({:?}):\n{}",
                marker,
                saved
            );
        }

        // The tracker's own structure and comments must survive.
        assert!(
            saved.contains("tab tracker_v2 (label=\"Calories\", mutable=true) {"),
            "the tracker's root node was lost:\n{}",
            saved
        );
        assert!(
            saved.contains("// Calorie tracker, handle-based records."),
            "the tracker's own leading comment was lost"
        );
        assert!(
            saved.contains("// The catalogued weight of one portion of this food"),
            "an interior comment was lost"
        );
        assert!(
            saved.contains("mount FOODS (source=\"foods.os/food_catalog/Catalog\""),
            "the mount declaration was rewritten"
        );

        // The only intended change is the logged record, so the document should differ from
        // its original by an added meal and nothing else.
        let added_lines = saved.lines().count() as i64 - original.lines().count() as i64;
        assert!(
            (0..=6).contains(&added_lines),
            "expected only a logged record to be added, line count moved by {}:\n{}",
            added_lines,
            saved
        );
    });
}
