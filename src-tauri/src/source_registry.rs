#[cfg(test)]
use parking_lot::ReentrantMutex;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    LazyLock, RwLock,
};

use crate::types::NodeSourceSnapshot;

/// Everything retained for one parse of one document.
#[derive(Default)]
struct ScopeData {
    snapshots: HashMap<u64, NodeSourceSnapshot>,
    /// Trivia after the last top-level node - comments at the end of that file.
    trailing: String,
    /// Node numbering is per scope, so re-parsing identical text reproduces identical ids.
    node_counter: u64,
}

/// Scope reused for a given source text, keyed by its hash.
///
/// Without this, every parse takes a fresh scope, and a mounted document is re-parsed on
/// every preload - which is once per edit. Scopes then churn fast enough to evict the host
/// document's own snapshots while it is still open, and the next save reformats every node
/// whose snapshot has gone: braces, comments, indentation and parameter order all lost.
/// Identical text yields identical snapshots, so it can share one scope no matter how often
/// it is parsed.
static SCOPE_BY_TEXT: LazyLock<RwLock<HashMap<u64, u64>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

fn text_hash(source: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

static SCOPES: LazyLock<RwLock<HashMap<u64, ScopeData>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));
/// Creation order, used to evict the oldest scopes once too many are retained.
static SCOPE_ORDER: LazyLock<RwLock<VecDeque<u64>>> = LazyLock::new(|| RwLock::new(VecDeque::new()));
/// Scopes with a parse still in progress somewhere; never evicted.
static ACTIVE_SCOPES: LazyLock<RwLock<HashSet<u64>>> =
    LazyLock::new(|| RwLock::new(HashSet::new()));

static SCOPE_COUNTER: AtomicU64 = AtomicU64::new(1);
static NODE_COUNTER: AtomicU64 = AtomicU64::new(1);

thread_local! {
    /// Scope that `register` and `append_document_trailing` write into. A stack, so a parse
    /// triggered while another is in progress restores the outer one when it finishes.
    static CURRENT_SCOPE: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

/// How many parsed documents to keep snapshots for. A document is re-parsed on every edit,
/// so this only needs to cover the documents that are live at once - a host plus whatever it
/// mounts - with room to spare.
const MAX_RETAINED_SCOPES: usize = 32;

#[cfg(test)]
pub static REGISTRY_TEST_MUTEX: LazyLock<ReentrantMutex<()>> =
    LazyLock::new(|| ReentrantMutex::new(()));

/// Registry of source snapshots, partitioned by document.
///
/// Nodes only carry snapshots in-process (they are skipped during serde), so a snapshot is
/// registered here and the node keeps an opaque id that can travel over the wire. Later
/// stages - the serializer above all - fetch the snapshot by id to replay original trivia.
///
/// Ids are scoped per parse. They have to be: a document that mounts another one has both
/// live simultaneously, and when the registry was a single flat map keyed by a global
/// counter, parsing the mounted file re-issued the same ids over the host's entries. Saving
/// the host then replayed the *mounted* document's text - its comments, its indentation, its
/// nodes - over the host's own. Scoping also makes concurrent parses independent, which
/// matters for tests running in one process.
pub struct SourceRegistry;

/// Marks a parse as in progress. Dropping it restores the previously current scope; the
/// scope's snapshots outlive it, since serialization happens after the parse has finished.
pub struct DocumentScope {
    id: u64,
}

impl DocumentScope {
    #[allow(dead_code)]
    pub fn id(&self) -> u64 {
        self.id
    }
}

impl Drop for DocumentScope {
    fn drop(&mut self) {
        CURRENT_SCOPE.with(|s| {
            let mut stack = s.borrow_mut();
            if let Some(pos) = stack.iter().rposition(|x| *x == self.id) {
                stack.remove(pos);
            }
        });
        if let Ok(mut active) = ACTIVE_SCOPES.write() {
            active.remove(&self.id);
        }
    }
}

fn format_id(scope: u64, node: u64) -> String {
    format!("s{}n{}", scope, node)
}

fn parse_id(id: &str) -> Option<(u64, u64)> {
    let body = id.strip_prefix('s')?;
    let (scope, node) = body.split_once('n')?;
    Some((scope.parse().ok()?, node.parse().ok()?))
}

#[cfg_attr(not(test), allow(dead_code))]
impl SourceRegistry {
    /// Begin a parse of `source`. Snapshots registered until the returned guard drops belong
    /// to this document and cannot be confused with any other's.
    ///
    /// Parsing the same text again reuses its scope rather than taking a new one, so a
    /// document that is re-read on every edit - a mounted catalog, say - does not push other
    /// documents' snapshots out of the registry.
    pub fn begin_document(source: &str) -> DocumentScope {
        let hash = text_hash(source);
        let existing = SCOPE_BY_TEXT
            .read()
            .ok()
            .and_then(|m| m.get(&hash).copied())
            .filter(|id| SCOPES.read().map(|s| s.contains_key(id)).unwrap_or(false));

        let id = match existing {
            Some(id) => {
                // Same text, so the snapshots about to be registered are the same ones. Reset
                // the scope and reuse its number, which also reproduces the same node ids.
                if let Ok(mut scopes) = SCOPES.write() {
                    if let Some(data) = scopes.get_mut(&id) {
                        data.snapshots.clear();
                        data.trailing.clear();
                        data.node_counter = 0;
                    }
                }
                if let Ok(mut order) = SCOPE_ORDER.write() {
                    if let Some(pos) = order.iter().position(|x| *x == id) {
                        order.remove(pos);
                    }
                    order.push_back(id);
                }
                id
            }
            None => {
                let id = SCOPE_COUNTER.fetch_add(1, Ordering::Relaxed);
                if let Ok(mut scopes) = SCOPES.write() {
                    scopes.insert(id, ScopeData::default());
                }
                if let Ok(mut order) = SCOPE_ORDER.write() {
                    order.push_back(id);
                }
                if let Ok(mut map) = SCOPE_BY_TEXT.write() {
                    map.insert(hash, id);
                }
                id
            }
        };

        if let Ok(mut active) = ACTIVE_SCOPES.write() {
            active.insert(id);
        }
        CURRENT_SCOPE.with(|s| s.borrow_mut().push(id));
        Self::evict_old_scopes();
        DocumentScope { id }
    }

    fn evict_old_scopes() {
        let Ok(mut order) = SCOPE_ORDER.write() else {
            return;
        };
        let Ok(active) = ACTIVE_SCOPES.read() else {
            return;
        };
        let Ok(mut scopes) = SCOPES.write() else {
            return;
        };
        while order.len() > MAX_RETAINED_SCOPES {
            // Find the oldest scope that no parse is currently using.
            let Some(pos) = order.iter().position(|id| !active.contains(id)) else {
                break;
            };
            if let Some(id) = order.remove(pos) {
                scopes.remove(&id);
                if let Ok(mut by_text) = SCOPE_BY_TEXT.write() {
                    by_text.retain(|_, scope| *scope != id);
                }
            }
        }
    }

    fn current_scope() -> Option<u64> {
        CURRENT_SCOPE.with(|s| s.borrow().last().copied())
    }

    /// Register a snapshot in the current document's scope and return its id.
    ///
    /// Registering outside any parse still works - the snapshot lands in a scope of its own -
    /// so a caller that constructs nodes directly is not silently dropped.
    pub fn register(snapshot: &NodeSourceSnapshot) -> String {
        let scope = match Self::current_scope() {
            Some(s) => s,
            None => {
                let guard = Self::begin_document("");
                let id = guard.id;
                std::mem::forget(guard);
                if let Ok(mut active) = ACTIVE_SCOPES.write() {
                    active.remove(&id);
                }
                CURRENT_SCOPE.with(|s| {
                    let mut stack = s.borrow_mut();
                    if let Some(pos) = stack.iter().rposition(|x| *x == id) {
                        stack.remove(pos);
                    }
                });
                id
            }
        };
        let mut key = String::new();
        if let Ok(mut scopes) = SCOPES.write() {
            if let Some(data) = scopes.get_mut(&scope) {
                data.node_counter += 1;
                let node = data.node_counter;
                data.snapshots.insert(node, snapshot.clone());
                key = format_id(scope, node);
            }
        }
        if key.is_empty() {
            key = format_id(scope, NODE_COUNTER.fetch_add(1, Ordering::Relaxed));
        }
        key
    }

    /// Retrieve a previously registered snapshot by id.
    pub fn get(id: &str) -> Option<NodeSourceSnapshot> {
        let (scope, node) = parse_id(id)?;
        let scopes = SCOPES.read().ok()?;
        scopes.get(&scope)?.snapshots.get(&node).cloned()
    }

    /// Number of snapshots retained across all scopes. Mostly useful for tests.
    pub fn len() -> usize {
        SCOPES
            .read()
            .map(|s| s.values().map(|d| d.snapshots.len()).sum())
            .unwrap_or(0)
    }

    /// Number of snapshots registered by the parse that produced `id`. This is what a test
    /// asking "did this document register everything?" actually means - a global count says
    /// nothing now that several documents are retained at once.
    pub fn len_for_document(id: &str) -> usize {
        let Some((scope, _)) = parse_id(id) else {
            return 0;
        };
        SCOPES
            .read()
            .ok()
            .and_then(|s| s.get(&scope).map(|d| d.snapshots.len()))
            .unwrap_or(0)
    }

    /// Drop every scope. Only for tests that want a clean slate; normal operation relies on
    /// scoping plus eviction rather than wholesale clearing, because clearing is exactly what
    /// used to destroy a host document's snapshots when a mounted file was parsed.
    pub fn reset() {
        if let Ok(mut scopes) = SCOPES.write() {
            scopes.clear();
        }
        if let Ok(mut order) = SCOPE_ORDER.write() {
            order.clear();
        }
        if let Ok(mut active) = ACTIVE_SCOPES.write() {
            active.clear();
        }
        if let Ok(mut by_text) = SCOPE_BY_TEXT.write() {
            by_text.clear();
        }
        CURRENT_SCOPE.with(|s| s.borrow_mut().clear());
    }

    pub fn append_document_trailing(chunk: &str) {
        if chunk.is_empty() {
            return;
        }
        if let (Some(scope), Ok(mut scopes)) = (Self::current_scope(), SCOPES.write()) {
            if let Some(data) = scopes.get_mut(&scope) {
                data.trailing.push_str(chunk);
            }
        }
    }

    /// Trailing trivia of the document that owns `id`.
    ///
    /// Non-destructive: serializing a document twice must produce the same text both times,
    /// and the trailing belongs to the scope for as long as the scope is retained.
    pub fn document_trailing_for_id(id: &str) -> String {
        let Some((scope, _)) = parse_id(id) else {
            return String::new();
        };
        SCOPES
            .read()
            .ok()
            .and_then(|s| s.get(&scope).map(|d| d.trailing.clone()))
            .unwrap_or_default()
    }
}
