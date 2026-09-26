//! A list of plain values holds one entry per value.
//!
//! `list Names (entry=string) { - "Eve" }` is in the spec and two of the examples, and it was
//! never read right. The parser looked for a value after a dash only when nothing at all followed
//! it, so inside a list - where the next line is the next entry - it never found one: `- "a"` came
//! out as a bare dash and a node named `a`, and `- 1` as a dash named 1 with no value. A list of
//! two counted four, or two of nothing. The file still came back unchanged, because saving replays
//! the source text, which is why nothing noticed.
//!
//! Reading the values then showed a second fault behind the first: the resolver replaced each
//! entry of such a list with a new node, which had no source to be written back from, so the
//! first save after reading them right would have moved every entry to the list's own depth.

use overseer::app_api;
use overseer::types::*;

const DOCUMENT: &str = r#"tab t (mutable=true) {

    list Names (entry=string) {
        - "Eve"
        - "Mallory"
    }

    list Scores (entry=int) {
        - 10
        - 20
        - 30
    }

    int names = $(Names.count())
    int scores = $(Scores.count())

    div (hidden=true) {
        div Rule {
            string title = ""
            int priority = 0
        }
    }

    // A dash followed by a name is a field of an entry, as it always was.
    list Rules (entry=<Rule>) {
        - {
            - title = "dishes"
            - priority = 5
        }
    }
}
"#;

fn list<'a>(nodes: &'a [OverseerNode], name: &str) -> &'a OverseerNode {
    nodes[0].children.iter().find(|c| c.name == name).unwrap_or_else(|| panic!("no {}", name))
}

fn computed(nodes: &[OverseerNode], name: &str) -> Option<OverseerValue> {
    nodes[0].children.iter().find(|c| c.name == name)?.parameters.get("_computed_value").cloned()
}

#[test]
fn each_value_is_one_entry_holding_it() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    let names: Vec<_> = list(&nodes, "Names").children.iter().map(|e| e.parameters.get("value").cloned()).collect();
    assert_eq!(names, vec![Some(OverseerValue::String("Eve".into())), Some(OverseerValue::String("Mallory".into()))]);
    let scores: Vec<_> = list(&nodes, "Scores").children.iter().map(|e| e.parameters.get("value").cloned()).collect();
    assert_eq!(scores, vec![Some(OverseerValue::Integer(10)), Some(OverseerValue::Integer(20)), Some(OverseerValue::Integer(30))]);
}

#[test]
fn a_count_counts_the_values() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    assert_eq!(computed(&nodes, "names"), Some(OverseerValue::Integer(2)));
    assert_eq!(computed(&nodes, "scores"), Some(OverseerValue::Integer(3)));
}

#[test]
fn every_entry_has_a_name_of_its_own() {
    // Named the way `append` names the ones it adds, so an address can tell them apart.
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    let names: Vec<_> = list(&nodes, "Scores").children.iter().map(|e| e.name.clone()).collect();
    assert_eq!(names, vec!["int__1", "int__2", "int__3"]);
}

#[test]
fn a_dash_followed_by_a_name_is_still_a_field() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    let rule = &list(&nodes, "Rules").children[0];
    let field = |name: &str| rule.children.iter().find(|c| c.name == name).and_then(|c| c.parameters.get("value").cloned());
    assert_eq!(field("title"), Some(OverseerValue::String("dishes".into())));
    assert_eq!(field("priority"), Some(OverseerValue::Integer(5)));
}

#[test]
fn it_is_written_back_as_it_was_read() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    let saved = overseer::file_ops::OverseerFileHandler::serialize_nodes(&nodes).expect("save");
    assert_eq!(saved, DOCUMENT);
}

#[test]
fn an_edited_value_is_written_in_its_place() {
    let root = std::env::temp_dir().join(format!("overseer_plain_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("p.os"), DOCUMENT).unwrap();
    let write: app_api::ValueWrite = serde_json::from_value(serde_json::json!({
        "node_path": ["t", "Names", "string__2"], "value": { "String": "Trent" }
    }))
    .unwrap();
    app_api::write_values_at(&root.join("p.os").to_string_lossy(), "p.os", vec![write], "s").expect("write");
    let after = std::fs::read_to_string(root.join("p.os")).unwrap();
    let changed: Vec<_> = DOCUMENT.lines().zip(after.lines()).filter(|(a, b)| a != b).collect();
    assert_eq!(changed, vec![("        - \"Mallory\"", "        - \"Trent\"")], "{}", after);
    assert_eq!(DOCUMENT.lines().count(), after.lines().count(), "{}", after);
}
