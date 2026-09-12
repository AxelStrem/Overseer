//! Saying "every Tuesday" or "the 7th of the month" in a formula.
//!
//! Durations cannot express either: a month is not a fixed number of days, and a weekday cannot
//! be derived from one by arithmetic that has no calendar in it - `%` exists now, and still does
//! not know when a month ends. These three functions are the whole of what the task scheduler's
//! calendar rules need.

use chrono::TimeZone;
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

// `minutes_of_day` is the fourth: the clock, rather than the calendar.
//
// Written to hold whatever zone the machine running the tests is in, because that is exactly
// the property being tested - a function that only works at Greenwich is the bug it replaces.

/// Local minutes since local midnight for an instant, worked out independently of the evaluator.
fn expected_local_minutes(instant: &str) -> i64 {
    use chrono::Timelike;
    let local = chrono::DateTime::parse_from_rfc3339(instant)
        .expect("bad instant")
        .with_timezone(&chrono::Local);
    local.hour() as i64 * 60 + local.minute() as i64
}

#[test]
fn the_clock_is_read_in_local_time() {
    for instant in ["2026-08-22T06:51:00Z", "2026-01-15T23:10:00Z", "2026-03-01T00:05:00Z"] {
        assert_eq!(
            int(at(instant, "minutes_of_day(now())")),
            expected_local_minutes(instant),
            "{instant} did not come out as the local wall clock"
        );
    }
}

#[test]
fn ninety_minutes_later_reads_ninety_minutes_later() {
    // Independent of the zone: whatever midnight it counts from, the gap is the gap.
    let before = int(at("2026-08-22T06:00:00Z", "minutes_of_day(now())"));
    let after = int(at("2026-08-22T07:30:00Z", "minutes_of_day(now())"));
    assert_eq!((after - before).rem_euclid(1440), 90);
}

#[test]
fn it_reads_a_stored_timestamp_too() {
    // The anchor is 2026-07-05T09:00:00Z, so this must not depend on when the test runs.
    assert_eq!(
        int(at("2026-01-01T00:00:00Z", "minutes_of_day(anchor)")),
        expected_local_minutes("2026-07-05T09:00:00Z")
    );
}

#[test]
fn it_is_not_the_workaround_it_replaces() {
    // `minutes_since(today())` was the only way to ask this before, and it answers in UTC:
    // `today()` is a local date, a bare date parses as midnight UTC, and `now()` is UTC. The
    // two agree only where the local offset is zero, which is why the bug survived review.
    use chrono::Offset;
    let instant = "2026-08-22T06:51:00Z";
    let offset_minutes = chrono::Local
        .from_utc_datetime(
            &chrono::DateTime::parse_from_rfc3339(instant).unwrap().naive_utc(),
        )
        .offset()
        .fix()
        .local_minus_utc() as i64
        / 60;

    let clock = int(at(instant, "minutes_of_day(now())"));
    let workaround = int(at(instant, "minutes_since(today())"));
    assert_eq!(
        (clock - workaround).rem_euclid(1440),
        offset_minutes.rem_euclid(1440),
        "the difference between the two should be exactly the local offset"
    );
}

#[test]
fn the_same_instant_spelled_two_ways_reads_the_same() {
    // True in any zone, which the offset test above is not: on a machine whose local time is
    // UTC it compares zero to zero. This one has something to say wherever it runs.
    let anchored = |written: &str| {
        let text = format!(
            "tab t (label=\"T\") {{
    timestamp anchor (hidden=true) = \"{written}\"
    int out = $(minutes_of_day(anchor))
}}
"
        );
        let nodes = app_api::load_document(text).expect("load");
        int(find(&nodes, "out").expect("no out field"))
    };
    assert_eq!(anchored("2026-08-22T09:00:00+04:00"), anchored("2026-08-22T05:00:00Z"));
    assert_eq!(anchored("2026-08-22T00:30:00-03:00"), anchored("2026-08-22T03:30:00Z"));
}

// `millis_since_epoch` is the one that does not move.
//
// Ordering a list newest-first means negating something, because `sort_by` sorts one way only.
// Negating "how long ago" works and costs the earth: that key changes every minute, so every
// entry of every history lands in every delta. This one depends on the instant alone.

/// `millis_since_epoch` of a stored timestamp, read through a document.
fn anchored_minutes(written: &str) -> i64 {
    let text = format!(
        "tab t (label=\"T\") {{
    timestamp anchor (hidden=true) = \"{written}\"
    int out = $(millis_since_epoch(anchor))
}}
"
    );
    let nodes = app_api::load_document(text).expect("load");
    int(find(&nodes, "out").expect("no out field"))
}

#[test]
fn it_does_not_move_with_the_clock() {
    // The anchor field is fixed, so asking at two very different moments must agree. This is
    // the whole property: a sort key that changes is a sort key in every delta.
    let a = int(at("2026-01-01T00:00:00Z", "millis_since_epoch(anchor)"));
    let b = int(at("2030-06-30T23:59:00Z", "millis_since_epoch(anchor)"));
    assert_eq!(a, b, "it moved with the clock");
}

#[test]
fn later_is_larger() {
    let earlier = anchored_minutes("2026-08-22T09:00:00Z");
    assert_eq!(anchored_minutes("2026-08-22T09:01:00Z") - earlier, 60_000, "a minute in ms");
    // Seconds matter: two things recorded in one turn are seconds apart, and minute
    // precision tied them so they read backwards.
    assert_eq!(anchored_minutes("2026-08-22T09:00:04Z") - earlier, 4_000, "seconds are lost");
    // Three things bought in one turn were fifteen milliseconds apart, and seconds tied
    // them - so they read forwards while everything around them read backwards.
    assert_eq!(anchored_minutes("2026-08-22T09:00:00.015Z") - earlier, 15, "ms are lost");
    assert!(anchored_minutes("2027-01-01T00:00:00Z") > anchored_minutes("2026-01-01T00:00:00Z"));
}

#[test]
fn the_same_instant_written_two_ways_gives_one_number() {
    assert_eq!(
        anchored_minutes("2026-08-22T09:00:00+04:00"),
        anchored_minutes("2026-08-22T05:00:00Z"),
        "the offset was not taken into account"
    );
}

#[test]
fn negating_it_puts_the_newest_first() {
    // What the histories write. Ascending on the negative is descending on the instant.
    let mut keys = ["2026-08-20T10:00:00Z", "2026-08-22T10:00:00Z", "2026-08-21T10:00:00Z"]
        .map(|t| (0 - anchored_minutes(t), t));
    keys.sort();
    assert_eq!(
        keys.map(|(_, t)| t),
        ["2026-08-22T10:00:00Z", "2026-08-21T10:00:00Z", "2026-08-20T10:00:00Z"]
    );
}

#[test]
fn two_things_in_one_day_are_still_ordered() {
    // What `days_since` could not do: it ties everything from the same day, and ties fall back
    // to the order the file holds - which put the oldest of the last day at the top.
    assert!(anchored_minutes("2026-08-22T21:30:00Z") > anchored_minutes("2026-08-22T08:15:00Z"));
}
