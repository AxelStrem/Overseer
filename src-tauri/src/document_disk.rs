//! What the document cache lets go of, kept on disk until it is asked for again.
//!
//! The cache in memory is bounded, and on the server tightly: at 64 MB it holds about two of the
//! documents in use, each a worked-out tree of 14 to 20 MB and a graph of 5 to 11 beside it. Being
//! asked about a third meant working out again whichever went, every time the bot moved between
//! them. So what leaves memory is written here, and a document missing from memory is looked for
//! here before it is worked out.
//!
//! Measured through the server on the real documents, moving between four of them at the default
//! 64 MB: an open that had to work its document out again took 360 to 650 ms, and one read back
//! from here 230 to 360, against 165 to 250 for one still held in memory - which is mostly the
//! answer being written out. A file is 12 to 15 MB: the tree, the graph, and the snapshots the
//! writer needs to give back the bytes it was given.
//!
//! A file is moved rather than copied: read back, it is deleted, and the document lives in memory
//! again until it is let go of once more. There is then one copy of each, and never an older one
//! on disk that a newer one in memory has moved past.
//!
//! What it must never do is hand back something worked out differently from what working it out
//! now would give. So a file is named by the text, the folder it was worked out in and the build
//! that worked it out, and holds all three to be checked against; a rebuilt server finds none of
//! the old files, since the rules they were worked out by may have changed. The files the
//! document's mounts were read from are checked as they are for a document held in memory.

use crate::dependencies::Graph;
use crate::source_registry::SourceRegistry;
use crate::types::{
    NodeBodySnapshot, NodeHeaderSnapshot, NodeSourceSnapshot, OverseerNode, SharedText,
    SnapshotOrigin,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A document as the cache held it: the tree, the graph it was worked out by, the stamps of the
/// files its mounts read, and the minute what reads the clock was worked out in.
pub struct Held {
    pub nodes: Vec<OverseerNode>,
    pub graph: Graph,
    pub mounts: Vec<(String, String)>,
    pub clock_minute: Option<i64>,
}

/// Where the files go and how many bytes of them to keep, in place of the build's own.
static OVERRIDE: std::sync::Mutex<Option<Option<(PathBuf, usize)>>> = std::sync::Mutex::new(None);

/// A folder and a budget for the duration of a test, or `Some(None)` for no disk at all. `None`
/// hands it back to the build.
pub fn set_override(setting: Option<Option<(PathBuf, usize)>>) {
    *OVERRIDE.lock().unwrap_or_else(|e| e.into_inner()) = setting;
}

/// Where the files go, and how many bytes of them to keep - or nothing, when nothing is kept.
///
/// `OVERSEER_DISK_CACHE` names the folder, and set empty turns this off. Otherwise a folder of its
/// own under the system's temporary one - never beside the documents, which are synced by commits.
/// `OVERSEER_DISK_CACHE_MB` is the budget, 512 unless it says otherwise. Off for the tests unless
/// one asks, as the memory cache's budget is their own: several check what is held in memory, and a
/// document coming back from a file another test left would answer for it.
fn setting() -> Option<(PathBuf, usize)> {
    if let Some(asked) = OVERRIDE.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        return asked;
    }
    static SETTING: std::sync::OnceLock<Option<(PathBuf, usize)>> = std::sync::OnceLock::new();
    SETTING
        .get_or_init(|| {
            let folder = match std::env::var("OVERSEER_DISK_CACHE") {
                Ok(named) if named.trim().is_empty() => return None,
                Ok(named) => PathBuf::from(named.trim()),
                Err(_) if cfg!(feature = "server") || cfg!(feature = "desktop") => {
                    std::env::temp_dir().join("overseer-cache")
                }
                Err(_) => return None,
            };
            let mb = std::env::var("OVERSEER_DISK_CACHE_MB")
                .ok()
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(512);
            (mb > 0).then(|| (folder, mb * 1024 * 1024))
        })
        .clone()
}

/// Which build this is: the running program's size and when it was written.
///
/// Rather than a version number someone is meant to raise when the way documents are worked out
/// changes - the one way a stored document could go on being handed out wrong without anybody
/// noticing is a change nobody thought to raise it for. Every build is a new program, and a new
/// program finds no files to read.
fn build() -> Option<&'static str> {
    static BUILD: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    BUILD
        .get_or_init(|| {
            let program = std::env::current_exe().ok()?;
            let about = std::fs::metadata(program).ok()?;
            let written = about
                .modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_nanos();
            Some(format!("{}:{}:{}", env!("CARGO_PKG_VERSION"), about.len(), written))
        })
        .as_deref()
}

/// The file a document worked out from this text in this folder goes in.
fn file_for(folder: &Path, build: &str, base: &Option<PathBuf>, text: &str) -> PathBuf {
    use std::hash::Hasher;
    let mut hasher = twox_hash::XxHash64::with_seed(0);
    hasher.write(build.as_bytes());
    hasher.write(&[0]);
    if let Some(base) = base {
        hasher.write(base.to_string_lossy().as_bytes());
    }
    hasher.write(&[0]);
    hasher.write(text.as_bytes());
    folder.join(format!("{:016x}.json", hasher.finish()))
}

#[derive(Serialize, Deserialize)]
struct Stored {
    build: String,
    base: Option<PathBuf>,
    text: String,
    mounts: Vec<(String, String)>,
    clock_minute: Option<i64>,
    /// Every text a snapshot is a view into, once each - the document's own and its mounts'.
    sources: Vec<String>,
    snapshots: Vec<StoredSnapshot>,
    /// What the registry held for each parse the nodes came from - see `SourceRegistry::restore`.
    scopes: Vec<StoredScope>,
    /// Each node's own snapshot, in the order a walk down the tree meets the nodes.
    held: Vec<Option<usize>>,
    nodes: Vec<OverseerNode>,
    reads: HashMap<String, HashSet<String>>,
}

#[derive(Serialize, Deserialize)]
struct StoredScope {
    scope: u64,
    trailing: String,
    snapshots: Vec<(u64, usize)>,
}

/// A `NodeSourceSnapshot` with its text named by where it lies in `Stored::sources` - the
/// snapshots of a document are views into one copy of it, and are stored that way.
#[derive(Serialize, Deserialize)]
struct StoredSnapshot {
    span: (usize, usize),
    leading_span: Option<(usize, usize)>,
    header_span: (usize, usize),
    body_span: Option<(usize, usize)>,
    trailing_span: Option<(usize, usize)>,
    full_text: Option<(usize, usize, usize)>,
    leading_trivia: String,
    header: NodeHeaderSnapshot,
    body: NodeBodySnapshot,
    trailing_trivia: String,
    indent_unit: Option<String>,
    newline: Option<String>,
    fingerprint: u64,
    origin: SnapshotOrigin,
}

/// Snapshots and the texts they view, each written once however many nodes share it.
#[derive(Default)]
struct Snapshots {
    sources: Vec<String>,
    source_at: HashMap<usize, usize>,
    stored: Vec<StoredSnapshot>,
    stored_at: HashMap<usize, usize>,
    /// Kept alive while being written, so an address taken of one is never reused by another.
    kept: Vec<Arc<NodeSourceSnapshot>>,
}

impl Snapshots {
    fn add(&mut self, snapshot: &Arc<NodeSourceSnapshot>) -> usize {
        let key = Arc::as_ptr(snapshot) as usize;
        if let Some(&at) = self.stored_at.get(&key) {
            return at;
        }
        let full_text = snapshot.full_text.parts().map(|(source, start, end)| {
            let key = Arc::as_ptr(source) as *const u8 as usize;
            let next = self.sources.len();
            let at = *self.source_at.entry(key).or_insert(next);
            if at == next {
                self.sources.push(source.to_string());
            }
            (at, start, end)
        });
        let s = snapshot.as_ref();
        self.stored.push(StoredSnapshot {
            span: s.span,
            leading_span: s.leading_span,
            header_span: s.header_span,
            body_span: s.body_span,
            trailing_span: s.trailing_span,
            full_text,
            leading_trivia: s.leading_trivia.clone(),
            header: s.header.clone(),
            body: s.body.clone(),
            trailing_trivia: s.trailing_trivia.clone(),
            indent_unit: s.indent_unit.clone(),
            newline: s.newline.clone(),
            fingerprint: s.fingerprint,
            origin: s.origin.clone(),
        });
        self.kept.push(snapshot.clone());
        self.stored_at.insert(key, self.stored.len() - 1);
        self.stored.len() - 1
    }
}

fn stored(text: String, base: Option<PathBuf>, build: &str, held: Held) -> Stored {
    let mut snapshots = Snapshots::default();
    let mut on_nodes = Vec::new();
    // What the registry holds under each id the tree carries: the writer looks there for a node
    // that has lost its own, and for the comments after a document's last node.
    let mut scopes: HashMap<u64, StoredScope> = HashMap::new();
    fn walk(
        nodes: &[OverseerNode],
        snapshots: &mut Snapshots,
        on_nodes: &mut Vec<Option<usize>>,
        scopes: &mut HashMap<u64, StoredScope>,
    ) {
        for node in nodes {
            on_nodes.push(node.source_snapshot.as_ref().map(|s| snapshots.add(s)));
            if let Some(id) = node.source_id.as_deref() {
                if let Some((scope, number)) = SourceRegistry::split_id(id) {
                    let entry = scopes.entry(scope).or_insert_with(|| StoredScope {
                        scope,
                        trailing: SourceRegistry::document_trailing_for_id(id),
                        snapshots: Vec::new(),
                    });
                    if let Some(registered) =
                        SourceRegistry::get(id).or_else(|| node.source_snapshot.clone())
                    {
                        entry.snapshots.push((number, snapshots.add(&registered)));
                    }
                }
            }
            walk(&node.children, snapshots, on_nodes, scopes);
        }
    }
    walk(&held.nodes, &mut snapshots, &mut on_nodes, &mut scopes);
    for scope in scopes.values_mut() {
        scope.snapshots.sort_unstable();
        scope.snapshots.dedup();
    }
    let mut scopes: Vec<StoredScope> = scopes.into_values().collect();
    scopes.sort_unstable_by_key(|s| s.scope);
    Stored {
        build: build.to_string(),
        base,
        text,
        mounts: held.mounts,
        clock_minute: held.clock_minute,
        sources: snapshots.sources,
        snapshots: snapshots.stored,
        scopes,
        held: on_nodes,
        nodes: held.nodes,
        reads: held.graph.into_reads(),
    }
}

/// The document a file held, its snapshots back on its nodes and in the registry.
fn unpacked(stored: Stored) -> Option<Held> {
    let sources: Vec<Arc<str>> = stored.sources.into_iter().map(Arc::from).collect();
    let mut snapshots = Vec::with_capacity(stored.snapshots.len());
    for s in stored.snapshots {
        let full_text = match s.full_text {
            Some((at, start, end)) => SharedText::view(sources.get(at)?.clone(), start, end),
            None => SharedText::default(),
        };
        snapshots.push(Arc::new(NodeSourceSnapshot {
            span: s.span,
            leading_span: s.leading_span,
            header_span: s.header_span,
            body_span: s.body_span,
            trailing_span: s.trailing_span,
            full_text,
            leading_trivia: s.leading_trivia,
            header: s.header,
            body: s.body,
            trailing_trivia: s.trailing_trivia,
            indent_unit: s.indent_unit,
            newline: s.newline,
            fingerprint: s.fingerprint,
            origin: s.origin,
        }));
    }
    let mut renumbered: HashMap<u64, u64> = HashMap::new();
    for scope in stored.scopes {
        let mut held = Vec::with_capacity(scope.snapshots.len());
        for (number, at) in scope.snapshots {
            held.push((number, snapshots.get(at)?.clone()));
        }
        renumbered.insert(scope.scope, SourceRegistry::restore(scope.trailing, held));
    }
    fn walk(
        nodes: &mut [OverseerNode],
        on_nodes: &mut std::slice::Iter<Option<usize>>,
        snapshots: &[Arc<NodeSourceSnapshot>],
        renumbered: &HashMap<u64, u64>,
    ) -> Option<()> {
        for node in nodes {
            if let Some(at) = on_nodes.next()? {
                node.source_snapshot = Some(snapshots.get(*at)?.clone());
            }
            if let Some(id) = node.source_id.as_deref() {
                if let Some((scope, number)) = SourceRegistry::split_id(id) {
                    let now = renumbered.get(&scope)?;
                    node.source_id = Some(SourceRegistry::join_id(*now, number));
                }
            }
            walk(&mut node.children, on_nodes, snapshots, renumbered)?;
        }
        Some(())
    }
    let mut nodes = stored.nodes;
    let mut on_nodes = stored.held.iter();
    walk(&mut nodes, &mut on_nodes, &snapshots, &renumbered)?;
    if on_nodes.next().is_some() {
        return None;
    }
    Some(Held {
        nodes,
        graph: Graph::from_stored(stored.reads),
        mounts: stored.mounts,
        clock_minute: stored.clock_minute,
    })
}

/// How many files are being written, so a test can wait for them - see `settled`.
static WRITING: std::sync::Mutex<usize> = std::sync::Mutex::new(0);
static WRITTEN: std::sync::Condvar = std::sync::Condvar::new();

/// Write a document the cache is letting go of, without making whoever let it go wait.
pub fn store(text: String, base: Option<PathBuf>, held: Held) {
    let Some((folder, budget)) = setting() else { return };
    let Some(build) = build() else { return };
    *WRITING.lock().unwrap_or_else(|e| e.into_inner()) += 1;
    std::thread::spawn(move || {
        // A file that cannot be written is a document worked out again later, and nothing more.
        let _ = write(&folder, budget, build, text, base, held);
        let mut writing = WRITING.lock().unwrap_or_else(|e| e.into_inner());
        *writing -= 1;
        WRITTEN.notify_all();
    });
}

fn write(
    folder: &Path,
    budget: usize,
    build: &str,
    text: String,
    base: Option<PathBuf>,
    held: Held,
) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    let file = file_for(folder, build, &base, &text);
    let bytes = serde_json::to_vec(&stored(text, base, build, held)).map_err(std::io::Error::other)?;
    if bytes.len() > budget {
        return Ok(());
    }
    // Written beside it and moved into place, so a read never finds half a file.
    static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let part = file.with_extension(format!(
        "{}-{}.part",
        std::process::id(),
        SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::write(&part, &bytes)?;
    if let Err(e) = std::fs::rename(&part, &file) {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    trim(folder, budget);
    Ok(())
}

/// Bring the folder back within budget, the files written longest ago going first.
fn trim(folder: &Path, budget: usize) {
    let Ok(listing) = std::fs::read_dir(folder) else { return };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = listing
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            let about = e.metadata().ok()?;
            Some((about.modified().ok()?, about.len(), e.path()))
        })
        .collect();
    files.sort_unstable();
    let mut total: u64 = files.iter().map(|(_, len, _)| len).sum();
    for (_, len, path) in files {
        if total <= budget as u64 {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total -= len;
        }
    }
}

/// The document worked out from this text in this folder, if it was written down - taken, so the
/// file goes with it. Nothing when there is none, or the one there will not do: from another
/// build, damaged, or for another text that happened to share its name.
pub fn take(text: &str, base: &Option<PathBuf>) -> Option<Held> {
    let (folder, _) = setting()?;
    let build = build()?;
    let file = file_for(&folder, build, base, text);
    let bytes = std::fs::read(&file).ok()?;
    let _ = std::fs::remove_file(&file);
    let stored: Stored = serde_json::from_slice(&bytes).ok()?;
    if stored.build != build || &stored.base != base || stored.text != text {
        return None;
    }
    unpacked(stored)
}

/// The file the document worked out from this text in this folder is kept in, if one is. For
/// tests, which have no other way to tell which file is which.
pub fn where_kept(text: &str, base: &Option<PathBuf>) -> Option<PathBuf> {
    let (folder, _) = setting()?;
    let file = file_for(&folder, build()?, base, text);
    file.exists().then_some(file)
}

/// Wait until every file being written has been. For tests, which look for a file straight after
/// the cache let its document go.
pub fn settled() {
    let mut writing = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    while *writing > 0 {
        writing = WRITTEN.wait(writing).unwrap_or_else(|e| e.into_inner());
    }
}
