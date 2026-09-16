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

/// The same document with its whole history in view.
///
/// `History` shows three days and leaves the rest uninstantiated, which is the point of the window
/// and a nuisance here: the days these tests read are the two oldest, chosen because between them
/// they exercise both ways of stating an amount. What is under test is how a record resolves, not
/// which day is on screen, so the window is opened rather than the tests being re-pinned to
/// whichever days happen to be newest - a pin that would come loose every time the bot logs a meal.
fn source_with_everything_in_view() -> String {
    let text = source();
    let windowed = "key=\"date\", keyPrecision=\"day\", window=3,";
    assert!(
        text.contains(windowed),
        "tracker_v2.os no longer windows its history the way this expected; if the window is gone          this helper can go with it"
    );
    text.replace(windowed, "key=\"date\", keyPrecision=\"day\",")
}

/// Mirrors opening the document in the app: the document's directory is registered, then
/// parse -> resolve -> preload mounts -> resolve again.
fn open() -> Vec<OverseerNode> {
    let host = examples_dir().join("tracker_v2.os");
    DocumentManager::set_current_document(Some(host.to_string_lossy().as_ref()));
    overseer::source_registry::SourceRegistry::reset();
    let (_rest, mut nodes) = parser::parse_document(&source_with_everything_in_view())
        .expect("tracker_v2.os should parse");
    resolver::resolve_document(&mut nodes);
    ActionExecutor::preload_mounts(&mut nodes);
    resolver::resolve_document(&mut nodes);
    nodes
}

/// Walks by name, looking through groups that exist only for layout - the same rule the rest
/// of the system follows, so a document that groups its fields does not need these paths
/// rewritten.
fn find<'a>(nodes: &'a [OverseerNode], path: &[&str]) -> Option<&'a OverseerNode> {
    fn child<'a>(node: &'a OverseerNode, name: &str) -> Option<&'a OverseerNode> {
        overseer::addressing::effective_children(node)
            .into_iter()
            .map(|(_, c)| c)
            .find(|c| c.name == name)
    }
    let mut cur = nodes.iter().find(|n| n.name == path[0])?;
    for seg in &path[1..] {
        cur = child(cur, seg)?;
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
        // Against the text `open` actually parsed, which is the document with its window opened.
        // That the document round-trips *as authored* - window and all - is checked where every
        // other save is, in app_load_save_cycle.rs.
        let original = source_with_everything_in_view();
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
        c.push("calories".to_string());
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
        c.push("calories".to_string());
        close(number(&nodes, &as_refs(&c)), 200.0, "latte calories");
    });
}

/// Totals must equal the sum of the day's records. Asserted against the document's own
/// contents rather than a remembered number, so logging another food does not break it.
#[test]
fn day_totals_sum_the_intake() {
    serialised(|| {
        let nodes = open();
        let history = find(&nodes, &["tracker_v2", "History"]).expect("History");

        for day in &history.children {
            let intake = day
                .children
                .iter()
                .find(|c| c.name == "intake")
                .expect("intake list");
            assert!(
                !intake.children.is_empty(),
                "{} should have records to sum",
                day.name
            );

            for macro_name in ["protein", "sugar", "salt"] {
                let summed: f64 = intake
                    .children
                    .iter()
                    .map(|meal| {
                        let p = vec![
                            "tracker_v2".to_string(),
                            "History".to_string(),
                            day.name.clone(),
                            "intake".to_string(),
                            meal.name.clone(),
                            "macros".to_string(),
                            macro_name.to_string(),
                        ];
                        number(&nodes, &as_refs(&p))
                    })
                    .sum();

                let total = vec![
                    "tracker_v2".to_string(),
                    "History".to_string(),
                    day.name.clone(),
                    "totals".to_string(),
                    macro_name.to_string(),
                ];
                close(
                    number(&nodes, &as_refs(&total)),
                    summed,
                    &format!("{} total {}", day.name, macro_name),
                );
            }
        }
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
fn the_add_button_appends_to_its_own_day() {
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
            "add_by_portions".to_string(),
        ];
        ActionExecutor::execute_event(&mut nodes, &path, "click").expect("add click");

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
            "add_by_portions".to_string(),
        ];
        ActionExecutor::execute_event(&mut nodes, &log, "click").expect("add a food");
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
        // Compare against the authored line rather than a literal, so the assertion tracks
        // the document instead of a remembered parameter order.
        let mount_line = |text: &str| {
            text.lines()
                .find(|l| l.contains("mount FOODS"))
                .map(|l| l.to_string())
                .expect("mount declaration")
        };
        assert_eq!(
            mount_line(&saved),
            mount_line(&original),
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

/// Each Add button must write only its own amount, leaving the other to derive. If both were
/// written, the derived one would be pinned to whatever the draft happened to hold.
#[test]
fn each_add_button_stores_only_its_own_amount() {
    serialised(|| {
        for (button, stored, derived) in [
            ("add_by_portions", "portions", "grams"),
            ("add_by_grams", "grams", "portions"),
        ] {
            let mut nodes = open();
            let day = day_with(&nodes, "2026-08-04");
            let path = vec![
                "tracker_v2".to_string(),
                "History".to_string(),
                day.clone(),
                button.to_string(),
            ];
            ActionExecutor::execute_event(&mut nodes, &path, "click")
                .unwrap_or_else(|e| panic!("{} click failed: {:?}", button, e));

            let added = find(&nodes, &["tracker_v2", "History", day.as_str(), "intake"])
                .expect("intake")
                .children
                .last()
                .expect("appended record")
                .clone();

            let raw = |name: &str| {
                overseer::addressing::effective_children(&added)
                    .into_iter()
                    .map(|(_, c)| c)
                    .find(|c| c.name == name)
                    .and_then(|c| c.parameters.get("value").cloned())
            };
            assert!(
                !matches!(raw(stored), None | Some(OverseerValue::Null)),
                "{}: {} should be stored, got {:?}",
                button,
                stored,
                raw(stored)
            );
            assert!(
                matches!(raw(derived), None | Some(OverseerValue::Null)),
                "{}: {} should be left to derive, but was stored as {:?}",
                button,
                derived,
                raw(derived)
            );

            // Both must still read as numbers - the derived one through its fallback.
            let base = vec![
                "tracker_v2".to_string(),
                "History".to_string(),
                day.clone(),
                "intake".to_string(),
                added.name.clone(),
            ];
            for field in ["portions", "grams"] {
                let mut p = base.clone();
                p.push(field.to_string());
                let v = number(&nodes, &as_refs(&p));
                assert!(
                    v > 0.0,
                    "{}: {} should resolve to a positive number, got {}",
                    button,
                    field,
                    v
                );
            }
        }
    });
}

/// A day record must contain exactly the fields it declares - nothing else.
///
/// A bare `//` line inside a block makes the parser read the surrounding comment text as
/// DSL, one node per word. Those nodes are invisible in the source but real in the tree:
/// they render as a run of empty blocks after the day's contents, and one of them is named
/// `intake` (from a comment mentioning that path), which an `append (list="../intake")` can
/// resolve to instead of the actual list - so the Add buttons silently do nothing.
#[test]
fn a_day_record_has_no_stray_children() {
    serialised(|| {
        let nodes = open();
        let history = find(&nodes, &["tracker_v2", "History"]).expect("History");

        let expected = [
            "date",
            // The previous recorded day, and what this day is aiming at. Both are declared on
            // DayRecord, so both belong here; the point of this test is the children nobody
            // declared.
            "previous_day",
            "targets",
            "totals",
            "intake",
            "add_by_portions",
            "add_by_grams",
            // The day's own Nutri-Score, worked out from its totals. Declared here rather than
            // instantiated from the shared block - see the ignored test in
            // tests/comment_after_childless_list.rs for why.
            "eaten_grams",
            "day_kj",
            "day_sugar",
            "day_sat_fat",
            "day_sodium_mg",
            "day_fibre",
            "day_protein",
            "day_p_energy",
            "day_p_sugar",
            "day_p_sat_fat",
            "day_p_sodium",
            "day_p_fibre",
            "day_p_protein",
            "day_score",
            // The headline group is named, so it stands for itself and appears here; the
            // grade moved inside it.
            "day_headline",
        ];
        for day in &history.children {
            // Through layout groups: this is looking for nodes nobody declared, and a `div`
            // arranging the day's summary is not one.
            let actual: Vec<String> = overseer::addressing::effective_children(day)
                .into_iter()
                .map(|(_, c)| c.name.clone())
                .collect();
            let stray: Vec<&String> = actual
                .iter()
                .filter(|n| !expected.contains(&n.as_str()))
                .collect();
            assert!(
                stray.is_empty(),
                "{} has {} stray children parsed out of comment text: {:?}",
                day.name,
                stray.len(),
                stray
            );
            assert_eq!(
                actual.iter().filter(|n| *n == "intake").count(),
                1,
                "{} should have exactly one `intake` child, found {:?}",
                day.name,
                actual
            );
        }
    });
}

/// The template itself must be clean too, since every day is instantiated from it.
#[test]
fn the_meal_record_template_has_no_stray_children() {
    serialised(|| {
        let nodes = open();
        let history = find(&nodes, &["tracker_v2", "History"]).expect("History");
        let day = history.children.first().expect("a day");
        let intake = day
            .children
            .iter()
            .find(|c| c.name == "intake")
            .expect("intake list");
        let meal = intake.children.first().expect("a meal record");

        // `quality` is the food's Nutri-Score block and `calories` its headline figure, both
        // declared on the template like the rest.
        // `at` is when it was eaten: the clock only, since the day is the entry it sits in.
        // `labels` is what the food is - vegan, dairy, alcohol - read from the catalogue rather
        // than stored here, like `name` beside it.
        let expected = [
            "food", "portion_weight", "at", "portions", "grams", "name", "labels", "calories",
            "quality", "macros",
        ];
        // Through any layout grouping: what this is looking for is a node nobody declared,
        // and a `div` used to arrange the fields is not one.
        let stray: Vec<String> = overseer::addressing::effective_children(meal)
            .into_iter()
            .map(|(_, c)| c.name.clone())
            .filter(|n| !expected.contains(&n.as_str()))
            .collect();
        assert!(
            stray.is_empty(),
            "meal record has stray children: {:?}",
            stray
        );
    });
}
