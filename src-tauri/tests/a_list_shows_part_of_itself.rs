//! A list can keep only part of itself in view, and the rest costs nothing.
//!
//! The food tracker's history is forty-three days and grows; three is what anyone reads. Before
//! this, all forty-three were instantiated from the template, had every formula worked out, and
//! took a place in the dependency graph, whether or not they were ever on screen.
//!
//! The saving is only real because the window is applied *before* templates are resolved. Measured
//! on the real document: 235,379 parameters resolved against 53,791 parsed, of which only 28,032
//! are worked-out values - so skipping evaluation alone would have saved the time and almost none
//! of the memory. With the window, the same document resolves to 86,683 parameters in 1.57 seconds
//! rather than 6.6, and its cache entry drops from 241 MB to 53.
//!
//! What must stay true is that the entries are still *there*: they are in the document, they save
//! back exactly as they were written, and anything addressing them still finds them.

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

fn out_of_view(node: &OverseerNode) -> bool {
    matches!(
        node.parameters.get("_out_of_view"),
        Some(OverseerValue::Boolean(true))
    )
}

/// A log of `rows` days, newest first, keeping `window` of them in view.
fn log(rows: usize, window: Option<usize>) -> String {
    let mut text = String::from(
        "tab t (label=\"T\") {\n\
         \x20   div (hidden=true) {\n\
         \x20       div Day (layout=\"horizontal\") {\n\
         \x20           int day (label=\"\") = 0\n\
         \x20           int size (label=\"\") = 0\n\
         \x20           int doubled (label=\"\") = $(size * 2)\n\
         \x20           string note (label=\"\") = \"from the template\"\n\
         \x20       }\n\
         \x20   }\n",
    );
    let windowing = match window {
        Some(n) => format!("window={n}, "),
        None => String::new(),
    };
    text.push_str(&format!(
        "\x20   list Days (entry=<Day>, {windowing}sort_by=$(|x| 0 - x/day)) {{\n"
    ));
    for i in 1..=rows {
        text.push_str(&format!(
            "\x20       - {{\n\
             \x20           - day = {i}\n\
             \x20           - size = {i}\n\
             \x20       }}\n"
        ));
    }
    text.push_str("\x20   }\n}\n");
    text
}

fn days(nodes: &[OverseerNode]) -> Vec<&OverseerNode> {
    find(nodes, "Days")
        .expect("no Days list")
        .children
        .iter()
        .filter(|c| c.node_type == "list_item" || c.node_type == "-" || c.node_type == "Day")
        .collect()
}

fn day_number(node: &OverseerNode) -> i64 {
    let field = node
        .children
        .iter()
        .find(|c| c.name == "day")
        .expect("no day field");
    match field
        .parameters
        .get("value")
        .or_else(|| field.parameters.get("_computed_value"))
    {
        Some(OverseerValue::Integer(i)) => *i,
        Some(OverseerValue::Float(f)) => *f as i64,
        other => panic!("day is not a number: {other:?}"),
    }
}

#[test]
fn only_the_window_is_in_view() {
    let nodes = app_api::load_document(log(10, Some(3))).expect("load");
    let all = days(&nodes);
    assert_eq!(all.len(), 10, "entries were removed rather than left out of view");
    let shown: Vec<i64> = all
        .iter()
        .filter(|d| !out_of_view(d))
        .map(|d| day_number(d))
        .collect();
    assert_eq!(shown.len(), 3, "expected three days in view, got {shown:?}");
}

#[test]
fn the_window_keeps_the_ones_that_would_be_shown_first() {
    // `sort_by` negates the day, so the newest sort first and those are the three to keep. Getting
    // this backwards would show the three days nobody is looking at, which is the whole mistake
    // worth guarding against.
    let nodes = app_api::load_document(log(10, Some(3))).expect("load");
    let mut shown: Vec<i64> = days(&nodes)
        .iter()
        .filter(|d| !out_of_view(d))
        .map(|d| day_number(d))
        .collect();
    shown.sort();
    assert_eq!(shown, vec![8, 9, 10]);
}

#[test]
fn a_list_shorter_than_its_window_shows_everything() {
    let nodes = app_api::load_document(log(2, Some(3))).expect("load");
    assert!(days(&nodes).iter().all(|d| !out_of_view(d)));
    assert!(
        find(&nodes, "Days")
            .expect("no list")
            .parameters
            .get("_left_out_of_view")
            .is_none(),
        "a list showing everything claimed to be holding something back"
    );
}

#[test]
fn the_list_says_how_many_it_left_out() {
    // Without this the other forty days look deleted rather than unshown.
    let nodes = app_api::load_document(log(10, Some(3))).expect("load");
    assert_eq!(
        find(&nodes, "Days")
            .expect("no list")
            .parameters
            .get("_left_out_of_view"),
        Some(&OverseerValue::Integer(7))
    );
}

#[test]
fn a_list_without_a_window_is_untouched() {
    // Every document that says nothing must behave exactly as it did.
    let nodes = app_api::load_document(log(10, None)).expect("load");
    let all = days(&nodes);
    assert_eq!(all.len(), 10);
    assert!(all.iter().all(|d| !out_of_view(d)));
    for day in &all {
        assert!(
            day.children.iter().any(|c| c.name == "note"),
            "an entry did not get its template"
        );
    }
}

#[test]
fn nothing_out_of_view_is_instantiated_or_worked_out() {
    // The saving itself. An entry out of view keeps what it was written with and gains nothing:
    // no template fields, and no worked-out values.
    let nodes = app_api::load_document(log(10, Some(3))).expect("load");
    for day in days(&nodes) {
        if out_of_view(&day) {
            assert!(
                !day.children.iter().any(|c| c.name == "note"),
                "day {} was given its template despite being out of view",
                day_number(&day)
            );
            let computed = day
                .children
                .iter()
                .flat_map(|c| c.parameters.keys())
                .filter(|k| k.starts_with("_computed_"))
                .count();
            assert_eq!(
                computed, 0,
                "day {} had values worked out despite being out of view",
                day_number(&day)
            );
        } else {
            assert!(
                day.children.iter().any(|c| c.name == "note"),
                "a day in view did not get its template"
            );
        }
    }
}

#[test]
fn a_window_costs_a_fraction_of_the_work() {
    // Not a timing test - those belong nowhere near a suite - but a count, which is what the time
    // is proportional to. A tenth of the days in view should be far short of a tenth of the work
    // only because the parts outside the list are paid for either way.
    fn parameters(nodes: &[OverseerNode]) -> usize {
        nodes
            .iter()
            .map(|n| n.parameters.len() + parameters(&n.children))
            .sum()
    }
    let windowed = parameters(&app_api::load_document(log(60, Some(3))).expect("load"));
    let whole = parameters(&app_api::load_document(log(60, None)).expect("load"));
    assert!(
        windowed * 3 < whole,
        "the window saved almost nothing: {windowed} parameters against {whole}"
    );
}

#[test]
fn what_is_out_of_view_still_saves_back_as_it_was_written() {
    // The entries are in the document, and a window is a reading decision. A save that dropped
    // forty days of history, or wrote them back half-instantiated, would be the worst possible
    // outcome of this feature.
    let text = log(10, Some(3));
    let nodes = app_api::load_document(text.clone()).expect("load");
    let written = overseer::file_ops::OverseerFileHandler::serialize_nodes(&nodes)
        .expect("serialize");
    let again = app_api::canonicalize_document(&written);
    assert_eq!(
        again,
        app_api::canonicalize_document(&text),
        "a windowed document did not save back as it was written"
    );
}
