//! An instance can be made from more than one template: `<Card, Fancy> one { ... }` is `Card`,
//! then what `Fancy` writes merged onto it, then the instance's own block - left to right, any
//! number of them. A list entry stacks on its list's template the same way: `- <Fancy> { }`.
//!
//! What follows the first gives what it writes: a plain template everything it holds, a template
//! made from another what the two come to, and an entry of a keyed list - `<Card, Things/[small]>`
//! - its own lines alone. Nothing a stack gave is the instance's own, so a save gives back what
//! was written and an edit of an inherited value is one override.

use overseer::server::DocumentRoot;
use overseer::types::*;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static SERIAL: Mutex<()> = Mutex::new(());

const DOC: &str = r##"tab t (label="T", mutable=true) {
    div (hidden=true) {
        div Card {
            string title (label="Title") = ""
            string note = "plain"
            int size = 1
        }
        div Fancy (background="#eee") {
            string note (font-weight="bold") = "fancy"
            string ribbon = "gold"
        }
        div Big {
            int size = 3
        }
        <Fancy> Fancier {
            - ribbon = "silver"
        }
        div Thing (layout="horizontal") {
            string title (width=40%) = ""
            int size = 0
        }
    }

    list Things (entry=<Thing>, key="title") {
        - {
            - title = "small"
            - size = 2
        }
    }

    <Card, Fancy> one {
        - title = "One"
    }
    <Card, Fancy, Big> two {
        - note = "mine"
    }
    <Card, Fancier> three { }
    <Card, Things/[small]> four { }
    <Card, Missing, Big> five {
        - title = "Five"
    }

    list Cards (entry=<Card>) {
        - <Fancy, Big> {
            - title = "in a list"
        }
        - <Things/[small]> { }
    }
}
"##;

fn folder(test: &str, text: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_stack_{}_{}", std::process::id(), test));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("d.os"), text).unwrap();
    root
}

fn read(root: &Path, address: &str) -> OverseerNode {
    DocumentRoot::new(root)
        .unwrap()
        .read_at("d.os", address)
        .unwrap_or_else(|e| panic!("nothing read at {}: {:?}", address, e))
        .node
}

fn value(root: &Path, address: &str) -> OverseerValue {
    let node = read(root, address);
    node.parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
        .cloned()
        .unwrap_or_else(|| panic!("no value at {}", address))
}

fn text(value: &str) -> OverseerValue {
    OverseerValue::String(value.to_string())
}

fn on_disk(root: &Path) -> String {
    std::fs::read_to_string(root.join("d.os")).unwrap()
}

fn saved_again(content: &str) -> String {
    let nodes = overseer::app_api::load_document(content.to_string()).unwrap();
    overseer::OverseerFileHandler::serialize_nodes(&nodes).unwrap()
}

#[test]
fn each_template_is_merged_onto_the_ones_before_it() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("merged", DOC);

    assert_eq!(value(&root, "t/one/title"), text("One"));
    assert_eq!(value(&root, "t/one/note"), text("fancy"));
    assert_eq!(value(&root, "t/one/ribbon"), text("gold"));
    assert_eq!(value(&root, "t/one/size"), OverseerValue::Integer(1));
    // A plain template gives everything it holds: its own parameters, and its fields'.
    let one = read(&root, "t/one");
    assert_eq!(one.node_type, "Card");
    assert_eq!(one.parameters.get("background"), Some(&text("#eee")));
    let note = read(&root, "t/one/note");
    assert_eq!(note.parameters.get("font-weight"), Some(&text("bold")));
    assert_eq!(read(&root, "t/one/title").parameters.get("label"), Some(&text("Title")));

    // Three of them, and the instance's own block over all.
    assert_eq!(value(&root, "t/two/note"), text("mine"));
    assert_eq!(value(&root, "t/two/ribbon"), text("gold"));
    assert_eq!(value(&root, "t/two/size"), OverseerValue::Integer(3));

    // A template made from another comes with what it was made from.
    assert_eq!(value(&root, "t/three/note"), text("fancy"));
    assert_eq!(value(&root, "t/three/ribbon"), text("silver"));

    // An entry of a list gives its lines, and not its list's template's look.
    assert_eq!(value(&root, "t/four/title"), text("small"));
    assert_eq!(value(&root, "t/four/size"), OverseerValue::Integer(2));
    let four = read(&root, "t/four");
    assert_eq!(four.parameters.get("layout"), None);
    assert_eq!(read(&root, "t/four/title").parameters.get("width"), None);
    assert_eq!(read(&root, "t/four/title").parameters.get("label"), Some(&text("Title")));
}

#[test]
fn a_list_entry_stacks_on_its_lists_template() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("entries", DOC);
    assert_eq!(value(&root, "t/Cards/Card__1/title"), text("in a list"));
    assert_eq!(value(&root, "t/Cards/Card__1/note"), text("fancy"));
    assert_eq!(value(&root, "t/Cards/Card__1/size"), OverseerValue::Integer(3));
    assert_eq!(value(&root, "t/Cards/Card__2/title"), text("small"));
    assert_eq!(value(&root, "t/Cards/Card__2/note"), text("plain"));
}

#[test]
fn a_template_that_is_not_there_is_said_and_the_rest_applied() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("missing", DOC);
    let five = read(&root, "t/five");
    match five.parameters.get("_inherit_error") {
        Some(OverseerValue::String(why)) => assert!(why.contains("Missing"), "{}", why),
        other => panic!("says {:?}", other),
    }
    assert_eq!(value(&root, "t/five/title"), text("Five"));
    assert_eq!(value(&root, "t/five/size"), OverseerValue::Integer(3));
    assert!(read(&root, "t/one").parameters.get("_inherit_error").is_none());
}

#[test]
fn an_entry_cannot_stand_first() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("first", &DOC.replace("<Card, Things/[small]> four", "<Things/[small]> four"));
    let nodes = DocumentRoot::new(&root).unwrap().open("d.os").unwrap();
    fn said(nodes: &[OverseerNode], out: &mut Vec<String>) {
        for node in nodes {
            if let Some(OverseerValue::String(why)) = node.parameters.get("_inherit_error") {
                out.push(why.clone());
            }
            said(&node.children, out);
        }
    }
    let mut errors = Vec::new();
    said(&nodes, &mut errors);
    assert!(
        errors.iter().any(|why| why.contains("can only follow a template")),
        "{:?}",
        errors
    );
}

#[test]
fn a_save_gives_back_what_was_written() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    assert_eq!(saved_again(DOC), DOC);
    let windows = DOC.replace('\n', "\r\n");
    assert_eq!(saved_again(&windows), windows);
}

#[test]
fn an_edit_of_an_inherited_value_is_one_override() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("edit", DOC);
    let service = DocumentRoot::new(&root).unwrap();

    service.set_at("d.os", "t/one/ribbon", text("red")).unwrap();
    let after_one = DOC.replace(
        "        - title = \"One\"\n",
        "        - title = \"One\"\n        - ribbon = \"red\"\n",
    );
    assert_eq!(on_disk(&root), after_one);
    assert_eq!(value(&root, "t/one/ribbon"), text("red"));
    assert_eq!(value(&root, "t/one/note"), text("fancy"));

    service.set_at("d.os", "t/Cards/Card__2/note", text("kept")).unwrap();
    let after_two = after_one.replace(
        "        - <Things/[small]> { }\n",
        "        - <Things/[small]> {\n            - note = \"kept\"\n        }\n",
    );
    assert_eq!(on_disk(&root), after_two);
    assert_eq!(value(&root, "t/Cards/Card__2/title"), text("small"));

    // What the stack names changes, and what was made from it follows once the document is
    // read again - as an instance follows its template's own values.
    service.set_at("d.os", "t/Things/[small]/size", OverseerValue::Integer(7)).unwrap();
    let saved = on_disk(&root);
    assert_eq!(saved, after_two.replace("- size = 2", "- size = 7"));
    let nodes = overseer::app_api::load_document(saved).unwrap();
    for address in ["t/four/size", "t/Cards/Card__2/size"] {
        let field = overseer::addressing::find(&nodes, address).unwrap();
        assert_eq!(field.parameters.get("value"), Some(&OverseerValue::Integer(7)), "{}", address);
    }
}
