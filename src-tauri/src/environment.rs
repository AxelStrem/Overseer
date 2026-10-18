//! The environment: the contexts every document is worked out inside.
//!
//! A document's contexts stack down from its top - see `contexts`. Above its top is the user's
//! own choice of look, which every document is drawn through until it says otherwise: a theme and
//! a mode, kept in `settings.os`, an ordinary document of `context` declarations. The server keeps
//! it in the folder it serves, the desktop app in its settings folder, as each does its `menu.os`.
//! Where there is none yet, the documents are worked out as though it held `DEFAULT`.
//!
//! What it declares is seen at the top of every document, before the document's own declarations
//! are merged onto it - so a document's `context theme { - accent = red }` changes one key of the
//! user's theme and keeps the rest. A mounted document is worked out without it: what it brings in
//! takes the contexts of the place it is mounted at, the environment among them.
//!
//! `mode` may say `"system"`, which is the scheme the page reports the browser is in - light or
//! dark - with each request. A scheme nobody reported, as for the bot, is light.

use crate::types::{OverseerNode, OverseerValue};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The settings document's name, beside the documents on the server and in the app's settings
/// folder.
pub const FILE: &str = "settings.os";

/// What the settings say until someone has chosen: the theme that ships with Overseer, light or
/// dark as the system is.
pub const DEFAULT: &str = "// How every document looks, until it says otherwise: the theme it is drawn in, and whether it\r\n// is light or dark. \"system\" follows the browser's or the computer's own setting.\r\n\r\ncontext theme (base=<themes/default.os/theme>) { }\r\ncontext mode = \"system\"\r\n";

/// The name whose `"system"` is the reported scheme.
pub const MODE: &str = "mode";

/// What `mode` says to follow the system.
pub const SYSTEM: &str = "system";

static FILE_HERE: Mutex<Option<PathBuf>> = Mutex::new(None);
static SCHEME_HERE: Mutex<Option<String>> = Mutex::new(None);

thread_local! {
    /// The settings a request is worked out under, where whoever serves it said - the server, for
    /// the folder it serves. Scoped as `DocumentManager::with_document` is, so two servers in one
    /// process, as the tests run them, do not decide for each other.
    static SCOPED_FILE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    /// The scheme the page that sent this request is in.
    static SCOPED_SCHEME: RefCell<Option<String>> = const { RefCell::new(None) };
    /// Set while a mounted document is worked out, which sees the environment through its host.
    static LEFT_OUT: Cell<bool> = const { Cell::new(false) };
}

/// The settings file for this process - the desktop app's, in its settings folder.
pub fn set_file(file: Option<PathBuf>) {
    if let Ok(mut held) = FILE_HERE.lock() {
        *held = file;
    }
}

/// The scheme the desktop app's page last reported.
pub fn set_scheme(scheme: Option<&str>) {
    if let Ok(mut held) = SCHEME_HERE.lock() {
        *held = scheme.map(str::to_string);
    }
}

fn scoped<T, V: Clone>(
    slot: &'static std::thread::LocalKey<RefCell<Option<V>>>,
    value: Option<V>,
    f: impl FnOnce() -> T,
) -> T {
    let previous = slot.with(|held| held.replace(value));
    let result = f();
    slot.with(|held| *held.borrow_mut() = previous);
    result
}

/// Work under this settings file.
pub fn with_file<T>(file: PathBuf, f: impl FnOnce() -> T) -> T {
    scoped(&SCOPED_FILE, Some(file), f)
}

/// Work as the page that reported this scheme sees. Nothing reported leaves what was there.
pub fn seen_as<T>(scheme: Option<&str>, f: impl FnOnce() -> T) -> T {
    match scheme {
        Some(scheme) => scoped(&SCOPED_SCHEME, Some(scheme.to_string()), f),
        None => f(),
    }
}

/// Work out a document with no environment of its own - a mounted one.
pub fn without<T>(f: impl FnOnce() -> T) -> T {
    let previous = LEFT_OUT.with(|off| off.replace(true));
    let result = f();
    LEFT_OUT.with(|off| off.set(previous));
    result
}

/// The settings file in force, if any is.
pub fn file() -> Option<PathBuf> {
    if LEFT_OUT.with(Cell::get) {
        return None;
    }
    SCOPED_FILE
        .with(|held| held.borrow().clone())
        .or_else(|| FILE_HERE.lock().ok().and_then(|held| held.clone()))
}

/// The scheme `"system"` stands for here: `"dark"` when that was reported, `"light"` otherwise.
pub fn scheme() -> &'static str {
    let reported = SCOPED_SCHEME
        .with(|held| held.borrow().clone())
        .or_else(|| SCHEME_HERE.lock().ok().and_then(|held| held.clone()));
    match reported.as_deref() {
        Some("dark") => "dark",
        _ => "light",
    }
}

/// The settings document's nodes, with its references settled where it is: a theme it names is
/// looked for beside it, then in the library. `DEFAULT` where there is no file.
fn settings(file: &Path) -> Option<Vec<OverseerNode>> {
    let key = file.to_string_lossy().replace('\\', "/");
    if file.exists() {
        crate::bases::load(&key).ok().map(|nodes| nodes.as_ref().clone())
    } else {
        crate::bases::parse_as_at(DEFAULT, &key).ok()
    }
}

/// The context declarations a document's top is seen under, in the order they are written. None
/// where no settings are in force.
pub fn declarations() -> Vec<OverseerNode> {
    let Some(file) = file() else {
        return Vec::new();
    };
    let Some(nodes) = settings(&file) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    gather(&nodes, &mut out);
    out
}

/// The declarations of the settings' top level, looked through its wrappers as a container's are.
fn gather(nodes: &[OverseerNode], out: &mut Vec<OverseerNode>) {
    for node in nodes {
        if crate::contexts::is_context(node) {
            out.push(node.clone());
        } else if crate::addressing::is_wrapper(node) {
            gather(&node.children, out);
        }
    }
}

/// A `mode` that says `"system"`, made the scheme it stands for. Any other node is left as it is.
pub fn settle_mode(node: &mut OverseerNode) {
    if node.name != MODE {
        return;
    }
    if let Some(OverseerValue::String(said)) = node.parameters.get("value") {
        if said == SYSTEM {
            node.parameters
                .insert("value".to_string(), OverseerValue::String(scheme().to_string()));
        }
    }
}

/// What tells two worked-out copies of one text in one folder apart: whether there were settings,
/// and the mode they came to. A document held worked out light is not the one a dark page wants.
pub fn key() -> String {
    if file().is_none() {
        return String::new();
    }
    let mode = declarations()
        .into_iter()
        .filter(|node| node.name == MODE)
        .last()
        .map(|mut node| {
            settle_mode(&mut node);
            match node.parameters.get("value") {
                Some(OverseerValue::String(said)) => said.clone(),
                _ => String::new(),
            }
        })
        .unwrap_or_default();
    format!("mode={}", mode)
}

/// Every file the environment was read from, and how each stood: the settings - or where they
/// would be - and the themes they are based on. A document held worked out is only handed back
/// while all of these stand as they did, as with the files its mounts and bases read.
pub fn held() -> Vec<(String, String)> {
    let Some(file) = file() else {
        return Vec::new();
    };
    let key = file.to_string_lossy().replace('\\', "/");
    let mut out = vec![(key.clone(), crate::actions::mount_stamp(&key))];
    for found in crate::bases::held(&declarations()) {
        if !out.contains(&found) {
            out.push(found);
        }
    }
    out
}

/// The themes there are to choose from: those that ship with Overseer and those in a `themes`
/// folder beside the settings, by name. One beside the settings takes the place of a shipped one
/// of the same name, as a base beside a document does.
pub fn themes() -> Vec<String> {
    let mut names = Vec::new();
    for folder in theme_folders() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("os") {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if !names.iter().any(|n| n == stem) {
                    names.push(stem.to_string());
                }
            }
        }
    }
    names.sort();
    names
}

/// The folders themes are kept in: the `themes` folder beside the settings first, then the one
/// that ships with Overseer - the order a name is looked for in, so that one beside the settings
/// takes the place of a shipped one. What a theme keeps beside it, its fonts, is looked for in
/// the same order.
pub fn theme_folders() -> Vec<PathBuf> {
    let mut folders = Vec::new();
    if let Some(dir) = file().as_deref().and_then(Path::parent) {
        folders.push(dir.join("themes"));
    }
    if let Some(library) = crate::bases::library() {
        folders.push(library.join("themes"));
    }
    folders
}
