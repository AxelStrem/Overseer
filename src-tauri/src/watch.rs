//! Who is watching which document, and telling them when it moves.
//!
//! A page shows a document until it is reloaded, and the bot writes to documents somebody has
//! open most of the day. So a page can listen for a document, and is told its file's version each
//! time it changes - only the version: what a page is shown depends on who is looking, and each
//! page asks for its own catch-up, described against what it holds, the way every edit is
//! answered.
//!
//! A write made here says so at once - see `wrote`. One made from outside - the backup syncing
//! the folder, a hand edit - passes through none of this, so while anyone listens the file is
//! also looked at every couple of seconds. Either way the version is worked out from what the
//! file says, so two writes in quick succession announce the last, and a write that changed
//! nothing announces nothing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime};
use tokio::sync::broadcast;

/// How often a watched file is looked at for a write made from outside.
pub const LOOKED_AT_EVERY: Duration = Duration::from_secs(2);

struct Watched {
    to: broadcast::Sender<String>,
    seen: Mutex<Seen>,
}

#[derive(Default)]
struct Seen {
    modified: Option<SystemTime>,
    len: u64,
    version: String,
}

fn watched() -> &'static Mutex<HashMap<PathBuf, Arc<Watched>>> {
    static WATCHED: OnceLock<Mutex<HashMap<PathBuf, Arc<Watched>>>> = OnceLock::new();
    WATCHED.get_or_init(Default::default)
}

/// The same file named the same way, whichever way the writer named it.
fn key(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Listen for a document: its version now, and each new one as it comes.
pub fn listen(path: &Path) -> (String, broadcast::Receiver<String>) {
    // Subscribed under the lock, or the next look could forget the entry as nobody's before
    // this listener is counted.
    let (watched_now, listening) = {
        let mut all = watched().lock().unwrap_or_else(|e| e.into_inner());
        let one = all
            .entry(key(path))
            .or_insert_with(|| {
                Arc::new(Watched { to: broadcast::channel(16).0, seen: Mutex::new(Seen::default()) })
            })
            .clone();
        let listening = one.to.subscribe();
        (one, listening)
    };
    look_again(path, &watched_now);
    let version = watched_now.seen.lock().unwrap_or_else(|e| e.into_inner()).version.clone();
    keep_looking();
    (version, listening)
}

/// Something here wrote this file: tell whoever listens now, rather than at the next look.
pub fn wrote(path: &Path) {
    let one = {
        let all = watched().lock().unwrap_or_else(|e| e.into_inner());
        if all.is_empty() {
            return;
        }
        all.get(&key(path)).cloned()
    };
    if let Some(one) = one {
        look_again(path, &one);
    }
}

/// Read the file again if it moved, and announce its version if that is new.
fn look_again(path: &Path, watched: &Watched) {
    let mut seen = watched.seen.lock().unwrap_or_else(|e| e.into_inner());
    let meta = std::fs::metadata(path).ok();
    let modified = meta.as_ref().and_then(|m| m.modified().ok());
    let len = meta.as_ref().map_or(0, |m| m.len());
    if !seen.version.is_empty() && seen.modified == modified && seen.len == len {
        return;
    }
    seen.modified = modified;
    seen.len = len;
    let Ok(text) = std::fs::read_to_string(path) else { return };
    let version = crate::app_api::version_of(&text);
    if version == seen.version {
        return;
    }
    let first = seen.version.is_empty();
    seen.version = version.clone();
    // The first look only learns what the file says; whoever asked is told that in reply.
    if !first {
        let _ = watched.to.send(version);
    }
}

/// Look at every watched file now and then, for as long as the server runs, and forget those
/// nobody listens to any more.
fn keep_looking() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        let _ = std::thread::Builder::new().name("overseer-watch".into()).spawn(|| loop {
            std::thread::sleep(LOOKED_AT_EVERY);
            let now: Vec<(PathBuf, Arc<Watched>)> = {
                let mut all = watched().lock().unwrap_or_else(|e| e.into_inner());
                all.retain(|_, one| one.to.receiver_count() > 0);
                all.iter().map(|(path, one)| (path.clone(), one.clone())).collect()
            };
            for (path, one) in now {
                look_again(&path, &one);
            }
        });
    });
}
