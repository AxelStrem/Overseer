//! Where the memory of a worked-out document goes: a measurement, not a check.
//!
//! The documents cache decides what it keeps from `document_cache::footprint` and
//! `Graph::footprint`, which are estimates. This opens a real document the way the server does -
//! parse, mounts, a recorded resolve - under an allocator that counts every byte, says how far the
//! estimates are from what was really allocated, and breaks the tree and the graph down into what
//! holds the bytes, and how much of it repeats.
//!
//! Ignored, because it needs a real document and prints rather than asserts. Run it on a copy:
//!
//! ```text
//! OVERSEER_TREESIZE_DOC="C:/copy/ap31/projects/overseer.os;C:/copy/ap31/tasks.os" \
//!     cargo test --release --test where_a_worked_out_documents_memory_goes -- --ignored --nocapture
//! ```

use overseer::dependencies::Graph;
use overseer::types::{OverseerNode, OverseerValue};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc(layout);
        if !p.is_null() {
            let now = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        System.dealloc(p, layout);
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let q = System.realloc(p, layout, new_size);
        if !q.is_null() {
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
            let now = LIVE.fetch_add(new_size, Ordering::Relaxed) + new_size;
            PEAK.fetch_max(now, Ordering::Relaxed);
        }
        q
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn live() -> usize {
    LIVE.load(Ordering::Relaxed)
}

fn mb(bytes: usize) -> String {
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

fn count_nodes(nodes: &[OverseerNode]) -> usize {
    nodes.iter().map(|n| 1 + count_nodes(&n.children)).sum()
}

/// What one kind of thing holds: how many, and the bytes they own.
#[derive(Default, Clone, Copy)]
struct Tally {
    count: usize,
    bytes: usize,
}

impl Tally {
    fn add(&mut self, bytes: usize) {
        self.count += 1;
        self.bytes += bytes;
    }
}

/// Strings counted twice over: all of them, and each distinct one once - what interning would hold.
#[derive(Default)]
struct Repeats {
    all: Tally,
    distinct: HashMap<String, usize>,
}

impl Repeats {
    fn add(&mut self, s: &str, capacity: usize) {
        self.all.add(capacity);
        *self.distinct.entry(s.to_string()).or_default() += 1;
    }
    /// Bytes an `Arc<str>` per distinct string would hold, with its two counts.
    fn interned(&self) -> usize {
        self.distinct.keys().map(|s| s.len() + 16).sum()
    }
}

#[derive(Default)]
struct Breakdown {
    nodes: usize,
    structs: usize,
    children_slack: usize,
    names: Tally,
    types: Tally,
    templates: Tally,
    other_strings: Tally,
    param_entries: Tally,
    param_keys: Repeats,
    formulas: Repeats,
    strings: Repeats,
    dates: Tally,
    order_entries: Tally,
    order_keys: Tally,
    by_key: HashMap<String, Tally>,
    /// Parameters the engine writes for itself, `_computed_value` and its kind.
    computed: Tally,
    snapshots_distinct: HashSet<usize>,
    nodes_with_snapshot: usize,
    texts: HashSet<usize>,
    text_bytes: usize,
    snapshot_strings: usize,
    /// Every subtree, by a hash of everything it holds, and the bytes of one copy of it.
    subtrees: HashMap<u64, (usize, usize)>,
}

fn value_bytes(value: &OverseerValue) -> usize {
    use OverseerValue::*;
    match value {
        String(s) | Date(s) | Timestamp(s) | Formula(s) | Template(s) => s.capacity(),
        _ => 0,
    }
}

/// The bytes a node owns itself, leaving its children and its shared snapshot out.
fn own_bytes(node: &OverseerNode) -> usize {
    let entry = std::mem::size_of::<(String, OverseerValue)>();
    let mut total = std::mem::size_of::<OverseerNode>()
        + node.name.capacity()
        + node.node_type.capacity()
        + node.template.as_ref().map_or(0, |s| s.capacity())
        + node.raw_value_literal.as_ref().map_or(0, |s| s.capacity())
        + node.source_id.as_ref().map_or(0, |s| s.capacity())
        + node.parameters.len() * entry
        + node.param_order.capacity() * std::mem::size_of::<String>();
    for (k, v) in &node.parameters {
        total += k.capacity() + value_bytes(v);
    }
    for k in &node.param_order {
        total += k.capacity();
    }
    total
}

/// A hash of a subtree - type, name, template, parameters in order, children - and its bytes.
fn walk_subtree(node: &OverseerNode, b: &mut Breakdown) -> (u64, usize) {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    node.node_type.hash(&mut h);
    node.name.hash(&mut h);
    node.template.hash(&mut h);
    for (k, v) in &node.parameters {
        k.hash(&mut h);
        format!("{:?}", v).hash(&mut h);
    }
    let mut bytes = own_bytes(node);
    for child in &node.children {
        let (ch, cb) = walk_subtree(child, b);
        ch.hash(&mut h);
        bytes += cb;
    }
    let hash = h.finish();
    let slot = b.subtrees.entry(hash).or_insert((0, bytes));
    slot.0 += 1;
    (hash, bytes)
}

fn walk(nodes: &[OverseerNode], capacity: usize, b: &mut Breakdown) {
    let entry = std::mem::size_of::<(String, OverseerValue)>();
    b.children_slack += (capacity - nodes.len()) * std::mem::size_of::<OverseerNode>();
    for node in nodes {
        b.nodes += 1;
        b.structs += std::mem::size_of::<OverseerNode>();
        b.names.add(node.name.capacity());
        b.types.add(node.node_type.capacity());
        if let Some(t) = &node.template {
            b.templates.add(t.capacity());
        }
        for s in [&node.raw_value_literal, &node.source_id].into_iter().flatten() {
            b.other_strings.add(s.capacity());
        }
        for (k, v) in &node.parameters {
            b.param_entries.add(entry);
            b.param_keys.add(k, k.capacity());
            let vb = value_bytes(v);
            match v {
                OverseerValue::Formula(s) => b.formulas.add(s, s.capacity()),
                OverseerValue::String(s) | OverseerValue::Template(s) => b.strings.add(s, s.capacity()),
                OverseerValue::Date(s) | OverseerValue::Timestamp(s) => b.dates.add(s.capacity()),
                _ => {}
            }
            b.by_key.entry(k.clone()).or_default().add(entry + k.capacity() + vb);
            if k.starts_with('_') {
                b.computed.add(entry + k.capacity() + vb);
            }
        }
        b.order_entries.add(node.param_order.capacity() * std::mem::size_of::<String>());
        for k in &node.param_order {
            b.order_keys.add(k.capacity());
        }
        if let Some(snap) = &node.source_snapshot {
            b.nodes_with_snapshot += 1;
            if b.snapshots_distinct.insert(std::sync::Arc::as_ptr(snap) as usize) {
                b.snapshot_strings += std::mem::size_of::<overseer::types::NodeSourceSnapshot>()
                    + snap.leading_trivia.capacity()
                    + snap.trailing_trivia.capacity()
                    + snap.header.trailing.capacity()
                    + snap.indent_unit.as_ref().map_or(0, |s| s.capacity())
                    + snap.newline.as_ref().map_or(0, |s| s.capacity());
                if let Some((at, len)) = snap.full_text.backing() {
                    if b.texts.insert(at) {
                        b.text_bytes += len;
                    }
                }
            }
        }
        walk(&node.children, node.children.capacity(), b);
    }
}

fn row(what: &str, t: Tally, total: usize) {
    println!(
        "  {:<34} {:>9} {:>10} {:>5.1}%",
        what,
        t.count,
        mb(t.bytes),
        100.0 * t.bytes as f64 / total.max(1) as f64
    );
}

fn report_tree(nodes: &[OverseerNode], measured: usize) {
    let mut b = Breakdown::default();
    walk(nodes, nodes.len(), &mut b);
    for n in nodes {
        walk_subtree(n, &mut b);
    }
    println!("tree: {} nodes, {} hold a snapshot, {} distinct snapshots", b.nodes, b.nodes_with_snapshot, b.snapshots_distinct.len());
    println!("  {:<34} {:>9} {:>10} {:>6}", "what", "count", "bytes", "of tree");
    let t = measured;
    row("node structs", Tally { count: b.nodes, bytes: b.structs }, t);
    row("children Vec slack", Tally { count: 0, bytes: b.children_slack }, t);
    row("names", b.names, t);
    row("node types", b.types, t);
    row("template paths", b.templates, t);
    row("raw literals, source ids", b.other_strings, t);
    row("parameter entries (key+value slot)", b.param_entries, t);
    row("parameter keys", b.param_keys.all, t);
    row("formula text", b.formulas.all, t);
    row("string values", b.strings.all, t);
    row("dates, timestamps", b.dates, t);
    row("param_order slots", b.order_entries, t);
    row("param_order keys", b.order_keys, t);
    row("snapshots (structs, trivia)", Tally { count: b.snapshots_distinct.len(), bytes: b.snapshot_strings }, t);
    row("source texts the snapshots view", Tally { count: b.texts.len(), bytes: b.text_bytes }, t);
    row("  of which engine params `_...`", b.computed, t);
    println!(
        "  interned: keys {} distinct, {} -> {}; formulas {} distinct of {}, {} -> {}; strings {} distinct of {}, {} -> {}",
        b.param_keys.distinct.len(),
        mb(b.param_keys.all.bytes),
        mb(b.param_keys.interned()),
        b.formulas.distinct.len(),
        b.formulas.all.count,
        mb(b.formulas.all.bytes),
        mb(b.formulas.interned()),
        b.strings.distinct.len(),
        b.strings.all.count,
        mb(b.strings.all.bytes),
        mb(b.strings.interned()),
    );
    let mut keys: Vec<_> = b.by_key.iter().collect();
    keys.sort_by(|a, b| b.1.bytes.cmp(&a.1.bytes));
    println!("  costliest parameters (slot + key + value):");
    for (k, tally) in keys.iter().take(15) {
        println!("    {:<30} {:>9} {:>10}", k, tally.count, mb(tally.bytes));
    }
    let mut formulas: Vec<_> = b.formulas.distinct.iter().collect();
    formulas.sort_by(|a, b| (b.1 * b.0.len()).cmp(&(a.1 * a.0.len())));
    println!("  most repeated formula text (copies x length):");
    for (f, n) in formulas.iter().take(8) {
        let shown: String = f.chars().take(70).collect();
        println!("    {:>6} x {:>4} B  {}", n, f.len(), shown.replace('\n', " "));
    }
    // A subtree that occurs more than once could be one shared copy: its repeats are what sharing
    // whole, unchanged subtrees would save, an upper bound on sharing template parts.
    let largest_repeats: usize = {
        // Counted from the top down, so a repeat inside a repeat is not counted twice.
        fn top(nodes: &[OverseerNode], b: &Breakdown, seen: &mut HashSet<u64>) -> usize {
            let mut saved = 0;
            for n in nodes {
                let mut probe = Breakdown::default();
                let (h, bytes) = walk_subtree(n, &mut probe);
                if b.subtrees.get(&h).is_some_and(|(c, _)| *c > 1) {
                    if !seen.insert(h) {
                        saved += bytes;
                    }
                    continue;
                }
                saved += top(&n.children, b, seen);
            }
            saved
        }
        top(nodes, &b, &mut HashSet::new())
    };
    println!(
        "  identical subtrees: sharing every repeated one, top-down, would save {} ({:.0}% of the tree)",
        mb(largest_repeats),
        100.0 * largest_repeats as f64 / t.max(1) as f64
    );
}

fn report_graph(graph: &Graph, measured: usize) {
    let reads = graph.clone().into_reads();
    let mut paths = Repeats::default();
    let mut edges = 0;
    for (value, read) in &reads {
        paths.add(value, value.capacity());
        for p in read {
            paths.add(p, p.capacity());
            edges += 1;
        }
    }
    println!(
        "graph: {} values, {} edges; path strings in `reads` {} ({} distinct, {} interned); `read_by` holds the same again",
        reads.len(),
        edges,
        mb(paths.all.bytes),
        paths.distinct.len(),
        mb(paths.interned())
    );
    println!(
        "  both directions as u32 ids over one interned table would be about {} (against {} measured)",
        mb(paths.interned() + edges * 2 * 4 * 2 + reads.len() * 64),
        mb(measured)
    );
    let mut longest: Vec<_> = paths.distinct.keys().map(|s| s.len()).collect();
    longest.sort();
    if !longest.is_empty() {
        println!("  path length median {} B, longest {} B", longest[longest.len() / 2], longest[longest.len() - 1]);
    }
}

fn measure(path: &str) {
    let file = std::path::PathBuf::from(path);
    let folder = file.parent().unwrap().to_path_buf();
    // A mount is read relative to the working directory, as on the server run from its root.
    std::env::set_current_dir(&folder).unwrap();
    overseer::app_api::forget_baseline();
    overseer::app_api::forget_dependencies();
    let text = std::fs::read_to_string(&file).unwrap();
    println!("\n=== {} ({} KB of text) ===", path, text.len() / 1024);
    overseer::docmgr::manager::DocumentManager::with_document_file(Some(file.clone()), || {
        let _clock = overseer::formula_evaluator::FormulaEvaluator::pin_the_clock();
        let start = live();
        PEAK.store(start, Ordering::Relaxed);
        let (_, mut nodes) = overseer::parser::parse_document(&text).expect("parses");
        let parsed = live() - start;
        let parsed_nodes = count_nodes(&nodes);
        overseer::actions::ActionExecutor::preload_mounts(&mut nodes);
        let mounted = live() - start;
        overseer::dependencies::start_recording();
        overseer::resolver::resolve_document(&mut nodes);
        let graph = overseer::dependencies::take_recording();
        let resolved = live() - start;
        let peak = PEAK.load(Ordering::Relaxed) - start;
        let resolved_nodes = count_nodes(&nodes);

        let estimate_tree = overseer::document_cache::footprint(&nodes);
        let estimate_graph = graph.footprint();

        // What each holds, measured as what goes when it is dropped. A clone of the tree shares
        // the snapshots, so the clone's bytes are the tree without them.
        let before = live();
        let copy = nodes.clone();
        let tree_without_snapshots = live() - before;
        drop(copy);

        let before = live();
        let copy = graph.clone();
        let graph_bytes = live() - before;
        drop(copy);

        println!(
            "parse {} ({} nodes) | with mounts {} | resolved and recorded {} ({} nodes, {:.1}x the parsed count) | peak {}",
            mb(parsed),
            parsed_nodes,
            mb(mounted),
            mb(resolved),
            resolved_nodes,
            resolved_nodes as f64 / parsed_nodes.max(1) as f64,
            mb(peak)
        );
        println!(
            "sizes: node {} B, parameter slot {} B (value {} B), snapshot {} B",
            std::mem::size_of::<OverseerNode>(),
            std::mem::size_of::<(String, OverseerValue)>(),
            std::mem::size_of::<OverseerValue>(),
            std::mem::size_of::<overseer::types::NodeSourceSnapshot>()
        );
        report_graph(&graph, graph_bytes);
        report_tree(&nodes, tree_without_snapshots);

        // Everything the open left allocated, let go of piece by piece and measured as it goes,
        // while the tree is still held - so the registry's share is only what no tree holds, which
        // is what holding its snapshots weakly would save.
        let (parts_n, parts_est) = overseer::parts::read_parts_held();
        let (mounts_n, mounts_est) = overseer::actions::mounted_held();
        let (bases_n, bases_est) = overseer::bases::parsed_held();
        let mut pieces: Vec<(&str, usize)> = Vec::new();
        let before = live();
        overseer::parts::let_go_of_summary_reads();
        pieces.push(("parts read only for summaries (the server lets go of these)", before - live()));
        let before = live();
        overseer::source_registry::SourceRegistry::reset();
        let registry_only = before - live();
        pieces.push(("snapshots only the source registry holds", registry_only));
        let before = live();
        drop(graph);
        let graph_dropped = before - live();
        pieces.push(("the graph", graph_dropped));
        let before = live();
        drop(nodes);
        let tree_dropped = before - live();
        pieces.push(("the tree, with the snapshots only it holds", tree_dropped));
        let before = live();
        overseer::partsums::forget_in_memory();
        pieces.push(("part summaries", before - live()));
        let left = live() - start;
        pieces.push(("left: parts the window reads, mounts, bases, thread-locals", left));
        println!("where the {} the open left allocated is:", mb(resolved));
        for (what, bytes) in &pieces {
            println!(
                "  {:<62} {:>9} {:>4.0}%",
                what,
                mb(*bytes),
                100.0 * *bytes as f64 / resolved.max(1) as f64
            );
        }
        println!(
            "  caches' own estimates: parts {} in {}, mounts {} in {}, bases {} in {}",
            mb(parts_est),
            parts_n,
            mb(mounts_est),
            mounts_n,
            mb(bases_est),
            bases_n
        );
        println!(
            "estimates: tree {} ({:.0}% of measured), graph {} ({:.0}% of measured); together {}, {:.0}x the {} of text",
            mb(estimate_tree),
            100.0 * estimate_tree as f64 / tree_dropped.max(1) as f64,
            mb(estimate_graph),
            100.0 * estimate_graph as f64 / graph_dropped.max(1) as f64,
            mb(estimate_tree + estimate_graph),
            (estimate_tree + estimate_graph) as f64 / text.len().max(1) as f64,
            mb(text.len())
        );
    });
}

#[test]
#[ignore]
fn where_a_worked_out_documents_memory_goes() {
    let Ok(docs) = std::env::var("OVERSEER_TREESIZE_DOC") else {
        println!("OVERSEER_TREESIZE_DOC names no document; nothing measured");
        return;
    };
    for path in docs.split(';').filter(|p| !p.trim().is_empty()) {
        measure(path.trim());
    }
}
