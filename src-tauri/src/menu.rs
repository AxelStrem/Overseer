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
// same way. Renamed and reordered here: the drawer calls a document by its `name` when it has one,
// by the label of the tab it is wrapped in when it has none, and by its file's name otherwise; the
// order below is the order it shows them in. `folder` gathers documents under one heading in the drawer;
// those with none come first. `path` is where the document is - a file on this computer for the
// desktop app, a document's name under the server's folder for the browser.

tab menu (label="Documents", mutable=true) {

    div (hidden=true) {
        div Place (layout="horizontal", margin=0, spacing=6, alignment="center") {
            string name (label="", width=30%) = ""
            string folder (label="", width=20%) = ""
            string path (label="", font-size=12px, width=50%) = ""
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
    /// The heading the drawer lists it under; none for the top of the list. A list made before
    /// there were folders has no such field, and every document in it is at the top.
    #[serde(default)]
    pub folder: String,
    /// The label of the tab the document is wrapped in, read from the document each time the list
    /// is asked for, so that renaming the tab renames it in the drawer too. Empty when it has no
    /// such tab or cannot be read - see `labelled`.
    #[serde(default)]
    pub label: String,
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
            (entry.name.clone(), Place { path: field("path"), name: field("name"), folder: field("folder"), label: String::new() })
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

/// The list with each document's label read from the document itself.
///
/// `locate` says which file an entry names - the entry itself on the desktop, a document under its
/// folder on the server - or nothing, for one that cannot be read there. Only the document's first
/// lines are read, so opening the drawer does not read every document it lists in full.
pub fn labelled(places: Vec<Place>, locate: impl Fn(&str) -> Option<std::path::PathBuf>) -> Vec<Place> {
    places
        .into_iter()
        .map(|mut place| {
            place.label = locate(&place.path).and_then(|file| label_in(&file)).unwrap_or_default();
            place
        })
        .collect()
}

/// The label of the tab a document is wrapped in, when its first node is a tab with one - written
/// as one, or an instance of a template in another file that is one, as the project trackers are.
///
/// Wrapping a whole document in a tab is a habit rather than a rule, but where it is followed the
/// tab's label is what the document calls itself - and what it is shown as when documents are put
/// together, each in its own tab.
fn label_in(file: &Path) -> Option<String> {
    use std::io::BufRead;
    let reader = std::io::BufReader::new(std::fs::File::open(file).ok()?);
    let mut head = String::new();
    for line in reader.lines().take(200) {
        let line = line.ok()?;
        let trimmed = line.trim_start();
        if head.is_empty() {
            if trimmed.is_empty() || trimmed.starts_with("//") {
                continue;
            }
            if !(trimmed.starts_with("tab ") || trimmed.starts_with("tab(") || trimmed.starts_with('<')) {
                return None;
            }
        }
        head.push_str(&line);
        head.push('\n');
        if let Some(open) = opening_brace(&head) {
            // The tab's header and nothing of what it holds: parsed as an empty tab.
            let header = format!("{}}}\n", &head[..=open]);
            let (_, nodes) = crate::parser::parse_document(&header).ok()?;
            let label = tab_label(nodes.first()?, file.parent()?, 8)?;
            return (!label.is_empty()).then_some(label);
        }
    }
    None
}

/// What a tab says it is called: its own label, or what its template says when it gives none.
/// Nothing when it is not a tab, nor an instance of one - the template followed through the files
/// it names, `depth` of them at most, as `bases` finds them for a document in `beside`. A template
/// of the document's own would be further down than the lines read, and is not looked for.
fn tab_label(node: &OverseerNode, beside: &Path, depth: usize) -> Option<String> {
    let own = text_of(node.parameters.get("label")).trim().to_string();
    let Some(template) = &node.template else {
        return (node.node_type == "tab").then_some(own);
    };
    let base = crate::bases::find_beside(beside, template).filter(|_| depth > 0)?;
    let inherited = tab_label(&base, beside, depth - 1)?;
    Some(if own.is_empty() { inherited } else { own })
}

/// Where the first `{` outside a quoted string is.
fn opening_brace(text: &str) -> Option<usize> {
    let mut quoted = false;
    let mut escaped = false;
    for (at, c) in text.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            '{' if !quoted => return Some(at),
            _ => {}
        }
    }
    None
}
