//! A context is a name whose value stacks down the hierarchy.
//!
//! `context accent = "blue"` is seen by everything inside the container it is written in, and a
//! `context accent = "red"` nearer down overrides it for what is inside that one. A context with a
//! body is overridden the way an instance overrides its template, and its formulas are worked out
//! where it is used, so a value derived from another follows wherever that other is overridden.

use overseer::parser::parse_document;
use overseer::resolver;
use overseer::types::{OverseerNode, OverseerValue};

fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    for node in nodes {
        if node.name == name && node.node_type != "context" {
            return Some(node);
        }
        if let Some(hit) = find(&node.children, name) {
            return Some(hit);
        }
    }
    None
}

fn resolved(source: &str) -> Vec<OverseerNode> {
    overseer::source_registry::SourceRegistry::reset();
    let mut nodes = parse_document(source).expect("parse").1;
    resolver::resolve_document(&mut nodes);
    nodes
}

fn text(value: Option<&OverseerValue>) -> String {
    match value {
        Some(OverseerValue::String(s)) => s.clone(),
        Some(OverseerValue::Integer(i)) => i.to_string(),
        Some(OverseerValue::Float(f)) => f.to_string(),
        Some(OverseerValue::Boolean(b)) => b.to_string(),
        other => format!("{:?}", other),
    }
}

/// What this field was worked out to be.
fn value_of(nodes: &[OverseerNode], name: &str) -> String {
    let node = find(nodes, name).unwrap_or_else(|| panic!("no node named `{}`", name));
    text(
        node.parameters
            .get("_computed_value")
            .or_else(|| node.parameters.get("value")),
    )
}

#[test]
fn a_context_reaches_everything_inside_its_container() {
    let nodes = resolved(
        r#"context accent = "blue"
tab t (label="T") {
    div a {
        div b {
            string deep = $(accent)
        }
    }
}
"#,
    );
    assert_eq!(value_of(&nodes, "deep"), "blue");
}

#[test]
fn where_the_line_sits_changes_nothing() {
    let nodes = resolved(
        r#"tab t (label="T") {
    string before = $(accent)
    context accent = "blue"
    string after = $(accent)
}
"#,
    );
    assert_eq!(value_of(&nodes, "before"), "blue");
    assert_eq!(value_of(&nodes, "after"), "blue");
}

#[test]
fn a_nearer_context_overrides_for_what_is_inside_it_only() {
    let nodes = resolved(
        r#"context accent = "blue"
tab t (label="T") {
    div side {
        context accent = "red"
        string inside = $(accent)
    }
    string outside = $(accent)
}
"#,
    );
    assert_eq!(value_of(&nodes, "inside"), "red");
    assert_eq!(value_of(&nodes, "outside"), "blue");
}

#[test]
fn an_override_changes_only_what_it_names() {
    let nodes = resolved(
        r#"context theme {
    - accent = "blue"
    - surface = "white"
}
tab t (label="T") {
    div side {
        context theme {
            - accent = "red"
        }
        string accent_inside = $(theme/accent)
        string surface_inside = $(theme/surface)
    }
    string accent_outside = $(theme/accent)
}
"#,
    );
    assert_eq!(value_of(&nodes, "accent_inside"), "red");
    assert_eq!(value_of(&nodes, "surface_inside"), "white");
    assert_eq!(value_of(&nodes, "accent_outside"), "blue");
}

#[test]
fn an_override_can_add_with_a_plus() {
    let nodes = resolved(
        r#"context theme {
    - accent = "blue"
}
tab t (label="T") {
    context theme {
        + string radius = "8px"
    }
    string radius_read = $(theme/radius)
    string accent_read = $(theme/accent)
}
"#,
    );
    assert_eq!(value_of(&nodes, "radius_read"), "8px");
    assert_eq!(value_of(&nodes, "accent_read"), "blue");
}

#[test]
fn two_in_one_container_merge_in_the_order_they_are_written() {
    let nodes = resolved(
        r#"tab t (label="T") {
    context theme {
        string accent = "first"
        string line = "kept"
    }
    context theme {
        - accent = "second"
    }
    string accent_read = $(theme/accent)
    string line_read = $(theme/line)
}
"#,
    );
    assert_eq!(value_of(&nodes, "accent_read"), "second");
    assert_eq!(value_of(&nodes, "line_read"), "kept");
}

#[test]
fn a_derived_value_follows_an_override_of_what_it_is_derived_from() {
    let nodes = resolved(
        r#"context theme {
    string text = "black"
    string line = $(text + " line")
}
tab t (label="T") {
    div side {
        context theme {
            - text = "white"
        }
        string inside = $(theme/line)
    }
    string outside = $(theme/line)
}
"#,
    );
    assert_eq!(value_of(&nodes, "inside"), "white line");
    assert_eq!(value_of(&nodes, "outside"), "black line");
}

#[test]
fn a_context_overridden_below_reworks_the_others_there() {
    // The night div says nothing about the theme, only about the mode; the theme seen there is
    // worked out again with that mode.
    let nodes = resolved(
        r#"context mode = "light"
context theme {
    string surface = $(mode == "dark" ? "black" : "white")
}
tab t (label="T") {
    div night {
        context mode = "dark"
        string inside = $(theme/surface)
    }
    string outside = $(theme/surface)
}
"#,
    );
    assert_eq!(value_of(&nodes, "inside"), "black");
    assert_eq!(value_of(&nodes, "outside"), "white");
}

#[test]
fn a_context_in_a_div_without_a_name_belongs_to_the_container_around_it() {
    let nodes = resolved(
        r#"tab t (label="T") {
    div record {
        div (layout="horizontal") {
            context accent = "red"
        }
        string beside = $(accent)
    }
}
"#,
    );
    assert_eq!(value_of(&nodes, "beside"), "red");
}

#[test]
fn a_template_body_sees_the_contexts_of_where_the_instance_is() {
    let nodes = resolved(
        r#"context accent = "blue"
div (hidden=true) {
    div Card {
        string shown = $(accent)
    }
}
tab t (label="T") {
    div side {
        context accent = "red"
        <Card> card
    }
}
"#,
    );
    let card = find(&nodes, "card").expect("the instance");
    assert_eq!(value_of(std::slice::from_ref(card), "shown"), "red");
}

#[test]
fn a_context_a_template_declares_is_in_every_entry() {
    let nodes = resolved(
        r#"context accent = "blue"
div (hidden=true) {
    div Task {
        string colour = ""
        context accent = $(colour)
        string shown = $(accent)
    }
}
tab t (label="T") {
    list tasks (entry=<Task>) {
        - first {
            - colour = "red"
        }
        - second {
            - colour = "green"
        }
    }
}
"#,
    );
    let tasks = find(&nodes, "tasks").expect("the list");
    let shown: Vec<String> = tasks
        .children
        .iter()
        .map(|entry| value_of(std::slice::from_ref(entry), "shown"))
        .collect();
    assert_eq!(shown, vec!["red", "green"]);
}

#[test]
fn a_context_is_not_among_what_its_container_holds() {
    let nodes = resolved(
        r#"tab t (label="T") {
    div box {
        context accent = "red"
        int a = 1
        int b = 2
    }
    int held = $(box.count())
}
"#,
    );
    assert_eq!(value_of(&nodes, "held"), "2");
}

#[test]
fn a_search_going_down_does_not_find_another_containers_context() {
    // `mode` is not visible in `main`: the only one is the sidebar's.
    let nodes = resolved(
        r#"tab t (label="T") {
    div sidebar {
        context mode = "dark"
    }
    div main {
        string read = $(mode)
    }
}
"#,
    );
    assert_ne!(value_of(&nodes, "read"), "dark");
}

#[test]
fn a_node_named_like_a_visible_context_is_refused() {
    let nodes = resolved(
        r#"context mode = "light"
tab t (label="T") {
    string mode = "mine"
}
"#,
    );
    let field = find(&nodes, "mode").expect("the field is there");
    assert_eq!(
        text(field.parameters.get(overseer::bases::ERROR)),
        "'mode' is a context here, and cannot name anything else"
    );
}

#[test]
fn a_template_cannot_be_named_context() {
    assert!(overseer::bases::is_type("context"));
}

#[test]
fn a_base_starts_the_context_from_another_node() {
    let nodes = resolved(
        r#"div (hidden=true) {
    div paper {
        string accent = "brown"
        string surface = "cream"
    }
}
context theme {
    string accent = "blue"
}
tab t (label="T") {
    context theme (base=<paper>) {
        - surface = "ivory"
    }
    string accent_read = $(theme/accent)
    string surface_read = $(theme/surface)
}
"#,
    );
    assert_eq!(value_of(&nodes, "accent_read"), "brown");
    assert_eq!(value_of(&nodes, "surface_read"), "ivory");
}

#[test]
fn resolving_twice_comes_to_the_same() {
    let source = r#"context accent = "blue"
tab t (label="T") {
    div side {
        context accent = "red"
        string inside = $(accent)
    }
}
"#;
    fn contexts(nodes: &[OverseerNode]) -> usize {
        nodes
            .iter()
            .map(|n| (n.node_type == "context") as usize + contexts(&n.children))
            .sum()
    }
    let mut nodes = resolved(source);
    let once = contexts(&nodes);
    resolver::resolve_document(&mut nodes);
    assert_eq!(contexts(&nodes), once, "the copies of the first time are not kept beside the new");
    assert_eq!(value_of(&nodes, "inside"), "red");
    assert_eq!(
        overseer::OverseerFileHandler::serialize_nodes(&nodes).expect("serialize"),
        source
    );
}

#[test]
fn the_file_gets_back_what_it_wrote() {
    let source = "context mode = \"light\"\r\ncontext theme {\r\n    string surface = $(mode == \"dark\" ? \"black\" : \"white\")\r\n}\r\n\r\ntab t (label=\"T\") {\r\n    div side {\r\n        // the night side\r\n        context mode = \"dark\"\r\n        context theme {\r\n            - surface = \"grey\"\r\n        }\r\n        string inside = $(theme/surface)\r\n    }\r\n}\r\n";
    let nodes = resolved(source);
    let written = overseer::OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
    assert_eq!(written, source);
}
