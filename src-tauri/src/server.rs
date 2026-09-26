//! Serving documents over HTTP.
//!
//! The logic lives here rather than in the request handlers so it can be tested without a
//! socket, and so the handlers stay thin enough to read. Everything here is about *which*
//! document, and whether the caller is allowed to ask for it; what a document is remains
//! [`crate::app_api`]'s business, shared with the desktop app so the two cannot drift.

use crate::actions::ActionExecutor;
use crate::app_api;
use crate::docmgr::manager::DocumentManager;
use crate::types::*;
use std::path::{Component, Path, PathBuf};

/// The directory documents are served from. Nothing outside it is reachable.
#[derive(Clone, Debug)]
pub struct DocumentRoot {
    root: PathBuf,
}

#[derive(Debug, PartialEq)]
pub enum RequestError {
    /// The name is not one this server will look up, regardless of what is on disk.
    Rejected(String),
    NotFound(String),
    Failed(String),
}

impl std::fmt::Display for RequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestError::Rejected(m) | RequestError::NotFound(m) | RequestError::Failed(m) => {
                write!(f, "{}", m)
            }
        }
    }
}

impl DocumentRoot {
    pub fn new(root: impl AsRef<Path>) -> std::io::Result<Self> {
        Ok(Self {
            root: root.as_ref().canonicalize()?,
        })
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    /// The file a requested name refers to.
    ///
    /// A name arrives from the network, so it is checked rather than trusted. Parent
    /// components are refused outright instead of being normalized away - `a/../../b` can be
    /// made to look harmless by normalizing, and there is no legitimate reason to write it.
    /// The resolved path is then confirmed to still sit under the root, which is what catches
    /// a symlink pointing somewhere else entirely.
    pub fn resolve(&self, name: &str) -> std::result::Result<PathBuf, RequestError> {
        if name.is_empty() {
            return Err(RequestError::Rejected("no document named".into()));
        }
        let relative = Path::new(name);
        if relative.is_absolute() {
            return Err(RequestError::Rejected(format!(
                "'{}' is an absolute path; documents are named relative to the root",
                name
            )));
        }
        for component in relative.components() {
            match component {
                Component::Normal(_) => {}
                _ => {
                    return Err(RequestError::Rejected(format!(
                        "'{}' contains a path component that is not a name",
                        name
                    )))
                }
            }
        }
        if relative.extension().and_then(|e| e.to_str()) != Some("os") {
            return Err(RequestError::Rejected(format!(
                "'{}' is not an .os document",
                name
            )));
        }

        let candidate = self.root.join(relative);
        let resolved = candidate
            .canonicalize()
            .map_err(|_| RequestError::NotFound(format!("no document '{}'", name)))?;
        if !resolved.starts_with(&self.root) {
            // Reported as missing rather than refused: which paths exist outside the root is
            // not something a caller needs to learn from the error it gets back.
            return Err(RequestError::NotFound(format!("no document '{}'", name)));
        }
        Ok(resolved)
    }

    /// Every document under the root, named as a caller would ask for it.
    pub fn list(&self) -> Vec<String> {
        fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, root, out);
                } else if path.extension().and_then(|e| e.to_str()) == Some("os") {
                    if let Ok(relative) = path.strip_prefix(root) {
                        out.push(relative.to_string_lossy().replace('\\', "/"));
                    }
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.root, &self.root, &mut out);
        out.sort();
        out
    }

    /// Read and resolve a document, the way opening it in the app would.
    pub fn open(&self, name: &str) -> std::result::Result<Vec<OverseerNode>, RequestError> {
        self.open_for("", name)
    }

    /// The document as one viewer sees it.
    ///
    /// The file is the same for everyone; what differs is the handful of fields the document
    /// marked as the viewer's - which day is being shown, what is folded. Those are held apart
    /// from the text and applied here, so two people can look at different days and neither
    /// writes their looking into the record.
    ///
    /// An empty session is a viewer with no view of its own, which is what the bot is.
    pub fn open_for(
        &self,
        session: &str,
        name: &str,
    ) -> std::result::Result<Vec<OverseerNode>, RequestError> {
        let path = self.resolve(name)?;
        let text = std::fs::read_to_string(&path)
            .map_err(|e| RequestError::Failed(format!("could not read '{}': {}", name, e)))?;
        // Naming the document is what lets its mounts resolve against its own directory, and
        // what keeps two documents being served at once from deciding for each other.
        let dir = path.parent().map(|d| d.to_path_buf());
        let looking_at = crate::viewstate::overlay(session, name);
        DocumentManager::with_document(dir, || {
            if looking_at.is_empty() {
                app_api::load_document(text)
            } else {
                // The same call an edit makes: apply these values, then work out what reads
                // them. The overlay is exactly a set of edits that are never written down.
                let addresses = looking_at.keys().cloned().collect();
                app_api::resolve_selective(text, addresses, Some(looking_at))
            }
        })
        .map_err(|e| RequestError::Failed(format!("could not resolve '{}': {:?}", name, e)))
    }
}


/// What running an event did.
#[derive(serde::Serialize)]
pub struct EventOutcome {
    pub address: String,
    /// What the event was run on *belongs to*, afterwards.
    ///
    /// A button is not interesting; what it did to its surroundings is. Pressing an exercise's
    /// Done button leaves that exercise with a new `last_done` - returning the button would
    /// only describe the button, and the caller would have to ask again to learn anything.
    pub node: Option<OverseerNode>,
    pub child_addresses: Vec<String>,
    /// True when the event removed the thing it was run on - a delete button, say.
    pub gone: bool,
}

impl DocumentRoot {
    /// Run an event a document declares, such as pressing a button.
    ///
    /// The document decides what may happen: this runs an `on <event>` block that is already
    /// written there, rather than offering a way to do anything the document does not describe.
    ///
    /// Addressed like everything else, though actions resolve a path of names rather than an
    /// address - the translation happens here so a caller never has to know there are two.
    pub fn run_event(
        &self,
        name: &str,
        address: &str,
        event: &str,
    ) -> std::result::Result<EventOutcome, RequestError> {
        let (_, nodes) = self.edit(name, address, |nodes| {
            let path = crate::addressing::name_path(nodes, address).ok_or_else(|| {
                RequestError::NotFound(format!("nothing at '{}' in '{}'", address, name))
            })?;
            // Refused rather than run, when there is nothing there to run. Every press used to
            // answer 200 with the surrounding node, whether or not the node had the handler -
            // so pressing `.../bought/click`, with the event written into the address, looked
            // exactly like pressing `.../bought`. It was accepted three times in a row, the
            // shopping list never changed, and the only conclusion available was that the
            // document was broken.
            if let Some(node) = crate::addressing::find(nodes, address) {
                if !responds_to(node, event) {
                    return Err(RequestError::Rejected(describe_no_handler(node, address, event)));
                }
            }
            ActionExecutor::execute_event(nodes, &path, event).map_err(|e| {
                RequestError::Failed(format!("could not run '{}' on '{}': {:?}", event, address, e))
            })
        })?;

        self.journal(serde_json::json!({
            "at": chrono::Utc::now().to_rfc3339(),
            "document": name,
            "operation": "event",
            "address": address,
            "event": event,
        }));

        // What surrounds the button, or the node itself when it has nothing above it.
        let reported = match address.rsplit_once('/') {
            Some((parent, _)) => parent,
            None => address,
        };
        // Whether the thing pressed is still there, asked of the thing pressed. It used to be
        // read off whether what surrounds it was, which is the same question for a button that
        // removes the entry it sits on - but a pressed entry that removes itself is surrounded
        // by its list, which stays, and it answered that nothing had gone.
        let gone = crate::addressing::find(&nodes, address).is_none();
        match crate::addressing::find(&nodes, reported) {
            Some(node) => Ok(EventOutcome {
                child_addresses: child_addresses(reported, node),
                node: Some(node.clone()),
                address: reported.to_string(),
                gone,
            }),
            // Some buttons exist to remove the thing they sit on, and then there is nothing
            // left to describe.
            None => Ok(EventOutcome {
                address: reported.to_string(),
                node: None,
                child_addresses: Vec::new(),
                gone: true,
            }),
        }
    }
}

/// Pull a string argument, accepting either spelling the frontend might send.
fn arg_str(args: &serde_json::Value, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|n| args.get(*n).and_then(|v| v.as_str()).map(|s| s.to_string()))
}

fn arg_strings(args: &serde_json::Value, names: &[&str]) -> Vec<String> {
    names
        .iter()
        .find_map(|n| args.get(*n).and_then(|v| v.as_array()))
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn from_value<T: serde::de::DeserializeOwned>(
    args: &serde_json::Value,
    name: &str,
) -> std::result::Result<T, RequestError> {
    let value = args
        .get(name)
        .cloned()
        .ok_or_else(|| RequestError::Rejected(format!("'{}' is required", name)))?;
    serde_json::from_value(value)
        .map_err(|e| RequestError::Rejected(format!("'{}' is not what it should be: {}", name, e)))
}

fn as_json<T: serde::Serialize>(value: T) -> std::result::Result<serde_json::Value, RequestError> {
    serde_json::to_value(value)
        .map_err(|e| RequestError::Failed(format!("could not encode the answer: {}", e)))
}

impl DocumentRoot {
    /// Answer one of the commands the frontend makes.
    ///
    /// The frontend speaks to the desktop app through a single call, so it can speak to this
    /// the same way and needs no knowledge of which it is talking to. The commands are the
    /// desktop's, handled by the same `app_api` code, which is what stops a document behaving
    /// differently depending on where it is opened.
    ///
    /// `document` names which document the caller is working on. It is required for anything
    /// that resolves, because a `mount` is written relative to the document that declares it,
    /// and a server has several in play - unlike the app, it cannot assume.
    pub fn command(
        &self,
        document: Option<&str>,
        cmd: &str,
        args: &serde_json::Value,
    ) -> std::result::Result<serde_json::Value, RequestError> {
        self.command_for("", document, cmd, args)
    }

    /// The same, on behalf of one viewer.
    ///
    /// Only some commands care. What a viewer is looking at matters to anything that works the
    /// document out or writes it; parsing a string the caller supplied does not know or need to
    /// know who asked.
    pub fn command_for(
        &self,
        session: &str,
        document: Option<&str>,
        cmd: &str,
        args: &serde_json::Value,
    ) -> std::result::Result<serde_json::Value, RequestError> {
        let _ = session;
        // Saving is the browser's way of writing: it runs the action, serializes the whole
        // document, and sends the text back. The write API the bot uses is finer-grained and
        // safer, but nothing in the page speaks it - a button press there is an ordinary save.
        //
        // The hazard a whole-document save brings is that the page may be stale: the bot could
        // have recorded something since it loaded, and this text would quietly replace it. So
        // where the client tells us what it started from, that is checked first.
        if cmd.starts_with("save_") {
            return self.save(cmd, args);
        }

        let dir = match document {
            Some(name) => Some(
                self.resolve(name)?
                    .parent()
                    .map(|d| d.to_path_buf())
                    .ok_or_else(|| RequestError::Failed("document has no directory".into()))?,
            ),
            None => None,
        };
        let named = document.is_some();

        DocumentManager::with_document(dir, || match cmd {
            "load_overseer_file" => {
                let path = arg_str(args, &["path"])
                    .ok_or_else(|| RequestError::Rejected("'path' is required".into()))?;
                let file = self.resolve(&path)?;
                let text = std::fs::read_to_string(&file)
                    .map_err(|e| RequestError::Failed(format!("could not read '{}': {}", path, e)))?;
                Ok(serde_json::Value::String(text))
            }
            "parse_overseer_content" => {
                let content = arg_str(args, &["content"])
                    .ok_or_else(|| RequestError::Rejected("'content' is required".into()))?;
                if !named {
                    return Err(RequestError::Rejected(
                        "say which document this is, so what it mounts can be found".into(),
                    ));
                }
                as_json(
                    app_api::load_document_for(content, document.unwrap_or_default(), session)
                        .map_err(|e| {
                            RequestError::Failed(format!(
                                "could not resolve the document: {:?}",
                                e
                            ))
                        })?,
                )
            }
            "parse_overseer_content_selective_update" => {
                let content = arg_str(args, &["content"])
                    .ok_or_else(|| RequestError::Rejected("'content' is required".into()))?;
                let fields = arg_strings(args, &["changedFields", "changed_fields"]);
                let values = args
                    .get("changedFieldValues")
                    .or_else(|| args.get("changed_field_values"))
                    .and_then(|v| serde_json::from_value(v.clone()).ok());
                as_json(
                    app_api::resolve_selective_update(content, fields, values).map_err(|e| {
                        RequestError::Failed(format!("could not resolve the edit: {:?}", e))
                    })?,
                )
            }
            "execute_overseer_event_update" => {
                let content = arg_str(args, &["content"])
                    .ok_or_else(|| RequestError::Rejected("'content' is required".into()))?;
                let path = arg_strings(args, &["node_path", "nodePath"]);
                let event = arg_str(args, &["event_name", "eventName"])
                    .ok_or_else(|| RequestError::Rejected("'event_name' is required".into()))?;
                // The write is not done here. A press is saved by the page, which asks for it
                // immediately now rather than waiting for somebody to press Save - and doing it
                // there rather than here is what makes the app and the browser behave the same,
                // since the app has its own set of commands and never comes through this one.
                let _ = session;
                as_json(app_api::execute_event_update(content, path, event).map_err(|e| {
                    RequestError::Failed(format!("could not run the event: {:?}", e))
                })?)
            }
            // The four that send an instruction rather than a document. The page names what to
            // change and the backend changes what is on disk, so a write made in between - the
            // bot recording a meal, another tab - is still there afterwards.
            "append_overseer_entry" => {
                let named = document
                    .ok_or_else(|| RequestError::Rejected("'document' is required".into()))?;
                let at = self.resolve(named)?;
                let list = arg_strings(args, &["list_path", "listPath"]);
                let fields: std::collections::HashMap<String, OverseerValue> =
                    match args.get("fields") {
                        Some(serde_json::Value::Null) | None => Default::default(),
                        Some(given) => serde_json::from_value(given.clone()).map_err(|e| {
                            RequestError::Rejected(format!("'fields' is not field to value: {}", e))
                        })?,
                    };
                as_json(
                    app_api::append_entry_at(
                        at.to_string_lossy().as_ref(),
                        named,
                        list,
                        fields,
                        session,
                    )
                    .map_err(|e| RequestError::Failed(format!("could not append: {:?}", e)))?,
                )
            }
            "remove_overseer_entry" => {
                let named = document
                    .ok_or_else(|| RequestError::Rejected("'document' is required".into()))?;
                let at = self.resolve(named)?;
                let entry = arg_strings(args, &["entry_path", "entryPath"]);
                as_json(
                    app_api::remove_entry_at(
                        at.to_string_lossy().as_ref(),
                        named,
                        entry,
                        session,
                    )
                    .map_err(|e| RequestError::Failed(format!("could not remove: {:?}", e)))?,
                )
            }
            "ensure_overseer_entry" => {
                let named = document.ok_or_else(|| {
                    RequestError::Rejected("'document' is required".into())
                })?;
                let at = self.resolve(named)?;
                let wanted: app_api::EntryWanted = from_value(args, "wanted")?;
                as_json(
                    app_api::ensure_entry_at(
                        at.to_string_lossy().as_ref(),
                        named,
                        wanted,
                        session,
                    )
                    .map_err(|e| {
                        RequestError::Failed(format!("could not make the entry: {:?}", e))
                    })?,
                )
            }
            "write_overseer_values" => {
                let named = document.ok_or_else(|| {
                    RequestError::Rejected("'document' is required".into())
                })?;
                let at = self.resolve(named)?;
                let values: Vec<app_api::ValueWrite> = from_value(args, "values")?;
                as_json(
                    app_api::write_values_at(
                        at.to_string_lossy().as_ref(),
                        named,
                        values,
                        session,
                    )
                    .map_err(|e| RequestError::Failed(format!("could not write: {:?}", e)))?,
                )
            }
            "run_overseer_event" => {
                let named = document.ok_or_else(|| {
                    RequestError::Rejected("'document' is required".into())
                })?;
                let at = self.resolve(named)?;
                let path = arg_strings(args, &["node_path", "nodePath"]);
                let event = arg_str(args, &["event_name", "eventName"])
                    .ok_or_else(|| RequestError::Rejected("'event_name' is required".into()))?;
                // What is typed into the page's textboxes, for this press to read. Absent is
                // nothing typed, not a mistake.
                let typed: Vec<app_api::ValueWrite> = match args.get("typed") {
                    Some(v) if !v.is_null() => from_value(args, "typed")?,
                    _ => Vec::new(),
                };
                as_json(
                    app_api::run_event_at(
                        at.to_string_lossy().as_ref(),
                        named,
                        path,
                        event,
                        session,
                        typed,
                    )
                    .map_err(|e| RequestError::Failed(format!("could not run the event: {:?}", e)))?,
                )
            }
            "serialize_overseer_nodes" | "serialize_overseer_nodes_raw" => {
                let nodes: Vec<OverseerNode> = from_value(args, "nodes")?;
                as_json(
                    crate::file_ops::OverseerFileHandler::serialize_nodes(&nodes).map_err(|e| {
                        RequestError::Failed(format!("could not serialize the document: {}", e))
                    })?,
                )
            }
            "execute_overseer_event_with_text" => {
                let content = arg_str(args, &["content"])
                    .ok_or_else(|| RequestError::Rejected("'content' is required".into()))?;
                let path = arg_strings(args, &["node_path", "nodePath"]);
                let event = arg_str(args, &["event_name", "eventName"])
                    .ok_or_else(|| RequestError::Rejected("'event_name' is required".into()))?;
                as_json(app_api::execute_event_on_text(content, path, event).map_err(|e| {
                    RequestError::Failed(format!("could not run the event: {:?}", e))
                })?)
            }
            "execute_overseer_event" => {
                let mut nodes: Vec<OverseerNode> = from_value(args, "nodes")?;
                let path = arg_strings(args, &["node_path", "nodePath"]);
                let event = arg_str(args, &["event_name", "eventName"])
                    .ok_or_else(|| RequestError::Rejected("'event_name' is required".into()))?;
                app_api::execute_event(&mut nodes, &path, &event).map_err(|e| {
                    RequestError::Failed(format!("could not run the event: {:?}", e))
                })?;
                as_json(nodes)
            }
            "parse_overseer_content_selective" => {
                let content = arg_str(args, &["content"])
                    .ok_or_else(|| RequestError::Rejected("'content' is required".into()))?;
                let changed = arg_strings(args, &["changed_fields", "changedFields"]);
                let values = args
                    .get("changedFieldValues")
                    .or_else(|| args.get("changed_field_values"))
                    .and_then(|v| serde_json::from_value(v.clone()).ok());
                as_json(app_api::resolve_selective(content, changed, values).map_err(|e| {
                    RequestError::Failed(format!("could not resolve the document: {:?}", e))
                })?)
            }
            "parse_overseer_content_selective_with_text" => {
                let content = arg_str(args, &["content"])
                    .ok_or_else(|| RequestError::Rejected("'content' is required".into()))?;
                let changed = arg_strings(args, &["changed_fields", "changedFields"]);
                let values = args
                    .get("changedFieldValues")
                    .or_else(|| args.get("changed_field_values"))
                    .and_then(|v| serde_json::from_value(v.clone()).ok());
                as_json(
                    app_api::resolve_selective_with_text(content, changed, values).map_err(|e| {
                        RequestError::Failed(format!("could not resolve the document: {:?}", e))
                    })?,
                )
            }
            "undo_overseer_file" => {
                // The page speaks this surface and not /v1, so the command exists here too
                // rather than the page learning a second way to talk.
                let name = arg_str(args, &["path", "document"])
                    .or_else(|| document.map(|d| d.to_string()))
                    .ok_or_else(|| RequestError::Rejected("'path' is required".into()))?;
                Ok(self.undo(&name)?)
            }
            "find_overseer_files" => as_json(self.list()),
            // The documents kept to hand - see `menu`. The server's list lives in its own folder
            // and names documents as this server does, so whichever document the page is showing
            // has nothing to do with it.
            "menu_places" | "menu_keep" | "menu_drop" | "menu_location" => {
                let file = self.root.join(crate::menu::FILE);
                let failed = |e: OverseerError| RequestError::Failed(format!("the menu: {:?}", e));
                match cmd {
                    "menu_places" => as_json(crate::menu::places(&file).map_err(failed)?),
                    "menu_keep" => {
                        let place = arg_str(args, &["place"])
                            .ok_or_else(|| RequestError::Rejected("'place' is required".into()))?;
                        let name = arg_str(args, &["name"]).unwrap_or_default();
                        as_json(crate::menu::keep(&file, &place, &name).map_err(failed)?)
                    }
                    "menu_drop" => {
                        let place = arg_str(args, &["place"])
                            .ok_or_else(|| RequestError::Rejected("'place' is required".into()))?;
                        as_json(crate::menu::drop(&file, &place).map_err(failed)?)
                    }
                    _ => as_json(crate::menu::FILE),
                }
            }
            other => Err(RequestError::Rejected(format!("no command '{}'", other))),
        })
    }
}

/// Who is allowed to ask.
///
/// One shared secret, which is the right size of mechanism for one person's documents. It is
/// checked the same way for a browser and for an agent, so there is a single rule to reason
/// about rather than a local exemption that quietly becomes the way in.
#[derive(Clone, Default)]
pub struct Access {
    token: Option<String>,
}

/// Compare without letting the time taken say how much of the token was right.
fn same_secret(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    // Length is not secret - it is visible from the request either way - but the contents are,
    // so every byte is looked at regardless of where the first difference is.
    let mut difference = (a.len() ^ b.len()) as u8;
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        difference |= x ^ y;
    }
    difference == 0
}

impl Access {
    /// Open to anyone who can reach the port. Only defensible on loopback.
    pub fn unrestricted() -> Self {
        Self { token: None }
    }

    pub fn with_token(token: impl Into<String>) -> Self {
        Self {
            token: Some(token.into()),
        }
    }

    pub fn is_restricted(&self) -> bool {
        self.token.is_some()
    }

    /// Whether a request carrying these credentials may proceed.
    ///
    /// A browser cannot be asked to set a header on a pasted link, so a cookie counts too;
    /// it is set once by visiting the address with the token and is what makes the document
    /// readable in a tab thereafter.
    pub fn permits(&self, bearer: Option<&str>, cookie: Option<&str>) -> bool {
        let Some(expected) = self.token.as_deref() else {
            return true;
        };
        let presented = bearer
            .and_then(|v| v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer ")))
            .map(str::trim)
            .or(cookie);
        presented.map(|t| same_secret(t, expected)).unwrap_or(false)
    }
}

/// Whether an address is one only this machine can reach.
pub fn is_loopback(addr: &std::net::IpAddr) -> bool {
    addr.is_loopback()
}

/// Why a server may not start, if it may not.
///
/// Reaching beyond this machine without a token would publish a personal record to whoever
/// finds the port, so it is refused at startup rather than left as something to remember.
pub fn refuse_to_start(bind: &std::net::IpAddr, access: &Access) -> Option<String> {
    if !is_loopback(bind) && !access.is_restricted() {
        return Some(format!(
            "refusing to listen on {} without a token: set OVERSEER_TOKEN, or pass --token-file, \
             or bind to 127.0.0.1 to keep this machine's documents on this machine",
            bind
        ));
    }
    None
}

/// Build the overrides for a new entry from field names, which may name a nested field.
///
/// A document writes a nested override as a nested block:
///
/// ```text
/// append (list=...) {
///     - handle = "kiwi"
///     div per_100g {
///         - calories = 61
///     }
/// }
/// ```
///
/// so `per_100g/calories` has to become that shape rather than a child literally called
/// "per_100g/calories". It cannot be set afterwards instead: a field a new entry inherits from
/// its template goes back to the template's default on the next resolve unless the entry
/// itself states it, and the default for a calorie figure is a plausible number rather than an
/// obviously missing one - so getting this wrong records a food that looks fine and is wrong.
/// Whether this node declares something to do for this event.
///
/// A handler is a child of type `on` named for the event. A mount is the one exception: it
/// loads and unloads without anyone writing that down.
fn responds_to(node: &OverseerNode, event: &str) -> bool {
    if node.node_type == "mount" && (event == "load" || event == "unload") {
        return true;
    }
    node.children
        .iter()
        .any(|child| child.node_type == "on" && child.name == event)
}

/// Why the press did nothing, and what to press instead.
fn describe_no_handler(node: &OverseerNode, address: &str, event: &str) -> String {
    let declared: Vec<&str> = node
        .children
        .iter()
        .filter(|c| c.node_type == "on")
        .map(|c| c.name.as_str())
        .collect();

    // The mistake that produced this: the event written onto the end of the address. The
    // handler is a real node, so the address resolves and nothing looks wrong.
    if address.rsplit('/').next() == Some(event) {
        if let Some(button) = address.strip_suffix(&format!("/{}", event)) {
            return format!(
                "'{}' is the handler for '{}', not something to run it on. Press '{}' instead -                  the event is a separate argument, not part of the address.",
                address, event, button
            );
        }
    }
    if declared.is_empty() {
        format!("'{}' has no '{}' to run, and declares no events at all.", address, event)
    } else {
        format!(
            "'{}' has no '{}' to run. It declares: {}.",
            address,
            event,
            declared.join(", ")
        )
    }
}

/// Refuse an entry whose key is already taken.
///
/// A list with a `key` addresses its entries by that field, so two entries sharing a value
/// share an address - and one of them is then unreachable. Worse, a reader keying them into a
/// map keeps only the last: two diary notes written in one turn, both stamped with the time of
/// the message rather than the times of the things they described, became one note by the time
/// anything read them back. Nothing failed and nothing said so.
///
/// Refused here, where the caller is told which value is taken and can pick another - a second
/// apart is enough - rather than discovered a day later in a recap that is quietly missing
/// half of what was said.
fn reject_duplicate_key(
    list: &OverseerNode,
    fields: &std::collections::HashMap<String, OverseerValue>,
) -> std::result::Result<(), RequestError> {
    let key = match list.parameters.get("key") {
        Some(OverseerValue::String(k)) if !k.is_empty() => k.clone(),
        // No key means entries are addressed by position, where duplicates are ordinary: two
        // of the same thing on a shopping list are two things to buy.
        _ => return Ok(()),
    };
    let Some(wanted) = fields.get(&key) else {
        // Nothing to collide with. The entry will take the template's default, which is its
        // own kind of trouble but not this one.
        return Ok(());
    };

    fn value_of(entry: &OverseerNode, name: &str) -> Option<OverseerValue> {
        for child in &entry.children {
            if child.name == name {
                return child
                    .parameters
                    .get("_computed_value")
                    .or_else(|| child.parameters.get("value"))
                    .cloned();
            }
            if let Some(found) = value_of(child, name) {
                return Some(found);
            }
        }
        None
    }
    fn as_text(value: &OverseerValue) -> String {
        match value {
            OverseerValue::String(s) | OverseerValue::Timestamp(s) | OverseerValue::Date(s) => {
                s.clone()
            }
            OverseerValue::Integer(i) => i.to_string(),
            OverseerValue::Float(f) => f.to_string(),
            other => format!("{:?}", other),
        }
    }

    let taken = as_text(wanted);
    if list
        .children
        .iter()
        .filter_map(|entry| value_of(entry, &key))
        .any(|existing| as_text(&existing) == taken)
    {
        return Err(RequestError::Rejected(format!(
            concat!(
                "this list is keyed by '{}' and an entry with '{}' = {} is already in ",
                "it. Two entries with the same key share one address, and whichever is ",
                "read second is the only one anything sees. Use a different {}."
            ),
            key, key, taken, key
        )));
    }
    Ok(())
}

/// Refuse to touch an entry that is not the one the caller thinks it is.
///
/// Compared as text, because a caller sends what it read back and a figure can arrive as 1 or
/// 1.0 without meaning anything different. What matters is whether this is the same meal, not
/// whether two numbers are the same type.
fn check_expected(
    entry: &OverseerNode,
    address: &str,
    expect: &std::collections::HashMap<String, OverseerValue>,
) -> std::result::Result<(), RequestError> {
    fn as_text(value: &OverseerValue) -> String {
        match value {
            OverseerValue::String(s) => s.clone(),
            OverseerValue::Timestamp(s) | OverseerValue::Date(s) => s.clone(),
            OverseerValue::Integer(i) => i.to_string(),
            OverseerValue::Float(f) => {
                if f.fract() == 0.0 {
                    format!("{}", *f as i64)
                } else {
                    f.to_string()
                }
            }
            OverseerValue::Boolean(b) => b.to_string(),
            other => format!("{:?}", other),
        }
    }
    fn look(node: &OverseerNode, name: &str) -> Option<OverseerValue> {
        for child in &node.children {
            if child.name == name {
                return child
                    .parameters
                    .get("_computed_value")
                    .or_else(|| child.parameters.get("value"))
                    .cloned();
            }
            if let Some(found) = look(child, name) {
                return Some(found);
            }
        }
        None
    }

    for (field, wanted) in expect {
        let found = look(entry, field);
        let matches = found.as_ref().map(as_text) == Some(as_text(wanted));
        if !matches {
            return Err(RequestError::Rejected(format!(
                "'{}' is not the entry you meant: its '{}' is {}, not {}. Read the list again - \
                 removing an entry renumbers the ones after it, so an address from a moment ago \
                 may now name something else.",
                address,
                field,
                found.as_ref().map(as_text).unwrap_or_else(|| "missing".into()),
                as_text(wanted),
            )));
        }
    }
    Ok(())
}

/// The node a template name refers to, wherever it was declared.
fn find_template<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    for node in nodes {
        if node.name == name {
            return Some(node);
        }
        if let Some(found) = find_template(&node.children, name) {
            return Some(found);
        }
    }
    None
}

/// Every field name an entry made from this template could accept.
///
/// Both forms callers use: the bare name of a field anywhere in the template, and the path to
/// one - `calories` and `per_100g/calories` are the same field asked for two ways, and the
/// catalogue is written with the second.
fn accepted_field_names(
    template: &OverseerNode,
) -> (
    std::collections::BTreeSet<String>,
    std::collections::BTreeSet<String>,
) {
    fn walk(
        node: &OverseerNode,
        prefix: &str,
        all: &mut std::collections::BTreeSet<String>,
        settable: &mut std::collections::BTreeSet<String>,
    ) {
        for child in &node.children {
            // A layout wrapper stands for nothing a caller would name, so its children are
            // offered at the level the wrapper sits at rather than beneath it. `div` is what an
            // unnamed one is called, and naming it in an error message would only mislead.
            if child.name.is_empty() || child.name.starts_with('_') || child.name == "div" {
                walk(child, prefix, all, settable);
                continue;
            }
            let path = if prefix.is_empty() {
                child.name.clone()
            } else {
                format!("{}/{}", prefix, child.name)
            };
            all.insert(child.name.clone());
            all.insert(path.clone());
            // Only what is worth *suggesting*. A field the template computes is derived from
            // the others, so setting it does nothing and offering it invites a second mistake
            // immediately after the first.
            let computed = matches!(child.parameters.get("value"), Some(OverseerValue::Formula(_)));
            let container = !child.children.is_empty();
            if !computed && !container {
                settable.insert(child.name.clone());
            }
            walk(child, &path, all, settable);
        }
    }
    let mut all = std::collections::BTreeSet::new();
    let mut settable = std::collections::BTreeSet::new();
    walk(template, "", &mut all, &mut settable);
    (all, settable)
}

/// Refuse an append that names a field the entry does not have.
///
/// Accepting one silently is how a meal became an apple. The bot was told to record "the
/// handle and one amount", wrote `handle` where the template says `food`, and the server took
/// it: the amount landed, the food did not, and the record fell back to the template's default
/// food. It resolved to a real, plausible meal that nobody ate, and the stray field was pruned
/// on the next write so nothing was left pointing at the cause.
///
/// A name is worth refusing over precisely because the caller is a model. Told which names
/// exist, it corrects itself in one step; told nothing, it invents a diagnosis - and this one
/// invented a bug in the resolver and started deleting records to work around it.
fn reject_unknown_fields(
    template: &OverseerNode,
    fields: &std::collections::HashMap<String, OverseerValue>,
) -> std::result::Result<(), RequestError> {
    let (accepted, settable) = accepted_field_names(template);
    let mut unknown: Vec<&str> = fields
        .keys()
        .map(|k| k.as_str())
        .filter(|k| !accepted.contains(*k))
        .collect();
    if unknown.is_empty() {
        return Ok(());
    }
    unknown.sort_unstable();
    let offered: Vec<&str> = settable.iter().map(|s| s.as_str()).collect();
    Err(RequestError::Rejected(format!(
        "'{}' has no field called {}. Its fields are: {}",
        template.name,
        unknown
            .iter()
            .map(|u| format!("'{}'", u))
            .collect::<Vec<_>>()
            .join(", "),
        offered.join(", ")
    )))
}


/// What a write did, for the caller to report or check.
#[derive(serde::Serialize)]
pub struct WriteOutcome {
    /// The address the caller named.
    pub address: String,
    /// The subtree after resolving.
    ///
    /// A wrong reference does not fail - a food handle that matches nothing simply leaves the
    /// derived fields at their defaults - so a caller cannot tell from a bare acknowledgement
    /// whether it recorded what it meant. Reading the result back is what turns a plausible
    /// wrong answer into something the caller can notice, and it is also what a bot needs to
    /// tell a person what it just recorded.
    pub node: OverseerNode,
    /// The address of each immediate child, in order.
    ///
    /// A caller that reads a list wants to address one of its entries afterwards, and an entry
    /// of a keyed list is addressed by its key rather than its position. Saying so here keeps
    /// that rule in one place: a client working it out for itself would be a second
    /// implementation of the addressing, free to drift from this one.
    pub child_addresses: Vec<String>,
}

/// A node, with the addresses of what is under it.
#[derive(serde::Serialize)]
pub struct NodeView {
    pub address: String,
    pub node: OverseerNode,
    pub child_addresses: Vec<String>,
}

/// The address of each immediate child of a node sitting at `address`.
fn child_addresses(address: &str, node: &OverseerNode) -> Vec<String> {
    crate::addressing::child_segments(node)
        .into_iter()
        .map(|segment| format!("{}/{}", address, segment))
        .collect()
}

impl DocumentRoot {
    /// Run `work` against a document and write the result back.
    ///
    /// The file is replaced atomically: a write that fails halfway would otherwise leave a
    /// half-serialized document where the original was, and the original is the only copy.

    /// Write a document the browser has serialized for us.
    fn save(
        &self,
        cmd: &str,
        args: &serde_json::Value,
    ) -> std::result::Result<serde_json::Value, RequestError> {
        let name = arg_str(args, &["path"])
            .ok_or_else(|| RequestError::Rejected("'path' is required".into()))?;
        let path = self.resolve(&name)?;

        let text = match cmd {
            "save_overseer_file_with_original" => {
                let regenerated = arg_str(args, &["regenerated"]).ok_or_else(|| {
                    RequestError::Rejected("'regenerated' is required".into())
                })?;
                // What the page was working from. If the file no longer says that, something
                // else has written since - the bot, or another tab - and this text was built
                // without it. Refusing is the only answer that cannot lose the other write.
                Self::refuse_if_moved_on(&path, &name, arg_str(args, &["original"]).as_deref())?;
                crate::app_api::canonicalize_document(&regenerated)
            }
            "save_overseer_file_from_text" => {
                let content = arg_str(args, &["content"])
                    .ok_or_else(|| RequestError::Rejected("'content' is required".into()))?;
                // The check was written for `save_overseer_file_with_original` and then the
                // page stopped using that command: sending the text already in hand is far
                // cheaper than uploading the document, so every save has come through here,
                // where nothing was checked. Same question, asked here too.
                Self::refuse_if_moved_on(&path, &name, arg_str(args, &["original"]).as_deref())?;
                let guarded: Vec<crate::app_api::GuardedRevert> = args
                    .get("guarded")
                    .and_then(|g| serde_json::from_value(g.clone()).ok())
                    .unwrap_or_default();
                crate::app_api::save_document_from_text(content, guarded).map_err(|e| {
                    RequestError::Failed(format!("could not prepare '{}': {:?}", name, e))
                })?
            }
            "save_overseer_file" => {
                let content = arg_str(args, &["content"])
                    .ok_or_else(|| RequestError::Rejected("'content' is required".into()))?;
                Self::refuse_if_moved_on(&path, &name, arg_str(args, &["original"]).as_deref())?;
                crate::app_api::canonicalize_document(&content)
            }
            other => {
                return Err(RequestError::Rejected(format!(
                    "'{}' is not something this server knows how to do",
                    other
                )))
            }
        };

        self.write_document(&path, &name, &text)?;

        self.journal(serde_json::json!({
            "at": chrono::Utc::now().to_rfc3339(),
            "document": name,
            "operation": "save",
            "via": cmd,
        }));

        Ok(serde_json::Value::Null)
    }

    /// Load a document, change it, and write it back.
    ///
    /// `touching` is the address about to be written. A list may be showing only part of itself,
    /// and what it leaves out has no template copied onto it - which is the point, and would also
    /// mean a write to an older entry failing for a reason nobody could see. Naming the address
    /// before the document is resolved keeps what the write is about in view, and costs nothing:
    /// it is one more entry instantiated, not another resolve.
    fn edit<T>(
        &self,
        name: &str,
        touching: &str,
        work: impl FnOnce(&mut Vec<OverseerNode>) -> std::result::Result<T, RequestError>,
    ) -> std::result::Result<(T, Vec<OverseerNode>), RequestError> {
        self.edit_for("", name, touching, work)
    }

    /// The same, on behalf of one viewer.
    ///
    /// What the work writes to a field the document marked as the viewer's does not reach the
    /// file: the action reports it instead, and it is kept against this session. So pressing
    /// "previous day" changes what this page sees and nothing else - no write, no undo point,
    /// nothing for the backup to commit, and another page still looking at its own day.
    fn edit_for<T>(
        &self,
        session: &str,
        name: &str,
        touching: &str,
        work: impl FnOnce(&mut Vec<OverseerNode>) -> std::result::Result<T, RequestError>,
    ) -> std::result::Result<(T, Vec<OverseerNode>), RequestError> {
        let path = self.resolve(name)?;
        let text = std::fs::read_to_string(&path)
            .map_err(|e| RequestError::Failed(format!("could not read '{}': {}", name, e)))?;
        let text_before = text.clone();
        let dir = path.parent().map(|d| d.to_path_buf());
        let looking_at = crate::viewstate::overlay(session, name);
        let held_for_the_viewer: Vec<String> = looking_at.keys().cloned().collect();

        crate::actions::start_reporting_and_settling();
        let (outcome, nodes, serialized) = DocumentManager::with_document(dir, || {
            // Named before the document is resolved, so a list showing only part of itself
            // keeps whatever this write is about - see `resolver::keeping_in_view`.
            let _in_view = crate::resolver::keeping_in_view(touching);
            // Worked out as this viewer sees it, or a second press on a day button would start
            // from what the file says again and never get past the first step back.
            let mut nodes = if looking_at.is_empty() {
                app_api::load_document(text)
            } else {
                let addresses = looking_at.keys().cloned().collect();
                app_api::resolve_selective(text, addresses, Some(looking_at))
            }
            .map_err(|e| RequestError::Failed(format!("could not resolve '{}': {:?}", name, e)))?;
            let outcome = work(&mut nodes)?;
            // Resolve again: what was written changes what derives from it, and the caller is
            // about to be shown the result.
            crate::resolver::resolve_document(&mut nodes);
            let serialized = crate::file_ops::OverseerFileHandler::serialize_nodes(&nodes)
                .map_err(|e| RequestError::Failed(format!("could not serialize: {}", e)))?;
            Ok::<_, RequestError>((outcome, nodes, serialized))
        })?;

        // Whatever the work said belongs to the viewer is kept against this session, and taken
        // back out of what is about to be written.
        //
        // Out rather than never in: the action writes it, because the document being handed
        // back has to show the day that was asked for. It is the *file* that must not have it.
        // Removing the override is what `save_document_from_text` already does for a page's
        // save - the authored formula stands again - so the same call does it here.
        // The same rules the desktop's own writes follow: what the viewer moved is taken back
        // out of the text, kept against their session instead, and a press that moved nothing
        // else leaves the file alone - no write, no undo point, nothing for the backup to commit.
        let report = crate::actions::take_report();
        let settled = crate::app_api::settle_the_viewers_values(
            serialized,
            &text_before,
            &held_for_the_viewer,
            report.as_ref(),
        );
        for (address, value) in settled.viewers {
            crate::viewstate::set(session, name, &address, value);
        }
        if settled.worth_writing && settled.text != text_before {
            self.write_document(&path, name, &settled.text)?;
        }
        Ok((outcome, nodes))
    }

    /// Refuse a save built on a document that has since moved on.
    ///
    /// A page holds the text it loaded and sends it back to be written; if the file no longer
    /// says that, something else has written since - the bot, or another tab - and this text
    /// was built without it. There is no merge to attempt here and no safe overwrite: the
    /// honest answer is to say so and let the person reload.
    fn refuse_if_moved_on(
        path: &std::path::Path,
        name: &str,
        was: Option<&str>,
    ) -> std::result::Result<(), RequestError> {
        let Some(was) = was else { return Ok(()) };
        // A document being written for the first time has nothing to have moved on from.
        let Ok(on_disk) = std::fs::read_to_string(path) else {
            return Ok(());
        };
        if crate::app_api::still_says_what_it_did(&on_disk, Some(was)) {
            return Ok(());
        }
        Err(RequestError::Rejected(format!(
            "'{}' has changed since this page loaded it. Saving now would throw that change away, so nothing has been written. Open the document again and make the edit.",
            name
        )))
    }

    /// Put a document back the way it was before the last write.
    ///
    /// Not a reversal of what was done - the previous text is simply written back, which is why
    /// an append, a remove and a re-parent all cost the same and why a change of shape costs
    /// nothing extra. The step is taken off the history rather than copied, so undoing twice
    /// walks back two writes instead of swapping between the same two states.
    ///
    /// This write does not record an undo point of its own: the state it is taking back would
    /// otherwise become the newest step, and the history would never move.
    pub fn undo(&self, name: &str) -> std::result::Result<serde_json::Value, RequestError> {
        let path = self.resolve(name)?;
        let Some(previous) = crate::undo::take(&self.root, name) else {
            return Err(RequestError::Rejected(format!(
                "there is nothing to take back for '{}'",
                name
            )));
        };
        let temporary = path.with_extension("os.writing");
        std::fs::write(&temporary, previous.as_bytes())
            .map_err(|e| RequestError::Failed(format!("could not write '{}': {}", name, e)))?;
        std::fs::rename(&temporary, &path).map_err(|e| {
            let _ = std::fs::remove_file(&temporary);
            RequestError::Failed(format!("could not replace '{}': {}", name, e))
        })?;
        self.journal(serde_json::json!({
            "at": chrono::Utc::now().to_rfc3339(),
            "document": name,
            "operation": "undo",
        }));
        Ok(serde_json::json!({
            "undone": name,
            "steps_left": crate::undo::depth(&self.root, name),
        }))
    }

    /// Write a document, keeping what it said so the write can be taken back.
    ///
    /// Both ways of writing come through here - the addressed edits the bot makes and the saves
    /// a page sends - so there is one place that knows a document is about to change, and one
    /// place that records the step. Written beside the file and renamed over it, so a reader
    /// never sees half a document.
    fn write_document(
        &self,
        path: &std::path::Path,
        name: &str,
        text: &str,
    ) -> std::result::Result<(), RequestError> {
        // Before the write, and only when there is something to keep: a document being created
        // has nothing to go back to.
        if let Ok(previous) = std::fs::read_to_string(path) {
            // And only when something moved. A write that says what the file already says is
            // not a change; a step recorded for it is an undo that does nothing, and it makes
            // the count of what is left mean something other than how many will.
            if previous == text {
                return Ok(());
            }
            crate::undo::remember(&self.root, name, &previous);
        }
        let temporary = path.with_extension("os.writing");
        std::fs::write(&temporary, text.as_bytes())
            .map_err(|e| RequestError::Failed(format!("could not write '{}': {}", name, e)))?;
        std::fs::rename(&temporary, path).map_err(|e| {
            let _ = std::fs::remove_file(&temporary);
            RequestError::Failed(format!("could not replace '{}': {}", name, e))
        })
    }

    /// One line per write, next to the documents. Not a transaction log - the document itself
    /// is the state - but enough to answer "what did the bot put here, and when".
    fn journal(&self, entry: serde_json::Value) {
        let path = self.root.join("overseer-writes.jsonl");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            use std::io::Write;
            let _ = writeln!(file, "{}", entry);
        }
    }

    /// The subtree at an address.
    pub fn read_at(
        &self,
        name: &str,
        address: &str,
    ) -> std::result::Result<NodeView, RequestError> {
        let nodes = self.open(name)?;
        let node = crate::addressing::find(&nodes, address)
            .cloned()
            .ok_or_else(|| {
                RequestError::NotFound(format!("nothing at '{}' in '{}'", address, name))
            })?;
        Ok(NodeView {
            child_addresses: child_addresses(address, &node),
            address: address.to_string(),
            node,
        })
    }

    /// Append an entry to the list at an address, with the given fields.
    pub fn append_at(
        &self,
        name: &str,
        address: &str,
        fields: &std::collections::HashMap<String, OverseerValue>,
    ) -> std::result::Result<WriteOutcome, RequestError> {
        let overrides = crate::app_api::entry_overrides(fields);

        let (_, nodes) = self.edit(name, address, |nodes| {
            let path = crate::addressing::name_path(nodes, address).ok_or_else(|| {
                RequestError::NotFound(format!("nothing at '{}' in '{}'", address, name))
            })?;
            let target = crate::addressing::find(nodes, address).ok_or_else(|| {
                RequestError::NotFound(format!("nothing at '{}' in '{}'", address, name))
            })?;
            if target.node_type != "list" {
                return Err(RequestError::Rejected(format!(
                    "'{}' is a {}, and entries can only be appended to a list",
                    address, target.node_type
                )));
            }
            // Checked against the template the list makes its entries from. A list that names
            // no template takes whatever it is given, as it always has - there is nothing to
            // check against, and refusing everything would be worse than accepting anything.
            if let Some(template_name) = target
                .parameters
                .get("entry")
                .and_then(|v| match v {
                    OverseerValue::Template(t) => Some(t.clone()),
                    OverseerValue::String(s) => Some(s.trim_matches(['<', '>']).to_string()),
                    _ => None,
                })
            {
                if let Some(template) = find_template(nodes, &template_name) {
                    reject_unknown_fields(&template, fields)?;
                }
            }
            reject_duplicate_key(target, fields)?;
            ActionExecutor::append_entry(nodes, &format!("/{}", path.join("/")), &overrides)
                .map_err(|e| RequestError::Failed(format!("could not append: {:?}", e)))
        })?;

        self.journal(serde_json::json!({
            "at": chrono::Utc::now().to_rfc3339(),
            "document": name,
            "operation": "append",
            "address": address,
            "fields": fields,
        }));

        let node = crate::addressing::find(&nodes, address)
            .cloned()
            .ok_or_else(|| RequestError::Failed("the list vanished while being written".into()))?;
        Ok(WriteOutcome {
            child_addresses: child_addresses(address, &node),
            address: address.to_string(),
            node,
        })
    }

    /// Set the value at an address.
    pub fn set_at(
        &self,
        name: &str,
        address: &str,
        value: OverseerValue,
    ) -> std::result::Result<WriteOutcome, RequestError> {
        let (_, nodes) = self.edit(name, address, |nodes| {
            let path = crate::addressing::name_path(nodes, address).ok_or_else(|| {
                RequestError::NotFound(format!("nothing at '{}' in '{}'", address, name))
            })?;
            ActionExecutor::assign_value(nodes, &format!("/{}", path.join("/")), value.clone())
                .map_err(|e| RequestError::Failed(format!("could not set the value: {:?}", e)))
        })?;

        self.journal(serde_json::json!({
            "at": chrono::Utc::now().to_rfc3339(),
            "document": name,
            "operation": "set",
            "address": address,
            "value": value,
        }));

        let node = crate::addressing::find(&nodes, address)
            .cloned()
            .ok_or_else(|| RequestError::Failed("the node vanished while being written".into()))?;
        Ok(WriteOutcome {
            child_addresses: child_addresses(address, &node),
            address: address.to_string(),
            node,
        })
    }

    /// Remove the entry at an address, and answer with the list it came out of.
    ///
    /// The list rather than the entry, because the entry is gone and what a caller needs next
    /// is what remains - both to report it and to address the entries that shifted up.
    /// Take an entry out of a list, optionally checking first that it is the right one.
    ///
    /// `expect` is field values the entry must already have. It exists because entries of a
    /// list with no key are addressed by position, and removing one renumbers every entry
    /// after it - so an address read a moment ago can already name a different thing. A caller
    /// working through several removals is therefore removing the wrong ones from the second
    /// onward, and nothing about the result says so.
    ///
    /// That is not hypothetical. A cider was recorded correctly, then five removals and five
    /// appends chased each other through one evening's meals, and what survived was a lager
    /// nobody had ordered and no cider at all. With `expect`, the second removal refuses and
    /// says what is actually at that address.
    pub fn remove_at(
        &self,
        name: &str,
        address: &str,
        expect: &std::collections::HashMap<String, OverseerValue>,
    ) -> std::result::Result<WriteOutcome, RequestError> {
        let parent_address = address
            .rsplit_once('/')
            .map(|(head, _)| head.to_string())
            .ok_or_else(|| {
                RequestError::Rejected(format!(
                    "'{}' is a whole document rather than an entry in a list",
                    address
                ))
            })?;

        let (_, nodes) = self.edit(name, address, |nodes| {
            let path = crate::addressing::name_path(nodes, address).ok_or_else(|| {
                RequestError::NotFound(format!("nothing at '{}' in '{}'", address, name))
            })?;
            let holder = crate::addressing::find(nodes, &parent_address).ok_or_else(|| {
                RequestError::NotFound(format!("nothing at '{}' in '{}'", parent_address, name))
            })?;
            if holder.node_type != "list" {
                return Err(RequestError::Rejected(format!(
                    "'{}' is in a {}, and only entries of a list can be removed",
                    address, holder.node_type
                )));
            }
            if !expect.is_empty() {
                let entry = crate::addressing::find(nodes, address).ok_or_else(|| {
                    RequestError::NotFound(format!("nothing at '{}' in '{}'", address, name))
                })?;
                check_expected(entry, address, expect)?;
            }
            ActionExecutor::remove_entry(nodes, &format!("/{}", path.join("/")))
                .map_err(|e| RequestError::Failed(format!("could not remove: {:?}", e)))
        })?;

        self.journal(serde_json::json!({
            "at": chrono::Utc::now().to_rfc3339(),
            "document": name,
            "operation": "remove",
            "address": address,
        }));

        let node = crate::addressing::find(&nodes, &parent_address)
            .cloned()
            .ok_or_else(|| RequestError::Failed("the list vanished while being written".into()))?;
        Ok(WriteOutcome {
            child_addresses: child_addresses(&parent_address, &node),
            address: parent_address,
            node,
        })
    }
}
