//! The documents kept to hand: what the drawer lists, held in a document of its own.
//!
//! Documents live wherever they were made - some beside the code, some in the repository that is
//! deployed - and getting from one to another meant the file dialog every time on the desktop, and
//! a bookmark per document in a browser. The drawer lists the ones worth getting to quickly, and
//! the list is an ordinary document: backed up with the rest, readable by the bot, and renamed or
//! reordered by opening it like any other.
//!
//! One per place the page runs. The desktop app keeps its own in its settings folder, where an
//! entry is a file anywhere on this computer; the server keeps one in its documents folder, where
//! an entry is a document's name under that folder. The two cannot share entries, so they do not
//! share a list - they share its shape, and everything that reads and writes it.
//!
//! Written through the same instructions as every other change, so adding or removing a document
//! is a step that Undo takes back like any other.

use crate::app_api;
use crate::types::*;
use std::collections::HashMap;
use std::path::Path;

/// The document's name, on the server and in the settings folder alike.
pub const FILE: &str = "menu.os";

const EMPTY: &str = r#"// The documents kept to hand, in the order the drawer lists them.
// -
// Added from the drawer, which offers to keep whatever document is open, and taken off it the
// same way. Renamed and reordered here: `name` is what the drawer calls a document, and the order
// below is the order it shows them in. `path` is where the document is - a file on this computer
// for the desktop app, a document's name under the server's folder for the browser.

tab menu (label="Documents", mutable=true) {

    div (hidden=true) {
        div Place (layout="horizontal", margin=0, spacing=6, alignment="center") {
            string name (label="", width=35%) = ""
            string path (label="", font-size=12px, width=65%) = ""
        }
    }

    list Places (entry=<Place>, key="path", layout="vertical", spacing=1) {
    }
}
"#;

/// One document in the list.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Place {
    pub path: String,
    pub name: String,
}

/// The list's file, made the first time it is asked for.
fn ensure(file: &Path) -> Result<()> {
    if file.exists() {
        return Ok(());
    }
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)
            .map_err(|e| OverseerError::IoError(format!("could not make the menu's folder: {}", e)))?;
    }
    std::fs::write(file, EMPTY)
        .map_err(|e| OverseerError::IoError(format!("could not make the menu: {}", e)))
}

fn text_of(value: Option<&OverseerValue>) -> String {
    match value {
        Some(OverseerValue::String(s)) | Some(OverseerValue::Date(s)) | Some(OverseerValue::Timestamp(s)) => s.clone(),
        _ => String::new(),
    }
}

/// The entries of the list, as the document holds them.
fn entries(nodes: &[OverseerNode]) -> Vec<(String, Place)> {
    let Some(places) = nodes
        .iter()
        .find(|n| n.name == "menu")
        .and_then(|tab| tab.children.iter().find(|c| c.name == "Places"))
    else {
        return Vec::new();
    };
    places
        .children
        .iter()
        .map(|entry| {
            let field = |name: &str| {
                entry
                    .children
                    .iter()
                    .find(|c| c.name == name)
                    .map(|c| text_of(c.parameters.get("_computed_value").or(c.parameters.get("value"))))
                    .unwrap_or_default()
            };
            (entry.name.clone(), Place { path: field("path"), name: field("name") })
        })
        .filter(|(_, place)| !place.path.is_empty())
        .collect()
}

fn read(file: &Path) -> Result<Vec<OverseerNode>> {
    ensure(file)?;
    let text = std::fs::read_to_string(file)
        .map_err(|e| OverseerError::IoError(format!("could not read the menu: {}", e)))?;
    app_api::load_document(text)
}

/// What the list holds, in order.
pub fn places(file: &Path) -> Result<Vec<Place>> {
    Ok(entries(&read(file)?).into_iter().map(|(_, place)| place).collect())
}

/// Keep a document in the list - once, however often it is asked for, with the name given.
pub fn keep(file: &Path, path: &str, name: &str) -> Result<Vec<Place>> {
    if path.trim().is_empty() {
        return Err(OverseerError::ValidationError("no document to keep".into()));
    }
    ensure(file)?;
    let mut fields = HashMap::new();
    if !name.trim().is_empty() {
        fields.insert("name".to_string(), OverseerValue::String(name.trim().to_string()));
    }
    let wanted = app_api::EntryWanted {
        list_path: vec!["menu".into(), "Places".into()],
        key_field: "path".into(),
        key_value: OverseerValue::String(path.to_string()),
        template: "Place".into(),
        position: Some("append".into()),
        fields,
        then: None,
    };
    app_api::ensure_entry_at(&file.to_string_lossy(), FILE, wanted, "menu")?;
    places(file)
}

/// Take a document off the list. Nothing to do when it is not there.
pub fn drop(file: &Path, path: &str) -> Result<Vec<Place>> {
    let held = entries(&read(file)?);
    if let Some((entry, _)) = held.iter().find(|(_, place)| place.path == path) {
        app_api::remove_entry_at(
            &file.to_string_lossy(),
            FILE,
            vec!["menu".into(), "Places".into(), entry.clone()],
            "menu",
        )?;
    }
    places(file)
}
