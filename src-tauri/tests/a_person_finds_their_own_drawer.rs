//! Each person's drawer is a document of their own, beside their settings, inheriting the one
//! Overseer ships. It is made the first time it is asked for, holding what their `menu.os` held;
//! whoever has no settings of their own is given the base, to read and never to write.

use overseer::access::{Asker, Need};
use overseer::drawer;
use overseer::menu;
use overseer::server::{DocumentRoot, RequestError};
use overseer::types::{OverseerNode, OverseerValue};
use serde_json::json;
use std::path::{Path, PathBuf};

const MENU: &str = r#"tab menu (label="Documents", mutable=true) {
    div (hidden=true) {
        div Place {
            string name = ""
            string folder = ""
            string path = ""
        }
    }

    list Places (entry=<Place>, key="path") {
        - {
            - path = "tasks.os"
            - name = "Tasks"
        }
        - {
            - path = "projects/overseer.os"
            - folder = "Projects"
        }
        - {
            - path = "food.os"
        }
        - {
            - path = "projects/spores.os"
            - name = "Spores"
            - folder = "Projects"
        }
        - {
            - path = "other/tasks.os"
            - name = "Tasks"
        }
    }
}
"#;

fn write(root: &Path, name: &str, text: &str) {
    let file = root.join(name);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

fn a_root(test: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("overseer-drawer-{}-{}", test, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root.canonicalize().unwrap()
}

fn served(root: &Path) -> DocumentRoot {
    DocumentRoot::new(root).unwrap().with_accounts(|name| name == "ap31")
}

fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    nodes.iter().find_map(|n| if n.name == name { Some(n) } else { find(&n.children, name) })
}

/// The names of a list's entries, each with the names under it.
fn names(list: &OverseerNode) -> Vec<String> {
    list.children
        .iter()
        .filter(|entry| entry.children.iter().any(|c| c.name == "name"))
        .map(|entry| {
            let name = match entry.children.iter().find(|c| c.name == "name").and_then(|c| {
                c.parameters.get("_computed_value").or(c.parameters.get("value"))
            }) {
                Some(OverseerValue::String(s)) => s.clone(),
                other => panic!("a name is {:?}", other),
            };
            match entry.children.iter().find(|c| c.name == "Places") {
                Some(under) if !names(under).is_empty() => format!("{} [{}]", name, names(under).join(", ")),
                _ => name,
            }
        })
        .collect()
}

#[test]
fn a_menu_becomes_the_drawers_places_with_its_folders_as_entries_holding_their_documents() {
    let root = a_root("seed");
    write(&root, "ap31/menu.os", MENU);
    write(&root, "ap31/food.os", "tab food (label=\"Food log\") {\n}\n");
    let documents = served(&root);
    let answer = documents.as_asker(Asker::Person("ap31".into()), || {
        documents.command(None, "drawer_location", &json!({})).unwrap()
    });
    assert_eq!(answer, json!(drawer::FILE));

    let text = std::fs::read_to_string(root.join("ap31").join(drawer::FILE)).unwrap();
    assert!(text.starts_with("<drawer/drawer.os/drawer> drawer {"), "{}", text);
    assert!(text.contains("- path = \"projects/overseer.os\""), "{}", text);

    let nodes = documents.as_asker(Asker::Person("ap31".into()), || documents.open("drawer.os").unwrap());
    let places = find(&nodes, "Places").expect("the drawer inherits the base's list");
    assert_eq!(
        names(places),
        vec![
            "Tasks".to_string(),
            "Food log".to_string(),
            "Tasks (2)".to_string(),
            "Projects [overseer, Spores]".to_string(),
        ],
        "{}",
        text
    );
    // The menu is left as it was.
    assert_eq!(std::fs::read_to_string(root.join("ap31").join(menu::FILE)).unwrap(), MENU);
}

#[test]
fn a_drawer_already_made_is_left_as_it_is() {
    let root = a_root("again");
    let mine = "<drawer/drawer.os/drawer> drawer {\r\n}\r\n";
    write(&root, "ap31/drawer.os", mine);
    write(&root, "ap31/menu.os", MENU);
    let documents = served(&root);
    documents.as_asker(Asker::Person("ap31".into()), || {
        documents.command(None, "drawer_location", &json!({})).unwrap()
    });
    assert_eq!(std::fs::read_to_string(root.join("ap31/drawer.os")).unwrap(), mine);
}

#[test]
fn someone_with_no_menu_gets_a_drawer_that_only_inherits_the_base() {
    let root = a_root("bare");
    let documents = served(&root);
    documents.as_asker(Asker::Person("ap31".into()), || {
        documents.command(None, "drawer_location", &json!({})).unwrap()
    });
    assert_eq!(
        std::fs::read_to_string(root.join("ap31/drawer.os")).unwrap(),
        "<drawer/drawer.os/drawer> drawer {\r\n}\r\n"
    );
}

#[test]
fn the_server_without_accounts_keeps_its_drawer_at_its_root() {
    let root = a_root("token");
    let documents = DocumentRoot::new(&root).unwrap();
    assert_eq!(documents.command(None, "drawer_location", &json!({})).unwrap(), json!(drawer::FILE));
    assert!(root.join(drawer::FILE).exists());
}

#[test]
fn a_stranger_reads_the_base_and_writes_nothing_to_it() {
    let root = a_root("stranger");
    let documents = served(&root);
    for asker in [Asker::Stranger, Asker::Visitor] {
        documents.as_asker(asker.clone(), || {
            assert_eq!(
                documents.command(None, "drawer_location", &json!({})).unwrap(),
                json!(drawer::LIBRARY_NAME)
            );
            let nodes = documents.open(drawer::LIBRARY_NAME).expect("the base is read");
            assert!(find(&nodes, "Places").is_some());
            assert!(matches!(
                documents.resolve_for(drawer::LIBRARY_NAME, Need::Edit),
                Err(RequestError::ReadOnly(_))
            ));
        });
    }
    assert!(!root.join(drawer::FILE).exists(), "nothing was made for a stranger");
}

#[test]
fn no_request_writes_to_the_library_and_no_other_library_file_is_opened_by_name() {
    let root = a_root("library");
    let documents = served(&root);
    let base = drawer::base_file().expect("the base ships");
    documents.as_asker(Asker::Person("ap31".into()), || {
        assert!(overseer::access::may(&base, Need::Read));
        assert!(!overseer::access::may(&base, Need::Edit));
        assert!(documents.resolve("library:projects/project_template.os").is_err());
        assert!(documents.resolve("library:drawer/../projects/project_template.os").is_err());
    });
    documents.as_asker(Asker::Anyone, || assert!(!overseer::access::may(&base, Need::Edit)));
}
