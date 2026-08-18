//! Buying something records where it was bought.
//!
//! The `bought` button copies the entry into History. Which shop it came from is not on the
//! entry - it is on the page, in `Selected/selected_shop`, because an item that two shops
//! sell belongs to neither until someone stands in one of them. So the append block reads
//! that field by absolute path, and this pins that it can: a relative-path habit would
//! silently write nothing and the history would say the milk came from nowhere.

use overseer::app_api;
use overseer::docmgr::manager::DocumentManager;
use overseer::types::*;

fn document() -> (String, Vec<OverseerNode>) {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/shopping/shopping.os");
    DocumentManager::set_current_document(Some(p.to_string_lossy().as_ref()));
    let text = std::fs::read_to_string(&p).unwrap();
    let nodes = app_api::load_document(text.clone()).unwrap();
    (text, nodes)
}

fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    for n in nodes {
        if n.name == name {
            return Some(n);
        }
        if let Some(found) = find(&n.children, name) {
            return Some(found);
        }
    }
    None
}

/// Path to the `bought` button of the entry whose `handle` field holds `handle`.
fn buy_button(nodes: &[OverseerNode], handle: &str) -> Vec<String> {
    let list = find(nodes, "List").expect("no shopping list");
    for entry in &list.children {
        let stored = find(std::slice::from_ref(entry), "handle")
            .and_then(|h| h.parameters.get("value").cloned())
            .map(|v| format!("{:?}", v))
            .unwrap_or_default();
        if stored.contains(handle) {
            return vec![
                "shopping".into(),
                "List".into(),
                entry.name.clone(),
                "bought".into(),
            ];
        }
    }
    panic!("no entry for {handle} in the fixture");
}

/// Every value under a node, by field name. A computed field answers with what it computed.
fn fields(node: &OverseerNode) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    fn walk(n: &OverseerNode, out: &mut std::collections::HashMap<String, String>) {
        if let Some(v) = n.parameters.get("_computed_value").or(n.parameters.get("value")) {
            out.insert(n.name.clone(), format!("{:?}", v));
        }
        for c in &n.children {
            walk(c, out);
        }
    }
    for c in &node.children {
        walk(c, &mut out);
    }
    out
}

#[test]
fn buying_records_the_shop_that_was_being_looked_at() {
    let (_, mut nodes) = document();

    // The fixture opens on Lidl, which sells bread.
    let path = buy_button(&nodes, "bread");
    app_api::execute_event(&mut nodes, &path, "click").expect("click");

    let history = find(&nodes, "History").expect("no history");
    assert_eq!(history.children.len(), 1, "the purchase did not reach History");
    let entry = fields(&history.children[0]);

    assert!(
        entry.get("shop").is_some_and(|s| s.contains("lidl")),
        "the history entry did not record the shop: {entry:?}"
    );
    assert!(
        entry.get("handle").is_some_and(|s| s.contains("bread")),
        "the wrong entry was bought: {entry:?}"
    );
    DocumentManager::set_current_document(None);
}

#[test]
fn the_shop_shows_as_its_name_rather_than_its_tag() {
    let (text, nodes) = document();
    let path = buy_button(&nodes, "bread");

    // Through the text, as the frontend does it, so the entry comes back resolved: the tag is
    // what is stored and the name is what is read, and only a resolve turns one into the other.
    let after = app_api::execute_event_on_text(text, path, "click".to_string()).expect("click");
    let nodes = app_api::load_document(after.text).expect("reload");

    let history = find(&nodes, "History").expect("no history");
    let entry = fields(&history.children[0]);
    assert!(
        entry.get("shop_name").is_some_and(|s| s.contains("Lidl")),
        "the shop tag was not looked up into a name: {entry:?}"
    );
    DocumentManager::set_current_document(None);
}
