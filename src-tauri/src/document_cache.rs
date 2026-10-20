//! What has already been worked out, kept for as long as it is worth keeping.
//!
//! This was a single slot: one document's resolved tree and one graph, each held against the text
//! it belonged to. Exactly right for a desktop app with one document open, and exactly wrong for
//! the server, which is asked about several in turn - `tasks.os`, then `tracker_v2.os`, then
//! `foods.os` - and evicted each by serving the next. Measured on the real documents, serving one
//! other document in between took a reopen of the food tracker from 0.96 seconds back to 8.72. The
//! bot interleaves documents inside a single conversation, so its hit rate was near zero and it
//! paid a full resolve on almost every request.
//!
//! Holding all of them is not an option either, and that is what shapes this module. Counted with
//! an allocator rather than guessed at, a resolved `tracker_v2.os` is **84 MB** and the graph that
//! goes with it another **66** - call it 150 MB for the one document, against about 27 MB for
//! `tasks.os` and under 3 for `diary.os`. Keeping the five documents this household uses would
//! cost more than the whole idle footprint the server was brought down to.
//!
//! So the cache is bounded **by bytes rather than by count**, and a document too large for the
//! budget is declined outright rather than evicting everything else and still not fitting.
//!
//! The budget differs by build because the constraint does: a desktop machine can spare half a
//! gigabyte to never wait eight seconds again, and a container billed by the gigabyte-hour cannot.
//! At the server's default the four smaller documents fit and the food tracker does not, which is
//! most of the benefit for a bounded cost.
//!
//! What is let go for want of room is not lost: it is written to disk and read back from there
//! when next asked for, which costs about a third of working it out again - see `document_disk`.

use crate::types::OverseerNode;

/// How many bytes of worked-out documents to keep.
///
/// `OVERSEER_CACHE_MB` overrides it, and `0` switches the cache off - which is the setting to
/// reach for if a container is ever killed for its memory.
pub fn budget_bytes() -> usize {
    let asked = BUDGET_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed);
    if asked >= 0 {
        return asked as usize;
    }
    static BUDGET: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *BUDGET.get_or_init(|| {
        if let Some(asked) = std::env::var("OVERSEER_CACHE_MB")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
        {
            return asked * 1024 * 1024;
        }
        // Read as "the server and nothing else", because the tests build neither feature and want
        // the generous default: they run on a development machine, and several of them check that
        // reopening a document agrees with loading it fresh - which needs it to have been kept.
        let modest = cfg!(feature = "server") && !cfg!(feature = "desktop");
        (if modest { 64 } else { 512 }) * 1024 * 1024
    })
}

/// A budget for the duration of a test, in place of the build's own.
///
/// The same shape as `FormulaEvaluator::set_time_override` and for the same reason: the real
/// setting is decided once per process, and what wants testing is the behaviour at several. `None`
/// hands it back to the build.
static BUDGET_OVERRIDE: std::sync::atomic::AtomicIsize =
    std::sync::atomic::AtomicIsize::new(-1);

pub fn set_budget_override(bytes: Option<usize>) {
    BUDGET_OVERRIDE.store(
        bytes.map_or(-1, |b| b as isize),
        std::sync::atomic::Ordering::Relaxed,
    );
}

/// Roughly what a resolved document occupies, erring high.
///
/// Erring high on purpose: this number decides what is kept, so an underestimate spends memory
/// that was budgeted not to be spent, while an overestimate only declines to keep something it
/// could have.
///
/// Counting the strings rather than the nodes is what makes it usable. The food tracker holds
/// about three times the bytes per parameter that the shopping list does, because its parameters
/// are formulas, so a figure worked out from node and parameter counts alone was out by a factor
/// of two on the one document that matters.
pub fn footprint(nodes: &[OverseerNode]) -> usize {
    fn of_value(value: &crate::types::OverseerValue) -> usize {
        use crate::types::OverseerValue::*;
        match value {
            String(s) | Date(s) | Timestamp(s) | Formula(s) | Template(s) => s.capacity(),
            _ => 0,
        }
    }
    /// What a node's source snapshot holds, counted once however many nodes share it - see
    /// `OverseerNode::source_snapshot`. The text it is a view of is counted once as well.
    fn snapshot(
        node: &OverseerNode,
        seen: &mut std::collections::HashSet<usize>,
        texts: &mut std::collections::HashSet<usize>,
    ) -> usize {
        let Some(held) = node.source_snapshot.as_ref() else { return 0 };
        if !seen.insert(std::sync::Arc::as_ptr(held) as usize) {
            return 0;
        }
        let mut total = std::mem::size_of::<crate::types::NodeSourceSnapshot>()
            + held.leading_trivia.capacity()
            + held.trailing_trivia.capacity();
        if let Some((at, len)) = held.full_text.backing() {
            if texts.insert(at) {
                total += len;
            }
        }
        total
    }
    fn walk(
        nodes: &[OverseerNode],
        seen: &mut std::collections::HashSet<usize>,
        texts: &mut std::collections::HashSet<usize>,
    ) -> usize {
        // A list of parameters, not a table - see `types::Params`.
        let entry = std::mem::size_of::<(std::string::String, crate::types::OverseerValue)>();
        // The slice says how many nodes there are, not how much room the Vec has; the slack a Vec
        // carries is part of what the factor at the end stands in for.
        let mut total = nodes.len() * std::mem::size_of::<OverseerNode>();
        for node in nodes {
            total += node.name.capacity() + node.node_type.capacity();
            total += node.template.as_ref().map_or(0, |s| s.capacity());
            total += node.raw_value_literal.as_ref().map_or(0, |s| s.capacity());
            total += node.parameters.len() * entry;
            total += snapshot(node, seen, texts);
            for (key, value) in &node.parameters {
                total += key.capacity() + of_value(value);
            }
            // Every parameter name a second time: the map keys them, and this keeps their order.
            total += node.param_order.len() * std::mem::size_of::<std::string::String>();
            for key in &node.param_order {
                total += key.capacity();
            }
            total += walk(&node.children, seen, texts);
        }
        total
    }
    // Against a counting allocator on the five real documents, the plain walk came to between 74%
    // and 104% of what was actually allocated - the remainder being allocator rounding and the
    // slack a String or a Vec carries beyond what it was asked for. Rather than pretend to model
    // that, the walk is raised until no document is underestimated.
    //
    // Checked again after `slimtree` made the parameters a list and the snapshots shared, the same
    // way, and the walk is close now: 12.9 MB for the food tracker against the allocator's 10.0
    // for a copy of the tree plus 2.9 of snapshots the copy shares rather than repeats. A factor
    // of seven quarters over that made the cache hold one heavy document where two fit, so it is
    // a quarter now - margin, rather than a correction.
    let mut seen = std::collections::HashSet::new();
    let mut texts = std::collections::HashSet::new();
    walk(nodes, &mut seen, &mut texts) * 5 / 4
}

/// One document, as far as it has been worked out.
///
/// The tree and the graph arrive separately - a load produces both, an edit refreshes one - so
/// either may be absent while the other is held.
struct Entry {
    /// The text this was worked out from, and the only thing that makes it usable: a tree or a
    /// graph applied to a different document would name nodes that do not exist and, worse, fail
    /// to name ones that do.
    text: String,
    /// The folder it was worked out in, and the other half of what makes it usable. A document's
    /// mounts are read relative to its own folder, so the same text in two folders is two
    /// documents: each mounting its own neighbour, and each judged fresh against its own files -
    /// see `mounts_unchanged`. Keyed on the text alone, the tests that copy one tracker beside a
    /// catalogue each were handed one another's tracker, worked out from a catalogue without the
    /// food just added to theirs.
    base: Option<std::path::PathBuf>,
    /// The environment it was worked out in, and the last of what makes it usable: the same
    /// document in the same folder is drawn dark for one page and light for another - see
    /// `environment::key`.
    seen: String,
    nodes: Option<Vec<OverseerNode>>,
    graph: Option<crate::dependencies::Graph>,
    bytes: usize,
    /// When this was last asked for, so the least useful entry is the one that goes.
    used: u64,
    /// The files the tree's mounts were read from and the bases its templates came from, and how
    /// each stood then - see `actions::mounts_held` and `bases::held`. The text is not all a tree
    /// was worked out from.
    mounts: Vec<(String, String)>,
    /// The minute what reads the clock in `nodes` was worked out in - see `WorkedOutIn`.
    clock_minute: WorkedOutIn,
}

impl Entry {
    /// Whether every file its mounts and bases were read from still stands as it did. A food added
    /// to the catalog changes the catalog's file and not the food tracker's text, and a tracker
    /// held from before it was handed out as current until something else moved it. A base a
    /// tracker's templates come from is the same case, and is held the same way.
    fn mounts_unchanged(&self) -> bool {
        self.mounts
            .iter()
            .all(|(file, stamp)| crate::actions::mount_stamp(file) == *stamp)
    }

    /// Whether whoever is asking may read every file it was worked out from - see `access`. What
    /// one person was shown, with a catalogue mounted they may read, is not for someone who may
    /// not. The settings it was drawn in are the server's, and read for everyone.
    fn readable(&self) -> bool {
        if crate::access::asker() == crate::access::Asker::Anyone {
            return true;
        }
        let settings = crate::environment::held();
        self.mounts
            .iter()
            .filter(|held| !settings.contains(held))
            .all(|(file, _)| crate::access::may_read(file))
    }
}

impl Entry {
    fn measure(&mut self) {
        self.bytes = self.text.capacity()
            + self.nodes.as_deref().map_or(0, footprint)
            + self.graph.as_ref().map_or(0, |g| g.footprint());
    }
}

static STORE: std::sync::Mutex<Vec<Entry>> = std::sync::Mutex::new(Vec::new());
static TICK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn now() -> u64 {
    TICK.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Bring the store back within budget.
///
/// Anything that cannot fit on its own goes first and unconditionally - there is no point evicting
/// four useful documents for one that will not fit either. Then the least recently asked-for, until
/// what is left fits.
fn trim(store: &mut Vec<Entry>) {
    let budget = budget_bytes();
    let mut at = 0;
    while at < store.len() {
        if store[at].bytes > budget {
            let_go(store.remove(at));
        } else {
            at += 1;
        }
    }
    while store.iter().map(|e| e.bytes).sum::<usize>() > budget {
        let Some(stalest) = store
            .iter()
            .enumerate()
            .min_by_key(|(_, e)| e.used)
            .map(|(at, _)| at)
        else {
            break;
        };
        let_go(store.remove(stalest));
    }
}

/// What leaves memory for want of room goes to disk, where it can be read back for less than it
/// would take to work out again - see `document_disk`.
///
/// Only a document with its graph: a document is only handed back as it stood when there is a
/// graph to say what in it reads the clock, and a graph without its document answers nothing. One
/// of the two goes missing only for a moment, while an edit replaces it.
fn let_go(entry: Entry) {
    if let (Some(nodes), Some(graph)) = (entry.nodes, entry.graph) {
        crate::document_disk::store(
            entry.text,
            entry.base,
            entry.seen,
            crate::document_disk::Held {
                nodes,
                graph,
                mounts: entry.mounts,
                clock_minute: entry.clock_minute,
            },
        );
    }
}

fn with<R>(work: impl FnOnce(&mut Vec<Entry>) -> R) -> R {
    let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    work(&mut store)
}

/// The folder whoever is asking is working in - see `Entry::base`.
fn here() -> Option<std::path::PathBuf> {
    crate::docmgr::manager::DocumentManager::base_dir()
}

/// Where the entry for this text, worked out in this folder, is - and nowhere, once a file its
/// mounts were read from has changed: then what it holds is let go, tree and graph, and whoever
/// asked works the document out from the files as though it had never been held.
fn at(store: &mut Vec<Entry>, text: &str) -> Option<usize> {
    let found = held_at(store, text)?;
    store[found].readable().then_some(found)
}

/// The same, whoever is asking: for putting something in the entry's place, which is no reading
/// of what it holds.
fn held_at(store: &mut Vec<Entry>, text: &str) -> Option<usize> {
    let base = here();
    let seen = crate::environment::key();
    let found = store
        .iter()
        .position(|entry| entry.text == text && entry.base == base && entry.seen == seen)?;
    if !store[found].mounts_unchanged() {
        store.remove(found);
        return None;
    }
    Some(found)
}

/// The same, and failing that the entry written to disk when it last left memory, brought back.
///
/// For whoever wants what was worked out, not for whoever is about to put something in its place.
/// Not trimmed on arrival, though it may not fit: the caller is about to take from it, and the next
/// thing put will make room.
fn found(store: &mut Vec<Entry>, text: &str) -> Option<usize> {
    if let Some(found) = at(store, text) {
        return Some(found);
    }
    if budget_bytes() == 0 {
        return None;
    }
    let base = here();
    let seen = crate::environment::key();
    let held = crate::document_disk::take(text, &base, &seen)?;
    let mut entry = Entry {
        text: text.to_string(),
        base,
        seen,
        nodes: Some(held.nodes),
        graph: Some(held.graph),
        bytes: 0,
        used: now(),
        mounts: held.mounts,
        clock_minute: held.clock_minute,
    };
    if !entry.mounts_unchanged() {
        return None;
    }
    entry.measure();
    let readable = entry.readable();
    store.push(entry);
    readable.then_some(store.len() - 1)
}

/// The graph held for this text, lent to `look` rather than copied out.
///
/// Every edit, every press and every reopen that refreshes what reads the clock asks the graph one
/// question - what a change reaches - and each copied all of it first to ask: 5 ms on the project
/// tracker and 27 on the food tracker, spent on every one of them. Asked under the store's lock,
/// held for as long as the question takes and no longer.
pub fn with_graph<R>(text: &str, look: impl FnOnce(&crate::dependencies::Graph) -> R) -> Option<R> {
    with(|store| {
        let found = found(store, text)?;
        store[found].used = now();
        store[found].graph.as_ref().map(look)
    })
}

/// Add what a later resolve recorded to the graph held for this text, where it is - see
/// `Graph::merge`. Nothing when no graph is held: what part of a document read is not a graph of
/// the document. The entry is measured again when it next moves, which every write makes it do.
pub fn add_to_graph(text: &str, newer: crate::dependencies::Graph) {
    with(|store| {
        if let Some(found) = at(store, text) {
            if let Some(graph) = store[found].graph.as_mut() {
                graph.merge(newer);
            }
        }
    })
}

/// The document this text produced, as it last stood, without taking it.
pub fn nodes_for(text: &str) -> Option<Vec<OverseerNode>> {
    with(|store| {
        let found = found(store, text)?;
        store[found].used = now();
        store[found].nodes.clone()
    })
}

/// The minute what reads the clock in a document held was worked out in.
///
/// Every function a document asks the time with answers in minutes or coarser - the minute of the
/// day, minutes since, days since, the same day - so within one minute there is nothing for the
/// clock to have moved. Several writes in a row are the usual shape of things: four foods from one
/// message, three tasks marked done, the sweep opening a morning's tasks. The first brings the
/// document up to the minute and the rest find it there.
///
/// Held with the document, put in and taken out with it under one lock, because it is a fact about
/// that document and no other. It used to be said in a step of its own after the document was put
/// in, and the cache is shared by every thread: one put its document in, another put in its own
/// from an earlier minute, and the first then marked the second's as current in its own. The next
/// open in that minute was handed the earlier one's figures, as current. A document put in by whoever
/// does not know how current it is - one taken and put back as it was - is put in with `None`.
pub type WorkedOutIn = Option<i64>;

/// The document this text produced, lent to `look` rather than copied out - for a question that
/// decides whether to take it at all.
pub fn with_nodes<R>(text: &str, look: impl FnOnce(&[OverseerNode]) -> R) -> Option<R> {
    with(|store| {
        let found = found(store, text)?;
        store[found].nodes.as_deref().map(look)
    })
}

/// The same, handed over rather than copied, with the minute it was worked out in.
///
/// For a caller that is about to replace it anyway: on a large document a copy costs more than the
/// diff it feeds.
pub fn take_nodes(text: &str) -> Option<(Vec<OverseerNode>, WorkedOutIn)> {
    with(|store| {
        let found = found(store, text)?;
        store[found].used = now();
        let taken = store[found].nodes.take();
        let minute = store[found].clock_minute.take();
        if taken.is_some() {
            store[found].measure();
        }
        taken.map(|nodes| (nodes, minute))
    })
}

/// What this text's values were worked out from.
pub fn graph_for(text: &str) -> Option<crate::dependencies::Graph> {
    with(|store| {
        let found = found(store, text)?;
        store[found].used = now();
        store[found].graph.clone()
    })
}

fn put(text: &str, work: impl FnOnce(&mut Entry)) {
    // Worked out without a file its asker may not read, it is not the document as anyone else
    // would be shown it - see `access::refused_any`.
    if crate::access::refused_any() {
        return;
    }
    with(|store| {
        let found = match held_at(store, text) {
            Some(found) => found,
            None => {
                store.push(Entry {
                    text: text.to_string(),
                    base: here(),
                    seen: crate::environment::key(),
                    nodes: None,
                    graph: None,
                    bytes: 0,
                    used: 0,
                    mounts: Vec::new(),
                    clock_minute: None,
                });
                store.len() - 1
            }
        };
        let entry = &mut store[found];
        entry.used = now();
        work(entry);
        entry.measure();
        trim(store);
    })
}

/// Every file a tree was worked out from besides its own text: what its mounts read, the bases its
/// templates came from, and the settings and theme it was drawn in.
fn read_from(nodes: &[OverseerNode]) -> Vec<(String, String)> {
    let mut files = crate::actions::mounts_held(nodes);
    for held in crate::bases::held(nodes).into_iter().chain(crate::environment::held()) {
        if !files.contains(&held) {
            files.push(held);
        }
    }
    files
}

pub fn put_nodes(text: &str, nodes: &[OverseerNode], worked_out_in: WorkedOutIn) {
    // Nothing is kept at all when the budget is nothing, rather than a copy being made and
    // immediately dropped.
    if budget_bytes() == 0 {
        return;
    }
    put(text, |entry| {
        entry.mounts = read_from(nodes);
        entry.nodes = Some(nodes.to_vec());
        entry.clock_minute = worked_out_in;
    });
}

/// The same, when the caller has a document it no longer needs.
pub fn own_nodes(text: &str, nodes: Vec<OverseerNode>, worked_out_in: WorkedOutIn) {
    if budget_bytes() == 0 {
        return;
    }
    put(text, |entry| {
        entry.mounts = read_from(&nodes);
        entry.nodes = Some(nodes);
        entry.clock_minute = worked_out_in;
    });
}

pub fn put_graph(text: &str, graph: crate::dependencies::Graph) {
    if budget_bytes() == 0 {
        return;
    }
    put(text, |entry| entry.graph = Some(graph));
}

/// Take the graph held for this text away, and hand it over.
///
/// For a graph that must not be moved onto another text by `rekey`, or must not be kept at all:
/// after a change of shape the one held describes a document that no longer exists.
pub fn take_graph(text: &str) -> Option<crate::dependencies::Graph> {
    with(|store| {
        let found = at(store, text)?;
        let graph = store[found].graph.take();
        store[found].measure();
        graph
    })
}

/// The document has been written out afresh; what is held still describes it, under its new text.
///
/// The old text is named because there may now be several entries and only one of them moved -
/// and only the one in the caller's folder, since the same text elsewhere was not written.
pub fn rekey(was: &str, now_text: &str) {
    with(|store| {
        // Whatever was already held for the new text is superseded by what is being moved onto it.
        if was != now_text {
            if let Some(stale) = at(store, now_text) {
                store.remove(stale);
            }
        }
        if let Some(found) = at(store, was) {
            store[found].text = now_text.to_string();
            store[found].used = now();
            store[found].measure();
        }
        trim(store);
    })
}

/// Drop every remembered document, keeping the graphs.
///
/// Nothing depends on this for correctness - a tree is only used when its text matches what the
/// caller sent, so a stale one is ignored rather than misapplied. It exists so a closed document
/// does not sit in memory, and so tests can start from a known state.
pub fn forget_nodes() {
    with(|store| {
        for entry in store.iter_mut() {
            entry.nodes = None;
            entry.measure();
        }
        store.retain(|entry| entry.graph.is_some());
    })
}

/// Drop every graph, keeping the documents.
pub fn forget_graphs() {
    with(|store| {
        for entry in store.iter_mut() {
            entry.graph = None;
            entry.measure();
        }
        store.retain(|entry| entry.nodes.is_some());
    })
}

/// What the cache is holding: how many documents, and how many bytes it believes they are.
///
/// For tests and for anyone wanting to know what the budget is buying.
pub fn held() -> (usize, usize) {
    with(|store| (store.len(), store.iter().map(|e| e.bytes).sum()))
}
