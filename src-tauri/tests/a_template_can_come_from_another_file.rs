//! A template reference can name a node of another file: `<base.os/project>`.
//!
//! The base is read as written and its node copied in as any template's is, so its formulas run
//! on the deriving document's data. What the base's own references mean is settled in the base:
//! a bare one that names a node of the base names that node, and a file is looked for next to the
//! base. A template that holds an instance of itself is refused rather than copied without end.

use overseer::docmgr::manager::DocumentManager;
use overseer::file_ops::OverseerFileHandler;
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
        string owner = ""
        int open = $(Items.filter(|x| x/stage == "filed").count())
        list Items (entry=<Task>, key="title") {
        }
    }
    tab tab1 (label="One") {
        string note = ""
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

/// A folder of its own for one test, holding these files.
fn folder(test: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "overseer_crossfile_{}_{}",
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
    let found = service
        .read_at("d.os", address)
        .unwrap_or_else(|e| panic!("nothing read at {}: {:?}", address, e));
    let params = &found.node.parameters;
    params.get("_computed_value").or_else(|| params.get("value")).cloned()
}

fn text(value: &str) -> Option<OverseerValue> {
    Some(OverseerValue::String(value.to_string()))
}

/// The document as the resolver works it out, read from the folder it is in.
fn worked_out(root: &Path) -> Vec<OverseerNode> {
    let content = std::fs::read_to_string(root.join("d.os")).unwrap();
    DocumentManager::with_document(Some(root.to_path_buf()), || {
        let (_rest, mut nodes) = overseer::parser::parse_document(&content).unwrap();
        overseer::resolver::resolve_document(&mut nodes);
        nodes
    })
}

fn errors(nodes: &[OverseerNode], found: &mut Vec<String>) {
    for node in nodes {
        if let Some(OverseerValue::String(why)) = node.parameters.get(overseer::bases::ERROR) {
            found.push(why.clone());
        }
        errors(&node.children, found);
    }
}

#[test]
fn an_instance_takes_its_fields_and_formulas_from_the_base() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("instance", &[("base.os", BASE), ("d.os", DERIVED)]);
    assert_eq!(read(&root, "t/project/name"), text("Overseer"));
    assert_eq!(read(&root, "t/project/owner"), text(""));
    // The base's own `<Task>` is the base's Task, and its default reaches the entry.
    assert_eq!(read(&root, "t/project/Items/[a]/stage"), text("filed"));
    assert_eq!(read(&root, "t/project/Items/[b]/stage"), text("ready"));
    // The base's formula counts the deriving document's entries.
    assert_eq!(read(&root, "t/project/open"), Some(OverseerValue::Integer(1)));

    // An entry added to the base's list is written as an entry, and nothing of where the base's
    // own `<Task>` was settled to comes into the file.
    let service = DocumentRoot::new(&root).unwrap();
    let mut fields = std::collections::HashMap::new();
    fields.insert("title".to_string(), OverseerValue::String("c".into()));
    service.append_at("d.os", "t/project/Items", &fields).expect("the append was refused");
    let saved = std::fs::read_to_string(root.join("d.os")).unwrap();
    assert_eq!(
        saved,
        DERIVED.replace(
            "                - stage = \"ready\"
            }
",
            "                - stage = \"ready\"
            }
            - {
                - title = \"c\"
            }
",
        )
    );
    assert_eq!(read(&root, "t/project/Items/[c]/stage"), text("filed"));
    assert_eq!(read(&root, "t/project/open"), Some(OverseerValue::Integer(2)));
}

#[test]
fn a_list_and_an_action_make_entries_from_the_base() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let doc = r##"tab t (label="T", mutable=true) {
    list Tasks (entry=<base.os/Task>, key="title") {
        - {
            - title = "x"
        }
    }
    button add (label="a") {
        on click {
            append (list="/t/Tasks", template=<base.os/Task>) {
                - title = "y"
            }
        }
    }
}
"##;
    let root = folder("list", &[("base.os", BASE), ("d.os", doc)]);
    assert_eq!(read(&root, "t/Tasks/[x]/stage"), text("filed"));

    overseer::app_api::forget_baseline();
    let service = DocumentRoot::new(&root).unwrap();
    service.run_event("d.os", "t/add", "click").expect("the press was refused");
    assert_eq!(read(&root, "t/Tasks/[y]/stage"), text("filed"));

    // The server checks the fields it is given against the base's template.
    let mut fields = std::collections::HashMap::new();
    fields.insert("title".to_string(), OverseerValue::String("z".into()));
    fields.insert("stage".to_string(), OverseerValue::String("ready".into()));
    service.append_at("d.os", "t/Tasks", &fields).expect("the append was refused");
    assert_eq!(read(&root, "t/Tasks/[z]/stage"), text("ready"));
    let saved = std::fs::read_to_string(root.join("d.os")).unwrap();
    assert_eq!(saved.matches("<base.os/Task>").count(), 2, "{}", saved);
    assert!(!saved.contains("overseer_crossfile"), "{}", saved);
    let mut unknown = std::collections::HashMap::new();
    unknown.insert("title".to_string(), OverseerValue::String("w".into()));
    unknown.insert("colour".to_string(), OverseerValue::String("red".into()));
    assert!(service.append_at("d.os", "t/Tasks", &unknown).is_err());
}

#[test]
fn unnamed_instances_are_tabs_and_named_ones_keep_apart() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let doc = r##"tab t (label="T", mutable=true) {
    <base.os/tab1>
    <base.os/tab1>
    <base.os/tab1>
    div d {
        <base.os/tab1> a {
            - note = "A"
        }
        <base.os/tab1> b
    }
}
"##;
    let root = folder("tabs", &[("base.os", BASE), ("d.os", doc)]);
    let nodes = worked_out(&root);
    let t = &nodes[0];
    let tabs = t
        .children
        .iter()
        .filter(|c| {
            matches!(c.parameters.get("_original_type"), Some(OverseerValue::String(k)) if k == "tab")
        })
        .count();
    assert_eq!(tabs, 3);
    assert_eq!(read(&root, "t/d/a/note"), text("A"));
    assert_eq!(read(&root, "t/d/b/note"), text(""));
}

#[test]
fn a_base_finds_its_own_files_beside_it() {
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
        string label = "from the third file"
    }
}
"##;
    let doc = r##"tab t (label="T", mutable=true) {
    <lib/base.os/Card> c {
        - name = "N"
    }
}
"##;
    let root = folder(
        "chain",
        &[("lib/base.os", base), ("lib/third.os", third), ("d.os", doc)],
    );
    assert_eq!(read(&root, "t/c/name"), text("N"));
    assert_eq!(read(&root, "t/c/badge/label"), text("from the third file"));
}

#[test]
fn a_template_holding_itself_is_refused() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let a = r##"div (hidden=true) {
    div X {
        string x = "x"
        <b.os/Y> y
    }
}
"##;
    let b = r##"div (hidden=true) {
    div Y {
        string y = "y"
        <a.os/X> x
    }
}
"##;
    let doc = r##"tab t (label="T", mutable=true) {
    <a.os/X> top
    div (hidden=true) {
        div Local {
            string z = ""
            <Local> again
        }
    }
    <Local> here
}
"##;
    let root = folder("cycle", &[("a.os", a), ("b.os", b), ("d.os", doc)]);
    let nodes = worked_out(&root);
    let mut found = Vec::new();
    errors(&nodes, &mut found);
    assert!(
        found
            .iter()
            .any(|why| why == "'X' holds an instance of itself" || why == "'Y' holds an instance of itself"),
        "{:?}",
        found
    );
    assert!(
        found.iter().any(|why| why.contains("'Local' holds an instance of itself")),
        "{:?}",
        found
    );
    assert_eq!(read(&root, "t/top/x"), text("x"));
    assert_eq!(read(&root, "t/top/y/y"), text("y"));
}

#[test]
fn a_qualified_name_and_a_bare_one_find_different_nodes() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let base = r##"div (hidden=true) {
    div Thing {
        string from = "base"
    }
}
"##;
    let doc = r##"tab t (label="T", mutable=true) {
    div (hidden=true) {
        div Thing {
            string from = "here"
        }
    }
    <base.os/Thing> a
    <Thing> b
}
"##;
    let root = folder("names", &[("base.os", base), ("d.os", doc)]);
    assert_eq!(read(&root, "t/a/from"), text("base"));
    assert_eq!(read(&root, "t/b/from"), text("here"));
}

#[test]
fn a_missing_base_says_so_and_is_kept_as_written() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let doc = r##"tab t (label="T", mutable=true) {
    <nowhere.os/X> x {
        - name = "N"
    }
    <base.os/NotThere> y
}
"##;
    let root = folder("missing", &[("base.os", BASE), ("d.os", doc)]);
    let nodes = worked_out(&root);
    let mut found = Vec::new();
    errors(&nodes, &mut found);
    assert!(found.iter().any(|why| why == "nowhere.os could not be read"), "{:?}", found);
    assert!(found.iter().any(|why| why == "no 'NotThere' in base.os"), "{:?}", found);
    // No folder in what is shown.
    assert!(found.iter().all(|why| !why.contains('/') && !why.contains('\\')), "{:?}", found);
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), doc);
}

#[test]
fn a_changed_base_is_read_again() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("changed", &[("base.os", BASE), ("d.os", DERIVED)]);
    let stage_of_a = |nodes: &[OverseerNode]| {
        overseer::addressing::find(nodes, "t/project/Items/[a]/stage")
            .and_then(|n| n.parameters.get("value").cloned())
    };
    assert_eq!(stage_of_a(&worked_out(&root)), text("filed"));
    std::fs::write(
        root.join("base.os"),
        BASE.replace(r#"string stage = "filed""#, r#"string stage = "planning""#),
    )
    .unwrap();
    assert_eq!(stage_of_a(&worked_out(&root)), text("planning"));
}

#[test]
fn a_derived_document_saves_only_its_edits() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("untouched", &[("base.os", BASE), ("d.os", DERIVED)]);
    assert_eq!(OverseerFileHandler::serialize_nodes(&worked_out(&root)).unwrap(), DERIVED);

    let cases = [
        (
            "t/project/name",
            "X",
            DERIVED.replace(r#"- name = "Overseer""#, r#"- name = "X""#),
        ),
        (
            "t/project/owner",
            "me",
            DERIVED.replace(
                "        - name = \"Overseer\"\n",
                "        - name = \"Overseer\"\n        - owner = \"me\"\n",
            ),
        ),
        (
            "t/project/Items/[a]/stage",
            "ready",
            DERIVED.replace(
                "                - title = \"a\"\n",
                "                - title = \"a\"\n                - stage = \"ready\"\n",
            ),
        ),
    ];
    for (address, value, expected) in cases {
        let root = folder(
            &format!("edit_{}", address.replace(['/', '[', ']'], "_")),
            &[("base.os", BASE), ("d.os", DERIVED)],
        );
        let service = DocumentRoot::new(&root).unwrap();
        service
            .set_at("d.os", address, OverseerValue::String(value.to_string()))
            .unwrap_or_else(|e| panic!("{} was refused: {:?}", address, e));
        let saved = std::fs::read_to_string(root.join("d.os")).unwrap();
        assert_eq!(saved, expected, "after setting {}", address);
    }
}

