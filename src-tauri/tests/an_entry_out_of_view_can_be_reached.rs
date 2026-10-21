//! A window decides what is drawn, not what there is.
//!
//! A list with `window=3` works out only the three entries it shows, which is what makes a long
//! history cheap to open. But the others are still the document's: a total over the whole list, a
//! lookup of an old entry by its key, a read of one by its address and a button writing to one all
//! have to find it as it would be in view - its template given and its formulas worked out. What
//! reaches an entry brings it in to be worked out, and the window still draws the three.

use overseer::app_api;
use overseer::server::DocumentRoot;
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

fn value(nodes: &[OverseerNode], name: &str) -> Option<OverseerValue> {
    let node = find(nodes, name)?;
    node.parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
        .cloned()
}

fn number(value: Option<OverseerValue>) -> Option<i64> {
    match value? {
        OverseerValue::Integer(i) => Some(i),
        OverseerValue::Float(f) => Some(f as i64),
        OverseerValue::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn hidden(node: &OverseerNode) -> bool {
    matches!(node.parameters.get("_out_of_view"), Some(OverseerValue::Boolean(true)))
}

/// Six days, newest first, three of them in view, and whatever else the document says after them.
fn log(rest: &str) -> String {
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
    text.push_str("\x20   }\n");
    text.push_str(rest);
    text.push_str("}\n");
    text
}

/// The entries of the list, in the order the file has them.
fn days(nodes: &[OverseerNode]) -> Vec<&OverseerNode> {
    find(nodes, "Days")
        .expect("no Days list")
        .children
        .iter()
        .filter(|c| c.name.starts_with("Day__"))
        .collect()
}

#[test]
fn a_total_over_a_windowed_list_counts_every_entry() {
    // `doubled` comes from the template, so an entry left as it was written has none, and the
    // total came out as what the three in view add up to: 8 + 10 + 12.
    let nodes = app_api::load_document(log(
        "    int total (label=\"\") = $(Days.sum(|x| x/doubled))\n",
    ))
    .expect("load");
    assert_eq!(number(value(&nodes, "total")), Some(42), "the total missed what is out of view");
}

#[test]
fn what_a_total_brings_in_is_worked_out_and_still_not_drawn() {
    let nodes = app_api::load_document(log(
        "    int total (label=\"\") = $(Days.sum(|x| x/doubled))\n",
    ))
    .expect("load");
    let all = days(&nodes);
    assert_eq!(all.iter().filter(|d| hidden(d)).count(), 3, "the window should still draw three");
    for day in all {
        assert!(
            day.children.iter().any(|c| c.name == "note"),
            "{} was reached and never given its template",
            day.name
        );
    }
    let left = find(&nodes, "Days").unwrap().parameters.get("_left_out_of_view").cloned();
    assert_eq!(left, Some(OverseerValue::Integer(3)), "the note under the list should still say three");
}

#[test]
fn a_lookup_by_key_reads_an_old_entry_as_it_would_be_in_view() {
    let nodes = app_api::load_document(log(
        "    int pick (label=\"\") = 1\n\
         \x20   int picked (label=\"\") = $(Days.find(../pick)/doubled)\n",
    ))
    .expect("load");
    assert_eq!(number(value(&nodes, "picked")), Some(2), "the old day was found unworked");
}

#[test]
fn a_lookup_brings_in_the_entry_it_found_and_nothing_else() {
    // The window is still worth having when a document looks up one old day: only that day is
    // paid for.
    let nodes = app_api::load_document(log(
        "    int pick (label=\"\") = 1\n\
         \x20   int picked (label=\"\") = $(Days.find(../pick)/doubled)\n",
    ))
    .expect("load");
    let given: Vec<&str> = days(&nodes)
        .into_iter()
        .filter(|d| d.children.iter().any(|c| c.name == "note"))
        .map(|d| d.name.as_str())
        .collect();
    assert_eq!(given.len(), 4, "worked out: {given:?}");
    assert!(given.contains(&"Day__1"));
}

#[test]
fn a_document_that_reaches_nothing_past_the_window_works_nothing_out_there() {
    let nodes = app_api::load_document(log("    int pick (label=\"\") = 1\n")).expect("load");
    let given = days(&nodes)
        .into_iter()
        .filter(|d| d.children.iter().any(|c| c.name == "note"))
        .count();
    assert_eq!(given, 3);
}

#[test]
fn a_button_can_write_to_an_old_entry_from_another() {
    // `note` is the template's, so the old entry as written has no such field to set, and reading
    // its `doubled` found nothing to copy.
    let text = log(
        "    button copy (label=\"Copy\") {\n\
         \x20       on click {\n\
         \x20           set (path=\"../Days/Day__1/note\") = $(\"twice \" + ../Days/Day__2/doubled)\n\
         \x20       }\n\
         \x20   }\n",
    );
    let pressed = app_api::execute_event_on_text(text, vec!["t".into(), "copy".into()], "click".into())
        .expect("the press should run");
    assert!(
        pressed.text.contains("note = \"twice 4\""),
        "the old entry did not get the note:\n{}",
        pressed.text
    );
}

#[test]
fn what_reaches_past_the_window_still_saves_back_as_it_was_written() {
    let text = log("    int total (label=\"\") = $(Days.sum(|x| x/doubled))\n");
    let nodes = app_api::load_document(text.clone()).expect("load");
    let saved = overseer::file_ops::OverseerFileHandler::serialize_nodes(&nodes).expect("save");
    assert_eq!(saved, text);
}

struct Sandbox {
    root: std::path::PathBuf,
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn a_read_by_address_answers_an_old_entry_worked_out() {
    let root = std::env::temp_dir().join(format!("overseer_viewreach_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let _sandbox = Sandbox { root: root.clone() };
    std::fs::write(root.join("log.os"), log("")).unwrap();
    let documents = DocumentRoot::new(&root).unwrap();
    let read = documents.read_at("log.os", "t/Days/[1]/doubled").expect("the old day's field should be readable");
    let doubled = read
        .node
        .parameters
        .get("_computed_value")
        .or_else(|| read.node.parameters.get("value"))
        .cloned();
    assert_eq!(number(doubled), Some(2));
}

#[test]
fn a_change_that_sends_a_lookup_to_another_old_entry_finds_it_worked_out() {
    // Worked out for one day, then asked about another: the edit is worked out selectively, and
    // the day it now reaches was never worked out at all.
    let text = log(
        "    int pick (label=\"\") = 1\n\
         \x20   int picked (label=\"\") = $(Days.find(../pick)/doubled)\n",
    );
    let mut values = std::collections::HashMap::new();
    values.insert("t/pick".to_string(), OverseerValue::Integer(2));
    let nodes = app_api::resolve_selective(text, vec!["t/pick".to_string()], Some(values)).expect("resolve");
    assert_eq!(number(value(&nodes, "picked")), Some(4));
}

#[test]
fn a_press_reaching_an_old_entry_keeps_every_entry_as_it_was_written() {
    // Bringing an entry in after the document was worked out once worked out the template's own
    // `$(today())` over a day's written date - see `resolver::keeping_in_view`. A press that
    // reaches an old day works the document out again with it held, and every date stays.
    let text = "tab t (label=\"T\", mutable=true) {
                    div (hidden=true) {
                        div Day {
                            date day = $(today())
                            int size = 0
                            int doubled = $(size * 2)
                            string note = \"\"
                        }
                    }
                    list Days (entry=<Day>, key=\"day\", window=1, sort_by=$(|x| 0 - millis_since_epoch(x/day))) {
                        - {
                            - day = 2026-01-01
                            - size = 1
                        }
                        - {
                            - day = 2026-01-02
                            - size = 2
                        }
                    }
                    button copy (label=\"Copy\") {
                        on click {
                            set (path=\"../Days/Day__1/note\") = $(\"was \" + ../Days/Day__1/doubled)
                        }
                    }
                }
";
    let pressed = app_api::execute_event_on_text(text.to_string(), vec!["t".into(), "copy".into()], "click".into())
        .expect("the press should run");
    assert!(pressed.text.contains("note = \"was 2\""), "{}", pressed.text);
    assert!(pressed.text.contains("day = 2026-01-01"), "{}", pressed.text);
    assert!(pressed.text.contains("day = 2026-01-02"), "{}", pressed.text);
    assert_eq!(pressed.text.matches("- {").count(), 2, "{}", pressed.text);
}

#[test]
fn a_press_through_the_server_writes_to_an_old_entry() {
    // The server presses on the document it holds worked out for the text, where the old day is
    // as it was written - so this is the press run again, and the change settled whole.
    let root = std::env::temp_dir().join(format!("overseer_viewreach_press_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let _sandbox = Sandbox { root: root.clone() };
    std::fs::write(
        root.join("log.os"),
        log("    button copy (label=\"Copy\") {
                     on click {
                         set (path=\"../Days/Day__1/note\") = $(\"twice \" + ../Days/Day__2/doubled)
                     }
                 }
                 int total (label=\"\") = $(Days.filter(|x| x/note == \"twice 4\").count())
"),
    )
    .unwrap();
    let documents = DocumentRoot::new(&root).unwrap();
    documents.open("log.os").expect("open");
    documents.run_event("log.os", "t/copy", "click").expect("the press should run");
    let written = std::fs::read_to_string(root.join("log.os")).unwrap();
    assert!(written.contains("note = \"twice 4\""), "{written}");
    let nodes = documents.open("log.os").expect("open again");
    assert_eq!(number(value(&nodes, "total")), Some(1));
}
