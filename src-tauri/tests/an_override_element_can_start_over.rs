//! An element of an override can start over rather than merge.
//!
//! An override changes only what it names: `context theme { - backdrop { - glow { ... } } }`
//! changes one layer and keeps the rest. `base=none` on one of its elements starts that element
//! from nothing instead, so a part of a document can drop a block it inherits - every layer of a
//! backdrop - without naming each thing in it. Its own block, if it has one, is all it holds.
//! `base=<node>` starts it from that node's block rather than from nothing. The same holds in an
//! instance's block, against its template.

use overseer::file_ops::OverseerFileHandler;
use overseer::parser::parse_document;
use overseer::resolver;
use overseer::types::{OverseerNode, OverseerValue};

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn resolved(source: &str) -> Vec<OverseerNode> {
    overseer::source_registry::SourceRegistry::reset();
    let mut nodes = parse_document(source).expect("parse").1;
    resolver::resolve_document(&mut nodes);
    nodes
}

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

/// The theme a container sees, as the copy worked out for it.
fn theme_of<'a>(nodes: &'a [OverseerNode], container: &str) -> &'a OverseerNode {
    let container = find(nodes, container).unwrap_or_else(|| panic!("no `{}`", container));
    container
        .children
        .iter()
        .find(|c| c.node_type == "context" && c.name == "theme" && overseer::contexts::is_context(c))
        .unwrap_or_else(|| panic!("`{}` has no theme of its own", container.name))
}

fn key<'a>(node: &'a OverseerNode, name: &str) -> Option<&'a OverseerNode> {
    node.children.iter().find(|c| c.name == name)
}

fn names(node: &OverseerNode) -> Vec<String> {
    node.children
        .iter()
        .filter(|c| !c.name.starts_with('_'))
        .map(|c| c.name.clone())
        .collect()
}

fn value(node: &OverseerNode) -> String {
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

const THEMED: &str = r#"context theme {
    - accent = "blue"
    - backdrop {
        - glow {
            - kind = "gradient"
            - from = "red"
            - to = "transparent"
        }
        - base {
            - kind = "gradient"
            - from = "white"
            - to = "grey"
        }
    }
}

tab t (label="T") {
    div plain {
        context theme {
            - backdrop (base=none)
        }
        string a = $(theme/accent)
    }
    div striped {
        context theme {
            - backdrop (base=none) {
                - stripe {
                    - kind = "gradient"
                    - from = "black"
                    - to = "white"
                }
            }
        }
        string b = ""
    }
    div tinted {
        context theme {
            - backdrop {
                - glow {
                    - to = "green"
                }
            }
        }
        string c = ""
    }
}
"#;

#[test]
fn base_none_leaves_an_inherited_block_empty() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = resolved(THEMED);
    let theme = theme_of(&nodes, "plain");
    let backdrop = key(theme, "backdrop").expect("the key itself stays");
    assert!(names(backdrop).is_empty(), "layers left: {:?}", names(backdrop));
    assert_eq!(value(key(theme, "accent").unwrap()), "blue", "the rest of the theme is kept");
}

#[test]
fn base_none_with_a_block_holds_only_that_block() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = resolved(THEMED);
    let backdrop = key(theme_of(&nodes, "striped"), "backdrop").unwrap();
    assert_eq!(names(backdrop), vec!["stripe"]);
    let stripe = key(backdrop, "stripe").unwrap();
    assert_eq!(value(key(stripe, "from").unwrap()), "black");
}

#[test]
fn without_it_an_override_still_merges_by_name() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = resolved(THEMED);
    let backdrop = key(theme_of(&nodes, "tinted"), "backdrop").unwrap();
    assert_eq!(names(backdrop), vec!["glow", "base"]);
    let glow = key(backdrop, "glow").unwrap();
    assert_eq!(value(key(glow, "to").unwrap()), "green");
    assert_eq!(value(key(glow, "from").unwrap()), "red");
}

const INSTANCE: &str = r#"div (hidden=true) {
    div Card {
        div parts {
            string a = "1"
            string b = "2"
        }
        string title = "card"
    }
}

tab t (label="T") {
    <Card> bare {
        - parts (base=none)
    }
    <Card> own {
        - parts (base=none) {
            string c = "3"
        }
    }
}
"#;

#[test]
fn an_instance_element_can_start_over_too() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = resolved(INSTANCE);
    let bare = find(&nodes, "bare").unwrap();
    assert!(names(key(bare, "parts").unwrap()).is_empty());
    assert!(key(bare, "title").is_some(), "the rest of the template is kept");
    let own = find(&nodes, "own").unwrap();
    assert_eq!(names(key(own, "parts").unwrap()), vec!["c"]);
}

fn round_trip(source: &str, reformat: bool) -> String {
    let mut nodes = resolved(source);
    fn drop_fingerprints(nodes: &mut [OverseerNode]) {
        for n in nodes.iter_mut() {
            n.source_fingerprint = None;
            drop_fingerprints(&mut n.children);
        }
    }
    if reformat {
        drop_fingerprints(&mut nodes);
    }
    OverseerFileHandler::serialize_nodes(&nodes).expect("serialize")
}

const FROM_A_NODE: &str = r#"context sunset {
    - dusk {
        - kind = "gradient"
        - from = "orange"
        - to = "purple"
    }
    - haze {
        - kind = "gradient"
        - from = "white"
        - to = "transparent"
    }
}

context theme {
    - backdrop {
        - glow {
            - kind = "gradient"
            - from = "red"
            - to = "transparent"
        }
    }
}

div (hidden=true) {
    div Card {
        div parts {
            string a = "1"
        }
    }
    div spare {
        string s = "spare"
    }
}

tab t (label="T") {
    div evening {
        context theme {
            - backdrop (base=<sunset>) {
                - haze {
                    - to = "grey"
                }
            }
        }
        string a = ""
    }
    div lost {
        context theme {
            - backdrop (base=<nowhere>)
        }
        string b = ""
    }
    <Card> other {
        - parts (base=<spare>)
    }
}
"#;

#[test]
fn base_a_node_starts_from_that_nodes_block() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = resolved(FROM_A_NODE);
    let backdrop = key(theme_of(&nodes, "evening"), "backdrop").unwrap();
    assert_eq!(names(backdrop), vec!["dusk", "haze"]);
    let haze = key(backdrop, "haze").unwrap();
    assert_eq!(value(key(haze, "to").unwrap()), "grey", "its own block merges onto it");
    assert_eq!(value(key(haze, "from").unwrap()), "white");

    let other = find(&nodes, "other").unwrap();
    assert_eq!(names(key(other, "parts").unwrap()), vec!["s"]);
}

#[test]
fn a_node_that_is_not_there_is_said_on_the_theme_it_was_for() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = resolved(FROM_A_NODE);
    match theme_of(&nodes, "lost").parameters.get("_inherit_error") {
        Some(OverseerValue::String(why)) => assert!(why.contains("nowhere"), "{}", why),
        other => panic!("no reason given: {:?}", other),
    }
    let backdrop = key(theme_of(&nodes, "lost"), "backdrop").unwrap();
    assert_eq!(names(backdrop), vec!["glow"], "what it would have started from stays");
}

#[test]
fn a_document_that_starts_over_saves_as_written() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for source in [THEMED, INSTANCE, FROM_A_NODE] {
        assert_eq!(round_trip(source, false), source);
        assert_eq!(round_trip(source, true), source, "reformatted");
    }
}

