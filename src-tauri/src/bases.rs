//! Templates written in another file: `<project_base.os/project>`.
//!
//! A template reference whose path starts with a file - any segment ending in `.os` before the
//! last - names a node of that file rather than of the document. The file is parsed and not
//! worked out: its nodes are copied into the document that names them as any template's are, so
//! their formulas run against that document's data.
//!
//! What a base's own references mean is settled when it is read, in its own scope: a reference to
//! a third file is looked up next to the base, and a bare one that names a node of the base names
//! that node. Both are rewritten to the base's or the third file's full path, so they mean the same
//! once copied into a document in another folder. A bare one the base does not define is left as
//! it is, and the document that inherits it supplies the node.

use crate::types::{OverseerNode, OverseerValue};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

/// The parameter an instance or a list is given when the template it names could not be had -
/// its file missing or unparsable, the node not in it, or an instance that holds itself.
pub const ERROR: &str = "_inherit_error";

/// The parameter an instance of another file's template is given: that file's full path. The
/// resolver lets go of the reference once the copy is made, and a document held worked out has to
/// know which files it was made from - see `held`.
pub const FROM: &str = "_inherit_from";

/// The parameter a list entry keeps what was stacked on its list's template in, as written:
/// `- <Catalog/[apple]> { }` keeps `Catalog/[apple]`. An instance keeps its whole reference,
/// `<Card, Fancy>`, where it always kept the one.
pub const STACKED: &str = "_stacked";

/// A template reference, split into the file it names, if any, and the node's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub file: Option<String>,
    pub name: String,
    /// The key of the list entry it names, when it ends in `[key]`: `<Catalog/[apple]>`. `name`
    /// is then the list's.
    pub entry: Option<String>,
    /// Its segments after the file, the key left out: where the node is, for one found by its
    /// place rather than by its name alone.
    pub path: Vec<String>,
}

/// The references of a template as written, one or several: `<Card, Fancy>` is `Card` and
/// `Fancy`. A comma inside `[...]` belongs to the key it is in.
pub fn references(path: &str) -> Vec<String> {
    let path = path.trim();
    let path = path
        .strip_prefix('<')
        .and_then(|p| p.strip_suffix('>'))
        .unwrap_or(path);
    split_outside_brackets(path, ',')
        .into_iter()
        .map(|one| one.trim().to_string())
        .filter(|one| !one.is_empty())
        .collect()
}

/// The first reference of a template as written, and those stacked after it.
pub fn first_and_stacked(path: &str) -> (String, Vec<String>) {
    let mut all = references(path);
    if all.is_empty() {
        return (String::new(), all);
    }
    let first = all.remove(0);
    (first, all)
}

fn split_outside_brackets(text: &str, at: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (index, c) in text.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            _ if c == at && depth == 0 => {
                parts.push(&text[start..index]);
                start = index + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

/// Split a reference as written - `<Task>`, `<../Task>`, `<base.os/Task>`,
/// `<base.os/Foods/[apple]>` - into its parts. The node is named by the last segment, as it always
/// has been; what comes before it only matters when it names a file, or when the reference names
/// an entry of a list. Of several, `<Card, Fancy>`, this is the first: the template the node is
/// made from, before anything is stacked on it.
pub fn reference(path: &str) -> Reference {
    let first = references(path).into_iter().next().unwrap_or_default();
    let mut segments: Vec<&str> = split_outside_brackets(&first, '/');
    let entry = segments
        .last()
        .map(|last| last.trim())
        .filter(|last| last.len() >= 2 && last.starts_with('[') && last.ends_with(']'))
        .map(|last| last[1..last.len() - 1].trim().to_string());
    if entry.is_some() {
        segments.pop();
    }
    let name = segments.last().copied().unwrap_or("").trim().to_string();
    let file_end = segments[..segments.len().saturating_sub(1)]
        .iter()
        .rposition(|segment| segment.trim().ends_with(".os"));
    let file = file_end.map(|end| segments[..=end].join("/"));
    let path = segments[file_end.map_or(0, |end| end + 1)..]
        .iter()
        .map(|segment| segment.trim().to_string())
        .filter(|segment| !segment.is_empty() && segment != ".." && segment != ".")
        .collect();
    Reference { file, name, entry, path }
}

/// A reference written again from its parts, to mean the same from wherever it is read: the file
/// in full, then the node by its name - or, for an entry, the list by its place and the key.
fn rewritten(file: Option<&str>, found: &Reference) -> String {
    let node = match &found.entry {
        Some(key) => format!("{}/[{}]", found.path.join("/"), key),
        None => found.name.clone(),
    };
    match file {
        Some(file) => format!("{}/{}", file, node),
        None => node,
    }
}

/// The reference cut to what finding its node needs: the name, with the file in front when it
/// names one. `<../Task>` comes out `Task`, `<base.os/Task>` comes out `base.os/Task`.
pub fn short(path: &str) -> String {
    let found = reference(path);
    match found.file {
        Some(file) => format!("{}/{}", file, found.name),
        None => found.name,
    }
}

/// The node a template reference names: in the file it names, or in the document.
pub fn find(document: &[OverseerNode], path: &str) -> Option<OverseerNode> {
    find_or_why(document, path).ok().flatten()
}

/// The node a reference to another file names, that file looked for as a document in `dir` would
/// look for it: beside the document first, then in the library. Nothing for a reference to a node
/// of the document's own, or one that cannot be had.
pub fn find_beside(dir: &Path, path: &str) -> Option<OverseerNode> {
    let found = reference(path);
    let base = load(&as_key(&locate(&found.file?, Some(dir)))).ok()?;
    by_name(&base, &found.name).cloned()
}

/// The kinds of node the engine and the page choose what to do by. An instance's type is its
/// template's name, so a template named after one of these would be taken for that kind of
/// field - a root named `tags` drew a tags field - and such a name is refused. The page lowercases
/// a type before choosing, so the comparison ignores case.
pub const TYPES: &[&str] = &[
    "tab", "div", "list", "grid", "array", "calendar", "list_item", "string", "textbox", "text", "tags", "enum", "int",
    "float", "date", "timestamp", "bool", "button", "checkbox", "chart", "image", "mark", "mount",
    "context",
];

/// Whether a name is one of `TYPES`, and so cannot name a template.
pub fn is_type(name: &str) -> bool {
    TYPES.iter().any(|ty| ty.eq_ignore_ascii_case(name.trim()))
}

/// As `find`, saying why a file the reference names could not be read, or why the node it names
/// cannot be a template. A node that is not there is `Ok(None)`, as it is for a template of the
/// document's own, which may yet be made.
pub fn find_or_why(document: &[OverseerNode], path: &str) -> Result<Option<OverseerNode>, String> {
    let found = reference(path);
    if is_type(&found.name) {
        return Err(format!(
            "'{}' is a field type, and cannot name a template",
            found.name
        ));
    }
    if let Some(key) = &found.entry {
        return Err(format!(
            "'{}/[{}]' is an entry of a list, and can only follow a template: <Template, {}/[{}]>",
            found.name, key, found.name, key
        ));
    }
    match &found.file {
        None => Ok(by_name(document, &found.name).cloned()),
        Some(file) => {
            let base = load(file)?;
            match by_name(&base, &found.name) {
                Some(node) => Ok(Some(node.clone())),
                None => Err(format!("no '{}' in {}", found.name, file_name(file))),
            }
        }
    }
}

/// The template a reference names in the document itself, where it is - nothing for one in
/// another file, which is no node of the document.
pub fn in_document<'a>(document: &'a [OverseerNode], path: &str) -> Option<&'a OverseerNode> {
    let found = reference(path);
    if found.file.is_some() || is_type(&found.name) {
        return None;
    }
    by_name(document, &found.name)
}

/// An entry of a list a reference names, `<Catalog/[apple]>` or `<foods.os/Catalog/[apple]>`,
/// as it stands in the document or as its file writes it, and the template that list makes its
/// entries from, as the list names it. `Ok(None)` for a list of the document's own that is not
/// there, which may yet be made.
///
/// The list is found by its place when the reference gives one from the top of the document -
/// unnamed containers being no step, as in an address - and otherwise by its name, as a template
/// is. The entry is the one whose key field, the list's `key=`, is written with that value.
pub fn find_entry_or_why(
    document: &[OverseerNode],
    found: &Reference,
) -> Result<Option<(OverseerNode, Option<String>)>, String> {
    let key = found.entry.as_deref().unwrap_or_default();
    let look = |nodes: &[OverseerNode], held: Option<&Arc<Vec<OverseerNode>>>| {
        let Some(list) = by_place(nodes, &found.path)
            .or_else(|| by_name(nodes, &found.name))
            .filter(|node| node.node_type == "list")
        else {
            return Err(format!("no list '{}'", found.name));
        };
        let Some(OverseerValue::String(key_field)) = list.parameters.get("key") else {
            return Err(format!(
                "list '{}' has no key=, and its entries cannot be named",
                found.name
            ));
        };
        let entry_template = match list.parameters.get("entry") {
            Some(OverseerValue::Template(path)) => Some(path.clone()),
            _ => None,
        };
        let entry = match held {
            Some(held) => entry_indexed(held, list, &found.path.join("/"), key_field, key),
            None => list
                .children
                .iter()
                .find(|entry| key_of(entry, key_field).as_deref() == Some(key)),
        };
        match entry {
            Some(entry) => Ok((entry.clone(), entry_template)),
            None => Err(format!("no '{}' in '{}'", key, found.name)),
        }
    };
    match &found.file {
        None => match look(document, None) {
            Ok(entry) => Ok(Some(entry)),
            Err(_) if by_name(document, &found.name).is_none() => Ok(None),
            Err(why) => Err(why),
        },
        Some(file) => {
            let base = load(file)?;
            look(&base, Some(&base))
                .map(Some)
                .map_err(|why| format!("{} of {}", why, file_name(file)))
        }
    }
}

/// What an entry's key field holds, as text: the value it writes, in the entry as parsed or as
/// worked out.
fn key_of(entry: &OverseerNode, key_field: &str) -> Option<String> {
    let field = entry.children.iter().find(|child| child.name == key_field)?;
    Some(match field.parameters.get("value")? {
        OverseerValue::String(text) => text.clone(),
        OverseerValue::Integer(n) => n.to_string(),
        OverseerValue::Float(f) => f.to_string(),
        OverseerValue::Boolean(b) => b.to_string(),
        _ => return None,
    })
}

/// Where each key of one list of one file is.
type EntryIndex = (Arc<Vec<OverseerNode>>, String, std::collections::HashMap<String, usize>);

thread_local! {
    /// Where each key of a list is, for the files entries were last taken from: a document of
    /// some hundreds of records made from a catalogue's entries would otherwise read down the
    /// catalogue once for each. Each is kept with the reading of the file it was made from,
    /// which is what tells one reading from the next.
    static ENTRIES: std::cell::RefCell<Vec<EntryIndex>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn entry_indexed<'a>(
    held: &Arc<Vec<OverseerNode>>,
    list: &'a OverseerNode,
    place: &str,
    key_field: &str,
    key: &str,
) -> Option<&'a OverseerNode> {
    ENTRIES.with(|kept| {
        let mut kept = kept.borrow_mut();
        let at = kept
            .iter()
            .position(|(file, list_at, _)| Arc::ptr_eq(file, held) && list_at == place);
        let at = match at {
            Some(at) => at,
            None => {
                let mut index = std::collections::HashMap::new();
                for (position, entry) in list.children.iter().enumerate() {
                    if let Some(key) = key_of(entry, key_field) {
                        index.entry(key).or_insert(position);
                    }
                }
                if kept.len() >= 8 {
                    kept.remove(0);
                }
                kept.push((held.clone(), place.to_string(), index));
                kept.len() - 1
            }
        };
        kept[at].2.get(key).and_then(|position| list.children.get(*position))
    })
}

/// The node at a place given from the top: each step a name, unnamed containers looked through.
fn by_place<'a>(nodes: &'a [OverseerNode], path: &[String]) -> Option<&'a OverseerNode> {
    fn step<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
        for node in nodes {
            if node.name == name {
                return Some(node);
            }
            if crate::addressing::is_wrapper(node) || node.name.is_empty() {
                if let Some(found) = step(&node.children, name) {
                    return Some(found);
                }
            }
        }
        None
    }
    let (first, rest) = path.split_first()?;
    let mut node = step(nodes, first)?;
    for name in rest {
        node = step(&node.children, name)?;
    }
    Some(node)
}

fn by_name<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    for node in nodes {
        if node.name == name {
            return Some(node);
        }
        if let Some(found) = by_name(&node.children, name) {
            return Some(found);
        }
    }
    None
}

/// Where a file a reference names is. The one place a base is looked for: next to the document
/// that names it, or next to the base that does, and failing that in the user's libraries and
/// then the templates that ship with Overseer - see `libraries`. A file beside the document
/// always comes first.
fn locate(file: &str, beside: Option<&Path>) -> PathBuf {
    let path = Path::new(file);
    if path.is_absolute() {
        return path.to_path_buf();
    }
    let near = nearby(file, beside);
    if near.exists() {
        return near;
    }
    in_library(file).unwrap_or(near)
}

/// Where a file a reference names would be if it were beside the document or base.
fn nearby(file: &str, beside: Option<&Path>) -> PathBuf {
    match beside {
        Some(dir) => dir.join(file),
        None => crate::docmgr::manager::DocumentManager::resolve_from_document(file),
    }
}

/// The file in the first library that has it, if one does. A path that climbs out of a folder
/// (`../x.os`) is written relative to the document and never means a library file.
fn in_library(file: &str) -> Option<PathBuf> {
    in_libraries(file).into_iter().find(|found| found.exists())
}

/// Where a file would be in each library, in the order they are looked in. Nothing for a path
/// that climbs out of a folder.
fn in_libraries(file: &str) -> Vec<PathBuf> {
    let path = Path::new(file);
    if path.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
        return Vec::new();
    }
    libraries().into_iter().map(|dir| dir.join(path)).collect()
}

/// The folders bases are looked for in, a line each. A document held worked out is checked
/// against this as well as against its bases' files: a library put somewhere else changes where a
/// base is found, and none of the files it was found in before has changed - see `document_cache`.
pub fn looked_in() -> String {
    libraries()
        .iter()
        .map(|dir| dir.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The file a user names their own library folders in, beside the settings - see `libraries`.
pub const LIBRARIES: &str = "libraries.txt";

/// Where the user's `libraries.txt` is, or would be: beside the settings.
pub fn libraries_file() -> Option<PathBuf> {
    crate::environment::folder().map(|dir| dir.join(LIBRARIES))
}

/// The folders bases are looked for in once they are not beside the document: the user's own,
/// in the order `libraries.txt` names them, and then the templates that ship with Overseer.
///
/// `libraries.txt` names a folder a line - so that a program that ships bases of its own, as
/// Brocessor does its patches', adds a line for its folder when it is installed and takes it off
/// when it goes. A blank line and one that starts with `#` are passed over, and a folder not
/// written in full is beside the file. It is read each time, as the files it lists are looked
/// for each time.
pub fn libraries() -> Vec<PathBuf> {
    let mut folders = Vec::new();
    if let Some(file) = libraries_file() {
        if let Ok(text) = std::fs::read_to_string(&file) {
            let beside = file.parent().map(Path::to_path_buf).unwrap_or_default();
            for line in text.lines().map(str::trim) {
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                folders.push(beside.join(line));
            }
        }
    }
    folders.extend(library());
    folders
}

/// The file a user names the extensions of their documents in, beside `libraries.txt` - see
/// `document_extensions`.
pub const EXTENSIONS: &str = "extensions.txt";

/// The extensions a file holding an Overseer document may have, lower case and without the dot:
/// `os`, and then those `extensions.txt` beside the settings names, a line each.
///
/// It is kept as `libraries.txt` is, so that a program that saves its own files as Overseer
/// text under an extension of its own - Brocessor its `.brot` patches - adds a line for it when
/// it is installed, and they open as any document does. A blank line and one that starts with `#`
/// are passed over, and a leading dot is allowed. It is read each time.
pub fn document_extensions() -> Vec<String> {
    let mut extensions = vec!["os".to_string()];
    let file = crate::environment::folder().map(|dir| dir.join(EXTENSIONS));
    if let Some(text) = file.and_then(|file| std::fs::read_to_string(file).ok()) {
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let extension = line.trim_start_matches('.').to_ascii_lowercase();
            if !extension.is_empty() && !extensions.contains(&extension) {
                extensions.push(extension);
            }
        }
    }
    extensions
}

/// Whether a file's extension is one of `document_extensions`, whatever its case.
pub fn is_document(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    let extension = extension.to_ascii_lowercase();
    document_extensions().contains(&extension)
}

/// The templates that ship with Overseer - its `library` folder - which any document can
/// inherit from wherever it is. `OVERSEER_LIBRARY` names the folder where it is put somewhere of
/// its own, as the server image does. Without it, the checkout the program was built from is
/// used while it is there, so an edit to a template is seen at once; and then the copy the
/// desktop bundle carries beside the executable (`library`, or on macOS in `Resources`).
///
/// The variable is read each time, so a test can point it elsewhere; the rest is looked for once.
pub fn library() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("OVERSEER_LIBRARY").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    static DEFAULT: LazyLock<Option<PathBuf>> = LazyLock::new(|| {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../library");
        let exe = std::env::current_exe().ok();
        let dir = exe.as_deref().and_then(Path::parent);
        let bundled = dir.map(|d| d.join("library"));
        let mac = dir.map(|d| d.join("../Resources/library"));
        std::iter::once(Some(source))
            .chain([bundled, mac])
            .flatten()
            .find(|p| p.is_dir())
            .map(|p| p.canonicalize().unwrap_or(p))
    });
    DEFAULT.clone()
}

/// A path as a reference can carry it: absolute where the file is there to be found, and with
/// forward slashes, which is what a reference is split on.
fn as_key(path: &Path) -> String {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let text = path.to_string_lossy().replace('\\', "/");
    // `canonicalize` on Windows gives the verbatim form, which nothing else here writes.
    text.strip_prefix("//?/").map(str::to_string).unwrap_or(text)
}

/// The file's name alone, for an error message - never the folder it is in.
fn file_name(file: &str) -> String {
    Path::new(file)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| file.to_string())
}

type Stamp = (std::time::SystemTime, u64);

fn stamp(path: &str) -> Option<Stamp> {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok().map(|t| (t, m.len())))
}

/// Bounded, so a base read once is not held for good - see `crate::bounded`.
static CACHE: LazyLock<Mutex<crate::bounded::Bounded<String, (Stamp, Arc<Vec<OverseerNode>>)>>> =
    LazyLock::new(|| Mutex::new(crate::bounded::Bounded::new()));

/// How many bases are held parsed, and about how many bytes - for the server's memory report.
pub fn parsed_held() -> (usize, usize) {
    CACHE.lock().map_or((0, 0), |cache| (cache.len(), cache.bytes()))
}

/// A base's nodes as parsed, with its own references settled in its scope. Read again only once
/// the file on disk has changed: a document is worked out again after every edit, and each time
/// it asks for the same base.
pub fn load(file: &str) -> Result<Arc<Vec<OverseerNode>>, String> {
    let key = as_key(&locate(file, None));
    // A base in someone else's folder is read on the same terms as a mount of it - see `access`.
    if !crate::access::may_read(&key) {
        return Err(format!("{} could not be read", file_name(file)));
    }
    let now = stamp(&key);
    if let Some(now) = now {
        if let Ok(mut cache) = CACHE.lock() {
            if let Some((then, nodes)) = cache.get(&key) {
                if *then == now {
                    return Ok(nodes.clone());
                }
            }
        }
    }
    let shown = file_name(file);
    let text = std::fs::read_to_string(&key).map_err(|_| format!("{} could not be read", shown))?;
    let nodes = Arc::new(parse_as_at(&text, &key)?);
    if let Some(now) = now {
        if let Ok(mut cache) = CACHE.lock() {
            let bytes = crate::document_cache::footprint(&nodes);
            cache.insert(key, (now, nodes.clone()), bytes, crate::bounded::limit());
        }
    }
    Ok(nodes)
}

/// A file's text parsed, with its references settled as though it were the file at `key`: a
/// reference to another file is looked for beside it, and one to a node of its own names that node
/// wherever it is copied to. What `load` does with a file it has read, and what the environment
/// does with the text it stands in for a file that is not there - see `environment`.
pub fn parse_as_at(text: &str, key: &str) -> Result<Vec<OverseerNode>, String> {
    let (_rest, mut nodes) = crate::parser::parse_document(text)
        .map_err(|_| format!("{} could not be parsed", file_name(key)))?;
    let beside = Path::new(key).parent().map(Path::to_path_buf);
    let mut own = std::collections::HashSet::new();
    names_in(&nodes, &mut own);
    for node in nodes.iter_mut() {
        settle(node, key, beside.as_deref(), &own);
    }
    Ok(nodes)
}

/// The full path of the file a reference names, as `load` reads it.
pub fn path_of(file: &str) -> String {
    as_key(&locate(file, None))
}

/// Every base a worked-out document was made from, and how each file stood then - those its
/// instances and lists name, and those each of these names in turn.
///
/// A document is held worked out under its own text, which an edit to its base does not change,
/// so it is only handed out while every one of these still stands as it did - as with the files
/// its mounts read, see `actions::mounts_held`.
pub fn held(nodes: &[OverseerNode]) -> Vec<(String, String)> {
    fn named(path: &str, out: &mut Vec<String>) {
        let each = references(path);
        if each.len() > 1 {
            for one in each {
                named(&one, out);
            }
            return;
        }
        if let Some(file) = reference(path).file {
            let found = path_of(&file);
            // A base taken from a library gives way to a file of that name put beside the
            // document later, or in a library looked in before, so where each of those would be
            // is watched as a missing base is.
            let near = as_key(&nearby(&file, None));
            let elsewhere = in_libraries(&file).into_iter().map(|path| as_key(&path));
            for file in [found, near].into_iter().chain(elsewhere) {
                if !out.contains(&file) {
                    out.push(file);
                }
            }
        }
    }
    fn walk(nodes: &[OverseerNode], out: &mut Vec<String>) {
        for node in nodes {
            // What a mount brought in answers for its own bases, as for its own mounts.
            if node.node_type == "mount" {
                continue;
            }
            // Several, one to a line, when what was stacked on it came from other files.
            if let Some(OverseerValue::String(from)) = node.parameters.get(FROM) {
                for file in from.lines() {
                    if !out.iter().any(|known| known == file) {
                        out.push(file.to_string());
                    }
                }
            }
            if let Some(path) = &node.template {
                named(path, out);
            }
            for value in node.parameters.values() {
                if let OverseerValue::Template(path) = value {
                    named(path, out);
                }
            }
            walk(&node.children, out);
        }
    }
    let mut files = Vec::new();
    walk(nodes, &mut files);
    // A base's own references to other files were settled to full paths when it was read, so
    // what it names is found the same way. Each file is looked at once, which also ends a cycle.
    let mut next = 0;
    while next < files.len() {
        if let Ok(base) = load(&files[next]) {
            walk(&base, &mut files);
        }
        next += 1;
    }
    // A folder named in `libraries.txt`, or taken off it, changes where a base is found.
    if !files.is_empty() {
        if let Some(list) = libraries_file() {
            files.push(as_key(&list));
        }
    }
    files
        .into_iter()
        .map(|file| {
            let stamp = crate::actions::mount_stamp(&file);
            (file, stamp)
        })
        .collect()
}

fn names_in(nodes: &[OverseerNode], names: &mut std::collections::HashSet<String>) {
    for node in nodes {
        if !node.name.is_empty() {
            names.insert(node.name.clone());
        }
        names_in(&node.children, names);
    }
}

/// Rewrite a base's references so they mean, wherever the node is copied to, what they mean in
/// the base.
fn settle(
    node: &mut OverseerNode,
    base: &str,
    beside: Option<&Path>,
    own: &std::collections::HashSet<String>,
) {
    let one = |path: &str| -> Option<String> {
        let found = reference(path);
        match &found.file {
            Some(file) => Some(rewritten(Some(&as_key(&locate(file, beside))), &found)),
            None if own.contains(&found.name) => Some(rewritten(Some(base), &found)),
            None => None,
        }
    };
    // Each reference of a stack by itself, the ones left as they are kept as written.
    let rewrite = |path: &str| -> Option<String> {
        let each = references(path);
        if each.len() < 2 {
            return one(path);
        }
        let settled: Vec<Option<String>> = each.iter().map(|path| one(path)).collect();
        if settled.iter().all(|new| new.is_none()) {
            return None;
        }
        Some(
            settled
                .into_iter()
                .zip(each)
                .map(|(new, old)| new.unwrap_or(old))
                .collect::<Vec<_>>()
                .join(", "),
        )
    };
    if let Some(path) = &node.template {
        if let Some(settled) = rewrite(path) {
            node.template = Some(settled);
        }
    }
    for (_key, value) in node.parameters.iter_mut() {
        if let OverseerValue::Template(path) = value {
            if let Some(settled) = rewrite(path) {
                *path = settled;
            }
        }
    }
    for child in node.children.iter_mut() {
        settle(child, base, beside, own);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reference_may_be_several_and_may_name_an_entry() {
        assert_eq!(references("<Card, Fancy>"), vec!["Card", "Fancy"]);
        assert_eq!(references("Card"), vec!["Card"]);
        assert_eq!(references("A, L/[x, y], B"), vec!["A", "L/[x, y]", "B"]);
        assert_eq!(reference("<Card, Fancy>").name, "Card");
        let entry = reference("foods.os/food_catalog/Catalog/[a/b]");
        assert_eq!(entry.file.as_deref(), Some("foods.os"));
        assert_eq!(entry.name, "Catalog");
        assert_eq!(entry.entry.as_deref(), Some("a/b"));
        assert_eq!(entry.path, vec!["food_catalog", "Catalog"]);
        assert_eq!(reference("Task").entry, None);
    }

    #[test]
    fn a_reference_names_its_file_only_when_a_segment_ends_in_os() {
        let parts = |path: &str| {
            let found = reference(path);
            (found.file, found.name)
        };
        assert_eq!(parts("Task"), (None, "Task".into()));
        assert_eq!(parts("../Task"), (None, "Task".into()));
        assert_eq!(parts("<Root/Task>"), (None, "Task".into()));
        assert_eq!(parts("base.os/Task"), (Some("base.os".into()), "Task".into()));
        assert_eq!(
            parts("E:/docs/lib/base.os/Task"),
            (Some("E:/docs/lib/base.os".into()), "Task".into())
        );
        assert_eq!(short("<../Task>"), "Task");
        assert_eq!(short("lib/base.os/Task"), "lib/base.os/Task");
    }
}
