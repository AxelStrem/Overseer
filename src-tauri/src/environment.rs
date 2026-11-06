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
//!
//! `week_start` is the day a week starts on, and may say `"system"` too: the first day of the
//! week in the page's own locale, which it reports with each request as it does the scheme. Where
//! nobody reported one, as for the bot, a week starts on Monday, as ISO says. It is there whether
//! or not settings are in force, as `me` is, so a calendar can count on it.

use crate::types::{OverseerNode, OverseerValue};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The settings document's name, beside the documents on the server and in the app's settings
/// folder.
pub const FILE: &str = "settings.os";

/// What the settings say until someone has chosen: the theme that ships with Overseer, light or
/// dark as the system is.
pub const DEFAULT: &str = "// How every document looks, until it says otherwise: the theme it is drawn in, whether it is\r\n// light or dark, and the day a week starts on. \"system\" follows the browser's or the computer's\r\n// own setting.\r\n\r\ncontext theme (base=<themes/default.os/theme>) { }\r\ncontext mode = \"system\"\r\ncontext week_start = \"system\"\r\n";

/// The name whose `"system"` is the reported scheme.
pub const MODE: &str = "mode";

/// What `mode` says to follow the system.
pub const SYSTEM: &str = "system";

/// The name of the day a week starts on, whose `"system"` is the reported first day.
pub const WEEK_START: &str = "week_start";

/// The days a week can start on, as `week_start` names them, from Monday.
pub const DAYS: [&str; 7] = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];

/// The context that says who is looking: `me/handle`, their username, `me/name`, what they are
/// called, and `me/admin`, whether they run the server. Empty and false for whoever has no
/// account - see `people`.
pub const ME: &str = "me";

/// The context that says what the document is shown in: `host/kind`, the themes there are and the
/// one chosen, and `host/open`, the document open beside it - see `Open`.
pub const HOST: &str = "host";

/// The list in `host` of the names of the library's icons - see `set_icons`.
pub const ICONS: &str = "icons";

/// What `host/kind` says until the process says otherwise: a page the server serves.
pub const BROWSER: &str = "browser";

/// The document open beside the one being worked out - the one a drawer is drawn next to - as
/// whoever serves the request says it: its name there, and what the asker may do with it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Open {
    pub path: String,
    pub name: String,
    /// What the drawer calls it: the label of the tab it is wrapped in, else `name`.
    pub label: String,
    /// `"edit"`, `"read"`, or empty where the asker may not open it at all.
    pub can: String,
    /// Whether there is a write to it that undo would take back.
    pub undoable: bool,
}

/// One of the keys of whoever is looking, as `me/keys` lists it: never the key itself.
#[derive(Clone, Debug, PartialEq)]
pub struct Key {
    pub name: String,
    pub made: chrono::DateTime<chrono::Utc>,
    /// When it was last used, if ever.
    pub used: Option<chrono::DateTime<chrono::Utc>>,
}

static FILE_HERE:Mutex<Option<PathBuf>> = Mutex::new(None);
static SCHEME_HERE: Mutex<Option<String>> = Mutex::new(None);
static WEEK_HERE: Mutex<Option<String>> = Mutex::new(None);
static KIND_HERE: Mutex<Option<String>> = Mutex::new(None);
/// `host/icons`, parsed once: it is cloned into every document that reads it.
static ICONS_HERE: Mutex<Option<Arc<OverseerNode>>> = Mutex::new(None);

thread_local! {
    /// The settings a request is worked out under, where whoever serves it said - the server, for
    /// the folder it serves. Scoped as `DocumentManager::with_document` is, so two servers in one
    /// process, as the tests run them, do not decide for each other.
    static SCOPED_FILE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    /// The scheme the page that sent this request is in.
    static SCOPED_SCHEME: RefCell<Option<String>> = const { RefCell::new(None) };
    /// The first day of the week in the locale of the page that sent this request.
    static SCOPED_WEEK: RefCell<Option<String>> = const { RefCell::new(None) };
    /// Whether whoever this request is answered for runs the server - something the server's
    /// accounts know, and the documents do not.
    static SCOPED_ADMIN: RefCell<Option<bool>> = const { RefCell::new(None) };
    /// The document the page that sent this request has open beside the one asked about.
    static SCOPED_OPEN: RefCell<Option<Open>> = const { RefCell::new(None) };
    /// The keys of whoever this request is answered for, which the server's accounts hold and
    /// no document does.
    static SCOPED_KEYS: RefCell<Option<Vec<Key>>> = const { RefCell::new(None) };
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

/// The first day of the week the desktop app's page last reported. Anything but a day is left out.
pub fn set_week_start(day: Option<&str>) {
    if let Ok(mut held) = WEEK_HERE.lock() {
        *held = day.and_then(a_day).map(str::to_string);
    }
}

/// What this process shows documents in, as `host/kind` says it: `"desktop"` for the app.
pub fn set_kind(kind: &str) {
    if let Ok(mut held) = KIND_HERE.lock() {
        *held = Some(kind.to_string());
    }
}

/// The names of the library's icons, as `host/icons` lists them. The page's build makes them
/// (`icons/index.json`, beside the icons), so whoever serves the page says what they are when it
/// starts - see `icon_names_in`. Until then the list is empty.
pub fn set_icons(names: Vec<String>) {
    let list = icon_list(&names);
    if let Ok(mut held) = ICONS_HERE.lock() {
        *held = Some(Arc::new(list));
    }
}

/// `list icons`, keyed by `tag`, an entry for each name.
fn icon_list(names: &[String]) -> OverseerNode {
    let mut text = format!("list {} (key=\"tag\") {{
", ICONS);
    // Written whole, by place, as `host/themes` is; `icon` draws each one before its name.
    for (at, name) in names.iter().enumerate() {
        text.push_str(&format!(
            "    div Icon__{} {{
        string tag = {}
        string icon = {}
    }}
",
            at + 1,
            quoted(name),
            quoted(name)
        ));
    }
    text.push_str("}
");
    crate::parser::parse_document(&text)
        .ok()
        .and_then(|(_, mut nodes)| nodes.pop())
        .expect("the list of icons is a document")
}

/// The names `host/icons` lists, in its order.
pub fn icons() -> Vec<String> {
    let list = ICONS_HERE.lock().ok().and_then(|held| held.clone());
    let Some(list) = list else {
        return Vec::new();
    };
    list.children
        .iter()
        .filter_map(|entry| entry.children.iter().find(|field| field.name == "tag"))
        .filter_map(|tag| match tag.parameters.get("value") {
            Some(OverseerValue::String(name)) => Some(name.clone()),
            _ => None,
        })
        .collect()
}

/// The icon names a built page keeps in `icons/index.json` under `folder`, or none where it has
/// none or they do not read.
pub fn icon_names_in(folder: &Path) -> Vec<String> {
    std::fs::read(folder.join("icons").join("index.json"))
        .ok()
        .and_then(|bytes| icon_names(&bytes))
        .unwrap_or_default()
}

/// The icon names an `index.json` holds: a list of strings.
pub fn icon_names(bytes: &[u8]) -> Option<Vec<String>> {
    serde_json::from_slice(bytes).ok()
}

/// Whether a document reads `host/icons` - names it in any parameter, a formula or a
/// `vocabulary`. Some 1,500 entries would cost every other document its work-out and its size
/// on the way to the page, so `host` carries them only for one that does - see `with_icons`.
pub fn reads_icons(nodes: &[OverseerNode]) -> bool {
    nodes.iter().any(|node| {
        node.parameters.values().any(|value| match value {
            OverseerValue::String(text) | OverseerValue::Formula(text) => {
                text.contains("host/icons")
            }
            _ => false,
        }) || reads_icons(&node.children)
    })
}

/// The environment's declarations with `host/icons` in its `host`.
pub fn with_icons(declarations: &mut [OverseerNode]) {
    let list = ICONS_HERE.lock().ok().and_then(|held| held.clone());
    let list = match list {
        Some(list) => (*list).clone(),
        None => icon_list(&[]),
    };
    if let Some(host) = declarations.iter_mut().find(|node| node.name == HOST) {
        if !host.children.iter().any(|child| child.name == ICONS) {
            host.children.push(list);
        }
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

/// Work as the page whose locale starts a week on this day sees. Nothing reported, or anything but
/// a day, leaves what was there.
pub fn weeks_start_on<T>(day: Option<&str>, f: impl FnOnce() -> T) -> T {
    match day.and_then(a_day) {
        Some(day) => scoped(&SCOPED_WEEK, Some(day.to_string()), f),
        None => f(),
    }
}

/// The day as `DAYS` names it, whatever its case, or None for anything else.
pub fn a_day(said: &str) -> Option<&'static str> {
    let said = said.trim();
    DAYS.iter().copied().find(|day| day.eq_ignore_ascii_case(said))
}

/// Work for someone who does, or does not, run the server.
pub fn as_admin<T>(admin: bool, f: impl FnOnce() -> T) -> T {
    scoped(&SCOPED_ADMIN, Some(admin), f)
}

/// Work for someone who holds these keys, as `me/keys` lists them.
pub fn with_keys<T>(keys: Vec<Key>, f: impl FnOnce() -> T) -> T {
    scoped(&SCOPED_KEYS, Some(keys), f)
}

/// The keys of whoever is looking: none for nobody, and wherever no server said.
fn keys() -> Vec<Key> {
    if looking().is_none() {
        return Vec::new();
    }
    SCOPED_KEYS.with(|held| held.borrow().clone()).unwrap_or_default()
}

/// Work as a page that has this document open beside the one asked about. Nothing said leaves
/// what was there.
pub fn beside<T>(open: Option<Open>, f: impl FnOnce() -> T) -> T {
    match open {
        Some(open) => scoped(&SCOPED_OPEN, Some(open), f),
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

/// The folder the settings are kept in, whether or not the document at hand is seen through them -
/// where the user's own files are, such as the `libraries.txt` a mounted document's bases are
/// looked for through as much as any other's.
pub fn folder() -> Option<PathBuf> {
    SCOPED_FILE
        .with(|held| held.borrow().clone())
        .or_else(|| FILE_HERE.lock().ok().and_then(|held| held.clone()))
        .and_then(|file| file.parent().map(Path::to_path_buf))
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

/// The day `week_start = "system"` stands for here: the one reported, Monday where none was.
pub fn first_weekday() -> &'static str {
    let reported = SCOPED_WEEK
        .with(|held| held.borrow().clone())
        .or_else(|| WEEK_HERE.lock().ok().and_then(|held| held.clone()));
    reported.as_deref().and_then(a_day).unwrap_or(DAYS[0])
}

/// The day a week starts on at a document's top: what the settings say, settled, else the day
/// reported. As said, which may be no day at all - a calendar that reads it says so.
pub fn week_start() -> String {
    declarations()
        .into_iter()
        .filter(|node| node.name == WEEK_START)
        .last()
        .map(|mut node| {
            settle(&mut node);
            match node.parameters.get("value") {
                Some(OverseerValue::String(said)) => said.clone(),
                _ => String::new(),
            }
        })
        .unwrap_or_else(|| first_weekday().to_string())
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

/// The context declarations a document's top is seen under, in the order they are written: those
/// of the settings, where any are in force, then `me` and `host`, which are there without them as
/// well - empty, as for anyone nobody has signed in - and `week_start`, `"system"` where the
/// settings do not say it. None for a mounted document, which sees the
/// environment through its host.
pub fn declarations() -> Vec<OverseerNode> {
    if LEFT_OUT.with(Cell::get) {
        return Vec::new();
    }
    let mut out = Vec::new();
    let settings = file().and_then(|file| settings(&file));
    if let Some(nodes) = &settings {
        gather(nodes, &mut out);
    }
    if !out.iter().any(|node| node.name == WEEK_START) {
        out.push(week_start_system());
    }
    out.push(me());
    out.push(host(settings.as_deref()));
    out
}

/// Who is looking, by username: the person a server is answering, and nobody otherwise.
fn looking() -> Option<String> {
    match crate::access::asker() {
        crate::access::Asker::Person(handle) => Some(handle),
        _ => None,
    }
}

/// Whether whoever is looking runs the server: false for nobody, and wherever no server said.
fn admin() -> bool {
    looking().is_some() && SCOPED_ADMIN.with(|held| held.borrow().unwrap_or(false))
}

/// Whoever is looking, as `me` says them: their username and their name, both empty for nobody.
fn who_is_looking() -> (String, String) {
    match looking() {
        Some(handle) => {
            // As plain text: a display name's markup is drawn in the person's chip, and a
            // formula that writes the name into a string would show it as written.
            let name = crate::access::root()
                .and_then(|root| crate::people::name_of(&root, &handle))
                .map(|name| crate::people::plain(&name))
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| handle.clone());
            (handle, name)
        }
        None => (String::new(), String::new()),
    }
}

/// Whoever is looking's chip, as the directory of people draws it: its colour, icon and shape,
/// all empty for nobody.
fn chip_of_whoever_is_looking() -> (String, String, String) {
    looking()
        .zip(crate::access::root())
        .and_then(|(handle, root)| crate::people::person_of(&root, &handle))
        .map(|person| {
            (
                person.colour.unwrap_or_default(),
                person.icon.unwrap_or_default(),
                person.shape.unwrap_or_default(),
            )
        })
        .unwrap_or_default()
}

/// `context me`, for whoever is looking. Their name and chip are read from the server's directory
/// of people, so a change is seen at once, in every document; their keys are as the server said.
fn me() -> OverseerNode {
    let (handle, name) = who_is_looking();
    let (colour, icon, shape) = chip_of_whoever_is_looking();
    let mut text = format!(
        "context {} {{\n    string handle = {}\n    string name = {}\n    bool admin = {}\n    string colour = {}\n    string icon = {}\n    string shape = {}\n",
        ME,
        quoted(&handle),
        quoted(&name),
        admin(),
        quoted(&colour),
        quoted(&icon),
        quoted(&shape)
    );
    // Written whole, by place, as `host/themes` is.
    text.push_str("    list keys (key=\"name\") {\n");
    for (at, key) in keys().iter().enumerate() {
        text.push_str(&format!(
            "        div Key__{} {{\n            string name = {}\n            timestamp made = {}\n            timestamp used = {}\n        }}\n",
            at + 1,
            quoted(&key.name),
            quoted(&key.made.to_rfc3339()),
            quoted(&key.used.map(|used| used.to_rfc3339()).unwrap_or_default())
        ));
    }
    text.push_str("    }\n}\n");
    crate::parser::parse_document(&text)
        .ok()
        .and_then(|(_, mut nodes)| nodes.pop())
        .expect("the context of who is looking is a document")
}

/// `context week_start = "system"`, for settings that do not say it, and where there are none.
fn week_start_system() -> OverseerNode {
    let text = format!("context {} = {}\n", WEEK_START, quoted(SYSTEM));
    crate::parser::parse_document(&text)
        .ok()
        .and_then(|(_, mut nodes)| nodes.pop())
        .expect("the context of the first weekday is a document")
}

fn quoted(s: &str) -> String {
    crate::file_ops::FileOperations::serialize_value(&OverseerValue::String(s.to_string()))
}

/// `context host`: what this process shows documents in, the themes there are to choose from and
/// the one the settings chose, and the document open beside this one, where the page said.
fn host(settings: Option<&[OverseerNode]>) -> OverseerNode {
    let kind = KIND_HERE
        .lock()
        .ok()
        .and_then(|held| held.clone())
        .unwrap_or_else(|| BROWSER.to_string());
    let theme = settings.and_then(crate::app_api::chosen_theme).unwrap_or_default();
    let open = SCOPED_OPEN.with(|held| held.borrow().clone()).unwrap_or_default();
    let mut text = format!(
        "context {} {{\n    string kind = {}\n    string theme = {}\n",
        HOST,
        quoted(&kind),
        quoted(&theme)
    );
    // A keyed list, so that it is a vocabulary: an enum picks a theme out of it. Its entries are
    // written whole, by place, as a list's are once worked out: the copy of a context is put in
    // after a document's templates are, and would hold entries nothing made.
    text.push_str("    list themes (key=\"tag\") {\n");
    for (at, name) in themes().iter().enumerate() {
        text.push_str(&format!(
            "        div Theme__{} {{\n            string tag = {}\n        }}\n",
            at + 1,
            quoted(name)
        ));
    }
    text.push_str("    }\n");
    text.push_str(&format!(
        "    div open {{\n        string path = {}\n        string name = {}\n        string label = {}\n        string can = {}\n        bool undoable = {}\n    }}\n}}\n",
        quoted(&open.path),
        quoted(&open.name),
        quoted(&open.label),
        quoted(&open.can),
        open.undoable
    ));
    crate::parser::parse_document(&text)
        .ok()
        .and_then(|(_, mut nodes)| nodes.pop())
        .expect("the context of the host is a document")
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

/// A `mode` or a `week_start` that says `"system"`, made the scheme or the day it stands for. Any
/// other node is left as it is.
pub fn settle(node: &mut OverseerNode) {
    let stands_for = match node.name.as_str() {
        MODE => scheme(),
        WEEK_START => first_weekday(),
        _ => return,
    };
    if let Some(OverseerValue::String(said)) = node.parameters.get("value") {
        if said == SYSTEM {
            node.parameters
                .insert("value".to_string(), OverseerValue::String(stands_for.to_string()));
        }
    }
}

/// What tells two worked-out copies of one text in one folder apart: whether there were settings,
/// and the mode they came to. A document held worked out light is not the one a dark page wants.
pub fn key() -> String {
    // What the page or the server says of the host, which no settings file holds. What it says of
    // the themes is read from files `held` names.
    let open = SCOPED_OPEN.with(|held| held.borrow().clone());
    let host = match open {
        Some(open) if !LEFT_OUT.with(Cell::get) => format!(
            ";open={}|{}|{}|{}|{}",
            open.path, open.name, open.label, open.can, open.undoable
        ),
        _ => String::new(),
    };
    let kind = KIND_HERE.lock().ok().and_then(|held| held.clone());
    let host = match kind {
        Some(kind) if kind != BROWSER => format!("{};kind={}", host, kind),
        _ => host,
    };
    // A week from Monday says nothing, so the copies kept for most people keep their names.
    let week = week_start();
    let host = if week == DAYS[0] { host } else { format!("{};week={}", host, week) };
    if file().is_none() {
        return host;
    }
    let mode = declarations()
        .into_iter()
        .filter(|node| node.name == MODE)
        .last()
        .map(|mut node| {
            settle(&mut node);
            match node.parameters.get("value") {
                Some(OverseerValue::String(said)) => said.clone(),
                _ => String::new(),
            }
        })
        .unwrap_or_default();
    // Two people never share a copy: each sees their own `me`, and their own settings.
    match looking() {
        Some(handle) => format!("mode={};me={};admin={}{}", mode, handle, admin(), host),
        None => format!("mode={}{}", mode, host),
    }
}

/// What the environment says besides who is looking: each declaration of the settings as it
/// stands, with `mode` and `week_start` settled, and the files its themes were read from and how each stood. Two
/// people whose settings say the same in the same scheme see the same here, wherever their
/// settings are kept - see `document_cache`, which hands a document worked out for one of them to
/// the other on the strength of it.
pub fn look() -> String {
    let declarations = declarations();
    let mut out = String::new();
    for node in declarations.iter().filter(|node| node.name != ME) {
        let mut node = node.clone();
        settle(&mut node);
        describe(&node, &mut out);
    }
    for (file, stamp) in crate::bases::held(&declarations) {
        out.push_str(&file);
        out.push(' ');
        out.push_str(&stamp);
        out.push('\n');
    }
    out
}

/// Who is looking, as `me` says them - see `look`.
pub fn who() -> String {
    let (handle, name) = who_is_looking();
    let (colour, icon, shape) = chip_of_whoever_is_looking();
    // When each key was last used is left out: using one would throw away every copy kept for
    // its owner.
    let keys: Vec<String> = keys()
        .iter()
        .map(|key| format!("{}@{}", key.name, key.made.to_rfc3339()))
        .collect();
    format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}",
        handle,
        name,
        admin(),
        colour,
        icon,
        shape,
        keys.join("|")
    )
}

/// The names the environment declares, `me` among them: what a document's top sees from it.
pub fn names() -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for node in declarations() {
        if !names.contains(&node.name) {
            names.push(node.name);
        }
    }
    names
}

/// A node as `look` compares it: what it says, and nothing of where it was read from.
fn describe(node: &OverseerNode, out: &mut String) {
    out.push_str(&node.node_type);
    out.push(' ');
    out.push_str(&node.name);
    if let Some(template) = &node.template {
        out.push_str(" <");
        out.push_str(template);
        out.push('>');
    }
    out.push_str(" (");
    for (key, value) in &node.parameters {
        out.push_str(&format!("{}={:?}, ", key, value));
    }
    out.push_str(") {");
    for child in &node.children {
        describe(child, out);
    }
    out.push_str("}\n");
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
    // The directory `me/name` is read from, so that a new name is not shown the old copy.
    if let (Some(_), Some(root)) = (looking(), crate::access::root()) {
        let people = root.join(crate::people::FILE).to_string_lossy().replace('\\', "/");
        let stamp = crate::actions::mount_stamp(&people);
        out.push((people, stamp));
    }
    for found in crate::bases::held(&declarations()) {
        if !out.contains(&found) {
            out.push(found);
        }
    }
    out
}

/// The themes there are to choose from, by name: those in a `themes` folder beside the settings,
/// in one of the user's libraries and in the library that ships with Overseer. One met first in
/// `theme_folders` takes the place of one of the same name further on, as a base beside a
/// document does.
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

/// The folders themes are kept in: the `themes` folder beside the settings first, then each
/// library's, the user's before the one that ships with Overseer - the order a base
/// `<themes/x.os/theme>` is looked for in, so that one met first takes the place of one of the
/// same name further on. What a theme keeps beside it, its fonts, is looked for in the same order.
pub fn theme_folders() -> Vec<PathBuf> {
    let mut folders = Vec::new();
    if let Some(dir) = file().as_deref().and_then(Path::parent) {
        folders.push(dir.join("themes"));
    }
    for library in crate::bases::libraries() {
        folders.push(library.join("themes"));
    }
    folders
}
