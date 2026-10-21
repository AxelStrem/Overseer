//! A link reaches the entry it names, as a formula does.
//!
//! A div with `link="/t/Days[key=$(../pick)]"` shows whichever entry its key names - the food
//! tracker's selected day is one. The page follows the link in the document the server sent, so an
//! entry the list's window leaves out has to come worked out like any other entry something reaches:
//! its template given and its formulas worked out, and still not drawn in the list.

use overseer::app_api;
use overseer::types::{OverseerNode, OverseerValue};

fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    for n in nodes {
        if n.name == name {
            return Some(n);
        }
        if let Some(f) = find(&n.children, name) {
            return Some(f);
        }
    }
    None
}

fn hidden(node: &OverseerNode) -> bool {
    matches!(node.parameters.get("_out_of_view"), Some(OverseerValue::Boolean(true)))
}

fn entry<'a>(nodes: &'a [OverseerNode], name: &str) -> &'a OverseerNode {
    find(nodes, "Days")
        .expect("no Days list")
        .children
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no {name}"))
}

fn worked_out(entry: &OverseerNode) -> bool {
    entry.children.iter().any(|c| c.name == "note")
}

fn doubled(entry: &OverseerNode) -> Option<OverseerValue> {
    let field = entry.children.iter().find(|c| c.name == "doubled")?;
    field.parameters.get("_computed_value").or_else(|| field.parameters.get("value")).cloned()
}

/// Six days, newest first, three of them in view, and a div showing the one `pick` names.
fn log() -> String {
    let mut text = String::from(
        "tab t (label=\"T\", mutable=true) {\n\
         \x20   div (hidden=true) {\n\
         \x20       div Day (layout=\"horizontal\") {\n\
         \x20           int day (label=\"\") = 0\n\
         \x20           int size (label=\"\") = 0\n\
         \x20           int doubled (label=\"\") = $(size * 2)\n\
         \x20           string note (label=\"\") = \"from the template\"\n\
         \x20       }\n\
         \x20   }\n\
         \x20   list Days (entry=<Day>, key=\"day\", window=3, sort_by=$(|x| 0 - x/day)) {\n",
    );
    for i in 1..=6 {
        text.push_str(&format!(
            "\x20       - {{\n\
             \x20           - day = {i}\n\
             \x20           - size = {i}\n\
             \x20       }}\n"
        ));
    }
    text.push_str(
        "\x20   }\n\
         \x20   div Selected {\n\
         \x20       int pick (label=\"\") = 1\n\
         \x20       div Shown (link=\"/t/Days[key=$(../pick)]\") { }\n\
         \x20   }\n\
         }\n",
    );
    text
}

#[test]
fn a_link_works_out_the_entry_it_names() {
    let nodes = app_api::load_document(log()).expect("load");
    let day = entry(&nodes, "Day__1");
    assert!(worked_out(day), "the linked day was left as it was written");
    assert_eq!(doubled(day), Some(OverseerValue::Integer(2)));
    assert!(hidden(day), "the linked day should still not be drawn in the list");
}

#[test]
fn a_link_brings_in_the_entry_it_names_and_nothing_else() {
    let nodes = app_api::load_document(log()).expect("load");
    for name in ["Day__2", "Day__3"] {
        assert!(!worked_out(entry(&nodes, name)), "{name} was worked out though nothing reaches it");
    }
    let left = find(&nodes, "Days").unwrap().parameters.get("_left_out_of_view").cloned();
    assert_eq!(left, Some(OverseerValue::Integer(3)), "the note under the list should still say three");
}

#[test]
fn a_change_of_the_key_sends_the_link_to_another_old_entry_worked_out() {
    // How the selected day moves: a guarded field changed for the viewer, worked out selectively.
    let mut values = std::collections::HashMap::new();
    values.insert("t/Selected/pick".to_string(), OverseerValue::Integer(2));
    let nodes = app_api::resolve_selective(log(), vec!["t/Selected/pick".to_string()], Some(values))
        .expect("resolve");
    let day = entry(&nodes, "Day__2");
    assert!(worked_out(day), "the day the link moved to was left as it was written");
    assert_eq!(doubled(day), Some(OverseerValue::Integer(4)));
}

#[test]
fn a_timestamp_finds_its_day_in_a_list_keyed_by_the_day() {
    // The food tracker's case: days written as dates, and a selected timestamp carrying an hour.
    let mut text = String::from(
        "tab t (label=\"T\", mutable=true) {\n\
         \x20   div (hidden=true) {\n\
         \x20       div Day {\n\
         \x20           timestamp date (precision=\"day\") = $(today())\n\
         \x20           string note (label=\"\") = \"from the template\"\n\
         \x20       }\n\
         \x20   }\n\
         \x20   list Days (entry=<Day>, key=\"date\", keyPrecision=\"day\", window=3, sort_by=$(|x| 0 - millis_since_epoch(x/date))) {\n",
    );
    for d in 1..=6 {
        text.push_str(&format!(
            "\x20       - {{\n\
             \x20           - date = \"2026-08-0{d}\"\n\
             \x20       }}\n"
        ));
    }
    text.push_str(
        "\x20   }\n\
         \x20   div Selected {\n\
         \x20       timestamp selected (precision=\"day\", mutable=\"guarded\") = $(today())\n\
         \x20       div Shown (link=\"/t/Days[key=$(../selected)]\") { }\n\
         \x20   }\n\
         }\n",
    );
    let mut values = std::collections::HashMap::new();
    values.insert(
        "t/Selected/selected".to_string(),
        OverseerValue::Timestamp("2026-08-02T09:30:00+03:00".to_string()),
    );
    let nodes = app_api::resolve_selective(text, vec!["t/Selected/selected".to_string()], Some(values))
        .expect("resolve");
    let day = entry(&nodes, "Day__2");
    assert!(worked_out(day), "the selected day was left as it was written");
    assert!(!worked_out(entry(&nodes, "Day__1")), "a day nothing selects was worked out");
    let date = day.children.iter().find(|c| c.name == "date").unwrap();
    let date = date.parameters.get("_computed_value").or_else(|| date.parameters.get("value"));
    assert!(
        matches!(date, Some(OverseerValue::String(s)) | Some(OverseerValue::Timestamp(s)) | Some(OverseerValue::Date(s)) if s.starts_with("2026-08-02")),
        "the day kept its written date: {date:?}"
    );
}

#[test]
fn a_link_reaching_past_the_window_still_saves_back_as_it_was_written() {
    let text = log();
    let nodes = app_api::load_document(text.clone()).expect("load");
    let saved = overseer::file_ops::OverseerFileHandler::serialize_nodes(&nodes).expect("save");
    assert_eq!(saved, text);
}
