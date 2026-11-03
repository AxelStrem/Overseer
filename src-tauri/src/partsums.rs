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
    let Ok(text) = serde_json::to_string(template) else { return };
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

/// The template a list's entries are made from, and the key it is kept under.
pub fn template_of(list: &OverseerNode) -> Option<(&str, Arc<OverseerNode>)> {
    let Some(OverseerValue::String(key)) = list.parameters.get(TEMPLATE) else { return None };
    let template = TEMPLATES.lock().ok()?.get(key)?.clone();
    Some((key.as_str(), template))
}

/// What a chain made of one part's entries.
#[derive(Debug, Default)]
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
    let entries = crate::parts::entries_in(Path::new(file))?;
    let made = Arc::new(make(entries));
    MADE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
