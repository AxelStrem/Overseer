use std::collections::HashMap;
use std::sync::{atomic::{AtomicU64, Ordering}, LazyLock, RwLock};
#[cfg(test)]
use parking_lot::ReentrantMutex;

use crate::types::NodeSourceSnapshot;

static SNAPSHOT_REGISTRY: LazyLock<RwLock<HashMap<String, NodeSourceSnapshot>>> = LazyLock::new(|| {
    RwLock::new(HashMap::new())
});

static REGISTRY_COUNTER: AtomicU64 = AtomicU64::new(1);

#[cfg(test)]
pub static REGISTRY_TEST_MUTEX: LazyLock<ReentrantMutex<()>> = LazyLock::new(|| ReentrantMutex::new(()));

/// Global registry that retains source snapshots for nodes across parse/serialize cycles.
///
/// Nodes only carry snapshots in-process (they are skipped during serde), so we register
/// the snapshot and keep an opaque identifier that can travel over the wire. Later stages
/// (e.g., serializer) can fetch the snapshot by ID to reconstruct original trivia.
pub struct SourceRegistry;

#[cfg_attr(not(test), allow(dead_code))]
impl SourceRegistry {
    /// Clear all registered snapshots and reset the identifier counter. A fresh parse should
    /// call this before registering new entries so stale snapshots do not leak between documents.
    pub fn reset() {
        let mut guard = SNAPSHOT_REGISTRY.write().expect("snapshot registry poisoned");
        guard.clear();
        REGISTRY_COUNTER.store(1, Ordering::Relaxed);
    }

    /// Register the provided snapshot and return an opaque identifier that can be stored on
    /// the node. Callers should ensure `reset` was invoked for the current parse session to
    /// avoid leaking old snapshots.
    pub fn register(snapshot: &NodeSourceSnapshot) -> String {
        let id = REGISTRY_COUNTER.fetch_add(1, Ordering::Relaxed);
        let key = format!("ns{}", id);
        let mut guard = SNAPSHOT_REGISTRY.write().expect("snapshot registry poisoned");
        guard.insert(key.clone(), snapshot.clone());
        key
    }

    /// Retrieve a previously registered snapshot by identifier.
    pub fn get(id: &str) -> Option<NodeSourceSnapshot> {
        let guard = SNAPSHOT_REGISTRY.read().expect("snapshot registry poisoned");
        guard.get(id).cloned()
    }

    /// Returns the number of snapshots currently cached. Mostly useful for tests.
    pub fn len() -> usize {
        let guard = SNAPSHOT_REGISTRY.read().expect("snapshot registry poisoned");
        guard.len()
    }
}
