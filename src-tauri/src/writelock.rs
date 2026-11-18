//! Writes to one document, one after another.
//!
//! An addressed write reads the file, works the change out and writes the whole text back. Two
//! of those at once on one document both started from the same text, and the second to finish
//! wrote over the first: it answered success and the first write was gone. A page's save has the
//! same gap between asking whether the file moved on and writing. `lock` is held from the read
//! to the end of the write, by everything that writes a document, so each write starts from the
//! text the one before it left.
//!
//! The lock is the process's, not a `DocumentRoot`'s: two roots over one folder still write one
//! file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

/// One document's: whether someone is writing it, and where the next writer waits.
#[derive(Default)]
struct Slot {
    busy: Mutex<bool>,
    free: Condvar,
}

type Slots = Mutex<HashMap<PathBuf, Arc<Slot>>>;

fn slots() -> &'static Slots {
    static SLOTS: OnceLock<Slots> = OnceLock::new();
    SLOTS.get_or_init(Default::default)
}

/// Held while a document is read, changed and written; dropping it lets the next writer in.
pub struct Held(Arc<Slot>);

impl Drop for Held {
    fn drop(&mut self) {
        *self.0.busy.lock().unwrap_or_else(|e| e.into_inner()) = false;
        self.0.free.notify_one();
    }
}

/// Wait until nothing else is writing the document at `path`, and hold it.
///
/// Not reentrant: whoever holds it must not ask for the same document again.
pub fn lock(path: &Path) -> Held {
    // Two names for one file share a lock; a file not yet there is known by the name given.
    let key = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let slot = {
        let mut all = slots().lock().unwrap_or_else(|e| e.into_inner());
        // A document nobody is writing or waiting on has nothing to keep, so the map stays the
        // size of what is being written rather than of everything ever written.
        all.retain(|_, slot| Arc::strong_count(slot) > 1);
        all.entry(key).or_default().clone()
    };
    let mut busy = slot.busy.lock().unwrap_or_else(|e| e.into_inner());
    while *busy {
        busy = slot.free.wait(busy).unwrap_or_else(|e| e.into_inner());
    }
    *busy = true;
    drop(busy);
    Held(slot)
}

/// Replace a file with `bytes`, so a reader never sees half of it.
///
/// Written beside it and renamed over it. The name written to is this write's alone: two writers
/// sharing one renamed it out from under each other, and the second rename found nothing there.
pub fn replace(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    static WRITES: AtomicU64 = AtomicU64::new(0);
    let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let temporary = path.with_file_name(format!(
        "{}.{}-{}.writing",
        file,
        std::process::id(),
        WRITES.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(&temporary, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temporary);
    })
}
