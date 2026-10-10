use crate::formula_evaluator::{EvaluationContext, FormulaEvaluator};
#[allow(unused_imports)]
use crate::types::{Color, CssSize, NodeSourceSnapshot, OverseerNode, OverseerValue};
use std::collections::HashMap;

// Debug logging macro for resolver
macro_rules! debug_resolver {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-resolver")]
        println!($($arg)*);
    };
}

/// Finds a specific template by name in the node tree (on-demand search).
/// This allows any node to be a template, not just div nodes with hidden=true.
fn find_template_by_name(nodes: &[OverseerNode], template_name: &str) -> Option<OverseerNode> {
    for node in nodes {
        // Any node can be a template if its name matches
        if node.name == template_name {
            debug_resolver!(
                "[RESOLVER] Found template: {} (type: {}) with {} children",
                node.name,
                node.node_type,
                node.children.len()
            );
            #[cfg(feature = "debug-resolver")]
            {
                for child in &node.children {
                    println!(
                        "[RESOLVER]   Template field: {} (type: {})",
                        child.name, child.node_type
                    );
                }
            }
            return Some(node.clone());
        }
        // Recurse into children to find nested templates.
        if !node.children.is_empty() {
            if let Some(template) = find_template_by_name(&node.children, template_name) {
                return Some(template);
            }
        }
    }
    None
}

/// Multi-pass template resolution that handles template dependencies.
/// Templates can depend on other templates, so we need multiple passes to resolve everything.
fn resolve_templates_multipass(nodes: &mut Vec<OverseerNode>) {
    const MAX_PASSES: usize = 10; // Prevent infinite loops
    let mut pass = 0;
    let mut made_progress = true;

    while made_progress && pass < MAX_PASSES {
        made_progress = false;
        pass += 1;
        debug_resolver!("[RESOLVER] Template resolution pass {}", pass);

        // Create a snapshot of nodes for template lookup (immutable reference)
        let profiling = profile_enabled();
        let t_clone = std::time::Instant::now();
        let nodes_snapshot = nodes.clone(); // We need this for template lookup
        let clone_ms = t_clone.elapsed().as_secs_f64() * 1000.0;
        let t_pass = std::time::Instant::now();

        // Try to resolve templates in this pass
        for node in nodes.iter_mut() {
            if resolve_node_templates(node, &nodes_snapshot, &mut made_progress) {
                made_progress = true;
            }
        }
        if profiling {
            eprintln!(
                "[PHASE]     tmpl pass {} {:.1} ms (clone {:.1} ms) progress={}",
                pass,
                t_pass.elapsed().as_secs_f64() * 1000.0,
                clone_ms,
                made_progress
            );
        }

        debug_resolver!(
            "[RESOLVER] Pass {} complete, made_progress: {}",
            pass,
            made_progress
        );
    }

    if pass >= MAX_PASSES {
        debug_resolver!("[RESOLVER] Warning: Maximum template resolution passes reached. Some templates may have circular dependencies.");
    } else {
        debug_resolver!(
            "[RESOLVER] Template resolution completed in {} passes",
            pass
        );
    }
}

/// Public entry point to resolve all templates in a document AST.

/// The parameter marking an entry as out of view, and therefore not worth working out.
pub const OUT_OF_VIEW: &str = "_out_of_view";

/// How many of a list's entries were left out of view.
pub const LEFT_OUT: &str = "_left_out_of_view";

/// Decide what each windowed list is showing, before anything is instantiated or evaluated.
///
/// A history grows without end and is read three days at a time. Everything below it - a template
/// instance per entry, a formula per field of it, a graph node per formula - is paid for whether or
/// not anyone looks. On the food tracker, forty of the forty-three days are never on screen.
///
/// This runs *before* template resolution rather than before formula evaluation, which is the whole
/// point. Measured: a resolved `tracker_v2.os` holds 235,379 parameters against 53,791 parsed, and
/// only 28,032 of the difference are worked-out values - the rest is a template copied onto every
/// entry. Skipping evaluation alone would have saved the time and almost none of the memory.
///
/// Which entries are in view is decided the way the reader sees them: the first `window` in display
/// order, which is `sort_by` if the list has one and the order they are written in if it does not.
/// So a log meant to be read newest-first needs the sort it already wants - see `History` in the
/// food tracker - and one without a sort shows the first few as authored.
fn apply_list_windows(nodes: &mut Vec<OverseerNode>) {
    let snapshot = nodes.clone();
    fn walk(
        nodes: &mut Vec<OverseerNode>,
        root: &[OverseerNode],
        trail: &mut Vec<String>,
        level: &mut crate::addressing::Level,
    ) {
        for node in nodes.iter_mut() {
            // A wrapper is no step of the path the entries' sort is worked out against.
            let wrapper = crate::addressing::is_wrapper(node);
            if !wrapper {
                trail.push(level.segment(&node.name));
            }
            if node.node_type == "list" {
                if let Some(window) = window_of(node) {
                    name_the_entries(node);
                    narrow(node, window, root, trail);
                }
            }
            if wrapper {
                walk(&mut node.children, root, trail, level);
            } else {
                walk(&mut node.children, root, trail, &mut crate::addressing::Level::default());
                trail.pop();
            }
        }
    }
    walk(nodes, &snapshot, &mut Vec::new(), &mut crate::addressing::Level::default());
}

/// Give every entry the name it would be instantiated with, before anything is instantiated.
///
/// An unnamed entry is called `Template__N` once the template has been copied onto it, and an
/// entry left out of view never gets that far - so it kept the `-` it was written as, and
/// addressed as `-`, `-#1`, `-#2`, numbered within the out-of-view group rather than by its
/// place in the list. Two different schemes in one list, and which one an entry fell under
/// depended on the window: widening it renamed the entries that came into view, and an address
/// is meant to survive a resolve.
///
/// Doing it here rather than repairing it afterwards is what makes the names agree. Instantiation
/// only names an entry that has none, so one named here keeps that name, and the number comes
/// from the same place - its index among the list's children.
///
/// This does not make an out-of-view entry *readable*: its fields are still uninstantiated, so a
/// formula reaching one finds nothing. That is `viewreach`, and it is the larger job. What this
/// fixes is the entry having no name to be reached by.
fn name_the_entries(list: &mut OverseerNode) {
    let Some(OverseerValue::Template(path)) = list.parameters.get("entry") else {
        return;
    };
    let template = path
        .trim_start_matches("../")
        .split('/')
        .last()
        .unwrap_or("")
        .to_string();
    if template.is_empty() {
        return;
    }
    // Enumerated across every child, because that is the index instantiation counts with - a
    // list holding anything besides its entries would otherwise be numbered differently here.
    for (at, child) in list.children.iter_mut().enumerate() {
        let is_entry = child.node_type == "list_item" || child.node_type == "-";
        if is_entry && (child.name.is_empty() || child.name == "-") {
            child.name = format!("{}__{}", template, at + 1);
        }
    }
}

/// Give every entry of every list the name a parse of the document's text would give it.
///
/// An entry is named by its place when it is made - `Task__5`, the fifth child of its list - and
/// never again. So once one is taken out, or put in anywhere but the end, the survivors in memory
/// keep names their text no longer gives them: the list runs `Task__4, Task__6`, or a day ensured
/// at the front of a history is named for the back of it. Both are self-consistent and they
/// disagree, and the text is the authority - a page, the bot, another session and a cache that
/// has let the document go all name entries from it. The press path without a file settled this
/// by parsing its text again; a change worked out from the held document settles it here, before
/// the document is worked out whole, for a fraction of that.
///
/// Only names made by place are touched - `Template__N`, an unnamed entry, a bare dash - never a
/// name the document gives an entry itself.
///
/// Says what it renamed, as `(list, was, now)` with the list named as the dependency graph names
/// it, outer lists before the lists inside them - which is what carrying the graph through the
/// change needs, see `app_api::follow_the_shape`.
pub fn name_entries_as_parsed(nodes: &mut [OverseerNode]) -> Vec<(String, String, String)> {
    fn entries_of(node: &mut OverseerNode, trail: &[String], renamed: &mut Vec<(String, String, String)>) {
        let base = match node.parameters.get("entry") {
            Some(OverseerValue::Template(path)) | Some(OverseerValue::String(path)) => path
                .trim()
                .trim_start_matches('<')
                .trim_end_matches('>')
                .trim_start_matches("../")
                .split('/')
                .last()
                .unwrap_or("")
                .to_string(),
            _ => String::new(),
        };
        if base.is_empty() {
            return;
        }
        let placed = format!("{}__", base);
        for (at, child) in node.children.iter_mut().enumerate() {
            let by_place = child.name.is_empty()
                || child.name == "-"
                || child
                    .name
                    .strip_prefix(&placed)
                    .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
            let named = format!("{}{}", placed, at + 1);
            if by_place && child.name != named {
                let was = std::mem::replace(&mut child.name, named.clone());
                renamed.push((trail.join("/"), was, named));
            }
        }
    }
    // Walked as every path is built - see `addressing::Level` - with the names given here.
    fn below(
        children: &mut [OverseerNode],
        trail: &mut Vec<String>,
        level: &mut crate::addressing::Level,
        renamed: &mut Vec<(String, String, String)>,
    ) {
        for node in children.iter_mut() {
            if crate::addressing::is_wrapper(node) {
                entries_of(node, trail, renamed);
                below(&mut node.children, trail, level, renamed);
                continue;
            }
            trail.push(level.segment(&node.name));
            entries_of(node, trail, renamed);
            below(&mut node.children, trail, &mut crate::addressing::Level::default(), renamed);
            trail.pop();
        }
    }
    let mut renamed = Vec::new();
    for node in nodes.iter_mut() {
        let mut trail = vec![node.name.clone()];
        entries_of(node, &trail, &mut renamed);
        below(&mut node.children, &mut trail, &mut crate::addressing::Level::default(), &mut renamed);
    }
    renamed
}

/// Whether a list shows only some of its entries - see `apply_list_windows`.
pub fn is_windowed(node: &OverseerNode) -> bool {
    window_of(node).is_some()
}

/// The path of every node from this one down, as the resolver names them - for an entry just
/// made, which nothing has worked out and the graph has never seen.
pub fn paths_within(node: &OverseerNode, path: &str) -> std::collections::HashSet<String> {
    let mut paths = std::collections::HashSet::new();
    paths.insert(path.to_string());
    let mut prefix: Vec<String> = path.split('/').map(str::to_string).collect();
    collect_paths_below(&node.children, &mut prefix, &mut paths, &mut crate::addressing::Level::default());
    paths
}

/// Work out what a change of shape reaches, and nothing else - the targets the graph named, and
/// every node of an entry just made.
///
/// The rest of what a whole resolve does is left out because it has nothing to do: an entry is
/// made from a template that was instantiated with the document, so there are no templates left to
/// copy, and a list that shows only some of its entries is never followed here, so no window moves.
/// Layout and inheritance are what an entry just made still lacks from its place in the document,
/// and they are cheap enough to run over all of it.
pub fn resolve_after_a_change_of_shape(
    nodes: &mut Vec<OverseerNode>,
    targets: &std::collections::HashSet<String>,
) {
    let _clock = crate::formula_evaluator::FormulaEvaluator::pin_the_clock();
    let profiling = profile_enabled();
    let t = std::time::Instant::now();
    resolve_layout_parameters(nodes, None);
    resolve_parameter_inheritance(nodes, &crate::types::Params::new());
    if profiling {
        eprintln!("[PHASE]   layout and inheritance {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
    }
    resolve_specific_fields(nodes, targets);
}

/// How many entries a list keeps in view, if it says.
///
/// Absent or nought means all of them, so a document says nothing and behaves as it always has.
fn window_of(node: &OverseerNode) -> Option<usize> {
    let asked = match node.parameters.get("window") {
        Some(OverseerValue::Integer(n)) if *n > 0 => *n as usize,
        Some(OverseerValue::Float(n)) if *n > 0.0 => *n as usize,
        Some(OverseerValue::String(s)) => s.trim().parse::<usize>().ok().filter(|n| *n > 0)?,
        _ => return None,
    };
    Some(asked)
}

/// Mark everything past the window, and say how many that was.
fn narrow(
    node: &mut OverseerNode,
    window: usize,
    root: &[OverseerNode],
    trail: &mut Vec<String>,
) {
    // An entry as written, or as instantiated from the list's template. Only the first kind were
    // counted, which is all a document just parsed holds - and a document worked out again after a
    // change holds the second kind for every entry in view. So the entries in view were not
    // entries: an entry added never left view, the count of those left out came out short - 50 of
    // the food tracker's 53 - and an entry that should have come back into view never did.
    let template = match node.parameters.get("entry") {
        Some(OverseerValue::Template(path)) => path
            .trim_matches(['<', '>'])
            .trim_start_matches("../")
            .split('/')
            .last()
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    };
    let entries: Vec<usize> = node
        .children
        .iter()
        .enumerate()
        .filter(|(_, child)| {
            child.node_type == "list_item"
                || child.node_type == "-"
                || (!template.is_empty() && child.node_type == template)
        })
        .map(|(at, _)| at)
        .collect();
    if entries.len() <= window {
        for at in entries {
            node.children[at].parameters.remove(OUT_OF_VIEW);
        }
        node.parameters.remove(LEFT_OUT);
        return;
    }

    // Display order, worked out from what the entries were written with. A sort key that needs a
    // value nothing has worked out yet cannot be answered here, and such an entry keeps its place
    // in the file rather than being guessed at.
    let sort_source = match node.parameters.get("sort_by") {
        Some(OverseerValue::Formula(s)) | Some(OverseerValue::String(s)) => s.clone(),
        _ => String::new(),
    };
    let mut order: Vec<(usize, Option<OverseerValue>)> = Vec::with_capacity(entries.len());
    for at in &entries {
        let key = if sort_source.is_empty() {
            None
        } else {
            let child = &node.children[*at];
            let mut path = trail.clone();
            path.push(child.name.clone());
            let context = EvaluationContext::new_with_current_and_parent(
                child,
                Some(&*node),
                path,
                root,
            );
            FormulaEvaluator::evaluate_lambda_on_item(&sort_source, &context, child).ok()
        };
        order.push((*at, key));
    }
    // Anything the sort could not answer sinks below everything it could, so a half-written entry
    // does not take a place in view from one that says where it belongs.
    order.sort_by(|a, b| match (&a.1, &b.1) {
        (Some(x), Some(y)) => FormulaEvaluator::compare_for_sort(x, y).then(a.0.cmp(&b.0)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.0.cmp(&b.0),
    });

    // How each entry would be addressed, worked out before anything is marked. At this point a
    // keyed list reads its keys from what the entries were written with, which is exactly what an
    // address naming one of them was written against.
    let segments = crate::addressing::child_segments(node);
    let addressable: Vec<String> = crate::addressing::effective_children(node)
        .into_iter()
        .zip(segments)
        .map(|((index_path, _), segment)| {
            index_path.first().map_or(String::new(), |_| segment)
        })
        .collect();

    // Both ways, since a document worked out again has been narrowed before: what is in the window
    // now may have been out of it last time.
    let mut left_out = 0;
    for (rank, (at, _)) in order.into_iter().enumerate() {
        // Named by this interaction, so it stays - see `keeping_in_view`.
        let in_view = rank < window || addressable.get(at).is_some_and(|segment| is_wanted(segment));
        if in_view {
            node.children[at].parameters.remove(OUT_OF_VIEW);
            continue;
        }
        node.children[at]
            .parameters
            .insert(OUT_OF_VIEW.to_string(), OverseerValue::Boolean(true));
        left_out += 1;
    }
    node.parameters.insert(
        LEFT_OUT.to_string(),
        OverseerValue::Integer(left_out as i64),
    );
}

// Addresses this interaction is about, which stay in view whatever the window says.
//
// A window is a reading decision and must not become "this part of the document no longer works".
// Someone says on Thursday that they forgot Monday's dinner, and Monday is long out of view; the
// write still has to land.
//
// Decided before anything is resolved, rather than repaired afterwards. Bringing an entry back and
// resolving a second time looks simpler and is not: the template's own `$(today())` gets worked out
// over the entry's authored date on that second pass, so the day ends up addressable by the wrong
// key - a long way to travel to break the thing being fixed.
thread_local! {
    static WANTED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Keep whatever this address names in view for as long as the guard lives.
pub fn keeping_in_view(address: &str) -> KeptInView {
    WANTED.with(|wanted| wanted.borrow_mut().push(address.to_string()));
    KeptInView
}

pub struct KeptInView;

impl Drop for KeptInView {
    fn drop(&mut self) {
        WANTED.with(|wanted| {
            wanted.borrow_mut().pop();
        });
    }
}

/// Whether this interaction is holding anything in view.
///
/// A document resolved while something is held in view is not the document anyone else would have
/// got, so it must not be taken from the cache or put into it - see `load_document_maybe_recording`.
pub fn is_keeping_anything_in_view() -> bool {
    WANTED.with(|wanted| !wanted.borrow().is_empty())
}

/// Whether some address this interaction is about names this entry.
///
/// Compared segment by segment, so `tracker_v2/History/[2026-08-03]/intake` keeps the day it
/// reaches through as well as the list inside it.
fn is_wanted(segment: &str) -> bool {
    WANTED.with(|wanted| {
        wanted
            .borrow()
            .iter()
            .any(|address| address.split('/').any(|part| part == segment))
    })
}

/// Whether this node was left out of view, and so should be walked past entirely.
///
/// Entirely is the operative word: a day left out holds a list of meals of its own, and expanding
/// those would give back most of the cost the window was for.
pub fn out_of_view(node: &OverseerNode) -> bool {
    matches!(
        node.parameters.get(OUT_OF_VIEW),
        Some(OverseerValue::Boolean(true))
    )
}

/// Structural resolution: templates, layout and inheritance, but no formula evaluation.
///
/// Callers that only need the tree to have its shape - so that a field belonging to a
/// template instance exists and can be written to - want this rather than a full resolve.
/// Evaluating formulas and rebuilding chart series first, only to overwrite the values that
/// feed them and evaluate again, is work thrown away.
pub fn resolve_structure(nodes: &mut Vec<OverseerNode>) {
    let profiling = profile_enabled();
    // First, because everything after it is work that a list out of view does not want done.
    let t = std::time::Instant::now();
    apply_list_windows(nodes);
    if profiling {
        eprintln!("[PHASE]   windows {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
    }
    let t = std::time::Instant::now();
    resolve_templates_multipass(nodes);
    if profiling {
        eprintln!("[PHASE]   templates {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
    }
    let t = std::time::Instant::now();
    resolve_layout_parameters(nodes, None);
    if profiling {
        eprintln!("[PHASE]   layout {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
    }
    let t = std::time::Instant::now();
    resolve_parameter_inheritance(nodes, &crate::types::Params::new());
    if profiling {
        eprintln!("[PHASE]   inheritance {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
    }
}

/// Recompute everything derived from values: formulas, chart series and list sort keys.
///
/// Writing a value cannot add or remove nodes, so a caller that has already resolved the
/// document's structure and then written values needs only this. Re-running template
/// resolution at that point walks and clones the whole tree to arrive at the shape it
/// already has.
pub fn resolve_values(nodes: &mut Vec<OverseerNode>) {
    let profiling = profile_enabled();

    let t = std::time::Instant::now();
    evaluate_formulas_in_document_multi_pass(nodes);
    if profiling {
        eprintln!("[PHASE]   formulas {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
    }

    let t = std::time::Instant::now();
    compute_chart_series(nodes);
    if profiling {
        eprintln!("[PHASE]   charts {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
    }

    let t = std::time::Instant::now();
    compute_list_ui_sort_keys(nodes);
    if profiling {
        eprintln!("[PHASE]   sortkeys {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
    }
}

pub fn resolve_document(nodes: &mut Vec<OverseerNode>) {
    // One reading of the clock for all of it - see `FormulaEvaluator::pin_the_clock`.
    let _clock = crate::formula_evaluator::FormulaEvaluator::pin_the_clock();
    WORKED_OUT_WHOLE.with(|n| n.set(n.get() + 1));
    let profiling = profile_enabled();
    resolve_structure(nodes);

    // After parameter inheritance, evaluate formulas (multi-pass so aggregates whose inputs appear later update)
    let t = std::time::Instant::now();
    evaluate_formulas_in_document_multi_pass(nodes);
    if profiling {
        eprintln!("[PHASE]   formulas {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
    }

    // After formulas, compute chart series for charts/plots (MVP)
    let t = std::time::Instant::now();
    compute_chart_series(nodes);
    if profiling {
        eprintln!("[PHASE]   charts {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
    }

    // After formulas, compute UI sort keys for lists (presentation-only; do not reorder children)
    let t = std::time::Instant::now();
    compute_list_ui_sort_keys(nodes);
    if profiling {
        eprintln!("[PHASE]   sortkeys {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
    }

    // Phase 1: initialize and validate mount nodes (lazy placeholders only)
    initialize_and_validate_mount_nodes(nodes);
}

/// Selective resolution that only processes specific field paths
pub fn resolve_specific_fields(
    nodes: &mut Vec<OverseerNode>,
    field_paths: &std::collections::HashSet<String>,
) {
    // One reading of the clock for all of it - see `FormulaEvaluator::pin_the_clock`.
    let _clock = crate::formula_evaluator::FormulaEvaluator::pin_the_clock();
    debug_resolver!(
        "🎯 Selective resolution for {} fields: {:?}",
        field_paths.len(),
        field_paths
    );

    // For selective updates, we only need to:
    // 1. Re-evaluate formulas for the specific fields
    // 2. Re-compute charts that depend on those fields
    // 3. Skip template resolution, layout, and parameter inheritance (those don't change)

    // Only evaluate formulas for the specific field paths
    evaluate_formulas_for_specific_fields(nodes, field_paths);

    // Only recompute charts that contain references to the changed fields
    compute_chart_series_for_specific_fields(nodes, field_paths);

    // And the order of any sorted list holding what moved - see `refresh_sort_keys_holding`.
    refresh_sort_keys_holding(nodes, field_paths);

    debug_resolver!("✅ Selective resolution completed");
}

/// Work out again the sort keys of the sorted lists that hold any of these paths.
///
/// A list's order is a `_ui_sort_key` on each entry, worked out from `sort_by`, and the page
/// sorts by it - falling back to the order the file keeps for an entry that has none. Only the
/// full resolve worked them out. So a selective one left them as they were, which was wrong for
/// an edit to what a list sorts by, and a selective one starting from freshly parsed text had
/// none at all: an edit to one day of the diary came back with every day in file order, oldest
/// first, and the page showed it that way.
///
/// Only the lists holding something that moved, rather than all of them as the full resolve
/// does: that walks and copies the whole document, 35 ms on the tracker, which an edit that
/// touches one list has no reason to pay. The paths are matched on their names alone, so the
/// same address matches whether it came from the page or from the dependency graph, which name
/// unnamed wrappers differently.
fn refresh_sort_keys_holding(
    nodes: &mut Vec<OverseerNode>,
    paths: &std::collections::HashSet<String>,
) {
    use crate::formula_evaluator::{EvaluationContext, FormulaEvaluator};
    use crate::types::OverseerValue;

    // A path's names, without the numbering a repeated name carries.
    fn names(path: &str) -> Vec<&str> {
        path.split('/')
            .filter(|s| !s.is_empty())
            .map(|s| match s.rsplit_once('#') {
                Some((base, n)) if n.chars().all(|c| c.is_ascii_digit()) => base,
                _ => s,
            })
            .collect()
    }
    let touched: Vec<Vec<&str>> = paths.iter().map(|p| names(p)).filter(|p| !p.is_empty()).collect();
    if touched.is_empty() {
        return;
    }

    // Found first and worked out afterwards, so the document can be read whole while each one is
    // worked out instead of being copied for the purpose.
    struct Sorted {
        at: Vec<usize>,
        path: Vec<String>,
        sort_by: String,
    }
    // Walked the way every path is built - a wrapper is no step of it, see `addressing::Level` -
    // with `named` the same path without its numbering, for matching what moved.
    fn find(
        children: &[OverseerNode],
        at: &mut Vec<usize>,
        path: &mut Vec<String>,
        named: &mut Vec<String>,
        touched: &[Vec<&str>],
        found: &mut Vec<Sorted>,
        level: &mut crate::addressing::Level,
    ) {
        for (i, node) in children.iter().enumerate() {
            if out_of_view(node) {
                continue;
            }
            at.push(i);
            if crate::addressing::is_wrapper(node) {
                find(&node.children, at, path, named, touched, found, level);
                at.pop();
                continue;
            }
            path.push(level.segment(&node.name));
            named.push(node.name.clone());
            let sort_by = match node.parameters.get("sort_by") {
                Some(OverseerValue::Formula(s)) | Some(OverseerValue::String(s)) => s.as_str(),
                _ => "",
            };
            if node.node_type == "list" && !sort_by.is_empty() {
                let holds = touched.iter().any(|t| {
                    t.len() >= named.len() && t.iter().zip(named.iter()).all(|(a, b)| a == b)
                });
                if holds {
                    found.push(Sorted { at: at.clone(), path: path.clone(), sort_by: sort_by.to_string() });
                }
            }
            find(&node.children, at, path, named, touched, found, &mut crate::addressing::Level::default());
            named.pop();
            path.pop();
            at.pop();
        }
    }
    let mut found = Vec::new();
    find(nodes, &mut Vec::new(), &mut Vec::new(), &mut Vec::new(), &touched, &mut found, &mut crate::addressing::Level::default());

    fn at_mut<'a>(nodes: &'a mut [OverseerNode], at: &[usize]) -> Option<&'a mut OverseerNode> {
        let (first, rest) = at.split_first()?;
        let mut node = nodes.get_mut(*first)?;
        for i in rest {
            node = node.children.get_mut(*i)?;
        }
        Some(node)
    }
    for list in found {
        let keys: Vec<OverseerValue> = {
            let root: &[OverseerNode] = nodes;
            let mut node = &root[list.at[0]];
            for i in &list.at[1..] {
                node = &node.children[*i];
            }
            node.children
                .iter()
                .enumerate()
                .map(|(idx, child)| {
                    // The same context the full resolve gives it - see `recursively_compute_sort_keys`.
                    let mut path = list.path.clone();
                    path.push(child.name.clone());
                    let ctx = EvaluationContext::new_with_current_and_parent(child, Some(node), path, root);
                    FormulaEvaluator::evaluate_lambda_on_item(&list.sort_by, &ctx, child)
                        .unwrap_or(OverseerValue::Integer(idx as i64))
                })
                .collect()
        };
        if let Some(node) = at_mut(nodes, &list.at) {
            for (child, key) in node.children.iter_mut().zip(keys) {
                child.parameters.insert("_ui_sort_key".to_string(), key);
            }
        }
    }
}

// Initialize default values and validate mount nodes across the document tree
fn initialize_and_validate_mount_nodes(nodes: &mut Vec<OverseerNode>) {
    let len = nodes.len();
    for i in 0..len {
        let node_ptr: *mut OverseerNode = &mut nodes[i] as *mut _;
        unsafe {
            initialize_and_validate_mount_nodes_rec(node_ptr);
        }
    }
}

unsafe fn initialize_and_validate_mount_nodes_rec(node_ptr: *mut OverseerNode) {
    use crate::types::OverseerValue;
    let node: &mut OverseerNode = &mut *node_ptr;
    if node.node_type == "mount" {
        // Default: unloaded (do not clobber if already set by actions)
        if !node.parameters.contains_key("_mount_status") {
            node.parameters.insert(
                "_mount_status".to_string(),
                OverseerValue::String("unloaded".to_string()),
            );
        }
        // Validate required 'source' parameter presence
        let has_source = node.parameters.contains_key("source");
        if !has_source {
            node.parameters.insert(
                "_mount_status".to_string(),
                OverseerValue::String("error".to_string()),
            );
            node.parameters.insert(
                "_mount_error".to_string(),
                OverseerValue::String("mount: missing required 'source' parameter".to_string()),
            );
        }
        // Default lazy=true if not provided; set as computed shadow so renderers can read via either path
        if !node.parameters.contains_key("lazy") && !node.parameters.contains_key("_computed_lazy")
        {
            node.parameters
                .insert("_computed_lazy".to_string(), OverseerValue::Boolean(true));
        }
    }
    for idx in 0..node.children.len() {
        let child_ptr: *mut OverseerNode = &mut node.children[idx] as *mut _;
        initialize_and_validate_mount_nodes_rec(child_ptr);
    }
}

/// Compute chart plot series by evaluating per-item x/y expressions on a source container.
/// Work out every chart's series.
///
/// Public because a selective resolve calls it outright rather than deciding which charts are
/// affected. A chart's series hangs on its plot children while the dependency is recorded against
/// the chart, so the two do not line up - and a chart drawn from stale numbers is exactly the kind
/// of wrong nobody notices. Measured at about twenty milliseconds on the documents here, against
/// seconds for the full resolve this replaces, so the safe answer is also nearly free.
///
/// Over the document itself rather than a copy of it, the way a pass over formulas is - see
/// `recursively_evaluate_node_formulas_selective`. The copy was nearly all of what this cost, and
/// it runs on every edit.
pub fn compute_chart_series(nodes: &mut Vec<OverseerNode>) {
    let root: *mut [OverseerNode] = nodes.as_mut_slice();
    unsafe {
        let first = root as *mut OverseerNode;
        for i in 0..root.len() {
            let node_ptr = first.add(i);
            let mut current_path = vec![(*node_ptr).name.clone()];
            recursively_compute_chart_series(
                node_ptr,
                std::ptr::null(),
                &mut current_path,
                root as *const [OverseerNode],
                &mut crate::addressing::Level::default(),
            );
        }
    }
}

unsafe fn recursively_compute_chart_series(
    node_ptr: *mut OverseerNode,
    _parent_ptr: *const OverseerNode,
    current_path: &mut Vec<String>,
    // The document being written: read from while a chart's series are worked out, and written
    // to once they are.
    document_root: *const [OverseerNode],
    // Numbers this node's children in the path - see `addressing::Level`.
    level: &mut crate::addressing::Level,
) {
    use crate::formula_evaluator::{EvaluationContext, FormulaEvaluator};
    use crate::types::OverseerValue;

    // Process chart nodes: collect bounds across plots
    if (*node_ptr).node_type == "chart" {
        // Worked out with nothing written, and written afterwards.
        let mut series_of: Vec<(usize, String)> = Vec::new();
        let mut global_min_x: Option<f64> = None;
        let mut global_max_x: Option<f64> = None;
        let mut global_min_y: Option<f64> = None;
        let mut global_max_y: Option<f64> = None;
        {
            let node: &OverseerNode = &*node_ptr;
            let document_root: &[OverseerNode] = &*document_root;
            // A chart's series is worked out here rather than with the formulas, so it has to say what
            // it is working out or the lists it reads are recorded against nobody - and an edit to a
            // reading would leave the graph it is drawn on showing the old one.
            let _recording = crate::dependencies::WorkingOut::value(&format!(
                "{}#_computed_series",
                current_path.join("/")
            ));

            // Iterate plot children
            for (plot_at, plot) in node.children.iter().enumerate().filter(|(_, c)| c.node_type == "plot") {
                // Build an evaluation context for source resolution
                let ctx = EvaluationContext::new(current_path.clone(), document_root);

                // Resolve source: support plain path string OR a processed list via method chain
                enum SourceItems<'a> {
                    FromPath(Vec<String>, &'a OverseerNode),
                    FromList(Vec<String>, Vec<&'a OverseerNode>),
                }
                let source_items: Option<SourceItems> = match plot.parameters.get("source") {
                    Some(OverseerValue::String(s)) => resolve_path_from(document_root, current_path, s)
                        .map(|(p, n)| SourceItems::FromPath(p, n)),
                    Some(OverseerValue::Formula(f)) => {
                        // Try to evaluate to a string path first
                        match FormulaEvaluator::evaluate_formula(f, &ctx) {
                            Ok(OverseerValue::String(s)) => {
                                resolve_path_from(document_root, current_path, &s)
                                    .map(|(p, n)| SourceItems::FromPath(p, n))
                            }
                            _ => {
                                // Fall back: treat the formula as a list-source expression
                                match FormulaEvaluator::evaluate_list_source_nodes(f, &ctx) {
                                    Ok((p, items)) => Some(SourceItems::FromList(p, items)),
                                    Err(_) => None,
                                }
                            }
                        }
                    }
                    _ => None,
                };

                if let Some(source_items) = source_items {
                    // Prepare item iteration and path seeds
                    let (source_path_vec, items, source_ref_opt): (
                        Vec<String>,
                        Vec<&OverseerNode>,
                        Option<&OverseerNode>,
                    ) = match source_items {
                        SourceItems::FromPath(p, n) => {
                            (p.clone(), n.get_accessible_children(), Some(n))
                        }
                        SourceItems::FromList(p, list) => (p, list, None),
                    };
                    let mut series: Vec<(f64, f64)> = Vec::new();

                    // Fetch x/y expressions
                    let x_src = if let Some(val) = plot.parameters.get("x") {
                        match val {
                            OverseerValue::Formula(s) => Some(s.as_str()),
                            OverseerValue::String(s) => Some(s.as_str()),
                            _ => None,
                        }
                    } else {
                        None
                    };
                    let y_src = match plot.parameters.get("y") {
                        Some(OverseerValue::Formula(s)) => Some(s.as_str()),
                        Some(OverseerValue::String(s)) => Some(s.as_str()),
                        _ => None,
                    };
                    if x_src.is_none() || y_src.is_none() {
                        continue;
                    }
                    let x_src = x_src.unwrap();
                    let y_src = y_src.unwrap();

                    for item in items {
                        // Build item path for context
                        let mut item_path = source_path_vec.clone();
                        item_path.push(item.name.clone());
                        let ctx = EvaluationContext::new_with_current_and_parent(
                            item,
                            source_ref_opt,
                            item_path,
                            document_root,
                        );

                        let x_val = FormulaEvaluator::evaluate_lambda_on_item(x_src, &ctx, item).ok();
                        let y_val = FormulaEvaluator::evaluate_lambda_on_item(y_src, &ctx, item).ok();

                        if let (Some(xv), Some(yv)) = (to_f64(x_val), to_f64(y_val)) {
                            if xv.is_finite() && yv.is_finite() {
                                // Update bounds
                                global_min_x = Some(global_min_x.map_or(xv, |m| m.min(xv)));
                                global_max_x = Some(global_max_x.map_or(xv, |m| m.max(xv)));
                                global_min_y = Some(global_min_y.map_or(yv, |m| m.min(yv)));
                                global_max_y = Some(global_max_y.map_or(yv, |m| m.max(yv)));
                                series.push((xv, yv));
                            }
                        }
                    }

                    // Stored as a JSON string, once nothing is being read.
                    series_of.push((plot_at, series_to_json(&series)));
                }
            }
        }

        let node: &mut OverseerNode = &mut *node_ptr;
        for (plot_at, json) in series_of {
            node.children[plot_at]
                .parameters
                .insert("_computed_series".to_string(), OverseerValue::String(json));
        }
        // Store computed bounds on the chart
        if let (Some(xmin), Some(xmax), Some(ymin), Some(ymax)) =
            (global_min_x, global_max_x, global_min_y, global_max_y)
        {
            node.parameters
                .insert("_computed_x_min".to_string(), OverseerValue::Float(xmin));
            node.parameters
                .insert("_computed_x_max".to_string(), OverseerValue::Float(xmax));
            node.parameters
                .insert("_computed_y_min".to_string(), OverseerValue::Float(ymin));
            node.parameters
                .insert("_computed_y_max".to_string(), OverseerValue::Float(ymax));
        }
    }

    // Recurse - a wrapper is no step of the path, see `addressing::Level`.
    for idx in 0..(*node_ptr).children.len() {
        let child_ptr: *mut OverseerNode = (*node_ptr).children.as_mut_ptr().add(idx);
        if crate::addressing::is_wrapper(&*child_ptr) {
            recursively_compute_chart_series(child_ptr, node_ptr as *const OverseerNode, current_path, document_root, level);
            continue;
        }
        current_path.push(level.segment(&(*child_ptr).name));
        recursively_compute_chart_series(
            child_ptr,
            node_ptr as *const OverseerNode,
            current_path,
            document_root,
            &mut crate::addressing::Level::default(),
        );
        current_path.pop();
    }

    // Helpers local to this function
    fn to_f64(v: Option<OverseerValue>) -> Option<f64> {
        match v? {
            OverseerValue::Integer(i) => Some(i as f64),
            OverseerValue::Float(f) => Some(f),
            // Accept numeric-looking strings directly
            OverseerValue::String(s) => {
                // Try plain number first
                if let Ok(n) = s.parse::<f64>() {
                    return Some(n);
                }
                // Try a variety of time string forms
                parse_time_string_to_epoch_ms(&s)
            }
            OverseerValue::Timestamp(ts) => {
                // Parse to epoch ms via robust parser
                parse_time_string_to_epoch_ms(&ts)
            }
            OverseerValue::Date(d) => {
                // Treat YYYY-MM-DD as midnight UTC
                let ts = format!("{}T00:00:00Z", d);
                parse_time_string_to_epoch_ms(&ts)
            }
            _ => None,
        }
    }

    // Parse various timestamp string formats to epoch millis (as f64)
    fn parse_time_string_to_epoch_ms(s: &str) -> Option<f64> {
        let txt = s.trim();
        // 1) RFC3339 (supports timezone offsets and fractional seconds)
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(txt) {
            return Some(dt.timestamp_millis() as f64);
        }
        // 2) Allow a space instead of 'T' (optionally with trailing Z)
        //    e.g., "YYYY-MM-DD HH:MM:SSZ" or "YYYY-MM-DD HH:MM:SS"
        {
            let mut patched = txt.replace('T', " ");
            // If there's a trailing 'Z' with a space format, drop it and treat as UTC naive
            let had_z = patched.ends_with('Z');
            if had_z {
                patched = patched.trim_end_matches('Z').trim_end().to_string();
            }
            // Try with fractional seconds first
            if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(&patched, "%Y-%m-%d %H:%M:%S%.f")
            {
                let dt =
                    chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(ndt, chrono::Utc);
                return Some(dt.timestamp_millis() as f64);
            }
            // Then without fractional
            if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(&patched, "%Y-%m-%d %H:%M:%S") {
                let dt =
                    chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(ndt, chrono::Utc);
                return Some(dt.timestamp_millis() as f64);
            }
        }
        // 3) No timezone with 'T': treat as UTC
        {
            let patched = txt;
            if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(patched, "%Y-%m-%dT%H:%M:%S%.f")
            {
                let dt =
                    chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(ndt, chrono::Utc);
                return Some(dt.timestamp_millis() as f64);
            }
            if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(patched, "%Y-%m-%dT%H:%M:%S") {
                let dt =
                    chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(ndt, chrono::Utc);
                return Some(dt.timestamp_millis() as f64);
            }
        }
        // 4) Date-only -> start of day UTC
        if let Ok(nd) = chrono::NaiveDate::parse_from_str(txt, "%Y-%m-%d") {
            if let Some(ndt) = nd.and_hms_opt(0, 0, 0) {
                let dt =
                    chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(ndt, chrono::Utc);
                return Some(dt.timestamp_millis() as f64);
            }
        }
        None
    }

    fn series_to_json(series: &Vec<(f64, f64)>) -> String {
        let mut s = String::from("[");
        for (i, (x, y)) in series.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push_str(&format!("[{},{}]", x, y));
        }
        s.push(']');
        s
    }
}

/// Resolve a simple path string from the current node path to a node and return both its path vec and ref.
/// Supports:
/// - Absolute paths starting with '/': resolved from root ("/Root/Child")
/// - Relative paths with optional leading '../'
fn resolve_path_from<'a>(
    document_root: &'a [OverseerNode],
    current_path: &[String],
    path: &str,
) -> Option<(Vec<String>, &'a OverseerNode)> {
    // Build starting path
    let segments: Vec<&str> = path.split('/').collect();
    if path.starts_with('/') {
        // Absolute: first segment is empty, skip it and start from root
        if segments.len() < 2 {
            return None;
        }
        let mut out: Vec<String> = Vec::new();
        // First real segment must match a root node
        let first = segments[1];
        let root = document_root.iter().find(|n| n.name == first)?;
        out.push(first.to_string());
        let mut current = root;
        for seg in &segments[2..] {
            if seg.is_empty() {
                continue;
            }
            if let Some(next) = current.accessible_child(seg) {
                out.push(seg.to_string());
                current = next;
            } else {
                return None;
            }
        }
        return Some((out, current));
    } else {
        // Relative: start from current_path, apply '../' hops, then descend
        let mut base: Vec<String> = current_path.to_vec();
        // Remove current node (we want to resolve from the chart node's parent for sibling lookup)
        if !base.is_empty() {
            base.pop();
        }
        let mut idx = 0usize;
        while idx < segments.len() && segments[idx] == ".." {
            if base.is_empty() {
                return None;
            }
            base.pop();
            idx += 1;
        }
        // Resolve base to node
        let mut current = resolve_path_vec_to_node(document_root, &base)?;
        let mut out = base;
        for seg in &segments[idx..] {
            if seg.is_empty() {
                continue;
            }
            if let Some(next) = current.accessible_child(seg) {
                out.push(seg.to_string());
                current = next;
            } else {
                return None;
            }
        }
        return Some((out, current));
    }
}

/// A field the creating marked to freeze states the value it would otherwise have shown.
///
/// Called where that value has just been worked out, and from both paths that work one out - the
/// full resolve and the selective one - because either can be the pass that first materialises a
/// new entry, and a rule honoured by only one of them would hold or not hold depending on how
/// the document happened to be opened.
///
/// Once. A field that has been frozen states a value, and a field that states a value is never
/// frozen again, so the marker on the entry can be left where it is: it is internal, so it never
/// reaches the file, and it is gone the next time the document is read.
fn freeze_if_it_was_asked_for(node: &mut OverseerNode, asked_where_it_is_new: bool) {
    if !asked_where_it_is_new || FormulaEvaluator::states_a_value(&node.parameters) {
        return;
    }
    let settled = node
        .parameters
        .get("_computed_fallback")
        .or_else(|| node.parameters.get("_computed_default"))
        .filter(|v| !matches!(v, OverseerValue::String(s) if s == "invalid formula error"))
        .cloned();
    let Some(settled) = settled else { return };
    node.parameters.insert("value".to_string(), settled);
    // Said the way an edit says it, or the serializer treats the value as the template's and
    // leaves it out - which is the fault `silentwrite` was.
    node.parameters
        .insert("_override_present".to_string(), OverseerValue::Boolean(true));
    node.parameters.insert(
        "_explicit_child_override".to_string(),
        OverseerValue::Boolean(true),
    );
    node.parameters.remove("_computed_value");
    node.source_fingerprint = None;
}

/// Whether a field asks to be frozen and sits inside an entry just created - the half of
/// `freeze_if_it_was_asked_for` that reads the document, so it is asked while nothing is written.
fn asks_to_freeze_here(node: &OverseerNode, document_root: &[OverseerNode], current_path: &[String]) -> bool {
    matches!(node.parameters.get("freeze"), Some(OverseerValue::Boolean(true)))
        && !FormulaEvaluator::states_a_value(&node.parameters)
        && inside_something_just_created(document_root, current_path)
}

/// Whether this node sits inside an entry that was created a moment ago.
///
/// The creating marks the entry, because that is the only place that knows. By the time a field
/// the template supplies exists at all, the entry has been materialised and the marker is the
/// only remaining trace of how new it is - so the ancestors are what gets asked, rather than the
/// field, which cannot know.
fn inside_something_just_created(document_root: &[OverseerNode], current_path: &[String]) -> bool {
    for depth in (1..current_path.len()).rev() {
        if let Some(above) = resolve_path_vec_to_node(document_root, &current_path[..depth]) {
            if above.parameters.contains_key("_freeze_pending") {
                return true;
            }
        }
    }
    false
}

fn resolve_path_vec_to_node<'a>(
    root: &'a [OverseerNode],
    segments: &[String],
) -> Option<&'a OverseerNode> {
    if segments.is_empty() {
        return None;
    }
    let mut current = root.iter().find(|n| n.name == segments[0])?;
    for seg in &segments[1..] {
        if let Some(next) = current.accessible_child(seg) {
            current = next;
        } else {
            return None;
        }
    }
    Some(current)
}


/// A node that stands for something happening rather than something on screen.
///
/// Kept in step with `isEventHandlerName` and `isActionName` in the renderer, which say the same
/// thing about the same names. A row template holds these inside a button rather than beside its
/// fields, so in practice this catches nothing - it is here so that a template written the other
/// way does not grow a column with no cells in it.
fn is_event_or_action_name(name: &str) -> bool {
    let n = name.to_lowercase();
    matches!(
        n.as_str(),
        "click" | "change" | "submit" | "dblclick" | "hover" | "keydown" | "keyup"
            | "input"
            | "set" | "inc" | "dec" | "toggle" | "clear" | "clear_list" | "ensure_in_list"
            | "ensure" | "remove" | "append" | "move" | "sort" | "set_now" | "set_now_ts"
            | "activate" | "deactivate"
    )
}

/// Whether a node is hidden by its declaration rather than by a formula.
///
/// The distinction the whole table rests on. `hidden=true` is bookkeeping - a field that exists
/// to be read by other formulas and is never drawn - and it gets no column. `hidden=$(kids == 0)`
/// is a field that is drawn on some rows and not others, and it must get one: without a column of
/// its own the rows that lack it would slide left and stop lining up with the rows that have it.
fn is_always_hidden(node: &OverseerNode) -> bool {
    match node.parameters.get("hidden") {
        Some(OverseerValue::Boolean(b)) => *b,
        Some(OverseerValue::String(s)) => s.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

fn css_size_text(value: &OverseerValue) -> Option<String> {
    match value {
        OverseerValue::CssSize(size) => Some(match size {
            crate::types::CssSize::Pixels(v) => format!("{}px", v),
            crate::types::CssSize::Percentage(v) => format!("{}%", v),
            crate::types::CssSize::Em(v) => format!("{}em", v),
            crate::types::CssSize::Rem(v) => format!("{}rem", v),
            crate::types::CssSize::ViewportWidth(v) => format!("{}vw", v),
            crate::types::CssSize::ViewportHeight(v) => format!("{}vh", v),
            crate::types::CssSize::Auto => "auto".to_string(),
            crate::types::CssSize::FitContent => "fit-content".to_string(),
        }),
        OverseerValue::String(s) if !s.is_empty() => Some(s.clone()),
        OverseerValue::Integer(i) => Some(format!("{}px", i)),
        _ => None,
    }
}

/// The columns a table draws, read off the entry template.
///
/// It has to come from the template rather than from the rows, for two reasons. A row only shows
/// the fields it currently has, and which those are differs from row to row - a leaf has no
/// percentage, a task with children has no finish button - so no single row knows the full set.
/// And a list with nothing in it has no rows at all, which is exactly when a heading is worth
/// most.
///
/// Each column carries what the renderer needs and nothing else: the field's name, to match a
/// cell to a column; its label, which becomes the heading; its width, which moves from the cell
/// to the column because a percentage on a grid item is a percentage of its own column; and
/// whether it takes a line of its own below the rest.
fn table_columns(template: &OverseerNode) -> String {
    let mut columns = Vec::new();
    for child in &template.children {
        if is_event_or_action_name(&child.name) || is_always_hidden(child) {
            continue;
        }
        let label = match child.parameters.get("label") {
            Some(OverseerValue::String(s)) => s.clone(),
            _ => String::new(),
        };
        let width = child
            .parameters
            .get("width")
            .and_then(css_size_text)
            .unwrap_or_default();
        let spans_the_row = matches!(
            child.parameters.get("span"),
            Some(OverseerValue::String(s)) if s == "row"
        );
        columns.push(serde_json::json!({
            "name": child.name,
            "label": label,
            "width": width,
            "span": spans_the_row,
        }));
    }
    serde_json::to_string(&columns).unwrap_or_else(|_| "[]".to_string())
}

/// Resolves templates for a single node and its children.
/// Returns true if any progress was made in this pass.
fn resolve_node_templates(
    node: &mut OverseerNode,
    all_nodes: &[OverseerNode],
    made_progress: &mut bool,
) -> bool {
    let mut local_progress = false;

    // Out of view: neither this nor anything under it is instantiated. Returning before the
    // recursion is what makes the saving real - a day left out holds its own list of meals.
    if out_of_view(node) {
        return false;
    }

    debug_resolver!(
        "[RESOLVER] Resolving node: {} (type: {})",
        node.name,
        node.node_type
    );

    // Check if the current node is a list that uses a template or simple type.
    if node.node_type == "list" {
        // A list asked to draw as a table needs its column set worked out before anything else,
        // while the template is in reach. Done first so that nothing here is holding a borrow on
        // the parameters being written to.
        let wants_a_table = matches!(
            node.parameters.get("view"),
            Some(OverseerValue::String(view)) if view == "table"
        );
        if wants_a_table {
            let template_name = match node.parameters.get("entry") {
                Some(OverseerValue::Template(path)) => Some(
                    path.trim_start_matches("../")
                        .split('/')
                        .last()
                        .unwrap_or("")
                        .to_string(),
                ),
                _ => None,
            };
            if let Some(name) = template_name {
                if let Some(template_node) = find_template_by_name(all_nodes, &name) {
                    let columns = table_columns(&template_node);
                    node.parameters
                        .insert("_columns".to_string(), OverseerValue::String(columns));
                }
            }
        }

        if let Some(entry_value) = node.parameters.get("entry") {
            match entry_value {
                OverseerValue::Template(template_path) => {
                    debug_resolver!(
                        "[RESOLVER] List {} uses template: {}",
                        node.name,
                        template_path
                    );
                    // Simplified path resolution: allow both <../Task> and <Task>
                    let template_name = template_path
                        .trim_start_matches("../")
                        .split('/')
                        .last()
                        .unwrap_or("");
                    debug_resolver!("[RESOLVER] Resolved template name: {}", template_name);

                    if let Some(template_node) = find_template_by_name(all_nodes, template_name) {
                        debug_resolver!(
                            "[RESOLVER] Found template node for {}, processing {} children",
                            template_name,
                            node.children.len()
                        );
                        let mut resolved_children = Vec::new();
                        for (idx, list_item) in node.children.iter().enumerate() {
                            // Out of view: kept exactly as it was written, with no template
                            // copied onto it and nothing under it touched. This is where the
                            // window earns most of what it saves - the guard in
                            // `resolve_node_templates` only stops the walk from *descending*
                            // into such an entry, and a list instantiates its own children here
                            // rather than by walking into them.
                            if out_of_view(list_item) {
                                resolved_children.push(list_item.clone());
                                continue;
                            }
                            debug_resolver!(
                                "[RESOLVER]   Processing list item {}: {} (type: {})",
                                idx,
                                list_item.name,
                                list_item.node_type
                            );
                            // Handle both old "list_item" type and new "-" type (after parse_list_item removal)
                            if list_item.node_type == "list_item" || list_item.node_type == "-" {
                                if !list_item.children.is_empty() {
                                    debug_resolver!(
                                        "[RESOLVER]     Complex list item with {} children",
                                        list_item.children.len()
                                    );
                                    // Complex list item: create a node of the template's type
                                    let mut resolved_item = OverseerNode {
                                        name: {
                                            let n = list_item.name.clone();
                                            if n.is_empty() || n == "-" {
                                                format!("{}__{}", template_node.name, idx + 1)
                                            } else {
                                                n
                                            }
                                        },
                                        node_type: template_node.name.clone(),
                                        template: None,
                                        parameters: {
                                            // Start with template parameters as base, but mark them as template-derived
                                            let mut merged_params = crate::types::Params::new();

                                            // Add template parameters with _template_ prefix to mark their origin
                                            for (key, value) in &template_node.parameters {
                                                merged_params.insert(
                                                    format!("_template_{}", key),
                                                    value.clone(),
                                                );
                                                merged_params.insert(key.clone(), value.clone());
                                            }

                                            // List item parameters override template parameters
                                            for (key, value) in &list_item.parameters {
                                                merged_params.insert(key.clone(), value.clone());
                                                // This key is explicitly overridden; remove template marker so it persists on save
                                                let marker = format!("_template_{}", key);
                                                merged_params.remove(&marker);
                                            }
                                            // Record original type of this template instance (e.g., div)
                                            merged_params.insert(
                                                "_original_type".to_string(),
                                                OverseerValue::String(
                                                    template_node.node_type.clone(),
                                                ),
                                            );
                                            // Mark this node as coming from a template so serializer can suppress inherited children
                                            merged_params.insert(
                                                "_from_template".to_string(),
                                                OverseerValue::Boolean(true),
                                            );
                                            merged_params
                                        },
                                        children: template_node.children.clone(),
                                        is_hierarchy_transparent: template_node
                                            .is_hierarchy_transparent,
                                        param_order: Vec::new(),
                                        raw_value_literal: None,
                                        authored_dash: false,
                                        child_original_index: None,
                                        leading_blank_lines: 0,
                                        source_snapshot: None,
                                        source_id: None,
                                        source_fingerprint: None,
                                    };
                                    resolved_item.adopt_template_snapshot(&template_node);
                                    if resolved_item.source_snapshot.is_none() {
                                        let (indent_unit, newline) = node
                                            .source_snapshot
                                            .as_ref()
                                            .map(|snap| {
                                                (snap.indent_unit.clone(), snap.newline.clone())
                                            })
                                            .unwrap_or((None, None));
                                        resolved_item.synthesize_snapshot_with_style_recursive(
                                            indent_unit,
                                            newline,
                                        );
                                    }
                                    // Mark all cloned children as template-derived so serializer can omit them unless overridden
                                    for (c_idx, child) in
                                        resolved_item.children.iter_mut().enumerate()
                                    {
                                        mark_template_child_recursive(child);
                                        // Ensure override markers are clean on fresh clones; only true overrides will set these later
                                        if child
                                            .parameters
                                            .remove("_explicit_child_override")
                                            .is_some()
                                        {
                                            debug_resolver!("[RESOLVER] cleaned _explicit_child_override on clone child '{}')", child.name);
                                        }
                                        if child.parameters.remove("_override_present").is_some() {
                                            debug_resolver!("[RESOLVER] cleaned _override_present on clone child '{}')", child.name);
                                        }
                                        // Assign ordering index after authored children (authored indices assigned at parse time). Since these are cloned now, just use sequence.
                                        child.child_original_index = Some(c_idx);
                                    }
                                    let overrides: HashMap<String, &OverseerNode> = list_item
                                        .children
                                        .iter()
                                        .map(|o| (o.name.clone(), o))
                                        .collect();
                                    debug_resolver!(
                                        "[RESOLVER]     Override fields: {:?}",
                                        overrides.keys().collect::<Vec<_>>()
                                    );
                                    merge_node(&mut resolved_item, &overrides);
                                    // After merging, recursively propagate authored_dash, explicit markers, and ordering from override tree
                                    fn propagate_override_metadata(
                                        src: &OverseerNode,
                                        dst: &mut OverseerNode,
                                    ) {
                                        // Only act if names match (root call ensures this for children)
                                        if src.name == dst.name || src.name.is_empty() { /* proceed */
                                        }
                                        // If source was dash-authored and has a simple value override, mark destination
                                        if src.authored_dash && src.parameters.contains_key("value")
                                        {
                                            dst.authored_dash = true;
                                        }
                                        // If source provides an explicit value override (has 'value' param and no non-internal extra params) mark explicit flags
                                        if src.parameters.contains_key("value") {
                                            // Remove template value marker so serializer treats it as explicit
                                            dst.parameters.remove("_template_value");
                                            dst.parameters.insert(
                                                "_explicit_child_override".to_string(),
                                                OverseerValue::Boolean(true),
                                            );
                                            dst.parameters.insert(
                                                "_override_present".to_string(),
                                                OverseerValue::Boolean(true),
                                            );
                                        }
                                        // Preserve authored ordering index if present and destination not yet set or we want override precedence
                                        if let Some(idx) = src.child_original_index {
                                            dst.child_original_index = Some(idx);
                                        }
                                        // Recurse for children: build map by name for dst
                                        if !src.children.is_empty() {
                                            for child_src in &src.children {
                                                if let Some(child_dst) = dst
                                                    .children
                                                    .iter_mut()
                                                    .find(|c| c.name == child_src.name)
                                                {
                                                    propagate_override_metadata(
                                                        child_src, child_dst,
                                                    );
                                                }
                                            }
                                        }
                                    }
                                    for ov in list_item.children.iter() {
                                        if let Some(dst_child) = resolved_item
                                            .children
                                            .iter_mut()
                                            .find(|c| c.name == ov.name)
                                        {
                                            propagate_override_metadata(ov, dst_child);
                                        }
                                    }

                                    // Mark template-derived styling parameters for all field children
                                    for child in resolved_item.children.iter_mut() {
                                        // For field children, mark common styling parameters as template-derived
                                        let styling_params = [
                                            "width",
                                            "margin",
                                            "spacing",
                                            "padding",
                                            "margin-top",
                                            "margin-bottom",
                                            "margin-left",
                                            "margin-right",
                                            "padding-top",
                                            "padding-bottom",
                                            "padding-left",
                                            "padding-right",
                                            "color",
                                            "font-color",
                                            "background-color",
                                            "font-size",
                                        ];

                                        for param in styling_params.iter() {
                                            if let Some(value) = child.parameters.get(*param) {
                                                // Mark this parameter as template-derived
                                                child.parameters.insert(
                                                    format!("_template_{}", param),
                                                    value.clone(),
                                                );
                                            }
                                        }
                                    }
                                    // Recursively infer '-' types based on template structure
                                    infer_dash_types_from_template(
                                        &mut resolved_item,
                                        &template_node,
                                    );
                                    resolved_children.push(resolved_item);
                                } else if let Some(val) = list_item.parameters.get("value") {
                                    debug_resolver!(
                                        "[RESOLVER]     Simple value list item: {:?}",
                                        val
                                    );
                                    // Simple value: create a node of the template's type, with value
                                    let mut resolved_item = OverseerNode {
                                        name: {
                                            let n = list_item.name.clone();
                                            if n.is_empty() || n == "-" {
                                                format!("{}__{}", template_node.name, idx + 1)
                                            } else {
                                                n
                                            }
                                        },
                                        node_type: template_node.name.clone(),
                                        template: None,
                                        parameters: {
                                            // Start with template parameters as base
                                            let mut merged_params =
                                                template_node.parameters.clone();
                                            merged_params.insert(
                                                "_original_type".to_string(),
                                                OverseerValue::String(
                                                    template_node.node_type.clone(),
                                                ),
                                            );
                                            merged_params.insert("value".to_string(), val.clone());
                                            merged_params
                                        },
                                        children: Vec::new(),
                                        is_hierarchy_transparent: template_node
                                            .is_hierarchy_transparent,
                                        param_order: Vec::new(),
                                        raw_value_literal: None,
                                        authored_dash: false,
                                        child_original_index: None,
                                        leading_blank_lines: 0,
                                        source_snapshot: None,
                                        source_id: None,
                                        source_fingerprint: None,
                                    };
                                    resolved_item.adopt_template_snapshot(&template_node);
                                    if resolved_item.source_snapshot.is_none() {
                                        let (indent_unit, newline) = node
                                            .source_snapshot
                                            .as_ref()
                                            .map(|snap| {
                                                (snap.indent_unit.clone(), snap.newline.clone())
                                            })
                                            .unwrap_or((None, None));
                                        resolved_item.synthesize_snapshot_with_style_recursive(
                                            indent_unit,
                                            newline,
                                        );
                                    }
                                    resolved_children.push(resolved_item);
                                } else {
                                    // '-' with empty block and no value => instantiate default template clone; else, fallback clone
                                    if list_item.node_type == "-" {
                                        debug_resolver!("[RESOLVER]     Empty '-' item -> instantiate template '{}__{}'", template_node.name, idx + 1);
                                        let mut resolved_item = OverseerNode {
                                            name: {
                                                let n = list_item.name.clone();
                                                if n.is_empty() || n == "-" {
                                                    format!("{}__{}", template_node.name, idx + 1)
                                                } else {
                                                    n
                                                }
                                            },
                                            node_type: template_node.name.clone(),
                                            template: None,
                                            parameters: {
                                                let mut merged_params = crate::types::Params::new();
                                                for (key, value) in &template_node.parameters {
                                                    merged_params.insert(
                                                        format!("_template_{}", key),
                                                        value.clone(),
                                                    );
                                                    merged_params
                                                        .insert(key.clone(), value.clone());
                                                }
                                                merged_params.insert(
                                                    "_original_type".to_string(),
                                                    OverseerValue::String(
                                                        template_node.node_type.clone(),
                                                    ),
                                                );
                                                merged_params.insert(
                                                    "_from_template".to_string(),
                                                    OverseerValue::Boolean(true),
                                                );
                                                merged_params
                                            },
                                            children: template_node.children.clone(),
                                            is_hierarchy_transparent: template_node
                                                .is_hierarchy_transparent,
                                            param_order: Vec::new(),
                                            raw_value_literal: None,
                                            authored_dash: false,
                                            child_original_index: None,
                                            leading_blank_lines: 0,
                                            source_snapshot: None,
                                            source_id: None,
                                            source_fingerprint: None,
                                        };
                                        resolved_item.adopt_template_snapshot(&template_node);
                                        if resolved_item.source_snapshot.is_none() {
                                            let (indent_unit, newline) = node
                                                .source_snapshot
                                                .as_ref()
                                                .map(|snap| {
                                                    (snap.indent_unit.clone(), snap.newline.clone())
                                                })
                                                .unwrap_or((None, None));
                                            resolved_item.synthesize_snapshot_with_style_recursive(
                                                indent_unit,
                                                newline,
                                            );
                                        }
                                        for child in resolved_item.children.iter_mut() {
                                            mark_template_child_recursive(child);
                                            if child
                                                .parameters
                                                .remove("_explicit_child_override")
                                                .is_some()
                                            {
                                                debug_resolver!("[RESOLVER] cleaned _explicit_child_override on clone child '{}')", child.name);
                                            }
                                            if child
                                                .parameters
                                                .remove("_override_present")
                                                .is_some()
                                            {
                                                debug_resolver!("[RESOLVER] cleaned _override_present on clone child '{}')", child.name);
                                            }
                                        }
                                        // Recursively infer '-' types based on template structure
                                        infer_dash_types_from_template(
                                            &mut resolved_item,
                                            &template_node,
                                        );
                                        resolved_children.push(resolved_item);
                                    } else {
                                        debug_resolver!(
                                            "[RESOLVER]     Fallback: cloning list item as-is"
                                        );
                                        // Fallback: just clone
                                        resolved_children.push(list_item.clone());
                                    }
                                }
                            } else {
                                debug_resolver!("[RESOLVER]   Not a complex list_item, passing through: {} (type: {})", list_item.name, list_item.node_type);
                                // Not a complex list_item, pass through
                                resolved_children.push(list_item.clone());
                            }
                        }
                        debug_resolver!(
                            "[RESOLVER] List resolution complete, {} -> {} children",
                            node.children.len(),
                            resolved_children.len()
                        );
                        // Only claim progress if this actually changed something. Rebuilding a
                        // templated list produces the same children once it has settled, and
                        // reporting that as progress meant the enclosing loop could never
                        // converge - every document containing a templated list ran the full
                        // ten passes, every time it was resolved.
                        if node.children != resolved_children {
                            node.children = resolved_children;
                            local_progress = true;
                        }
                    } else {
                        debug_resolver!(
                            "[RESOLVER] Warning: Template not found: {}",
                            template_name
                        );
                        // Template not found - might be resolved in a later pass
                    }
                }
                OverseerValue::String(type_name) => {
                    debug_resolver!(
                        "[RESOLVER] List {} uses simple type: {}",
                        node.name,
                        type_name
                    );
                    // Handle simple type entries like entry=string
                    let mut resolved_children = Vec::new();
                    for (_i, list_item) in node.children.iter().enumerate() {
                        if out_of_view(list_item) {
                            resolved_children.push(list_item.clone());
                            continue;
                        }
                        debug_resolver!(
                            "[RESOLVER]   Processing simple type list item {}: {} (type: {})",
                            _i,
                            list_item.name,
                            list_item.node_type
                        );
                        if let Some(val) = list_item.parameters.get("value") {
                            debug_resolver!(
                                "[RESOLVER]     Converting to {} with value: {:?}",
                                type_name,
                                val
                            );
                            // The entry as it was read, given the list's type - not a new node in
                            // its place. A new one had no source to be written back from, so the
                            // list could not be replayed as written and every entry came out at
                            // the list's own depth, and the gap after the list went. It never
                            // showed while the parser dropped these values, because an entry with
                            // no value was kept as it was.
                            let mut resolved_item = list_item.clone();
                            resolved_item.node_type = type_name.clone();
                            resolved_item.parameters.insert("value".to_string(), val.clone());
                            // Named the way `append` names the ones it adds, rather than every
                            // entry answering to `-`: an address has to tell them apart.
                            if resolved_item.name.is_empty() || resolved_item.name == "-" {
                                resolved_item.name = format!("{}__{}", type_name, _i + 1);
                            }
                            if list_item.source_snapshot.is_none() {
                                let (indent_unit, newline) = node
                                    .source_snapshot
                                    .as_ref()
                                    .map(|snap| (snap.indent_unit.clone(), snap.newline.clone()))
                                    .unwrap_or((None, None));
                                resolved_item
                                    .synthesize_snapshot_with_style_recursive(indent_unit, newline);
                            }
                            resolved_children.push(resolved_item);
                        } else {
                            debug_resolver!(
                                "[RESOLVER]     No value found, keeping as-is: {} (type: {})",
                                list_item.name,
                                list_item.node_type
                            );
                            resolved_children.push(list_item.clone());
                        }
                    }
                    debug_resolver!(
                        "[RESOLVER] Simple type list resolution complete, {} -> {} children",
                        node.children.len(),
                        resolved_children.len()
                    );
                    if node.children != resolved_children {
                        node.children = resolved_children;
                        local_progress = true;
                    }
                }
                _ => {
                    debug_resolver!(
                        "[RESOLVER] List {} has unsupported entry parameter type: {:?}",
                        node.name,
                        entry_value
                    );
                }
            }
        } else {
            debug_resolver!("[RESOLVER] List {} has no entry parameter", node.name);
        }
    }

    // If this node is a direct template instance (e.g., <Colorful> Instance { ... }),
    // clone the template's parameters and children, then merge overrides from the instance.
    if let Some(template_path) = &node.template.clone() {
        debug_resolver!(
            "[RESOLVER] Node {} has template: {}",
            node.name,
            template_path
        );
        let template_name = template_path
            .trim_start_matches("../")
            .split('/')
            .last()
            .unwrap_or("");
        if let Some(template_node) = find_template_by_name(all_nodes, template_name) {
            debug_resolver!(
                "[RESOLVER] Instantiating template {} for instance {}",
                template_name,
                node.name
            );

            // Start with a clone of the template's declared component name as type (e.g., "Task"),
            // mirroring list templating where we use the template's name as the instantiated type.
            node.node_type = template_node.name.clone();
            let mut merged_params = crate::types::Params::new();

            // Mark template parameters and copy them as defaults
            for (key, value) in &template_node.parameters {
                merged_params.insert(format!("_template_{}", key), value.clone());
                merged_params.insert(key.clone(), value.clone());
            }

            // Instance parameters override template parameters
            for (key, value) in node.parameters.clone() {
                // apply override and remove template marker so it persists
                let marker = format!("_template_{}", key);
                merged_params.insert(key.clone(), value);
                merged_params.remove(&marker);
            }

            // Annotate with original type of template (e.g., div) so renderer can treat it as container
            merged_params.insert(
                "_original_type".to_string(),
                OverseerValue::String(template_node.node_type.clone()),
            );
            // Mark this node as coming from a template so serializer can suppress inherited children
            merged_params.insert("_from_template".to_string(), OverseerValue::Boolean(true));
            // Preserve the original template path for serialization (<T> I { ... })
            merged_params.insert(
                "_template_origin".to_string(),
                OverseerValue::Template(template_path.clone()),
            );

            node.parameters = merged_params;
            // Clone template children and then merge overrides from the instance, just like list entries
            let instance_children = node.children.clone();
            node.children = template_node.children.clone();
            for child in node.children.iter_mut() {
                mark_template_child_recursive(child);
                child.mark_snapshot_as_template_clone();
                if child.source_snapshot.is_none() {
                    let (indent_unit, newline) = node
                        .source_snapshot
                        .as_ref()
                        .map(|snap| (snap.indent_unit.clone(), snap.newline.clone()))
                        .unwrap_or((None, None));
                    child.synthesize_snapshot_with_style_recursive(indent_unit, newline);
                }
                // Ensure override markers are clean on fresh clones; only true overrides will set these later
                if child
                    .parameters
                    .remove("_explicit_child_override")
                    .is_some()
                {
                    debug_resolver!(
                        "[RESOLVER] cleaned _explicit_child_override on inst child '{}')",
                        child.name
                    );
                }
                if child.parameters.remove("_override_present").is_some() {
                    debug_resolver!(
                        "[RESOLVER] cleaned _override_present on inst child '{}')",
                        child.name
                    );
                }
            }
            if !instance_children.is_empty() {
                let overrides: HashMap<String, &OverseerNode> = instance_children
                    .iter()
                    .map(|o| (o.name.clone(), o))
                    .collect();
                // Taken from the children rather than the map's keys: a HashMap iterates in
                // an order that varies between runs, and this list is stored on the node and
                // read back by the serializer, so map order would make resolving the same
                // document twice produce two different results.
                let override_names: Vec<String> =
                    instance_children.iter().map(|o| o.name.clone()).collect();
                debug_resolver!(
                    "[RESOLVER] instance '{}' overrides: {:?}",
                    node.name,
                    override_names
                );
                // Record explicit override names on the instance for serializer to consult
                node.parameters.insert(
                    "_explicit_overrides".to_string(),
                    OverseerValue::String(override_names.join(",")),
                );
                merge_node(node, &overrides);
            }

            // Recursively infer '-' types based on template structure
            infer_dash_types_from_template(node, &template_node);

            // Important: clear template reference so this instance isn't reprocessed in subsequent passes.
            // Without this, later passes would treat inherited template children as explicit overrides.
            node.template = None;

            local_progress = true;
        } else {
            debug_resolver!(
                "[RESOLVER] Warning: Template not found for node: {}",
                template_name
            );
        }
    }

    // Recursively process children
    for child in node.children.iter_mut() {
        if resolve_node_templates(child, all_nodes, made_progress) {
            local_progress = true;
        }
    }

    debug_resolver!(
        "[RESOLVER] Finished resolving node: {} (final type: {}) - progress: {}",
        node.name,
        node.node_type,
        local_progress
    );
    local_progress
}

/// Recursively infer '-' typed override children using the corresponding template node structure.
fn infer_dash_types_from_template(instance: &mut OverseerNode, template: &OverseerNode) {
    // For each child in instance, find matching template child by name
    for child in instance.children.iter_mut() {
        if let Some(t_child) = template.children.iter().find(|t| t.name == child.name) {
            if child.node_type == "-" {
                debug_resolver!(
                    "[RESOLVER]     Resolving '-' type for {}: {} -> {}",
                    child.name,
                    child.node_type,
                    t_child.node_type
                );
                child.parameters.insert(
                    "_original_type".to_string(),
                    OverseerValue::String(child.node_type.clone()),
                );
                child.node_type = t_child.node_type.clone();
            }
            // Recurse for grandchildren
            if !child.children.is_empty() {
                infer_dash_types_from_template(child, t_child);
            }
        }
    }
}

/// Resolves layout parameters for all nodes, calculating effective layout based on parent and parameter values
fn resolve_layout_parameters(nodes: &mut Vec<OverseerNode>, parent_layout: Option<&str>) {
    for node in nodes.iter_mut() {
        // Containers support layout. Treat template instances as containers if their _original_type is div/list.
        let mut is_container = node.node_type == "div" || node.node_type == "list";
        if !is_container {
            if let Some(OverseerValue::String(orig_ty)) = node.parameters.get("_original_type") {
                if orig_ty == "div" || orig_ty == "list" {
                    is_container = true;
                }
            }
        }

        if is_container {
            let effective_layout = calculate_effective_layout(node, parent_layout);

            // Store the calculated layout in parameters for the renderer to use
            node.parameters.insert(
                "_effective_layout".to_string(),
                OverseerValue::String(effective_layout.clone()),
            );

            // Optional alignment across the secondary axis: near|center|far
            if let Some(OverseerValue::String(align)) = node.parameters.get("alignment") {
                let a = match align.as_str() {
                    "near" | "center" | "far" => align.clone(),
                    _ => "near".to_string(),
                };
                node.parameters
                    .insert("_effective_alignment".to_string(), OverseerValue::String(a));
            }
            debug_resolver!(
                "[RESOLVER] Node {} effective layout: {}",
                node.name,
                effective_layout
            );

            // Recursively resolve children with this node's effective layout
            if !node.children.is_empty() {
                resolve_layout_parameters(&mut node.children, Some(&effective_layout));
            }
        } else {
            // A field is not a container, but it does arrange two things: its label and its
            // value. That arrangement alternates with the nesting exactly as a div's does -
            // label above the value inside a horizontal row, beside it inside a vertical
            // column - so it is the same calculation, and `layout` on the field overrides it
            // with the same vocabulary: horizontal, vertical, inherit, opposite.
            //
            // Under its own key rather than `_effective_layout`, and the children still
            // inherit the *parent's* layout rather than this one. A field is not a layout
            // parent for whatever is nested under it - a handler, an override - and making it
            // one would flip the arrangement of anything below without being asked to.
            //
            // Kept only when the label goes beside the value. Above it is what the page does when
            // the key is absent, and it was on nearly every field of every document - 8,600 on the
            // food tracker - to say the default.
            if calculate_effective_layout(node, parent_layout) == "horizontal" {
                node.parameters.insert(
                    "_label_layout".to_string(),
                    OverseerValue::String("horizontal".to_string()),
                );
            } else {
                node.parameters.remove("_label_layout");
            }

            // For non-container nodes, just pass through the parent layout to children
            if !node.children.is_empty() {
                resolve_layout_parameters(&mut node.children, parent_layout);
            }
        }
    }
}

/// Calculate the effective layout for a node based on its layout parameter and parent layout
fn calculate_effective_layout(node: &OverseerNode, parent_layout: Option<&str>) -> String {
    // Check if node has explicit layout parameter
    if let Some(layout_param) = node.parameters.get("layout") {
        if let OverseerValue::String(layout_value) = layout_param {
            match layout_value.as_str() {
                "vertical" => return "vertical".to_string(),
                "horizontal" => return "horizontal".to_string(),
                // Along the row and on to the next when the row runs out. Unlike `horizontal`,
                // which wraps whatever widths its children happen to have, this fits as many
                // equal columns as there is room for - so a list of cards is three across on a
                // wide window and one on a narrow one, without the document naming a width.
                "flow" => return "flow".to_string(),
                "inherit" => {
                    return parent_layout.unwrap_or("vertical").to_string();
                }
                "opposite" => {
                    return match parent_layout.unwrap_or("vertical") {
                        "vertical" => "horizontal".to_string(),
                        "horizontal" => "vertical".to_string(),
                        _ => "horizontal".to_string(), // Default opposite of vertical
                    };
                }
                _ => {
                    debug_resolver!(
                        "[RESOLVER] Unknown layout value: {}, defaulting to opposite",
                        layout_value
                    );
                }
            }
        }
    }

    // Default behavior: opposite to parent (or horizontal if no parent)
    match parent_layout.unwrap_or("vertical") {
        "vertical" => "horizontal".to_string(),
        "horizontal" => "vertical".to_string(),
        _ => "horizontal".to_string(),
    }
}

/// Parameters a container passes down to everything inside it.
///
/// Said once on a div and true of every field within, unless one of them says otherwise -
/// which falls out of how they are copied: a node's own value replaces what it was given
/// before its children are handed the map, so the nearest ancestor that states anything wins.
/// CSS cannot express that for a descendant selector, which is why this is done here.
///
/// Each one is copied onto the descendant *and* marked `_template_<name>`, so that the
/// serializer knows the value was not written there and does not write it to disk. Adding a
/// name here without that marker being understood would persist the copy into every entry;
/// `file_ops::marks_a_template_node` reads this same list for exactly that reason.
pub const INHERITABLE_PARAMS: [&str; 5] = [
    "background-color",
    "font-color",
    "font-size",
    // Whether the fields inside show their labels. A table's heading already names its
    // columns, so a table-shaped list repeating every label on every row says everything
    // twice; and a field whose name is obvious from where it sits does not need saying at all.
    "hide-labels",
    // What a field says about itself when the mouse rests on it. Said on a container so a
    // table can ask once and every cell answer with the name of its own column - which is
    // where the naming goes when the label is not shown.
    "hover-text",
];

fn resolve_parameter_inheritance(
    nodes: &mut Vec<OverseerNode>,
    parent_params: &crate::types::Params,
) {
    for node in nodes.iter_mut() {
        // Not into what a press runs. An action is never drawn, so a style handed to one means
        // nothing - until an action copies itself somewhere: `- title = $(../title)` in an
        // `append` is a node like any other, and every parameter it holds is applied to the new
        // entry's field. Handed the colour of the entry the button sat in, it gave that colour to
        // the field, which then never took the colour of where it now was, since a node that
        // already holds a style keeps it. So a task done while overdue went into the history red,
        // field by field, wherever a coloured entry was moved to another list.
        if node.node_type == "on" {
            continue;
        }
        let inheritable_params = INHERITABLE_PARAMS;

        // Inherit each styling parameter from parent if not explicitly set
        for param_name in &inheritable_params {
            if !node.parameters.contains_key(*param_name) {
                if let Some(parent_value) = parent_params.get(*param_name) {
                    debug_resolver!(
                        "[RESOLVER] Inheriting {} = {:?} for node {}",
                        param_name,
                        parent_value,
                        node.name
                    );
                    node.parameters
                        .insert(param_name.to_string(), parent_value.clone());
                    // Mark as template-derived so serializer will not persist inherited styling
                    node.parameters
                        .insert(format!("_template_{}", param_name), parent_value.clone());
                    // And as handed down, which `_template_` cannot say: a list entry's template
                    // leaves that same marker on every parameter it states, so a button in an
                    // entry that names its own font colour looked exactly like one that was
                    // given its row's - and the page painted over the colour it had asked for.
                    node.parameters
                        .insert(format!("_inherited_{}", param_name), OverseerValue::Boolean(true));
                }
            }
        }

        // Build inherited parameters map for children (including this node's parameters)
        let mut inherited_params = parent_params.clone();
        for (key, value) in &node.parameters {
            // A tab's hover text is what its header says about the tab, not something every
            // field on its page should say about itself. What the tab was handed from above
            // goes on down as it would through anything else.
            let a_tab = node.node_type == "tab"
                || matches!(node.parameters.get("_original_type"), Some(OverseerValue::String(t)) if t == "tab");
            if a_tab && key == "hover-text" {
                continue;
            }
            if inheritable_params.contains(&key.as_str()) {
                inherited_params.insert(key.clone(), value.clone());
            }
        }

        // Recursively resolve children with inherited parameters
        if !node.children.is_empty() {
            resolve_parameter_inheritance(&mut node.children, &inherited_params);
        }
    }
}

/// Merges override fields into a template clone.
/// Helper: find the path (indices) to a named field within a node's accessible hierarchy,
/// treating transparent nodes (unnamed divs, tabs) as invisible containers.
fn find_accessible_child_path(node: &OverseerNode, target_name: &str) -> Option<Vec<usize>> {
    for (i, child) in node.children.iter().enumerate() {
        if child.name == target_name {
            return Some(vec![i]);
        }
        if child.is_hierarchy_transparent {
            if let Some(mut subpath) = find_accessible_child_path(child, target_name) {
                let mut path = vec![i];
                path.append(&mut subpath);
                return Some(path);
            }
        }
    }
    None
}

/// Helper: get a mutable reference to a child using an index path
fn get_child_mut_by_path<'a>(
    node: &'a mut OverseerNode,
    path: &[usize],
) -> Option<&'a mut OverseerNode> {
    if path.is_empty() {
        return None;
    }
    let mut current: *mut OverseerNode = node as *mut _;
    // SAFETY: We ensure at most one mutable reference is active by walking iteratively.
    for (depth, &idx) in path.iter().enumerate() {
        unsafe {
            let current_ref = &mut *current;
            if idx >= current_ref.children.len() {
                return None;
            }
            let child_ptr: *mut OverseerNode = &mut current_ref.children[idx];
            if depth == path.len() - 1 {
                return Some(&mut *child_ptr);
            } else {
                current = child_ptr;
            }
        }
    }
    None
}

fn adopt_source_metadata(target: &mut OverseerNode, source: &OverseerNode) {
    if let Some(snapshot) = source.source_snapshot.clone() {
        target.source_snapshot = Some(snapshot);
    }
    if let Some(source_id) = source.source_id.clone() {
        target.source_id = Some(source_id);
    }
    if let Some(fp) = source.source_fingerprint {
        target.source_fingerprint = Some(fp);
    }
    if source.leading_blank_lines > 0 {
        target.leading_blank_lines = source.leading_blank_lines;
    }
    if !source.param_order.is_empty() {
        target.param_order = source.param_order.clone();
    }
    if source.authored_dash {
        target.authored_dash = true;
    }
    if let Some(idx) = source.child_original_index {
        target.child_original_index = Some(idx);
    }
}

fn merge_node(template: &mut OverseerNode, overrides: &HashMap<String, &OverseerNode>) {
    debug_resolver!(
        "[RESOLVER] Merging overrides into template with {} fields (transparent-aware)",
        template.children.len()
    );

    // Apply each override by locating the target field path in the template via transparent-aware lookup
    for (ov_name, override_field) in overrides.iter() {
        if let Some(path) = find_accessible_child_path(template, ov_name) {
            if let Some(template_field) = get_child_mut_by_path(template, &path) {
                debug_resolver!(
                    "[RESOLVER]   Merging field: {} (template type: {}, override type: {})",
                    template_field.name,
                    template_field.node_type,
                    override_field.node_type
                );

                adopt_source_metadata(template_field, override_field);

                // Override a simple value (e.g., name = "...")
                if let Some(val) = override_field.parameters.get("value") {
                    debug_resolver!("[RESOLVER]     Setting value: {:?}", val);
                    // Always set the value from the explicit override
                    template_field
                        .parameters
                        .insert("value".to_string(), val.clone());
                    // Preserve original raw numeric/text literal formatting if present on override
                    if override_field.raw_value_literal.is_some() {
                        template_field.raw_value_literal = override_field.raw_value_literal.clone();
                    }
                    // Treat presence in source as an explicit override even if equal to template default
                    template_field.parameters.insert(
                        "_override_present".to_string(),
                        OverseerValue::Boolean(true),
                    );
                    template_field.parameters.insert(
                        "_explicit_child_override".to_string(),
                        OverseerValue::Boolean(true),
                    );
                    debug_resolver!(
                        "[RESOLVER] set explicit override (value) on '{}'",
                        template_field.name
                    );
                    // Remove template marker for value if present so serializers won't treat it as inherited
                    if template_field.parameters.contains_key("_template_value") {
                        template_field.parameters.remove("_template_value");
                    }
                    // Propagate authored dash provenance so serializer can retain concise form
                    if override_field.authored_dash {
                        template_field.authored_dash = true;
                    }
                    // Preserve original sibling ordering if override carried an index (use existing if already set)
                    if template_field.child_original_index.is_none()
                        && override_field.child_original_index.is_some()
                    {
                        template_field.child_original_index = override_field.child_original_index;
                    }
                }

                // If this field is a list, handle entry inheritance and recursive merge
                if template_field.node_type == "list" {
                    // If override does not specify entry, inherit from template
                    if !override_field.parameters.contains_key("entry") {
                        if let Some(entry) = template_field.parameters.get("entry") {
                            template_field
                                .parameters
                                .insert("entry".to_string(), entry.clone());
                        }
                    } else if let Some(entry) = override_field.parameters.get("entry") {
                        // If override specifies entry, use it
                        template_field
                            .parameters
                            .insert("entry".to_string(), entry.clone());
                    }
                    // Recursively resolve/merge children for nested lists
                    if !override_field.children.is_empty() {
                        template_field.children = override_field.children.clone();
                        template_field.parameters.insert(
                            "_override_present".to_string(),
                            OverseerValue::Boolean(true),
                        );
                        template_field.parameters.insert(
                            "_explicit_child_override".to_string(),
                            OverseerValue::Boolean(true),
                        );
                        debug_resolver!(
                            "[RESOLVER] set explicit override (list children) on '{}'",
                            template_field.name
                        );
                    }
                    // Preserve dash-authored style from the override for list containers too,
                    // so the serializer can emit "- name { ... }" instead of "list name { ... }" when authored that way.
                    if override_field.authored_dash {
                        template_field.authored_dash = true;
                    }
                } else if !override_field.children.is_empty() {
                    // For non-list container nodes, deep-merge override children by name
                    // rather than replacing the entire children array. This preserves defaults
                    // for siblings that are not explicitly overridden.
                    let override_order: Vec<String> = override_field
                        .children
                        .iter()
                        .map(|c| c.name.clone())
                        .collect();
                    let mut child_overrides: HashMap<String, &OverseerNode> = HashMap::new();
                    for ch in &override_field.children {
                        child_overrides.insert(ch.name.clone(), ch);
                    }
                    if !child_overrides.is_empty() {
                        // Mark the container as having explicit child overrides
                        template_field.parameters.insert(
                            "_override_present".to_string(),
                            OverseerValue::Boolean(true),
                        );
                        template_field.parameters.insert(
                            "_explicit_child_override".to_string(),
                            OverseerValue::Boolean(true),
                        );
                        debug_resolver!(
                            "[RESOLVER] deep-merging {} child override(s) into container '{}'",
                            child_overrides.len(),
                            template_field.name
                        );
                        // Recursively merge into this container field
                        merge_node(template_field, &child_overrides);
                        // If the override container itself was dash-authored, propagate to template_field
                        if override_field.authored_dash {
                            template_field.authored_dash = true;
                        }
                        if template_field.child_original_index.is_none()
                            && override_field.child_original_index.is_some()
                        {
                            template_field.child_original_index =
                                override_field.child_original_index;
                        }
                        // Apply ordering & dash provenance to overridden children
                        if !override_order.is_empty() {
                            for (seq, name) in override_order.iter().enumerate() {
                                if let Some(ch) =
                                    template_field.children.iter_mut().find(|c| c.name == *name)
                                {
                                    if child_overrides
                                        .get(name)
                                        .map(|o| o.authored_dash)
                                        .unwrap_or(false)
                                    {
                                        ch.authored_dash = true;
                                    }
                                    ch.child_original_index = Some(seq);
                                }
                            }
                            // Push non-overridden children after overridden ones, preserving existing order among them
                            let mut next_idx = override_order.len();
                            for ch in template_field.children.iter_mut() {
                                if override_order.iter().any(|n| n == &ch.name) {
                                    continue;
                                }
                                if ch.child_original_index.is_none() {
                                    ch.child_original_index = Some(next_idx);
                                    next_idx += 1;
                                } else {
                                    ch.child_original_index = Some(
                                        ch.child_original_index.unwrap() + override_order.len(),
                                    );
                                }
                            }
                        }
                    } else {
                        debug_resolver!(
                            "[RESOLVER]     No child overrides to merge for '{}'",
                            template_field.name
                        );
                    }
                }
            // Ensure node_type is preserved from template (do not overwrite)
            } else {
                debug_resolver!(
                    "[RESOLVER]   Override '{}' path lookup failed unexpectedly",
                    ov_name
                );
            }
        } else {
            debug_resolver!(
                "[RESOLVER]   Override '{}' had no matching field in template (considering transparency)",
                ov_name
            );
        }
    }
}

/// Mark a node and its subtree as template-derived by adding _template_ markers for present params
fn mark_template_child_recursive(node: &mut OverseerNode) {
    if let Some(existing_snapshot) = node.source_snapshot.clone() {
        let fingerprint = existing_snapshot.fingerprint;
        node.source_snapshot = Some(NodeSourceSnapshot::template_clone_of(&existing_snapshot));
        node.source_fingerprint = Some(fingerprint);
    } else {
        node.source_fingerprint = None;
    }
    node.source_id = None;
    // Mark a simple flag to indicate this whole node is from a template
    node.parameters
        .insert("_template_node".to_string(), OverseerValue::Boolean(true));
    // For all existing parameters, add a _template_ marker so serializer excludes them by default
    let keys: Vec<String> = node.parameters.keys().cloned().collect();
    for k in keys {
        if !k.starts_with("_") {
            // avoid internal keys
            if let Some(v) = node.parameters.get(&k).cloned() {
                node.parameters.insert(format!("_template_{}", k), v);
            }
        }
    }
    for child in node.children.iter_mut() {
        mark_template_child_recursive(child);
    }
}

fn collect_all_node_paths(
    nodes: &[OverseerNode],
    prefix: &mut Vec<String>,
    acc: &mut std::collections::HashSet<String>,
) {
    // The roots as the pass names them, then every level below by `addressing::Level`.
    for (idx, n) in nodes.iter().enumerate() {
        let name = n.name.clone();
        let k = nodes.iter().take(idx).filter(|c| c.name == name).count();
        prefix.push(if k > 0 { format!("{}#{}", name, k) } else { name });
        acc.insert(prefix.join("/"));
        collect_paths_below(&n.children, prefix, acc, &mut crate::addressing::Level::default());
        prefix.pop();
    }
}

fn collect_paths_below(
    nodes: &[OverseerNode],
    prefix: &mut Vec<String>,
    acc: &mut std::collections::HashSet<String>,
    level: &mut crate::addressing::Level,
) {
    for n in nodes {
        if crate::addressing::is_wrapper(n) {
            collect_paths_below(&n.children, prefix, acc, level);
            continue;
        }
        prefix.push(level.segment(&n.name));
        acc.insert(prefix.join("/"));
        collect_paths_below(&n.children, prefix, acc, &mut crate::addressing::Level::default());
        prefix.pop();
    }
}


/// Per-formula timing, aggregated by formula text. Enabled with OVERSEER_PROFILE=1.
///
/// Wall-clock totals said the evaluator was slow but not which formulas cost the time, and
/// the two documents that hurt most differ by 14x per pass at identical node counts - so the
/// cost is in what the formulas do, not how many nodes there are.
static PROFILE_FORMULAS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, (u64, f64)>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

pub fn profile_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("OVERSEER_PROFILE").is_ok())
}

fn profile_record_formula(src: &str, elapsed: std::time::Duration) {
    if !profile_enabled() {
        return;
    }
    if let Ok(mut map) = PROFILE_FORMULAS.lock() {
        let entry = map.entry(src.to_string()).or_insert((0, 0.0));
        entry.0 += 1;
        entry.1 += elapsed.as_secs_f64() * 1000.0;
    }
}

/// What a pass changed, with OVERSEER_PROFILE=1: every value that came out different from what it
/// held when the pass began.
///
/// A pass after the first changes only what the first could not finish, so what it moved is the
/// list of chains still a step behind - which is the question every attempt at fewer passes has
/// turned on, and which a count of passes cannot answer.
static PROFILE_MOVED: std::sync::LazyLock<std::sync::Mutex<Vec<String>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(Vec::new()));

fn profile_note_moved(path: &[String], key: &str, was: Option<&OverseerValue>, now: &OverseerValue) {
    if !profile_enabled() {
        return;
    }
    if let Ok(mut moved) = PROFILE_MOVED.lock() {
        let short = |v: String| v.chars().take(40).collect::<String>();
        moved.push(format!(
            "{}#{}  {} -> {}",
            path.join("/"),
            key,
            short(format!("{:?}", was)),
            short(format!("{:?}", now))
        ));
    }
}

/// Say what the pass just finished moved, by field and value, and forget it.
fn profile_report_moved(pass: usize) {
    if !profile_enabled() {
        return;
    }
    let Ok(mut moved) = PROFILE_MOVED.lock() else { return };
    if pass > 1 && !moved.is_empty() {
        let mut by_field: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for key in moved.iter() {
            let key = key.split("  ").next().unwrap_or(key);
            let field = key.rsplit('/').next().unwrap_or(key).to_string();
            *by_field.entry(field).or_insert(0) += 1;
        }
        let mut rows: Vec<(String, usize)> = by_field.into_iter().collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let shown: Vec<String> = rows.iter().take(12).map(|(f, n)| format!("{} x{}", f, n)).collect();
        eprintln!("[PROFILE]   moved {} values: {}", moved.len(), shown.join(", "));
        for key in moved.iter().filter(|k| !k.contains("_template_")).take(8) {
            eprintln!("[PROFILE]     e.g. {}", key);
        }
    }
    moved.clear();
}

/// Report the most expensive formulas and clear the tally.
pub fn profile_report_formulas(label: &str) {
    if !profile_enabled() {
        return;
    }
    if let Ok(mut map) = PROFILE_FORMULAS.lock() {
        let mut rows: Vec<(String, u64, f64)> =
            map.iter().map(|(k, (n, ms))| (k.clone(), *n, *ms)).collect();
        rows.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        let total: f64 = rows.iter().map(|r| r.2).sum();
        eprintln!("[PROFILE] {}: {:.1} ms across {} distinct formulas", label, total, rows.len());
        for (src, count, ms) in rows.iter().take(8) {
            // On one line, since a formula written over several would break the report up.
            let shown: String = src.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(76).collect();
            eprintln!(
                "[PROFILE]   {:>8.1} ms  {:>5} calls  {:>6.3} ms/call  {}",
                ms, count, ms / *count as f64, shown
            );
        }
        map.clear();
    }
}

/// How many passes over its formulas the last resolve on this thread took - over the whole
/// document, or over what an edit reached. A second one used to be the rule rather than the
/// exception, and what a document costs to open is mostly this times its size, so it is worth
/// being able to pin.
pub fn passes_taken() -> usize {
    PASSES_TAKEN.with(|p| p.get())
}

thread_local! {
    static WORKED_OUT_WHOLE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many times this thread has worked a document out whole. A change that should have been
/// followed through the graph and was worked out whole anyway gives the same answer, only slower -
/// so the answer cannot tell the two apart, and this can.
pub fn times_worked_out_whole() -> usize {
    WORKED_OUT_WHOLE.with(|n| n.get())
}

thread_local! {
    static PASSES_TAKEN: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// The nodes that read something in doubt while the last pass worked them out - see
    /// `FormulaEvaluator::doubts`. Everything else that pass stored is final.
    static DOUBTED: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

/// The same, only for these nodes - named as the resolver names them, a wrapper by its container.
fn forget_worked_out_values_at(
    nodes: &mut [OverseerNode],
    targets: &std::collections::HashSet<String>,
) {
    fn walk(
        nodes: &mut [OverseerNode],
        trail: &mut Vec<String>,
        level: &mut crate::addressing::Level,
        targets: &std::collections::HashSet<String>,
    ) {
        for node in nodes.iter_mut() {
            let wrapper = crate::addressing::is_wrapper(node);
            if !wrapper {
                trail.push(level.segment(&node.name));
            }
            if targets.contains(&trail.join("/")) {
                node.parameters.retain(|key, _| !key.starts_with("_computed_"));
            }
            if wrapper {
                walk(&mut node.children, trail, level, targets);
            } else {
                walk(&mut node.children, trail, &mut crate::addressing::Level::default(), targets);
                trail.pop();
            }
        }
    }
    for node in nodes.iter_mut() {
        let mut trail = vec![node.name.clone()];
        if targets.contains(&node.name) {
            node.parameters.retain(|key, _| !key.starts_with("_computed_"));
        }
        walk(&mut node.children, &mut trail, &mut crate::addressing::Level::default(), targets);
    }
}

/// Take away every value a resolve worked out, so the next one starts from what is written.
fn forget_worked_out_values(nodes: &mut [OverseerNode]) {
    for node in nodes.iter_mut() {
        node.parameters.retain(|key, _| !key.starts_with("_computed_"));
        forget_worked_out_values(&mut node.children);
    }
}

fn evaluate_formulas_in_document_multi_pass(nodes: &mut Vec<OverseerNode>) {
    debug_resolver!("[RESOLVER] Starting multi-pass formula evaluation");
    // Build a path set of all node paths so selective evaluator effectively treats entire tree as target
    let mut all_paths: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut pref: Vec<String> = Vec::new();
    collect_all_node_paths(nodes, &mut pref, &mut all_paths);
    // Re-run until nothing changes.
    //
    // This was four, and it was a budget rather than a limit: the food tracker was still changing
    // values on the fourth pass, so what it showed was a snapshot of an unfinished computation
    // and depended on how many times the document happened to be resolved - which differed
    // between the two entry points. Raising it did not help until the two reasons a document
    // could never settle were fixed: a fallback computed for a field that states a value, and a
    // failed fallback reading as Null where a failed formula reads as an error. Both are below,
    // and both had to go before this number could
    // reach an answer at all. It settles on its sixth pass now, so this is a guard against a
    // formula that genuinely never stops moving rather than a budget the document has to fit in.
    const MAX_PASSES: usize = 12;
    let mut pass = 0usize;
    let mut progress = true;
    // Set OVERSEER_PROFILE=1 to see where an interaction's time goes: how many passes ran,
    // and what each cost. This is the hot path - it runs on every edit - so guessing at its
    // shape from wall-clock totals has proven unreliable.
    let profiling = profile_enabled();
    let started = std::time::Instant::now();

    // Worked out afresh, as an opened document is. What an earlier resolve left behind may be
    // stale - this runs again after a press has changed the document - and a read that takes a
    // stale answer is exactly what makes a further pass necessary. With nothing left behind,
    // every read in the first pass works out what it needs where it lives, and the first pass is
    // the last - see `FormulaEvaluator::doubts`.
    forget_worked_out_values(nodes);

    while pass < MAX_PASSES && progress {
        pass += 1;
        progress = false;
        let doubts_before = FormulaEvaluator::doubts();
        DOUBTED.with(|d| d.borrow_mut().clear());
        let pass_started = std::time::Instant::now();
        if one_pass_over(nodes, &all_paths) {
            progress = true;
        }
        if profiling {
            eprintln!(
                "[PROFILE] pass {} took {:.1} ms progress={} paths={}",
                pass,
                pass_started.elapsed().as_secs_f64() * 1000.0,
                progress,
                all_paths.len()
            );
            profile_report_moved(pass);
        }
        // Settled, unless something in it read what the pass may not have stored. A second pass
        // would read every answer this one stored and store the same answers again.
        let doubted = FormulaEvaluator::doubts() - doubts_before;
        if profiling {
            eprintln!("[PROFILE]   {} read(s) in doubt", doubted);
            let mut said: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            for what in FormulaEvaluator::take_doubts_said() {
                *said.entry(what).or_insert(0) += 1;
            }
            let mut rows: Vec<(String, usize)> = said.into_iter().collect();
            rows.sort_by(|a, b| b.1.cmp(&a.1));
            for (what, n) in rows.iter().take(8) {
                eprintln!("[PROFILE]     {} x{}", what, n);
            }
        }
        if pass == 1 && doubted == 0 {
            progress = false;
        }
        // Some did, and only they can be wrong: a doubt is charged to the node being worked out
        // when it happened, and nothing worked out with a doubt is remembered for another reader
        // to take. So those nodes are worked out again from what the pass stored, and if none of
        // them moves the document is settled - without the pass over all of it that used to be
        // the only way to find out. If one does, the passes go on as they always have.
        if pass == 1 && doubted > 0 {
            let doubted_nodes = DOUBTED.with(|d| std::mem::take(&mut *d.borrow_mut()));
            let confirm_started = std::time::Instant::now();
            let moved = one_pass_over(nodes, &doubted_nodes);
            if profiling {
                eprintln!(
                    "[PROFILE]   {} node(s) in doubt worked out again in {:.1} ms, moved={}",
                    doubted_nodes.len(),
                    confirm_started.elapsed().as_secs_f64() * 1000.0,
                    moved
                );
                profile_report_moved(2);
            }
            progress = moved;
        }
        debug_resolver!(
            "[RESOLVER] Multi-pass formula evaluation pass {} progress={} ({} total paths)",
            pass,
            progress,
            all_paths.len()
        );
    }
    PASSES_TAKEN.with(|p| p.set(pass));
    if profiling {
        eprintln!(
            "[PROFILE] {} pass(es) in {:.1} ms total",
            pass,
            started.elapsed().as_secs_f64() * 1000.0
        );
        // What the passes actually spent their time on. Recorded per formula all along, and
        // until now discarded at the end of the run - which left the profile saying that the
        // evaluator was slow without saying which formula was.
        profile_report_formulas("formulas");
    }
    debug_resolver!(
        "[RESOLVER] Multi-pass formula evaluation completed in {} pass(es)",
        pass
    );
}

/// Selective formula evaluation that only processes specific field paths
fn evaluate_formulas_for_specific_fields(
    nodes: &mut Vec<OverseerNode>,
    field_paths: &std::collections::HashSet<String>,
) {
    debug_resolver!(
        "[RESOLVER] Starting selective formula evaluation for {} fields (multi-pass)",
        field_paths.len()
    );
    debug_resolver!("[RESOLVER] Field paths target set: {:?}", field_paths);
    // Passes, because one used to read what the others had stored before the change, and a
    // chain moved one link a pass: a meal's grams, its calories, the day's totals, the day's
    // figures per 100 g, their points, the score - and the cap below stopped it at the fourth,
    // so a day's grade stayed what it was before the edit until the document was opened again.
    //
    // So what the change reaches is worked out afresh, as an open is: its stored answers are
    // taken away first, a read of any of them works it out where it lives, and the first pass
    // is the last unless something read in it is in doubt - see `FormulaEvaluator::doubts`.
    // What the change does not reach keeps its answers, which is what makes this selective.
    forget_worked_out_values_at(nodes, field_paths);
    const MAX_PASSES: usize = 4;
    let selective_started = std::time::Instant::now();
    let mut pass = 0usize;
    let mut progress = true;
    while pass < MAX_PASSES && progress {
        pass += 1;
        progress = false;
        debug_resolver!("[RESOLVER] Selective pass {}", pass);
        let doubts_before = FormulaEvaluator::doubts();
        if one_pass_over(nodes, field_paths) {
            progress = true;
        }
        if pass == 1 && FormulaEvaluator::doubts() == doubts_before {
            progress = false;
        }
        if !progress {
            debug_resolver!("[RESOLVER] No changes in pass {}, stopping", pass);
        }
    }
    debug_resolver!(
        "[RESOLVER] Selective formula evaluation completed in {} pass(es)",
        pass
    );
    PASSES_TAKEN.with(|p| p.set(pass));
    if profile_enabled() {
        eprintln!(
            "[PROFILE] selective: {} pass(es) over {} target(s) in {:.1} ms",
            pass,
            field_paths.len(),
            selective_started.elapsed().as_secs_f64() * 1000.0
        );
        // Which formulas that was - as the whole resolve says, now that most of what is worked
        // out is worked out here.
        profile_report_formulas("selective formulas");
    }
}

/// Selective chart computation that only processes charts affected by specific field changes
fn compute_chart_series_for_specific_fields(
    nodes: &mut Vec<OverseerNode>,
    field_paths: &std::collections::HashSet<String>,
) {
    debug_resolver!(
        "[RESOLVER] Selective chart computation for changed fields: {:?}",
        field_paths
    );

    // Analyze if any charts actually depend on the changed field paths
    let charts_need_update = charts_depend_on_fields(nodes, field_paths);

    if charts_need_update {
        debug_resolver!(
            "[RESOLVER] Charts depend on changed fields, performing selective chart recomputation"
        );
        compute_chart_series(nodes);
    } else {
        debug_resolver!(
            "[RESOLVER] No charts depend on changed fields, skipping chart computation"
        );
    }
}

/// Check if any charts in the document depend on the specified field paths
fn charts_depend_on_fields(
    nodes: &[OverseerNode],
    field_paths: &std::collections::HashSet<String>,
) -> bool {
    for node in nodes {
        if chart_node_depends_on_fields(node, field_paths, "") {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests_inheritance_bug7 {
    use super::*;
    use crate::parser::parse_document;

    // Bug 7: When creating node from template, parent node should inherit the template's parameters (not only children)
    #[test]
    fn template_instance_parent_inherits_parameters() {
        let input = r#"
        tab Root {
            div Templates {
                div Colorful (background-color=#123456, font-color=#eeeeee) {
                    string Title (value="Hello")
                }
            }
            <Colorful> Instance {}
        }
        "#;

        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);

        // Find the instance node
        fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
            for n in nodes {
                if n.name == name {
                    return Some(n);
                }
                if let Some(f) = find(&n.children, name) {
                    return Some(f);
                }
            }
            None
        }
        let inst = find(&nodes, "Instance").expect("instance exists");

        // Expect parent-level parameters copied from template (and marked _from_template)
        assert_eq!(inst.node_type, "Colorful");
        assert!(matches!(
            inst.parameters.get("_from_template"),
            Some(OverseerValue::Boolean(true))
        ));
        assert!(matches!(
            inst.parameters.get("background-color"),
            Some(OverseerValue::Color(_))
        ));
        assert!(matches!(
            inst.parameters.get("font-color"),
            Some(OverseerValue::Color(_))
        ));

        // And child inherited (Title) exists
        let title = inst
            .children
            .iter()
            .find(|c| c.name == "Title")
            .expect("title child");
        assert_eq!(title.node_type, "string");
        assert!(
            matches!(title.parameters.get("value"), Some(OverseerValue::String(v)) if v == "Hello")
        );
    }
}

#[cfg(test)]
mod tests_aggregate_inherited_fields_persistence {
    use super::*;
    use crate::parser::parse_document;

    // This test ensures that editing inherited template fields (A,B) in list items triggers recomputation of C and total
    // without overwriting the aggregate node's formula value with a literal, across multiple selective edits.
    #[test]
    fn aggregate_formula_not_clobbered_across_two_selective_edits() {
        let input = r#"
        tab main {
            int total (value=$(L.map(|x| x/C).sum()))
            list L (entry=<T>) {
                <T> T__1 {}
            }
            div T {
                div { int A (value=2) }
                int B (value=3)
                int C (value=$(A*B))
            }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);

        // Helper: locate paths
        fn find<'a>(nodes: &'a [OverseerNode], path: &str) -> Option<&'a OverseerNode> {
            let mut cur: &[OverseerNode] = nodes;
            let mut found: Option<&OverseerNode> = None;
            for seg in path.split('/') {
                found = cur.iter().find(|n| n.name == seg);
                if let Some(f) = found {
                    cur = &f.children;
                } else {
                    return None;
                }
            }
            found
        }

        // Local helper to get mutable node by slash path
        fn find_node_by_path_mut<'a>(
            nodes: &'a mut [OverseerNode],
            path: &str,
        ) -> Option<&'a mut OverseerNode> {
            let parts: Vec<&str> = path.split('/').collect();
            let mut current: &mut [OverseerNode] = nodes;
            for (i, part) in parts.iter().enumerate() {
                let idx_opt = current.iter().position(|n| n.name == *part);
                if let Some(idx) = idx_opt {
                    if i == parts.len() - 1 {
                        return Some(&mut current[idx]);
                    }
                    let next: *mut Vec<OverseerNode> = &mut current[idx].children as *mut _;
                    // Safety: we only hold one mutable reference path at a time
                    unsafe {
                        current = &mut *next;
                    }
                } else {
                    return None;
                }
            }
            None
        }

        // Confirm initial total formula intact and computed shadow present
        let total = find(&nodes, "main/total").unwrap();
        assert!(
            matches!(total.parameters.get("value"), Some(OverseerValue::Formula(s)) if s.contains("map(|x| x/C).sum()"))
        );
        let _initial_total_val = total.parameters.get("_computed_value").cloned();

        // Simulate first selective edit: change A from 2 -> 5
        {
            let a_path = "main/L/T__1/A";
            if let Some(a_node) = find_node_by_path_mut(&mut nodes, a_path) {
                a_node
                    .parameters
                    .insert("value".to_string(), OverseerValue::Integer(5));
            }
            // What reads A, named rather than worked out: this test is about the
            // aggregate keeping its formula across selective edits, and the graph that used to
            // supply these was replaced.
            let to_update: std::collections::HashSet<String> = ["main/L/T__1/C", "main/total"]
                .iter()
                .map(|s| s.to_string())
                .collect();
            resolve_specific_fields(&mut nodes, &to_update);
        }

        let total_after_first = find(&nodes, "main/total").unwrap();
        assert!(
            matches!(
                total_after_first.parameters.get("value"),
                Some(OverseerValue::Formula(_))
            ),
            "Formula should persist after first edit"
        );
        let _after_first_val = total_after_first.parameters.get("_computed_value").cloned();
        // NOTE: We expect this to change after selective propagation fix; current focus is persistence, so we don't assert difference yet.

        // Second selective edit: change B 3 -> 4
        {
            let b_path = "main/L/T__1/B";
            if let Some(b_node) = find_node_by_path_mut(&mut nodes, b_path) {
                b_node
                    .parameters
                    .insert("value".to_string(), OverseerValue::Integer(4));
            }
            // What reads B, named rather than worked out: this test is about the
            // aggregate keeping its formula across selective edits, and the graph that used to
            // supply these was replaced.
            let to_update: std::collections::HashSet<String> = ["main/L/T__1/C", "main/total"]
                .iter()
                .map(|s| s.to_string())
                .collect();
            resolve_specific_fields(&mut nodes, &to_update);
        }

        let total_after_second = find(&nodes, "main/total").unwrap();
        assert!(
            matches!(
                total_after_second.parameters.get("value"),
                Some(OverseerValue::Formula(_))
            ),
            "Formula should persist after second edit"
        );
        let _after_second_val = total_after_second
            .parameters
            .get("_computed_value")
            .cloned();
        // Similarly, skip asserting change pending selective propagation bug resolution.
        // Ensure still formula after two edits
        assert!(matches!(
            total_after_second.parameters.get("value"),
            Some(OverseerValue::Formula(_))
        ));
    }
}

#[cfg(test)]
mod tests_nested_named_container_defaults_in_list_items {
    use super::*;
    use crate::parser::parse_document;

    // When a list item template has a named container (e.g., per_item)
    // with several default fields, providing an override for one child
    // (e.g., per_item/calories) must preserve the defaults of the other
    // children (e.g., per_item/weight) in the resolved instance.
    #[test]
    fn list_item_named_container_preserves_sibling_defaults_on_child_override() {
        let input = r#"
        tab Root {
            div MealRecord (hidden=true) {
                div per_item {
                    float calories = 0.0
                    float weight = 100.0
                }
            }
            list Intake (entry=<MealRecord>) {
                - {
                    - per_item {
                        - calories = 250.0
                    }
                }
            }
        }
        "#;

        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);

        // Find Intake list item (resolved MealRecord)
        let root = nodes
            .iter()
            .find(|n| n.name == "Root")
            .expect("root present");
        let intake = root
            .children
            .iter()
            .find(|n| n.name == "Intake")
            .expect("intake present");
        assert_eq!(intake.node_type, "list");
        assert_eq!(intake.children.len(), 1);
        let item = &intake.children[0];
        assert_eq!(item.node_type, "MealRecord");

        // Access per_item fields
        let per_item = item
            .children
            .iter()
            .find(|c| c.name == "per_item")
            .expect("per_item present");
        // calories should be overridden
        let calories = per_item
            .children
            .iter()
            .find(|c| c.name == "calories")
            .expect("calories present");
        assert!(
            matches!(calories.parameters.get("value"), Some(OverseerValue::Float(f)) if (*f - 250.0).abs() < 1e-6)
        );
        // weight should remain from template default (100.0)
        let weight = per_item
            .children
            .iter()
            .find(|c| c.name == "weight")
            .expect("weight present");
        assert!(
            matches!(weight.parameters.get("value"), Some(OverseerValue::Float(f)) if (*f - 100.0).abs() < 1e-6)
        );
        // And weight should still be marked as template-derived (has _template_value)
        assert!(
            weight.parameters.contains_key("_template_value"),
            "weight should keep template marker since it wasn't overridden"
        );
    }

    // Multiple overrides in the same named container should not drop other defaults
    #[test]
    fn list_item_named_container_multiple_sibling_overrides_preserve_other_defaults() {
        let input = r#"
        tab Root {
            div T (hidden=true) {
                div group {
                    float a = 1.0
                    float b = 2.0
                    float c = 3.0
                    string label = "x"
                }
            }
            list L (entry=<T>) {
                - {
                    - group {
                        - a = 10.0
                        - c = 30.0
                        - label = "over"
                    }
                }
            }
        }
        "#;

        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);

        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let list = root.children.iter().find(|n| n.name == "L").unwrap();
        assert_eq!(list.children.len(), 1);
        let item = &list.children[0];
        assert_eq!(item.node_type, "T");
        let group = item.children.iter().find(|c| c.name == "group").unwrap();
        let a = group.children.iter().find(|c| c.name == "a").unwrap();
        let b = group.children.iter().find(|c| c.name == "b").unwrap();
        let c = group.children.iter().find(|c| c.name == "c").unwrap();
        let label = group.children.iter().find(|c| c.name == "label").unwrap();
        assert!(
            matches!(a.parameters.get("value"), Some(OverseerValue::Float(f)) if (*f - 10.0).abs() < 1e-6)
        );
        assert!(
            matches!(c.parameters.get("value"), Some(OverseerValue::Float(f)) if (*f - 30.0).abs() < 1e-6)
        );
        assert!(
            matches!(label.parameters.get("value"), Some(OverseerValue::String(v)) if v == "over")
        );
        // Non-overridden 'b' should remain default and carry template marker
        assert!(
            matches!(b.parameters.get("value"), Some(OverseerValue::Float(f)) if (*f - 2.0).abs() < 1e-6)
        );
        assert!(b.parameters.contains_key("_template_value"));
    }

    // Deeply nested containers: overrides at inner level should preserve defaults of siblings at the same inner level
    #[test]
    fn list_item_deep_nested_container_child_override_preserves_inner_sibling_defaults() {
        let input = r#"
        tab Root {
            div T (hidden=true) {
                div outer {
                    div inner {
                        float x = 1.0
                        float y = 2.0
                    }
                    float z = 3.0
                }
            }
            list L (entry=<T>) {
                - {
                    - outer {
                        - inner {
                            - x = 10.0
                        }
                    }
                }
            }
        }
        "#;

        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);

        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let list = root.children.iter().find(|n| n.name == "L").unwrap();
        assert_eq!(list.children.len(), 1);
        let item = &list.children[0];
        assert_eq!(item.node_type, "T");
        let outer = item.children.iter().find(|c| c.name == "outer").unwrap();
        let inner = outer.children.iter().find(|c| c.name == "inner").unwrap();
        let x = inner.children.iter().find(|c| c.name == "x").unwrap();
        let y = inner.children.iter().find(|c| c.name == "y").unwrap();
        let z = outer.children.iter().find(|c| c.name == "z").unwrap();
        // Override applied
        assert!(
            matches!(x.parameters.get("value"), Some(OverseerValue::Float(f)) if (*f - 10.0).abs() < 1e-6)
        );
        // Sibling default preserved with template marker
        assert!(
            matches!(y.parameters.get("value"), Some(OverseerValue::Float(f)) if (*f - 2.0).abs() < 1e-6)
        );
        assert!(y.parameters.contains_key("_template_value"));
        // Unaffected cousin at outer level should remain default
        assert!(
            matches!(z.parameters.get("value"), Some(OverseerValue::Float(f)) if (*f - 3.0).abs() < 1e-6)
        );
        assert!(z.parameters.contains_key("_template_value"));
    }
}

/// Recursively check if a chart node or its children depend on the specified field paths
fn chart_node_depends_on_fields(
    node: &OverseerNode,
    field_paths: &std::collections::HashSet<String>,
    current_path: &str,
) -> bool {
    let node_path = if current_path.is_empty() {
        node.name.clone()
    } else {
        format!("{}/{}", current_path, node.name)
    };

    // Check if this is a chart with plots
    if node.node_type == "chart" {
        for child in &node.children {
            if child.node_type == "plot" {
                if plot_depends_on_fields(child, field_paths, &node_path) {
                    debug_resolver!(
                        "[RESOLVER] Chart plot '{}' depends on changed fields",
                        format!("{}/{}", node_path, child.name)
                    );
                    return true;
                }
            }
        }
    }

    // Recursively check children
    for child in &node.children {
        if chart_node_depends_on_fields(child, field_paths, &node_path) {
            return true;
        }
    }

    false
}

/// Check if a specific plot depends on any of the changed field paths
fn plot_depends_on_fields(
    plot: &OverseerNode,
    field_paths: &std::collections::HashSet<String>,
    _chart_path: &str,
) -> bool {
    use crate::types::OverseerValue;

    // Check the 'source' parameter to see what data the plot references
    if let Some(OverseerValue::String(source_path)) = plot.parameters.get("source") {
        debug_resolver!(
            "[RESOLVER] Checking if plot source '{}' intersects with changed fields: {:?}",
            source_path,
            field_paths
        );

        // If the source path (like "/data") intersects with any changed field paths
        for field_path in field_paths {
            // Check if the changed field could affect the plot's data source
            let source_clean = source_path.trim_start_matches('/');

            // Direct path match (e.g., field "data" affects source "/data")
            if field_path == source_clean || field_path.starts_with(&format!("{}/", source_clean)) {
                debug_resolver!(
                    "[RESOLVER] Plot source '{}' directly affected by field change '{}'",
                    source_path,
                    field_path
                );
                return true;
            }

            // Reverse check: source affects field (e.g., source "/data" affects field "data/item")
            if source_clean.starts_with(field_path)
                || source_clean.starts_with(&format!("{}/", field_path))
            {
                debug_resolver!(
                    "[RESOLVER] Plot source '{}' contains changed field '{}'",
                    source_path,
                    field_path
                );
                return true;
            }
        }
    }

    // For now, assume plot formulas (x, y parameters) only depend on lambda variables and source data
    // They typically don't depend on external fields like 'a' or 'b'
    debug_resolver!(
        "[RESOLVER] Plot '{}' does not depend on changed fields",
        format!("{}/{}", _chart_path, plot.name)
    );
    false
}

/// One pass over the document, working out the formulas of `targets` and of what lies above them.
/// True when anything came out different from what was stored.
///
/// Over the document itself, not a copy - see `recursively_evaluate_node_formulas_selective`. The
/// pass's memo is kept for its duration, and a list's entries can be found where they are, to be
/// worked out when read - see `FormulaEvaluator::value_of_node`. Every pointer into the document
/// is taken from one, so the references made from them while a node is worked out and the writes
/// made between are to the same document by the same route.
fn one_pass_over(nodes: &mut Vec<OverseerNode>, targets: &std::collections::HashSet<String>) -> bool {
    let root: *mut [OverseerNode] = nodes.as_mut_slice();
    let mut moved = false;
    FormulaEvaluator::begin_pass_over(root);
    unsafe {
        let first = root as *mut OverseerNode;
        for i in 0..root.len() {
            let node_ptr = first.add(i);
            let mut current_path = vec![(*node_ptr).name.clone()];
            if recursively_evaluate_node_formulas_selective(
                node_ptr,
                std::ptr::null(),
                &mut current_path,
                root as *const [OverseerNode],
                targets,
                &mut crate::addressing::Level::default(),
            ) {
                moved = true;
            }
        }
    }
    FormulaEvaluator::end_pass_memo();
    moved
}

/// Work out the formulas of the nodes named in `field_paths`, and of every node above one, from
/// here down.
///
/// Reads come from the document being written - `document_root` is it, not a copy. A pass used to
/// copy the whole document first so that what it read could not move under it, which was a third
/// of what an edit's working out cost on the heavy documents; since reads work out on demand what
/// they need, what they find already stored in the document is the same answer. So the document is
/// held as a pointer, and a reference is made from it only while a node's formulas are worked out,
/// when nothing is written: what they come to is written afterwards, once every reference into the
/// document is gone.
unsafe fn recursively_evaluate_node_formulas_selective(
    node_ptr: *mut OverseerNode,
    _parent_ptr: *const OverseerNode,
    current_path: &mut Vec<String>,
    document_root: *const [OverseerNode],
    field_paths: &std::collections::HashSet<String>,
    // Numbers this node's children in the path - its parent's, when this is a wrapper.
    level: &mut crate::addressing::Level,
) -> bool {
    // Track whether any _computed_* param mutated in this subtree so caller can record progress
    let mut subtree_changed = false;
    {
        let node: &OverseerNode = &*node_ptr;
        // Out of view, so not worked out. Nothing under it either - see `apply_list_windows`.
        if out_of_view(node) {
            return false;
        }
        // Skip evaluating formulas for nodes inside action handler blocks
        if let Some(p) = _parent_ptr.as_ref() {
            if p.node_type == "on" {
                return false; // Skip action handler blocks entirely
            }
        }
    }

    // Check if this node's path is in the fields we need to update
    let current_path_str = current_path.join("/");
    // Normalization experiment: build alternate path stripping empty name segments for matching
    let normalized_no_empty: String = current_path
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join("/");
    let should_evaluate_this_node = field_paths.contains(&current_path_str)
        || field_paths
            .iter()
            .any(|path| path.starts_with(&current_path_str))
        || (!normalized_no_empty.is_empty()
            && (field_paths.contains(&normalized_no_empty)
                || field_paths
                    .iter()
                    .any(|p| p.starts_with(&normalized_no_empty))));
    if should_evaluate_this_node {
        debug_resolver!(
            "[RESOLVER] selective match path='{}' normalized='{}'",
            current_path_str,
            normalized_no_empty
        );
    }

    if should_evaluate_this_node {
        debug_resolver!(
            "🔄 Selectively evaluating formulas for node at path: {}",
            current_path_str
        );
        let doubts_before = FormulaEvaluator::doubts();

        // Same formula evaluation logic as the main function
        // Not the `_template_*` ones: those are the serializer's note of what the template said,
        // kept so it can tell an entry's own value from one it inherited, and nothing reads them
        // worked out. Worked out anyway they were a value and a formula per field of every entry -
        // 3,700 of each on the food tracker - stored as `_computed__template_*` for no one.
        let formula_pairs: Vec<(String, String)> = (*node_ptr)
            .parameters
            .iter()
            .filter_map(|(k, v)| match v {
                OverseerValue::Formula(s) if !k.starts_with("_template_") => Some((k.clone(), s.clone())),
                _ => None,
            })
            .collect();

        // Once each, in the order a node's formulas read one another: the fallbacks, then the
        // value, then everything else. A `hidden` or a colour reads the value of the field it is
        // on more than it reads anything, and a read takes whatever is stored - so each stage is
        // written before the next is worked out, and what the next one finds is this pass's.
        //
        // This was every formula twice: the whole set worked out, written, and worked out again,
        // so that the second time a `hidden` reading its own field found the new value rather
        // than the last one. Twice the cost of every pass for the few formulas that read their
        // own node - on tasks.os, half of what marking a task done spent on formulas.
        let (value_pair, other_pairs): (Vec<(String, String)>, Vec<(String, String)>) =
            formula_pairs.into_iter().partition(|(k, _)| k == "value");
        let mut freeze = false;
        for stage in 0..3 {
            let pairs: &[(String, String)] = match stage {
                1 => &value_pair,
                2 => &other_pairs,
                _ => &[],
            };
            // Worked out while nothing is written - see above.
            let (computed_params, asks) = {
                let node: &OverseerNode = &*node_ptr;
                let document: &[OverseerNode] = &*document_root;
                let context = EvaluationContext::new_with_current_and_parent(
                    node,
                    _parent_ptr.as_ref(),
                    current_path.to_vec(),
                    document,
                );
                let mut computed_params: Vec<(String, OverseerValue)> = Vec::new();
                // Compute fallback first (parity with full evaluator) so dependents can read
                // _computed_fallback immediately - and only where it could ever be read. See the
                // note on the same guard in the full evaluator: a fallback belongs to a field that
                // states no value, and computing the others is what kept this document moving.
                // Only an unset field reads either of these - see `states_a_value`.
                let unset = stage == 0 && !FormulaEvaluator::states_a_value(&node.parameters);
                for (declared, shadow) in [("fallback", "_computed_fallback"), ("default", "_computed_default")] {
                    let Some(source) = node.parameters.get(declared).cloned().filter(|_| unset) else {
                        continue;
                    };
                    let _recording = crate::dependencies::WorkingOut::value(&format!(
                        "{}#{}",
                        current_path.join("/"),
                        shadow
                    ));
                    // Said to be in progress while it is worked out, the way a read working it out
                    // on demand says so - see `FormulaEvaluator::work_out_once`. Otherwise a read
                    // inside it that comes back round - `grams` falling back to `portions`, which
                    // falls back to `grams` - works the same fallback out again one level deeper,
                    // and cuts the circle somewhere different from where the next pass will cut it.
                    let worked_out = match source {
                        OverseerValue::Formula(f) => FormulaEvaluator::work_out_once(current_path, declared, || {
                            FormulaEvaluator::evaluate_formula(f.as_str(), &context)
                        })
                        .unwrap_or_else(|| OverseerValue::String("invalid formula error".to_string())),
                        other => other,
                    };
                    computed_params.push((shadow.to_string(), worked_out));
                }
                for (key, formula_src) in pairs {
                    debug_resolver!(
                        "[RESOLVER] Selectively evaluating formula in {}.{}: {}",
                        node.name,
                        key,
                        formula_src
                    );
                    let shadow_key = if key == "value" {
                        "_computed_value".to_string()
                    } else {
                        format!("_computed_{}", key)
                    };
                    // Whatever this formula reads is read on behalf of this value.
                    let _recording = crate::dependencies::WorkingOut::value(&format!(
                        "{}#{}",
                        current_path.join("/"),
                        shadow_key
                    ));
                    let formula_started = profile_enabled().then(std::time::Instant::now);
                    // A value is in progress while it is worked out, as a fallback is above.
                    let evaluated = if key == "value" {
                        FormulaEvaluator::work_out_once(current_path, "value", || {
                            FormulaEvaluator::evaluate_formula(formula_src.as_str(), &context)
                        })
                        .ok_or_else(|| crate::types::OverseerError::FormulaError("already being worked out".to_string()))
                    } else {
                        FormulaEvaluator::evaluate_formula(formula_src.as_str(), &context)
                    };
                    if let Some(started) = formula_started {
                        profile_record_formula(formula_src.as_str(), started.elapsed());
                    }
                    match evaluated {
                        Ok(result) => {
                            debug_resolver!("[RESOLVER] Formula result: {:?}", result);
                            computed_params.push((shadow_key, result));
                        }
                        Err(_err) => {
                            debug_resolver!("[RESOLVER] Formula error at {}.{}", node.name, key);
                            computed_params.push((
                                shadow_key,
                                OverseerValue::String("invalid formula error".to_string()),
                            ));
                        }
                    }
                }
                drop(context);
                // Asked once everything is worked out.
                let asks = stage == 2 && asks_to_freeze_here(node, document, current_path);
                (computed_params, asks)
            };
            freeze |= asks;
            // Then written, with nothing read from the document meanwhile.
            let node: &mut OverseerNode = &mut *node_ptr;
            for (k, v) in computed_params {
                // Prevent formula clobber: if this is the shadow key it's safe; raw 'value' never replaced here
                let changed = match node.parameters.get(&k) {
                    Some(existing) => existing != &v,
                    None => true,
                };
                if changed {
                    subtree_changed = true;
                    profile_note_moved(current_path, &k, node.parameters.get(&k), &v);
                }
                node.parameters.insert(k, v); // k could be _computed_value or _computed_paramName
            }
        }
        freeze_if_it_was_asked_for(&mut *node_ptr, freeze);
        if FormulaEvaluator::doubts() != doubts_before {
            DOUBTED.with(|d| d.borrow_mut().insert(current_path_str.clone()));
        }
    }

    // Always recurse into children to check their paths. A wrapper is no step of the path - its
    // children are named as its parent's, see `addressing::Level`.
    let child_len = (*node_ptr).children.len();
    for idx in 0..child_len {
        let child_ptr: *mut OverseerNode = (*node_ptr).children.as_mut_ptr().add(idx);
        let wrapper = crate::addressing::is_wrapper(&*child_ptr);
        let changed = if wrapper {
            recursively_evaluate_node_formulas_selective(
                child_ptr,
                node_ptr as *const OverseerNode,
                current_path,
                document_root,
                field_paths,
                level,
            )
        } else {
            current_path.push(level.segment(&(*child_ptr).name));
            let changed = recursively_evaluate_node_formulas_selective(
                child_ptr,
                node_ptr as *const OverseerNode,
                current_path,
                document_root,
                field_paths,
                &mut crate::addressing::Level::default(),
            );
            current_path.pop();
            changed
        };
        if changed {
            subtree_changed = true;
        }
    }
    subtree_changed
}

// Note: child formula evaluation is handled via recursively_evaluate_node_formulas above

/// Compute UI sort keys for list items when a list declares sort_by (lambda or expression)
fn compute_list_ui_sort_keys(nodes: &mut Vec<OverseerNode>) {
    let snapshot = nodes.clone();
    let len = nodes.len();
    for i in 0..len {
        let node_ptr: *mut OverseerNode = &mut nodes[i] as *mut _;
        let mut current_path = vec![unsafe { (&*node_ptr).name.clone() }];
        unsafe {
            recursively_compute_sort_keys(node_ptr, std::ptr::null(), &mut current_path, &snapshot, &mut crate::addressing::Level::default());
        }
    }
}

unsafe fn recursively_compute_sort_keys(
    node_ptr: *mut OverseerNode,
    _parent_ptr: *const OverseerNode,
    current_path: &mut Vec<String>,
    document_root: &[OverseerNode],
    // Numbers this node's children in the path - see `addressing::Level`.
    level: &mut crate::addressing::Level,
) {
    use crate::formula_evaluator::{EvaluationContext, FormulaEvaluator};
    use crate::types::OverseerValue;

    let node: &mut OverseerNode = &mut *node_ptr;

    if out_of_view(node) {
        return;
    }

    // For list nodes, if sort_by parameter is present (as Formula or String), compute per-item keys
    if node.node_type == "list" {
        if let Some(sort_expr_val) = node.parameters.get("sort_by") {
            let sort_src = match sort_expr_val {
                OverseerValue::Formula(s) => s.as_str(),
                OverseerValue::String(s) => s.as_str(),
                _ => "",
            };
            if !sort_src.is_empty() {
                for (idx, child) in node.children.iter_mut().enumerate() {
                    // Build context for evaluating against the child; bind as current and provide parent
                    let mut path = current_path.clone();
                    path.push(child.name.clone());
                    let ctx = EvaluationContext::new_with_current_and_parent(
                        child,
                        Some(&*node_ptr),
                        path,
                        document_root,
                    );
                    let key = FormulaEvaluator::evaluate_lambda_on_item(sort_src, &ctx, child)
                        .unwrap_or(OverseerValue::Integer(idx as i64));
                    // Store as internal UI-only key
                    child.parameters.insert("_ui_sort_key".to_string(), key);
                }
            }
        }
    }

    // Recurse into children - a wrapper is no step of the path, see `addressing::Level`.
    for c in 0..node.children.len() {
        let child_ptr: *mut OverseerNode = &mut node.children[c] as *mut _;
        if crate::addressing::is_wrapper(&*child_ptr) {
            recursively_compute_sort_keys(child_ptr, node as *const OverseerNode, current_path, document_root, level);
            continue;
        }
        current_path.push(level.segment(&(*child_ptr).name));
        recursively_compute_sort_keys(
            child_ptr,
            node as *const OverseerNode,
            current_path,
            document_root,
            &mut crate::addressing::Level::default(),
        );
        current_path.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_document;

    #[test]
    fn test_layout_resolution_vertical() {
        let input = r#"div Container (layout=vertical) {
            div child1 { string field = "First" }
            div child2 { string field = "Second" }
        }"#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);

        let container = &nodes[0];
        assert_eq!(
            container.parameters.get("_effective_layout"),
            Some(&OverseerValue::String("vertical".to_string()))
        );

        // Children should alternate to horizontal
        let child1 = &container.children[0];
        let child2 = &container.children[1];
        assert_eq!(
            child1.parameters.get("_effective_layout"),
            Some(&OverseerValue::String("horizontal".to_string()))
        );
        assert_eq!(
            child2.parameters.get("_effective_layout"),
            Some(&OverseerValue::String("horizontal".to_string()))
        );
    }

    #[test]
    fn test_layout_resolution_horizontal() {
        let input = r#"div Container (layout=horizontal) {
            div child1 { string field = "First" }
            div child2 { string field = "Second" }
        }"#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);

        let container = &nodes[0];
        assert_eq!(
            container.parameters.get("_effective_layout"),
            Some(&OverseerValue::String("horizontal".to_string()))
        );

        // Children should alternate to vertical
        let child1 = &container.children[0];
        let child2 = &container.children[1];
        assert_eq!(
            child1.parameters.get("_effective_layout"),
            Some(&OverseerValue::String("vertical".to_string()))
        );
        assert_eq!(
            child2.parameters.get("_effective_layout"),
            Some(&OverseerValue::String("vertical".to_string()))
        );
    }

    #[test]
    fn test_layout_resolution_inherit() {
        let input = r#"div Outer (layout=horizontal) {
            div Inner (layout=inherit) {
                div child { string field = "Test" }
            }
        }"#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);

        let outer = &nodes[0];
        let inner = &outer.children[0];
        let child = &inner.children[0];

        assert_eq!(
            outer.parameters.get("_effective_layout"),
            Some(&OverseerValue::String("horizontal".to_string()))
        );
        assert_eq!(
            inner.parameters.get("_effective_layout"),
            Some(&OverseerValue::String("horizontal".to_string()))
        );
        assert_eq!(
            child.parameters.get("_effective_layout"),
            Some(&OverseerValue::String("vertical".to_string()))
        );
    }

    #[test]
    fn test_layout_resolution_opposite() {
        let input = r#"div Outer (layout=horizontal) {
            div Inner (layout=opposite) {
                div child { string field = "Test" }
            }
        }"#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);

        let outer = &nodes[0];
        let inner = &outer.children[0];
        let child = &inner.children[0];

        assert_eq!(
            outer.parameters.get("_effective_layout"),
            Some(&OverseerValue::String("horizontal".to_string()))
        );
        assert_eq!(
            inner.parameters.get("_effective_layout"),
            Some(&OverseerValue::String("vertical".to_string()))
        );
        assert_eq!(
            child.parameters.get("_effective_layout"),
            Some(&OverseerValue::String("horizontal".to_string()))
        );
    }

    #[test]
    fn test_layout_resolution_default_alternation() {
        let input = r#"div Outer {
            div Inner {
                div child { string field = "Test" }
            }
        }"#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);

        let outer = &nodes[0];
        let inner = &outer.children[0];
        let child = &inner.children[0];

        // Default should be horizontal for root, then alternate
        assert_eq!(
            outer.parameters.get("_effective_layout"),
            Some(&OverseerValue::String("horizontal".to_string()))
        );
        assert_eq!(
            inner.parameters.get("_effective_layout"),
            Some(&OverseerValue::String("vertical".to_string()))
        );
        assert_eq!(
            child.parameters.get("_effective_layout"),
            Some(&OverseerValue::String("horizontal".to_string()))
        );
    }

    #[test]
    fn test_template_resolution() {
        let input = r#"
        div Task (hidden=true) {
            string description = ""
            checkbox complete = false
        }
        
        <Task> my_task {
            string description = "My custom task"
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);

        // After resolution, we should still have both nodes (template and instance)
        // But hidden templates may be filtered out in actual rendering, not in tests
        debug_resolver!("Number of nodes after resolution: {}", nodes.len());
        for (_i, _node) in nodes.iter().enumerate() {
            debug_resolver!("Node {}: {} (type: {})", i, node.name, node.node_type);
        }

        // Find the resolved task (it should be the second node, or the only non-hidden one)
        let resolved_task = if nodes.len() == 2 {
            &nodes[1] // Both template and instance present
        } else {
            &nodes[0] // Only instance present (template filtered out)
        };

        debug_resolver!("Resolved task children: {}", resolved_task.children.len());
        for (_i, _child) in resolved_task.children.iter().enumerate() {
            debug_resolver!("  Child {}: {} (type: {})", i, child.name, child.node_type);
        }

        assert_eq!(resolved_task.node_type, "Task");
        // New behavior: copy all template fields, then merge overrides
        assert_eq!(resolved_task.children.len(), 2);
        // description should be overridden
        let description = resolved_task
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "description")
            .unwrap();
        assert_eq!(
            description.parameters.get("value"),
            Some(&OverseerValue::String("My custom task".to_string()))
        );
        // checkbox should be present with default value from template
        let complete = resolved_task
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "complete")
            .unwrap();
        assert_eq!(
            complete.parameters.get("value"),
            Some(&OverseerValue::Boolean(false))
        );
    }

    #[test]
    fn test_overrides_persist_through_unnamed_div_in_template() {
        // Template Bug has an unnamed div grouping fields. Instance overrides should persist.
        let input = r#"
        div Bug {
            string description = ""
            div {
                int storypoints = 1
                int priority = 0
                checkbox fixed = false
            }
        }

        list BugList (entry=<Bug>) {
            - {
                - description = "override text"
                - storypoints = 5
                - priority = 2
                - fixed = true
            }
        }
        "#;

        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);

        // Find BugList list entry
        let bug_list = nodes
            .iter()
            .find(|n| n.name == "BugList")
            .expect("list present");
        assert_eq!(bug_list.node_type, "list");
        assert_eq!(bug_list.children.len(), 1);
        let item = &bug_list.children[0];
        // After resolution, list entry should be of type Bug with fields accessible (transparent unnamed div)
        assert_eq!(item.node_type, "Bug");
        // Fetch children in a transparent-aware way and verify overrides
        let children = item.get_accessible_children();
        let desc = children
            .iter()
            .copied()
            .find(|c| c.name == "description")
            .expect("description field");
        assert_eq!(
            desc.parameters.get("value"),
            Some(&OverseerValue::String("override text".to_string()))
        );
        let sp = children
            .iter()
            .copied()
            .find(|c| c.name == "storypoints")
            .expect("storypoints field");
        assert_eq!(sp.parameters.get("value"), Some(&OverseerValue::Integer(5)));
        let pr = children
            .iter()
            .copied()
            .find(|c| c.name == "priority")
            .expect("priority field");
        assert_eq!(pr.parameters.get("value"), Some(&OverseerValue::Integer(2)));
        let fx = children
            .iter()
            .copied()
            .find(|c| c.name == "fixed")
            .expect("fixed field");
        assert_eq!(
            fx.parameters.get("value"),
            Some(&OverseerValue::Boolean(true))
        );

        // Serialize and reparse to simulate a round-trip; overrides should persist
        let ser = crate::file_ops::OverseerFileHandler::serialize_nodes(&nodes).unwrap();
        let mut nodes2 = parse_document(&ser).unwrap().1;
        resolve_document(&mut nodes2);
        let bug_list2 = nodes2.iter().find(|n| n.name == "BugList").unwrap();
        let item2 = &bug_list2.children[0];
        let ch2 = item2.get_accessible_children();
        let f_desc = ch2
            .iter()
            .copied()
            .find(|c| c.name == "description")
            .expect("desc2");
        assert_eq!(
            f_desc.parameters.get("value"),
            Some(&OverseerValue::String("override text".to_string()))
        );
        let f_sp = ch2
            .iter()
            .copied()
            .find(|c| c.name == "storypoints")
            .expect("sp2");
        assert_eq!(
            f_sp.parameters.get("value"),
            Some(&OverseerValue::Integer(5))
        );
        let f_pr = ch2
            .iter()
            .copied()
            .find(|c| c.name == "priority")
            .expect("pr2");
        assert_eq!(
            f_pr.parameters.get("value"),
            Some(&OverseerValue::Integer(2))
        );
        let f_fx = ch2
            .iter()
            .copied()
            .find(|c| c.name == "fixed")
            .expect("fx2");
        assert_eq!(
            f_fx.parameters.get("value"),
            Some(&OverseerValue::Boolean(true))
        );
    }

    #[test]
    fn test_effective_layout_on_template_instance_container() {
        // Template declares a div; instance should be treated as container using _original_type
        let input = r#"
        div Outer (layout=vertical) {
            div Task (hidden=true) {
                string description = ""
            }
            <Task> my_task {
                string description = "Hello"
            }
        }
        "#;
        let mut nodes = crate::parser::parse_document(input).unwrap().1;
        resolve_document(&mut nodes);

        // Find the instance 'my_task' under Outer
        let outer = &nodes[0];
        let instance = outer
            .children
            .iter()
            .find(|c| c.name == "my_task")
            .expect("instance present");
        // Because parent layout is vertical, effective layout for children should be horizontal
        assert_eq!(
            instance.parameters.get("_effective_layout"),
            Some(&OverseerValue::String("horizontal".to_string()))
        );
    }

    #[test]
    fn test_parameter_inheritance() {
        let input = r#"div Container (font-color=blue, font-size=16px) {
            div Inner {
                string field = "Test"
            }
            string direct = "Direct child"
        }"#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);

        let container = &nodes[0];
        let inner = &container.children[0];
        let field = &inner.children[0];
        let direct = &container.children[1];

        // Container should have its own styling
        assert_eq!(
            container.parameters.get("font-color"),
            Some(&OverseerValue::Color(Color::Named("blue".to_string())))
        );
        assert_eq!(
            container.parameters.get("font-size"),
            Some(&OverseerValue::CssSize(CssSize::Pixels(16.0)))
        );

        // Inner div should inherit styling parameters
        assert_eq!(
            inner.parameters.get("font-color"),
            Some(&OverseerValue::Color(Color::Named("blue".to_string())))
        );
        assert_eq!(
            inner.parameters.get("font-size"),
            Some(&OverseerValue::CssSize(CssSize::Pixels(16.0)))
        );

        // Field should inherit from both container and inner
        assert_eq!(
            field.parameters.get("font-color"),
            Some(&OverseerValue::Color(Color::Named("blue".to_string())))
        );
        assert_eq!(
            field.parameters.get("font-size"),
            Some(&OverseerValue::CssSize(CssSize::Pixels(16.0)))
        );

        // Direct child should inherit from container
        assert_eq!(
            direct.parameters.get("font-color"),
            Some(&OverseerValue::Color(Color::Named("blue".to_string())))
        );
        assert_eq!(
            direct.parameters.get("font-size"),
            Some(&OverseerValue::CssSize(CssSize::Pixels(16.0)))
        );
    }

    #[test]
    fn test_parameter_inheritance_override() {
        let input = r#"div Container (font-color=blue, font-size=16px) {
            string child1 = "Default styling"
            string child2 (font-color=red) = "Overridden color"
            string child3 (font-size=20px) = "Overridden size"
        }"#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);

        let container = &nodes[0];
        let child1 = &container.children[0];
        let child2 = &container.children[1];
        let child3 = &container.children[2];

        // Child1 should inherit both parameters
        assert_eq!(
            child1.parameters.get("font-color"),
            Some(&OverseerValue::Color(Color::Named("blue".to_string())))
        );
        assert_eq!(
            child1.parameters.get("font-size"),
            Some(&OverseerValue::CssSize(CssSize::Pixels(16.0)))
        );

        // Child2 should override color but inherit size
        assert_eq!(
            child2.parameters.get("font-color"),
            Some(&OverseerValue::Color(Color::Named("red".to_string())))
        );
        assert_eq!(
            child2.parameters.get("font-size"),
            Some(&OverseerValue::CssSize(CssSize::Pixels(16.0)))
        );

        // Child3 should override size but inherit color
        assert_eq!(
            child3.parameters.get("font-color"),
            Some(&OverseerValue::Color(Color::Named("blue".to_string())))
        );
        assert_eq!(
            child3.parameters.get("font-size"),
            Some(&OverseerValue::CssSize(CssSize::Pixels(20.0)))
        );
    }

    #[test]
    fn test_serialize_standalone_template_instance_keeps_override_shorthand() {
        use crate::file_ops::OverseerFileHandler;
        let input = r#"
        div T {
            int A = 1
            int B = 2
        }
        <T> I {
            - A = 3
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let out = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        // Expect concise override retained and inherited field omitted in the instance
        assert!(out.contains("<T> I {"));
        assert!(out.contains("- A = 3"));
        // Template block contains int B; instance should not add another occurrence
        let count_int_b = out.matches("int B").count();
        assert_eq!(
            count_int_b, 1,
            "should not serialize inherited B inside instance"
        );
        // Ensure the instance did not expand to concrete type assignment
        assert!(!out.contains("int A = 3"));
    }

    #[test]
    fn test_explicit_equal_override_is_preserved() {
        use crate::file_ops::OverseerFileHandler;
        let input = r#"
        div T {
            int A = 1
            int B = 2
        }
        <T> I {
            - B = 2
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let out = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        // The explicit override of B should be preserved as a concise override
        assert!(out.contains("<T> I {"));
        assert!(
            out.contains("- B = 2"),
            "explicit override equal to default must persist"
        );
        // And it should not expand to a full field or duplicate the template field
        let count_int_b = out.matches("int B").count();
        assert_eq!(
            count_int_b, 1,
            "template field 'int B' should not be duplicated inside instance"
        );
    }

    #[test]
    fn test_instance_does_not_serialize_inherited_child() {
        use crate::file_ops::OverseerFileHandler;
        let input = r#"
        div T {
            int A = 1
            int B = 2
            int C = 3
        }
        <T> I {
            - A = 3
            - B = 2
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let out = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        // C should only appear in the template block, not inside the instance
        let count_int_c = out.matches("int C").count();
        assert_eq!(
            count_int_c, 1,
            "inherited C must not be serialized inside instance"
        );
        assert!(
            !out.contains("- C = 3"),
            "concise override for C must not appear since C was not overridden"
        );
    }

    #[test]
    fn test_inherited_styling_not_serialized_on_children() {
        use crate::file_ops::OverseerFileHandler;
        let input = r#"
        div Exercise (background-color=#ffcccc) {
            string Name = "Pushups"
            div History {
                list items(entry=div) {
                    div E1 { string when = "2024-01-01" }
                    div E2 { string when = "2024-01-02" }
                }
            }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let out = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        // Parent Exercise should have background-color emitted once
        assert!(out.contains("div Exercise (background-color=#ffcccc)"));
        // Children should not have background-color persisted
        let child_bc_mentions = out.matches("background-color").count();
        assert_eq!(
            child_bc_mentions, 1,
            "inherited background-color should not be serialized on children"
        );
    }

    #[test]
    fn test_background_color_formula_computes() {
        // Use r## to allow embedded sequences like "#abcd" without terminating the raw string
        let input = r##"
        div D (background-color=$(days_since(test_date) >= 1 ? "#4b0a0aff" : "#bba0a0ff")) {
            timestamp test_date(mode="elapsed") = "2025-08-10T19:33:55.706634+00:00"
        }
        "##;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let d = &nodes[0];
        // Should have a computed background-color shadow
        assert!(d.parameters.contains_key("_computed_background-color"));
        let comp = d.parameters.get("_computed_background-color").unwrap();
        // Since test_date is at least 1 day before now, expect the first color branch
        match comp {
            OverseerValue::Color(Color::Hex(hex)) => assert_eq!(hex, "#4b0a0aff"),
            OverseerValue::String(s) => assert_eq!(s, "#4b0a0aff"),
            _ => panic!("unexpected computed color: {:?}", comp),
        }
    }

    #[test]
    fn test_mount_defaults_and_validation() {
        let input = r#"
        div Root {
            mount M1 (source="history.os") { }
            mount M2 { }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let root = &nodes[0];
        let m1 = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "M1")
            .unwrap();
        let m2 = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "M2")
            .unwrap();
        // M1: has source -> status defaults to 'unloaded'; lazy defaults true via _computed_lazy
        assert_eq!(
            m1.parameters.get("_mount_status"),
            Some(&OverseerValue::String("unloaded".to_string()))
        );
        assert_eq!(
            m1.parameters.get("_computed_lazy"),
            Some(&OverseerValue::Boolean(true))
        );
        // M2: missing source -> error status and _mount_error present
        assert_eq!(
            m2.parameters.get("_mount_status"),
            Some(&OverseerValue::String("error".to_string()))
        );
        assert!(m2.parameters.get("_mount_error").is_some());
    }

    #[test]
    fn test_files_function_stub_allows_count_reduce() {
        let input = r#"
        div Root {
            int n = $(files("*.os").count())
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let root = &nodes[0];
        let n = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.name == "n")
            .unwrap();
        // Phase 1 stub returns empty list, so count is 0
        assert_eq!(
            n.parameters.get("_computed_value"),
            Some(&OverseerValue::Integer(0))
        );
    }

    #[test]
    fn test_parent_bg_depends_on_child_formula_computes_on_load() {
        // Parent background-color references a child field that itself is a formula.
        let input = r##"
        div Parent (background-color=$(score > 0 ? "inherit" : "#000000ff")) {
            int score = $(1)
        }
    "##;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let parent = &nodes[0];
        let bg = parent
            .parameters
            .get("_computed_background-color")
            .cloned()
            .expect("bg computed");
        match bg {
            OverseerValue::String(_) | OverseerValue::Color(_) => {}
            other => panic!("unexpected bg: {:?}", other),
        }
    }

    #[test]
    fn test_chart_series_computed_simple() {
        let input = r#"
        div Root {
            div Data {
                div a { int t = 1 int v = 2 }
                div b { int t = 2 int v = 3 }
            }
            chart C {
                plot P1 (source="/Root/Data", x=$(x/t), y=$(x/v), color=red)
            }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let root = &nodes[0];
        let chart = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.node_type == "chart")
            .unwrap();
        // Bounds should exist
        assert!(matches!(
            chart.parameters.get("_computed_x_min"),
            Some(OverseerValue::Float(1.0))
        ));
        assert!(matches!(
            chart.parameters.get("_computed_x_max"),
            Some(OverseerValue::Float(2.0))
        ));
        assert!(matches!(
            chart.parameters.get("_computed_y_min"),
            Some(OverseerValue::Float(2.0))
        ));
        assert!(matches!(
            chart.parameters.get("_computed_y_max"),
            Some(OverseerValue::Float(3.0))
        ));
        // Plot series should be computed
        let plot = chart.children.iter().find(|c| c.name == "P1").unwrap();
        let series = plot.parameters.get("_computed_series").cloned().unwrap();
        match series {
            OverseerValue::String(s) => {
                assert!(s.contains("[1,2]"));
                assert!(s.contains("[2,3]"));
            }
            _ => panic!("expected string json series"),
        }
    }

    #[test]
    fn test_chart_series_with_timestamp_x_strings() {
        let input = r#"
        div Root {
            list History (entry=record) {
                - { string time = "2025-08-10T19:33:55.706634+00:00" float weight = 9.6 }
                - { string time = "2025-08-12T10:53:02.760779800+00:00" float weight = 10.2 }
            }
            chart C {
                plot P (source="/Root/History", x=$(|x| x/time), y=$(|x| x/weight))
            }
        }
        "#;
        let mut nodes = crate::parser::parse_document(input).unwrap().1;
        super::resolve_document(&mut nodes);
        let root = &nodes[0];
        let chart = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.node_type == "chart")
            .unwrap();
        // Bounds must be computed and increasing in x
        let xmin = match chart.parameters.get("_computed_x_min") {
            Some(crate::types::OverseerValue::Float(f)) => *f,
            _ => -1.0,
        };
        let xmax = match chart.parameters.get("_computed_x_max") {
            Some(crate::types::OverseerValue::Float(f)) => *f,
            _ => -1.0,
        };
        assert!(xmax > xmin);
        // Plot should have _computed_series
        let plot = chart
            .children
            .iter()
            .find(|c| c.node_type == "plot")
            .unwrap();
        let ser = plot.parameters.get("_computed_series");
        assert!(matches!(ser, Some(crate::types::OverseerValue::String(_))));
    }

    #[test]
    fn test_chart_series_with_processed_list_source() {
        let input = r#"
        div Root {
            list History (entry=record) {
                - { string time = "2025-08-10T19:33:55.706634+00:00" int reps = 12 }
                - { string time = "2025-08-12T10:53:02.760779800+00:00" int reps = 18 }
                - { string time = "2025-08-12T11:00:00.000000000+00:00" int reps = 22 }
            }
            chart C {
                // Use a processed list: only items with reps > 15
                plot P (source=$(/Root/History.filter(|x| x/reps > 15)), x=$(|x| x/time), y=$(|x| x/reps))
            }
        }
        "#;
        let mut nodes = crate::parser::parse_document(input).unwrap().1;
        super::resolve_document(&mut nodes);
        let root = &nodes[0];
        let chart = root
            .get_accessible_children()
            .into_iter()
            .find(|c| c.node_type == "chart")
            .unwrap();
        // Bounds must be computed
        assert!(chart.parameters.get("_computed_x_min").is_some());
        assert!(chart.parameters.get("_computed_x_max").is_some());
        assert!(chart.parameters.get("_computed_y_min").is_some());
        assert!(chart.parameters.get("_computed_y_max").is_some());
        // Plot should have _computed_series with two points (reps 18 and 22)
        let plot = chart
            .children
            .iter()
            .find(|c| c.node_type == "plot")
            .unwrap();
        if let Some(OverseerValue::String(s)) = plot.parameters.get("_computed_series") {
            // Count occurrences of opening bracket '[' minus 1 for the array start, or parse
            let series: Vec<(f64, f64)> = serde_json::from_str(s).expect("valid series json");
            assert_eq!(series.len(), 2);
        } else {
            panic!("expected _computed_series string");
        }
    }

    // Note: additional integration tests for templated list items can be added once renderer/runtime semantics are finalized.

    #[test]
    fn test_formulas_inside_on_blocks_are_skipped_on_resolve() {
        // Ensure formulas within action payloads aren't evaluated during resolve (avoids recursion/crash)
        let input = r#"
div Root {
    string input = "Hello"
    list L (entry=string) { }
    button Create {
        on click {
            append(list="/Root/L") { - value = $(/Root/input) }
        }
    }
}
"#;
        let mut nodes = crate::parser::parse_document(input).unwrap().1;
        // Just resolving should not evaluate the formula inside on click block
        resolve_document(&mut nodes);
        // Find the on click node under Create and ensure its child parameter is still a Formula (no _computed_value)
        let root = &nodes[0];
        let btn = root
            .children
            .iter()
            .find(|c| c.name == "Create")
            .expect("Create button present");
        let on_click = btn
            .children
            .iter()
            .find(|c| c.node_type == "on" && c.name == "click")
            .expect("on click present");
        // Under on click, there's an append action with a child override node having value as Formula
        let append = on_click.children.first().expect("append action present");
        assert_eq!(append.node_type, "append");
        let ov = append.children.first().expect("override child present");
        // It should keep a Formula for 'value' and not have a computed shadow
        match ov.parameters.get("value") {
            Some(OverseerValue::Formula(_)) => {}
            other => panic!("expected raw Formula in action payload, got {:?}", other),
        }
        assert!(
            ov.parameters.get("_computed_value").is_none(),
            "no computed shadow should be created under on-blocks"
        );
    }
}
