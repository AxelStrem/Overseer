//! When a task rule says it is due.
//!
//! The rules are the whole of the scheduler's judgement - the sweep that acts on them only
//! reads `due` and appends. So every question worth asking about when a task appears is a
//! question about this document, and gets asked here rather than against a running bot.

use overseer::app_api;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::types::{OverseerNode, OverseerValue};

const DOC: &str = "../examples/tasks/tasks.os";

/// The document as it stands at a given instant, with the named rule's fields overridden.
fn rules_at(instant: &str, handle: &str, overrides: &[(&str, &str)]) -> Vec<OverseerNode> {
    let when = chrono::DateTime::parse_from_rfc3339(instant)
        .expect("bad instant")
        .with_timezone(&chrono::Utc);
    FormulaEvaluator::set_time_override(Some(when));

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(DOC);
    let mut text = std::fs::read_to_string(path).expect("no tasks.os");

    // Rewrite the named rule's entry in place: find its `- handle = "x"` line and append the
    // overrides to that block. Cheaper and clearer than assembling a fixture that would drift
    // from the real document.
    let anchor = format!("            - handle = \"{handle}\"\n");
    assert!(text.contains(&anchor), "no rule {handle} in the document");
    let extra: String = overrides
        .iter()
        .map(|(k, v)| format!("            - {k} = {v}\n"))
        .collect();
    text = text.replace(&anchor, &format!("{anchor}{extra}"));

    let nodes = app_api::load_document(text).expect("load");
    FormulaEvaluator::set_time_override(None);
    nodes
}

fn field(nodes: &[OverseerNode], name: &str) -> Option<OverseerValue> {
    for n in nodes {
        if n.name == name {
            return n
                .parameters
                .get("_computed_value")
                .or(n.parameters.get("value"))
                .cloned();
        }
        if let Some(v) = field(&n.children, name) {
            return Some(v);
        }
    }
    None
}

/// Whether the named rule considers itself due.
fn due(nodes: &[OverseerNode], handle: &str) -> bool {
    fn list<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
        for n in nodes {
            if n.name == name {
                return Some(n);
            }
            if let Some(f) = list(&n.children, name) {
                return Some(f);
            }
        }
        None
    }
    let rules = list(nodes, "Rules").expect("no rules");
    for entry in &rules.children {
        let own = field(std::slice::from_ref(entry), "handle")
            .map(|v| format!("{v:?}"))
            .unwrap_or_default();
        if own.contains(handle) {
            return match field(std::slice::from_ref(entry), "due") {
                Some(OverseerValue::Boolean(b)) => b,
                other => panic!("rule {handle} has no boolean `due`: {other:?}"),
            };
        }
    }
    panic!("no rule {handle}");
}

// -- calendar rules --------------------------------------------------------------------------

#[test]
fn a_weekly_rule_fires_on_its_weekday_and_no_other() {
    // `bins` is weekday 2, Tuesday. 2026-08-11 is a Tuesday.
    assert!(due(&rules_at("2026-08-11T09:00:00Z", "bins", &[]), "bins"));
    assert!(!due(&rules_at("2026-08-12T09:00:00Z", "bins", &[]), "bins"));
    assert!(!due(&rules_at("2026-08-10T09:00:00Z", "bins", &[]), "bins"));
}

#[test]
fn a_monthly_rule_fires_on_its_day_of_the_month() {
    assert!(due(&rules_at("2026-09-01T09:00:00Z", "rent", &[]), "rent"));
    assert!(!due(&rules_at("2026-09-02T09:00:00Z", "rent", &[]), "rent"));
    // Every month, not every 30 days.
    assert!(due(&rules_at("2027-02-01T09:00:00Z", "rent", &[]), "rent"));
}

#[test]
fn a_calendar_rule_does_not_fire_twice_in_one_day() {
    let already = [("last_created", "\"2026-08-11T06:00:00Z\"")];
    assert!(!due(&rules_at("2026-08-11T09:00:00Z", "bins", &already), "bins"));
    // ...and is due again the next time the day comes round.
    assert!(due(&rules_at("2026-08-18T09:00:00Z", "bins", &already), "bins"));
}

#[test]
fn a_missed_occasion_is_skipped_rather_than_queued() {
    // `kitchen_floor` has an open task and has never run, so `never_ran` would fire it. The
    // open one is the only thing stopping it, which is exactly the behaviour under test.
    let nodes = rules_at("2026-08-18T09:00:00Z", "kitchen_floor", &[]);
    assert!(
        !due(&nodes, "kitchen_floor"),
        "a rule fired while its own task was still open"
    );
}

#[test]
fn duplicates_are_allowed_when_the_rule_says_so() {
    let allow = [("allow_duplicates", "true"), ("mode", "\"daily\"")];
    assert!(
        due(&rules_at("2026-08-18T09:00:00Z", "kitchen_floor", &allow), "kitchen_floor"),
        "an open task blocked a rule that permits duplicates"
    );
}

// -- interval rules --------------------------------------------------------------------------

#[test]
fn a_rule_that_never_ran_fires_once_straight_away() {
    // `water_plants` has never been created and its trigger has never been completed. It
    // should not wait out an interval it has nothing to count from.
    assert!(due(&rules_at("2026-08-11T09:00:00Z", "water_plants", &[]), "water_plants"));
}

#[test]
fn an_interval_rule_waits_out_its_interval_after_the_trigger_was_done() {
    // The floor was washed on the 11th; the plants want watering 3 days later.
    let history = |done: &str| {
        format!(
            "    list History (entry=<Record>, layout=\"vertical\") {{
        - {{
            - done_at = \"{done}\"
            - rule = \"kitchen_floor\"
            - title = \"Wash the kitchen floor\"
            - difficulty = 25
        }}
    }}"
        )
    };
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(DOC);
    let base = std::fs::read_to_string(path).expect("no tasks.os");
    let empty = "    list History (entry=<Record>, layout=\"vertical\") { }";
    assert!(base.contains(empty), "the history fixture is no longer empty");

    let with_history = base.replace(empty, &history("2026-08-11T18:00:00Z"));
    // `water_plants` must also look like it has run once, or `never_ran` fires it regardless.
    let with_history = with_history.replace(
        "            - handle = \"water_plants\"\n",
        "            - handle = \"water_plants\"\n            - last_created = \"2026-08-01T09:00:00Z\"\n",
    );

    let at = |instant: &str| {
        let when = chrono::DateTime::parse_from_rfc3339(instant).unwrap().with_timezone(&chrono::Utc);
        FormulaEvaluator::set_time_override(Some(when));
        let nodes = app_api::load_document(with_history.clone()).expect("load");
        FormulaEvaluator::set_time_override(None);
        nodes
    };

    assert!(!due(&at("2026-08-13T09:00:00Z"), "water_plants"), "fired before its interval");
    assert!(due(&at("2026-08-15T09:00:00Z"), "water_plants"), "did not fire after its interval");

    // Having fired, it waits for the trigger to be completed again rather than firing daily.
    let fired = with_history.replace(
        "            - last_created = \"2026-08-01T09:00:00Z\"\n",
        "            - last_created = \"2026-08-15T09:05:00Z\"\n",
    );
    let when = chrono::DateTime::parse_from_rfc3339("2026-08-20T09:00:00Z").unwrap().with_timezone(&chrono::Utc);
    FormulaEvaluator::set_time_override(Some(when));
    let nodes = app_api::load_document(fired).expect("load");
    FormulaEvaluator::set_time_override(None);
    assert!(
        !due(&nodes, "water_plants"),
        "fired again for a completion it had already answered"
    );
}

// -- the document itself ---------------------------------------------------------------------

#[test]
fn the_document_survives_a_load_and_a_save() {
    // Everything above reasons about formulas; this checks the file they live in still comes
    // back as itself, which is the failure that costs real data rather than a wrong answer.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(DOC);
    let text = std::fs::read_to_string(path).expect("no tasks.os");
    let canonical = app_api::canonicalize_document(&text);
    let nodes = app_api::load_document(canonical.clone()).expect("load");
    let saved = overseer::file_ops::OverseerFileHandler::serialize_nodes(&nodes).expect("save");
    assert_eq!(app_api::canonicalize_document(&saved), canonical, "the document drifted on save");
}

#[test]
fn closing_a_task_moves_it_into_the_history_with_its_difficulty() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(DOC);
    overseer::docmgr::manager::DocumentManager::set_current_document(Some(path.to_string_lossy().as_ref()));
    let text = std::fs::read_to_string(&path).expect("no tasks.os");
    let mut nodes = app_api::load_document(text).expect("load");

    fn list<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
        for n in nodes {
            if n.name == name {
                return Some(n);
            }
            if let Some(f) = list(&n.children, name) {
                return Some(f);
            }
        }
        None
    }
    let open = list(&nodes, "Open").expect("no open list");
    let first = open.children[0].name.clone();
    let before = open.children.len();
    let path_to_button = vec!["tasks".into(), "Open".into(), first, "done".into()];
    app_api::execute_event(&mut nodes, &path_to_button, "click").expect("click");

    assert_eq!(list(&nodes, "Open").unwrap().children.len(), before - 1, "it stayed open");
    let history = list(&nodes, "History").expect("no history");
    assert_eq!(history.children.len(), 1, "it did not reach the history");
    let record = &history.children[0];
    assert!(
        field(std::slice::from_ref(record), "difficulty").is_some(),
        "the history record lost the difficulty it will be counted by"
    );
    overseer::docmgr::manager::DocumentManager::set_current_document(None);
}

// -- the switch ------------------------------------------------------------------------------

#[test]
fn a_rule_that_is_switched_off_is_never_due() {
    // `make_bed` is daily, so it is due every day there is - unless it is off.
    assert!(due(&rules_at("2026-08-11T09:00:00Z", "make_bed", &[]), "make_bed"));
    let off = [("active", "false")];
    assert!(
        !due(&rules_at("2026-08-11T09:00:00Z", "make_bed", &off), "make_bed"),
        "a switched-off rule still said it was due"
    );
}

#[test]
fn switching_a_rule_off_does_not_cost_it_what_it_knows() {
    // The point of a switch rather than deleting the rule: it comes back the same.
    let off = [("active", "false")];
    let nodes = rules_at("2026-08-11T09:00:00Z", "kitchen_floor", &off);
    fn entry<'a>(nodes: &'a [OverseerNode], handle: &str) -> &'a OverseerNode {
        fn list<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
            for n in nodes {
                if n.name == name {
                    return Some(n);
                }
                if let Some(f) = list(&n.children, name) {
                    return Some(f);
                }
            }
            None
        }
        let rules = list(nodes, "Rules").expect("no rules");
        rules
            .children
            .iter()
            .find(|e| {
                field(std::slice::from_ref(e), "handle")
                    .map(|v| format!("{v:?}").contains(handle))
                    .unwrap_or(false)
            })
            .expect("no such rule")
    }
    let rule = entry(&nodes, "kitchen_floor");
    assert_eq!(
        field(std::slice::from_ref(rule), "interval_days"),
        Some(OverseerValue::Integer(10)),
        "the interval was lost when the rule was switched off"
    );
    assert_eq!(
        field(std::slice::from_ref(rule), "state"),
        Some(OverseerValue::String("off".into())),
        "a switched-off rule did not say so"
    );
}
