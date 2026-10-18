//! A template can itself be an instance of another, and what is made from it gets everything both
//! give.
//!
//! `<Thing> Movie { ... }` is a template made from `Thing`: an entry of a list with
//! `entry=<Movie>`, or an instance `<Movie> m`, has Thing's fields and buttons with Movie's
//! changes on them. Each pass of the resolver once worked from a copy of the document taken before
//! the pass, where Movie was still only what it wrote, so what was made from it got Movie's
//! overrides alone - a button whose label Movie changed came without its click, and a button Movie
//! added found none of the fields Thing gave the entry.

use overseer::app_api;
use overseer::file_ops::OverseerFileHandler;
use overseer::server::DocumentRoot;
use overseer::types::{OverseerNode, OverseerValue};

const DOCUMENT: &str = r#"tab t (mutable=true) {

    div (hidden=true) {
        div Thing {
            string name = ""
            string kind = "film"
            string note = ""
            button mark (label="Mark") {
                on click {
                    set (path="../note") = "marked"
                }
            }
            button drop (label="Drop") {
                on click {
                    remove (from="/t/All", keyField="name", keyValue=$(../name))
                }
            }
        }

        <Thing> Movie {
            - kind = "movie"
            - mark (label="Mark it")
            +button watched (label="Watched") {
                on click {
                    move (from="/t/All", keyField="kind", keyValue=$(../kind), to="/t/Done")
                }
            }
        }
    }

    list All (entry=<Movie>, key="name") {
        - {
            - name = "a"
            - note = "kept"
        }
        - {
            - name = "b"
            - kind = "series"
        }
    }

    list Done (entry=<Movie>, key="name") { }

    <Movie> single {
        - name = "s"
    }
}
"#;

fn served(tag: &str, files: &[(&str, &str)]) -> (std::path::PathBuf, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_derived_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for (name, text) in files {
        std::fs::write(root.join(name), text).unwrap();
    }
    overseer::viewstate::forget_all();
    let service = DocumentRoot::new(&root).unwrap();
    (root, service)
}

fn node(service: &DocumentRoot, address: &str) -> OverseerNode {
    service
        .read_at("d.os", address)
        .unwrap_or_else(|e| panic!("nothing at {}: {:?}", address, e))
        .node
}

fn value(service: &DocumentRoot, address: &str) -> Option<OverseerValue> {
    node(service, address).parameters.get("value").cloned()
}

fn text(s: &str) -> Option<OverseerValue> {
    Some(OverseerValue::String(s.to_string()))
}

#[test]
fn an_entry_has_what_both_templates_give() {
    let (_root, service) = served("both", &[("d.os", DOCUMENT)]);
    for entry in ["t/All/[a]", "t/single"] {
        assert_eq!(value(&service, &format!("{}/kind", entry)), text("movie"), "{}", entry);
        let mark = node(&service, &format!("{}/mark", entry));
        assert_eq!(mark.node_type, "button", "{}", entry);
        assert_eq!(mark.parameters.get("label"), text("Mark it").as_ref(), "{}", entry);
        assert_eq!(node(&service, &format!("{}/drop", entry)).node_type, "button", "{}", entry);
        assert_eq!(node(&service, &format!("{}/watched", entry)).node_type, "button", "{}", entry);
    }
}

#[test]
fn a_button_the_derived_template_changed_keeps_its_click() {
    let (_root, service) = served("changed", &[("d.os", DOCUMENT)]);
    service.run_event("d.os", "t/All/[b]/mark", "click").expect("the press");
    assert_eq!(value(&service, "t/All/[b]/note"), text("marked"));
    service.run_event("d.os", "t/single/mark", "click").expect("the press");
    assert_eq!(value(&service, "t/single/note"), text("marked"));
}

#[test]
fn a_button_the_derived_template_added_moves_its_entry_with_all_it_holds() {
    let (root, service) = served("added", &[("d.os", DOCUMENT)]);
    service.run_event("d.os", "t/All/[a]/watched", "click").expect("the press");
    assert_eq!(value(&service, "t/Done/[a]/note"), text("kept"));
    assert_eq!(value(&service, "t/Done/[a]/kind"), text("movie"));
    assert!(service.read_at("d.os", "t/All/[a]").is_err(), "the entry stayed where it was");
    let saved = std::fs::read_to_string(root.join("d.os")).unwrap();
    let done = &saved[saved.find("list Done").unwrap()..];
    assert!(done.contains("- name = \"a\"") && done.contains("- note = \"kept\""), "{}", saved);
    assert!(!done.contains("kind"), "a field the templates give was written in: {}", saved);
}

#[test]
fn an_inherited_button_works_on_an_entry() {
    let (_root, service) = served("inherited", &[("d.os", DOCUMENT)]);
    service.run_event("d.os", "t/All/[b]/drop", "click").expect("the press");
    assert!(service.read_at("d.os", "t/All/[b]").is_err(), "the entry was not removed");
}

#[test]
fn saved_untouched_it_is_the_same_bytes() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).unwrap();
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), DOCUMENT);
}

#[test]
fn a_press_writes_only_what_it_changed() {
    let (root, service) = served("only", &[("d.os", DOCUMENT)]);
    service.run_event("d.os", "t/All/[b]/mark", "click").expect("the press");
    let expected = DOCUMENT.replacen(
        "            - kind = \"series\"\n",
        "            - kind = \"series\"\n            - note = \"marked\"\n",
        1,
    );
    assert_eq!(std::fs::read_to_string(root.join("d.os")).unwrap(), expected);
}

const BASE: &str = r#"tab base {
    div Thing {
        string name = ""
        string note = ""
        button mark (label="Mark") {
            on click {
                set (path="../note") = "marked"
            }
        }
    }
}
"#;

const FROM_A_BASE: &str = r#"tab t (mutable=true) {

    div (hidden=true) {
        <base.os/Thing> Movie {
            - mark (label="Mark it")
        }
    }

    list All (entry=<Movie>, key="name") {
        - {
            - name = "a"
        }
    }
}
"#;

#[test]
fn a_template_made_from_another_files_works_the_same() {
    let (_root, service) = served("base", &[("base.os", BASE), ("d.os", FROM_A_BASE)]);
    let mark = node(&service, "t/All/[a]/mark");
    assert_eq!(mark.parameters.get("label"), text("Mark it").as_ref());
    service.run_event("d.os", "t/All/[a]/mark", "click").expect("the press");
    assert_eq!(value(&service, "t/All/[a]/note"), text("marked"));
}

#[test]
fn two_templates_made_from_each_other_still_resolve() {
    let document = r#"tab t {
    div (hidden=true) {
        <B> A {
            - x = "a"
        }
        <A> B {
            - x = "b"
        }
    }
    list L (entry=<A>) {
        - {
            - x = "1"
        }
    }
}
"#;
    let nodes = app_api::load_document(document.to_string()).unwrap();
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), document);
}
