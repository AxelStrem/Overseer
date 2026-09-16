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
    /// What a hash table of `n` entries allocates, near enough: capacity is rounded up to a power
    /// of two above 8/7 of the length, and every bucket is paid for whether it holds anything.
    fn table(entries: usize, per_entry: usize) -> usize {
        if entries == 0 {
            return 0;
        }
        (entries * 8 / 7 + 1).next_power_of_two() * (per_entry + 1)
    }
    fn walk(nodes: &[OverseerNode]) -> usize {
        let bucket = std::mem::size_of::<(std::string::String, crate::types::OverseerValue)>();
        // The slice says how many nodes there are, not how much room the Vec has; the slack a Vec
        // carries is part of what the factor at the end stands in for.
        let mut total = nodes.len() * std::mem::size_of::<OverseerNode>();
        for node in nodes {
            total += node.name.capacity() + node.node_type.capacity();
            total += node.template.as_ref().map_or(0, |s| s.capacity());
            total += node.raw_value_literal.as_ref().map_or(0, |s| s.capacity());
            total += table(node.parameters.len(), bucket);
            for (key, value) in &node.parameters {
                total += key.capacity() + of_value(value);
            }
            // Every parameter name a second time: the map keys them, and this keeps their order.
            total += node.param_order.len() * std::mem::size_of::<std::string::String>();
            for key in &node.param_order {
                total += key.capacity();
            }
            total += walk(&node.children);
        }
        total
    }
    // Against a counting allocator on the five real documents, the plain walk came to between 74%
    // and 104% of what was actually allocated - the remainder being allocator rounding and the
    // slack a String or a Vec carries beyond what it was asked for. Rather than pretend to model
    // that, the walk is raised until no document is underestimated.
    walk(nodes) * 7 / 4
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
    nodes: Option<Vec<OverseerNode>>,
    graph: Option<crate::dependencies::Graph>,
    bytes: usize,
    /// When this was last asked for, so the least useful entry is the one that goes.
    used: u64,
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
    store.retain(|entry| entry.bytes <= budget);
    while store.iter().map(|e| e.bytes).sum::<usize>() > budget {
        let Some(stalest) = store
            .iter()
            .enumerate()
            .min_by_key(|(_, e)| e.used)
            .map(|(at, _)| at)
        else {
            break;
        };
        store.remove(stalest);
    }
}

fn with<R>(work: impl FnOnce(&mut Vec<Entry>) -> R) -> R {
    let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    work(&mut store)
}

fn at(store: &mut [Entry], text: &str) -> Option<usize> {
    store.iter().position(|entry| entry.text == text)
}

/// The document this text produced, as it last stood, without taking it.
pub fn nodes_for(text: &str) -> Option<Vec<OverseerNode>> {
    with(|store| {
        let found = at(store, text)?;
        store[found].used = now();
        store[found].nodes.clone()
    })
}

/// The same, handed over rather than copied.
///
/// For a caller that is about to replace it anyway: on a large document a copy costs more than the
/// diff it feeds.
pub fn take_nodes(text: &str) -> Option<Vec<OverseerNode>> {
    with(|store| {
        let found = at(store, text)?;
        store[found].used = now();
        let taken = store[found].nodes.take();
        if taken.is_some() {
            store[found].measure();
        }
        taken
    })
}

/// What this text's values were worked out from.
pub fn graph_for(text: &str) -> Option<crate::dependencies::Graph> {
    with(|store| {
        let found = at(store, text)?;
        store[found].used = now();
        store[found].graph.clone()
    })
}

fn put(text: &str, work: impl FnOnce(&mut Entry)) {
    with(|store| {
        let found = match at(store, text) {
            Some(found) => found,
            None => {
                store.push(Entry {
                    text: text.to_string(),
                    nodes: None,
                    graph: None,
                    bytes: 0,
                    used: 0,
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

pub fn put_nodes(text: &str, nodes: &[OverseerNode]) {
    // Nothing is kept at all when the budget is nothing, rather than a copy being made and
    // immediately dropped.
    if budget_bytes() == 0 {
        return;
    }
    put(text, |entry| entry.nodes = Some(nodes.to_vec()));
}

/// The same, when the caller has a document it no longer needs.
pub fn own_nodes(text: &str, nodes: Vec<OverseerNode>) {
    if budget_bytes() == 0 {
        return;
    }
    put(text, |entry| entry.nodes = Some(nodes));
}

pub fn put_graph(text: &str, graph: crate::dependencies::Graph) {
    if budget_bytes() == 0 {
        return;
    }
    put(text, |entry| entry.graph = Some(graph));
}

/// The document has been written out afresh; what is held still describes it, under its new text.
///
/// The old text is named because there may now be several entries and only one of them moved.
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
