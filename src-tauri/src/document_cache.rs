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
    /// The environment it was worked out in, whole - see `environment::key` - which is what a copy
    /// let go to disk is kept under.
    seen: String,
    /// How much of its windowed lists the reader asked to see - see `resolver::reading_more`. A
    /// tree that shows more of a list is another tree, whoever is looking.
    more: String,
    /// The environment it was worked out in, in its parts: what the settings say, who was looking,
    /// and the names the two declare - see `environment::look`. The same document is drawn dark
    /// for one page and light for another, and says `me` for each person as themselves.
    ///
    /// But most documents read none of it outside the copies of the contexts it declares, and two
    /// people kept two copies of every such document, each worked out whole: the agent loop
    /// writes the project tracker as one person while the owner reads it as another, and each
    /// write made the owner's page work the tracker out again. So an entry is handed to whoever
    /// sees the environment otherwise, as long as nothing outside the copies read what differs -
    /// see `Leans` - with its copies worked out again as they see them.
    look: String,
    who: String,
    names: Vec<String>,
    /// What the tree holds of the environment outside the copies - see `Leans`. Learned from the
    /// tree when it is put in, and from the graph, which says what its values read, when that is.
    /// Kept while either is taken out to be worked on, since it says what the text produced.
    tree_leans: Option<Leans>,
    graph_leans: Option<Leans>,
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

/// Which parts of the environment a tree's values hold outside the copies of its contexts: what
/// the settings say, and who is looking. A tree that holds neither is the same tree for anyone,
/// once its copies are worked out as they see them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Leans {
    look: bool,
    me: bool,
}

impl Leans {
    const ALL: Leans = Leans { look: true, me: true };

    fn or(self, other: Leans) -> Leans {
        Leans { look: self.look || other.look, me: self.me || other.me }
    }

    fn of(read: &[String]) -> Leans {
        Leans {
            look: read.iter().any(|name| name != crate::environment::ME),
            me: read.iter().any(|name| name == crate::environment::ME),
        }
    }
}

/// What the graph says the values outside the copies read of the environment.
fn leans_of_graph(graph: &crate::dependencies::Graph, names: &[String]) -> Leans {
    Leans::of(&graph.names_read_from_outside(names))
}

/// What the tree says by its shape. A node of the document's own named as the environment names
/// something hides it from what is inside, and the graph cannot tell what is under that node from
/// what is under a copy: such a tree is held as the environment saw it, and handed to nobody else.
fn leans_of_tree(nodes: &[OverseerNode], names: &[String]) -> Leans {
    fn named(nodes: &[OverseerNode], names: &[String]) -> bool {
        nodes.iter().any(|node| {
            (!crate::contexts::is_context(node) && names.contains(&node.name)) || named(&node.children, names)
        })
    }
    if named(nodes, names) {
        Leans::ALL
    } else {
        Leans::default()
    }
}

/// Whoever is asking, as the cache tells one environment from another.
struct Want {
    base: Option<std::path::PathBuf>,
    seen: String,
    more: String,
    look: String,
    who: String,
    names: Vec<String>,
    /// Whether what is asked for is the tree, rather than the graph or the entry's place.
    a_tree: bool,
}

impl Want {
    fn now() -> Want {
        let more = crate::resolver::reading_more_key();
        Want {
            base: here(),
            seen: crate::environment::key() + &more,
            more,
            look: crate::environment::look(),
            who: crate::environment::who(),
            names: crate::environment::names(),
            a_tree: false,
        }
    }

    fn a_tree() -> Want {
        Want { a_tree: true, ..Want::now() }
    }
}

impl Entry {
    fn new(text: &str, want: &Want) -> Entry {
        Entry {
            text: text.to_string(),
            base: want.base.clone(),
            seen: want.seen.clone(),
            more: want.more.clone(),
            look: want.look.clone(),
            who: want.who.clone(),
            names: want.names.clone(),
            tree_leans: None,
            graph_leans: None,
            nodes: None,
            graph: None,
            bytes: 0,
            used: 0,
            mounts: Vec::new(),
            clock_minute: None,
        }
    }

    /// What the tree holds of the environment, as far as is known: all of it, until both the tree
    /// and the graph have said.
    fn leans(&self) -> Leans {
        match (self.tree_leans, self.graph_leans) {
            (Some(tree), Some(graph)) => tree.or(graph),
            _ => Leans::ALL,
        }
    }

    /// Whether this serves whoever is asking for this text: `Some(true)` as it stands, worked out
    /// in their environment; `Some(false)` once its copies are worked out again as they see them.
    fn serves(&self, text: &str, want: &Want) -> Option<bool> {
        if self.text != text || self.base != want.base || self.more != want.more {
            return None;
        }
        let same_look = self.look == want.look;
        let same_who = self.who == want.who;
        if same_look && same_who && self.seen == want.seen {
            return Some(true);
        }
        if self.names != want.names {
            return None;
        }
        let leans = self.leans();
        if (!same_look && leans.look) || (!same_who && leans.me) {
            return None;
        }
        Some(false)
    }

    /// Said to be in the environment that one was worked out in.
    fn held_as(mut self, other: &Entry) -> Entry {
        self.seen = other.seen.clone();
        self.look = other.look.clone();
        self.who = other.who.clone();
        self.names = other.names.clone();
        self
    }

    /// Now whoever is asking's, its tree having been worked out, or worked out again, for them.
    fn become_theirs(&mut self, want: &Want) {
        self.seen = want.seen.clone();
        self.look = want.look.clone();
        self.who = want.who.clone();
        self.names = want.names.clone();
    }

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
    /// not.
    fn readable(&self) -> bool {
        if crate::access::asker() == crate::access::Asker::Anyone {
            return true;
        }
        self.mounts.iter().all(|(file, _)| crate::access::may_read(file))
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

/// The entry that serves whoever is asking, one worked out in their own environment first - see
/// `Entry::serves`.
fn serving(entries: &[Entry], text: &str, want: &Want) -> Option<(usize, bool)> {
    let mut shared = None;
    for (at, entry) in entries.iter().enumerate() {
        // A tree is wanted, and this one is out being worked on: one held as someone else saw it
        // serves better than none.
        if want.a_tree && entry.nodes.is_none() {
            continue;
        }
        match entry.serves(text, want) {
            Some(true) => return Some((at, true)),
            Some(false) if shared.is_none() => shared = Some((at, false)),
            _ => {}
        }
    }
    shared
}

/// Where the entry for this text, worked out in this folder, is - and nowhere, once a file its
/// mounts were read from has changed: then what it holds is let go, tree and graph, and whoever
/// asked works the document out from the files as though it had never been held. With whether
/// its tree stands as whoever asked sees it - see `Entry::serves`.
fn at(store: &mut Vec<Entry>, text: &str, want: &Want) -> Option<(usize, bool)> {
    let found = held_at(store, text, want)?;
    store[found.0].readable().then_some(found)
}

/// The same, whoever is asking: for putting something in the entry's place, which is no reading
/// of what it holds.
fn held_at(store: &mut Vec<Entry>, text: &str, want: &Want) -> Option<(usize, bool)> {
    let found = serving(store, text, want)?;
    if !store[found.0].mounts_unchanged() {
        supersede(store.remove(found.0));
        return None;
    }
    Some(found)
}

/// A tree handed to whoever is asking, worked out as they see it if it was held as someone else
/// saw it. Outside the store's lock: working anything out may want the store.
fn as_they_see_it(mut nodes: Vec<OverseerNode>, as_it_stands: bool) -> Vec<OverseerNode> {
    if !as_it_stands {
        crate::resolver::see_contexts_as_asked(&mut nodes);
    }
    nodes
}

/// Trees let go because a file their mounts were read from moved, kept a little longer for one
/// use: a page still shows such a tree, and a catch-up is described against what the page holds -
/// see `app_api::catch_up_at`. Without it the page is sent the whole document, and drawn again
/// from scratch, each time the bot writes to a file it mounts. Never handed out to work from.
static SUPERSEDED: std::sync::Mutex<Vec<Entry>> = std::sync::Mutex::new(Vec::new());
const SUPERSEDED_KEPT: usize = 2;

fn supersede(mut entry: Entry) {
    if entry.nodes.is_none() || budget_bytes() == 0 {
        return;
    }
    entry.graph = None;
    let mut kept = SUPERSEDED.lock().unwrap_or_else(|e| e.into_inner());
    kept.retain(|held| !(held.text == entry.text && held.base == entry.base && held.seen == entry.seen));
    kept.push(entry);
    let over = kept.len().saturating_sub(SUPERSEDED_KEPT);
    kept.drain(..over);
}

/// A copy of the tree this text produced in this folder from its mounts as they stood when
/// `reads` named them - see `app_api::reads_version` - held or superseded, if it is still kept.
pub fn worked_out_from(text: &str, reads: &str) -> Option<Vec<OverseerNode>> {
    let want = Want::now();
    let from = |entries: &[Entry]| {
        entries.iter().find_map(|entry| {
            let as_it_stands = entry.serves(text, &want)?;
            let nodes = entry.nodes.as_deref()?;
            let fits = crate::app_api::reads_version(&crate::actions::mounts_held(nodes)) == reads
                && entry.readable();
            fits.then(|| (nodes.to_vec(), as_it_stands))
        })
    };
    let found = with(|store| from(store))
        .or_else(|| from(&SUPERSEDED.lock().unwrap_or_else(|e| e.into_inner())));
    found.map(|(nodes, as_it_stands)| as_they_see_it(nodes, as_it_stands))
}

/// Keep the tree a text produced after the file has moved on from that text, for the same one use
/// as a tree let go for its mounts: a page still shows it.
///
/// A write moves what is held onto the text it wrote - see `rekey` - so the tree for the text
/// before is gone the moment it is written. Every other page open on the document then asked to
/// catch up from a text nobody held, and was sent the whole of it: 9.4 MB for the project tracker
/// each time the loop or the bot wrote one field, and every press made meanwhile the same.
///
/// What it holds of the environment is what the entry it was taken from said, which is still in
/// the store under that text when a write keeps it: without that it is held as the writer saw it,
/// and a page of someone else's could not catch up from it.
pub fn keep_superseded(text: &str, nodes: Vec<OverseerNode>) {
    if budget_bytes() == 0 || crate::access::refused_any() {
        return;
    }
    let want = Want::now();
    let leans = with(|store| {
        store
            .iter()
            .filter(|entry| entry.text == text && entry.base == want.base && entry.more == want.more)
            .filter(|entry| entry.names == want.names)
            .map(Entry::leans)
            .find(|leans| *leans != Leans::ALL)
    });
    let mut entry = Entry::new(text, &want);
    entry.mounts = read_from(&nodes);
    entry.nodes = Some(nodes);
    entry.used = now();
    entry.tree_leans = leans;
    entry.graph_leans = leans;
    supersede(entry);
}

/// A copy of a tree kept by `keep_superseded` or `supersede` for this text in this folder, and,
/// when `reads` names them, worked out from the mounts as they stood then - see
/// `app_api::reads_version`.
pub fn superseded_for(text: &str, reads: Option<&str>) -> Option<Vec<OverseerNode>> {
    let want = Want::now();
    let found = {
        let kept = SUPERSEDED.lock().unwrap_or_else(|e| e.into_inner());
        let entries: Vec<&Entry> = kept.iter().rev().collect();
        let fits = |entry: &Entry| {
            let nodes = entry.nodes.as_deref()?;
            let fits = reads.is_none_or(|reads| {
                crate::app_api::reads_version(&crate::actions::mounts_held(nodes)) == reads
            }) && entry.readable();
            fits.then(|| nodes.to_vec())
        };
        // One kept as whoever is asking saw it first, as everywhere else.
        entries
            .iter()
            .filter(|entry| entry.serves(text, &want) == Some(true))
            .find_map(|entry| fits(entry).map(|nodes| (nodes, true)))
            .or_else(|| {
                entries
                    .iter()
                    .filter(|entry| entry.serves(text, &want) == Some(false))
                    .find_map(|entry| fits(entry).map(|nodes| (nodes, false)))
            })
    };
    found.map(|(nodes, as_it_stands)| as_they_see_it(nodes, as_it_stands))
}

/// The same, and failing that the entry written to disk when it last left memory, brought back.
///
/// For whoever wants what was worked out, not for whoever is about to put something in its place.
/// Not trimmed on arrival, though it may not fit: the caller is about to take from it, and the next
/// thing put will make room.
fn found(store: &mut Vec<Entry>, text: &str, want: &Want) -> Option<(usize, bool)> {
    if let Some(found) = at(store, text, want) {
        return Some(found);
    }
    if budget_bytes() == 0 {
        return None;
    }
    let held = crate::document_disk::take(text, &want.base, &want.seen)?;
    let mut entry = Entry::new(text, want);
    entry.tree_leans = Some(leans_of_tree(&held.nodes, &want.names));
    entry.graph_leans = Some(leans_of_graph(&held.graph, &want.names));
    entry.nodes = Some(held.nodes);
    entry.graph = Some(held.graph);
    entry.used = now();
    entry.mounts = held.mounts;
    entry.clock_minute = held.clock_minute;
    if !entry.mounts_unchanged() {
        return None;
    }
    entry.measure();
    let readable = entry.readable();
    store.push(entry);
    readable.then_some((store.len() - 1, true))
}

/// The graph held for this text, lent to `look` rather than copied out.
///
/// Every edit, every press and every reopen that refreshes what reads the clock asks the graph one
/// question - what a change reaches - and each copied all of it first to ask: 5 ms on the project
/// tracker and 27 on the food tracker, spent on every one of them. Asked under the store's lock,
/// held for as long as the question takes and no longer.
///
/// The graph of a tree handed to anyone who sees the environment otherwise is theirs as well: it
/// names the same values, read from the same places.
pub fn with_graph<R>(text: &str, look: impl FnOnce(&crate::dependencies::Graph) -> R) -> Option<R> {
    let want = Want::now();
    with(|store| {
        let (found, _) = found(store, text, &want)?;
        store[found].used = now();
        store[found].graph.as_ref().map(look)
    })
}

/// Add what a later resolve recorded to the graph held for this text, where it is - see
/// `Graph::merge`. Nothing when no graph is held: what part of a document read is not a graph of
/// the document. The entry is measured again when it next moves, which every write makes it do.
pub fn add_to_graph(text: &str, newer: crate::dependencies::Graph) {
    let want = Want::now();
    with(|store| {
        if let Some((found, _)) = at(store, text, &want) {
            let entry = &mut store[found];
            if entry.graph.is_some() {
                let newly = leans_of_graph(&newer, &entry.names);
                entry.graph_leans = entry.graph_leans.map(|held| held.or(newly));
            }
            if let Some(graph) = entry.graph.as_mut() {
                graph.merge(newer);
            }
        }
    })
}

/// The document this text produced, as it last stood, without taking it.
pub fn nodes_for(text: &str) -> Option<Vec<OverseerNode>> {
    let want = Want::a_tree();
    let found = with(|store| {
        let (found, as_it_stands) = found(store, text, &want)?;
        store[found].used = now();
        store[found].nodes.clone().map(|nodes| (nodes, as_it_stands))
    });
    found.map(|(nodes, as_it_stands)| as_they_see_it(nodes, as_it_stands))
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
/// decides whether to take it at all. Lent as it was held, which may be as someone else saw it:
/// for a question about its shape, which is the same for anyone it is handed to.
pub fn with_nodes<R>(text: &str, look: impl FnOnce(&[OverseerNode]) -> R) -> Option<R> {
    let want = Want::a_tree();
    with(|store| {
        let (found, _) = found(store, text, &want)?;
        store[found].nodes.as_deref().map(look)
    })
}

/// The same, handed over rather than copied, with the minute it was worked out in.
///
/// For a caller that is about to replace it anyway: on a large document a copy costs more than the
/// diff it feeds.
///
/// One held as someone else saw it is also kept as it was, as `keep_superseded` keeps one: it is
/// what their page shows, and once the taker has written, the page catches up from it. Kept only
/// as the taker saw it, it would be worked out again as they see it for every such catch-up.
pub fn take_nodes(text: &str) -> Option<(Vec<OverseerNode>, WorkedOutIn)> {
    let want = Want::a_tree();
    let taken = with(|store| {
        let (found, as_it_stands) = found(store, text, &want)?;
        let entry = &mut store[found];
        entry.used = now();
        let taken = entry.nodes.take();
        let minute = entry.clock_minute.take();
        if taken.is_some() {
            entry.measure();
        }
        let as_held = (!as_it_stands).then(|| Entry {
            mounts: entry.mounts.clone(),
            tree_leans: entry.tree_leans,
            graph_leans: entry.graph_leans,
            used: now(),
            ..Entry::new(text, &want)
        }.held_as(entry));
        taken.map(|nodes| (nodes, minute, as_it_stands, as_held))
    });
    taken.map(|(nodes, minute, as_it_stands, as_held)| {
        if let Some(mut as_held) = as_held {
            if budget_bytes() > 0 && !crate::access::refused_any() {
                as_held.nodes = Some(nodes.clone());
                supersede(as_held);
            }
        }
        (as_they_see_it(nodes, as_it_stands), minute)
    })
}

/// The same, copied and left where it is: for a caller that changes nothing, so that another
/// arriving meanwhile finds it held rather than working the document out again.
pub fn copy_nodes(text: &str) -> Option<(Vec<OverseerNode>, WorkedOutIn)> {
    let want = Want::a_tree();
    let copied = with(|store| {
        let (found, as_it_stands) = found(store, text, &want)?;
        store[found].used = now();
        let minute = store[found].clock_minute;
        store[found].nodes.clone().map(|nodes| (nodes, minute, as_it_stands))
    });
    copied.map(|(nodes, minute, as_it_stands)| (as_they_see_it(nodes, as_it_stands), minute))
}

/// What this text's values were worked out from.
pub fn graph_for(text: &str) -> Option<crate::dependencies::Graph> {
    let want = Want::now();
    with(|store| {
        let (found, _) = found(store, text, &want)?;
        store[found].used = now();
        store[found].graph.clone()
    })
}

/// Put something in the entry for this text, as whoever is asking worked it out.
///
/// An entry held as someone else saw it is theirs from now on when what is put is the tree, or it
/// holds no tree just now - taken to be worked on, and coming back as this. One that still holds
/// a tree as someone else saw it is left to them, and this goes in an entry of its own.
fn put(text: &str, replaces_the_tree: bool, work: impl FnOnce(&mut Entry, &Want)) {
    // Worked out without a file its asker may not read, it is not the document as anyone else
    // would be shown it - see `access::refused_any`.
    if crate::access::refused_any() {
        return;
    }
    let want = Want::now();
    with(|store| {
        let held = held_at(store, text, &want).filter(|(found, as_it_stands)| {
            *as_it_stands || replaces_the_tree || store[*found].nodes.is_none()
        });
        let found = match held {
            Some((found, _)) => found,
            None => {
                store.push(Entry::new(text, &want));
                store.len() - 1
            }
        };
        let entry = &mut store[found];
        entry.become_theirs(&want);
        entry.used = now();
        work(entry, &want);
        entry.measure();
        trim(store);
    })
}

/// Every file a tree was worked out from besides its own text and the environment: what its
/// mounts read and the bases its templates came from. The environment is told by what it says -
/// see `Entry::look` - rather than by the files it was read from, which are each person's own.
fn read_from(nodes: &[OverseerNode]) -> Vec<(String, String)> {
    let mut files = crate::actions::mounts_held(nodes);
    for held in crate::bases::held(nodes) {
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
    put(text, true, |entry, want| {
        entry.mounts = read_from(nodes);
        entry.tree_leans = Some(leans_of_tree(nodes, &want.names));
        entry.nodes = Some(nodes.to_vec());
        entry.clock_minute = worked_out_in;
    });
}

/// The same, when the caller has a document it no longer needs.
pub fn own_nodes(text: &str, nodes: Vec<OverseerNode>, worked_out_in: WorkedOutIn) {
    if budget_bytes() == 0 {
        return;
    }
    put(text, true, |entry, want| {
        entry.mounts = read_from(&nodes);
        entry.tree_leans = Some(leans_of_tree(&nodes, &want.names));
        entry.nodes = Some(nodes);
        entry.clock_minute = worked_out_in;
    });
}

pub fn put_graph(text: &str, graph: crate::dependencies::Graph) {
    if budget_bytes() == 0 {
        return;
    }
    put(text, false, |entry, want| {
        entry.graph_leans = Some(leans_of_graph(&graph, &want.names));
        entry.graph = Some(graph);
    });
}

/// Take the graph held for this text away, and hand it over.
///
/// For a graph that must not be moved onto another text by `rekey`, or must not be kept at all:
/// after a change of shape the one held describes a document that no longer exists.
pub fn take_graph(text: &str) -> Option<crate::dependencies::Graph> {
    let want = Want::now();
    with(|store| {
        let (found, _) = at(store, text, &want)?;
        let graph = store[found].graph.take();
        store[found].measure();
        graph
    })
}

/// The document has been written out afresh; what is held still describes it, under its new text.
///
/// The old text is named because there may now be several entries and only one of them moved -
/// and only the one in the caller's folder, since the same text elsewhere was not written.
///
/// What the graph said the values read moves with it, taken out or not: a write that takes the
/// graph to follow a change of shape puts it back under the new text, and what it then says is
/// learned again - see `put_graph`. One worked out whole without recording says it has nothing
/// to put back - see `reads_unknown`.
pub fn rekey(was: &str, now_text: &str) {
    let want = Want::now();
    with(|store| {
        // Whatever was already held for the new text is superseded by what is being moved onto it.
        if was != now_text {
            if let Some((stale, _)) = at(store, now_text, &want) {
                store.remove(stale);
            }
        }
        if let Some((found, _)) = at(store, was, &want) {
            let entry = &mut store[found];
            entry.text = now_text.to_string();
            entry.used = now();
            entry.measure();
        }
        trim(store);
    })
}

/// The document held for this text was worked out whole with nothing recorded, and what its values
/// read of the environment is not known: it is held as whoever worked it out saw it.
pub fn reads_unknown(text: &str) {
    let want = Want::now();
    with(|store| {
        if let Some((found, _)) = at(store, text, &want) {
            store[found].graph_leans = None;
        }
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
    });
    SUPERSEDED.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

/// Drop every graph, keeping the documents.
pub fn forget_graphs() {
    with(|store| {
        for entry in store.iter_mut() {
            entry.graph = None;
            entry.graph_leans = None;
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
