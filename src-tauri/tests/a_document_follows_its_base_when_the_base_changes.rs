//! A document made from another file's template is held worked out under its own text, which an
//! edit to the base does not change. It is worked out again once a base it was made from - or a
//! file that base is made from - has changed, or once a base it names and could not find is there.

use overseer::server::DocumentRoot;
use overseer::types::*;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static SERIAL: Mutex<()> = Mutex::new(());

const BASE: &str = r##"div (hidden=true) {
    div Task {
        string title = ""
        string stage = "filed"
    }
    div project {
        string name = ""
        int open = $(Items.filter(|x| x/stage == "filed").count())
        list Items (entry=<Task>, key="title") {
        }
    }
}
"##;

const DERIVED: &str = r##"tab t (label="T", mutable=true) {
    <base.os/project> project {
        - name = "Overseer"
        - Items {
            - {
                - title = "a"
            }
            - {
                - title = "b"
                - stage = "ready"
            }
        }
    }
}
"##;

fn folder(test: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "overseer_basereload_{}_{}",
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

/// Read twice, so that what is read after the change could have come from what was held.
fn read_held(root: &Path, address: &str) -> Option<OverseerValue> {
    let first = read(root, address);
    assert_eq!(read(root, address), first);
    first
}

fn text(value: &str) -> Option<OverseerValue> {
    Some(OverseerValue::String(value.to_string()))
}

#[test]
fn an_edited_base_reaches_the_document_made_from_it() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("edited", &[("base.os", BASE), ("d.os", DERIVED)]);
    assert_eq!(read_held(&root, "t/project/Items/[a]/stage"), text("filed"));
    assert_eq!(read_held(&root, "t/project/open"), Some(OverseerValue::Integer(1)));

    std::fs::write(
        root.join("base.os"),
        BASE.replace(r#"string stage = "filed""#, r#"string stage = "planning""#)
            .replace(r#"x/stage == "filed""#, r#"x/stage != "planning""#),
    )
    .unwrap();
    assert_eq!(read(&root, "t/project/Items/[a]/stage"), text("planning"));
    // Under the old formula nothing is filed now; the new one counts what is not planning.
    assert_eq!(read(&root, "t/project/open"), Some(OverseerValue::Integer(1)));

    std::fs::write(
        root.join("base.os"),
        BASE.replace(
            "        string name = \"\"\n",
            "        string name = \"\"\n        string owner = \"nobody yet\"\n",
        ),
    )
    .unwrap();
    assert_eq!(read(&root, "t/project/owner"), text("nobody yet"));
    assert_eq!(read(&root, "t/project/Items/[a]/stage"), text("filed"));
}

#[test]
fn a_list_follows_the_base_its_entries_come_from() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let doc = r##"tab t (label="T", mutable=true) {
    list Tasks (entry=<base.os/Task>, key="title") {
        - {
            - title = "x"
        }
    }
    int count = $(Tasks.count())
}
"##;
    let root = folder("list", &[("base.os", BASE), ("d.os", doc)]);
    assert_eq!(read_held(&root, "t/Tasks/[x]/stage"), text("filed"));
    std::fs::write(
        root.join("base.os"),
        BASE.replace(r#"string stage = "filed""#, r#"string stage = "someday""#),
    )
    .unwrap();
    assert_eq!(read(&root, "t/Tasks/[x]/stage"), text("someday"));
}

#[test]
fn a_file_the_base_is_made_from_reaches_the_document_too() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let base = r##"div (hidden=true) {
    div Card {
        string name = ""
        <third.os/Badge> badge
    }
}
"##;
    let third = r##"div (hidden=true) {
    div Badge {
        string label = "first"
    }
}
"##;
    let doc = r##"tab t (label="T", mutable=true) {
    <lib/base.os/Card> c {
        - name = "N"
    }
    string shown = $(c/name)
}
"##;
    let root = folder(
        "third",
        &[("lib/base.os", base), ("lib/third.os", third), ("d.os", doc)],
    );
    assert_eq!(read_held(&root, "t/c/badge/label"), text("first"));
    std::fs::write(root.join("lib/third.os"), third.replace("first", "second one")).unwrap();
    assert_eq!(read(&root, "t/c/badge/label"), text("second one"));
}

#[test]
fn a_base_that_was_missing_is_read_once_it_is_there() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // A formula of the document's own, so that it is held worked out with no base to give it any.
    let doc = DERIVED.replace("
}
", "
    int one = $(1 + 0)
}
");
    let root = folder("missing", &[("d.os", &doc)]);
    assert_eq!(read_held(&root, "t/project/Items/[a]/stage"), None);
    std::fs::write(root.join("base.os"), BASE).unwrap();
    assert_eq!(read(&root, "t/project/Items/[a]/stage"), text("filed"));
}
