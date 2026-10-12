//! A document can name an SVG beside it as an icon, `<icon=icons/carrot.svg>`, and the page asks
//! for its text with `load_overseer_icon`. Both doors answer from the document's own folder and
//! below, and from nowhere else: the path comes from a document, and on the server through the
//! network, so what leaves the folder, or is not an SVG, is never read.

use overseer::app_api::{icon_beside, IconRefused, ICON_MOST_BYTES};
use overseer::server::{DocumentRoot, RequestError};
use serde_json::json;
use std::path::{Path, PathBuf};

const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9"/></svg>"#;

/// A root with `kitchen/pantry.os`, its icons beside it, and an SVG and a document outside the
/// kitchen it may not reach.
fn a_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_docicons_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("kitchen/icons")).expect("make the root");
    std::fs::write(root.join("kitchen/pantry.os"), "tab pantry {\n    string jar (label=\"<icon=icons/jar.svg>\") = \"\"\n}\n").expect("write");
    std::fs::write(root.join("kitchen/icons/jar.svg"), SVG).expect("write");
    std::fs::write(root.join("kitchen/icons/Carrot.SVG"), SVG).expect("write");
    std::fs::write(root.join("kitchen/icons/notes.txt"), "not an icon").expect("write");
    std::fs::write(root.join("kitchen/icons/huge.svg"), "x".repeat(ICON_MOST_BYTES as usize + 1)).expect("write");
    std::fs::write(root.join("upstairs.svg"), SVG).expect("write");
    root
}

fn ask(service: &DocumentRoot, icon: &str) -> Result<serde_json::Value, RequestError> {
    service.command_for(
        "alice",
        Some("kitchen/pantry.os"),
        "load_overseer_icon",
        &json!({ "path": "kitchen/pantry.os", "icon": icon }),
    )
}

#[test]
fn the_server_hands_out_an_icon_beside_the_document() {
    let root = a_root("served");
    let service = DocumentRoot::new(&root).expect("open the root");
    assert_eq!(ask(&service, "icons/jar.svg"), Ok(json!(SVG)));
    assert_eq!(ask(&service, "icons/Carrot.SVG"), Ok(json!(SVG)));
}

#[test]
fn the_server_reads_nothing_outside_the_documents_folder() {
    let root = a_root("refused");
    let service = DocumentRoot::new(&root).expect("open the root");
    let outside = root.join("upstairs.svg").to_string_lossy().replace('\\', "/");
    for icon in ["../upstairs.svg", "icons/../../upstairs.svg", "/upstairs.svg", "C:/upstairs.svg", outside.as_str(), "./icons/jar.svg", ""] {
        assert!(
            matches!(ask(&service, icon), Err(RequestError::Rejected(_))),
            "'{}' was not refused: {:?}",
            icon,
            ask(&service, icon)
        );
    }
    for icon in ["icons/notes.txt", "icons/huge.svg", "pantry.os"] {
        assert!(matches!(ask(&service, icon), Err(RequestError::Rejected(_))), "'{}' was not refused", icon);
    }
    assert!(matches!(ask(&service, "icons/none.svg"), Err(RequestError::NotFound(_))));
}

#[test]
fn the_server_asks_for_a_document_it_serves() {
    let root = a_root("document");
    let service = DocumentRoot::new(&root).expect("open the root");
    let asked = service.command_for(
        "alice",
        None,
        "load_overseer_icon",
        &json!({ "path": "../pantry.os", "icon": "icons/jar.svg" }),
    );
    assert!(matches!(asked, Err(RequestError::Rejected(_))));
}

#[test]
fn the_desktop_app_reads_beside_the_open_file_as_the_server_does() {
    let root = a_root("desktop");
    let document = root.join("kitchen/pantry.os");
    assert_eq!(icon_beside(&document, "icons/jar.svg"), Ok(SVG.to_string()));
    for icon in ["../upstairs.svg", "/upstairs.svg", "icons/notes.txt", "icons/huge.svg"] {
        assert!(matches!(icon_beside(&document, icon), Err(IconRefused::NotAName(_))), "'{}' was not refused", icon);
    }
    let absolute = root.join("upstairs.svg");
    assert!(icon_beside(&document, absolute.to_str().unwrap()).is_err());
    assert!(matches!(icon_beside(&document, "icons/none.svg"), Err(IconRefused::Missing(_))));
}

#[test]
fn a_link_out_of_the_folder_is_not_followed() {
    let root = a_root("link");
    let link = root.join("kitchen/icons/away.svg");
    let target = root.join("upstairs.svg");
    #[cfg(windows)]
    let made = std::os::windows::fs::symlink_file(&target, &link);
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(&target, &link);
    if made.is_err() {
        // Making a link needs a privilege Windows does not give everyone; nothing to check then.
        return;
    }
    assert!(matches!(icon_beside(Path::new(&root.join("kitchen/pantry.os")), "icons/away.svg"), Err(IconRefused::Missing(_))));
}
