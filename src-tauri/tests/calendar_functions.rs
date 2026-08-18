//! Saying "every Tuesday" or "the 7th of the month" in a formula.
//!
//! Durations cannot express either: a month is not a fixed number of days, and without a
//! modulo operator a weekday cannot be derived from one. These three functions are the whole
//! of what the task scheduler's calendar rules need.

use overseer::app_api;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::types::{OverseerNode, OverseerValue};

/// Evaluate a formula in a document pinned to a known instant.
fn at(instant: &str, formula: &str) -> OverseerValue {
    let when = chrono::DateTime::parse_from_rfc3339(instant)
        .expect("bad instant")
        .with_timezone(&chrono::Utc);
    FormulaEvaluator::set_time_override(Some(when));
    let text = format!(
        "tab t (label=\"T\") {{
    timestamp anchor (hidden=true) = \"2026-07-05T09:00:00Z\"
    int out = $({formula})
}}
"
    );
    let nodes = app_api::load_document(text).expect("load");
    let value = find(&nodes, "out").expect("no out field");
    FormulaEvaluator::set_time_override(None);
    value
}

fn find(nodes: &[OverseerNode], name: &str) -> Option<OverseerValue> {
    for n in nodes {
        if n.name == name {
            return n
                .parameters
                .get("_computed_value")
                .or(n.parameters.get("value"))
                .cloned();
        }
        if let Some(v) = find(&n.children, name) {
            return Some(v);
        }
    }
    None
}

fn int(v: OverseerValue) -> i64 {
    match v {
        OverseerValue::Integer(i) => i,
        OverseerValue::Float(f) => f as i64,
        other => panic!("not a number: {other:?}"),
    }
}

#[test]
fn weekdays_are_iso_numbered_from_monday() {
    // 2026-08-10 is a Monday.
    for (day, expected) in [
        ("2026-08-10", 1),
        ("2026-08-11", 2),
        ("2026-08-15", 6),
        ("2026-08-16", 7),
    ] {
        let v = at(&format!("{day}T12:00:00Z"), "weekday(today())");
        assert_eq!(int(v), expected, "{day} came out as the wrong weekday");
    }
}

#[test]
fn the_day_and_the_month_come_off_the_calendar() {
    assert_eq!(int(at("2026-07-05T12:00:00Z", "day_of_month(today())")), 5);
    assert_eq!(int(at("2026-07-05T12:00:00Z", "month_of(today())")), 7);
    assert_eq!(int(at("2028-02-29T12:00:00Z", "day_of_month(today())")), 29);
}

#[test]
fn they_read_a_stored_timestamp_as_well_as_todays_date() {
    // The anchor is a fixed field, so the answer must not depend on when this runs.
    assert_eq!(int(at("2026-01-01T00:00:00Z", "day_of_month(anchor)")), 5);
    assert_eq!(int(at("2026-01-01T00:00:00Z", "month_of(anchor)")), 7);
    // 2026-07-05 is a Sunday.
    assert_eq!(int(at("2026-01-01T00:00:00Z", "weekday(anchor)")), 7);
}

#[test]
fn a_calendar_rule_reads_as_the_sentence_it_stands_for() {
    // What the scheduler actually writes: fires on the 5th of July and no other day.
    let rule = "month_of(today()) == 7 && day_of_month(today()) == 5";
    assert_eq!(
        at("2026-07-05T09:00:00Z", &format!("{rule} ? 1 : 0")),
        OverseerValue::Integer(1)
    );
    for other in ["2026-07-04T09:00:00Z", "2026-08-05T09:00:00Z"] {
        assert_eq!(
            at(other, &format!("{rule} ? 1 : 0")),
            OverseerValue::Integer(0),
            "fired on {other}"
        );
    }
}

// -- one notion of a timestamp ---------------------------------------------------------------

/// Every way a document, a button or a bot writes an instant.
const WRITTEN_AS: [&str; 6] = [
    "2026-07-05T09:00:00+00:00", // what now() produces
    "2026-07-05T09:00:00Z",      // what most things produce
    "2026-07-05T09:00:00",       // what a model produces when not told otherwise
    "2026-07-05 09:00:00",       // ...or this
    "2026-07-05T09:00:00.123Z",  // with the fractional seconds a clock gives
    "2026-07-05",                // a date, which is midnight
];

/// The formula, evaluated with `anchor` set to each of the above in turn.
fn for_each_form(formula: &str) -> Vec<(&'static str, OverseerValue)> {
    WRITTEN_AS
        .iter()
        .map(|written| {
            let when = chrono::DateTime::parse_from_rfc3339("2026-07-12T09:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc);
            FormulaEvaluator::set_time_override(Some(when));
            let text = format!(
                "tab t (label=\"T\") {{
    timestamp anchor (hidden=true) = \"{written}\"
    string out = $({formula})
}}
"
            );
            let nodes = app_api::load_document(text).expect("load");
            let value = find(&nodes, "out").expect("no out field");
            FormulaEvaluator::set_time_override(None);
            (*written, value)
        })
        .collect()
}

#[test]
fn every_date_function_reads_the_same_timestamp() {
    // The bug this pins: `same_day` accepted a timestamp with no zone and `days_since` did not,
    // so a bot-written task showed "invalid formula error" in its priority. Whether a string is
    // a timestamp cannot be a question two functions answer differently.
    for formula in [
        "days_since(anchor)",
        "same_day(anchor, anchor) ? 1 : 0",
        "weekday(anchor)",
        "day_of_month(anchor)",
        "month_of(anchor)",
        "date_add_days(anchor, 1)",
    ] {
        for (written, value) in for_each_form(formula) {
            let text = format!("{value:?}");
            assert!(
                !text.contains("error"),
                "{formula} could not read a timestamp written as {written:?}: {text}"
            );
        }
    }
}

#[test]
fn the_forms_that_mean_the_same_instant_give_the_same_answer() {
    // The three that are 09:00 UTC however they are spelled.
    let same = ["2026-07-05T09:00:00+00:00", "2026-07-05T09:00:00Z", "2026-07-05T09:00:00"];
    for formula in ["days_since(anchor)", "day_of_month(anchor)"] {
        let answers = for_each_form(formula);
        let wanted: Vec<_> = answers
            .iter()
            .filter(|(w, _)| same.contains(w))
            .map(|(_, v)| format!("{v:?}"))
            .collect();
        assert_eq!(
            wanted.iter().collect::<std::collections::HashSet<_>>().len(),
            1,
            "{formula} gave different answers for one instant spelled three ways: {wanted:?}"
        );
    }
}

// -- deadlines -------------------------------------------------------------------------------

#[test]
fn minutes_since_can_tell_late_from_early() {
    // The whole reason it exists: `days_since` truncates toward zero, so three hours either
    // side of a deadline both come out 0 and a task cannot know whether it is overdue.
    let three_hours_late = at("2026-07-05T12:00:00Z", "days_since(anchor)");
    assert_eq!(three_hours_late, OverseerValue::Integer(0));
    let three_hours_early = at("2026-07-05T06:00:00Z", "days_since(anchor)");
    assert_eq!(three_hours_early, OverseerValue::Integer(0), "days_since cannot tell them apart");

    assert_eq!(int(at("2026-07-05T12:00:00Z", "minutes_since(anchor)")), 180);
    assert_eq!(int(at("2026-07-05T06:00:00Z", "minutes_since(anchor)")), -180);
}

#[test]
fn overdue_reads_as_a_sentence() {
    // What a task actually asks. Minute resolution, so this means "at least a minute late" -
    // which is the point: `>= 0` would also be true 59 seconds *before* the deadline, because
    // truncation pulls a fraction of a minute either side of zero to the same 0.
    let overdue = "minutes_since(anchor) > 0 ? 1 : 0";
    assert_eq!(at("2026-07-05T09:01:00Z", overdue), OverseerValue::Integer(1));
    assert_eq!(at("2026-07-05T08:59:00Z", overdue), OverseerValue::Integer(0));
    // The seconds either side of the deadline belong to neither, and nothing is waiting on
    // them: the sweep that notices comes round every few minutes.
    assert_eq!(at("2026-07-05T09:00:01Z", overdue), OverseerValue::Integer(0));
}

#[test]
fn a_deadline_can_be_set_an_interval_after_something() {
    let day_later = at("2026-07-05T09:00:00Z", "date_add_hours(anchor, 24)");
    match day_later {
        OverseerValue::Timestamp(ts) => assert!(ts.starts_with("2026-07-06T09:00:00"), "{ts}"),
        other => panic!("not a timestamp: {other:?}"),
    }
}

#[test]
fn half_an_hour_is_a_fraction_rather_than_a_third_function() {
    let soon = at("2026-07-05T09:00:00Z", "date_add_hours(anchor, 0.5)");
    match soon {
        OverseerValue::Timestamp(ts) => assert!(ts.starts_with("2026-07-05T09:30:00"), "{ts}"),
        other => panic!("not a timestamp: {other:?}"),
    }
}

#[test]
fn the_two_compose_into_what_a_rule_needs() {
    // A deadline 24 hours after the task was added, asked about 25 hours later.
    let due_in_a_day = "minutes_since(date_add_hours(anchor, 24)) > 0 ? 1 : 0";
    assert_eq!(at("2026-07-06T10:00:00Z", due_in_a_day), OverseerValue::Integer(1));
    assert_eq!(at("2026-07-06T08:00:00Z", due_in_a_day), OverseerValue::Integer(0));
}
