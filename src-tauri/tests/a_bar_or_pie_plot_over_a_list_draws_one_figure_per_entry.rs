//! A bar or pie plot with a `source` gives one figure per entry of the list it reads, each
//! worked out by its lambdas on that entry, and an edit to an entry changes its figure.

use overseer::app_api;
use overseer::parser;
use overseer::resolver;
use overseer::types::{OverseerNode, OverseerValue};

const DOC: &str = r##"tab shop (mutable=true) {
    div (hidden=true) {
        div Shelf {
            string name = ""
            string colour = "#888888"
            int stocked = 0
            int room = 10
        }
    }
    list Shelves (entry=<Shelf>, key="name") {
        - {
            - name = "tea"
            - colour = "#ff0000"
            - stocked = 4
        }
        - {
            - name = "rice"
            - stocked = 7
            - room = 20
        }
    }
    chart fill (kind="bar") {
        plot first (label="first", amount=1, limit=2)
        plot each (source=$(/shop/Shelves), label=$(|s| s/name), color=$(|s| s/colour),
                   amount=$(|s| s/stocked), limit=$(|s| s/room), suffix=" kg")
    }
    chart share (kind="pie") {
        plot each (source=$(/shop/Shelves), label=$(|s| s/name), color="#00ff00", amount=$(|s| s/stocked))
    }
}
"##;

fn find<'a>(nodes: &'a [OverseerNode], path: &[&str]) -> &'a OverseerNode {
    fn walk<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
        for n in nodes {
            if n.name == name {
                return Some(n);
            }
            if n.name.is_empty() {
                if let Some(found) = walk(&n.children, name) {
                    return Some(found);
                }
            }
        }
        None
    }
    let mut here = nodes;
    let mut node = None;
    for step in path {
        node = Some(walk(here, step).unwrap_or_else(|| panic!("no {step}")));
        here = &node.unwrap().children;
    }
    node.unwrap()
}

fn bars(nodes: &[OverseerNode], chart: &str) -> serde_json::Value {
    let plot = find(nodes, &["shop", chart, "each"]);
    match plot.parameters.get("_computed_bars") {
        Some(OverseerValue::String(s)) => serde_json::from_str(s).expect("bars are JSON"),
        other => panic!("no figures on {chart}: {other:?}"),
    }
}

#[test]
fn a_bar_plot_over_a_list_gives_each_entry_its_own_figures() {
    let (_rest, mut nodes) = parser::parse_document(DOC).expect("parse");
    resolver::resolve_document(&mut nodes);

    assert_eq!(
        bars(&nodes, "fill"),
        serde_json::json!([
            {"label": "tea", "color": "#ff0000", "amount": 4.0, "limit": 10.0},
            {"label": "rice", "color": "#888888", "amount": 7.0, "limit": 20.0},
        ])
    );
    // A plot of its own beside it keeps its own figures and gets no list of them.
    let first = find(&nodes, &["shop", "fill", "first"]);
    assert!(first.parameters.get("_computed_bars").is_none());
}

#[test]
fn a_pie_plot_over_a_list_shares_a_plain_value_between_its_entries() {
    let (_rest, mut nodes) = parser::parse_document(DOC).expect("parse");
    resolver::resolve_document(&mut nodes);

    assert_eq!(
        bars(&nodes, "share"),
        serde_json::json!([
            {"label": "tea", "color": "#00ff00", "amount": 4.0},
            {"label": "rice", "color": "#00ff00", "amount": 7.0},
        ])
    );
}

#[test]
fn an_edit_to_an_entry_changes_its_figure() {
    app_api::load_document_with_dependencies(DOC.to_string()).expect("load");
    let field = "shop/Shelves/Shelf__2/stocked".to_string();
    let mut values = std::collections::HashMap::new();
    values.insert(field.clone(), OverseerValue::Integer(12));
    let nodes = app_api::resolve_selective(DOC.to_string(), vec![field], Some(values)).expect("selective");

    let figures = bars(&nodes, "fill");
    assert_eq!(figures[1]["amount"], serde_json::json!(12.0));
    assert_eq!(figures[0]["amount"], serde_json::json!(4.0));
}
