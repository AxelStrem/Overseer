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
        let path = self.resolve(name)?;
        let text = std::fs::read_to_string(&path)
            .map_err(|e| RequestError::Failed(format!("could not read '{}': {}", name, e)))?;
        // Naming the document is what lets its mounts resolve against its own directory, and
        // what keeps two documents being served at once from deciding for each other.
        let dir = path.parent().map(|d| d.to_path_buf());
        DocumentManager::with_document(dir, || app_api::load_document(text))
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
        let (_, nodes) = self.edit(name, |nodes| {
            let path = crate::addressing::name_path(nodes, address).ok_or_else(|| {
                RequestError::NotFound(format!("nothing at '{}' in '{}'", address, name))
            })?;
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
        match crate::addressing::find(&nodes, reported) {
            Some(node) => Ok(EventOutcome {
                child_addresses: child_addresses(reported, node),
                node: Some(node.clone()),
                address: reported.to_string(),
                gone: false,
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
        // Writing is a separate matter from reading, and this server does not do it yet.
        // Refused explicitly rather than left to fail somewhere less obvious.
        if cmd.starts_with("save_") {
            return Err(RequestError::Rejected(
                "this server is read-only; changes cannot be saved".into(),
            ));
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
                as_json(app_api::load_document(content).map_err(|e| {
                    RequestError::Failed(format!("could not resolve the document: {:?}", e))
                })?)
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
                as_json(
                    app_api::execute_event_update(content, path, event).map_err(|e| {
                        RequestError::Failed(format!("could not run the event: {:?}", e))
                    })?,
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
            "get_next_timer_due_ms_from_text" => {
                let content = arg_str(args, &["content"])
                    .ok_or_else(|| RequestError::Rejected("'content' is required".into()))?;
                as_json(app_api::next_due_ms_on_text(content).map_err(|e| {
                    RequestError::Failed(format!("could not read the timers: {:?}", e))
                })?)
            }
            "get_next_timer_due_ms" => Ok(serde_json::Value::Null),
            "find_overseer_files" => as_json(self.list()),
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
fn entry_overrides(fields: &std::collections::HashMap<String, OverseerValue>) -> Vec<OverseerNode> {
    let mut roots: Vec<OverseerNode> = Vec::new();
    for (field, value) in fields {
        let mut segments = field.split('/').filter(|s| !s.is_empty()).peekable();
        let mut level = &mut roots;
        while let Some(segment) = segments.next() {
            let leaf = segments.peek().is_none();
            let existing = level.iter().position(|n| n.name == segment);
            let index = match existing {
                Some(i) => i,
                None => {
                    let mut node = if leaf {
                        let mut n = OverseerNode::new_with_type("-".to_string(), Some(segment.to_string()));
                        n.authored_dash = true;
                        n
                    } else {
                        OverseerNode::new_with_type("div".to_string(), Some(segment.to_string()))
                    };
                    node.name = segment.to_string();
                    level.push(node);
                    level.len() - 1
                }
            };
            if leaf {
                level[index]
                    .parameters
                    .insert("value".to_string(), value.clone());
            }
            level = &mut level[index].children;
        }
    }
    roots
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
    fn edit<T>(
        &self,
        name: &str,
        work: impl FnOnce(&mut Vec<OverseerNode>) -> std::result::Result<T, RequestError>,
    ) -> std::result::Result<(T, Vec<OverseerNode>), RequestError> {
        let path = self.resolve(name)?;
        let text = std::fs::read_to_string(&path)
            .map_err(|e| RequestError::Failed(format!("could not read '{}': {}", name, e)))?;
        let dir = path.parent().map(|d| d.to_path_buf());

        let (outcome, nodes, serialized) = DocumentManager::with_document(dir, || {
            let mut nodes = app_api::load_document(text).map_err(|e| {
                RequestError::Failed(format!("could not resolve '{}': {:?}", name, e))
            })?;
            let outcome = work(&mut nodes)?;
            // Resolve again: what was written changes what derives from it, and the caller is
            // about to be shown the result.
            crate::resolver::resolve_document(&mut nodes);
            let serialized = crate::file_ops::OverseerFileHandler::serialize_nodes(&nodes)
                .map_err(|e| RequestError::Failed(format!("could not serialize: {}", e)))?;
            Ok::<_, RequestError>((outcome, nodes, serialized))
        })?;

        let temporary = path.with_extension("os.writing");
        std::fs::write(&temporary, serialized.as_bytes())
            .map_err(|e| RequestError::Failed(format!("could not write '{}': {}", name, e)))?;
        std::fs::rename(&temporary, &path).map_err(|e| {
            let _ = std::fs::remove_file(&temporary);
            RequestError::Failed(format!("could not replace '{}': {}", name, e))
        })?;
        Ok((outcome, nodes))
    }

    /// Record what was done, so a mistake made from outside the app can be found afterwards.
    ///
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
        let overrides = entry_overrides(fields);

        let (_, nodes) = self.edit(name, |nodes| {
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
        let (_, nodes) = self.edit(name, |nodes| {
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
    pub fn remove_at(
        &self,
        name: &str,
        address: &str,
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

        let (_, nodes) = self.edit(name, |nodes| {
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
