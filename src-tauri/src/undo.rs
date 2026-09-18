//! What each document said before the last few writes.
//!
//! Undo here is not reversing operations. A document is text, so taking a write back is putting
//! the previous text where it was - which costs the same whether the write set one field, added
//! an entry, moved one between lists or changed the shape entirely. Nothing has to understand
//! what happened, which is the whole reason this is small.
//!
//! Kept on disk rather than in memory, because the process restarts on every deployment and a
//! history that does not survive that is a history nobody can rely on. Kept beside the documents
//! but not among them: the snapshots end in `.undo`, so `Service::list` does not mistake them for
//! documents and the backup - which stages `*.os`, `*.md` and the journal - never commits them.
//!
//! Bounded, because otherwise a document written to every five minutes fills a volume. What sits
//! behind the bound is git: `backup.py` commits every fifteen minutes, so this is for the last
//! few minutes and the repository is for the last few months.

use std::path::{Path, PathBuf};

/// How many steps back one document keeps.
///
/// Enough to undo a run of presses, few enough that a large document's history stays a few
/// megabytes. A deeper history is what the git backup is.
const KEPT: usize = 20;

/// The directory a document's snapshots live in.
///
/// Named after the document so it can be read by a person, and salted with a hash of the full
/// name so two documents cannot share one - `a/b.os` and `a__b.os` would otherwise collide.
fn store(root: &Path, document: &str) -> PathBuf {
    let readable: String = document
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
        .collect();
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in document.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    root.join(".undo").join(format!("{}.{:08x}", readable, hash as u32))
}

/// The snapshots for one document, oldest first.
fn snapshots(root: &Path, document: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(store(root, document)) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("undo"))
        .collect();
    // Named by the moment they were taken, so sorting by name sorts by age.
    found.sort();
    found
}

/// Keep what the document says now, before something writes over it.
///
/// Failures are silent on purpose. Not being able to save an undo point is a reason to lose the
/// ability to undo, not a reason to refuse the write the person asked for.
pub fn remember(root: &Path, document: &str, text: &str) {
    let directory = store(root, document);
    if std::fs::create_dir_all(&directory).is_err() {
        return;
    }
    hide_from_git(root);
    // Nanoseconds, because two writes in one millisecond are ordinary - a button that appends
    // and then sets is two writes, and they must not land on one name.
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    if std::fs::write(directory.join(format!("{:039}.undo", at)), text).is_err() {
        return;
    }
    let held = snapshots(root, document);
    for stale in held.iter().take(held.len().saturating_sub(KEPT)) {
        let _ = std::fs::remove_file(stale);
    }
}

/// The text before the most recent write, taken off the history.
///
/// Taken rather than copied: undoing twice should walk back two writes, not swap between the
/// same two states forever. The caller writes it, and does not record an undo point for that
/// write - otherwise the step it just took back would immediately be the newest one.
pub fn take(root: &Path, document: &str) -> Option<String> {
    let newest = snapshots(root, document).pop()?;
    let text = std::fs::read_to_string(&newest).ok()?;
    let _ = std::fs::remove_file(&newest);
    Some(text)
}

/// How many writes can still be taken back.
pub fn depth(root: &Path, document: &str) -> usize {
    snapshots(root, document).len()
}

/// Forget a document's history, which is what happens when it is deleted or renamed.
pub fn forget(root: &Path, document: &str) {
    let _ = std::fs::remove_dir_all(store(root, document));
}

/// Keep the store out of the backup's way.
///
/// The backup stages `*.os`, `*.md` and the journal, so nothing here is ever committed - but it
/// would still be reported as untracked on every cycle, which is noise in a repository whose
/// whole job is to be a clean record of the documents. A `.gitignore` holding `*` covers the
/// snapshots and itself, so git sees a directory with nothing in it to report.
///
/// Written rather than required of anyone: the store is the server's, and a document set cloned
/// onto a fresh volume should not need someone to remember this.
fn hide_from_git(root: &Path) {
    let marker = root.join(".undo").join(".gitignore");
    if marker.exists() {
        return;
    }
    let _ = std::fs::write(marker, "*
");
}
