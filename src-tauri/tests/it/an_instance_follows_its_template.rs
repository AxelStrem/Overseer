//! What is made from a template follows a change to the template at once: an instance, an entry of
//! a list made from it, a template made from it and that one's instances, and whatever reads any of
//! them. The same for what is stacked from an entry of a keyed list, `<Card, Things/[small]>`.
//!
//! A write used to settle only what the dependency graph says reads the changed field, and a copy
//! made from a template is no reader of it - nor did working the held document out whole make the
//! copies again. The file was right, and the document held for it kept every instance as it was
//! until it was read from its text.

use crate::common::serial;
use overseer::app_api;
use overseer::server::DocumentRoot;
use overseer::types::*;
use std::path::{Path, PathBuf};


const DOC: &str = r##"tab t (label="T", mutable=true) {
    div (hidden=true) {
        div Card {
            string title (label="Title") = ""
            string note = "plain"
            int size = 1
            int twice = $(size * 2)
        }
        <Card> Wide {
            - title = "wide"
        }
        div Thing {
            string title = ""
            int size = 0
        }
    }

    list Things (entry=<Thing>, key="title") {
        - {
            - title = "small"
            - size = 2
        }
        - {
            - title = "other"
            - size = 4
        }
    }

    <Card> one {
        - title = "One"
    }
    <Wide> broad { }
    <Card, Things/[small]> four { }

    list Cards (entry=<Card>) {
        - {
            - title = "in a list"
        }
        - <Things/[small]> { }
    }

    string shown = $(one/note)
    int sizes = $(Cards.sum(|x| x/size))

    button mark (label="Mark") {
        on click {
            set (path="/t/Card/note") = "pressed"
        }
    }
}
"##;

fn folder(test: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_follow_{}_{}", std::process::id(), test));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("d.os"), DOC).unwrap();
    root
}

fn value_of(node: &OverseerNode) -> OverseerValue {
    node.parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
        .cloned()
        .unwrap_or_else(|| panic!("no value on {}", node.name))
}

fn value(service: &DocumentRoot, address: &str) -> OverseerValue {
    value_of(
        &service
            .read_at("d.os", address)
            .unwrap_or_else(|e| panic!("nothing read at {}: {:?}", address, e))
            .node,
    )
}

fn text(value: &str) -> OverseerValue {
    OverseerValue::String(value.to_string())
}

fn on_disk(root: &Path) -> String {
    std::fs::read_to_string(root.join("d.os")).unwrap()
}

const NOTES: [&str; 6] =
    ["t/one/note", "t/broad/note", "t/four/note", "t/Cards/Card__1/note", "t/Cards/Card__2/note", "t/shown"];

#[test]
fn a_value_set_on_a_template_is_its_instances_at_once() {
    let _guard = serial();
    let root = folder("template");
    let service = DocumentRoot::new(&root).unwrap();
    // Read first, so the document is held worked out and the writes start from it.
    for address in NOTES {
        assert_eq!(value(&service, address), text("plain"), "{address} to begin with");
    }

    for said in ["changed", "again"] {
        service.set_at("d.os", "t/Card/note", text(said)).unwrap();
        for address in NOTES {
            assert_eq!(value(&service, address), text(said), "{address} after the template said {said}");
        }
    }

    // What an instance works out from the changed field follows as well.
    service.set_at("d.os", "t/Card/size", OverseerValue::Integer(5)).unwrap();
    assert_eq!(value(&service, "t/one/size"), OverseerValue::Integer(5));
    assert_eq!(value(&service, "t/one/twice"), OverseerValue::Integer(10));
    assert_eq!(value(&service, "t/broad/twice"), OverseerValue::Integer(10));
    // And what the instance was given by something stacked on the template stays that.
    assert_eq!(value(&service, "t/four/size"), OverseerValue::Integer(2));
    assert_eq!(value(&service, "t/sizes"), OverseerValue::Integer(7));

    // One line of the file changed for each field, and nothing was written into an instance.
    let expected = DOC
        .replace("string note = \"plain\"", "string note = \"again\"")
        .replace("int size = 1", "int size = 5");
    assert_eq!(on_disk(&root), expected);
}

#[test]
fn a_value_set_on_an_entry_is_what_was_stacked_from_it_at_once() {
    let _guard = serial();
    let root = folder("entry");
    let service = DocumentRoot::new(&root).unwrap();
    assert_eq!(value(&service, "t/four/size"), OverseerValue::Integer(2));

    for size in [9, 10] {
        service.set_at("d.os", "t/Things/[small]/size", OverseerValue::Integer(size)).unwrap();
        assert_eq!(value(&service, "t/four/size"), OverseerValue::Integer(size));
        assert_eq!(value(&service, "t/four/twice"), OverseerValue::Integer(size * 2));
        assert_eq!(value(&service, "t/Cards/Card__2/size"), OverseerValue::Integer(size));
        assert_eq!(value(&service, "t/sizes"), OverseerValue::Integer(size + 1));
    }

    assert_eq!(on_disk(&root), DOC.replace("- size = 2", "- size = 10"));
}

#[test]
fn a_press_that_sets_a_template_shows_its_instances_changed() {
    let _guard = serial();
    let root = folder("press");
    let service = DocumentRoot::new(&root).unwrap();
    assert_eq!(value(&service, "t/one/note"), text("plain"));

    // What the press answers with is the document as the caller is about to be shown it.
    let pressed = service.run_event("d.os", "t/mark", "click").unwrap();
    let shown = pressed.node.expect("the tab the button is in");
    let find = |name: &str| -> OverseerNode {
        fn walk<'a>(node: &'a OverseerNode, name: &str) -> Option<&'a OverseerNode> {
            node.children.iter().find_map(|c| if c.name == name { Some(c) } else { walk(c, name) })
        }
        walk(&shown, name).unwrap_or_else(|| panic!("no {name} in the answer")).clone()
    };
    assert_eq!(value_of(&find("shown")), text("pressed"));
    assert_eq!(value_of(find("one").children.iter().find(|c| c.name == "note").unwrap()), text("pressed"));
    for address in NOTES {
        assert_eq!(value(&service, address), text("pressed"), "{address} after the press");
    }
}

#[test]
fn the_desktops_press_shows_them_changed_too() {
    let _guard = serial();
    // Opened, so the document is held with its graph, as the page's is.
    app_api::load_document(DOC.to_string()).unwrap();
    let update = app_api::execute_event_update(DOC.to_string(), vec!["t".into(), "mark".into()], "click".into())
        .unwrap();
    let nodes = app_api::load_document(update.text.clone()).unwrap();
    for address in NOTES {
        let node = overseer::addressing::find(&nodes, address).unwrap();
        assert_eq!(value_of(node), text("pressed"), "{address} in the document held for the new text");
    }

    let pressed =
        app_api::execute_event_on_text(DOC.to_string(), vec!["t".into(), "mark".into()], "click".into()).unwrap();
    for address in NOTES {
        let node = overseer::addressing::find(&pressed.nodes, address).unwrap();
        assert_eq!(value_of(node), text("pressed"), "{address} in what the press handed back");
    }
}

#[test]
fn a_write_anywhere_else_is_still_followed_through_the_graph() {
    let _guard = serial();
    let root = folder("elsewhere");
    let service = DocumentRoot::new(&root).unwrap();
    assert_eq!(value(&service, "t/one/twice"), OverseerValue::Integer(2));

    let before = overseer::resolver::times_worked_out_whole();
    service.set_at("d.os", "t/one/size", OverseerValue::Integer(3)).unwrap();
    service.set_at("d.os", "t/Things/[other]/size", OverseerValue::Integer(6)).unwrap();
    assert_eq!(value(&service, "t/one/twice"), OverseerValue::Integer(6));
    assert_eq!(
        overseer::resolver::times_worked_out_whole(),
        before,
        "an instance's own field, and an entry nothing is stacked from, are no template"
    );
}
