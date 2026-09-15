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

/// The document text with `records` put inside its empty `History` list.
///
/// Found by the `{ }` that ends the declaration rather than by the whole line: the list carries
/// parameters - a sort, now - and matching the declaration verbatim meant that adding one made
/// two fixtures here quietly stop finding anywhere to put anything.
fn filled_history(text: &str, records: &str) -> String {
    let at = text.find("list History (entry=<Record>").expect("no History list");
    let empty = text[at..]
        .find(") { }")
        .expect("the History list is not empty, so there is nowhere to put these")
        + at;
    format!(
        "{}) {{\n{records}    }}{}",
        &text[..empty],
        &text[empty + ") { }".len()..]
    )
}

/// The document with an extra record in `History`, at a given instant.
fn with_history(instant: &str, record: &str) -> Vec<OverseerNode> {
    let when = chrono::DateTime::parse_from_rfc3339(instant)
        .expect("bad instant")
        .with_timezone(&chrono::Utc);
    FormulaEvaluator::set_time_override(Some(when));
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(DOC);
    let text = std::fs::read_to_string(path).expect("no tasks.os");
    let text = filled_history(&text, record);
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
    let allow = [("when_open", "\"add\""), ("mode", "\"daily\"")];
    assert!(
        due(&rules_at("2026-08-18T09:00:00Z", "kitchen_floor", &allow), "kitchen_floor"),
        "an open task blocked a rule that permits duplicates"
    );
}

// -- what happens to the task that never got done ---------------------------------------------

#[test]
fn a_rule_that_expires_its_tasks_comes_due_with_one_still_open() {
    // The difference that makes the whole feature. `skip` waits for the open one to be closed
    // by hand - which is what silently attributed today's effort to yesterday's task.
    let replace = [("when_open", "\"fail\""), ("mode", "\"daily\"")];
    let nodes = rules_at("2026-08-18T09:00:00Z", "kitchen_floor", &replace);
    assert!(due(&nodes, "kitchen_floor"), "an open task blocked a rule that expires them");
    assert_eq!(
        rule_field(&nodes, "kitchen_floor", "expires_open"),
        OverseerValue::Boolean(true),
        "the rule must tell the sweep to close the open one"
    );
}

#[test]
fn skipping_is_still_the_default() {
    // Every rule already written says nothing about this, and must keep waiting.
    let daily = [("mode", "\"daily\"")];
    let nodes = rules_at("2026-08-18T09:00:00Z", "kitchen_floor", &daily);
    assert!(!due(&nodes, "kitchen_floor"), "the default stopped waiting for the open task");
    assert_eq!(
        rule_field(&nodes, "kitchen_floor", "expires_open"),
        OverseerValue::Boolean(false)
    );
}

#[test]
fn a_task_closed_unfinished_does_not_start_an_interval_countdown() {
    // "Wash the floors every 10 days" must not be satisfied by ten days of not washing them.
    // `kitchen_floor` is an interval rule; the history entry below is one of its tasks closed
    // as not done, long enough ago to have earned an occasion if it counted as a completion.
    let nodes = rules_at("2026-08-18T09:00:00Z", "kitchen_floor", &[]);
    let before = rule_field(&nodes, "kitchen_floor", "done_count");

    let with_failure = with_history(
        "2026-08-18T09:00:00Z",
        r#"        - {
            - done_at = "2026-08-01T09:00:00Z"
            - rule = "kitchen_floor"
            - title = "Wash the kitchen floor"
            - difficulty = 25
            - failed = true
        }
"#,
    );
    assert_eq!(
        rule_field(&with_failure, "kitchen_floor", "done_count"),
        before,
        "a task closed as not done was counted as having been done"
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
fn a_follow_up_written_before_its_trigger_ever_ran_arrives_once() {
    // The same rule read the other way round, and the reason it is worth writing down.
    //
    // `water_plants` waits on the floor being washed, and the floor has never been washed - so
    // "three days after the floor" is due now. That is deliberate for a chore phased off
    // another chore, and surprising for a genuine follow-up: "unload the washing machine",
    // written before any cycle has been recorded, arrives immediately.
    //
    // It happens once. After the trigger has been completed there is always something to count
    // from, and the rule behaves exactly as it reads. Recording one completion of the trigger
    // first is the whole of the workaround.
    let waiting = rules_at("2026-08-11T09:00:00Z", "water_plants", &[]);
    assert!(due(&waiting, "water_plants"), "the cold start no longer fires");

    let settled = rule_and_history(
        "2026-08-11T09:00:00Z",
        "water_plants",
        &[],
        &floor_washed("2026-08-11T08:00:00Z"),
    );
    assert!(
        !due(&settled, "water_plants"),
        "an hour after the floor was washed, a three-day follow-up was still due"
    );
}

#[test]
fn an_interval_rule_waits_out_its_interval_after_the_trigger_was_done() {
    // The floor was washed on the 11th; the plants want watering 3 days later.
    let record = |done: &str| {
        format!(
            "        - {{
            - done_at = \"{done}\"
            - rule = \"kitchen_floor\"
            - title = \"Wash the kitchen floor\"
            - difficulty = 25
        }}
"
        )
    };
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(DOC);
    let base = std::fs::read_to_string(path).expect("no tasks.os");
    let with_history = filled_history(&base, &record("2026-08-11T18:00:00Z"));
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

// -- the hour of the day ---------------------------------------------------------------------
//
// A calendar rule used to be due for the whole of its day, so a daily task appeared at
// whatever moment the first sweep after the quiet hours happened to run. Naming an hour is
// what makes "the pills, at half seven" a rule rather than a reminder to look at the list.

/// A UTC instant for a local wall-clock time, so these read as the times a person would say.
fn local(day: &str, hour: u32, minute: u32) -> String {
    use chrono::TimeZone;
    let date = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").expect("bad day");
    chrono::Local
        .from_local_datetime(&date.and_hms_opt(hour, minute, 0).expect("bad time"))
        .earliest()
        .expect("no such local time")
        .with_timezone(&chrono::Utc)
        .to_rfc3339()
}

/// One computed field of one rule, by handle.
fn rule_field(nodes: &[OverseerNode], handle: &str, name: &str) -> OverseerValue {
    fn list<'a>(nodes: &'a [OverseerNode], what: &str) -> Option<&'a OverseerNode> {
        for n in nodes {
            if n.name == what {
                return Some(n);
            }
            if let Some(f) = list(&n.children, what) {
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
            return field(std::slice::from_ref(entry), name)
                .unwrap_or_else(|| panic!("rule {handle} has no {name}"));
        }
    }
    panic!("no rule {handle}");
}

fn number(v: OverseerValue) -> f64 {
    match v {
        OverseerValue::Integer(i) => i as f64,
        OverseerValue::Float(f) => f,
        other => panic!("not a number: {other:?}"),
    }
}

#[test]
fn a_rule_that_names_an_hour_waits_for_it() {
    let half_seven = [("at_hour", "7"), ("at_minute", "30")];
    assert!(!due(&rules_at(&local("2026-08-11", 6, 0), "make_bed", &half_seven), "make_bed"));
    assert!(!due(&rules_at(&local("2026-08-11", 7, 29), "make_bed", &half_seven), "make_bed"));
    assert!(due(&rules_at(&local("2026-08-11", 7, 30), "make_bed", &half_seven), "make_bed"));
    assert!(due(&rules_at(&local("2026-08-11", 23, 0), "make_bed", &half_seven), "make_bed"));
}

#[test]
fn naming_no_hour_is_what_every_rule_did_before() {
    // The default has to stay a no-op, or every rule already written changes meaning.
    for hour in [0, 5, 9, 22] {
        assert!(
            due(&rules_at(&local("2026-08-11", hour, 0), "make_bed", &[]), "make_bed"),
            "a rule with no hour should be due at {hour}:00"
        );
    }
}

#[test]
fn the_hour_applies_to_the_calendar_rules_too() {
    // 2026-08-11 is a Tuesday, which is `bins`.
    let evening = [("at_hour", "18"), ("at_minute", "0")];
    assert!(!due(&rules_at(&local("2026-08-11", 9, 0), "bins", &evening), "bins"));
    assert!(due(&rules_at(&local("2026-08-11", 18, 0), "bins", &evening), "bins"));
    // Still only on its own weekday.
    assert!(!due(&rules_at(&local("2026-08-12", 18, 0), "bins", &evening), "bins"));
}

#[test]
fn a_deadline_can_be_a_time_of_day_rather_than_a_length() {
    let morning = [
        ("at_hour", "7"), ("at_minute", "30"),
        ("due_hour", "9"), ("due_minute", "0"),
    ];
    let nodes = rules_at(&local("2026-08-11", 7, 30), "make_bed", &morning);
    assert_eq!(rule_field(&nodes, "make_bed", "has_due_time"), OverseerValue::Boolean(true));
    assert!(
        (number(rule_field(&nodes, "make_bed", "hours_until_due")) - 1.5).abs() < 0.001,
        "half seven to nine is an hour and a half"
    );
}

#[test]
fn a_task_that_opens_late_is_late_rather_than_given_another_day() {
    // The container was down until ten. The promise was nine, and it is now broken - which is
    // the honest answer. Rolling it to nine tomorrow would hand a missed sweep a free day.
    let morning = [
        ("at_hour", "7"), ("at_minute", "30"),
        ("due_hour", "9"), ("due_minute", "0"),
    ];
    let nodes = rules_at(&local("2026-08-11", 10, 0), "make_bed", &morning);
    assert!(
        number(rule_field(&nodes, "make_bed", "hours_until_due")) < 0.0,
        "a deadline already passed should be negative, not tomorrow"
    );
}

#[test]
fn a_deadline_before_the_appearing_hour_means_the_next_day() {
    // "Up at ten at night, due by two in the morning" is one occurrence, not a promise
    // sixteen hours in the past.
    let overnight = [
        ("at_hour", "22"), ("at_minute", "0"),
        ("due_hour", "2"), ("due_minute", "0"),
    ];
    let nodes = rules_at(&local("2026-08-11", 22, 0), "make_bed", &overnight);
    let hours = number(rule_field(&nodes, "make_bed", "hours_until_due"));
    assert!((hours - 4.0).abs() < 0.001, "ten at night to two is four hours, got {hours}");
}

// -- a wait said in hours ----------------------------------------------------------------------
//
// `after` already made one rule follow another. What it could not say was "soon": the wait was
// whole days, and `days_since` truncates, so the shortest follow-up expressible was roughly
// "tomorrow". Two hours after the washing machine finishes is the useful case, and was the one
// that could not be written.

/// The document with the named rule given an extra field, and one record in `History`.
fn rule_and_history(
    instant: &str,
    handle: &str,
    overrides: &[(&str, &str)],
    records: &str,
) -> Vec<OverseerNode> {
    let when = chrono::DateTime::parse_from_rfc3339(instant)
        .expect("bad instant")
        .with_timezone(&chrono::Utc);
    FormulaEvaluator::set_time_override(Some(when));

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(DOC);
    let mut text = std::fs::read_to_string(path).expect("no tasks.os");
    let anchor = format!("            - handle = \"{handle}\"\n");
    assert!(text.contains(&anchor), "no rule {handle} in the document");
    let extra: String = overrides
        .iter()
        .map(|(k, v)| format!("            - {k} = {v}\n"))
        .collect();
    text = text.replace(&anchor, &format!("{anchor}{extra}"));
    let text = filled_history(&text, records);

    let nodes = app_api::load_document(text).expect("load");
    FormulaEvaluator::set_time_override(None);
    nodes
}

/// A completion of the floor-washing rule, which is what `water_plants` waits on.
fn floor_washed(done: &str) -> String {
    format!(
        "        - {{
            - done_at = \"{done}\"
            - rule = \"kitchen_floor\"
            - title = \"Wash the kitchen floor\"
            - difficulty = 25
        }}
"
    )
}

#[test]
fn a_follow_up_in_hours_waits_hours_rather_than_days() {
    let washed = floor_washed("2026-08-11T09:00:00Z");
    let hours = [("interval_hours", "2")];

    assert!(
        !due(
            &rule_and_history("2026-08-11T10:30:00Z", "water_plants", &hours, &washed),
            "water_plants"
        ),
        "an hour and a half after the trigger, a two-hour follow-up came due"
    );
    assert!(
        due(
            &rule_and_history("2026-08-11T11:30:00Z", "water_plants", &hours, &washed),
            "water_plants"
        ),
        "two and a half hours after the trigger, a two-hour follow-up had not come due"
    );
}

#[test]
fn a_wait_in_days_still_means_what_it_always_meant() {
    // `interval_hours` wins only when it is set. Every rule written before it existed leaves it
    // at nought and must behave exactly as it did; this is the whole of that promise.
    let washed = floor_washed("2026-08-11T09:00:00Z");

    assert!(
        !due(
            &rule_and_history("2026-08-13T09:00:00Z", "water_plants", &[], &washed),
            "water_plants"
        ),
        "two days after the floor, a three-day follow-up came due"
    );
    assert!(
        due(
            &rule_and_history("2026-08-15T09:00:00Z", "water_plants", &[], &washed),
            "water_plants"
        ),
        "four days after the floor, a three-day follow-up had not come due"
    );
}

// -- recurring, but on no schedule -------------------------------------------------------------
//
// The dishwasher runs when it is full. A rule for it must not nag on a schedule that is not real,
// and must not sit in the open list as a permanent reproach - so it does neither, and records
// what happened at the moment it happens.

#[test]
fn an_on_demand_rule_is_never_due() {
    for instant in ["2026-08-11T09:00:00Z", "2026-09-01T23:00:00Z"] {
        let nodes = rules_at(instant, "kitchen_floor", &[("mode", "\"on_demand\"")]);
        assert!(
            !due(&nodes, "kitchen_floor"),
            "an on-demand rule came due at {instant}, so the sweep would open a task for it"
        );
    }
}

#[test]
fn pressing_did_it_records_a_completion_without_opening_a_task() {
    let when = chrono::DateTime::parse_from_rfc3339("2026-08-11T09:00:00Z")
        .expect("bad instant")
        .with_timezone(&chrono::Utc);
    FormulaEvaluator::set_time_override(Some(when));

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(DOC);
    let text = std::fs::read_to_string(path).expect("no tasks.os");
    let anchor = "            - handle = \"kitchen_floor\"\n";
    let text = text.replace(anchor, &format!("{anchor}            - mode = \"on_demand\"\n"));
    let mut nodes = app_api::load_document(text).expect("load");

    let before = count(&nodes, "History");
    let open_before = count(&nodes, "Open");

    let button = button_path(&nodes, "kitchen_floor", "did");
    app_api::execute_event(&mut nodes, &button, "click").expect("click");
    FormulaEvaluator::set_time_override(None);

    assert_eq!(
        count(&nodes, "History"),
        before + 1,
        "pressing `did it` recorded nothing"
    );
    assert_eq!(
        count(&nodes, "Open"),
        open_before,
        "pressing `did it` opened a task, which is the one thing an on-demand rule must not do"
    );
}

#[test]
fn what_was_done_on_demand_starts_a_follow_up() {
    // The two features meeting, which is the point of both. An on-demand completion is a
    // completion like any other - the record it writes carries the rule's handle and is not a
    // failure - so anything naming that rule in `after` counts from it, with no extra
    // machinery. That is how the washing machine comes to ask to be unloaded.
    //
    // The record here is the one the test above proves `did it` writes.
    let an_hour_later = rule_and_history(
        "2026-08-11T10:00:00Z",
        "water_plants",
        &[("interval_hours", "2")],
        &floor_washed("2026-08-11T09:00:00Z"),
    );
    assert!(
        !due(&an_hour_later, "water_plants"),
        "an hour after the trigger was done, a two-hour follow-up was already due"
    );

    let later = rule_and_history(
        "2026-08-11T11:30:00Z",
        "water_plants",
        &[("interval_hours", "2")],
        &floor_washed("2026-08-11T09:00:00Z"),
    );
    assert!(
        due(&later, "water_plants"),
        "two and a half hours after the trigger was done, the follow-up had not come due"
    );
}

/// How many entries a named list holds.
fn count(nodes: &[OverseerNode], list_name: &str) -> usize {
    named(nodes, list_name).map(|l| l.children.len()).unwrap_or(0)
}

fn named<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    for n in nodes {
        if n.name == name {
            return Some(n);
        }
        if let Some(f) = named(&n.children, name) {
            return Some(f);
        }
    }
    None
}

/// The address of a button on the named rule.
fn button_path(nodes: &[OverseerNode], handle: &str, button: &str) -> Vec<String> {
    fn path_to(node: &OverseerNode, want: &str, trail: &mut Vec<String>) -> bool {
        for child in &node.children {
            trail.push(child.name.clone());
            if child.name == want {
                return true;
            }
            if path_to(child, want, trail) {
                return true;
            }
            trail.pop();
        }
        false
    }

    let rules = named(nodes, "Rules").expect("no rules");
    for entry in &rules.children {
        let own = field(std::slice::from_ref(entry), "handle")
            .map(|v| format!("{v:?}"))
            .unwrap_or_default();
        if own.contains(handle) {
            let mut trail = vec!["tasks".to_string(), "Rules".to_string(), entry.name.clone()];
            assert!(
                path_to(entry, button, &mut trail),
                "no `{button}` button on rule {handle}"
            );
            return trail;
        }
    }
    panic!("no rule {handle}");
}

// -- what kind of thing it was -----------------------------------------------------------------
//
// A tag is set on the rule and travels: onto every task the rule opens, and from the task into
// the record when it closes. Set once, on the thing that has a kind.
//
// The travelling is the whole point. Every statistic about this document is computed from
// History - "which sort of thing do I not get round to", "what earned the most last week" - and a
// record that did not keep its tags could only be re-tagged by looking the rule up afterwards,
// which would silently re-tag last month's history the day a rule was re-categorised.

/// The `labels` of the last entry in a named list.
fn last_labels(nodes: &[OverseerNode], list_name: &str) -> String {
    let list = named(nodes, list_name).unwrap_or_else(|| panic!("no {list_name}"));
    let entry = list.children.last().unwrap_or_else(|| panic!("{list_name} is empty"));
    field(std::slice::from_ref(entry), "labels")
        .map(|v| format!("{v:?}"))
        .unwrap_or_default()
}

fn document_at(instant: &str) -> Vec<OverseerNode> {
    let when = chrono::DateTime::parse_from_rfc3339(instant)
        .expect("bad instant")
        .with_timezone(&chrono::Utc);
    FormulaEvaluator::set_time_override(Some(when));
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(DOC);
    let text = std::fs::read_to_string(path).expect("no tasks.os");
    let nodes = app_api::load_document(text).expect("load");
    FormulaEvaluator::set_time_override(None);
    nodes
}

#[test]
fn a_rule_puts_its_tags_on_what_it_opens() {
    let mut nodes = document_at("2026-08-11T09:00:00Z");
    let button = button_path(&nodes, "kitchen_floor", "add");
    app_api::execute_event(&mut nodes, &button, "click").expect("click");

    assert!(
        last_labels(&nodes, "Open").contains("chores"),
        "the task opened by a tagged rule came out untagged: {}",
        last_labels(&nodes, "Open")
    );
}

#[test]
fn closing_a_task_keeps_its_tags_in_the_record() {
    let mut nodes = document_at("2026-08-11T09:00:00Z");

    // Open one from a rule, so it is tagged the way a real one would be, then close it.
    let add = button_path(&nodes, "kitchen_floor", "add");
    app_api::execute_event(&mut nodes, &add, "click").expect("add");

    let open = named(&nodes, "Open").expect("no open list");
    let newest = open.children.last().expect("nothing opened").name.clone();
    let done = vec![
        "tasks".to_string(),
        "Open".to_string(),
        newest,
        "done".to_string(),
    ];
    app_api::execute_event(&mut nodes, &done, "click").expect("done");

    assert!(
        last_labels(&nodes, "History").contains("chores"),
        "the record forgot what kind of thing it was: {}",
        last_labels(&nodes, "History")
    );
}

#[test]
fn an_on_demand_completion_is_tagged_too() {
    // It never becomes a task, so it is the one path where the tag would have to be copied a
    // second time rather than travelling with something.
    let when = chrono::DateTime::parse_from_rfc3339("2026-08-11T09:00:00Z")
        .expect("bad instant")
        .with_timezone(&chrono::Utc);
    FormulaEvaluator::set_time_override(Some(when));
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(DOC);
    let text = std::fs::read_to_string(path).expect("no tasks.os");
    let anchor = "            - handle = \"kitchen_floor\"\n";
    let text = text.replace(anchor, &format!("{anchor}            - mode = \"on_demand\"\n"));
    let mut nodes = app_api::load_document(text).expect("load");
    FormulaEvaluator::set_time_override(None);

    let button = button_path(&nodes, "kitchen_floor", "did");
    app_api::execute_event(&mut nodes, &button, "click").expect("click");

    assert!(
        last_labels(&nodes, "History").contains("chores"),
        "an on-demand completion was recorded without its tags: {}",
        last_labels(&nodes, "History")
    );
}

#[test]
fn every_tag_in_use_is_one_the_document_offers() {
    // A tag that is not in the vocabulary still shows, as the bare handle marked "no longer
    // listed" - honest, and not what anyone wants to find. This catches the typo at the point
    // it is introduced rather than on the card weeks later.
    let nodes = document_at("2026-08-11T09:00:00Z");

    let vocabulary: Vec<String> = named(&nodes, "Labels")
        .expect("no Labels list")
        .children
        .iter()
        .filter_map(|entry| field(std::slice::from_ref(entry), "tag"))
        .map(|v| format!("{v:?}"))
        .collect();
    assert!(!vocabulary.is_empty(), "the vocabulary is empty");

    for list_name in ["Rules", "Open", "History"] {
        let Some(list) = named(&nodes, list_name) else {
            continue;
        };
        for entry in &list.children {
            let held = field(std::slice::from_ref(entry), "labels")
                .map(|v| format!("{v:?}"))
                .unwrap_or_default();
            for tag in held
                .trim_matches(|c: char| !c.is_alphanumeric() && c != ',')
                .split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
            {
                assert!(
                    vocabulary.iter().any(|known| known.contains(tag)),
                    "`{tag}` in {list_name} is not one of the tags this document offers"
                );
            }
        }
    }
}
