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
//!
//! A document that mounts another shows that file's content too, and the bot writes to those as
//! well - a food added to the catalogue the food tracker mounts. So what a document was last
//! worked out from is reported here each time it is (`reads`), those files are looked at with it,
//! and what they say is announced beside the version as `read_from`, named as an answer names it -
//! see `app_api::reads_version`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime};
use tokio::sync::broadcast;

/// How often a watched file is looked at for a write made from outside.
pub const LOOKED_AT_EVERY: Duration = Duration::from_secs(2);

/// What a page is told: the version of the document's file, and what the files its mounts read
/// say - empty for a document that mounts nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Heard {
    pub version: String,
    pub read_from: String,
}

struct Watched {
    to: broadcast::Sender<Heard>,
    seen: Mutex<Seen>,
}

#[derive(Default)]
struct Seen {
    modified: Option<SystemTime>,
    len: u64,
    heard: Heard,
}

/// The files each document's mounts were read from when it was last worked out, and how each
/// stood then - see `reads`.
fn reading() -> &'static Mutex<HashMap<PathBuf, Vec<(String, String)>>> {
    static READING: OnceLock<Mutex<HashMap<PathBuf, Vec<(String, String)>>>> = OnceLock::new();
    READING.get_or_init(Default::default)
}

/// The document at `path` was just worked out from these mounted files, as they stood - see
/// `actions::mounts_held`. Kept whether anyone watches it or not, since a page works its document
/// out before it starts listening.
pub fn reads(path: &Path, files: &[(String, String)]) {
    let mut all = reading().lock().unwrap_or_else(|e| e.into_inner());
    if files.is_empty() {
        if !all.is_empty() {
            all.remove(&key(path));
        }
        return;
    }
    let key = key(path);
    if all.get(&key).map(|held| held.as_slice()) != Some(files) {
        all.insert(key, files.to_vec());
    }
}

/// What the files the document was last worked out from say now, named as an answer names them -
/// or, `as_read`, as they stood when it was.
fn read_from(path: &Path, as_read: bool) -> String {
    let files = reading().lock().unwrap_or_else(|e| e.into_inner()).get(&key(path)).cloned();
    let Some(files) = files else { return String::new() };
    if as_read {
        return crate::app_api::reads_version(&files);
    }
    let now: Vec<(String, String)> = files
        .into_iter()
        .map(|(file, _)| {
            let stamp = crate::actions::mount_stamp(&file);
            (file, stamp)
        })
        .collect();
    crate::app_api::reads_version(&now)
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
///
/// What its mounts read is said as the document was last worked out - most likely for the page
/// that is about to listen, a moment ago - and a mounted file that has moved since is announced
/// straight after, so a write in between is not missed.
pub fn listen(path: &Path) -> (Heard, broadcast::Receiver<Heard>) {
    // Subscribed under the lock, or the next look could forget the entry as nobody's before
    // this listener is counted.
    let (watched_now, listening) = {
        let mut all = watched().lock().unwrap_or_else(|e| e.into_inner());
        let one = all
            .entry(key(path))
            .or_insert_with(|| {
                let seen = Seen {
                    heard: Heard { version: String::new(), read_from: read_from(path, true) },
                    ..Seen::default()
                };
                Arc::new(Watched { to: broadcast::channel(16).0, seen: Mutex::new(seen) })
            })
            .clone();
        let listening = one.to.subscribe();
        (one, listening)
    };
    // The text first, which the first look only learns, and then what the mounts read now.
    look_again(path, &watched_now);
    let now = watched_now.seen.lock().unwrap_or_else(|e| e.into_inner()).heard.clone();
    look_again(path, &watched_now);
    keep_looking();
    (now, listening)
}

/// Whether any page listens for this file now, and so will ask to catch up once it is written.
pub fn listened(path: &Path) -> bool {
    if watched().lock().unwrap_or_else(|e| e.into_inner()).is_empty() {
        return false;
    }
    let key = key(path);
    let all = watched().lock().unwrap_or_else(|e| e.into_inner());
    all.get(&key).is_some_and(|one| one.to.receiver_count() > 0)
}

/// Something here wrote this file: tell whoever listens now, rather than at the next look. The
/// file may be one another document mounts, so every document watched is looked at again - a
/// look at one that has not moved costs a few stamps.
pub fn wrote(_path: &Path) {
    let now: Vec<(PathBuf, Arc<Watched>)> = {
        let all = watched().lock().unwrap_or_else(|e| e.into_inner());
        all.iter().map(|(path, one)| (path.clone(), one.clone())).collect()
    };
    for (path, one) in now {
        look_again(&path, &one);
    }
}

/// Read the file again if it moved, look at what its mounts read, and announce what is new.
fn look_again(path: &Path, watched: &Watched) {
    let mut seen = watched.seen.lock().unwrap_or_else(|e| e.into_inner());
    let first = seen.heard.version.is_empty();
    let mut moved = false;
    let meta = std::fs::metadata(path).ok();
    let modified = meta.as_ref().and_then(|m| m.modified().ok());
    let len = meta.as_ref().map_or(0, |m| m.len());
    if first || seen.modified != modified || seen.len != len {
        seen.modified = modified;
        seen.len = len;
        let Ok(text) = std::fs::read_to_string(path) else { return };
        let version = crate::app_api::version_of(&text);
        if version != seen.heard.version {
            seen.heard.version = version;
            moved = true;
        }
    }
    // The first look only learns what the file says; whoever asked is told that in reply.
    if first {
        return;
    }
    let read_from = read_from(path, false);
    if read_from != seen.heard.read_from {
        seen.heard.read_from = read_from;
        moved = true;
    }
    if moved {
        let _ = watched.to.send(seen.heard.clone());
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
