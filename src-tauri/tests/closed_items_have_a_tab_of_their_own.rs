//! Closed items have a tab of their own.
//!
//! A project page holds the open list and, below it, every item ever finished or cancelled -
//! which grows without end and pushed the tags further down with each one. The template splits
//! it into two tabs, Open and Closed. They are unnamed, so they are no step of an address: the
//! bot, the sweep, the guide and the template's own formulas all say `/project/Items` and
//! `/project/History`, and those must keep meaning what they did.

use overseer::addressing;
use overseer::app_api;
use overseer::types::*;

const TEMPLATE: &str = include_str!("../../examples/projects/project_template.os");

fn label(node: &OverseerNode) -> String {
    match node.parameters.get("label") {
        Some(OverseerValue::String(s)) => s.clone(),
        _ => String::new(),
    }
}

/// The chain of nodes from the top of the document down to the first one named `name`.
fn chain_to<'a>(level: &'a [OverseerNode], name: &str) -> Option<Vec<&'a OverseerNode>> {
    for node in level {
        if node.name == name && node.node_type == "list" {
            return Some(vec![node]);
        }
        if let Some(mut below) = chain_to(&node.children, name) {
            below.insert(0, node);
            return Some(below);
        }
    }
    None
}

/// The label of the tab a list sits in directly, and whether that tab is a step of an address.
fn tab_around(nodes: &[OverseerNode], list: &str) -> (String, bool) {
    let chain = chain_to(nodes, list).unwrap_or_else(|| panic!("no list {}", list));
    let tab = chain[chain.len() - 2];
    assert_eq!(tab.node_type, "tab", "{} is not directly in a tab", list);
    (label(tab), !tab.is_hierarchy_transparent)
}

#[test]
fn the_open_list_and_the_closed_one_are_on_tabs_of_their_own() {
    let nodes = app_api::load_document(TEMPLATE.to_string()).expect("the template loads");
    assert_eq!(tab_around(&nodes, "Items"), ("Open".to_string(), false));
    assert_eq!(tab_around(&nodes, "History"), ("Closed".to_string(), false));
}

#[test]
fn the_lists_keep_their_addresses() {
    let nodes = app_api::load_document(TEMPLATE.to_string()).expect("the template loads");
    for address in ["project/Items", "project/History", "project/Labels", "project/NewTask"] {
        assert!(addressing::find(&nodes, address).is_some(), "nothing at {}", address);
    }
}
