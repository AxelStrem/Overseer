//! A cache that keeps what it is given only while there is room for it.
//!
//! The document cache is bounded by `OVERSEER_CACHE_MB`, but the caches beside it were not: the
//! parsed parts of every history read so far, every mounted file worked out, every base parsed -
//! each kept for as long as the server ran, and growing as the documents did. On a host that
//! bills by memory held that is a bill which only ever rises. So each of them is one of these:
//! bounded by bytes, letting go of what was asked for least lately once it is over its limit.
//!
//! What is let go is only read again from its file when next asked for, as after a restart.

use std::collections::HashMap;
use std::hash::Hash;

/// How many bytes each of the caches beside the document cache may keep.
///
/// A quarter of the document cache's budget each, so the server's memory stays a multiple of the
/// one number a host sets. But never under a floor: a tracker's newest parts and the catalogue a
/// food tracker mounts are read again on every write to them, and a budget of `0` - the document
/// cache switched off - should not have every write parse them again.
pub fn limit() -> usize {
    let asked = LIMIT_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed);
    if asked >= 0 {
        return asked as usize;
    }
    const FLOOR: usize = 16 * 1024 * 1024;
    (crate::document_cache::budget_bytes() / 4).max(FLOOR)
}

/// A limit for the duration of a test, in place of the one worked out from the budget - the same
/// shape as `document_cache::set_budget_override`. `None` hands it back.
static LIMIT_OVERRIDE: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(-1);

pub fn set_limit_override(bytes: Option<usize>) {
    LIMIT_OVERRIDE.store(bytes.map_or(-1, |b| b as isize), std::sync::atomic::Ordering::Relaxed);
}

pub struct Bounded<K, V> {
    held: HashMap<K, Held<V>>,
    bytes: usize,
    tick: u64,
}

struct Held<V> {
    value: V,
    bytes: usize,
    used: u64,
}

impl<K: Eq + Hash + Clone, V> Bounded<K, V> {
    pub fn new() -> Self {
        Bounded { held: HashMap::new(), bytes: 0, tick: 0 }
    }

    /// What is held under this key, counted as a use.
    pub fn get(&mut self, key: &K) -> Option<&V> {
        self.tick += 1;
        let tick = self.tick;
        self.held.get_mut(key).map(|held| {
            held.used = tick;
            &held.value
        })
    }

    /// What is held under this key, without counting it as a use.
    pub fn peek(&self, key: &K) -> Option<&V> {
        self.held.get(key).map(|held| &held.value)
    }

    /// Keep `value`, which occupies about `bytes`, and let go of the least lately used until what
    /// is held is within `limit`. What was just put in stays even when it alone is over the limit:
    /// whoever put it in is about to ask for it again.
    pub fn insert(&mut self, key: K, value: V, bytes: usize, limit: usize) {
        self.tick += 1;
        if let Some(was) = self.held.insert(key.clone(), Held { value, bytes, used: self.tick }) {
            self.bytes -= was.bytes;
        }
        self.bytes += bytes;
        while self.bytes > limit && self.held.len() > 1 {
            let Some(stalest) = self
                .held
                .iter()
                .filter(|(k, _)| **k != key)
                .min_by_key(|(_, held)| held.used)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            if let Some(gone) = self.held.remove(&stalest) {
                self.bytes -= gone.bytes;
            }
        }
    }

    pub fn len(&self) -> usize {
        self.held.len()
    }

    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    /// About how many bytes are held, as they were counted when put in.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    pub fn clear(&mut self) {
        self.held.clear();
        self.bytes = 0;
    }
}

impl<K: Eq + Hash + Clone, V> Default for Bounded<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_least_lately_used_goes_first_and_the_newest_stays() {
        let mut held: Bounded<&str, u32> = Bounded::new();
        held.insert("a", 1, 10, 25);
        held.insert("b", 2, 10, 25);
        assert_eq!(held.get(&"a"), Some(&1), "a is used, so b is now the stalest");
        held.insert("c", 3, 10, 25);
        assert_eq!(held.peek(&"b"), None);
        assert_eq!((held.len(), held.bytes()), (2, 20));
        held.insert("d", 4, 100, 25);
        assert_eq!((held.len(), held.bytes()), (1, 100), "one alone over the limit is still kept");
        held.insert("d", 5, 5, 25);
        assert_eq!((held.peek(&"d"), held.bytes()), (Some(&5), 5), "put in again, it is counted once");
    }
}
