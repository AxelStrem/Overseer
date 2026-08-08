//! Navigating days has to accumulate.
//!
//! `Prev Day` sets `selected_date` to `date_add_days(../selected_date, -1)`, so each click has
//! to see where the last one left it. The click now travels as the document's text rather than
//! as the document, which means the value an action writes must survive serialization - if the
//! node is written back out as the `$(today())` it was authored with, every click computes from
//! today and navigation goes one day and stops.

use overseer::app_api;
use overseer::docmgr::manager::DocumentManager;
use overseer::types::*;

fn find_path(nodes: &[OverseerNode], name: &str, prefix: Vec<String>) -> Option<Vec<String>> {
    for (i, n) in nodes.iter().enumerate() {
        let ordinal = nodes[..i].iter().filter(|s| s.name == n.name).count();
        let mut here = prefix.clone();
        here.push(if ordinal == 0 {
            n.name.clone()
        } else {
            format!("{}#{}", n.name, ordinal)
        });
        if n.name == name {
            return Some(here);
        }
        if let Some(found) = find_path(&n.children, name, here) {
            return Some(found);
        }
    }
    None
}

fn selected_date(nodes: &[OverseerNode]) -> Option<OverseerValue> {
    fn go(nodes: &[OverseerNode]) -> Option<OverseerValue> {
        for n in nodes {
            if n.name == "selected_date" {
                return n
                    .parameters
                    .get("_computed_value")
                    .or_else(|| n.parameters.get("value"))
                    .cloned();
            }
            if let Some(v) = go(&n.children) {
                return Some(v);
            }
        }
        None
    }
    go(nodes)
}

#[test]
fn each_click_moves_another_day() {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/weight_tracker/tracker_v2.os");
    DocumentManager::set_current_document(Some(p.to_string_lossy().as_ref()));
    let text = std::fs::read_to_string(&p).unwrap();

    let loaded = app_api::load_document(text.clone()).unwrap();
    let start = selected_date(&loaded).expect("no selected_date in the document");
    let prev = find_path(&loaded, "Prev", Vec::new()).expect("no Prev button");

    let first = app_api::execute_event_on_text(text, prev.clone(), "click".to_string())
        .expect("first click");
    let after_one = selected_date(&first.nodes).expect("selected_date vanished");
    assert_ne!(after_one, start, "the first click did not move the date");

    // The second click starts from the text the first returned, as the app does.
    let second = app_api::execute_event_on_text(first.text, prev, "click".to_string())
        .expect("second click");
    let after_two = selected_date(&second.nodes).expect("selected_date vanished");

    assert_ne!(
        after_two, after_one,
        "the second click landed on the same day as the first: the date it wrote did not \
         survive into the text the click was replayed from"
    );
    DocumentManager::set_current_document(None);
}
