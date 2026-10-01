//! A write reads the clock as it is when the write happens, not as it was when the document was
//! last worked out.
//!
//! A write starts from the document held for its text, which is quick - and what reads the clock in
//! it is as old as the last time it was worked out. An open brings that up to date first; a write
//! did not. So the sweep's press that opens the morning's tasks read how long each had until it was
//! due as it had stood the evening before, and every task opened already overdue: on 2026-09-29
//! `Morning hygiene`, due by ten, came due thirteen hours before it appeared, and three more with it
//! - all worked out from 23:38 the night before, the last time anything had written to the list.

use chrono::{DateTime, TimeZone, Timelike, Utc};
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::server::DocumentRoot;
use overseer::types::*;

const DOCUMENT: &str = r#"tab t (label="T", mutable=true) {
    div (hidden=true) {
        div Task (layout="horizontal") {
            timestamp added = ""
            timestamp deadline = ""
        }
    }

    int due_minutes = 1200
    int clock_now = $(minutes_of_day(now()))
    float hours_until_due = $((due_minutes - clock_now) / 60.0)

    list Open (entry=<Task>) {
    }

    button open (label="open") {
        on click {
            append (list="/t/Open") {
                - added = $(now())
                - deadline = $(date_add_hours(now(), ../hours_until_due))
            }
        }
    }
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    FormulaEvaluator::set_time_override(None);
    out
}

/// A moment at this hour and minute of the local day - which is the day `minutes_of_day` counts in.
fn at_local(month: u32, day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
    chrono::Local
        .with_ymd_and_hms(2026, month, day, hour, minute, 0)
        .single()
        .expect("a local moment")
        .with_timezone(&Utc)
}

fn root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_clock_now_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("d.os"), DOCUMENT).unwrap();
    overseer::app_api::forget_baseline();
    overseer::app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    root
}

/// How long the task opened last was given, in minutes: its deadline less when it was added.
fn given(root: &std::path::Path) -> i64 {
    let text = std::fs::read_to_string(root.join("d.os")).unwrap();
    let stamp = |field: &str| -> DateTime<Utc> {
        // The entry's, which holds a stamp - not the button's, which holds the formula.
        let line = text
            .lines()
            .find(|l| l.trim_start().starts_with(&format!("- {} = \"", field)))
            .unwrap_or_else(|| panic!("no {} written:\n{}", field, text));
        let quoted = line.split('"').nth(1).unwrap_or_else(|| panic!("not a stamp: {}\n{}", line, text));
        DateTime::parse_from_rfc3339(quoted).expect("a stamp").with_timezone(&Utc)
    };
    (stamp("deadline") - stamp("added")).num_minutes()
}

fn due_in_minutes(from: DateTime<Utc>) -> i64 {
    let local = from.with_timezone(&chrono::Local);
    1200 - (local.hour() * 60 + local.minute()) as i64
}

#[test]
fn the_bot_opening_a_task_in_the_morning_works_from_the_morning() {
    serialised(|| {
        let root = root("bot");
        let documents = DocumentRoot::new(&root).unwrap();
        // Worked out and held late the evening before.
        let evening = at_local(9, 28, 23, 38);
        FormulaEvaluator::set_time_override(Some(evening));
        documents.open("d.os").expect("open");

        let morning = at_local(9, 29, 7, 4);
        FormulaEvaluator::set_time_override(Some(morning));
        documents.run_event("d.os", "t/open", "click").expect("the press");
        assert_eq!(
            given(&root),
            due_in_minutes(morning),
            "the deadline was worked out from when the document was last worked out"
        );
    });
}

#[test]
fn the_page_opening_a_task_in_the_morning_works_from_the_morning() {
    serialised(|| {
        let root = root("page");
        let file = root.join("d.os").to_string_lossy().to_string();
        let evening = at_local(9, 28, 23, 38);
        FormulaEvaluator::set_time_override(Some(evening));
        let page = overseer::app_api::load_document(DOCUMENT.to_string()).expect("open");
        let button = overseer::addressing::name_path(&page, "t/open").expect("the button");

        let morning = at_local(9, 29, 7, 4);
        FormulaEvaluator::set_time_override(Some(morning));
        let answer = overseer::app_api::run_event_at(&file, "d.os", button, "click".into(), "s", Vec::new())
            .expect("the press");
        assert_eq!(given(&root), due_in_minutes(morning));
        // And the page is told what moved with the clock, not only what the press changed.
        let changes = answer.changes.expect("answered with what changed");
        assert!(
            changes.iter().any(|c| c.address() == "t/clock_now"),
            "the page was left showing the evening's clock: {:?}",
            changes.iter().map(|c| c.address()).collect::<Vec<_>>()
        );
    });
}

#[test]
fn the_sweep_reading_the_rules_before_it_presses_still_works_from_the_morning() {
    // The sweep reads the rules to see which are due, then presses each one's `add` - within the
    // same minute. Opening a document brought a copy of it up to the minute and marked the one
    // held as though it had been, so the press that followed found it "already caught up" and read
    // the evening's figures: every task of 2026-10-01 opened overdue, a day after the fix that
    // should have stopped it.
    serialised(|| {
        let root = root("sweep");
        let documents = DocumentRoot::new(&root).unwrap();
        FormulaEvaluator::set_time_override(Some(at_local(9, 30, 23, 58)));
        documents.open("d.os").expect("open");

        let morning = at_local(10, 1, 8, 4);
        FormulaEvaluator::set_time_override(Some(morning));
        documents.read_at("d.os", "t").expect("the sweep reads what is due");
        documents.run_event("d.os", "t/open", "click").expect("then presses");
        assert_eq!(given(&root), due_in_minutes(morning), "the press read the evening's figures");
    });
}
