//! The directory of the people a server keeps accounts for: `people.os` at its root.
//!
//! A document stores a person by their username and draws their display name through this list,
//! as a vocabulary - so a person who changes their name is shown by the new one everywhere, and
//! no document is written for it. The server writes the file whole from its accounts, whenever
//! they change; it is read by anyone and changed by no one (see `access`).
//!
//! A display name is inline markdown, as any vocabulary entry's name is: the page draws its bold,
//! colours and icons in the person's chip, and `plain` is what it says where only text can go.
//! A person may also choose a chip colour and a library icon, drawn before the name.

use crate::types::{OverseerNode, OverseerValue};
use std::path::Path;
use std::sync::{LazyLock, Mutex};

/// The directory's name, at the root of the folder a server serves.
pub const FILE: &str = "people.os";

/// One person as the directory lists them.
#[derive(Clone, Debug, PartialEq)]
pub struct Person {
    pub tag: String,
    pub name: String,
    /// Their chip's colour, where they chose one; the template's otherwise.
    pub colour: Option<String>,
    /// The library icon their chip shows before the name, where they chose one.
    pub icon: Option<String>,
}

/// The directory's text, an entry for each person in the order given.
pub fn text(people: &[Person]) -> String {
    let quoted = |s: &str| crate::file_ops::FileOperations::serialize_value(&OverseerValue::String(s.to_string()));
    let mut out = String::new();
    out.push_str("// Written by the server from its accounts. A display name is changed in the profile.\r\n");
    out.push_str("\r\n");
    out.push_str("div (hidden=true) {\r\n");
    out.push_str("    div Person (layout=\"horizontal\") {\r\n");
    out.push_str("        string tag = \"\"\r\n");
    out.push_str("        string name = \"\"\r\n");
    out.push_str("        string colour = \"#6b7280\"\r\n");
    out.push_str("        string icon = \"\"\r\n");
    out.push_str("    }\r\n");
    out.push_str("}\r\n");
    out.push_str("\r\n");
    if people.is_empty() {
        out.push_str("list People (entry=<Person>, key=\"tag\") { }\r\n");
        return out;
    }
    out.push_str("list People (entry=<Person>, key=\"tag\") {\r\n");
    for person in people {
        out.push_str("    - {\r\n");
        out.push_str(&format!("        - tag = {}\r\n", quoted(&person.tag)));
        out.push_str(&format!("        - name = {}\r\n", quoted(&person.name)));
        if let Some(colour) = &person.colour {
            out.push_str(&format!("        - colour = {}\r\n", quoted(colour)));
        }
        if let Some(icon) = &person.icon {
            out.push_str(&format!("        - icon = {}\r\n", quoted(icon)));
        }
        out.push_str("    }\r\n");
    }
    out.push_str("}\r\n");
    out
}

/// Write the directory into `root`, unless it already says this. A file rewritten with the same
/// text would still look changed to everything that keeps a copy worked out from it.
pub fn write(root: &Path, people: &[Person]) -> std::io::Result<()> {
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

/// A name with its markup taken out, as the page shows it in plain text: `<color=... | x>` is `x`,
/// a `<#key>` its key, an icon nothing, and the marks of bold, italic, strikethrough and code go -
/// an underscore only where it opens or closes, since inside a word markdown reads it as a letter.
pub fn plain(name: &str) -> String {
    // Icons first, as the page takes them out first: one inside a colour would end it early.
    static ICON: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"<icon=[^<>]*>").unwrap());
    let name = ICON.replace_all(name, " ");
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && chars.get(i + 1).is_some_and(|n| n.is_ascii_punctuation()) {
            out.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if c == '<' {
            if let Some(end) = chars[i..].iter().position(|&e| e == '>') {
                let tag: String = chars[i + 1..i + end].iter().collect();
                let taken = if let Some(inner) = tag.strip_prefix("color=") {
                    inner.find('|').map(|bar| plain(&inner[bar + 1..]))
                } else {
                    tag.strip_prefix('#').map(String::from)
                };
                if let Some(taken) = taken {
                    out.push_str(&taken);
                    i += end + 1;
                    continue;
                }
            }
        }
        let inside_a_word = || {
            i > 0 && chars[i - 1].is_alphanumeric() && chars.get(i + 1).is_some_and(|a| a.is_alphanumeric())
        };
        if c == '*' || c == '~' || c == '`' || (c == '_' && !inside_a_word()) {
            i += 1;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
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
        let person = |tag: &str, name: &str, colour: Option<&str>, icon: Option<&str>| Person {
            tag: tag.into(),
            name: name.into(),
            colour: colour.map(String::from),
            icon: icon.map(String::from),
        };
        let people = vec![
            person("ap31", "Dmitrii", Some("#e11d48"), None),
            person("bob", "Bob \"the\" \\ Builder", None, Some("hammer")),
        ];
        write(&dir, &people).unwrap();
        assert_eq!(
            read(&dir),
            people.iter().map(|p| (p.tag.clone(), p.name.clone())).collect::<Vec<_>>()
        );
        // Only a colour or an icon someone chose is written; everyone else has the template's.
        let text = std::fs::read_to_string(dir.join(FILE)).unwrap();
        assert_eq!(text.matches("- colour = ").count(), 1, "{}", text);
        assert_eq!(text.matches("- icon = ").count(), 1, "{}", text);
        assert_eq!(name_of(&dir, "bob").as_deref(), Some("Bob \"the\" \\ Builder"));
        assert_eq!(name_of(&dir, "carol"), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_name_is_plain_without_its_markup() {
        assert_eq!(plain("**C**laude <icon=bot>"), "Claude");
        assert_eq!(plain("<color=#e11d48 | C><color=#2563eb | laude>"), "Claude");
        assert_eq!(plain("*agent* ~~old~~ `code`"), "agent old code");
        assert_eq!(plain("agent_ap31 and _this_"), "agent_ap31 and this");
        assert_eq!(plain("a \\* star, and <#ap31>"), "a * star, and ap31");
        assert_eq!(plain("1 < 2 > 0"), "1 < 2 > 0");
        assert_eq!(plain("<color=red | <icon=bot> Bot>"), "Bot");
    }
}
