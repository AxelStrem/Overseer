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

/// A template reference, split into the file it names, if any, and the node's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub file: Option<String>,
    pub name: String,
}

/// Split a reference as written - `<Task>`, `<../Task>`, `<base.os/Task>` - into its parts. The
/// node is named by the last segment, as it always has been; what comes before it only matters
/// when it names a file.
pub fn reference(path: &str) -> Reference {
    let path = path.trim();
    let path = path
        .strip_prefix('<')
        .and_then(|p| p.strip_suffix('>'))
        .unwrap_or(path)
        .trim();
    let segments: Vec<&str> = path.split('/').collect();
    let name = segments.last().copied().unwrap_or("").trim().to_string();
    let file_end = segments[..segments.len().saturating_sub(1)]
        .iter()
        .rposition(|segment| segment.trim().ends_with(".os"));
    let file = file_end.map(|end| segments[..=end].join("/"));
    Reference { file, name }
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
    "tab", "div", "list", "grid", "list_item", "string", "textbox", "text", "tags", "enum", "filter", "int",
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
/// that names it, or next to the base that does, and failing that in the templates that ship
/// with Overseer - see `library`. A file beside the document always comes first.
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

/// The file in the library, if it is there. A path that climbs out of a folder (`../x.os`) is
/// written relative to the document and never means a library file.
fn in_library(file: &str) -> Option<PathBuf> {
    let path = Path::new(file);
    if path.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
        return None;
    }
    let found = library()?.join(path);
    found.exists().then_some(found)
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

static CACHE: LazyLock<Mutex<std::collections::HashMap<String, (Stamp, Arc<Vec<OverseerNode>>)>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

/// A base's nodes as parsed, with its own references settled in its scope. Read again only once
/// the file on disk has changed: a document is worked out again after every edit, and each time
/// it asks for the same base.
pub fn load(file: &str) -> Result<Arc<Vec<OverseerNode>>, String> {
    let key = as_key(&locate(file, None));
    let now = stamp(&key);
    if let Some(now) = now {
        if let Ok(cache) = CACHE.lock() {
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
            cache.insert(key, (now, nodes.clone()));
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
        if let Some(file) = reference(path).file {
            let found = path_of(&file);
            // A base taken from the library gives way to a file of that name put beside the
            // document later, so where that file would be is watched as a missing base is.
            let near = as_key(&nearby(&file, None));
            for file in [found, near] {
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
            if let Some(OverseerValue::String(file)) = node.parameters.get(FROM) {
                if !out.contains(file) {
                    out.push(file.clone());
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
    let rewrite = |path: &str| -> Option<String> {
        let found = reference(path);
        match found.file {
            Some(file) => Some(format!("{}/{}", as_key(&locate(&file, beside)), found.name)),
            None if own.contains(&found.name) => Some(format!("{}/{}", base, found.name)),
            None => None,
        }
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
    fn a_reference_names_its_file_only_when_a_segment_ends_in_os() {
        assert_eq!(reference("Task"), Reference { file: None, name: "Task".into() });
        assert_eq!(reference("../Task"), Reference { file: None, name: "Task".into() });
        assert_eq!(reference("<Root/Task>"), Reference { file: None, name: "Task".into() });
        assert_eq!(
            reference("base.os/Task"),
            Reference { file: Some("base.os".into()), name: "Task".into() }
        );
        assert_eq!(
            reference("E:/docs/lib/base.os/Task"),
            Reference { file: Some("E:/docs/lib/base.os".into()), name: "Task".into() }
        );
        assert_eq!(short("<../Task>"), "Task");
        assert_eq!(short("lib/base.os/Task"), "lib/base.os/Task");
    }
}
