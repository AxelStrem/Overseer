//! The drawer Overseer ships, `library/drawer/drawer.os`, draws everything the page's own drawer
//! drew: the documents kept to hand, with folders; keeping the open one and editing the list; the
//! open document's path and Undo; the look; and for a person signed in, their chip, profile,
//! password, keys and sharing, and for an admin the links that let others in. Each part shows to
//! whoever it is for, and keeping a document writes one entry into the person's own drawer.
#![cfg(feature = "server")]

use overseer::access::Asker;
use overseer::drawer;
use overseer::environment::{as_admin, beside};
use overseer::server::DocumentRoot;
use overseer::types::{OverseerNode, OverseerValue};
use std::path::{Path, PathBuf};

const MINE: &str = "<drawer/drawer.os/drawer> drawer {\r\n    - Places {\r\n        - {\r\n            - name = \"Tasks\"\r\n            - path = \"tasks.os\"\r\n        }\r\n        - {\r\n            - name = \"Projects\"\r\n            list Places {\r\n                - {\r\n                    - name = \"Spores\"\r\n                    - path = \"projects/spores.os\"\r\n                }\r\n            }\r\n        }\r\n    }\r\n}\r\n";

fn write(root: &Path, name: &str, text: &str) {
    let file = root.join(name);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

fn served(test: &str) -> (DocumentRoot, PathBuf) {
    let root = std::env::temp_dir().join(format!("overseer-base-drawer-{}-{}", test, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write(&root, "ap31/drawer.os", MINE);
    write(&root, "ap31/tasks.os", "tab tasks (label=\"Chores\", mutable=true) {\n    string note = \"\"\n}\n");
    write(&root, "ap31/projects/spores.os", "tab spores (label=\"Spores\") {\n}\n");
    write(&root, "ap31/food.os", "tab food (label=\"Food log\", mutable=true) {\n}\n");
    let documents = DocumentRoot::new(&root).unwrap().with_accounts(|name| name == "ap31");
    (documents, root)
}

fn hidden(node: &OverseerNode) -> bool {
    matches!(
        node.parameters.get("_computed_hidden").or(node.parameters.get("hidden")),
        Some(OverseerValue::Boolean(true))
    )
}

/// Whether the node of that name is drawn: there, and neither it nor anything around it hidden.
/// Nodes inside the hidden templates are not looked in.
fn shown(nodes: &[OverseerNode], name: &str) -> Option<bool> {
    fn walk(nodes: &[OverseerNode], name: &str, above: bool) -> Option<bool> {
        for node in nodes {
            if node.node_type == "context" {
                continue;
            }
            let here = above || hidden(node);
            if node.name == name {
                return Some(!here);
            }
            if let Some(found) = walk(&node.children, name, here) {
                return Some(found);
            }
        }
        None
    }
    let drawer = nodes.iter().find(|n| n.name == "drawer").expect("the drawer's tab");
    // The templates are no part of what is drawn.
    let drawn: Vec<OverseerNode> = drawer
        .children
        .iter()
        .filter(|c| !c.children.iter().any(|t| t.name == "Place"))
        .cloned()
        .collect();
    walk(&drawn, name, false)
}

fn is_shown(nodes: &[OverseerNode], name: &str) -> bool {
    shown(nodes, name).unwrap_or_else(|| panic!("the drawer has no {}", name))
}

fn person() -> Asker {
    Asker::Person("ap31".into())
}

fn drawn(documents: &DocumentRoot, asker: Asker, admin: bool, open: Option<&str>) -> Vec<OverseerNode> {
    documents.as_asker(asker, || {
        as_admin(admin, || {
            let open = open.map(|name| documents.open_beside(name));
            beside(open, || documents.open(&documents_drawer(documents)).unwrap())
        })
    })
}

fn documents_drawer(documents: &DocumentRoot) -> String {
    let named = documents.command(None, "drawer_location", &serde_json::json!({})).unwrap();
    named.as_str().unwrap().to_string()
}

#[test]
fn a_person_sees_every_part_of_the_drawer_and_an_admin_the_people_too() {
    let (documents, _) = served("person");
    let nodes = drawn(&documents, person(), true, Some("tasks.os"));
    for part in [
        "Places", "edit", "all", "undo", "look", "shade", "week", "who", "sharing", "Profile", "save", "current",
        "password", "change", "Keys", "make", "People", "invite", "reset", "sign_out",
    ] {
        assert!(is_shown(&nodes, part), "{} is drawn", part);
    }
    // Opening and making files are the desktop's.
    assert!(!is_shown(&nodes, "open_file"));
    assert!(!is_shown(&nodes, "new_file"));
    // The open document is kept already, in the list's top level.
    assert!(!is_shown(&nodes, "keep"));

    let nodes = drawn(&documents, person(), false, Some("projects/spores.os"));
    assert!(!is_shown(&nodes, "keep"), "kept inside a folder is kept too");
    assert!(!is_shown(&nodes, "People"), "only an admin lets others in");

    let nodes = drawn(&documents, person(), false, Some("food.os"));
    assert!(is_shown(&nodes, "keep"));
}

#[test]
fn the_open_document_is_marked_in_the_list() {
    let (documents, _) = served("marked");
    let nodes = drawn(&documents, person(), false, Some("projects/spores.os"));
    let here = |nodes: &[OverseerNode]| -> Vec<String> {
        fn walk(nodes: &[OverseerNode], out: &mut Vec<String>) {
            for node in nodes {
                if node.name.starts_with("Place__") {
                    let field = |name: &str| node.children.iter().find(|c| c.name == name).and_then(|c| {
                        c.parameters.get("_computed_value").or(c.parameters.get("value")).cloned()
                    });
                    if field("here") == Some(OverseerValue::Boolean(true)) {
                        if let Some(OverseerValue::String(name)) = field("name") {
                            out.push(name);
                        }
                    }
                }
                walk(&node.children, out);
            }
        }
        let mut out = Vec::new();
        let drawer = nodes.iter().find(|n| n.name == "drawer").unwrap();
        let places = drawer.children.iter().flat_map(|c| std::iter::once(c).chain(c.children.iter()))
            .find(|c| c.name == "Places" && c.node_type == "list")
            .expect("the list of places");
        walk(&places.children, &mut out);
        out
    };
    assert_eq!(here(&nodes), vec!["Spores".to_string()]);
}

#[test]
fn a_stranger_sees_the_open_document_and_none_of_what_is_a_persons() {
    let (documents, _) = served("stranger");
    let nodes = drawn(&documents, Asker::Stranger, false, None);
    for part in ["Places", "edit", "all", "look", "shade", "week", "keep", "sharing", "Profile", "sign_out"] {
        assert!(!is_shown(&nodes, part), "{} is not offered a stranger", part);
    }
}

#[test]
fn a_server_without_accounts_offers_the_list_and_the_look_and_no_account() {
    let (documents, root) = served("token");
    write(&root, drawer::FILE, MINE);
    let nodes = drawn(&documents, Asker::Anyone, false, Some("ap31/food.os"));
    for part in ["Places", "keep", "edit", "all", "undo", "look", "shade", "week"] {
        assert!(is_shown(&nodes, part), "{} is drawn", part);
    }
    for part in ["sharing", "Profile", "Keys", "sign_out"] {
        assert!(!is_shown(&nodes, part), "{} needs an account", part);
    }
}

#[test]
fn keeping_the_open_document_writes_one_entry_into_the_persons_drawer() {
    let (documents, root) = served("keep");
    documents.as_asker(person(), || {
        let open = documents.open_beside("food.os");
        beside(Some(open), || documents.run_event("drawer.os", "drawer/keep", "click")).expect("the press");
    });
    let text = std::fs::read_to_string(root.join("ap31/drawer.os")).unwrap();
    assert_eq!(
        text,
        MINE.replace(
            "    }\r\n}\r\n",
            "        - {\r\n            - name = \"Food log\"\r\n            - path = \"food.os\"\r\n        }\r\n    }\r\n}\r\n"
        ),
        "{}",
        text
    );
    let nodes = drawn(&documents, person(), false, Some("food.os"));
    assert!(!is_shown(&nodes, "keep"), "kept now");
}
