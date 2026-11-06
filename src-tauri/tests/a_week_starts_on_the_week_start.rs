//! A week starts on the `week_start` a calendar sees: a context of the environment, `"system"`
//! where the settings do not say otherwise, which is the first day of the week in the page's
//! locale - Monday where nothing reported one. A person's settings choose it for every document,
//! and a document or a part of one changes it for what is inside, as any context.

use overseer::environment;
use overseer::server::DocumentRoot;
use overseer::types::*;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Sandbox(std::path::PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sandbox(tag: &str, files: &[(&str, &str)]) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_week_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for (name, text) in files {
        std::fs::write(root.join(name), text).unwrap();
    }
    overseer::app_api::forget_baseline();
    overseer::app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let documents = DocumentRoot::new(&root).unwrap();
    (Sandbox(root), documents)
}

/// A week holding Wednesday, 4 November 2026, and what the document reads of the first weekday.
const WEEK: &str = "tab t (label=\"T\") {\r\n    string seen = $(week_start)\r\n    calendar Days (entry=int, span=\"week\", at=\"2026-11-04\") { }\r\n}\r\n";

fn first_in_view(nodes: &[OverseerNode], address: &str) -> String {
    let calendar = overseer::addressing::find(nodes, address).expect("the calendar");
    let days: Vec<String> = calendar
        .children
        .iter()
        .filter(|c| matches!(c.parameters.get("_in_view"), Some(OverseerValue::Boolean(true))))
        .map(|c| c.name.clone())
        .collect();
    assert_eq!(days.len(), 7, "a week is seven days: {:?}", days);
    days[0].clone()
}

fn text_at(nodes: &[OverseerNode], address: &str) -> Option<String> {
    let node = overseer::addressing::find(nodes, address)?;
    match node
        .parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
    {
        Some(OverseerValue::String(s)) => Some(s.clone()),
        _ => None,
    }
}

#[test]
fn with_nothing_reported_a_week_starts_on_monday() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_sandbox, documents) = sandbox("monday", &[("p.os", WEEK)]);
    let nodes = documents.open("p.os").expect("open");
    assert_eq!(first_in_view(&nodes, "t/Days"), "2026-11-02");
    assert_eq!(text_at(&nodes, "t/seen").as_deref(), Some("monday"));
}

#[test]
fn a_week_starts_on_the_day_the_page_reports() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_sandbox, documents) = sandbox("reported", &[("p.os", WEEK)]);
    let sunday = environment::weeks_start_on(Some("sunday"), || documents.open("p.os")).expect("open");
    assert_eq!(first_in_view(&sunday, "t/Days"), "2026-11-01");
    assert_eq!(text_at(&sunday, "t/seen").as_deref(), Some("sunday"), "a formula reads the day, never \"system\"");

    // Held worked out for each locale apart: a page whose weeks start on Monday is not handed the
    // Sunday copy.
    let monday = documents.open("p.os").expect("reopen");
    assert_eq!(first_in_view(&monday, "t/Days"), "2026-11-02");
}

#[test]
fn the_settings_choose_the_day_over_the_locale() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let settings = "context mode = \"system\"\r\ncontext week_start = \"tuesday\"\r\n";
    let (_sandbox, documents) = sandbox("settings", &[("p.os", WEEK), (environment::FILE, settings)]);
    let nodes = environment::weeks_start_on(Some("sunday"), || documents.open("p.os")).expect("open");
    assert_eq!(first_in_view(&nodes, "t/Days"), "2026-11-03");
    assert_eq!(text_at(&nodes, "t/seen").as_deref(), Some("tuesday"));
}

#[test]
fn a_part_of_a_document_changes_the_day_for_what_is_inside_it() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let doc = format!(
        "{}tab u (label=\"U\") {{\r\n    context week_start = \"saturday\"\r\n    calendar Days (entry=int, span=\"week\", at=\"2026-11-04\") {{ }}\r\n}}\r\n",
        WEEK
    );
    let settings = "context week_start = \"tuesday\"\r\n";
    let (_sandbox, documents) = sandbox("part", &[("p.os", &doc), (environment::FILE, settings)]);
    let nodes = environment::weeks_start_on(Some("sunday"), || documents.open("p.os")).expect("open");
    assert_eq!(first_in_view(&nodes, "u/Days"), "2026-10-31");
    assert_eq!(first_in_view(&nodes, "t/Days"), "2026-11-03", "only what is inside the tab");
}

#[test]
fn a_week_start_that_is_no_day_is_said_on_the_calendar() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let doc = format!("context week_start = \"someday\"\r\n{}", WEEK);
    let (_sandbox, documents) = sandbox("noday", &[("p.os", &doc)]);
    let nodes = documents.open("p.os").expect("open");
    let calendar = overseer::addressing::find(&nodes, "t/Days").expect("the calendar");
    let why = calendar.parameters.get("_inherit_error").expect("a problem said");
    assert!(matches!(why, OverseerValue::String(s) if s.contains("someday")), "{:?}", why);
}

#[test]
fn choosing_the_day_writes_it_to_the_settings() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = sandbox("choose", &[("p.os", WEEK)]);
    let chosen = documents
        .command(None, "settings_choose", &serde_json::json!({ "week_start": "sunday" }))
        .expect("the choice");
    assert_eq!(chosen["week_start"], "sunday");
    let written = std::fs::read_to_string(sandbox.0.join(environment::FILE)).unwrap();
    assert_eq!(
        written,
        environment::DEFAULT.replace("context week_start = \"system\"", "context week_start = \"sunday\"")
    );
    let nodes = documents.open("p.os").expect("open");
    assert_eq!(first_in_view(&nodes, "t/Days"), "2026-11-01");

    let refused = documents.command(None, "settings_choose", &serde_json::json!({ "week_start": "someday" }));
    assert!(refused.is_err(), "only a day, or system");
}

#[test]
fn settings_from_before_the_first_weekday_are_given_it_when_it_is_chosen() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let settings = "context mode = \"dark\"\r\n";
    let (sandbox, documents) = sandbox("older", &[("p.os", WEEK), (environment::FILE, settings)]);
    let before = documents.command(None, "settings_read", &serde_json::json!({})).expect("read");
    assert_eq!(before["week_start"], "system");
    documents
        .command(None, "settings_choose", &serde_json::json!({ "week_start": "saturday" }))
        .expect("the choice");
    let written = std::fs::read_to_string(sandbox.0.join(environment::FILE)).unwrap();
    assert!(written.starts_with(settings), "what was there stays as it was: {:?}", written);
    assert!(written.contains("context week_start = \"saturday\""), "{:?}", written);
    let nodes = documents.open("p.os").expect("open");
    assert_eq!(first_in_view(&nodes, "t/Days"), "2026-10-31");
}
