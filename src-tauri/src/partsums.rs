//! What an aggregate over a list with parts reads of the parts it has not read: a summary of each.
//!
//! A windowed list with parts reads only its newest parts - see `parts::bring_in`. A total over the
//! whole history, `History.filter(|x| x/status != "cancelled").map(|x| x/points).sum()`, still has
//! to count the rest. Reading them for it would read the whole history on every open, which is
//! what the parts were for. So such a chain is answered from the entries the list holds, and from
//! a summary of each part it does not: what the chain makes of that part's entries, before its
//! last step - the points to be summed, the dates of which the latest is wanted.
//!
//! A summary is made once for a part as it stands, and kept for as long as the server runs: an
//! append to this week's part changes how that part stands, and leaves the older ones' summaries as
//! they were. It holds what the chain made of each entry, so a summary is exactly what reading the
//! part and running the chain over it would have counted.
//!
//! Which chains can be answered so, and what is made of an entry, is `formula_evaluator`'s:
//! `FormulaEvaluator::summarised`. What is kept here is the summaries, and the template the
//! entries of a part are made from.

use crate::types::{OverseerNode, OverseerValue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};

/// On a list with parts not read: the key its entries' template is kept under - see `keep_template`.
pub const TEMPLATE: &str = "_parts_template";

/// The templates the entries of lists with unread parts are made from, by a hash of the template.
///
/// Kept apart from the document and named on its list, since a formula is worked out over the
/// document as it stands, where the template a list from another file's template names is no
/// node of the tree.
static TEMPLATES: LazyLock<Mutex<HashMap<String, Arc<OverseerNode>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Say on a list with parts not read which template its entries are made from, as the resolver
/// makes the entries it holds.
pub fn keep_template(list: &mut OverseerNode, template: &OverseerNode) {
    if !crate::parts::leaves_parts_unread(list) {
        list.parameters.remove(TEMPLATE);
        return;
    }
    // Without the ids of the parse it came from, which differ from one reading of the document
    // to the next: the key is the template as written, and summaries on disk are kept by it.
    let mut written = template.clone();
    without_parse_ids(&mut written);
    let Ok(text) = serde_json::to_string(&written) else { return };
    let mut hasher = twox_hash::XxHash64::with_seed(0);
    std::hash::Hasher::write(&mut hasher, text.as_bytes());
    let key = format!("{:016x}", std::hash::Hasher::finish(&hasher));
    if let Ok(mut kept) = TEMPLATES.lock() {
        // A server meets few templates; one that met many would only lose some to make again.
        if kept.len() > 256 {
            kept.clear();
        }
        kept.entry(key.clone()).or_insert_with(|| Arc::new(template.clone()));
    }
    list.parameters.insert(TEMPLATE.to_string(), OverseerValue::String(key));
}

fn without_parse_ids(node: &mut OverseerNode) {
    node.source_id = None;
    node.source_snapshot = None;
    node.children.iter_mut().for_each(without_parse_ids);
}

/// The template a list's entries are made from, and the key it is kept under.
pub fn template_of(list: &OverseerNode) -> Option<(&str, Arc<OverseerNode>)> {
    let Some(OverseerValue::String(key)) = list.parameters.get(TEMPLATE) else { return None };
    let template = TEMPLATES.lock().ok()?.get(key)?.clone();
    Some((key.as_str(), template))
}

/// What a chain made of one part's entries.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Summary {
    /// What each entry gave, for a chain that compares no field with its reader.
    pub values: Vec<OverseerValue>,
    /// For a chain whose filter compares a field with its reader, `x/parent == ../handle`: what
    /// each entry gave, by the string that field holds.
    pub by_key: HashMap<String, Vec<OverseerValue>>,
    /// The same, for entries holding something other than a string there, which are compared
    /// one by one - equality across types is not something a map can stand in for.
    pub odd: Vec<(OverseerValue, OverseerValue)>,
    /// Why the part cannot be summarised: a field the chain reads is worked out by a formula,
    /// which only the document can work out.
    pub cannot: Option<String>,
}

/// Summaries by the chain and the template they were made for, then by the part and how it stood.
type Kept = HashMap<String, HashMap<String, Arc<Summary>>>;

static SUMMARIES: LazyLock<Mutex<Kept>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// How many summaries are made: a part not read is summarised once for each chain, and never
/// again until it changes.
static MADE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub fn summaries_made() -> usize {
    MADE.load(std::sync::atomic::Ordering::Relaxed)
}

/// The summary of a part for a chain, made by `make` from the part's entries the first time.
///
/// `chain` names the chain and the template together, `file` and `stamp` the part as it stood when
/// the list was read: a part that changes is a new summary, and its old one is never asked for
/// again. `None` when the part cannot be read.
///
/// One not held in memory is looked for on disk, by the part's text, before it is made - see
/// `on_disk` - so a server that has just started reads no part a summary was kept for.
pub fn summary(
    chain: &str,
    file: &str,
    stamp: &str,
    make: impl FnOnce(Vec<OverseerNode>) -> Summary,
) -> Option<Arc<Summary>> {
    let part = format!("{}\t{}", file, stamp);
    if let Ok(kept) = SUMMARIES.lock() {
        if let Some(found) = kept.get(chain).and_then(|parts| parts.get(&part)) {
            return Some(found.clone());
        }
    }
    let made = match on_disk::folder() {
        Some(folder) => {
            let text = on_disk::text_of(file, stamp)?;
            let key = on_disk::Key::new(folder, chain, text);
            match key.read() {
                Some(found) => Arc::new(found),
                None => {
                    // From the parse the part's other chains share: parsing it for each chain
                    // made a fresh server's first open a third slower than one keeping nothing.
                    let (parsed, entries) = crate::parts::text_and_entries_in(Path::new(file))?;
                    let made = Arc::new(make(entries));
                    MADE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    // Written down only under the text it was made from, should the part have
                    // changed between the two reads.
                    if on_disk::text(&parsed) == text {
                        key.write(made.clone());
                    }
                    made
                }
            }
        }
        None => {
            let made = Arc::new(make(crate::parts::entries_in(Path::new(file))?));
            MADE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            made
        }
    };
    if let Ok(mut kept) = SUMMARIES.lock() {
        // Bounded by what a server meets: every chain over every part of every list read, a few
        // thousand small summaries. One that met more makes them again as it reads them.
        if kept.values().map(HashMap::len).sum::<usize>() > 50_000 {
            kept.clear();
        }
        let parts = kept.entry(chain.to_string()).or_default();
        // Only the part as it stands now is kept: a part that has changed is never read as it was.
        parts.retain(|held, _| held.split_once('\t').map(|(f, _)| f) != Some(file));
        parts.insert(part, made.clone());
    }
    Some(made)
}

/// Let go of every summary held in memory, as a server that has just started holds none. For
/// tests, which have no other way to tell a summary read from disk from one held.
pub fn forget_in_memory() {
    if let Ok(mut kept) = SUMMARIES.lock() {
        kept.clear();
    }
    if let Ok(mut held) = HELD.lock() {
        held.clear();
    }
}

/// Summaries kept on disk, beside the documents `document_disk` keeps, so that a server that has
/// just started - a deploy, a restart - does not read every part of a history again to make them.
///
/// A file is named by the build, the chain with its template, the time zone, and the part's text,
/// and holds all of them to be checked against. Nothing else goes into a summary: the chain reads
/// only its entry, through functions that read no clock, though the days and weeks they count are
/// local ones. A part edited, a template or a chain changed, a server rebuilt: each is another
/// name, and the old file is never read. A file is read without being taken, unlike a document's,
/// since every open of every document reading the part wants it; files no longer asked for go as
/// the folder is trimmed.
mod on_disk {
    use super::Summary;
    use serde::{Deserialize, Serialize};
    use std::hash::Hasher;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    /// The share of the disk cache's budget summaries may take: they are small, and a document
    /// worked out again costs more than any one of them.
    const SHARE: usize = 8;

    /// How many files are written between trims of the folder: listing it is what a trim costs.
    const TRIM_EVERY: usize = 64;

    pub struct Folder {
        path: PathBuf,
        budget: usize,
        build: &'static str,
    }

    /// Where summaries go, or nothing when the disk cache is off.
    pub fn folder() -> Option<Folder> {
        let (folder, budget) = crate::document_disk::setting()?;
        let build = crate::document_disk::build()?;
        Some(Folder { path: folder.join("sums"), budget: budget / SHARE, build })
    }

    /// The part's text, as two hashes and its length.
    pub type Text = (u64, u64, usize);

    pub fn text(text: &str) -> Text {
        (hash(0, &[text]), hash(1, &[text]), text.len())
    }

    /// The part's text as it stood with this stamp, read and hashed once for every chain over it:
    /// an open of a tracker asks for each of its parts half a dozen times.
    pub fn text_of(file: &str, stamp: &str) -> Option<Text> {
        static HASHED: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<String, (String, Text)>>> =
            std::sync::LazyLock::new(Default::default);
        if let Some((was, text)) = HASHED.lock().ok()?.get(file) {
            if was == stamp {
                return Some(*text);
            }
        }
        let hashed = text(&std::fs::read_to_string(file).ok()?);
        if let Ok(mut held) = HASHED.lock() {
            held.insert(file.to_string(), (stamp.to_string(), hashed));
        }
        Some(hashed)
    }

    #[derive(Deserialize)]
    struct Stored {
        build: String,
        chain: String,
        zone: String,
        text: Text,
        summary: Summary,
    }

    #[derive(Serialize)]
    struct Storing<'a> {
        build: &'a str,
        chain: &'a str,
        zone: &'a str,
        text: Text,
        summary: &'a Summary,
    }

    pub struct Key {
        folder: Folder,
        chain: String,
        zone: String,
        text: Text,
        file: PathBuf,
    }

    impl Key {
        pub fn new(folder: Folder, chain: &str, text: Text) -> Key {
            let zone = zone();
            let said = format!("{:016x}{:016x}{}", text.0, text.1, text.2);
            let name = hash(0, &[folder.build, chain, &zone, &said]);
            let file = folder.path.join(format!("{:016x}.json", name));
            Key { folder, chain: chain.to_string(), zone, text, file }
        }

        /// The summary kept under this key, if there is one that will do.
        pub fn read(&self) -> Option<Summary> {
            let bytes = std::fs::read(&self.file).ok()?;
            let stored: Stored = serde_json::from_slice(&bytes).ok()?;
            if stored.build != self.folder.build
                || stored.chain != self.chain
                || stored.zone != self.zone
                || stored.text != self.text
            {
                return None;
            }
            // Marked as used: a trim lets go of the files touched longest ago first.
            if let Ok(file) = std::fs::File::options().write(true).open(&self.file) {
                let _ = file.set_modified(std::time::SystemTime::now());
            }
            Some(stored.summary)
        }

        /// Write the summary down, without making the open that made it wait.
        pub fn write(self, summary: Arc<Summary>) {
            crate::document_disk::in_background(move || {
                let storing = Storing {
                    build: self.folder.build,
                    chain: &self.chain,
                    zone: &self.zone,
                    text: self.text,
                    summary: &summary,
                };
                let Ok(bytes) = serde_json::to_vec(&storing) else { return };
                if bytes.len() > self.folder.budget {
                    return;
                }
                // A summary that cannot be written is one made again later, and nothing more.
                if crate::document_disk::put(&self.folder.path, &self.file, &bytes).is_ok() {
                    trim_now_and_then(&self.folder.path, self.folder.budget);
                }
            });
        }
    }

    fn trim_now_and_then(folder: &Path, budget: usize) {
        static WRITTEN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        if WRITTEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % TRIM_EVERY == 0 {
            crate::document_disk::trim(folder, budget);
        }
    }

    fn hash(seed: u64, parts: &[&str]) -> u64 {
        let mut hasher = twox_hash::XxHash64::with_seed(seed);
        for part in parts {
            hasher.write(part.as_bytes());
            hasher.write(&[0]);
        }
        hasher.finish()
    }

    /// The machine's time zone, as far as a summary can tell: its offset at the start of 2026 and
    /// in its summer. `day`, `week` and the rest count local days.
    fn zone() -> String {
        use chrono::{Local, TimeZone};
        [1_767_225_600i64, 1_782_864_000]
            .iter()
            .filter_map(|at| Local.timestamp_opt(*at, 0).single())
            .map(|at| at.offset().to_string())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The summary of entries a list holds that nothing has worked out, made by `make` the first
/// time: the rest of the part read for a window. Kept by what the entries' text was, which is
/// what a summary of them is made from.
pub fn summary_of_held(chain: &str, entries: &[&OverseerNode], make: impl FnOnce() -> Summary) -> Arc<Summary> {
    let mut key = String::from(chain);
    for entry in entries {
        match entry.source_fingerprint {
            Some(fingerprint) => key.push_str(&format!(" {:016x}", fingerprint)),
            None => return Arc::new(make()),
        }
    }
    if let Some(found) = HELD.lock().ok().and_then(|held| held.get(&key).cloned()) {
        return found;
    }
    let made = Arc::new(make());
    if let Ok(mut held) = HELD.lock() {
        if held.len() > 1024 {
            held.clear();
        }
        held.insert(key, made.clone());
    }
    made
}

static HELD: LazyLock<Mutex<HashMap<String, Arc<Summary>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// The template cut down to what a chain reads of an entry - see `made_from`.
pub fn cut_down(template: &OverseerNode, fields: &[String]) -> OverseerNode {
    let mut cut = template.clone();
    keep_only(&mut cut.children, fields);
    cut
}

/// An entry of a part made from its template, as far as the fields a chain reads: the template cut
/// down to those - `cut_down` - and what the entry writes of them laid over it as the resolver
/// lays an entry over its template.
pub fn made_from(cut: &OverseerNode, fields: &[String], entry: &OverseerNode, root: &[OverseerNode]) -> OverseerNode {
    let mut made = cut.clone();
    let written: Vec<OverseerNode> = entry
        .children
        .iter()
        .filter(|c| fields.iter().any(|f| *f == c.name) || is_unnamed(c))
        .cloned()
        .collect();
    crate::resolver::merge_node(&mut made, &written, root);
    crate::resolver::hold_numbers_as_declared(std::slice::from_mut(&mut made));
    made
}

/// The template cut down to the fields read, and the unnamed containers they may sit in: a
/// history's entry holds its comments and its runs, which a total never reads.
fn keep_only(children: &mut Vec<OverseerNode>, fields: &[String]) {
    children.retain_mut(|child| {
        if child.is_hierarchy_transparent {
            keep_only(&mut child.children, fields);
            true
        } else {
            fields.iter().any(|f| *f == child.name)
        }
    });
}

fn is_unnamed(node: &OverseerNode) -> bool {
    node.name.is_empty() || node.name == node.node_type
}
