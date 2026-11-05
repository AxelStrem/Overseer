//! A list a context holds is read as any list is: counted, filtered, its first entry read. Whether
//! the context is declared in a container, at the document's top, or is the environment's own -
//! `host/themes`, the themes there are to choose from.

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

/// What this field was worked out to be.
fn value_of(nodes: &[OverseerNode], name: &str) -> String {
    let node = find(nodes, name).unwrap_or_else(|| panic!("no node named `{}`", name));
    match node
        .parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
    {
        Some(OverseerValue::String(s)) => s.clone(),
        Some(OverseerValue::Integer(i)) => i.to_string(),
        other => format!("{:?}", other),
    }
}

#[test]
fn a_list_in_a_context_at_the_top_is_counted_and_read() {
    let nodes = resolved(
        r#"context c {
    list themes (key="tag") {
        div default {
            string tag = "default"
        }
        div paper {
            string tag = "paper"
        }
    }
}
tab d {
    int n = $(c/themes.count())
    string t = $(c/themes.first()/tag)
    int papers = $(c/themes.filter(|x| x/tag == "paper").count())
}
"#,
    );
    assert_eq!(value_of(&nodes, "n"), "2");
    assert_eq!(value_of(&nodes, "t"), "default");
    assert_eq!(value_of(&nodes, "papers"), "1");
}

#[test]
fn a_list_in_a_context_of_a_container_is_counted_and_read() {
    let nodes = resolved(
        r#"tab d {
    context c {
        list themes (key="tag") {
            div default {
                string tag = "default"
            }
        }
    }
    div e {
        int n = $(c/themes.count())
        string t = $(c/themes.first()/tag)
    }
}
"#,
    );
    assert_eq!(value_of(&nodes, "n"), "1");
    assert_eq!(value_of(&nodes, "t"), "default");
}

#[test]
fn a_list_in_a_context_made_from_a_template_is_counted() {
    let nodes = resolved(
        r#"div (hidden=true) {
    div Theme {
        string tag = ""
    }
}
context c {
    list themes (entry=<Theme>, key="tag") {
        - {
            - tag = "a"
        }
        - {
            - tag = "b"
        }
    }
}
tab d {
    int n = $(c/themes.count())
    string t = $(c/themes.first()/tag)
}
"#,
    );
    assert_eq!(value_of(&nodes, "n"), "2");
    assert_eq!(value_of(&nodes, "t"), "a");
}

#[test]
fn the_themes_the_host_offers_are_counted() {
    let nodes = resolved(
        r#"tab d {
    int n = $(host/themes.count())
    string t = $(host/themes.first()/tag)
}
"#,
    );
    let n: usize = value_of(&nodes, "n").parse().expect("a count");
    assert_eq!(n, overseer::environment::themes().len());
    assert!(n > 0);
    assert_eq!(value_of(&nodes, "t"), overseer::environment::themes()[0]);
}

#[test]
fn a_list_on_another_tab_is_counted() {
    let nodes = resolved(
        r#"tab a {
    list items {
        div one {
            int x = 1
        }
        div two {
            int x = 2
        }
    }
}
tab b {
    int n = $(a/items.count())
}
"#,
    );
    assert_eq!(value_of(&nodes, "n"), "2");
}
