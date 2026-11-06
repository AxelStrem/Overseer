//! The drawer: a document of each person's own, beside their settings.
//!
//! What the drawer draws is an ordinary document that inherits the one Overseer ships,
//! `library/drawer/drawer.os`, as the project trackers inherit theirs - so a person who changes
//! nothing sees the base, and one who wants another drawer writes it. The documents kept to hand
//! are its own `Places`, edited in it like anything else, where they were a `menu.os` of their own.
//!
//! The first time a person's drawer is asked for it is made, holding what their `menu.os` held: a
//! folder there becomes an entry holding its documents, since the drawer lists them as a tree. The
//! `menu.os` itself is left as it was.
//!
//! Someone with no settings of their own - a stranger, a visitor - is given the base itself, read
//! and never written.

use crate::types::*;
use std::path::{Path, PathBuf};

/// The person's drawer, beside their settings.
pub const FILE: &str = "drawer.os";

/// The base it inherits, in the library.
pub const BASE: &str = "drawer/drawer.os";

/// The name the server gives the base to whoever has no drawer of their own. Not a name under its
/// folder, which is why it says where it is.
pub const LIBRARY_NAME: &str = "library:drawer/drawer.os";

/// The base's file, wherever the library that ships with Overseer is.
pub fn base_file() -> Option<PathBuf> {
    crate::bases::library().map(|dir| dir.join(BASE)).filter(|file| file.exists())
}

/// Whether a file is the shipped library's - read by a document, written by none.
pub fn in_shipped_library(file: &Path) -> bool {
    let Some(dir) = crate::bases::library().and_then(|dir| dir.canonicalize().ok()) else {
        return false;
    };
    file.canonicalize().is_ok_and(|file| file.starts_with(dir))
}

/// The drawer's file, made the first time it is asked for from what `menu` held.
///
/// `locate` says which file a menu entry names, as `menu::labelled` takes it, so that an entry the
/// menu gave no name to is named as the old drawer showed it.
pub fn ensure(file: &Path, menu: &Path, locate: impl Fn(&str) -> Option<PathBuf>) -> Result<()> {
    if file.exists() {
        return Ok(());
    }
    let places = if menu.exists() {
        crate::menu::labelled(crate::menu::places(menu)?, locate)
    } else {
        Vec::new()
    };
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)
            .map_err(|e| OverseerError::IoError(format!("could not make the drawer's folder: {}", e)))?;
    }
    std::fs::write(file, seed(&places))
        .map_err(|e| OverseerError::IoError(format!("could not make the drawer: {}", e)))
}

/// What a drawer is made with: the base, and the documents the menu kept, in its order - those in
/// no folder first, then each folder where its first document was.
pub fn seed(places: &[crate::menu::Place]) -> String {
    let mut top: Vec<Entry> = Vec::new();
    let mut folders: Vec<(String, usize)> = Vec::new();
    for place in places {
        let entry = Entry { name: shown(place), path: place.path.clone(), places: Vec::new() };
        let folder = place.folder.trim();
        if folder.is_empty() {
            top.push(entry);
            continue;
        }
        match folders.iter().find(|(name, _)| name == folder) {
            Some((_, at)) => top[*at].places.push(entry),
            None => {
                folders.push((folder.to_string(), top.len()));
                top.push(Entry { name: folder.to_string(), path: String::new(), places: vec![entry] });
            }
        }
    }
    // Folders after the documents in none, as the old drawer drew them.
    let (mut first, rest): (Vec<Entry>, Vec<Entry>) = top.into_iter().partition(|e| !e.path.is_empty());
    first.extend(rest);

    let mut out = format!("<{}/drawer> drawer {{\r\n", BASE);
    if !first.is_empty() {
        out.push_str("    - Places {\r\n");
        write_entries(&mut out, first, 2);
        out.push_str("    }\r\n");
    }
    out.push_str("}\r\n");
    out
}

struct Entry {
    name: String,
    path: String,
    places: Vec<Entry>,
}

/// What the old drawer called a document: its name, the label of its tab, or its file's name.
fn shown(place: &crate::menu::Place) -> String {
    [place.name.trim(), place.label.trim()]
        .into_iter()
        .find(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            let file = place.path.rsplit(['/', '\\']).next().unwrap_or(&place.path);
            file.strip_suffix(".os").unwrap_or(file).to_string()
        })
}

/// The entries of one list, each name kept once among its siblings, since the list is keyed by it.
fn write_entries(out: &mut String, entries: Vec<Entry>, depth: usize) {
    let pad = "    ".repeat(depth);
    let mut taken: Vec<String> = Vec::new();
    for entry in entries {
        let mut name = entry.name.clone();
        let mut n = 2;
        while taken.contains(&name) {
            name = format!("{} ({})", entry.name, n);
            n += 1;
        }
        taken.push(name.clone());
        out.push_str(&format!("{pad}- {{\r\n"));
        out.push_str(&format!("{pad}    - name = {}\r\n", quoted(&name)));
        if !entry.path.is_empty() {
            out.push_str(&format!("{pad}    - path = {}\r\n", quoted(&entry.path)));
        }
        if !entry.places.is_empty() {
            out.push_str(&format!("{pad}    list Places {{\r\n"));
            write_entries(out, entry.places, depth + 2);
            out.push_str(&format!("{pad}    }}\r\n"));
        }
        out.push_str(&format!("{pad}}}\r\n"));
    }
}

fn quoted(s: &str) -> String {
    crate::file_ops::FileOperations::serialize_value(&OverseerValue::String(s.to_string()))
}
