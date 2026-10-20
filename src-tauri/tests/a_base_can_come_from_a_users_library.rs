//! A user keeps library folders of their own, which any document can inherit from as it can from
//! the one that ships with Overseer: `libraries.txt`, beside the settings, names them a line each.
//! A base is looked for beside the document, then in those folders in their order, then in the
//! shipped library - so a program such as Brocessor ships its bases in its own folder and names
//! it there, and nothing is copied into Overseer's.

use overseer::server::DocumentRoot;
use overseer::types::*;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static SERIAL: Mutex<()> = Mutex::new(());

const DERIVED: &str = r##"<brocessor/v200.os/patch> patch {
    - name = "Lead"
}
"##;

fn base(origin: &str) -> String {
    format!(
        r##"div (hidden=true) {{
    div patch {{
        string name = ""
        string origin = "{}"
    }}
}}
"##,
        origin
    )
}

fn folder(test: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "overseer_user_library_{}_{}",
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

fn problem(root: &Path, address: &str) -> Option<OverseerValue> {
    let service = DocumentRoot::new(root).unwrap();
    let found = service.read_at("d.os", address).ok()?;
    found.node.parameters.get(overseer::bases::ERROR).cloned()
}

fn text(value: &str) -> Option<OverseerValue> {
    Some(OverseerValue::String(value.to_string()))
}

/// `libraries.txt` naming these folders, a line each.
fn listing(folders: &[&Path]) -> String {
    folders
        .iter()
        .map(|f| f.to_string_lossy().replace('\\', "/"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_base_is_found_in_a_folder_the_user_names() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let brocessor = folder("app", &[("brocessor/v200.os", &base("app"))]);
    let list = format!("# Brocessor\n\n{}\n", listing(&[&brocessor]));
    let root = folder("named", &[("d.os", DERIVED), ("libraries.txt", &list)]);
    assert_eq!(read(&root, "patch/origin"), text("app"));
    assert_eq!(read(&root, "patch/name"), text("Lead"));
    assert_eq!(problem(&root, "patch"), None);
}

#[test]
fn a_folder_named_by_a_relative_path_is_beside_the_list() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder(
        "relative",
        &[
            ("d.os", DERIVED),
            ("libraries.txt", "mine\n"),
            ("mine/brocessor/v200.os", &base("mine")),
        ],
    );
    assert_eq!(read(&root, "patch/origin"), text("mine"));
}

#[test]
fn the_document_folder_comes_first_and_then_the_libraries_in_their_order() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let first = folder("first", &[("brocessor/v200.os", &base("first"))]);
    let second = folder("second", &[("brocessor/v200.os", &base("second"))]);
    let list = listing(&[&first, &second]);
    let root = folder("order", &[("d.os", DERIVED), ("libraries.txt", &list)]);
    assert_eq!(read(&root, "patch/origin"), text("first"));

    std::fs::create_dir_all(root.join("brocessor")).unwrap();
    std::fs::write(root.join("brocessor/v200.os"), base("beside")).unwrap();
    assert_eq!(read(&root, "patch/origin"), text("beside"));
}

#[test]
fn a_users_library_comes_before_the_shipped_one() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let shadow = r##"div (hidden=true) {
    tab project (label="Shadow", mutable=true) {
        string origin = "user"
        list Items (key="handle") {
        }
    }
}
"##;
    let mine = folder("over_shipped", &[("projects/project_template.os", shadow)]);
    let doc = "<projects/project_template.os/project> project {\n}\n";
    let root = folder("shipped", &[("d.os", doc), ("libraries.txt", &listing(&[&mine]))]);
    assert_eq!(read(&root, "project/origin"), text("user"));
}

#[test]
fn a_document_follows_the_list_of_libraries() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let brocessor = folder("later", &[("brocessor/v200.os", &base("app"))]);
    let root = folder("follows", &[("d.os", DERIVED)]);
    assert_eq!(read(&root, "patch/origin"), None);
    assert_eq!(problem(&root, "patch"), text("v200.os could not be read"));

    // The document was held worked out without the base; a folder named since is noticed.
    std::fs::write(root.join("libraries.txt"), listing(&[&brocessor])).unwrap();
    assert_eq!(read(&root, "patch/origin"), text("app"));

    // And so is a library looked in before the one the base came from, once it has the base.
    let earlier = folder("earlier", &[]);
    std::fs::write(root.join("libraries.txt"), listing(&[&earlier, &brocessor])).unwrap();
    assert_eq!(read(&root, "patch/origin"), text("app"));
    std::fs::create_dir_all(earlier.join("brocessor")).unwrap();
    std::fs::write(earlier.join("brocessor/v200.os"), base("earlier")).unwrap();
    assert_eq!(read(&root, "patch/origin"), text("earlier"));
}

#[test]
fn a_path_out_of_the_folder_is_never_a_users_library_file() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let brocessor = folder("climbed", &[("brocessor/v200.os", &base("app"))]);
    let doc = "<../brocessor/v200.os/patch> patch {\n}\n";
    let root = folder("climb", &[("d.os", doc), ("libraries.txt", &listing(&[&brocessor]))]);
    assert_eq!(read(&root, "patch/origin"), None);
    assert_eq!(problem(&root, "patch"), text("v200.os could not be read"));
}

#[test]
fn a_users_library_offers_its_themes() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let theme = "context theme (base=<themes/default.os/theme>) {\n    - accent = \"#123456\"\n}\n";
    let mine = folder("themed", &[("themes/ink.os", theme)]);
    let root = folder("themes", &[("libraries.txt", &listing(&[&mine]))]);
    let themes = overseer::environment::with_file(
        root.join(overseer::environment::FILE),
        overseer::environment::themes,
    );
    assert!(themes.contains(&"ink".to_string()), "{:?}", themes);
    assert!(themes.contains(&"default".to_string()), "{:?}", themes);
}
