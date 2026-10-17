//! The templates that ship with Overseer - its `library` folder - can be inherited from any
//! document, wherever it is: a file a reference names that is not beside the document is looked
//! for there. A file of the same name beside the document comes first, and the library is only
//! ever read.

use overseer::docmgr::manager::DocumentManager;
use overseer::file_ops::OverseerFileHandler;
use overseer::server::DocumentRoot;
use overseer::types::*;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static SERIAL: Mutex<()> = Mutex::new(());

const DERIVED: &str = r##"<projects/project_template.os/project> project {
    - name = "Elsewhere"
    - Items {
        - {
            - handle = "one"
            - title = "The first item"
        }
    }
}
"##;

const SHADOW: &str = r##"div (hidden=true) {
    tab project (label="Shadow", mutable=true) {
        string name = ""
        string origin = "beside"
        list Items (key="handle") {
        }
    }
}
"##;

fn folder(test: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "overseer_library_{}_{}",
        std::process::id(),
        test
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
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

fn text(value: &str) -> Option<OverseerValue> {
    Some(OverseerValue::String(value.to_string()))
}

fn shipped() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../library/projects/project_template.os")
}

#[test]
fn a_document_anywhere_inherits_the_shipped_project_template() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let before = std::fs::read(shipped()).unwrap();
    let root = folder("anywhere", &[("d.os", DERIVED)]);

    let nodes = worked_out(&root);
    let mut found = Vec::new();
    errors(&nodes, &mut found);
    assert!(found.is_empty(), "{:?}", found);
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), DERIVED);

    assert_eq!(read(&root, "project/name"), text("Elsewhere"));
    assert_eq!(read(&root, "project/Items/[one]/stage"), text("filed"));

    let service = DocumentRoot::new(&root).unwrap();
    service
        .set_at("d.os", "project/Items/[one]/stage", OverseerValue::String("ready".into()))
        .unwrap();
    let saved = std::fs::read_to_string(root.join("d.os")).unwrap();
    assert_eq!(
        saved,
        DERIVED.replace(
            "            - title = \"The first item\"\n",
            "            - title = \"The first item\"\n            - stage = \"ready\"\n",
        )
    );
    assert_eq!(std::fs::read(shipped()).unwrap(), before, "the library file was written");
}

#[test]
fn a_file_beside_the_document_comes_before_the_library() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("shadow", &[("d.os", DERIVED)]);
    assert_eq!(read(&root, "project/origin"), None);
    assert_eq!(read(&root, "project/name"), text("Elsewhere"));

    std::fs::create_dir_all(root.join("projects")).unwrap();
    std::fs::write(root.join("projects/project_template.os"), SHADOW).unwrap();
    // The document was held worked out from the library; the file put beside it is noticed.
    assert_eq!(read(&root, "project/origin"), text("beside"));
}

#[test]
fn the_library_can_be_put_anywhere() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let library = folder("elsewhere_library", &[("projects/project_template.os", SHADOW)]);
    let root = folder("elsewhere", &[("d.os", DERIVED)]);
    std::env::set_var("OVERSEER_LIBRARY", &library);
    let origin = read(&root, "project/origin");
    std::env::remove_var("OVERSEER_LIBRARY");
    assert_eq!(origin, text("beside"));
}

#[test]
fn a_path_out_of_the_folder_is_never_a_library_file() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let doc = r##"<../projects/project_template.os/project> project {
    - name = "N"
}
"##;
    let root = folder("climb", &[("d.os", doc)]);
    let mut found = Vec::new();
    errors(&worked_out(&root), &mut found);
    assert_eq!(found, vec!["project_template.os could not be read".to_string()]);
}
