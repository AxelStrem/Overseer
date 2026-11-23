//! A `context vocabulary` is a value, `"/project/Items"`, or a block whose elements each name lists,
//! a mark in front of a path saying which chips in text read it: `- people = "@/people/People"`.
//! A block merges as any context's does, so each layer - a template, an instance made from it, a
//! container inside - adds its own element with `+`, or replaces one by name, and keeps the rest.

use crate::common::serial;
use overseer::parser::parse_document;
use overseer::resolver;
use overseer::types::{OverseerNode, OverseerValue};

fn resolved(source: &str) -> Vec<OverseerNode> {
    overseer::source_registry::SourceRegistry::reset();
    let mut nodes = parse_document(source).expect("parse").1;
    resolver::resolve_document(&mut nodes);
    nodes
}

fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    for node in nodes {
        if node.node_type == "context" {
            continue;
        }
        if node.name == name {
            return Some(node);
        }
        if let Some(hit) = find(&node.children, name) {
            return Some(hit);
        }
    }
    None
}

/// The elements of the copy of `vocabulary` a container holds, by name and what each says.
fn elements(nodes: &[OverseerNode], name: &str) -> Vec<(String, String)> {
    let node = find(nodes, name).unwrap_or_else(|| panic!("no node named `{}`", name));
    let copy = node
        .children
        .iter()
        .find(|child| overseer::contexts::is_copy(child) && child.name == "vocabulary")
        .unwrap_or_else(|| panic!("`{}` holds no copy of the vocabulary", name));
    copy.children
        .iter()
        .map(|element| {
            let said = match element.parameters.get("_computed_value").or_else(|| element.parameters.get("value")) {
                Some(OverseerValue::String(s)) => s.clone(),
                other => format!("{:?}", other),
            };
            (element.name.clone(), said)
        })
        .collect()
}

fn pair(name: &str, said: &str) -> (String, String) {
    (name.to_string(), said.to_string())
}

#[test]
fn an_instance_adds_an_element_to_its_templates_vocabulary() {
    let _guard = serial();
    let nodes = resolved(
        r#"div (hidden=true) {
    tab Project {
        context vocabulary {
            - items = "/p/Items"
        }
        string note = ""
    }
}

<Project> p {
    context vocabulary {
        + people = "@/p/People"
    }
}
"#,
    );
    let mut said = elements(&nodes, "p");
    said.sort();
    assert_eq!(said, vec![pair("items", "/p/Items"), pair("people", "@/p/People")]);
}

#[test]
fn a_container_inside_replaces_one_element_and_keeps_the_rest() {
    let _guard = serial();
    let nodes = resolved(
        r#"tab p {
    context vocabulary {
        - items = "/p/Items"
        - people = "@/p/People"
    }
    div side {
        context vocabulary {
            - people = "@/p/Guests"
        }
        string note = ""
    }
}
"#,
    );
    assert_eq!(elements(&nodes, "p"), vec![pair("items", "/p/Items"), pair("people", "@/p/People")]);
    // The copy holds what the container changes; the page climbs on to the one above for the rest.
    assert_eq!(elements(&nodes, "side"), vec![pair("people", "@/p/Guests")]);
}

#[test]
fn a_block_added_to_a_value_keeps_the_value() {
    let _guard = serial();
    let nodes = resolved(
        r#"tab p {
    context vocabulary = "/p/Items"
    context vocabulary {
        + people = "@/p/People"
    }
    string note = ""
}
"#,
    );
    let copy = find(&nodes, "p")
        .unwrap()
        .children
        .iter()
        .find(|child| overseer::contexts::is_copy(child) && child.name == "vocabulary")
        .expect("a copy");
    assert_eq!(
        overseer::contexts::vocabulary_texts(copy),
        vec!["/p/Items", "@/p/People"]
    );
}

#[test]
fn a_mark_is_not_part_of_the_path() {
    assert_eq!(overseer::contexts::unmarked(" @/p/People"), "/p/People");
    assert_eq!(overseer::contexts::unmarked("/p/Items"), "/p/Items");
    assert_eq!(overseer::contexts::unmarked("% /p/Docs"), "/p/Docs");
}

#[test]
fn the_file_gets_back_what_it_wrote() {
    let _guard = serial();
    let source = "tab p {\r\n    context vocabulary {\r\n        - items = \"/p/Items\"\r\n        + people = \"@/p/People\"\r\n    }\r\n    string note = \"\"\r\n}\r\n";
    let nodes = resolved(source);
    let written = overseer::OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
    assert_eq!(written, source);
}

#[test]
fn an_instance_replaces_an_element_of_its_templates_vocabulary() {
    let _guard = serial();
    let nodes = resolved(
        r#"div (hidden=true) {
    tab Project {
        context vocabulary {
            - items = "/p/Items"
            - people = "@/p/People"
        }
        string note = ""
    }
}

<Project> p {
    context vocabulary {
        - people = "@/p/Guests"
    }
}
"#,
    );
    let mut said = elements(&nodes, "p");
    said.sort();
    assert_eq!(said, vec![pair("items", "/p/Items"), pair("people", "@/p/Guests")]);
}
