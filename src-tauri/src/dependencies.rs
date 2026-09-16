//! What each computed value was worked out from.
//!
//! Replaces a graph that tried to predict dependencies by reading formulas. That one got the easy
//! cases right - a field naming a sibling - and missed the two that matter in a real document:
//! an aggregate over a list contributed no edges at all, and an absolute path contributed none
//! either, so a day's totals looked independent of the meals they add up and every day's target
//! looked independent of the target it reads. A graph that is confidently incomplete is worse
//! than none, because what it misses is not recomputed and nobody is told.
//!
//! So this does not predict. It **records**: while a formula is being worked out, every path the
//! evaluator resolves on its behalf is noted against it. What comes out is what was actually read,
//! including the things no static reading could know - which entry of a keyed list a lookup
//! landed on, which branch of a conditional was taken.
//!
//! The recording is deliberately coarse in one direction and never in the other. Resolving
//! `intake` to run an aggregate over it records a dependency on the list, not on the particular
//! entries the aggregate touched: so a change anywhere in that list recomputes the aggregate,
//! which is more work than strictly needed and never less. Erring that way is the whole point -
//! recomputing something that did not need it costs time, and failing to recompute something that
//! did costs a wrong number that nothing reports.
//!
//! # A path is a number while this is running
//!
//! Reads happen tens of millions of times in one resolve of a large document, and the first
//! version of this built a string for each, then cloned it into a set and again into every
//! enclosing frame. Recording cost four times the resolve it was watching.
//!
//! A path is now hashed straight from its segments - no string built - and carried as a number.
//! Its text is kept once, the first time that path is seen, and the graph handed out at the end is
//! made of names again: the saving belongs on the hot path, and everything that reads a graph
//! would rather have names than numbers.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};

/// A path, as it is carried while recording.
type PathId = u64;

thread_local! {
    /// The value currently being worked out, if any, and what it has read so far.
    ///
    /// A stack, because working one value out can require working out another - a formula read
    /// through a node whose own value is a formula, or an answer taken from the memo. What the
    /// inner one reads the outer one reads too, and that is settled when a frame closes rather
    /// than on every read.
    static BEING_WORKED_OUT: RefCell<Vec<(PathId, Vec<PathId>)>> =
        const { RefCell::new(Vec::new()) };

    /// What each value read while it was worked out.
    static RECORDED: RefCell<HashMap<PathId, HashSet<PathId>>> = RefCell::new(HashMap::new());

    /// The text of every path seen, kept once.
    static NAMES: RefCell<HashMap<PathId, String>> = RefCell::new(HashMap::new());

    /// Whether to record at all. Off by default: evaluation happens in places that have nothing
    /// to do with building a graph, and paying for it there would be waste.
    static RECORDING: RefCell<bool> = const { RefCell::new(false) };
}

/// The same number for the same path, whether it arrives in segments or already joined.
///
/// Fed the bytes a joined path would have, so `["a", "b"]` and `"a/b"` cannot disagree - and they
/// must not, because a value is recorded under a joined name and looked up by the segments
/// something else resolved.
fn hash_segments(segments: &[String]) -> PathId {
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for (i, segment) in segments.iter().enumerate() {
        if i > 0 {
            hasher.write_u8(b'/');
        }
        hasher.write(segment.as_bytes());
    }
    hasher.finish()
}

fn hash_path(path: &str) -> PathId {
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hasher.write(path.as_bytes());
    hasher.finish()
}

/// Remember a path's text against its number, the first time it is seen.
fn name_it(id: PathId, text: impl FnOnce() -> String) {
    NAMES.with(|names| {
        names.borrow_mut().entry(id).or_insert_with(text);
    });
}

/// Start recording. Anything previously recorded is discarded.
pub fn start_recording() {
    RECORDING.with(|on| *on.borrow_mut() = true);
    RECORDED.with(|r| r.borrow_mut().clear());
    NAMES.with(|n| n.borrow_mut().clear());
    BEING_WORKED_OUT.with(|s| s.borrow_mut().clear());
}

/// Stop, and take what was recorded, as names.
pub fn take_recording() -> Graph {
    RECORDING.with(|on| *on.borrow_mut() = false);
    BEING_WORKED_OUT.with(|s| s.borrow_mut().clear());
    let recorded = RECORDED.with(|r| std::mem::take(&mut *r.borrow_mut()));
    let names = NAMES.with(|n| std::mem::take(&mut *n.borrow_mut()));

    // Back to names once, here, rather than millions of times while recording.
    let mut reads: HashMap<String, HashSet<String>> = HashMap::with_capacity(recorded.len());
    for (target, sources) in recorded {
        let Some(target_name) = names.get(&target) else {
            continue;
        };
        let named: HashSet<String> = sources
            .into_iter()
            .filter_map(|s| names.get(&s).cloned())
            .collect();
        reads.insert(target_name.clone(), named);
    }
    Graph::from_reads(reads)
}

pub fn is_recording() -> bool {
    RECORDING.with(|on| *on.borrow())
}

/// Note that a value is about to be worked out. The returned guard closes it.
pub struct WorkingOut(bool);

impl WorkingOut {
    pub fn value(path: &str) -> Self {
        if !is_recording() {
            return WorkingOut(false);
        }
        let id = hash_path(path);
        name_it(id, || path.to_string());
        BEING_WORKED_OUT.with(|s| s.borrow_mut().push((id, Vec::new())));
        // Recorded even when it reads nothing, so that "this value exists and depends on
        // nothing" can be told apart from "this value was never looked at".
        RECORDED.with(|r| {
            r.borrow_mut().entry(id).or_default();
        });
        WorkingOut(true)
    }
}

impl Drop for WorkingOut {
    fn drop(&mut self) {
        if self.0 {
            close_frame();
        }
    }
}

/// Note that the value being worked out read this path.
///
/// The hot one. No string is built unless this path has never been seen before.
pub fn note_read(segments: &[String]) {
    if !is_recording() || segments.is_empty() {
        return;
    }
    let id = hash_segments(segments);
    let first_time = BEING_WORKED_OUT.with(|stack| {
        let mut stack = stack.borrow_mut();
        let Some((target, reads)) = stack.last_mut() else {
            return false;
        };
        if *target == id {
            return false; // reading itself says nothing
        }
        reads.push(id);
        true
    });
    if first_time {
        name_it(id, || segments.join("/"));
    }
}

/// Close the innermost frame: write what it read against its own name, and hand the same reads to
/// the frame above.
///
/// The handing up is the propagation - working one value out can require working out another, and
/// what the inner one read the outer one read too. Done here, in bulk, rather than on every read.
fn close_frame() -> Vec<String> {
    let Some((target, reads)) = BEING_WORKED_OUT.with(|s| s.borrow_mut().pop()) else {
        return Vec::new();
    };
    RECORDED.with(|r| {
        let mut recorded = r.borrow_mut();
        recorded.entry(target).or_default().extend(reads.iter().copied());
    });
    BEING_WORKED_OUT.with(|s| {
        if let Some((_, above)) = s.borrow_mut().last_mut() {
            above.extend(reads.iter().copied());
        }
    });
    // Named for the one caller that keeps them: the memo, which hands the same reads back every
    // later time it answers from its cache.
    NAMES.with(|names| {
        let names = names.borrow();
        reads
            .into_iter()
            .filter_map(|id| names.get(&id).cloned())
            .collect()
    })
}

/// Work something out under its own name, and say what it read.
///
/// For a memoised answer: the reads belong to the answer, so the next thing handed that answer can
/// be told the same ones. Gives back nothing when not recording, and does not build the name.
pub fn reads_while<T>(name: impl FnOnce() -> String, work: impl FnOnce() -> T) -> (T, Vec<String>) {
    if !is_recording() {
        return (work(), Vec::new());
    }
    let named = name();
    let id = hash_path(&named);
    name_it(id, || named);
    BEING_WORKED_OUT.with(|s| s.borrow_mut().push((id, Vec::new())));
    let out = work();
    (out, close_frame())
}

/// Attribute reads that something else made earlier to whatever is being worked out now.
pub fn replay_reads(reads: &[String]) {
    if !is_recording() || reads.is_empty() {
        return;
    }
    for read in reads {
        let id = hash_path(read);
        let first_time = BEING_WORKED_OUT.with(|stack| {
            let mut stack = stack.borrow_mut();
            let Some((target, collected)) = stack.last_mut() else {
                return false;
            };
            if *target == id {
                return false;
            }
            collected.push(id);
            true
        });
        if first_time {
            name_it(id, || read.clone());
        }
    }
}

/// What was read by what, and the other way round.
#[derive(Debug, Default, Clone)]
pub struct Graph {
    /// value -> everything it read
    reads: HashMap<String, HashSet<String>>,
    /// path -> every value that read it
    read_by: HashMap<String, HashSet<String>>,
}

impl Graph {
    fn from_reads(reads: HashMap<String, HashSet<String>>) -> Self {
        let mut graph = Graph {
            reads,
            read_by: HashMap::new(),
        };
        graph.index();
        graph
    }

    fn index(&mut self) {
        let mut read_by: HashMap<String, HashSet<String>> = HashMap::new();
        for (target, sources) in &self.reads {
            for source in sources {
                read_by
                    .entry(source.clone())
                    .or_default()
                    .insert(target.clone());
            }
        }
        self.read_by = read_by;
    }

    pub fn is_empty(&self) -> bool {
        self.reads.is_empty()
    }

    /// How many values were recorded.
    pub fn len(&self) -> usize {
        self.reads.len()
    }

    /// Every value that was recorded, for looking around a graph rather than asking it about a
    /// path you already knew.
    pub fn values(&self) -> Vec<&String> {
        self.reads.keys().collect()
    }

    /// What this value was worked out from.
    pub fn reads(&self, value: &str) -> Vec<String> {
        self.reads
            .get(value)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Whether anything in this document was worked out from what time it is.
    ///
    /// A document that never asks cannot go stale while its text is unchanged - every answer in it
    /// is a function of the text alone - so it can be handed back as it was rather than worked out
    /// again. A list of tasks asks on every row, because a priority climbs by the day and a
    /// deadline passes; a shopping list never does.
    pub fn reads_the_clock(&self) -> bool {
        self.read_by.contains_key(crate::formula_evaluator::CLOCK)
    }

    /// The node a recorded key belongs to.
    ///
    /// A key is a path and a parameter joined by `#` - but a path segment can carry a `#` of its
    /// own, because the resolver tells duplicate siblings apart that way: the second unnamed div
    /// is `div#2`. Splitting on the first `#` turns `project/div#2/points_open#_computed_value`
    /// into `project/div`, which names a different node, and an edit then recomputes the wrong
    /// thing and leaves the right thing stale. The parameter is always last and always begins
    /// `_computed`.
    pub fn node_of(key: &str) -> &str {
        match key.rfind("#_computed") {
            Some(at) => &key[..at],
            None => key,
        }
    }

    /// The nodes that have to be worked out again when these paths change.
    ///
    /// The same answer as `cascade`, with the shadow keys dropped: a node is the unit the resolver
    /// deals in, and asking it to redo one is asking it to redo all of that node's values. The
    /// changed paths are included, because whatever changed has to be reconsidered too - an edit to
    /// a field with a fallback changes which of them applies.
    pub fn nodes_to_work_out_again(&self, changed: &[String]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for key in changed.iter().cloned().chain(self.cascade(changed)) {
            let node = Self::node_of(&key).to_string();
            if seen.insert(node.clone()) {
                out.push(node);
            }
        }
        out
    }

    /// Take in what a later, smaller resolve recorded.
    ///
    /// A node that was worked out again has said afresh what it read, and that replaces whatever it
    /// said before - which is what keeps the graph honest when an edit changes where a lookup
    /// lands. Nodes the smaller resolve did not touch keep the edges they had.
    pub fn absorb(&mut self, newer: Graph) {
        for (value, sources) in newer.reads {
            self.reads.insert(value, sources);
        }
        self.index();
    }

    /// Everything that has to be worked out again when these paths change, nearest first.
    ///
    /// A value that read a path depends on it; so does a value that read anything *under* it,
    /// because a list is read as a whole and its entries are what changed. The changed paths
    /// themselves are not included - the caller already has those.
    pub fn cascade(&self, changed: &[String]) -> Vec<String> {
        let mut seen: HashSet<String> = HashSet::new();
        let mut order: Vec<String> = Vec::new();
        let mut queue: VecDeque<String> = changed.iter().cloned().collect();

        while let Some(path) = queue.pop_front() {
            for dependent in self.readers_of(&path) {
                if seen.insert(dependent.clone()) {
                    order.push(dependent.clone());
                    queue.push_back(dependent);
                }
            }
        }
        order
    }

    /// Everything that read this path, or read a container this path sits inside.
    ///
    /// The second half is what makes an aggregate work: `intake.map(...).sum()` records a read of
    /// the list, and what changed is one meal inside it.
    fn readers_of(&self, path: &str) -> Vec<String> {
        let mut found: HashSet<String> = HashSet::new();
        if let Some(direct) = self.read_by.get(path) {
            found.extend(direct.iter().cloned());
        }
        // Every ancestor of the changed path: a read of the list covers a change to its entries.
        let mut walk = path;
        while let Some(cut) = walk.rfind('/') {
            walk = &walk[..cut];
            if let Some(above) = self.read_by.get(walk) {
                found.extend(above.iter().cloned());
            }
        }
        found.into_iter().collect()
    }
}
