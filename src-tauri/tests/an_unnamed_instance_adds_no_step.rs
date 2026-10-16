//! An instance written without a name, `<tags.os/tag_list> { ... }`, is a wrapper, as an unnamed
//! div is: it adds no step to any address, and what it holds is named as though it sat in its
//! parent. So a panel kept in a base of its own can be inherited by several templates while
//! `/project/Labels` stays where it was.

use overseer::docmgr::manager::DocumentManager;
use overseer::server::DocumentRoot;
use overseer::types::*;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static SERIAL: Mutex<()> = Mutex::new(());

const TAGS: &str = r##"div (hidden=true) {
    div Label {
        string tag = ""
        string name = ""
    }
    div tag_list {
        list Labels (entry=<Label>, key="tag") {
        }
        int count = $(Labels.count())
        div NewTag {
            textbox tag = ""
            textbox name = ""
            button add {
                on click {
                    if (cond=$(../tag == "")) {
                        refuse = "say a tag"
                    }
                    append (list="../../Labels", from="..")
                }
            }
        }
    }
}
"##;

const DOC: &str = r##"tab project (label="P", mutable=true) {
    string title = "Overseer"
    <lib/tags.os/tag_list> {
        - Labels {
            - {
                - tag = "ui"
                - name = "interface"
            }
        }
    }
    int labels = $(count)
}
"##;

fn folder(test: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "overseer_unnamed_instance_{}_{}",
        std::process::id(),
        test
    ));
    let _ = std::fs::remove_dir_all(&root);
    for (name, text) in files {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    root
}

fn read(root: &Path, address: &str) -> Option<OverseerValue> {
    let service = DocumentRoot::new(root).unwrap();
    let found = service.read_at("d.os", address).ok()?;
    let params = &found.node.parameters;
    params.get("_computed_value").or_else(|| params.get("value")).cloned()
}

fn text(value: &str) -> Option<OverseerValue> {
    Some(OverseerValue::String(value.to_string()))
}

fn saved(root: &Path) -> String {
    std::fs::read_to_string(root.join("d.os")).unwrap()
}

#[test]
fn its_list_is_at_the_parents_address() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("address", &[("lib/tags.os", TAGS), ("d.os", DOC)]);
    assert_eq!(read(&root, "project/Labels/[ui]/name"), text("interface"));
    assert_eq!(read(&root, "project/tag_list/Labels/[ui]/name"), None);
    // A formula beside the instance reads what it holds by its bare name.
    assert_eq!(read(&root, "project/count"), Some(OverseerValue::Integer(1)));
    assert_eq!(read(&root, "project/labels"), Some(OverseerValue::Integer(1)));
}

#[test]
fn an_action_inside_it_reaches_its_list_and_the_file_keeps_its_shape() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("action", &[("lib/tags.os", TAGS), ("d.os", DOC)]);
    let service = DocumentRoot::new(&root).unwrap();
    service
        .set_at("d.os", "project/NewTag/tag", OverseerValue::String("dsl".into()))
        .expect("the tag was not set");
    service
        .set_at("d.os", "project/NewTag/name", OverseerValue::String("language".into()))
        .expect("the name was not set");
    service.run_event("d.os", "project/NewTag/add", "click").expect("the press was refused");
    assert_eq!(read(&root, "project/Labels/[dsl]/name"), text("language"));
    assert_eq!(read(&root, "project/count"), Some(OverseerValue::Integer(2)));

    let file = saved(&root);
    assert!(file.contains("    <lib/tags.os/tag_list> {\n"), "{}", file);
    assert!(!file.contains("tag_list> tag_list"), "{}", file);
    assert!(
        file.contains(
            "            - {\n                - tag = \"dsl\"\n                - name = \"language\"\n            }\n"
        ),
        "{}",
        file
    );
}

#[test]
fn an_edit_inside_it_saves_only_the_edit() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("edit", &[("lib/tags.os", TAGS), ("d.os", DOC)]);
    let service = DocumentRoot::new(&root).unwrap();
    service
        .set_at("d.os", "project/Labels/[ui]/name", OverseerValue::String("page".into()))
        .expect("the edit was refused");
    assert_eq!(saved(&root), DOC.replace("\"interface\"", "\"page\""));
}

#[test]
fn it_saves_as_it_was_written() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("roundtrip", &[("lib/tags.os", TAGS), ("d.os", DOC)]);
    let nodes = DocumentManager::with_document(Some(root.clone()), || {
        let (_rest, mut nodes) = overseer::parser::parse_document(DOC).unwrap();
        overseer::resolver::resolve_document(&mut nodes);
        nodes
    });
    let written = overseer::file_ops::OverseerFileHandler::serialize_nodes(&nodes).expect("serializes");
    assert_eq!(written, DOC);
}

#[test]
fn two_of_them_holding_one_name_are_told_apart_as_two_divs_are() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let base = r##"div (hidden=true) {
    div Note {
        string note = ""
    }
}
"##;
    let doc = r##"tab t (label="T", mutable=true) {
    <base.os/Note> {
        - note = "first"
    }
    <base.os/Note> {
        - note = "second"
    }
}
"##;
    let root = folder("twice", &[("base.os", base), ("d.os", doc)]);
    assert_eq!(read(&root, "t/note"), text("first"));
    assert_eq!(read(&root, "t/note#1"), text("second"));
}
