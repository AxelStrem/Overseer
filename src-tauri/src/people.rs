//! The directory of the people a server keeps accounts for: `people.os` at its root.
//!
//! A document stores a person by their username and draws their display name through this list,
//! as a vocabulary - so a person who changes their name is shown by the new one everywhere, and
//! no document is written for it. The server writes the file whole from its accounts, whenever
//! they change; it is read by anyone and changed by no one (see `access`).

use crate::types::{OverseerNode, OverseerValue};
use std::path::Path;
use std::sync::{LazyLock, Mutex};

/// The directory's name, at the root of the folder a server serves.
pub const FILE: &str = "people.os";

/// The directory's text, an entry for each `(username, display name)` in the order given.
pub fn text(people: &[(String, String)]) -> String {
    let quoted = |s: &str| crate::file_ops::FileOperations::serialize_value(&OverseerValue::String(s.to_string()));
    let mut out = String::new();
    out.push_str("// Written by the server from its accounts. A display name is changed in the profile.\r\n");
    out.push_str("\r\n");
    out.push_str("div (hidden=true) {\r\n");
    out.push_str("    div Person (layout=\"horizontal\") {\r\n");
    out.push_str("        string tag = \"\"\r\n");
    out.push_str("        string name = \"\"\r\n");
    out.push_str("        string colour = \"#6b7280\"\r\n");
    out.push_str("    }\r\n");
    out.push_str("}\r\n");
    out.push_str("\r\n");
    if people.is_empty() {
        out.push_str("list People (entry=<Person>, key=\"tag\") { }\r\n");
        return out;
    }
    out.push_str("list People (entry=<Person>, key=\"tag\") {\r\n");
    for (tag, name) in people {
        out.push_str("    - {\r\n");
        out.push_str(&format!("        - tag = {}\r\n", quoted(tag)));
        out.push_str(&format!("        - name = {}\r\n", quoted(name)));
        out.push_str("    }\r\n");
    }
    out.push_str("}\r\n");
    out
}

/// Write the directory into `root`, unless it already says this. A file rewritten with the same
/// text would still look changed to everything that keeps a copy worked out from it.
pub fn write(root: &Path, people: &[(String, String)]) -> std::io::Result<()> {
    let file = root.join(FILE);
    let wanted = text(people);
    if std::fs::read(&file).ok().as_deref() == Some(wanted.as_bytes()) {
        return Ok(());
    }
    // Written beside it and moved into place, so a reader never meets half a file.
    let fresh = file.with_extension("tmp");
    std::fs::write(&fresh, wanted.as_bytes())?;
    std::fs::rename(&fresh, &file)
}

type Stamp = Option<(std::time::SystemTime, u64)>;

static HELD: LazyLock<Mutex<std::collections::HashMap<std::path::PathBuf, (Stamp, Vec<(String, String)>)>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

/// Everyone the directory in `root` names, as `(username, display name)`, read again only once
/// the file has changed. Empty where there is none.
pub fn read(root: &Path) -> Vec<(String, String)> {
    let file = root.join(FILE);
    let stamp = std::fs::metadata(&file)
        .ok()
        .and_then(|m| m.modified().ok().map(|t| (t, m.len())));
    if let Ok(held) = HELD.lock() {
        if let Some((then, people)) = held.get(&file) {
            if *then == stamp {
                return people.clone();
            }
        }
    }
    let people = std::fs::read_to_string(&file)
        .ok()
        .and_then(|text| crate::parser::parse_document(&text).ok().map(|(_, nodes)| nodes))
        .map(|mut nodes| {
            crate::environment::without(|| crate::resolver::resolve_document(&mut nodes));
            let mut out = Vec::new();
            collect(&nodes, &mut out);
            out
        })
        .unwrap_or_default();
    if let Ok(mut held) = HELD.lock() {
        held.insert(file, (stamp, people.clone()));
    }
    people
}

/// The display name the directory in `root` gives `username`, if it names them.
pub fn name_of(root: &Path, username: &str) -> Option<String> {
    read(root)
        .into_iter()
        .find(|(tag, _)| tag == username)
        .map(|(_, name)| name)
}

fn collect(nodes: &[OverseerNode], out: &mut Vec<(String, String)>) {
    for node in nodes {
        if node.node_type == "list" {
            for (_, entry) in crate::addressing::effective_children(node) {
                let field = |name: &str| {
                    crate::addressing::effective_children(entry)
                        .into_iter()
                        .map(|(_, child)| child)
                        .find(|child| child.name == name)
                        .and_then(|child| {
                            child
                                .parameters
                                .get("_computed_value")
                                .or_else(|| child.parameters.get("value"))
                                .cloned()
                        })
                        .and_then(|value| match value {
                            OverseerValue::String(s) => Some(s),
                            _ => None,
                        })
                };
                if let Some(tag) = field("tag") {
                    out.push((tag, field("name").unwrap_or_default()));
                }
            }
        } else {
            collect(&node.children, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_directory_reads_back_what_it_was_written_with() {
        let dir = std::env::temp_dir().join(format!("overseer-people-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let people = vec![
            ("ap31".to_string(), "Dmitrii".to_string()),
            ("bob".to_string(), "Bob \"the\" \\ Builder".to_string()),
        ];
        write(&dir, &people).unwrap();
        assert_eq!(read(&dir), people);
        assert_eq!(name_of(&dir, "bob").as_deref(), Some("Bob \"the\" \\ Builder"));
        assert_eq!(name_of(&dir, "carol"), None);
        std::fs::remove_dir_all(&dir).ok();
    }
}
