use crate::formula_evaluator::{BoundValue, EvaluationContext, FormulaEvaluator};
use crate::resolver;
use crate::types::{
    NodeSourceSnapshot, OverseerError, OverseerNode, OverseerValue, SnapshotOrigin,
};
use chrono::{Duration, Local, Utc};

#[derive(Clone, Debug, Default)]
struct ListEntryStyleGuide {
    leading_blank_lines: u8,
    newline: Option<String>,
    indent_unit: Option<String>,
}


// Debug logging macro for actions
macro_rules! debug_actions {
    ($($arg:tt)*) => {
    #[cfg(feature = "debug-resolver")]
        println!($($arg)*);
    };
}


thread_local! {
    /// Whether bringing a mount in should resolve the document afterwards.
    ///
    /// True for a `load` that a person or a rule triggered: the actions after it in the same
    /// block have to see what arrived. False while preloading, where the caller resolves once as
    /// soon as every mount is in - there, resolving per mount is a full pass over every node in
    /// the document for an answer that is about to be computed again.
    static RESOLVE_AFTER_MOUNT: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

/// Restores the setting when it goes out of scope, including on the way out of an error.
struct MountResolveSuspended;

impl Drop for MountResolveSuspended {
    fn drop(&mut self) {
        RESOLVE_AFTER_MOUNT.with(|flag| flag.set(true));
    }
}

/// The file a mount was read from, on the mount - see `mounts_held`.
pub const MOUNT_FILE: &str = "_mount_file";
/// How that file stood when it was read - see `mount_stamp`.
pub const MOUNT_STAMP: &str = "_mount_stamp";

/// When a file was last written and how long it is: what decides whether something read from it
/// is still what it says. Anything that changes the file changes at least one of them.
fn file_stamp(path: &str) -> Option<(std::time::SystemTime, u64)> {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok().map(|t| (t, m.len())))
}

/// The same, as the text a mount keeps it in. A file that is not there has a stamp of its own, so
/// a mount that failed for want of one is found stale when it appears.
pub fn mount_stamp(path: &str) -> String {
    match file_stamp(path) {
        Some((written, length)) => {
            let nanos = written
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default();
            format!("{}:{}", nanos, length)
        }
        None => "absent".to_string(),
    }
}

/// Every file a document's mounts were read from, and how each stood then.
///
/// A document worked out and held has what its mounts brought in inside it - the food catalog,
/// inside the food tracker - and is held under its own text, which a food added to the catalog
/// does not change. So it went on being handed out with the catalog as it was: a food the bot had
/// just added could not be recorded, and every figure of the meal was an error, until something
/// changed the tracker's own text. Held with this, it is only handed out while every one of these
/// files still stands as it did - see `document_cache`.
pub fn mounts_held(nodes: &[OverseerNode]) -> Vec<(String, String)> {
    fn walk(nodes: &[OverseerNode], out: &mut Vec<(String, String)>) {
        for node in nodes {
            if node.node_type == "mount" {
                if let (Some(OverseerValue::String(file)), Some(OverseerValue::String(stamp))) =
                    (node.parameters.get(MOUNT_FILE), node.parameters.get(MOUNT_STAMP))
                {
                    if !out.iter().any(|(f, s)| f == file && s == stamp) {
                        out.push((file.clone(), stamp.clone()));
                    }
                }
                // What a mount brought in is a document of its own, whose mounts it held when
                // it was read - not this one's to answer for.
                continue;
            }
            walk(&node.children, out);
        }
    }
    let mut out = Vec::new();
    walk(nodes, &mut out);
    out
}

/// What an action touched.
///
/// An action reported nothing until now, so the only safe thing to do after one was to serialize
/// the document, parse it again and resolve the lot - because an append or a remove moves the
/// addresses of everything after it, and nothing said whether this action was that kind. Most are
/// not: a `set`, an `inc`, a `toggle` change one value, and the dependency graph already knows
/// what reads it.
#[derive(Default, Debug, Clone)]
pub struct Changed {
    /// The addresses whose values moved, named the way the dependency graph names them.
    pub fields: Vec<String>,
    /// Whether the shape of the document changed - an entry added, removed or reordered. When it
    /// did, every address after the change may mean something else.
    pub structural: bool,
    /// The changes of shape that can be followed, in the order they were made: an entry made in a
    /// list or taken out of one. The graph can be carried through those - see
    /// `app_api::follow_the_shape` - rather than the whole document worked out again.
    pub shapes: Vec<Shape>,
    /// Whether the shape moved in a way `shapes` does not say - a list cleared, reordered or
    /// copied into, a mount brought in. Then nothing short of working the document out whole will
    /// do, which is what every change of shape did before `shapecost`.
    pub shape_unknown: bool,
    /// Writes the document said belong to whoever is looking rather than to the document -
    /// `mutable="guarded"`, in force here or anywhere above. They are named and their values
    /// carried out rather than written down, and the caller decides where a viewer's state
    /// lives. The document is left exactly as it was.
    pub view_state: Vec<(String, OverseerValue)>,
    /// Whether anything ran that could have changed the document.
    ///
    /// A press whose handler only asked for a field to be opened, or whose `if` let nothing
    /// through, leaves the document exactly as it was - and working it out again anyway cost as
    /// much as opening it, which on tasks.os is most of a second between tapping and typing.
    ///
    /// Erring towards yes, because a no skips working the document out: every action but those
    /// two says yes, so does noting any change, and so does every way into this module that
    /// changes a document without an action - a write, an entry made or taken out.
    pub acted: bool,
    /// A field the press asked to have opened for editing, by address - see `start_editing`.
    ///
    /// Opening one is the page's business, so the action names it and the page does it. An
    /// address rather than child indices, because a later action in the same press may add or
    /// remove entries, and the indices are only worked out once the document has settled.
    pub start_editing: Option<String>,
    /// Divs the press asked to have folded, unfolded or toggled, by address and in order - see
    /// `fold`. A fold is a view, never a fact, so like opening a field it is the page's to do.
    pub folds: Vec<(String, FoldHow)>,
}

/// What a fold action asks of the div it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FoldHow {
    Fold,
    Unfold,
    Toggle,
}

/// An entry made in a list or taken out of one, named the way the dependency graph names them.
///
/// `entry` is what the entry was called at that moment. In a list that names its entries by place
/// that is not what the text calls it afterwards - see `resolver::name_entries_as_parsed` - and
/// the settle works out the difference.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Added { list: String, entry: String },
    Removed { list: String, entry: String },
}

impl Shape {
    pub fn list(&self) -> &str {
        match self {
            Shape::Added { list, .. } | Shape::Removed { list, .. } => list,
        }
    }
}

/// A write whose actions change the document worked out for `text`, whose graph says what reads
/// what - see `follow_against`.
pub struct Following {
    pub text: String,
    /// The graph, once a change of shape has been followed with it between two actions. Taken
    /// out of the cache for the rest of the write, because it describes the document as it is
    /// becoming rather than the text it was held under - and a write that fails halfway leaves
    /// no graph behind rather than a wrong one.
    pub graph: Option<crate::dependencies::Graph>,
    /// How many of the report's fields a settle between actions has already seen to.
    pub fields_settled: usize,
}

thread_local! {
    /// Whether whoever asked is going to work the document out afterwards - see
    /// `caller_will_settle_it`.
    static CALLER_SETTLES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// See `follow_against`.
    static FOLLOWING: std::cell::RefCell<Option<Following>> = const { std::cell::RefCell::new(None) };
    /// Collected here rather than returned, because an action runs six levels down through
    /// `if` blocks and mount handlers, and threading a return through all of them would touch
    /// every arm to say nothing. The same shape as `dependencies::start_recording`.
    static REPORT: std::cell::RefCell<Option<Changed>> = const { std::cell::RefCell::new(None) };
}

/// Start noting what actions change. Anything previously noted is discarded.
///
/// For a caller that does not work the document out afterwards, so the event settles it itself.
pub fn start_reporting() {
    CALLER_SETTLES.with(|it| it.set(false));
    FOLLOWING.with(|f| *f.borrow_mut() = None);
    REPORT.with(|r| *r.borrow_mut() = Some(Changed::default()));
}

/// Say that the document these actions change is the one worked out for this text, as the cache
/// holds it - so the graph held for the text describes it, and a change of shape can be followed
/// through the graph rather than the document worked out whole. Said after reporting starts, and
/// forgotten when it next does.
///
/// Only a caller holding exactly that document may say it: one worked out for a viewer, or with
/// something held in view, is not what the graph was recorded against.
///
/// Forgotten when the guard goes, so a write that fails halfway leaves nothing behind for the next
/// thing this thread runs to follow a different document with.
#[must_use]
pub fn follow_against(text: &str) -> FollowingGuard {
    FOLLOWING.with(|f| {
        *f.borrow_mut() = Some(Following { text: text.to_string(), graph: None, fields_settled: 0 })
    });
    FollowingGuard
}

pub struct FollowingGuard;

impl Drop for FollowingGuard {
    fn drop(&mut self) {
        FOLLOWING.with(|f| *f.borrow_mut() = None);
    }
}

/// Stop following, and take what was being followed.
pub(crate) fn stop_following() -> Option<Following> {
    FOLLOWING.with(|f| f.borrow_mut().take())
}

/// Carry on following, with what a settle between actions made of it.
pub(crate) fn keep_following(following: Following) {
    FOLLOWING.with(|f| *f.borrow_mut() = Some(following));
}

/// What the report says so far, for a settle between actions: the fields, the changes of shape not
/// yet seen to, and whether one could not be said.
pub(crate) fn report_so_far() -> Option<(Vec<String>, Vec<Shape>, bool)> {
    REPORT.with(|r| {
        r.borrow()
            .as_ref()
            .map(|changed| (changed.fields.clone(), changed.shapes.clone(), changed.shape_unknown))
    })
}

/// The first `seen` changes of shape have been seen to, and are not the caller's to settle again.
pub(crate) fn shapes_seen_to(seen: usize) {
    REPORT.with(|r| {
        if let Some(changed) = r.borrow_mut().as_mut() {
            changed.shapes.drain(..seen.min(changed.shapes.len()));
        }
    });
}

/// Stop, and take what was noted. `None` when nobody asked.
pub fn take_report() -> Option<Changed> {
    REPORT.with(|r| r.borrow_mut().take())
}

/// One value moved, at this address.
fn note_field(address: String) {
    REPORT.with(|r| {
        if let Some(changed) = r.borrow_mut().as_mut() {
            changed.acted = true;
            if !changed.fields.contains(&address) {
                changed.fields.push(address);
            }
        }
    });
}

/// Whether whoever asked for this event is going to work the document out afterwards.
///
/// Said by the caller rather than guessed at. Three of the four callers that collect a report
/// do settle - both of `execute_event_update`'s endings resolve, `change_document` resolves or
/// asks the graph, and the bot's door resolves outright - so the resolve at the end of the event
/// is work done twice for them. `execute_event_on_text` does not, and has to be told apart.
///
/// It used to be guessed at, as "a report is being collected and the shape has not moved", and
/// the second half of that was wrong: a caller that settles does so whether the shape moved or
/// not. What it cost was exactly the case the guess was meant to protect - pressing a button
/// that appends worked the whole document out at the end of the event and again in the caller,
/// which on the food tracker was 800 ms of the 1,600 a logged meal took.
fn caller_will_settle_it() -> bool {
    CALLER_SETTLES.with(|it| it.get()) && REPORT.with(|r| r.borrow().is_some())
}

/// A write the document says is the viewer's. Noted and not made.
fn note_view_state(address: String, value: OverseerValue) {
    REPORT.with(|r| {
        if let Some(changed) = r.borrow_mut().as_mut() {
            changed.acted = true;
            changed.view_state.push((address, value));
        }
    });
}

/// The same, for a caller that will work the document out once the event is done.
///
/// Saying so is what lets the event skip the resolve at its end. Say it only if you resolve
/// afterwards on every path out, including the one where the shape moved.
pub fn start_reporting_and_settling() {
    start_reporting();
    CALLER_SETTLES.with(|it| it.set(true));
}

thread_local! {
    /// The textboxes a copy took its values from during this press, named the way the page named
    /// them - see `hold_typed_text`.
    static EMPTIED: std::cell::RefCell<Vec<Vec<String>>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Text typed into textboxes, held on the document for the length of one press.
///
/// What a textbox holds while someone types belongs to the page, never to the file: the file says
/// what the box starts with, and that is all it ever says. But a press has to be able to read what
/// was typed - copying a form into a new entry is the reason textboxes exist - so the page sends
/// the text along with the press, and it is put on the boxes here, where actions and the formulas
/// in them read it like any other value. `let_go_of_typed_text` puts back what the file said
/// before anything is worked out, cached or written, so the text never gets further than the press.
///
/// Only onto textboxes. Anything else named here is left alone: this is a way to say what was
/// typed, not a way to write a field without it being written.
pub struct TypedText {
    held: std::collections::HashMap<String, (Option<OverseerValue>, Option<OverseerValue>)>,
}

/// The marker a held textbox carries, saying which of the page's paths it is.
const TYPED_FROM: &str = "_typed_from";

pub fn hold_typed_text(nodes: &mut Vec<OverseerNode>, typed: &[(Vec<String>, OverseerValue)]) -> TypedText {
    EMPTIED.with(|e| e.borrow_mut().clear());
    let mut held = std::collections::HashMap::new();
    for (path, value) in typed {
        let Some((_, indices)) = ActionExecutor::get_node_mut_by_path(nodes, path) else { continue };
        let Some(node) = ActionExecutor::get_node_mut_by_indices(nodes, &indices) else { continue };
        if node.node_type != "textbox" {
            continue;
        }
        let text = match value {
            OverseerValue::String(s) => s.clone(),
            other => match crate::actions::as_text(other) {
                Some(s) => s,
                None => continue,
            },
        };
        let key = serde_json::to_string(path).unwrap_or_default();
        held.insert(
            key.clone(),
            (node.parameters.get("value").cloned(), node.parameters.get("_computed_value").cloned()),
        );
        node.parameters.insert("value".to_string(), OverseerValue::String(text));
        node.parameters.remove("_computed_value");
        node.parameters.insert(TYPED_FROM.to_string(), OverseerValue::String(key));
    }
    TypedText { held }
}

/// Put back what the file says the textboxes start with - anywhere a held one ended up, since a
/// whole-node copy can have taken one somewhere else.
pub fn let_go_of_typed_text(nodes: &mut Vec<OverseerNode>, typed: TypedText) {
    if typed.held.is_empty() {
        return;
    }
    fn walk(nodes: &mut [OverseerNode], held: &std::collections::HashMap<String, (Option<OverseerValue>, Option<OverseerValue>)>) {
        for node in nodes.iter_mut() {
            if let Some(OverseerValue::String(key)) = node.parameters.remove(TYPED_FROM) {
                if let Some((value, computed)) = held.get(&key) {
                    match value {
                        Some(v) => node.parameters.insert("value".to_string(), v.clone()),
                        None => node.parameters.remove("value"),
                    };
                    match computed {
                        Some(v) => node.parameters.insert("_computed_value".to_string(), v.clone()),
                        None => node.parameters.remove("_computed_value"),
                    };
                }
            }
            walk(&mut node.children, held);
        }
    }
    walk(nodes, &typed.held);
}

/// The textboxes a copy used during this press, for the page to empty. Taken once.
pub fn take_emptied() -> Vec<Vec<String>> {
    EMPTIED.with(|e| std::mem::take(&mut *e.borrow_mut()))
}

fn note_emptied(key: &str) {
    if let Ok(path) = serde_json::from_str::<Vec<String>>(key) {
        EMPTIED.with(|e| {
            let mut e = e.borrow_mut();
            if !e.contains(&path) {
                e.push(path);
            }
        });
    }
}

/// A value as text, for a field that holds text or for a conversion that starts from it.
pub(crate) fn as_text(value: &OverseerValue) -> Option<String> {
    match value {
        OverseerValue::String(s) | OverseerValue::Date(s) | OverseerValue::Timestamp(s) => Some(s.clone()),
        OverseerValue::Integer(n) => Some(n.to_string()),
        OverseerValue::Float(f) => Some(if f.fract() == 0.0 && f.abs() < 1e15 {
            format!("{}", *f as i64)
        } else {
            format!("{}", f)
        }),
        OverseerValue::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

/// A value as a field of this type holds it, when it can be said that way.
///
/// What lets a form of textboxes fill an entry: everything typed is text, and the entry wants a
/// number here and a date there. `None` when it cannot be converted - and for nothing at all, so an
/// empty box leaves the template's own default standing rather than writing an empty one over it.
pub(crate) fn converted(value: &OverseerValue, to: &str) -> Option<OverseerValue> {
    let text = as_text(value)?;
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    // A decimal comma, as a phone keyboard in half of Europe types it.
    let number = || t.replace(',', ".");
    match to {
        "string" | "text" | "textbox" | "tags" | "enum" => Some(OverseerValue::String(text.clone())),
        "int" => match value {
            OverseerValue::Integer(n) => Some(OverseerValue::Integer(*n)),
            _ => number()
                .parse::<i64>()
                .ok()
                .or_else(|| number().parse::<f64>().ok().filter(|f| f.fract() == 0.0 && f.abs() < 9e15).map(|f| f as i64))
                .map(OverseerValue::Integer),
        },
        "float" => match value {
            OverseerValue::Float(f) => Some(OverseerValue::Float(*f)),
            OverseerValue::Integer(n) => Some(OverseerValue::Float(*n as f64)),
            _ => number().parse::<f64>().ok().filter(|f| f.is_finite()).map(OverseerValue::Float),
        },
        "bool" | "checkbox" => match t.to_ascii_lowercase().as_str() {
            "true" | "yes" | "1" | "on" => Some(OverseerValue::Boolean(true)),
            "false" | "no" | "0" | "off" => Some(OverseerValue::Boolean(false)),
            _ => None,
        },
        "date" => {
            let day = t.get(..10).unwrap_or(t);
            chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
                .ok()
                .filter(|_| t.len() == 10 || chrono::DateTime::parse_from_rfc3339(t).is_ok())
                .map(|_| OverseerValue::Date(day.to_string()))
        }
        "timestamp" => {
            if chrono::DateTime::parse_from_rfc3339(t).is_ok() {
                Some(OverseerValue::Timestamp(t.to_string()))
            } else {
                chrono::NaiveDate::parse_from_str(t, "%Y-%m-%d")
                    .ok()
                    .map(|_| OverseerValue::Timestamp(format!("{}T00:00:00Z", t)))
            }
        }
        _ => None,
    }
}

/// Something ran that may have changed the document.
fn note_acted() {
    REPORT.with(|r| {
        if let Some(changed) = r.borrow_mut().as_mut() {
            changed.acted = true;
        }
    });
}

/// The field to open for editing once the press is answered. The last one asked for wins.
fn note_start_editing(address: String) {
    REPORT.with(|r| {
        if let Some(changed) = r.borrow_mut().as_mut() {
            changed.start_editing = Some(address);
        }
    });
}

/// A div to fold, unfold or toggle once the press is answered, after any asked for before it.
fn note_fold(address: String, how: FoldHow) {
    REPORT.with(|r| {
        if let Some(changed) = r.borrow_mut().as_mut() {
            changed.folds.push((address, how));
        }
    });
}

/// The document's shape moved, so no address can be trusted to still mean what it did.
fn note_structural() {
    REPORT.with(|r| {
        if let Some(changed) = r.borrow_mut().as_mut() {
            changed.acted = true;
            changed.structural = true;
            changed.shape_unknown = true;
        }
    });
}

/// The document's shape moved in a way that can be said - see `Shape`.
fn note_shape(shape: Shape) {
    REPORT.with(|r| {
        if let Some(changed) = r.borrow_mut().as_mut() {
            changed.acted = true;
            changed.structural = true;
            changed.shapes.push(shape);
        }
    });
}

/// Whatever a settle between actions could not follow, the caller cannot either.
pub(crate) fn note_shape_unknown() {
    REPORT.with(|r| {
        if let Some(changed) = r.borrow_mut().as_mut() {
            changed.shape_unknown = true;
        }
    });
}

/// Where a new entry goes when the list already has some.
///
/// A keyed history is read in whatever order `sort_by` says and written at either end, so which
/// end is a choice the document makes rather than something to assume. A view that materialises
/// its key says which through `phantom-materialize`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhereItGoes {
    First,
    Last,
}

impl WhereItGoes {
    /// Read from what was said, however it was said it.
    ///
    /// `first` and `prepend` mean the front; anything else, including nothing at all, means the
    /// back - which is what a list did before it could be asked. The page passes on what
    /// `phantom-materialize` says, so `prepend-on-edit` arrives here as `prepend`.
    pub fn from_said(said: Option<&str>) -> Self {
        match said.map(|s| s.trim().to_lowercase()) {
            Some(word) if word == "first" || word == "prepend" || word == "prepend-on-edit" => {
                WhereItGoes::First
            }
            _ => WhereItGoes::Last,
        }
    }
}

pub struct ActionExecutor;

impl ActionExecutor {
    // Convert an OverseerValue into a boolean using common truthiness rules
    fn to_bool(v: &OverseerValue) -> bool {
        match v {
            OverseerValue::Boolean(b) => *b,
            OverseerValue::String(s) => s.eq_ignore_ascii_case("true") || s == "1",
            OverseerValue::Integer(i) => *i != 0,
            OverseerValue::Float(f) => *f != 0.0,
            OverseerValue::Date(_) | OverseerValue::Timestamp(_) => true,
            _ => false,
        }
    }


    /// Build a disambiguated name path (using name#k when needed) from an indices chain
    fn build_disambiguated_path(nodes: &Vec<OverseerNode>, indices: &[usize]) -> Vec<String> {
        fn eff(n: &OverseerNode) -> &str {
            if !n.name.is_empty() {
                &n.name
            } else {
                &n.node_type
            }
        }
        let mut out: Vec<String> = Vec::new();
        if indices.is_empty() {
            return out;
        }
        // root
        let root_idx = indices[0];
        if root_idx >= nodes.len() {
            return out;
        }
        let root = &nodes[root_idx];
        let mut count = 0usize;
        for n in nodes.iter().take(root_idx) {
            if eff(n) == eff(root) {
                count += 1;
            }
        }
        let mut seg = eff(root).to_string();
        if count > 0 {
            seg = format!("{}#{}", seg, count);
        }
        out.push(seg);
        // Descend, naming each step the way every other path is named: a wrapper is no step,
        // and a repeated name is numbered among everything its level holds with the wrappers
        // looked through - see `addressing::Level`. This is what a write reports it changed and
        // what an action's formulas are worked out against, so it has to say what the dependency
        // graph and the resolver say, or an edit reaches nothing that reads it.
        fn ordinal(level: &OverseerNode, target: *const OverseerNode, name: &str) -> usize {
            fn walk(nodes: &[OverseerNode], target: *const OverseerNode, name: &str, met: &mut usize) -> bool {
                for n in nodes {
                    if std::ptr::eq(n, target) {
                        return true;
                    }
                    if crate::addressing::is_wrapper(n) {
                        if walk(&n.children, target, name, met) {
                            return true;
                        }
                    } else if eff(n) == name {
                        *met += 1;
                    }
                }
                false
            }
            let mut met = 0;
            walk(&level.children, target, name, &mut met);
            met
        }
        let mut cur: &OverseerNode = root;
        let mut level: &OverseerNode = root;
        for idx in indices.iter().skip(1) {
            if *idx >= cur.children.len() {
                break;
            }
            let child = &cur.children[*idx];
            cur = child;
            if crate::addressing::is_wrapper(child) {
                continue;
            }
            let base = eff(child);
            let prior = ordinal(level, child as *const OverseerNode, base);
            out.push(if prior > 0 { format!("{}#{}", base, prior) } else { base.to_string() });
            level = child;
        }
        out
    }
    fn opposite_layout(layout: &str) -> String {
        match layout {
            "horizontal" => "vertical".to_string(),
            _ => "horizontal".to_string(), // default opposite of vertical
        }
    }
    /// Execute an on <eventName> block under the node at node_path, mutating the document.
    /// Transaction: apply actions in order, then run resolve_document once.
    /// Whether an action can add, remove or move nodes, as opposed to only writing values.
    fn action_changes_structure(action_type: &str) -> bool {
        matches!(
            action_type,
            "append"
                | "prepend"
                | "remove"
                | "clear"
                | "clear_list"
                | "move"
                | "sort"
                | "ensure_in_list"
                | "set_in_list"
                | "load"
                | "unload"
                | "load_mount"
                | "unload_mount"
                | "if"
        )
    }

    pub fn execute_event(
        nodes: &mut Vec<OverseerNode>,
        node_path: &[String],
        event_name: &str,
    ) -> Result<(), OverseerError> {
        // One reading of the clock for every action in the press and whatever it works out -
        // see `FormulaEvaluator::pin_the_clock`.
        let _clock = crate::formula_evaluator::FormulaEvaluator::pin_the_clock();
        debug_actions!(
            "[ACTIONS] execute_event at {:?} on '{}'",
            node_path,
            event_name
        );
        // 1) Locate owning node mutably by path
        let (owner_ptr, owner_indices) = match Self::get_node_mut_by_path(nodes, node_path) {
            Some(res) => res,
            None => {
                #[cfg(feature = "debug-resolver")]
                eprintln!("[ACTIONS] Owner node not found at path {:?}", node_path);
                return Err(OverseerError::ValidationError(format!(
                    "Owner node not found at path {:?}",
                    node_path
                )));
            }
        };

        // SAFETY: we use raw pointer to allow nested borrows during traversal of action children
        let owner: &mut OverseerNode = unsafe { &mut *owner_ptr };
        // Build an internal, disambiguated path for evaluation contexts. Read from the document
        // itself: a copy of the whole of it, made only to read a path, was a fifth of what a press
        // on tasks.os cost once it no longer worked the document out again.
        let owner_eval_path = Self::build_disambiguated_path(nodes, &owner_indices);

        // 2) Find matching on block(s)
        #[cfg(feature = "debug-resolver")]
        eprintln!(
            "[ACTIONS] Owner: {} (type={}) children: {:?}",
            owner.name,
            owner.node_type,
            owner
                .children
                .iter()
                .map(|c| format!("{}/{}", c.node_type, c.name))
                .collect::<Vec<_>>()
        );
        // Default mount behavior: if a mount receives 'load'/'unload' and has no explicit on-block,
        // perform the corresponding action implicitly.
        if owner.node_type == "mount" && (event_name == "load" || event_name == "unload") {
            let has_on = owner
                .children
                .iter()
                .any(|c| c.node_type == "on" && c.name == event_name);
            if !has_on {
                // Loading or unloading changes what the document holds, whether or not anything
                // written says so - and its shape.
                note_structural();
                match event_name {
                    "load" => {
                        // Perform load
                        Self::perform_load_mount_on_owner(
                            nodes,
                            owner_ptr,
                            &owner_indices,
                            &owner_eval_path,
                        )?;
                        if RESOLVE_AFTER_MOUNT.with(|flag| flag.get()) {
                            resolver::resolve_document(nodes);
                        }
                        return Ok(());
                    }
                    "unload" => {
                        Self::perform_unload_mount_on_owner(nodes, owner_ptr, &owner_indices)?;
                        resolver::resolve_document(nodes);
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }
        // Whether anything has been written since the last resolve.
        let mut pending_changes = false;
        for child in owner.children.clone() {
            if child.node_type == "on" && child.name == event_name {
                // Execute each action child in order
                let in_order = child.children;
                let last = in_order.len().saturating_sub(1);
                for (at, action) in in_order.into_iter().enumerate() {
                    #[cfg(feature = "debug-resolver")]
                    eprintln!(
                        "[ACTIONS] Action node: type='{}' name='{}' params={:?}",
                        action.node_type, action.name, action.parameters
                    );
                    Self::execute_action(nodes, &owner_indices, &owner_eval_path, &action)?;
                    // Re-resolve between actions only when the tree's shape may have moved,
                    // and only when there is a later action to traverse it.
                    //
                    // A full resolve is expensive - templates, formulas, chart series, sort
                    // keys, over every node - and running one after each action meant a
                    // six-action button paid for seven of them. Actions that only write a
                    // value cannot add or remove nodes, so template instantiation cannot
                    // change and the final resolve below already recomputes everything that
                    // depends on the values written. Structural actions genuinely do change
                    // the tree that later actions traverse, so those resolve in place.
                    //
                    // Between actions, though - not after the last one. The Add buttons on a
                    // day hold a single `append` each, so resolving in place settled a tree
                    // nothing was going to read before the caller settled it again: two full
                    // resolves of the document for one press, and on the food tracker that was
                    // 800 ms of the 1,600 a logged meal cost. A structural action at the end
                    // leaves the change pending like any other, and the resolve below or the
                    // caller answers for it.
                    //
                    // And in place means what the change reaches, where the graph can follow it:
                    // `done` on a task appends to the history and then removes the task, and the
                    // resolve between the two was a third of the press.
                    if Self::action_changes_structure(&action.node_type) && at < last {
                        Self::settle_for_what_comes_next(nodes);
                        pending_changes = false;
                    } else {
                        pending_changes = true;
                    }
                }
            }
        }

        // Final resolve so computed values reflect the end-of-event state, avoiding a
        // one-step lag for formulas that depend on several actions in a block. Skipped when
        // the last action was structural and already resolved with nothing written since -
        // resolving twice over an unchanged document produces the same answer twice.
        if pending_changes && !caller_will_settle_it() {
            resolver::resolve_document(nodes);
        }

        // Note: do not run timers here; scheduling handles timer firing.
        Ok(())
    }

    /// Settle what the actions so far changed, for the ones still to come to read.
    ///
    /// Followed through the graph when the caller holds the document its graph was recorded
    /// against and will settle the rest itself - see `app_api::settle_so_far`. Worked out whole
    /// otherwise, which is what always happened here.
    pub(crate) fn settle_for_what_comes_next(nodes: &mut Vec<OverseerNode>) {
        if caller_will_settle_it() && crate::app_api::settle_so_far(nodes) {
            return;
        }
        resolver::resolve_document(nodes);
    }


    fn execute_action(
        nodes: &mut Vec<OverseerNode>,
        owner_indices: &[usize],
        owner_path: &[String],
        action: &OverseerNode,
    ) -> Result<(), OverseerError> {
        debug_actions!(
            "[ACTIONS] Executing action {} with params {:?}",
            action.node_type,
            action.parameters
        );
        // Everything but these may change the document. An `if` says so through whatever it
        // lets run, and opening a field changes nothing until somebody types.
        if !matches!(action.node_type.as_str(), "if" | "start_editing" | "fold" | "unfold" | "toggle_fold") {
            note_acted();
        }
        match action.node_type.as_str() {
            // Opens a field for editing, as though it had been double-tapped.
            //
            // For the field that is not there to tap: a comment that takes no room until it has
            // something to say is hidden while it is empty, so a button beside it opens it. The
            // page does the opening; this finds the field by the same rules as every other
            // action's path, so `../comment` means here what it means to `set`, and names it in
            // the answer. Nothing is written, and a field that is not there is refused like any
            // other target.
            "start_editing" => {
                let target = Self::require_string(&action.parameters, "path")?;
                let (segments, _param, anchored) = Self::split_path_and_param(&target);
                let indices = Self::resolve_target_indices(nodes, owner_path, anchored, &segments)
                    .ok_or_else(|| OverseerError::ValidationError(format!("Target not found: {}", target)))?;
                let address = crate::delta::address_at(nodes, &indices).ok_or_else(|| {
                    OverseerError::ValidationError(format!("{} names nothing that can be shown", target))
                })?;
                note_start_editing(address);
                Ok(())
            }
            // Folds a div away, brings it back, or turns it over - see `foldable`.
            //
            // Found the way `start_editing` finds its field, and for the same reason left to the
            // page: what is folded is how somebody is looking at the document, not something the
            // document says, so nothing is written and nothing is worked out again. Only a div
            // that says it can fold is a target; anything else is refused, so a mistyped path
            // says so rather than doing nothing.
            "fold" | "unfold" | "toggle_fold" => {
                let target = Self::require_string(&action.parameters, "path")?;
                let (segments, _param, anchored) = Self::split_path_and_param(&target);
                let indices = Self::resolve_target_indices(nodes, owner_path, anchored, &segments)
                    .ok_or_else(|| OverseerError::ValidationError(format!("Target not found: {}", target)))?;
                let foldable = Self::node_by_indices(nodes, &indices).is_some_and(|node| {
                    node.node_type == "div" && node.parameters.get("foldable").is_some_and(Self::to_bool)
                });
                if !foldable {
                    return Err(OverseerError::ValidationError(format!("{} is not a div that can fold", target)));
                }
                let address = crate::delta::address_at(nodes, &indices).ok_or_else(|| {
                    OverseerError::ValidationError(format!("{} names nothing that can be shown", target))
                })?;
                let how = match action.node_type.as_str() {
                    "fold" => FoldHow::Fold,
                    "unfold" => FoldHow::Unfold,
                    _ => FoldHow::Toggle,
                };
                note_fold(address, how);
                Ok(())
            }
            "if" => {
                // if(cond=...) { <actions...> }
                // Evaluate cond in owner's context (defaults to false if missing)
                // Read where it is - see the key of `remove`.
                let cond_val = match action.parameters.get("cond") {
                    Some(v) => Self::evaluate_in_context(v, owner_path, nodes)?,
                    None => OverseerValue::Boolean(false),
                };
                if Self::to_bool(&cond_val) {
                    // Settled between the actions it lets through by the rule the press itself
                    // follows - see `execute_event`: in place when the shape moved and another
                    // action is still to read it, and otherwise left to whatever settles the
                    // press. It used to work the whole document out after every one of them, and
                    // the tag form's one append paid that before the press was settled anyway.
                    let last = action.children.len().saturating_sub(1);
                    for (at, child) in action.children.iter().enumerate() {
                        Self::execute_action(nodes, owner_indices, owner_path, child)?;
                        if Self::action_changes_structure(&child.node_type) && at < last {
                            Self::settle_for_what_comes_next(nodes);
                        }
                    }
                }
                Ok(())
            }
            "load_mount" => {
                // What a mount brings in is part of the document's shape.
                note_structural();
                Self::execute_load_mount(nodes, owner_indices, owner_path, action)?;
                Ok(())
            }
            "unload_mount" => {
                note_structural();
                Self::execute_unload_mount(nodes, owner_indices, owner_path, action)?;
                Ok(())
            }
            "set" => {
                let target = Self::require_string(&action.parameters, "path")?;
                // New: support cloning entire nodes into the target node when specifying a source
                // - fromList + keyField + keyValue [+ template?] -> find item by key and clone
                // - or fromPath -> clone node at path
                // Only applies when no explicit parameter was specified (i.e., path points to a node, not node.param)
                let (segments, explicit_param, anchored) = Self::split_path_and_param(&target);
                if explicit_param.is_none()
                    && (action.parameters.contains_key("fromList")
                        || action.parameters.contains_key("fromPath"))
                {
                    // Whole nodes copied in: a change of shape, not of a value.
                    note_structural();
                    // Use a snapshot for all reads to avoid &mut conflicts
                    let snapshot = nodes.clone();
                    // Resolve target indices using snapshot first
                    let target_indices = match Self::resolve_target_indices(
                        &snapshot, owner_path, anchored, &segments,
                    ) {
                        Some(ix) => ix,
                        None => {
                            #[cfg(feature = "debug-resolver")]
                            eprintln!("[ACTIONS] set(from*): Target not found: {} (owner_path={:?}, anchored={}, segments={:?})", target, owner_path, anchored, segments);
                            return Err(OverseerError::ValidationError(format!(
                                "Target not found: {}",
                                target
                            )));
                        }
                    };

                    // Determine source node to clone
                    let source_node_opt: Option<OverseerNode> = if let Some(from_list_val) =
                        action.parameters.get("fromList")
                    {
                        // fromList path + keyField + keyValue required
                        let from_list_path =
                            Self::evaluate_in_context(from_list_val, owner_path, &snapshot)?;
                        let list_path_str = match from_list_path {
                            OverseerValue::String(s) => s,
                            other => {
                                return Err(OverseerError::ValidationError(format!(
                                    "fromList must evaluate to string path, got {:?}",
                                    other
                                )))
                            }
                        };
                        let key_field = match action.parameters.get("keyField") {
                            Some(OverseerValue::String(s)) => s.clone(),
                            _ => {
                                return Err(OverseerError::ValidationError(
                                    "Missing keyField".to_string(),
                                ))
                            }
                        };
                        let key_val_raw = action
                            .parameters
                            .get("keyValue")
                            .ok_or_else(|| {
                                OverseerError::ValidationError("Missing keyValue".to_string())
                            })?
                            .clone();
                        let key_value =
                            Self::evaluate_in_context(&key_val_raw, owner_path, &snapshot)?;
                        let (list_segments, _exp, list_anchored) =
                            Self::split_path_and_param(&list_path_str);
                        let list_indices = match Self::resolve_target_indices(
                            &snapshot,
                            owner_path,
                            list_anchored,
                            &list_segments,
                        ) {
                            Some(ix) => ix,
                            None => {
                                return Err(OverseerError::ValidationError(format!(
                                    "List not found: {}",
                                    list_path_str
                                )))
                            }
                        };
                        // Read via a temporary mutable clone of snapshot to get an owned node
                        let list_node = {
                            let mut snap2 = snapshot.clone();
                            Self::get_node_mut_by_indices(&mut snap2, &list_indices)
                                .map(|n| n.clone())
                                .ok_or_else(|| {
                                    OverseerError::ValidationError(format!(
                                        "List not found: {}",
                                        list_path_str
                                    ))
                                })?
                        };
                        if list_node.node_type != "list" {
                            return Err(OverseerError::ValidationError(
                                "set(fromList): target is not a list".to_string(),
                            ));
                        }
                        // Find the first item matching keyField==keyValue
                        let mut found: Option<OverseerNode> = None;
                        'search: for it in &list_node.children {
                            if let Some(v) = Self::get_field_value(it, &key_field) {
                                if Self::value_equals_with_key_precision(&list_node, v, &key_value)
                                {
                                    found = Some(it.clone());
                                    break 'search;
                                }
                            }
                        }
                        found
                    } else if let Some(from_path_val) = action.parameters.get("fromPath") {
                        let from_path_eval =
                            Self::evaluate_in_context(from_path_val, owner_path, &snapshot)?;
                        let from_path = match from_path_eval {
                            OverseerValue::String(s) => s,
                            other => {
                                return Err(OverseerError::ValidationError(format!(
                                    "fromPath must evaluate to string path, got {:?}",
                                    other
                                )))
                            }
                        };
                        let (src_segments, _exp, src_anchored) =
                            Self::split_path_and_param(&from_path);
                        let src_indices = match Self::resolve_target_indices(
                            &snapshot,
                            owner_path,
                            src_anchored,
                            &src_segments,
                        ) {
                            Some(ix) => ix,
                            None => {
                                return Err(OverseerError::ValidationError(format!(
                                    "Source not found: {}",
                                    from_path
                                )))
                            }
                        };
                        let src_node = {
                            let mut snap2 = snapshot.clone();
                            Self::get_node_mut_by_indices(&mut snap2, &src_indices)
                                .map(|n| n.clone())
                                .ok_or_else(|| {
                                    OverseerError::ValidationError(format!(
                                        "Source not found: {}",
                                        from_path
                                    ))
                                })?
                        };
                        Some(src_node)
                    } else {
                        None
                    };

                    if let Some(mut source_node) = source_node_opt {
                        // Clear any computed shadows so values recompute in the new context
                        Self::clear_computed_recursive(&mut source_node);
                        // Finally borrow target node mutably in the real tree and apply
                        let target_node = Self::get_node_mut_by_indices(nodes, &target_indices)
                            .ok_or_else(|| {
                                OverseerError::ValidationError(format!(
                                    "Target not found: {}",
                                    target
                                ))
                            })?;
                        // Clone parameters and children into the target, preserving target name/type
                        target_node.parameters = source_node.parameters.clone();
                        target_node.children = source_node.children.clone();
                        // Mark explicit override for serializer when this target is a template instance child
                        if !target_indices.is_empty() {
                            let mut anc = target_indices.clone();
                            let child_name = target_node.name.clone();
                            anc.pop();
                            while !anc.is_empty() {
                                if let Some(parent) = Self::get_node_mut_by_indices(nodes, &anc) {
                                    let is_instance =
                                        matches!(
                                            parent.parameters.get("_from_template"),
                                            Some(OverseerValue::Boolean(true))
                                        ) || parent.parameters.contains_key("_template_origin");
                                    if is_instance {
                                        let entry = parent
                                            .parameters
                                            .entry("_explicit_overrides".to_string())
                                            .or_insert(OverseerValue::String(String::new()));
                                        if let OverseerValue::String(s) = entry {
                                            if !s.split(',').any(|n| n == child_name) {
                                                if !s.is_empty() {
                                                    s.push(',');
                                                }
                                                s.push_str(&child_name);
                                            }
                                        }
                                        break;
                                    }
                                }
                                anc.pop();
                            }
                        }
                        return Ok(());
                    } else {
                        return Err(OverseerError::ValidationError(
                            "Source item not found for set(from*)".to_string(),
                        ));
                    }
                }
                // Optional mode: "value" (default) evaluates and writes the result; "formula" copies raw formula
                let mode = match action.parameters.get("mode") {
                    Some(OverseerValue::String(s)) => s.to_lowercase(),
                    _ => "value".to_string(),
                };
                let value = if mode == "formula" {
                    // Copy raw value exactly as provided
                    Self::require_value(&action.parameters, "value")?
                } else {
                    // Always evaluate a Formula now in the owner's context to avoid stale _computed_value
                    if let Some(OverseerValue::Formula(expr)) = action.parameters.get("value") {
                        // Read where it is: nothing is written while it is worked out, and a copy
                        // of the whole document to read one value from was over twenty
                        // milliseconds - `done` on a task paid it for the key of its `remove`.
                        let ctx = EvaluationContext::new(owner_path.to_vec(), nodes);
                        FormulaEvaluator::evaluate_formula(expr, &ctx)?
                    } else if let Some(v) = action.parameters.get("value") {
                        v.clone()
                    } else if let Some(v) = Self::get_effective(&action.parameters, "value") {
                        // Fallback to any precomputed value if present
                        v.clone()
                    } else {
                        return Err(OverseerError::ValidationError(
                            "Missing parameter 'value'".to_string(),
                        ));
                    }
                };
                Self::set_value(nodes, owner_indices, owner_path, &target, value)
            }
            "inc" => {
                let target = Self::require_string(&action.parameters, "path")?;
                let by = match action.parameters.get("by") {
                    Some(OverseerValue::Integer(i)) => *i as f64,
                    Some(OverseerValue::Float(f)) => *f,
                    None => 1.0,
                    _ => {
                        return Err(OverseerError::ValidationError(
                            "inc.by must be number".to_string(),
                        ))
                    }
                };
                Self::inc_value(nodes, owner_indices, owner_path, &target, by)
            }
            "dec" => {
                let target = Self::require_string(&action.parameters, "path")?;
                let by = match action.parameters.get("by") {
                    Some(OverseerValue::Integer(i)) => *i as f64,
                    Some(OverseerValue::Float(f)) => *f,
                    None => 1.0,
                    _ => {
                        return Err(OverseerError::ValidationError(
                            "dec.by must be number".to_string(),
                        ))
                    }
                };
                Self::inc_value(nodes, owner_indices, owner_path, &target, -by)
            }
            "toggle" => {
                let target = Self::require_string(&action.parameters, "path")?;
                Self::toggle_value(nodes, owner_indices, owner_path, &target)
            }
            "clear" => {
                let target = Self::require_string(&action.parameters, "path")?;
                Self::clear_value(nodes, owner_indices, owner_path, &target)
            }
            "clear_list" => {
                // clear_list(path=...) empties the children of a list field
                let target = Self::require_string(&action.parameters, "path")?;
                let (segments, _explicit_param, anchored) = Self::split_path_and_param(&target);
                let indices = match Self::resolve_target_indices(
                    &nodes, owner_path, anchored, &segments,
                ) {
                    Some(ix) => ix,
                    None => {
                        #[cfg(feature = "debug-resolver")]
                        eprintln!("[ACTIONS] clear_list: Target not found: {} (owner_path={:?}, anchored={}, segments={:?})", target, owner_path, anchored, segments);
                        return Err(OverseerError::ValidationError(format!(
                            "Target not found: {}",
                            target
                        )));
                    }
                };
                let node = Self::get_node_mut_by_indices(nodes, &indices).ok_or_else(|| {
                    OverseerError::ValidationError(format!("Target not found: {}", target))
                })?;
                if node.node_type != "list" {
                    return Err(OverseerError::ValidationError(
                        "clear_list target must be a list".to_string(),
                    ));
                }
                node.children.clear();
                // Shape, not value - see `note_structural`. Unsaid, what reached the resolver looked like a
                // change to values: the graph was asked about a document whose entries had moved, and the
                // survivors kept names their text no longer gives them.
                note_structural();
                // Mark explicit so serializer persists empty explicit list on template instance
                Self::mark_field_explicit_override(nodes, &indices);
                Ok(())
            }
            "set_now" => {
                let target = Self::require_string(&action.parameters, "path")?;
                let clock = match action.parameters.get("clock") {
                    Some(OverseerValue::String(s)) => s.as_str(),
                    _ => "local",
                };
                let now = if clock == "utc" {
                    if let Some(ovr) =
                        crate::formula_evaluator::FormulaEvaluator::get_time_override()
                    {
                        ovr.date_naive()
                    } else {
                        Utc::now().date_naive()
                    }
                } else {
                    if let Some(ovr) =
                        crate::formula_evaluator::FormulaEvaluator::get_time_override()
                    {
                        let local_dt: chrono::DateTime<Local> =
                            chrono::DateTime::<Local>::from(ovr);
                        local_dt.date_naive()
                    } else {
                        Local::now().date_naive()
                    }
                };
                let val = OverseerValue::Date(now.to_string());
                Self::set_value(nodes, owner_indices, owner_path, &target, val)
            }
            "set_now_ts" => {
                // set_now_ts(path=..., offset=seconds?) -> sets RFC3339 Timestamp
                // path can be either a node field (sets its value) or a specific parameter via trailing .param (e.g., ../after_10s.at)
                let target = Self::require_string(&action.parameters, "path")?;
                let offset_secs: i64 = match action
                    .parameters
                    .get("offset")
                    .or(action.parameters.get("offsetSeconds"))
                {
                    Some(OverseerValue::Integer(i)) => *i,
                    Some(OverseerValue::Float(f)) => *f as i64,
                    Some(OverseerValue::String(s)) => s.parse::<i64>().unwrap_or(0),
                    _ => 0,
                };
                let base = if let Some(ovr) =
                    crate::formula_evaluator::FormulaEvaluator::get_time_override()
                {
                    ovr
                } else {
                    Utc::now()
                };
                let ts = (base + Duration::seconds(offset_secs)).to_rfc3339();
                let val = OverseerValue::Timestamp(ts);
                // If target specifies a parameter explicitly (../node.param), set that parameter; else set 'value'
                let (_segments, explicit_param, _anchored) = Self::split_path_and_param(&target);
                if explicit_param.is_some() {
                    Self::set_value(nodes, owner_indices, owner_path, &target, val)
                } else {
                    // force write to value by ensuring no explicit param
                    Self::set_value(nodes, owner_indices, owner_path, &target, val)
                }
            }
            // Increment 3: list identity and mutations (MVP subset)
            "ensure_in_list" => {
                let list_path = Self::require_string(&action.parameters, "list")?;
                let key_field = match action.parameters.get("keyField") {
                    Some(OverseerValue::String(s)) => s.clone(),
                    _ => "".to_string(),
                };
                let key_value = match action.parameters.get("keyValue") {
                    Some(OverseerValue::Formula(expr)) => {
                        // Read where it is: nothing is written while it is worked out, and a copy
                        // of the whole document to read one value from was over twenty
                        // milliseconds - `done` on a task paid it for the key of its `remove`.
                        let ctx = EvaluationContext::new(owner_path.to_vec(), nodes);
                        FormulaEvaluator::evaluate_formula(expr, &ctx)?
                    }
                    Some(OverseerValue::String(s)) => OverseerValue::String(s.clone()),
                    Some(OverseerValue::Integer(i)) => OverseerValue::Integer(*i),
                    Some(OverseerValue::Float(f)) => OverseerValue::Float(*f),
                    Some(OverseerValue::Boolean(b)) => OverseerValue::Boolean(*b),
                    Some(OverseerValue::Date(d)) => OverseerValue::Date(d.clone()),
                    Some(OverseerValue::Timestamp(ts)) => OverseerValue::Timestamp(ts.clone()),
                    _ => {
                        return Err(OverseerError::ValidationError(
                            "ensure_in_list.keyValue required".to_string(),
                        ))
                    }
                };
                // Template to clone
                let template_name = match action.parameters.get("template") {
                    Some(OverseerValue::Template(t)) => {
                        let raw = t.split('/').last().unwrap_or("");
                        let trimmed = raw.trim();
                        if trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.len() >= 2
                        {
                            trimmed[1..trimmed.len() - 1].to_string()
                        } else {
                            trimmed.to_string()
                        }
                    }
                    Some(OverseerValue::String(s)) => {
                        let raw = s.split('/').last().unwrap_or("");
                        let trimmed = raw.trim();
                        if trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.len() >= 2
                        {
                            trimmed[1..trimmed.len() - 1].to_string()
                        } else {
                            trimmed.to_string()
                        }
                    }
                    _ => {
                        return Err(OverseerError::ValidationError(
                            "ensure_in_list.template required".to_string(),
                        ))
                    }
                };
                let goes = WhereItGoes::from_said(match action.parameters.get("position") {
                    Some(OverseerValue::String(s)) => Some(s.as_str()),
                    _ => None,
                });
                Self::ensure_in_list(
                    nodes,
                    owner_path,
                    &list_path,
                    &template_name,
                    &key_field,
                    key_value,
                    goes,
                )
                .map(|_| ())
            }
            "remove_from_list" | "remove" => {
                let list_path = match action
                    .parameters
                    .get("list")
                    .or(action.parameters.get("from"))
                {
                    Some(OverseerValue::String(s)) => s.clone(),
                    _ => {
                        return Err(OverseerError::ValidationError(
                            "remove.list (or from) required".to_string(),
                        ))
                    }
                };
                let key_field = match action.parameters.get("keyField") {
                    Some(OverseerValue::String(s)) => s.clone(),
                    _ => "".to_string(),
                };
                let key_value = match action.parameters.get("keyValue") {
                    Some(OverseerValue::Formula(expr)) => {
                        // Read where it is: nothing is written while it is worked out, and a copy
                        // of the whole document to read one value from was over twenty
                        // milliseconds - `done` on a task paid it for the key of its `remove`.
                        let ctx = EvaluationContext::new(owner_path.to_vec(), nodes);
                        FormulaEvaluator::evaluate_formula(expr, &ctx)?
                    }
                    Some(OverseerValue::String(s)) => OverseerValue::String(s.clone()),
                    Some(OverseerValue::Integer(i)) => OverseerValue::Integer(*i),
                    Some(OverseerValue::Float(f)) => OverseerValue::Float(*f),
                    Some(OverseerValue::Boolean(b)) => OverseerValue::Boolean(*b),
                    Some(OverseerValue::Date(d)) => OverseerValue::Date(d.clone()),
                    Some(OverseerValue::Timestamp(ts)) => OverseerValue::Timestamp(ts.clone()),
                    _ => {
                        return Err(OverseerError::ValidationError(
                            "remove.keyValue required".to_string(),
                        ))
                    }
                };
                Self::remove_from_list(nodes, owner_path, &list_path, &key_field, &key_value)
            }
            "set_in_list" => {
                // set_in_list(list=/path, keyField=..., keyValue=..., field=..., value=...)
                let list_path = Self::require_string(&action.parameters, "list")?;
                let key_field =
                    Self::require_string(&action.parameters, "keyField").unwrap_or_default();
                let field_name = Self::require_string(&action.parameters, "field")?;
                // Evaluate keyValue and value in the owner's context (if provided as formulas) -
                // read where they are, before anything is written; see the key of `remove`.
                let key_value = match action.parameters.get("keyValue") {
                    Some(v) => Self::evaluate_in_context(v, owner_path, nodes)?,
                    None => {
                        return Err(OverseerError::ValidationError(
                            "set_in_list.keyValue required".to_string(),
                        ))
                    }
                };
                let new_value = match action.parameters.get("value") {
                    Some(v) => Self::evaluate_in_context(v, owner_path, nodes)?,
                    None => {
                        return Err(OverseerError::ValidationError(
                            "set_in_list.value required".to_string(),
                        ))
                    }
                };
                // Resolve the list by path, then mutate it
                let (segments, _explicit_param, anchored) = Self::split_path_and_param(&list_path);
                let indices = match Self::resolve_target_indices(
                    nodes, owner_path, anchored, &segments,
                ) {
                    Some(ix) => ix,
                    None => {
                        return Err(OverseerError::ValidationError(format!(
                            "List not found: {}",
                            list_path
                        )))
                    }
                };
                let list_node =
                    Self::get_node_mut_by_indices(nodes, &indices).ok_or_else(|| {
                        OverseerError::ValidationError(format!("List not found: {}", list_path))
                    })?;
                if list_node.node_type != "list" {
                    return Err(OverseerError::ValidationError(
                        "set_in_list target must be a list".to_string(),
                    ));
                }
                // Determine effective key field (prefer explicit, else list.key)
                let effective_key_field = if !key_field.is_empty() {
                    key_field
                } else if let Some(OverseerValue::String(s)) = list_node.parameters.get("key") {
                    s.clone()
                } else {
                    return Err(OverseerError::ValidationError(
                        "set_in_list.keyField missing and list has no key".to_string(),
                    ));
                };
                // Find matching item and set field value
                let holding = Self::the_one_holding(list_node, &effective_key_field, &key_value, &list_path, "change", |v| {
                    Self::value_equals(v, &key_value)
                })?;
                if let Some(item) = holding.and_then(|at| list_node.children.get_mut(at)) {
                    Self::set_field_value_on_item(item, &field_name, new_value);
                }
                Ok(())
            }
            "append" => {
                // append(list=/path, template=<...>?){ overrides... } for template lists
                // or append(list=/path, value=...) for simple-type lists
                let list_path = Self::require_string(&action.parameters, "list")
                    .or_else(|_| Self::require_string(&action.parameters, "to"))?;
                // Optional template override
                let template_name_opt: Option<String> = match action.parameters.get("template") {
                    Some(OverseerValue::Template(t)) => {
                        let raw = t.split('/').last().unwrap_or("");
                        let trimmed = raw.trim();
                        if trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.len() >= 2
                        {
                            Some(trimmed[1..trimmed.len() - 1].to_string())
                        } else {
                            Some(trimmed.to_string())
                        }
                    }
                    Some(OverseerValue::String(s)) => {
                        let raw = s.split('/').last().unwrap_or("");
                        let trimmed = raw.trim();
                        if trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.len() >= 2
                        {
                            Some(trimmed[1..trimmed.len() - 1].to_string())
                        } else {
                            Some(trimmed.to_string())
                        }
                    }
                    _ => None,
                };
                let value_opt = action.parameters.get("value").cloned();
                // Pass action block overrides to append semantics
                let overrides = action.children.clone();
                Self::append_to_list_from(
                    nodes,
                    owner_path,
                    &list_path,
                    template_name_opt.as_deref(),
                    value_opt,
                    &overrides,
                    action.parameters.get("from"),
                )
            }
            "prepend" => {
                // prepend(list=/path, template=<...>?){ overrides... } or value=... for simple lists
                let list_path = Self::require_string(&action.parameters, "list")
                    .or_else(|_| Self::require_string(&action.parameters, "to"))?;
                let template_name_opt: Option<String> = match action.parameters.get("template") {
                    Some(OverseerValue::Template(t)) => {
                        let raw = t.split('/').last().unwrap_or("");
                        let trimmed = raw.trim();
                        if trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.len() >= 2
                        {
                            Some(trimmed[1..trimmed.len() - 1].to_string())
                        } else {
                            Some(trimmed.to_string())
                        }
                    }
                    Some(OverseerValue::String(s)) => {
                        let raw = s.split('/').last().unwrap_or("");
                        let trimmed = raw.trim();
                        if trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.len() >= 2
                        {
                            Some(trimmed[1..trimmed.len() - 1].to_string())
                        } else {
                            Some(trimmed.to_string())
                        }
                    }
                    _ => None,
                };
                let value_opt = action.parameters.get("value").cloned();
                let overrides = action.children.clone();
                Self::prepend_to_list(
                    nodes,
                    owner_path,
                    &list_path,
                    template_name_opt.as_deref(),
                    value_opt,
                    &overrides,
                    action.parameters.get("from"),
                )
            }
            "move" => {
                // move(from=/list, keyField=..., keyValue=..., to=/targetList?, at=index?)
                let from_path = Self::require_string(&action.parameters, "from")?;
                let to_path = match action.parameters.get("to") {
                    Some(OverseerValue::String(s)) => s.clone(),
                    _ => from_path.clone(),
                };
                let key_field = match action.parameters.get("keyField") {
                    Some(OverseerValue::String(s)) => s.clone(),
                    _ => "".to_string(),
                };
                let key_value = match action.parameters.get("keyValue") {
                    Some(OverseerValue::Formula(expr)) => {
                        // Read where it is: nothing is written while it is worked out, and a copy
                        // of the whole document to read one value from was over twenty
                        // milliseconds - `done` on a task paid it for the key of its `remove`.
                        let ctx = EvaluationContext::new(owner_path.to_vec(), nodes);
                        FormulaEvaluator::evaluate_formula(expr, &ctx)?
                    }
                    Some(OverseerValue::String(s)) => OverseerValue::String(s.clone()),
                    Some(OverseerValue::Integer(i)) => OverseerValue::Integer(*i),
                    Some(OverseerValue::Float(f)) => OverseerValue::Float(*f),
                    Some(OverseerValue::Boolean(b)) => OverseerValue::Boolean(*b),
                    Some(OverseerValue::Date(d)) => OverseerValue::Date(d.clone()),
                    Some(OverseerValue::Timestamp(ts)) => OverseerValue::Timestamp(ts.clone()),
                    _ => {
                        return Err(OverseerError::ValidationError(
                            "move.keyValue required".to_string(),
                        ))
                    }
                };
                let at_index = match action.parameters.get("at") {
                    Some(OverseerValue::Integer(i)) => Some(*i as usize),
                    _ => None,
                };
                Self::move_in_list(
                    nodes, owner_path, &from_path, &to_path, &key_field, &key_value, at_index,
                )
            }
            "sort" => {
                // sort(list=/path, by=$(...), order=asc|desc, stable=true)
                let list_path = Self::require_string(&action.parameters, "list")?;
                let mut by_expr = match action.parameters.get("by") {
                    Some(OverseerValue::Formula(s)) => s.clone(),
                    Some(OverseerValue::String(s)) => s.clone(),
                    _ => {
                        return Err(OverseerError::ValidationError(
                            "sort.by must be a formula string".to_string(),
                        ))
                    }
                };
                // Allow passing with $(...) wrapper; strip it if present
                let trimmed = by_expr.trim();
                if trimmed.starts_with("$(") && trimmed.ends_with(')') {
                    let inner = &trimmed[2..trimmed.len() - 1];
                    by_expr = inner.trim().to_string();
                }
                let order = match action.parameters.get("order") {
                    Some(OverseerValue::String(s)) => s.to_lowercase(),
                    _ => "asc".to_string(),
                };
                let stable = match action.parameters.get("stable") {
                    Some(OverseerValue::Boolean(b)) => *b,
                    _ => true,
                };
                Self::sort_list(nodes, owner_path, &list_path, &by_expr, &order, stable)
            }
            _ => {
                // Unknown action: no-op for now
                debug_actions!("[ACTIONS] Unknown action '{}', skipping", action.node_type);
                Ok(())
            }
        }
    }

    fn require_string(
        map: &crate::types::Params,
        key: &str,
    ) -> Result<String, OverseerError> {
        match map.get(key) {
            Some(OverseerValue::String(s)) => Ok(s.clone()),
            Some(_) => Err(OverseerError::ValidationError(format!(
                "Parameter '{}' must be string",
                key
            ))),
            None => Err(OverseerError::ValidationError(format!(
                "Missing parameter '{}'",
                key
            ))),
        }
    }

    fn require_value(
        map: &crate::types::Params,
        key: &str,
    ) -> Result<OverseerValue, OverseerError> {
        match map.get(key) {
            Some(v) => Ok(v.clone()),
            None => Err(OverseerError::ValidationError(format!(
                "Missing parameter '{}'",
                key
            ))),
        }
    }

    fn get_effective<'a>(
        params: &'a crate::types::Params,
        key: &str,
    ) -> Option<&'a OverseerValue> {
        if key == "value" {
            if let Some(v) = params.get("_computed_value") {
                return Some(v);
            }
        } else {
            let shadow = format!("_computed_{}", key);
            if let Some(v) = params.get(&shadow) {
                return Some(v);
            }
        }
        params.get(key)
    }

    fn split_path_and_param(path: &str) -> (Vec<String>, Option<String>, bool) {
        // returns (segments, explicit_param, anchored)
        let anchored = path.starts_with('/') || path.starts_with("/ ");
        let mut p = path.trim();
        if p.starts_with('/') {
            p = &p[1..];
        }
        let mut explicit_param: Option<String> = None;
        // support trailing .param on the last segment only, not for ".."
        let last_slash = p.rfind('/');
        let last_dot = p.rfind('.');
        if let Some(d) = last_dot {
            let is_after_slash = last_slash.map_or(true, |s| d > s);
            let is_not_parent = if d > 0 { &p[d - 1..=d] != ".." } else { true };
            let has_rhs = d + 1 < p.len();
            if is_after_slash && is_not_parent && has_rhs {
                let (lhs, rhs) = p.split_at(d);
                explicit_param = Some(rhs[1..].to_string());
                p = lhs;
            }
        }
        let segments = p.split('/').map(|s| s.trim().to_string()).collect();
        (segments, explicit_param, anchored)
    }

    /// Resolve a mutable reference to a node given a base owner path and target path string.
    fn resolve_target_indices(
        nodes: &Vec<OverseerNode>,
        owner_path: &[String],
        anchored: bool,
        segments: &[String],
    ) -> Option<Vec<usize>> {
        // For anchored paths (starting with '/'): treat as absolute from root
        // For relative paths: use the owner_path as the base
        let bases: Vec<Vec<String>> = if anchored {
            vec![Vec::new()]
        } else {
            vec![owner_path.to_vec()]
        };

        for base in bases {
            // Build absolute name path by applying segments (support "..")
            let mut abs = base.clone();
            let mut valid = true;
            // Minimum depth allowed before processing ".."
            let min_len = if anchored { 0 } else { 1 };
            for seg in segments {
                // The node the action belongs to, as a formula's `./` is.
                if seg == "." {
                    continue;
                }
                if seg == ".." {
                    // Up to the enclosing node the document actually names. A `div` used to
                    // group fields for layout is not something an author thinks of as
                    // containing anything, and stopping on one makes `../items` resolve a
                    // level too shallow - so a button written beside a list stops finding it
                    // the moment the fields around it are grouped.
                    loop {
                        if abs.len() > min_len {
                            abs.pop();
                        } else {
                            valid = false;
                            break;
                        }
                        let landed_on_wrapper = Self::find_indices_by_name_path(nodes, &abs)
                            .and_then(|indices| Self::node_by_indices(nodes, &indices))
                            .map(crate::addressing::is_wrapper)
                            .unwrap_or(false);
                        if !landed_on_wrapper {
                            break;
                        }
                    }
                    if !valid {
                        break;
                    }
                } else if seg.is_empty() {
                    continue;
                } else {
                    abs.push(seg.clone());
                }
            }
            if !valid {
                continue;
            }
            #[cfg(feature = "debug-resolver")]
            eprintln!(
                "[ACTIONS] resolve_target_indices: base={:?} segs={:?} => abs={:?}",
                base, segments, abs
            );
            if let Some(indices) = Self::find_indices_by_name_path(nodes, &abs) {
                return Some(indices);
            }
        }
        None
    }

    fn node_by_indices<'a>(nodes: &'a [OverseerNode], indices: &[usize]) -> Option<&'a OverseerNode> {
        let (first, rest) = indices.split_first()?;
        let mut node = nodes.get(*first)?;
        for index in rest {
            node = node.children.get(*index)?;
        }
        Some(node)
    }

    fn find_indices_by_name_path(nodes: &Vec<OverseerNode>, path: &[String]) -> Option<Vec<usize>> {
        if path.is_empty() {
            return None;
        }
        // Parse a segment of the form "name#k" into (name, ordinal)
        fn parse_seg(seg: &str) -> (&str, Option<usize>) {
            if let Some(hash_pos) = seg.rfind('#') {
                let (base, ord_str) = seg.split_at(hash_pos);
                if let Ok(k) = ord_str[1..].parse::<usize>() {
                    return (base, Some(k));
                }
            }
            (seg, None)
        }
        // Effective display name used by the frontend: prefer name, then node_type
        fn eff_name(n: &OverseerNode) -> &str {
            if !n.name.is_empty() {
                &n.name
            } else {
                &n.node_type
            }
        }
        fn matches_base(effective: &str, base: &str) -> bool {
            effective == base || effective.starts_with(&format!("{}__", base))
        }
        // Helper: DFS to find a descendant by name through transparent nodes, returning index chain from 'cur'
        fn find_child_chain(cur: &OverseerNode, target: &str) -> Option<Vec<usize>> {
            for (i, ch) in cur.children.iter().enumerate() {
                if matches_base(eff_name(ch), target) {
                    return Some(vec![i]);
                }
                if ch.is_hierarchy_transparent {
                    if let Some(mut sub) = find_child_chain(ch, target) {
                        let mut out = vec![i];
                        out.append(&mut sub);
                        return Some(out);
                    }
                }
            }
            None
        }
        // Helper: select the k-th direct child whose effective name matches target
        fn find_kth_direct_child(cur: &OverseerNode, target: &str, k: usize) -> Option<usize> {
            let mut count = 0usize;
            for (i, ch) in cur.children.iter().enumerate() {
                if matches_base(eff_name(ch), target) {
                    if count == k {
                        return Some(i);
                    }
                    count += 1;
                }
            }
            None
        }
        // Helper: search from roots for first segment, allowing ordinal and transparent wrappers
        fn find_root_chain(nodes: &Vec<OverseerNode>, first_seg: &str) -> Option<Vec<usize>> {
            let (base, ord) = parse_seg(first_seg);
            if let Some(k) = ord {
                // k-th direct root with effective name == base
                let mut count = 0usize;
                for (i, n) in nodes.iter().enumerate() {
                    if matches_base(eff_name(n), base) {
                        if count == k {
                            return Some(vec![i]);
                        }
                        count += 1;
                    }
                }
                None
            } else {
                // No ordinal: try direct root first, else search through transparent wrappers
                for (i, n) in nodes.iter().enumerate() {
                    if matches_base(eff_name(n), base) {
                        return Some(vec![i]);
                    }
                    if let Some(mut sub) = find_child_chain(n, base) {
                        let mut out = vec![i];
                        out.append(&mut sub);
                        return Some(out);
                    }
                }
                None
            }
        }

        let mut indices: Vec<usize> = Vec::new();
        // Find first segment anywhere in the (transparent-flattened) roots
        let mut chain = find_root_chain(nodes, &path[0])?;
        indices.append(&mut chain);
        // Walk remaining segments, allowing transparent traversal and ordinal selection at each step
        let mut cur: &OverseerNode = {
            let mut node_ref: &OverseerNode = &nodes[indices[0]];
            for idx in indices.iter().skip(1) {
                node_ref = &node_ref.children[*idx];
            }
            node_ref
        };
        for seg in &path[1..] {
            let (base, ord) = parse_seg(seg);
            if eff_name(cur) == base && ord.is_none() {
                // Segment refers to current node by name; continue
                continue;
            }
            let next_idx_opt = if let Some(k) = ord {
                find_kth_direct_child(cur, base, k).map(|i| vec![i])
            } else {
                find_child_chain(cur, base)
            };
            if let Some(mut sub) = next_idx_opt {
                indices.append(&mut sub);
                // advance cur to new node
                let mut node_ref: &OverseerNode = &nodes[indices[0]];
                for idx in indices.iter().skip(1) {
                    node_ref = &node_ref.children[*idx];
                }
                cur = node_ref;
            } else {
                return None;
            }
        }
        Some(indices)
    }

    fn get_node_mut_by_indices<'a>(
        nodes: &'a mut Vec<OverseerNode>,
        indices: &[usize],
    ) -> Option<&'a mut OverseerNode> {
        if indices.is_empty() {
            return None;
        }
        let mut cur_ptr: *mut OverseerNode = &mut nodes[indices[0]] as *mut _;
        for (i, idx) in indices.iter().enumerate() {
            if i == 0 {
                continue;
            }
            unsafe {
                let cur_ref = &mut *cur_ptr;
                if *idx >= cur_ref.children.len() {
                    return None;
                }
                cur_ptr = &mut cur_ref.children[*idx] as *mut _;
            }
        }
        unsafe { Some(&mut *cur_ptr) }
    }

    fn get_node_ref_by_indices<'a>(
        nodes: &'a Vec<OverseerNode>,
        indices: &[usize],
    ) -> Option<&'a OverseerNode> {
        if indices.is_empty() {
            return None;
        }
        let mut cur: &OverseerNode = &nodes[indices[0]];
        for (i, idx) in indices.iter().enumerate() {
            if i == 0 {
                continue;
            }
            if *idx >= cur.children.len() {
                return None;
            }
            cur = &cur.children[*idx];
        }
        Some(cur)
    }

    /// Return raw pointer to node and its indices path for reuse.
    fn get_node_mut_by_path(
        nodes: &mut Vec<OverseerNode>,
        path: &[String],
    ) -> Option<(*mut OverseerNode, Vec<usize>)> {
        if path.is_empty() {
            return None;
        }
        let mut cur: *mut OverseerNode;
        // Reuse transparent-aware name path resolution to compute indices, then fetch pointer
        let indices = match Self::find_indices_by_name_path(nodes, path) {
            Some(ix) => ix,
            None => {
                // Fallback 1: strip any ordinal suffix (#k) from segments and retry
                let stripped: Vec<String> = path
                    .iter()
                    .map(|s| s.split('#').next().unwrap_or("").to_string())
                    .collect();
                if let Some(ix2) = Self::find_indices_by_name_path(nodes, &stripped) {
                    ix2
                } else {
                    // Fallback 2: strip instance suffixes ("__n") from segments and retry
                    let base_only: Vec<String> = stripped
                        .iter()
                        .map(|s| {
                            if let Some(pos) = s.rfind("__") {
                                s[..pos].to_string()
                            } else {
                                s.clone()
                            }
                        })
                        .collect();
                    Self::find_indices_by_name_path(nodes, &base_only)?
                }
            }
        };
        // Walk indices to yield a mutable pointer
        cur = &mut nodes[indices[0]] as *mut _;
        for idx in indices.iter().skip(1) {
            unsafe {
                let cur_ref = &mut *cur;
                if *idx >= cur_ref.children.len() {
                    return None;
                }
                cur = &mut cur_ref.children[*idx] as *mut _;
            }
        }
        Some((cur, indices))
    }

    fn set_value(
        nodes: &mut Vec<OverseerNode>,
        _owner_indices: &[usize],
        owner_path: &[String],
        target: &str,
        value: OverseerValue,
    ) -> Result<(), OverseerError> {
        let (segments, explicit_param, anchored) = Self::split_path_and_param(target);
        let indices = match Self::resolve_target_indices(&nodes, owner_path, anchored, &segments) {
            Some(ix) => ix,
            None => {
                #[cfg(feature = "debug-resolver")]
                eprintln!("[ACTIONS] set: Target not found: {} (owner_path={:?}, anchored={}, segments={:?})", target, owner_path, anchored, segments);
                return Err(OverseerError::ValidationError(format!(
                    "Target not found: {}",
                    target
                )));
            }
        };
        // Named before the borrow, in the form the dependency graph uses, so whatever reads
        // this value can be found without working the document out again.
        let address = Self::build_disambiguated_path(nodes, &indices).join("/");
        // A field the document says is the viewer's is not written down. The value is carried
        // out in the report instead, and the caller decides where a viewer's state lives - which
        // is nowhere near the file. Doing this here rather than at the caller is what keeps the
        // document untouched: there is nothing to undo afterwards, and nothing to serialize.
        let viewers = crate::mutability::along(nodes, &indices).is_guarded();
        let node = Self::get_node_mut_by_indices(nodes, &indices).ok_or_else(|| {
            OverseerError::ValidationError(format!("Target not found: {}", target))
        })?;
        let key = explicit_param.unwrap_or_else(|| "value".to_string());
        // Equality-aware override: don't mark as override if value is unchanged
        let same = node.parameters.get(&key).map_or(false, |v| v == &value);
        if !same {
            let named = if key == "value" { address } else { format!("{}/{}", address, key) };
            if viewers {
                // Written, because whoever asked for this is looking at the result and the day
                // has to move. Named as the viewer's, because it must not reach the file: the
                // caller takes it back out before writing, and keeps it against the session.
                note_view_state(named, value.clone());
            } else {
                // Only when it moved. A set that writes what was already there reaches nothing.
                note_field(named);
            }
        }
        node.parameters.insert(key.clone(), value.clone());
        if !same {
            // The serializer replays a node from its source snapshot while the
            // fingerprint it was parsed with still matches, and that fingerprint says
            // nothing about what has since been written here. Leaving it means the
            // node is written back out as the text it was read from, discarding what
            // this action just did: a day-navigation button that writes
            // date_add_days(selected_date, -1) would be replayed as its authored
            // $(today()), so every click would move one day from today and no further.
            node.source_fingerprint = None;
        }
        // If overriding a parameter that had a template marker, remove the marker so it persists
        let marker = format!("_template_{}", key);
        if !same {
            node.parameters.remove(&marker);
        }
        // If overriding the 'value' of a template-derived child, mark explicit override for serializer
        if key == "value" {
            if !same {
                node.parameters.insert(
                    "_override_present".to_string(),
                    OverseerValue::Boolean(true),
                );
                node.parameters.insert(
                    "_explicit_child_override".to_string(),
                    OverseerValue::Boolean(true),
                );
                node.parameters.remove("_template_value");
            }
            // Clear any stale computed value
            node.parameters.remove("_computed_value");
            // Also record this child name on the nearest template instance ancestor's _explicit_overrides list,
            // so serializers that consult this list will include it even when suppressing template children.
            if !indices.is_empty() {
                let child_name = node.name.clone();
                // Drop child index to get parent, then walk up until a template-instance boundary if needed
                let mut anc = indices.clone();
                anc.pop();
                while !anc.is_empty() {
                    // Borrow-drop 'node' before taking another mutable borrow (scope ends here)
                    // SAFETY: we immediately re-borrow below and never keep two &mut at the same time
                    if let Some(parent) = Self::get_node_mut_by_indices(nodes, &anc) {
                        let is_instance = matches!(
                            parent.parameters.get("_from_template"),
                            Some(OverseerValue::Boolean(true))
                        ) || parent.parameters.contains_key("_template_origin");
                        // Update explicit overrides list on this ancestor
                        if is_instance {
                            let entry = parent
                                .parameters
                                .entry("_explicit_overrides".to_string())
                                .or_insert(OverseerValue::String(String::new()));
                            if let OverseerValue::String(s) = entry {
                                if !s.split(',').any(|n| n == child_name) {
                                    if !s.is_empty() {
                                        s.push(',');
                                    }
                                    s.push_str(&child_name);
                                }
                            }
                            break;
                        }
                    }
                    anc.pop();
                }
            }
        }
        Ok(())
    }

    /// Load every mount that asks to be loaded up front.
    ///
    /// Mounted content is never serialized into the host document, so a freshly parsed
    /// document always comes back with its mounts empty. Any formula reading through a mount
    /// would fail until the user clicked Load - which makes a mounted lookup table (a food
    /// catalog, say) unusable as a data source. A mount opts in with `lazy=false` or
    /// `preload=true`; the default stays lazy, so existing documents are unaffected.
    ///
    /// Failures are recorded on the mount as `_mount_status` / `_mount_error` and are never
    /// propagated: a missing or broken side file must not stop the host document opening.
    pub fn preload_mounts(nodes: &mut Vec<OverseerNode>) -> bool {
        fn wants_preload(node: &OverseerNode) -> bool {
            let truthy = |v: Option<&OverseerValue>| match v {
                Some(OverseerValue::Boolean(b)) => Some(*b),
                Some(OverseerValue::String(s)) => match s.trim().to_ascii_lowercase().as_str() {
                    "true" | "yes" | "1" => Some(true),
                    "false" | "no" | "0" => Some(false),
                    _ => None,
                },
                _ => None,
            };
            let p = &node.parameters;
            if truthy(p.get("preload").or_else(|| p.get("_computed_preload"))) == Some(true) {
                return true;
            }
            truthy(p.get("lazy").or_else(|| p.get("_computed_lazy"))) == Some(false)
        }

        fn collect(nodes: &[OverseerNode], trail: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
            for n in nodes {
                trail.push(n.name.clone());
                if n.node_type == "mount" && wants_preload(n) {
                    let already_loaded = matches!(
                        n.parameters.get("_mount_status"),
                        Some(OverseerValue::String(s)) if s == "loaded"
                    ) && !n.children.is_empty();
                    if !already_loaded {
                        out.push(trail.clone());
                    }
                }
                collect(&n.children, trail, out);
                trail.pop();
            }
        }

        let mut targets = Vec::new();
        collect(nodes, &mut Vec::new(), &mut targets);
        let loaded_any = !targets.is_empty();
        RESOLVE_AFTER_MOUNT.with(|flag| flag.set(false));
        let _restore_when_done = MountResolveSuspended;
        for path in targets {
            // Errors are already surfaced on the node itself by the load routine.
            let _ = Self::execute_event(nodes, &path, "load");
        }
        // Tells the caller whether anything was brought in. A document with no mounts needs
        // no second resolve, and that resolve is a full pass over every node in it.
        loaded_any
    }

    fn perform_load_mount_on_owner(
        nodes: &mut Vec<OverseerNode>,
        _owner_ptr: *mut OverseerNode,
        owner_indices: &[usize],
        owner_path: &[String],
    ) -> Result<(), OverseerError> {
        // Call the same logic as execute_load_mount with implicit target = owner
        let action_stub = OverseerNode {
            name: "_implicit".to_string(),
            node_type: "load_mount".to_string(),
            template: None,
            parameters: crate::types::Params::new(),
            children: vec![],
            is_hierarchy_transparent: false,
            param_order: Vec::new(),
            raw_value_literal: None,
            authored_dash: false,
            child_original_index: None,
            leading_blank_lines: 0,
            source_snapshot: None,
            source_id: None,
            source_fingerprint: None,
        };
        Self::execute_load_mount(nodes, owner_indices, owner_path, &action_stub)
    }

    fn perform_unload_mount_on_owner(
        nodes: &mut Vec<OverseerNode>,
        _owner_ptr: *mut OverseerNode,
        owner_indices: &[usize],
    ) -> Result<(), OverseerError> {
        let action_stub = OverseerNode {
            name: "_implicit".to_string(),
            node_type: "unload_mount".to_string(),
            template: None,
            parameters: crate::types::Params::new(),
            children: vec![],
            is_hierarchy_transparent: false,
            param_order: Vec::new(),
            raw_value_literal: None,
            authored_dash: false,
            child_original_index: None,
            leading_blank_lines: 0,
            source_snapshot: None,
            source_id: None,
            source_fingerprint: None,
        };
        // owner_path not needed
        Self::execute_unload_mount(nodes, owner_indices, &Vec::new(), &action_stub)
    }

    /// Parse and resolve a mounted document, reusing the last result while the file on disk
    /// is unchanged.
    ///
    /// Mounts are preloaded after every parse, and the host document is re-parsed on every
    /// edit, so without this a mounted catalog is read, parsed and resolved from scratch for
    /// each keystroke - work that produces an identical answer every time. The file's
    /// modification time and length decide whether the cached copy still applies; anything
    /// that changed the file changes at least one of them.
    fn load_mounted_document(path: &str) -> std::result::Result<Vec<OverseerNode>, String> {
        use std::sync::{LazyLock, Mutex};
        type Stamp = (std::time::SystemTime, u64);
        static CACHE: LazyLock<Mutex<std::collections::HashMap<String, (Stamp, Vec<OverseerNode>)>>> =
            LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

        let stamp = file_stamp(path);

        if let Some(stamp) = stamp {
            if let Ok(cache) = CACHE.lock() {
                if let Some((cached_stamp, nodes)) = cache.get(path) {
                    if *cached_stamp == stamp {
                        return Ok(nodes.clone());
                    }
                }
            }
        }

        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read mount file '{}': {}", path, e))?;
        let (_rem, mut ext_nodes) = crate::parser::parse_document(&content)
            .map_err(|e| format!("Failed to parse mount file '{}': {:?}", path, e))?;
        crate::resolver::resolve_document(&mut ext_nodes);

        if let Some(stamp) = stamp {
            if let Ok(mut cache) = CACHE.lock() {
                cache.insert(path.to_string(), (stamp, ext_nodes.clone()));
            }
        }
        Ok(ext_nodes)
    }

    fn execute_load_mount(
        nodes: &mut Vec<OverseerNode>,
        owner_indices: &[usize],
        owner_path: &[String],
        action: &OverseerNode,
    ) -> Result<(), OverseerError> {
        // Determine target mount: action.parameters.path (optional). If omitted, target is owner.
        let target_indices: Vec<usize> =
            if let Some(OverseerValue::String(path_str)) = action.parameters.get("path") {
                let (segments, _explicit_param, anchored) = Self::split_path_and_param(path_str);
                Self::resolve_target_indices(&nodes, owner_path, anchored, &segments).ok_or_else(
                    || OverseerError::ValidationError("load_mount target not found".to_string()),
                )?
            } else {
                owner_indices.to_vec()
            };
        // Use immutable borrow to read source before mutating
        let mount_ro = Self::get_node_ref_by_indices(&nodes, &target_indices).ok_or_else(|| {
            OverseerError::ValidationError("load_mount target not found".to_string())
        })?;
        if mount_ro.node_type != "mount" {
            return Err(OverseerError::ValidationError(
                "load_mount target must be a 'mount' node".to_string(),
            ));
        }
        let source_val = mount_ro
            .parameters
            .get("source")
            .or_else(|| mount_ro.parameters.get("_computed_source"))
            .ok_or_else(|| {
                OverseerError::ValidationError("mount missing 'source' parameter".to_string())
            })?;
        let source = match source_val {
            OverseerValue::String(s) => s.clone(),
            _ => {
                return Err(OverseerError::ValidationError(
                    "mount.source must be a string".to_string(),
                ))
            }
        };
        // Parse source into (file_path, internal_path)
        let (file_path_opt, internal_path): (Option<String>, Vec<String>) = {
            if let Some(pos) = source.to_lowercase().find(".os") {
                let end = pos + 3; // include .os
                let file = source[..end].to_string();
                let rest = source[end..].to_string();
                let segs: Vec<String> = rest
                    .trim_start_matches('/')
                    .split('/')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect();
                (Some(file), segs)
            } else {
                // No explicit file; treat as absolute/internal path against current document
                (
                    None,
                    source
                        .trim_start_matches('/')
                        .split('/')
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .collect(),
                )
            }
        };
        // Load source nodes with error capture and status updates
        let mut load_error: Option<String> = None;
        // The file read, and how it stood when it was - see `mount_stamp`.
        let mut read_from: Option<(String, String)> = None;
        let loaded_roots: Vec<OverseerNode> = if let Some(fp) = file_path_opt {
            // A mount's source is written relative to the document that declares it, not to
            // wherever the process happens to be running from.
            let fp = crate::docmgr::manager::DocumentManager::resolve_from_document(&fp)
                .to_string_lossy()
                .to_string();
            // Taken before the file is read, so a write landing in between makes the stamp older
            // than what was read, and the next look finds it stale - never the other way round.
            let file = std::fs::canonicalize(&fp).map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| fp.clone());
            read_from = Some((file.clone(), mount_stamp(&file)));
            match Self::load_mounted_document(&fp) {
                Ok(ext_nodes) => ext_nodes,
                Err(message) => {
                    load_error = Some(message);
                    Vec::new()
                }
            }
        } else {
            nodes.clone()
        };
        // Find internal path target and handle missing segments as errors
        let target_node_opt = if internal_path.is_empty() {
            loaded_roots.first().cloned()
        } else if loaded_roots.is_empty() {
            None
        } else {
            let mut cur_opt: Option<OverseerNode> = None;
            for n in &loaded_roots {
                if n.name == internal_path[0] {
                    cur_opt = Some(n.clone());
                    break;
                }
            }
            let mut cur = match cur_opt {
                Some(n) => n,
                None => {
                    if load_error.is_none() {
                        load_error = Some("mount internal path root not found".to_string());
                    }
                    OverseerNode {
                        name: String::new(),
                        node_type: String::new(),
                        template: None,
                        parameters: Default::default(),
                        children: Vec::new(),
                        is_hierarchy_transparent: false,
                        param_order: Vec::new(),
                        raw_value_literal: None,
                        authored_dash: false,
                        child_original_index: None,
                        leading_blank_lines: 0,
                        source_snapshot: None,
                        source_id: None,
                        source_fingerprint: None,
                    }
                }
            };
            if load_error.is_none() && !cur.name.is_empty() {
                for seg in internal_path.iter().skip(1) {
                    if let Some(next) = cur.children.iter().find(|c| &c.name == seg) {
                        cur = next.clone();
                    } else {
                        load_error =
                            Some(format!("mount internal path segment not found: {}", seg));
                        break;
                    }
                }
            }
            if load_error.is_none() && !cur.name.is_empty() {
                Some(cur)
            } else {
                None
            }
        };
        // Mutate the mount node and set status
        let mount_node =
            Self::get_node_mut_by_indices(nodes, &target_indices).ok_or_else(|| {
                OverseerError::ValidationError("load_mount target not found".to_string())
            })?;
        // Which file this came from and how it stood, whether it loaded or not: a document held
        // worked out has this mount's content inside it, and is only as current as that file.
        match &read_from {
            Some((file, stamp)) => {
                mount_node.parameters.insert(MOUNT_FILE.to_string(), OverseerValue::String(file.clone()));
                mount_node.parameters.insert(MOUNT_STAMP.to_string(), OverseerValue::String(stamp.clone()));
            }
            None => {
                mount_node.parameters.remove(MOUNT_FILE);
                mount_node.parameters.remove(MOUNT_STAMP);
            }
        }
        if let Some(err) = load_error {
            mount_node.children.clear();
            mount_node.parameters.insert(
                "_mount_status".to_string(),
                OverseerValue::String("error".to_string()),
            );
            mount_node
                .parameters
                .insert("_mount_error".to_string(), OverseerValue::String(err));
            Ok(())
        } else if let Some(embed) = target_node_opt {
            mount_node.children = vec![embed];
            mount_node.parameters.insert(
                "_mount_status".to_string(),
                OverseerValue::String("loaded".to_string()),
            );
            mount_node.parameters.remove("_mount_error");
            Ok(())
        } else {
            mount_node.children.clear();
            mount_node.parameters.insert(
                "_mount_status".to_string(),
                OverseerValue::String("error".to_string()),
            );
            mount_node.parameters.insert(
                "_mount_error".to_string(),
                OverseerValue::String("mount source resolved to empty document".to_string()),
            );
            Ok(())
        }
    }

    fn execute_unload_mount(
        nodes: &mut Vec<OverseerNode>,
        owner_indices: &[usize],
        owner_path: &[String],
        action: &OverseerNode,
    ) -> Result<(), OverseerError> {
        let target_indices: Vec<usize> =
            if let Some(OverseerValue::String(path_str)) = action.parameters.get("path") {
                let (segments, _explicit_param, anchored) = Self::split_path_and_param(path_str);
                Self::resolve_target_indices(&nodes, owner_path, anchored, &segments).ok_or_else(
                    || OverseerError::ValidationError("unload_mount target not found".to_string()),
                )?
            } else {
                owner_indices.to_vec()
            };
        let mount_node =
            Self::get_node_mut_by_indices(nodes, &target_indices).ok_or_else(|| {
                OverseerError::ValidationError("unload_mount target not found".to_string())
            })?;
        if mount_node.node_type != "mount" {
            return Err(OverseerError::ValidationError(
                "unload_mount target must be a 'mount' node".to_string(),
            ));
        }
        mount_node.children.clear();
        mount_node.parameters.insert(
            "_mount_status".to_string(),
            OverseerValue::String("unloaded".to_string()),
        );
        mount_node.parameters.remove("_mount_error");
        // Nothing of the file is held any more, so nothing depends on it.
        mount_node.parameters.remove(MOUNT_FILE);
        mount_node.parameters.remove(MOUNT_STAMP);
        Ok(())
    }

    fn inc_value(
        nodes: &mut Vec<OverseerNode>,
        _owner_indices: &[usize],
        owner_path: &[String],
        target: &str,
        by: f64,
    ) -> Result<(), OverseerError> {
        let (segments, explicit_param, anchored) = Self::split_path_and_param(target);
        let indices = match Self::resolve_target_indices(&nodes, owner_path, anchored, &segments) {
            Some(ix) => ix,
            None => {
                #[cfg(feature = "debug-resolver")]
                eprintln!("[ACTIONS] inc: Target not found: {} (owner_path={:?}, anchored={}, segments={:?})", target, owner_path, anchored, segments);
                return Err(OverseerError::ValidationError(format!(
                    "Target not found: {}",
                    target
                )));
            }
        };
        let address = Self::build_disambiguated_path(nodes, &indices).join("/");
        let node = Self::get_node_mut_by_indices(nodes, &indices).ok_or_else(|| {
            OverseerError::ValidationError(format!("Target not found: {}", target))
        })?;
        let key = explicit_param.unwrap_or_else(|| "value".to_string());
        let cur_val = Self::get_effective(&node.parameters, &key)
            .or_else(|| node.parameters.get(&key))
            .ok_or_else(|| {
                OverseerError::ValidationError(format!("No current value at {}.{}", target, key))
            })?;
        let new_val = match cur_val {
            OverseerValue::Integer(i) => {
                let res = *i as f64 + by;
                if res.fract() == 0.0 {
                    OverseerValue::Integer(res as i64)
                } else {
                    OverseerValue::Float(res)
                }
            }
            OverseerValue::Float(f) => OverseerValue::Float(*f + by),
            _ => {
                return Err(OverseerError::ValidationError(
                    "inc target must be number".to_string(),
                ))
            }
        };
        node.parameters.insert(key.clone(), new_val);
        // The serializer replays a node from its source text while the fingerprint it was
        // parsed with still matches, and that fingerprint says nothing about what was just
        // written here. Without this the increment lands in memory, the document is written
        // back as exactly what it was read from, and the press reports success having changed
        // nothing. `set`, `toggle` and `clear` have always done this; `inc` never did.
        node.source_fingerprint = None;
        Self::record_an_override(node, &key);
        note_field(if key == "value" { address } else { format!("{}/{}", address, key) });
        Ok(())
    }

    /// Say that this value was written here rather than inherited from a template.
    ///
    /// An entry made from a template is written out as only what distinguishes it from that
    /// template, and the serializer decides what that is by looking for this marker. Without it
    /// a write into such an entry lands in memory, is reported as a change, answers Ok - and is
    /// then dropped on the way to the file. A write that reports success and does nothing.
    ///
    /// `set` and `toggle` have always left the marker. `inc` never did, so a button counting
    /// something up worked everywhere except on a list entry, which is where such buttons
    /// mostly are. Kept in one place now so the next action added cannot quietly omit it.
    fn record_an_override(node: &mut OverseerNode, key: &str) {
        node.parameters.remove(&format!("_template_{}", key));
        if key != "value" {
            return;
        }
        node.parameters.insert(
            "_override_present".to_string(),
            OverseerValue::Boolean(true),
        );
        node.parameters.insert(
            "_explicit_child_override".to_string(),
            OverseerValue::Boolean(true),
        );
        // A value written here is no longer one worked out from a formula.
        node.parameters.remove("_computed_value");
    }

    fn toggle_value(
        nodes: &mut Vec<OverseerNode>,
        _owner_indices: &[usize],
        owner_path: &[String],
        target: &str,
    ) -> Result<(), OverseerError> {
        let (segments, explicit_param, anchored) = Self::split_path_and_param(target);
        let indices = match Self::resolve_target_indices(&nodes, owner_path, anchored, &segments) {
            Some(ix) => ix,
            None => {
                #[cfg(feature = "debug-resolver")]
                eprintln!("[ACTIONS] toggle: Target not found: {} (owner_path={:?}, anchored={}, segments={:?})", target, owner_path, anchored, segments);
                return Err(OverseerError::ValidationError(format!(
                    "Target not found: {}",
                    target
                )));
            }
        };
        Self::invalidate_source_fingerprints(nodes, &indices);
        let address = Self::build_disambiguated_path(nodes, &indices).join("/");
        let node = Self::get_node_mut_by_indices(nodes, &indices).ok_or_else(|| {
            OverseerError::ValidationError(format!("Target not found: {}", target))
        })?;
        let key = explicit_param.unwrap_or_else(|| "value".to_string());
        let cur_val = Self::get_effective(&node.parameters, &key)
            .or_else(|| node.parameters.get(&key))
            .ok_or_else(|| {
                OverseerError::ValidationError(format!("No current value at {}.{}", target, key))
            })?;
        let new_val = match cur_val {
            OverseerValue::Boolean(b) => OverseerValue::Boolean(!b),
            _ => {
                return Err(OverseerError::ValidationError(
                    "toggle target must be boolean".to_string(),
                ))
            }
        };
    node.parameters.insert(key.clone(), new_val);
    node.source_fingerprint = None;
        note_field(if key == "value" { address } else { format!("{}/{}", address, key) });
        // Mark explicit override if this is a template-derived child and we're toggling its value
        if key == "value" {
            node.parameters.insert(
                "_override_present".to_string(),
                OverseerValue::Boolean(true),
            );
            node.parameters.insert(
                "_explicit_child_override".to_string(),
                OverseerValue::Boolean(true),
            );
            node.parameters.remove("_template_value");
            node.parameters.remove("_computed_value");
            node.authored_dash = true;
            // Record on nearest template instance ancestor
            if !indices.is_empty() {
                let child_name = node.name.clone();
                let mut anc = indices.clone();
                anc.pop();
                while !anc.is_empty() {
                    if let Some(parent) = Self::get_node_mut_by_indices(nodes, &anc) {
                        let is_instance = matches!(
                            parent.parameters.get("_from_template"),
                            Some(OverseerValue::Boolean(true))
                        ) || parent.parameters.contains_key("_template_origin");
                        if is_instance {
                            let entry = parent
                                .parameters
                                .entry("_explicit_overrides".to_string())
                                .or_insert(OverseerValue::String(String::new()));
                            if let OverseerValue::String(s) = entry {
                                if !s.split(',').any(|n| n == child_name) {
                                    if !s.is_empty() {
                                        s.push(',');
                                    }
                                    s.push_str(&child_name);
                                }
                            }
                            break;
                        }
                    }
                    anc.pop();
                }
            }
        }
        Ok(())
    }

    fn invalidate_source_fingerprints(nodes: &mut Vec<OverseerNode>, indices: &[usize]) {
        if indices.is_empty() {
            return;
        }
        let mut path = indices.to_vec();
        while !path.is_empty() {
            if let Some(node) = Self::get_node_mut_by_indices(nodes, &path) {
                node.source_fingerprint = None;
            }
            path.pop();
        }
    }

    fn clear_value(
        nodes: &mut Vec<OverseerNode>,
        _owner_indices: &[usize],
        owner_path: &[String],
        target: &str,
    ) -> Result<(), OverseerError> {
        let (segments, explicit_param, anchored) = Self::split_path_and_param(target);
        let indices = match Self::resolve_target_indices(&nodes, owner_path, anchored, &segments) {
            Some(ix) => ix,
            None => {
                #[cfg(feature = "debug-resolver")]
                eprintln!("[ACTIONS] clear: Target not found: {} (owner_path={:?}, anchored={}, segments={:?})", target, owner_path, anchored, segments);
                return Err(OverseerError::ValidationError(format!(
                    "Target not found: {}",
                    target
                )));
            }
        };
        let address = Self::build_disambiguated_path(nodes, &indices).join("/");
        let node = Self::get_node_mut_by_indices(nodes, &indices).ok_or_else(|| {
            OverseerError::ValidationError(format!("Target not found: {}", target))
        })?;
        let key = explicit_param.unwrap_or_else(|| "value".to_string());
        node.parameters.remove(&key);
        // Same fault `inc` had: without clearing the fingerprint the serializer writes the node
        // back as the text it was read from, so clearing a field changes nothing on disk.
        node.source_fingerprint = None;
        node.parameters.remove(&format!("_template_{}", key));
        note_field(if key == "value" { address } else { format!("{}/{}", address, key) });
        Ok(())
    }

    // Find any node by name recursively
    fn find_node_by_name<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
        for n in nodes {
            if n.name == name {
                return Some(n);
            }
            if let Some(found) = Self::find_node_by_name(&n.children, name) {
                return Some(found);
            }
        }
        None
    }

    // Create a list item by cloning template definition and marking template-derived fields
    /// Cut a cloned subtree loose from the text the template was written in.
    ///
    /// The root of a new instance already does this; its children were being cloned wholesale,
    /// keeping ids that point at the template's own source. The serializer replays a node from
    /// there when it can, so every entry added to a list arrived carrying the template's
    /// comments - one more copy of "// Per 100 g - the canonical side" per food, for ever.
    ///
    /// A new entry has no source text of its own. Saying so is the whole fix: what it needs
    /// written is computed from the node instead.
    fn forget_where_the_template_came_from(node: &mut OverseerNode) {
        node.source_id = None;
        node.source_fingerprint = None;
        node.source_snapshot = None;
        // Spacing belonged to the template's layout, not to this entry.
        node.leading_blank_lines = 0;
        for child in node.children.iter_mut() {
            Self::forget_where_the_template_came_from(child);
        }
    }

    fn clone_from_template(template: &OverseerNode) -> OverseerNode {
        // Start parameters with template defaults and add _template_ markers so serializer can skip them
        let mut params = template.parameters.clone();
        // Preserve original container type if present on the template (e.g., an instance carries _original_type="div").
        // Fallback to template.node_type when no explicit original type is recorded.
        let orig_ty = match template.parameters.get("_original_type") {
            Some(OverseerValue::String(s)) => s.clone(),
            _ => template.node_type.clone(),
        };
        params.insert("_original_type".to_string(), OverseerValue::String(orig_ty));
        // Mark that this node originates from a template so serializer can suppress inherited children
        params.insert("_from_template".to_string(), OverseerValue::Boolean(true));
        // Add per-parameter template markers (skip internal keys)
        let keys: Vec<String> = params
            .keys()
            .filter(|k| !k.starts_with('_'))
            .cloned()
            .collect();
        for k in keys {
            if let Some(v) = params.get(&k).cloned() {
                params.insert(format!("_template_{}", k), v);
            }
        }

        // Clone children and mark entire subtree as template-derived so we don't save inherited fields
        let mut children = template.children.clone();
        for ch in children.iter_mut() {
            Self::mark_template_child_recursive_action(ch);
            Self::clear_computed_recursive(ch);
            Self::forget_where_the_template_came_from(ch);
        }

        // Preserve the instantiated type semantics: if this template node represents an instance of a component
        // (node_type == template.name) and carries _original_type indicating the container kind (e.g., div),
        // keep node_type equal to the component name (like list/template instantiation does) so fields are found
        // by name, but also record _original_type for layout/rendering. This mirrors resolver behavior for instances.
        let mut node = OverseerNode {
            name: template.name.clone(),
            node_type: template.name.clone(),
            template: None,
            parameters: params,
            children,
            is_hierarchy_transparent: template.is_hierarchy_transparent,
            param_order: Vec::new(),
            raw_value_literal: None,
            authored_dash: false,
            child_original_index: None,
            leading_blank_lines: 0,
            source_snapshot: None,
            source_id: None,
            source_fingerprint: None,
        };
        // Also clear computed params at the root clone
        Self::clear_computed_recursive(&mut node);
        node.adopt_template_snapshot(template);
        if node.source_snapshot.is_none() {
            let (indent_unit, newline) = template
                .source_snapshot
                .as_ref()
                .map(|snap| (snap.indent_unit.clone(), snap.newline.clone()))
                .unwrap_or((None, None));
            node.synthesize_snapshot_with_style_recursive(indent_unit, newline);
        }
        node
    }

    // Mark a node and its subtree as template-derived for serializer filtering
    fn mark_template_child_recursive_action(node: &mut OverseerNode) {
        if let Some(existing_snapshot) = node.source_snapshot.clone() {
            let fingerprint = existing_snapshot.fingerprint;
            node.source_snapshot = Some(NodeSourceSnapshot::template_clone_of(&existing_snapshot));
            node.source_fingerprint = Some(fingerprint);
        } else {
            node.source_fingerprint = None;
        }
        node.source_id = None;
        node.parameters
            .insert("_template_node".to_string(), OverseerValue::Boolean(true));
        let keys: Vec<String> = node
            .parameters
            .keys()
            .filter(|k| !k.starts_with('_'))
            .cloned()
            .collect();
        for k in keys {
            if let Some(v) = node.parameters.get(&k).cloned() {
                node.parameters.insert(format!("_template_{}", k), v);
            }
        }
        for ch in node.children.iter_mut() {
            Self::mark_template_child_recursive_action(ch);
        }
    }

    fn set_field_value_on_item(item: &mut OverseerNode, field: &str, value: OverseerValue) {
        if let Some(child) = item.children.iter_mut().find(|c| c.name == field) {
            child.parameters.insert("value".to_string(), value);
            // Mark explicit override so serializer will persist this child even if template-derived
            child.source_fingerprint = None;
            child.parameters.insert(
                "_override_present".to_string(),
                OverseerValue::Boolean(true),
            );
            child.parameters.insert(
                "_explicit_child_override".to_string(),
                OverseerValue::Boolean(true),
            );
            // Treat runtime-created explicit overrides as dash-authored for concise style persistence
            child.authored_dash = true;
            // Remove template marker for value on this field if present
            child.parameters.remove("_template_value");
            // Clear any stale computed value on this field
            child.parameters.remove("_computed_value");
            // Track override at parent level for completeness
            let entry = item
                .parameters
                .entry("_explicit_overrides".to_string())
                .or_insert(OverseerValue::String(String::new()));
            if let OverseerValue::String(s) = entry {
                if !s.split(',').any(|n| n == field) {
                    if !s.is_empty() {
                        s.push(',');
                    }
                    s.push_str(field);
                }
            }
        } else {
            // Add simple string field if missing
            let mut new_field = OverseerNode {
                name: field.to_string(),
                node_type: "string".to_string(),
                template: None,
                parameters: {
                    let mut m = crate::types::Params::new();
                    m.insert("value".to_string(), value);
                    m.insert(
                        "_override_present".to_string(),
                        OverseerValue::Boolean(true),
                    );
                    m.insert(
                        "_explicit_child_override".to_string(),
                        OverseerValue::Boolean(true),
                    );
                    m
                },
                children: Vec::new(),
                is_hierarchy_transparent: false,
                param_order: Vec::new(),
                raw_value_literal: None,
                authored_dash: true,
                child_original_index: None,
                leading_blank_lines: 0,
                source_snapshot: None,
                source_id: None,
                source_fingerprint: None,
            };
            let (indent_unit, newline) = item
                .source_snapshot
                .as_ref()
                .map(|snap| (snap.indent_unit.clone(), snap.newline.clone()))
                .unwrap_or((None, None));
            new_field.synthesize_snapshot_with_style_recursive(indent_unit, newline);
            item.children.push(new_field);
            // Track override at parent level
            let entry = item
                .parameters
                .entry("_explicit_overrides".to_string())
                .or_insert(OverseerValue::String(String::new()));
            if let OverseerValue::String(s) = entry {
                if !s.split(',').any(|n| n == field) {
                    if !s.is_empty() {
                        s.push(',');
                    }
                    s.push_str(field);
                }
            }
        }
    }

    // Remove any computed shadow parameters so formulas recompute in the new instance context
    fn clear_computed_recursive(node: &mut OverseerNode) {
        let keys: Vec<String> = node
            .parameters
            .keys()
            .filter(|k| k.starts_with("_computed_"))
            .cloned()
            .collect();
        for k in keys {
            node.parameters.remove(&k);
        }
        // Special-case top-level computed value
        node.parameters.remove("_computed_value");
        for ch in node.children.iter_mut() {
            Self::clear_computed_recursive(ch);
        }
    }

    fn value_equals(a: &OverseerValue, b: &OverseerValue) -> bool {
        a == b
    }

    // Compare values with optional keyPrecision semantics defined on a list node (e.g., "day").
    fn value_equals_with_key_precision(
        list_node: &OverseerNode,
        a: &OverseerValue,
        b: &OverseerValue,
    ) -> bool {
        // Support keyPrecision="day" to treat timestamps/dates on the same calendar day as equal
        let precision = list_node
            .parameters
            .get("keyPrecision")
            .or_else(|| list_node.parameters.get("key_precision"));
        if let Some(OverseerValue::String(p)) = precision {
            if p == "day" {
                fn to_date(v: &OverseerValue) -> Option<chrono::NaiveDate> {
                    match v {
                        OverseerValue::Date(s) => {
                            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
                        }
                        OverseerValue::Timestamp(s) | OverseerValue::String(s) => {
                            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
                                Some(dt.with_timezone(&chrono::Utc).date_naive())
                            } else if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
                                Some(d)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    }
                }
                if let (Some(da), Some(db)) = (to_date(a), to_date(b)) {
                    return da == db;
                }
            }
        }
        Self::value_equals(a, b)
    }

    /// An entry's field, by name, for finding an entry by its key. Looked for among the entry's
    /// own fields and those of the unnamed divs that lay it out, as addresses and
    /// `refuse_a_key_already_held` do: a project's closed record keeps its handle in such a row,
    /// and a `remove` by that handle found nothing and quietly left the record where it was.
    fn get_field_value<'a>(item: &'a OverseerNode, field: &str) -> Option<&'a OverseerValue> {
        if let Some(found) = item.children.iter().find(|c| c.name == field) {
            return found.parameters.get("value");
        }
        item.children
            .iter()
            .filter(|c| c.is_hierarchy_transparent && (c.name.is_empty() || c.name == c.node_type))
            .find_map(|c| Self::get_field_value(c, field))
    }

    /// Make sure the list has an entry with this key, and leave it alone if it already does.
    ///
    /// This is what a view onto a key the list lacks means by making its preview real: the entry
    /// appears at that key, with the template's fields, and everything after that is an ordinary
    /// entry being edited.
    ///
    /// Finished the same way an appended or prepended entry is - the list's own entry style, and
    /// the mark that says the entry is new. That mark is what `freeze=true` acts on, so without
    /// it a day created this way silently kept reading a standing figure that a day created any
    /// other way had written into it.
    fn ensure_in_list(
        nodes: &mut Vec<OverseerNode>,
        owner_path: &[String],
        list_path: &str,
        template_name: &str,
        key_field: &str,
        key_value: OverseerValue,
        goes: WhereItGoes,
    ) -> Result<String, OverseerError> {
        let (segments, _explicit_param, anchored) = Self::split_path_and_param(list_path);
        let indices = Self::resolve_target_indices(nodes, owner_path, anchored, &segments)
            .ok_or_else(|| {
                OverseerError::ValidationError(format!("List not found: {}", list_path))
            })?;
        // Named before the entry is made, as the graph names the list - see `Shape`.
        let list_address = Self::build_disambiguated_path(nodes, &indices).join("/");

        // Everything the entry is made from, read before anything is written - the way `append`
        // makes one, see `put_entry_in`, and for the same reason: a copy of the whole document
        // taken to read the template from while the list was held open was over twenty
        // milliseconds of making the first entry of a day.
        let new_item = {
            let document: &Vec<OverseerNode> = nodes;
            let list_node = Self::get_node_ref_by_indices(document, &indices).ok_or_else(|| {
                OverseerError::ValidationError(format!("List not found: {}", list_path))
            })?;
            if list_node.node_type != "list" {
                return Err(OverseerError::ValidationError(
                    "ensure_in_list.target is not a list".to_string(),
                ));
            }

            // Derive key field from list parameters if not provided
            let effective_key_field = if !key_field.is_empty() {
                key_field.to_string()
            } else if let Some(OverseerValue::String(s)) = list_node.parameters.get("key") {
                s.clone()
            } else {
                return Err(OverseerError::ValidationError(
                    "ensure_in_list.keyField missing and list has no key".to_string(),
                ));
            };

            // Already there: nothing to make, and the caller is told which one it is. Saying so
            // rather than just "done" is what lets one instruction mean "make sure of this entry
            // and then write into it" - the same sentence whether the entry was a preview a moment
            // ago or has been in the file for a month.
            if let Some(found) = list_node.children.iter().find(|it| {
                Self::get_field_value(it, &effective_key_field).map_or(false, |v| {
                    Self::value_equals_with_key_precision(list_node, v, &key_value)
                })
            }) {
                return Ok(found.name.clone());
            }

            // Find template by name (accept both "Record" and "<Record>" forms)
            let tn = if template_name.starts_with('<')
                && template_name.ends_with('>')
                && template_name.len() >= 2
            {
                &template_name[1..template_name.len() - 1]
            } else {
                template_name
            };
            let template_def = Self::find_node_by_name(document, tn).ok_or_else(|| {
                OverseerError::ValidationError(format!("Template not found: {}", template_name))
            })?;
            let style_guide = Self::derive_list_entry_style(list_node, goes == WhereItGoes::First);
            let mut new_item = Self::clone_from_template(template_def);
            Self::set_field_value_on_item(&mut new_item, &effective_key_field, key_value);
            // Apply layout opposite to parent list's effective layout (to match default alternation rule)
            if let Some(OverseerValue::String(parent_eff)) =
                list_node.parameters.get("_effective_layout")
            {
                let opp = Self::opposite_layout(parent_eff);
                new_item
                    .parameters
                    .insert("_effective_layout".to_string(), OverseerValue::String(opp));
            }
            // Assign a unique instance name mirroring resolver reload semantics (e.g., T__1, T__2)
            // This prevents duplicate sibling names that break name-based path resolution during formula evaluation.
            let ordinal = list_node.children.len() + 1; // 1-based index after append
            new_item.name = format!("{}__{}", template_def.name, ordinal);
            Self::apply_list_entry_style(&mut new_item, &style_guide);
            Self::mark_the_entry_as_new(&mut new_item);
            new_item
        };

        let made = new_item.name.clone();
        let list_node = Self::get_node_mut_by_indices(nodes, &indices).ok_or_else(|| {
            OverseerError::ValidationError(format!("List not found: {}", list_path))
        })?;
        match goes {
            WhereItGoes::First => list_node.children.insert(0, new_item),
            WhereItGoes::Last => list_node.children.push(new_item),
        }
        // The list's text no longer matches what was parsed from it, so it has to be written out
        // again rather than replayed - the same reason removing an entry clears this.
        list_node.source_fingerprint = None;
        // Shape, not value - and only when an entry was made: finding the one already there
        // changes nothing.
        note_shape(Shape::Added { list: list_address, entry: made.clone() });
        Ok(made)
    }

    /// Make sure a list has an entry with this key, and say which entry that is.
    ///
    /// The public way in, for a caller holding a path of names rather than an action block: the
    /// page, when a view onto a key the list lacks is edited and the preview has to become real.
    pub fn ensure_entry(
        nodes: &mut Vec<OverseerNode>,
        list_path: &str,
        template_name: &str,
        key_field: &str,
        key_value: OverseerValue,
        goes: WhereItGoes,
    ) -> Result<String, OverseerError> {
        note_acted();
        Self::ensure_in_list(
            nodes,
            &[],
            list_path,
            template_name,
            key_field,
            key_value,
            goes,
        )
    }

    /// Where the one entry of a list holding `value` in `field` stands, if any does.
    ///
    /// Refused when several do, rather than taking the first. The first of several used to be
    /// the one removed, changed or moved, and the press asking was nearly always about another:
    /// `finish` on the fourth of four project tasks sharing an `added` stamp recorded that one as
    /// finished and took the first off the list - gone, and recorded nowhere. A refused press
    /// writes nothing at all, so the worst it costs is the press.
    fn the_one_holding(
        list: &OverseerNode,
        field: &str,
        value: &OverseerValue,
        list_path: &str,
        doing: &str,
        holds: impl Fn(&OverseerValue) -> bool,
    ) -> Result<Option<usize>, OverseerError> {
        let holding: Vec<usize> = list
            .children
            .iter()
            .enumerate()
            .filter(|(_, it)| Self::get_field_value(it, field).map_or(false, &holds))
            .map(|(at, _)| at)
            .collect();
        if holding.len() > 1 {
            return Err(OverseerError::ValidationError(format!(
                "{} entries of {} hold {} = {}, so which one to {} is not clear; nothing was changed",
                holding.len(),
                list_path,
                field,
                as_text(value).unwrap_or_else(|| format!("{:?}", value)),
                doing
            )));
        }
        Ok(holding.first().copied())
    }

    /// Refuse an entry a keyed list would not be able to tell apart from another.
    ///
    /// A list with `key=` is addressed by that field, so an entry arriving with it empty, or with a
    /// value an entry there holds already, gives the list two entries at one address - and every
    /// later remove, change or move by that key is then refused by `the_one_holding`. The form in
    /// the project documents checks before it appends, but only because it was written to; the
    /// bot, another document's button or a form that forgets did not. So it is checked here, where
    /// every append, prepend and move into another list arrives. Duplicates already in a file are
    /// left alone: only a new one is refused.
    ///
    /// The field is looked for among the entry's own fields and those of the unnamed divs that lay
    /// it out, since a template often wraps its fields in a row. A key still a formula is left to
    /// the resolver: what it comes to is not known yet, and every entry made from that template
    /// would look the same until it is.
    fn refuse_a_key_already_held(
        list: &OverseerNode,
        entry: &OverseerNode,
        list_path: &str,
        doing: &str,
    ) -> Result<(), OverseerError> {
        let Some(OverseerValue::String(field)) = list.parameters.get("key") else { return Ok(()) };
        if field.is_empty() || entry.children.is_empty() {
            return Ok(());
        }
        fn key_of<'a>(entry: &'a OverseerNode, field: &str) -> Option<&'a OverseerValue> {
            if let Some(found) = entry.children.iter().find(|c| c.name == field) {
                return found.parameters.get("_computed_value").or_else(|| found.parameters.get("value"));
            }
            entry
                .children
                .iter()
                .filter(|c| c.is_hierarchy_transparent && (c.name.is_empty() || c.name == c.node_type))
                .find_map(|c| key_of(c, field))
        }
        let wanted = key_of(entry, field);
        if matches!(wanted, Some(OverseerValue::Formula(_))) {
            return Ok(());
        }
        let wanted_text = wanted.and_then(as_text).unwrap_or_default();
        if wanted_text.trim().is_empty() {
            return Err(OverseerError::ValidationError(format!(
                "{} is kept by {}, and the entry to {} has none; nothing was changed",
                list_path, field, doing
            )));
        }
        let wanted = wanted.unwrap();
        let taken = list.children.iter().filter_map(|it| key_of(it, field)).any(|held| {
            Self::value_equals_with_key_precision(list, held, wanted)
                || as_text(held).map_or(false, |t| t == wanted_text)
        });
        if taken {
            return Err(OverseerError::ValidationError(format!(
                "{} already holds an entry with {} = {}, so the entry to {} would share its address; nothing was changed",
                list_path, field, wanted_text, doing
            )));
        }
        Ok(())
    }

    fn remove_from_list(
        nodes: &mut Vec<OverseerNode>,
        owner_path: &[String],
        list_path: &str,
        key_field: &str,
        key_value: &OverseerValue,
    ) -> Result<(), OverseerError> {
        let (segments, _explicit_param, anchored) = Self::split_path_and_param(list_path);
        let indices = Self::resolve_target_indices(&nodes, owner_path, anchored, &segments)
            .ok_or_else(|| {
                OverseerError::ValidationError(format!("List not found: {}", list_path))
            })?;
        // As the graph names the list - see `Shape`.
        let list_address = Self::build_disambiguated_path(nodes, &indices).join("/");
        let list_node = Self::get_node_mut_by_indices(nodes, &indices).ok_or_else(|| {
            OverseerError::ValidationError(format!("List not found: {}", list_path))
        })?;
        if list_node.node_type != "list" {
            return Err(OverseerError::ValidationError(
                "remove.target is not a list".to_string(),
            ));
        }

        // Determine key field
        let effective_key_field = if !key_field.is_empty() {
            key_field.to_string()
        } else if let Some(OverseerValue::String(s)) = list_node.parameters.get("key") {
            s.clone()
        } else {
            return Err(OverseerError::ValidationError(
                "remove.keyField missing and list has no key".to_string(),
            ));
        };

        let holding = Self::the_one_holding(list_node, &effective_key_field, key_value, list_path, "remove", |v| {
            Self::value_equals_with_key_precision(list_node, v, key_value)
        })?;
        if let Some(pos) = holding {
            let gone = list_node.children.remove(pos);
            // Shape, not value - and only when something was taken out.
            note_shape(Shape::Removed { list: list_address, entry: gone.name });
            // The list no longer matches the text it was read from. Saying so is what
            // makes the removal stick: the serializer replays a node from its source
            // snapshot while the fingerprint still matches, and every remaining entry
            // does match - so the list would be written back exactly as it was, with
            // the removed entry among them. An append needs no such marking, because
            // the new entry has no source to replay and gives the list away.
            list_node.source_fingerprint = None;
            // Mark this list field as explicitly overridden so mutations persist on template instances
            Self::mark_field_explicit_override(nodes, &indices);
        }
        Ok(())
    }

    /// Append an entry to a list, as an `append` action in a document would.
    ///
    /// Exposed for callers that are not documents - the API a bot writes through - so that a
    /// row added from outside is built exactly like a row added by a button. Anything else
    /// and the two would drift, and the difference would show up as a document that behaves
    /// differently depending on who wrote to it.
    pub fn append_entry(
        nodes: &mut Vec<OverseerNode>,
        list_path: &str,
        overrides: &Vec<OverseerNode>,
    ) -> Result<(), OverseerError> {
        note_acted();
        Self::append_to_list(nodes, &[], list_path, None, None, overrides)
    }

    /// Take one entry out of the list it sits in, named by its own path.
    ///
    /// By path rather than by key, which is what the `remove` action uses: a list does not
    /// have to declare a key, and the one meals are recorded in does not. An entry is
    /// identified by where it is, which is what a caller reading the document already has.
    pub fn remove_entry(
        nodes: &mut Vec<OverseerNode>,
        entry_path: &str,
    ) -> Result<(), OverseerError> {
        note_acted();
        let (segments, _explicit_param, anchored) = Self::split_path_and_param(entry_path);
        let indices = Self::resolve_target_indices(&nodes, &[], anchored, &segments)
            .ok_or_else(|| {
                OverseerError::ValidationError(format!("Nothing at: {}", entry_path))
            })?;

        let (last, parent_indices) = indices
            .split_last()
            .ok_or_else(|| OverseerError::ValidationError("Nothing to remove".to_string()))?;
        // As the graph names the list - see `Shape`.
        let list_address = Self::build_disambiguated_path(nodes, parent_indices).join("/");

        let parent = Self::get_node_mut_by_indices(nodes, parent_indices).ok_or_else(|| {
            OverseerError::ValidationError(format!("No list holds {}", entry_path))
        })?;
        if parent.node_type != "list" {
            return Err(OverseerError::ValidationError(format!(
                "{} is not in a list, so there is nothing to remove it from",
                entry_path
            )));
        }
        if *last >= parent.children.len() {
            return Err(OverseerError::ValidationError(format!(
                "Nothing at: {}",
                entry_path
            )));
        }

        let gone = parent.children.remove(*last);
        // Shape, not value - see `Shape`. Unsaid, what reached the resolver looked like a change
        // to values: the graph was asked about a document whose entries had moved, and the
        // survivors kept names their text no longer gives them.
        note_shape(Shape::Removed { list: list_address, entry: gone.name });
        // The list's text no longer matches what was parsed from it, so it has to be written
        // out again rather than replayed - the same reason an edit to a value clears this.
        parent.source_fingerprint = None;
        Ok(())
    }

    /// Set a value, as a `set` action would.
    pub fn assign_value(
        nodes: &mut Vec<OverseerNode>,
        target: &str,
        value: OverseerValue,
    ) -> Result<(), OverseerError> {
        note_acted();
        Self::set_value(nodes, &[], &[], target, value)
    }

    /// The node a copy from `from` reads, found from the action's own place.
    ///
    /// `append (list=..., from="..")` fills the new entry from another node - a div of textboxes,
    /// in the case it was made for, so a form can add a list entry in one press without the
    /// action naming each field, and a list entry, so a project item closes and reopens without
    /// its handlers naming each field it carries. A copy is taken, since the document is written
    /// to while the source's lists are still being copied out of it.
    fn source_of_a_copy(
        snapshot: &Vec<OverseerNode>,
        owner_path: &[String],
        from: &OverseerValue,
    ) -> Result<OverseerNode, OverseerError> {
        let from_path = match Self::evaluate_in_context(from, owner_path, snapshot)? {
            OverseerValue::String(s) => s,
            other => {
                return Err(OverseerError::ValidationError(format!(
                    "from must name a path, got {:?}",
                    other
                )))
            }
        };
        let (segments, _param, anchored) = Self::split_path_and_param(&from_path);
        Self::resolve_target_indices(snapshot, owner_path, anchored, &segments)
            .and_then(|at| Self::get_node_ref_by_indices(snapshot, &at))
            .cloned()
            .ok_or_else(|| OverseerError::ValidationError(format!("Source not found: {}", from_path)))
    }

    /// What a copy from `source` puts into an entry made from this template: the overrides for its
    /// fields, and the lists whose entries are to follow it in.
    ///
    /// The entry is still made from the list's template; the source only supplies values. A field
    /// is matched by its name, and by the names of the divs it sits in, with unnamed wrappers
    /// ignored on both sides, since those arrange rather than mean anything. The template's type
    /// is kept where the two differ and the value converted to it where it can be - text into a
    /// number, a date, a flag - and left out where it cannot, or where there is nothing in it, so
    /// the template's default stands. A field the action's own block names is the block's:
    /// `- added = $(now())` beside a copy says what the form does not.
    ///
    /// A field the template works out by formula is never filled: a project item and its record
    /// each keep a colour of their own, and a copy by name would write one over the other's
    /// formula. A field the source works out is copied as what it works out to, like any value -
    /// a form's id box offers its key that way. A list is matched the way a field is, and comes
    /// back with the source's list, for its entries to be copied one by one once the entry is in
    /// place, each made from the target list's own template.
    ///
    /// The textboxes read here are noted, so the page can empty them once the press is done.
    fn copied_into(
        source: &OverseerNode,
        template: &OverseerNode,
        block: &[OverseerNode],
    ) -> (Vec<OverseerNode>, Vec<(String, OverseerNode)>) {
        const HOLDS_A_VALUE: [&str; 11] =
            ["string", "text", "textbox", "int", "float", "bool", "checkbox", "date", "timestamp", "tags", "enum"];
        fn is_wrapper(node: &OverseerNode) -> bool {
            node.is_hierarchy_transparent && (node.name.is_empty() || node.name == node.node_type)
        }
        type Found<'a> = Vec<(String, &'a OverseerNode)>;
        /// Every field and every list under a node, by the names that lead to it.
        fn fields<'a>(node: &'a OverseerNode, trail: &mut Vec<String>, out: &mut Found<'a>, lists: &mut Found<'a>) {
            for child in &node.children {
                if HOLDS_A_VALUE.contains(&child.node_type.as_str()) || child.node_type == "list" {
                    if !child.name.is_empty() {
                        trail.push(child.name.clone());
                        let found = if child.node_type == "list" { &mut *lists } else { &mut *out };
                        found.push((trail.join("/"), child));
                        trail.pop();
                    }
                } else if child.node_type == "on" || child.node_type == "button" {
                    // What a form holds besides its fields: the button that sends it.
                } else if is_wrapper(child) {
                    fields(child, trail, out, lists);
                } else if !child.name.is_empty() {
                    trail.push(child.name.clone());
                    fields(child, trail, out, lists);
                    trail.pop();
                }
            }
        }
        let (mut wanted, mut wanted_lists) = (Vec::new(), Vec::new());
        fields(template, &mut Vec::new(), &mut wanted, &mut wanted_lists);
        let (mut offered, mut offered_lists) = (Vec::new(), Vec::new());
        fields(source, &mut Vec::new(), &mut offered, &mut offered_lists);

        let said_by_the_block: Vec<&str> = block.iter().map(|o| o.name.as_str()).collect();
        let said = |path: &str| said_by_the_block.contains(&path.split('/').next().unwrap_or(""));
        let mut values = std::collections::HashMap::new();
        for (path, field) in offered {
            if let Some(OverseerValue::String(key)) = field.parameters.get(TYPED_FROM) {
                note_emptied(key);
            }
            if said(&path) {
                continue;
            }
            let Some((_, into)) = wanted.iter().find(|(p, _)| *p == path) else { continue };
            if matches!(into.parameters.get("value"), Some(OverseerValue::Formula(_))) {
                continue;
            }
            let value = field
                .parameters
                .get("_computed_value")
                .or_else(|| field.parameters.get("value"));
            if let Some(v) = value.and_then(|v| converted(v, &into.node_type)) {
                values.insert(path, v);
            }
        }
        let lists = offered_lists
            .into_iter()
            .filter(|(path, list)| {
                !list.children.is_empty() && !said(path) && wanted_lists.iter().any(|(p, _)| p == path)
            })
            .map(|(path, list)| (path, list.clone()))
            .collect();
        (crate::app_api::entry_overrides(&values), lists)
    }

    /// Whether a line of an action's block gives a field exactly the value its template does.
    fn restates_the_template(
        line: &OverseerNode,
        template: &OverseerNode,
        owner_path: &[String],
        snapshot: &Vec<OverseerNode>,
    ) -> Result<bool, OverseerError> {
        let Some(value) = line.parameters.get("value") else { return Ok(false) };
        if !line.children.is_empty() || line.parameters.keys().any(|k| k != "value" && !k.starts_with('_')) {
            return Ok(false);
        }
        fn field<'a>(node: &'a OverseerNode, name: &str) -> Option<&'a OverseerNode> {
            node.children.iter().find(|c| c.name == name).or_else(|| {
                node.children
                    .iter()
                    .filter(|c| crate::addressing::is_wrapper(c))
                    .find_map(|c| field(c, name))
            })
        }
        let Some(default) = field(template, &line.name).and_then(|f| f.parameters.get("value")) else {
            return Ok(false);
        };
        if matches!(default, OverseerValue::Formula(_)) {
            return Ok(false);
        }
        Ok(Self::value_equals(&Self::evaluate_in_context(value, owner_path, snapshot)?, default))
    }

    /// Where a list a copy fills sits in the entry just made, by the path `copied_into` gave it,
    /// with unnamed wrappers looked through.
    fn list_in_entry(nodes: &Vec<OverseerNode>, entry: &[usize], path: &str) -> Option<Vec<usize>> {
        let mut at = entry.to_vec();
        for name in path.split('/') {
            loop {
                let node = Self::get_node_ref_by_indices(nodes, &at)?;
                if let Some(i) = node.children.iter().position(|c| c.name == name) {
                    at.push(i);
                    break;
                }
                let i = node
                    .children
                    .iter()
                    .position(|c| crate::addressing::is_wrapper(c) && Self::holds_named(c, name))?;
                at.push(i);
            }
        }
        Some(at)
    }

    fn append_to_list(
        nodes: &mut Vec<OverseerNode>,
        owner_path: &[String],
        list_path: &str,
        template_name: Option<&str>,
        value_opt: Option<OverseerValue>,
        overrides: &Vec<OverseerNode>,
    ) -> Result<(), OverseerError> {
        Self::append_to_list_from(nodes, owner_path, list_path, template_name, value_opt, overrides, None)
    }

    fn append_to_list_from(
        nodes: &mut Vec<OverseerNode>,
        owner_path: &[String],
        list_path: &str,
        template_name: Option<&str>,
        value_opt: Option<OverseerValue>,
        overrides: &Vec<OverseerNode>,
        from: Option<&OverseerValue>,
    ) -> Result<(), OverseerError> {
        Self::put_entry_in(nodes, owner_path, list_path, template_name, value_opt, overrides, from, WhereItGoes::Last)
    }

    fn prepend_to_list(
        nodes: &mut Vec<OverseerNode>,
        owner_path: &[String],
        list_path: &str,
        template_name: Option<&str>,
        value_opt: Option<OverseerValue>,
        overrides: &Vec<OverseerNode>,
        from: Option<&OverseerValue>,
    ) -> Result<(), OverseerError> {
        Self::put_entry_in(nodes, owner_path, list_path, template_name, value_opt, overrides, from, WhereItGoes::First)
    }

    /// Make an entry for a list and put it at one end - what `append` and `prepend` do.
    ///
    /// The entry is made from what the document says before anything is written: the list's
    /// template, what a copy from a form supplies, the overrides worked out in the owner's
    /// context. It used to be made against a copy of the whole document taken for the purpose, so
    /// that the list could be held open for writing meanwhile - over twenty milliseconds of every
    /// append, and so of every meal logged and every task done.
    #[allow(clippy::too_many_arguments)]
    fn put_entry_in(
        nodes: &mut Vec<OverseerNode>,
        owner_path: &[String],
        list_path: &str,
        template_name: Option<&str>,
        value_opt: Option<OverseerValue>,
        overrides: &Vec<OverseerNode>,
        from: Option<&OverseerValue>,
        goes: WhereItGoes,
    ) -> Result<(), OverseerError> {
        let (segments, _explicit_param, anchored) = Self::split_path_and_param(list_path);
        let indices = Self::resolve_target_indices(nodes, owner_path, anchored, &segments)
            .ok_or_else(|| {
                OverseerError::ValidationError(format!("List not found: {}", list_path))
            })?;
        let source = match from {
            Some(from) => Some(Self::source_of_a_copy(nodes, owner_path, from)?),
            None => None,
        };
        Self::put_entry_at(nodes, owner_path, indices, list_path, template_name, value_opt, overrides, source.as_ref(), goes)
    }

    /// Put an entry into the list at `indices`, filled from `source` when there is one.
    ///
    /// A copy's lists follow the entry in once it is in place, each of their entries put into the
    /// new entry's list of the same name the same way - made from that list's template and filled
    /// by name - so a list nested deeper still comes along too, and every one is said in the file
    /// as an entry appended to it would be.
    #[allow(clippy::too_many_arguments)]
    fn put_entry_at(
        nodes: &mut Vec<OverseerNode>,
        owner_path: &[String],
        indices: Vec<usize>,
        list_path: &str,
        template_name: Option<&str>,
        value_opt: Option<OverseerValue>,
        overrides: &Vec<OverseerNode>,
        source: Option<&OverseerNode>,
        goes: WhereItGoes,
    ) -> Result<(), OverseerError> {
        let verb = match goes {
            WhereItGoes::First => "prepend",
            WhereItGoes::Last => "append",
        };
        let mut lists_to_copy = Vec::new();
        // As the graph names the list - see `Shape`.
        let list_address = Self::build_disambiguated_path(nodes, &indices).join("/");

        let (new_item, style_guide) = {
            let document: &Vec<OverseerNode> = nodes;
            let list_node = Self::get_node_ref_by_indices(document, &indices).ok_or_else(|| {
                OverseerError::ValidationError(format!("List not found: {}", list_path))
            })?;
            if list_node.node_type != "list" {
                return Err(OverseerError::ValidationError(format!(
                    "{}.target is not a list",
                    verb
                )));
            }
            let style_guide = Self::derive_list_entry_style(list_node, goes == WhereItGoes::First);
            // A unique instance name (T__1, T__2) whichever end it goes, so no two siblings share
            // one. Its place settles its name afterwards - see `resolver::name_entries_as_parsed`.
            let ordinal = list_node.children.len() + 1;
            let mut new_item = match list_node.parameters.get("entry") {
                Some(OverseerValue::Template(t)) => {
                    // Choose template: explicit override or list entry template
                    let chosen_template_name = if let Some(name) = template_name {
                        name.to_string()
                    } else {
                        let raw = t.split('/').last().unwrap_or("");
                        let trimmed = raw.trim();
                        if trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.len() >= 2
                        {
                            trimmed[1..trimmed.len() - 1].to_string()
                        } else {
                            trimmed.to_string()
                        }
                    };
                    let template_def = Self::find_node_by_name(document, &chosen_template_name)
                        .ok_or_else(|| {
                            OverseerError::ValidationError(format!(
                                "Template not found: {}",
                                chosen_template_name
                            ))
                        })?;
                    let mut new_item = Self::clone_from_template(template_def);
                    // New item should adopt opposite of parent list effective layout
                    if let Some(OverseerValue::String(parent_eff)) =
                        list_node.parameters.get("_effective_layout")
                    {
                        let opp = Self::opposite_layout(parent_eff);
                        new_item
                            .parameters
                            .insert("_effective_layout".to_string(), OverseerValue::String(opp));
                    }
                    new_item.name = format!("{}__{}", template_def.name, ordinal);
                    // Apply evaluated overrides from action block, after whatever a copy supplies
                    let mut all = match source {
                        Some(source) => {
                            let (copied, lists) = Self::copied_into(source, template_def, overrides);
                            lists_to_copy = lists;
                            copied
                        }
                        None => Vec::new(),
                    };
                    for line in overrides {
                        // Beside a copy, a line of the block that gives the template's own value
                        // is there to keep the source's out - a reopened item comes back unrated -
                        // and the entry says nothing about the field, as one never rated would.
                        if source.is_some() && Self::restates_the_template(line, template_def, owner_path, document)? {
                            continue;
                        }
                        all.push(line.clone());
                    }
                    Self::apply_overrides_evaluated(&mut new_item, &all, owner_path, document)?;
                    new_item
                }
                Some(OverseerValue::String(type_name)) => {
                    // Simple type list requires a value
                    let val = value_opt.ok_or_else(|| {
                        OverseerError::ValidationError(format!(
                            "{}.value required for simple list",
                            verb
                        ))
                    })?;
                    let mut item = OverseerNode {
                        name: format!("{}__{}", type_name, ordinal),
                        node_type: type_name.clone(),
                        template: None,
                        parameters: Default::default(),
                        children: Vec::new(),
                        is_hierarchy_transparent: false,
                        param_order: Vec::new(),
                        raw_value_literal: None,
                        authored_dash: false,
                        child_original_index: None,
                        leading_blank_lines: 0,
                        source_snapshot: None,
                        source_id: None,
                        source_fingerprint: None,
                    };
                    item.parameters.insert("value".to_string(), val);
                    item
                }
                Some(_) => {
                    return Err(OverseerError::ValidationError(format!(
                        "{}: unsupported entry type",
                        verb
                    )))
                }
                None => {
                    return Err(OverseerError::ValidationError(format!(
                        "{}: list has no entry parameter",
                        verb
                    )))
                }
            };
            Self::apply_list_entry_style(&mut new_item, &style_guide);
            Self::mark_the_entry_as_new(&mut new_item);
            Self::refuse_a_key_already_held(list_node, &new_item, list_path, verb)?;
            (new_item, style_guide)
        };

        let made = new_item.name.clone();
        let list_node = Self::get_node_mut_by_indices(nodes, &indices).ok_or_else(|| {
            OverseerError::ValidationError(format!("List not found: {}", list_path))
        })?;
        let at = match goes {
            WhereItGoes::First => {
                list_node.children.insert(0, new_item);
                0
            }
            WhereItGoes::Last => {
                list_node.children.push(new_item);
                list_node.children.len() - 1
            }
        };
        Self::harmonize_list_entry_spacing(list_node, &style_guide);
        // Mark this list field as explicitly overridden so mutations persist on template instances
        Self::mark_field_explicit_override(nodes, &indices);
        // Shape, not value - see `Shape`. Unsaid, what reached the resolver looked like a change to
        // values: the graph was asked about a document whose entries had moved, and the survivors
        // kept names their text no longer gives them.
        note_shape(Shape::Added { list: list_address, entry: made });

        let mut entry = indices;
        entry.push(at);
        for (path, list) in lists_to_copy {
            let Some(into) = Self::list_in_entry(nodes, &entry, &path) else { continue };
            let into_path = format!("{}/{}", list_path, path);
            for copied in &list.children {
                // A list of plain values copies the values; one of entries copies each entry.
                let (value, from) = if copied.children.is_empty() && copied.parameters.contains_key("value") {
                    (copied.parameters.get("value").cloned(), None)
                } else {
                    (None, Some(copied))
                };
                Self::put_entry_at(nodes, owner_path, into.clone(), &into_path, None, value, &Vec::new(), from, WhereItGoes::Last)?;
            }
        }
        Ok(())
    }

    fn derive_list_entry_style(
        list_node: &OverseerNode,
        inserting_at_front: bool,
    ) -> ListEntryStyleGuide {
        // Deliberately not seeded from the list's own indentation. That is where the list
        // sits, and an entry sits one level inside it - so seeding from there wrote every new
        // entry at its list's depth. An entry lines up with the entries around it, and when
        // there are none it is left unsaid, so the serializer works it out from how deep the
        // node actually is.
        //
        // It went unnoticed for as long as it did because a list with entries already in it
        // supplies the right answer from the loop below, and because a second mechanism in the
        // serializer lines entries up with their neighbours after the fact. Neither helps the
        // first entry of a list nested inside a template entry - a day's notes, a reading - and
        // those came out at their parent list's depth, four levels wrong.
        let mut guide = ListEntryStyleGuide {
            leading_blank_lines: 1,
            newline: list_node
                .source_snapshot
                .as_ref()
                .and_then(|snap| snap.newline.clone()),
            indent_unit: None,
        };

        let iter: Box<dyn Iterator<Item = &OverseerNode>> = if inserting_at_front {
            Box::new(list_node.children.iter())
        } else {
            Box::new(list_node.children.iter().rev())
        };

        let mut fallback_blank_lines: Option<u8> = None;

        for child in iter {
            if guide.newline.is_none() {
                if let Some(snap) = child.source_snapshot.as_ref() {
                    if let Some(nl) = snap.newline.clone() {
                        guide.newline = Some(nl);
                    }
                }
            }
            if guide.indent_unit.is_none() {
                if let Some(snap) = child.source_snapshot.as_ref() {
                    if let Some(ind) = snap.indent_unit.clone() {
                        if !ind.is_empty() {
                            guide.indent_unit = Some(ind);
                        }
                    }
                }
            }
            if let Some(snapshot) = child.source_snapshot.as_ref() {
                if matches!(snapshot.origin, SnapshotOrigin::Parsed) {
                    guide.leading_blank_lines = child.leading_blank_lines.max(1);
                    return guide;
                }
            }
            if fallback_blank_lines.is_none() {
                fallback_blank_lines = Some(child.leading_blank_lines.max(1));
            }
        }

        if guide.leading_blank_lines == 1 {
            if let Some(lines) = fallback_blank_lines {
                guide.leading_blank_lines = lines;
            }
        }

        // No default here either. One indent unit is not an indentation, and stamping it on
        // the entry hid the depth the serializer would otherwise have computed correctly.
        guide
    }

    /// Say that this entry has just been created.
    ///
    /// A field can fall back to a figure kept elsewhere - a day's calorie target to the standing
    /// one - and then it reads that figure for ever, which is wrong for anything past: the
    /// history ends up claiming every day was aiming at whatever the standing figure is today.
    /// What such a day wants is the figure as it stood when the day began, and no formula can
    /// say that, because a formula is re-read every time it is looked at.
    ///
    /// `freeze=true` on a field says to write the value in instead, once, when the entry is
    /// created. Only the creating knows when that is, so it says so here and the resolver acts
    /// on it - which has to be that way round, because at this moment the entry holds only what
    /// it was given. Everything the template supplies, the field in question included, is
    /// materialised later while the document is worked out.
    ///
    /// Left on rather than taken off. It is an internal marker, so it is never written to the
    /// file and is gone the next time the document is read; and within this process it cannot
    /// fire twice, because a field that has been frozen states a value and a field that states
    /// a value is not frozen again.
    fn mark_the_entry_as_new(node: &mut OverseerNode) {
        node.parameters
            .insert("_freeze_pending".to_string(), OverseerValue::Boolean(true));
    }

    fn apply_list_entry_style(node: &mut OverseerNode, style: &ListEntryStyleGuide) {
        node.leading_blank_lines = style.leading_blank_lines;
        // The node's own copy from here on, since the one it has may be shared.
        match node.source_snapshot.as_mut().map(std::sync::Arc::make_mut) {
            Some(snapshot) => {
                if let Some(indent) = style.indent_unit.clone() {
                    snapshot.indent_unit = Some(indent);
                }
                if snapshot.newline.is_none() {
                    snapshot.newline = style.newline.clone();
                }
            }
            None => {
                if style.indent_unit.is_some() || style.newline.is_some() {
                    node.synthesize_snapshot_with_style(
                        style.indent_unit.clone(),
                        style.newline.clone(),
                    );
                }
            }
        }
    }

    fn harmonize_list_entry_spacing(list_node: &mut OverseerNode, style: &ListEntryStyleGuide) {
        if list_node.children.len() <= 1 {
            return;
        }
        let desired = style.leading_blank_lines.max(1);
        if desired <= 1 {
            return;
        }
        for child in list_node.children.iter_mut().skip(1) {
            if child.leading_blank_lines < desired {
                child.leading_blank_lines = desired;
            }
        }
    }

    /// Mark a field (at indices) as explicitly overridden on its parent so serializer/resolver persist mutations.
    fn mark_field_explicit_override(nodes: &mut Vec<OverseerNode>, indices: &[usize]) {
        if indices.is_empty() {
            return;
        }
        // Mark child itself
        if let Some(child) = Self::get_node_mut_by_indices(nodes, indices) {
            child.parameters.insert(
                "_override_present".to_string(),
                OverseerValue::Boolean(true),
            );
            child.parameters.insert(
                "_explicit_child_override".to_string(),
                OverseerValue::Boolean(true),
            );
        }
        // Mark on parent list of explicit override names
        if indices.len() >= 2 {
            let parent_path = &indices[..indices.len() - 1];
            if let Some(parent) = Self::get_node_mut_by_indices(nodes, parent_path) {
                // Child name
                let child_name = if let Some(ch) = parent.children.get(indices[indices.len() - 1]) {
                    ch.name.clone()
                } else {
                    String::new()
                };
                let entry = parent
                    .parameters
                    .entry("_explicit_overrides".to_string())
                    .or_insert(OverseerValue::String(String::new()));
                if let OverseerValue::String(s) = entry {
                    if !s.split(',').any(|n| n == child_name) {
                        if !s.is_empty() {
                            s.push(',');
                        }
                        s.push_str(&child_name);
                    }
                }
            }
        }
        Self::mark_named_divs_up_to_the_entry(nodes, indices);
    }

    /// A list entry is written as what it overrides, and a named div inside it that it does not
    /// override is left out whole - with whatever was just changed inside it. A comment appended
    /// to `Items/[x]/Talk/Comments` changed the list in memory and reached the file nowhere,
    /// answering that nothing was written. So the named divs between a changed node and the entry
    /// holding it are marked as overridden too, the way a block written by hand reads, and the
    /// entry is written `div Talk { list Comments { ... } }`, holding only what changed. A div
    /// without a name is no step of any address and is written through, so it is left alone.
    fn mark_named_divs_up_to_the_entry(nodes: &mut Vec<OverseerNode>, indices: &[usize]) {
        // The entry: the nearest ancestor whose parent is a list.
        let Some(entry_depth) = (2..=indices.len()).rev().find(|&depth| {
            Self::get_node_ref_by_indices(nodes, &indices[..depth - 1]).is_some_and(|parent| parent.node_type == "list")
        }) else {
            return;
        };
        for depth in entry_depth + 1..indices.len() {
            let at = &indices[..depth];
            let unmarked_named_div = Self::get_node_ref_by_indices(nodes, at).is_some_and(|node| {
                node.node_type == "div"
                    && !node.name.is_empty()
                    && !crate::addressing::is_wrapper(node)
                    && !matches!(node.parameters.get("_explicit_child_override"), Some(OverseerValue::Boolean(true)))
            });
            if unmarked_named_div {
                Self::mark_field_explicit_override(nodes, at);
            }
        }
    }

    // Evaluate formulas in value/params against owner_path context and apply into target
    /// Whether a node holds a child of this name, looking through layout-only groups.
    fn holds_named(node: &OverseerNode, name: &str) -> bool {
        node.children.iter().any(|c| {
            c.name == name || (crate::addressing::is_wrapper(c) && Self::holds_named(c, name))
        })
    }

    fn apply_overrides_evaluated(
        target: &mut OverseerNode,
        overrides: &Vec<OverseerNode>,
        owner_path: &[String],
        snapshot: &Vec<OverseerNode>,
    ) -> Result<(), OverseerError> {
        for ov in overrides {
            let name = ov.name.clone();
            // Find or create corresponding child in target
            let mut idx_opt = target.children.iter().position(|c| c.name == name);

            // Not beside the group, then: a template is free to keep a field inside one for
            // layout, and an override names the field rather than the arrangement. Creating a
            // second field beside the group instead would leave the real one at its default,
            // with every formula reading that one and the written value showing nowhere.
            if idx_opt.is_none() {
                let inside = target.children.iter().position(|c| {
                    crate::addressing::is_wrapper(c) && Self::holds_named(c, &name)
                });
                if let Some(wrapper_index) = inside {
                    let wrapper = &mut target.children[wrapper_index];
                    Self::apply_overrides_evaluated(
                        wrapper,
                        &vec![ov.clone()],
                        owner_path,
                        snapshot,
                    )?;
                    continue;
                }
            }
            let _ = &mut idx_opt;
            if let Some(idx) = idx_opt {
                // Merge parameters (evaluate any Formula)
                let child = target.children.get_mut(idx).unwrap();
                // Prune equal-to-template overrides for simple value equality
                // If override sets only 'value' and equals the template's current child value, skip marking override
                let only_value_override =
                    ov.parameters.len() == 1 && ov.parameters.contains_key("value");
                let mut applied_any = false;
                for (k, v) in ov.parameters.iter() {
                    // Special case: list-to-list copy without explicit loops, e.g. "- intake = $(../intake)"
                    // If the target field is a list and the override provides a formula path, resolve it to a source list
                    // and deep-copy its children into the target list.
                    if child.node_type == "list" && k == "value" {
                        if let OverseerValue::Formula(expr) = v {
                            let path_str = expr.trim();
                            let (segments, _explicit_param, anchored) =
                                Self::split_path_and_param(path_str);
                            if let Some(indices) = Self::resolve_target_indices(
                                snapshot, owner_path, anchored, &segments,
                            ) {
                                if let Some(src) = Self::get_node_ref_by_indices(snapshot, &indices)
                                {
                                    if src.node_type == "list" {
                                        // Deep copy list items
                                        child.children = src.children.clone();
                                        // Ensure any lingering 'value' param on list is cleared; list value is its children
                                        child.parameters.remove("value");
                                        applied_any = true;
                                        // Skip normal value assignment for this key
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                    let eval = Self::evaluate_in_context(v, owner_path, snapshot)?;
                    if k == "value" && only_value_override {
                        if let Some(template_val) = child.parameters.get("value") {
                            if Self::value_equals(&eval, template_val) {
                                // Skip applying equal override
                                continue;
                            }
                        }
                    }
                    child.parameters.insert(k.clone(), eval);
                    applied_any = true;
                }
                if applied_any {
                    // Mark explicit override and clear template marker for value, if present
                    child.source_fingerprint = None;
                    child.parameters.insert(
                        "_override_present".to_string(),
                        OverseerValue::Boolean(true),
                    );
                    child.parameters.insert(
                        "_explicit_child_override".to_string(),
                        OverseerValue::Boolean(true),
                    );
                    child.parameters.remove("_template_value");
                    // Treat explicit runtime override as dash-authored for serializer stylistic reproduction
                    child.authored_dash = true;
                    // Track at parent level
                    let entry = target
                        .parameters
                        .entry("_explicit_overrides".to_string())
                        .or_insert(OverseerValue::String(String::new()));
                    if let OverseerValue::String(s) = entry {
                        if !s.split(',').any(|n| n == name) {
                            if !s.is_empty() {
                                s.push(',');
                            }
                            s.push_str(&name);
                        }
                    }
                }
                // Recurse into children overrides
                if !ov.children.is_empty() {
                    Self::apply_overrides_evaluated(child, &ov.children, owner_path, snapshot)?;
                    // If any descendant was explicitly overridden, ensure the container child itself is marked
                    if Self::has_explicit_override_descendant(child) {
                        child.source_fingerprint = None;
                        child.parameters.insert(
                            "_override_present".to_string(),
                            OverseerValue::Boolean(true),
                        );
                        child.parameters.insert(
                            "_explicit_child_override".to_string(),
                            OverseerValue::Boolean(true),
                        );
                        child.authored_dash = true;
                        // Track at parent level so serializers can include this container
                        let entry = target
                            .parameters
                            .entry("_explicit_overrides".to_string())
                            .or_insert(OverseerValue::String(String::new()));
                        if let OverseerValue::String(s) = entry {
                            if !s.split(',').any(|n| n == child.name) {
                                if !s.is_empty() {
                                    s.push(',');
                                }
                                s.push_str(&child.name);
                            }
                        }
                    }
                }
            } else {
                // Create new child with inferred type from override/value
                let mut new_child = OverseerNode {
                    name: name.clone(),
                    node_type: ov.node_type.clone(),
                    template: None,
                    parameters: Default::default(),
                    children: Vec::new(),
                    is_hierarchy_transparent: false,
                    param_order: Vec::new(),
                    raw_value_literal: None,
                    authored_dash: true,
                    child_original_index: None,
                    leading_blank_lines: 0,
                    source_snapshot: None,
                    source_id: None,
                    source_fingerprint: None,
                };
                // Copy/evaluate params
                for (k, v) in ov.parameters.iter() {
                    // Special case: creating a list field via value=$(../list) should deep-copy children
                    if k == "value" && ov.node_type == "list" {
                        if let OverseerValue::Formula(expr) = v {
                            let path_str = expr.trim();
                            let (segments, _explicit_param, anchored) =
                                Self::split_path_and_param(path_str);
                            if let Some(indices) = Self::resolve_target_indices(
                                snapshot, owner_path, anchored, &segments,
                            ) {
                                if let Some(src) = Self::get_node_ref_by_indices(snapshot, &indices)
                                {
                                    if src.node_type == "list" {
                                        new_child.children = src.children.clone();
                                        // Explicit list override
                                        new_child.parameters.insert(
                                            "_override_present".to_string(),
                                            OverseerValue::Boolean(true),
                                        );
                                        new_child.parameters.insert(
                                            "_explicit_child_override".to_string(),
                                            OverseerValue::Boolean(true),
                                        );
                                        continue; // don't set a scalar 'value' on the list
                                    }
                                }
                            }
                        }
                    }
                    let eval = Self::evaluate_in_context(v, owner_path, snapshot)?;
                    new_child.parameters.insert(k.clone(), eval);
                }
                new_child.parameters.insert(
                    "_override_present".to_string(),
                    OverseerValue::Boolean(true),
                );
                new_child.parameters.insert(
                    "_explicit_child_override".to_string(),
                    OverseerValue::Boolean(true),
                );
                new_child.parameters.remove("_template_value");
                new_child.authored_dash = true;
                // Recurse
                if !ov.children.is_empty() {
                    Self::apply_overrides_evaluated(
                        &mut new_child,
                        &ov.children,
                        owner_path,
                        snapshot,
                    )?;
                }
                // Infer node type from evaluated value if empty
                if new_child.node_type.is_empty() {
                    if let Some(val) = new_child.parameters.get("value") {
                        new_child.node_type = match val {
                            OverseerValue::Integer(_) => "int".to_string(),
                            OverseerValue::Float(_) => "float".to_string(),
                            OverseerValue::Boolean(_) => "bool".to_string(),
                            OverseerValue::String(_) => "string".to_string(),
                            OverseerValue::Date(_) => "date".to_string(),
                            OverseerValue::Timestamp(_) => "timestamp".to_string(),
                            _ => new_child.node_type.clone(),
                        };
                    }
                }
                if new_child.source_snapshot.is_none() {
                    let (indent_unit, newline) = target
                        .source_snapshot
                        .as_ref()
                        .map(|snap| (snap.indent_unit.clone(), snap.newline.clone()))
                        .unwrap_or((None, None));
                    new_child.synthesize_snapshot_with_style(indent_unit.clone(), newline.clone());
                    for child in new_child.children.iter_mut() {
                        if child.source_snapshot.is_none() {
                            child.synthesize_snapshot_with_style_recursive(
                                indent_unit.clone(),
                                newline.clone(),
                            );
                        }
                    }
                }
                target.children.push(new_child);
                // Track at parent level
                let entry = target
                    .parameters
                    .entry("_explicit_overrides".to_string())
                    .or_insert(OverseerValue::String(String::new()));
                if let OverseerValue::String(s) = entry {
                    if !s.split(',').any(|n| n == name) {
                        if !s.is_empty() {
                            s.push(',');
                        }
                        s.push_str(&name);
                    }
                }
            }
        }
        Ok(())
    }

    // Detect whether any descendant node in this subtree carries an explicit override marker
    fn has_explicit_override_descendant(node: &OverseerNode) -> bool {
        for ch in &node.children {
            if matches!(
                ch.parameters.get("_explicit_child_override"),
                Some(OverseerValue::Boolean(true))
            ) {
                return true;
            }
            if Self::has_explicit_override_descendant(ch) {
                return true;
            }
        }
        false
    }

    fn evaluate_in_context(
        val: &OverseerValue,
        owner_path: &[String],
        snapshot: &Vec<OverseerNode>,
    ) -> Result<OverseerValue, OverseerError> {
        match val {
            OverseerValue::Formula(expr) => {
                let ctx = EvaluationContext::new(owner_path.to_vec(), snapshot);
                let evaluated = FormulaEvaluator::evaluate_formula(expr, &ctx)?;
                Ok(evaluated)
            }
            other => Ok(other.clone()),
        }
    }

    fn move_in_list(
        nodes: &mut Vec<OverseerNode>,
        owner_path: &[String],
        from_path: &str,
        to_path: &str,
        key_field: &str,
        key_value: &OverseerValue,
        at_index: Option<usize>,
    ) -> Result<(), OverseerError> {
        // An entry moved is a change of shape - see `note_structural`.
        note_structural();
        let (from_segments, _p1, from_anchored) = Self::split_path_and_param(from_path);
        let (to_segments, _p2, to_anchored) = Self::split_path_and_param(to_path);
        let snapshot = nodes.clone();
        let from_indices =
            Self::resolve_target_indices(&snapshot, owner_path, from_anchored, &from_segments)
                .ok_or_else(|| {
                    OverseerError::ValidationError(format!("From list not found: {}", from_path))
                })?;
        let to_indices =
            Self::resolve_target_indices(&snapshot, owner_path, to_anchored, &to_segments)
                .ok_or_else(|| {
                    OverseerError::ValidationError(format!("To list not found: {}", to_path))
                })?;
        let same_list = from_indices == to_indices;

        // Borrow lists mutably via indices carefully
        if same_list {
            let list_node =
                Self::get_node_mut_by_indices(nodes, &from_indices).ok_or_else(|| {
                    OverseerError::ValidationError(format!("List not found: {}", from_path))
                })?;
            if list_node.node_type != "list" {
                return Err(OverseerError::ValidationError(
                    "move.target is not a list".to_string(),
                ));
            }
            let effective_key_field = if !key_field.is_empty() {
                key_field.to_string()
            } else if let Some(OverseerValue::String(s)) = list_node.parameters.get("key") {
                s.clone()
            } else {
                return Err(OverseerError::ValidationError(
                    "move.keyField missing and list has no key".to_string(),
                ));
            };
            if let Some(pos) = Self::the_one_holding(list_node, &effective_key_field, key_value, from_path, "move", |v| {
                Self::value_equals(v, key_value)
            })? {
                let item = list_node.children.remove(pos);
                let insert_at = at_index.unwrap_or(list_node.children.len());
                let idx = if insert_at > list_node.children.len() {
                    list_node.children.len()
                } else {
                    insert_at
                };
                list_node.children.insert(idx, item);
                // As in `remove_from_list`: every entry still matches the text it was read
                // from, so unless the list says it no longer does, the serializer replays it
                // in its old order.
                list_node.source_fingerprint = None;
                Self::mark_field_explicit_override(nodes, &from_indices);
            }
        } else {
            // Different lists: remove from source, push/insert into dest
            let item_opt = {
                let from_node =
                    Self::get_node_mut_by_indices(nodes, &from_indices).ok_or_else(|| {
                        OverseerError::ValidationError(format!(
                            "From list not found: {}",
                            from_path
                        ))
                    })?;
                if from_node.node_type != "list" {
                    return Err(OverseerError::ValidationError(
                        "move.from is not a list".to_string(),
                    ));
                }
                let effective_key_field = if !key_field.is_empty() {
                    key_field.to_string()
                } else if let Some(OverseerValue::String(s)) = from_node.parameters.get("key") {
                    s.clone()
                } else {
                    return Err(OverseerError::ValidationError(
                        "move.keyField missing and list has no key".to_string(),
                    ));
                };
                if let Some(pos) = Self::the_one_holding(from_node, &effective_key_field, key_value, from_path, "move", |v| {
                    Self::value_equals(v, key_value)
                })? {
                    Some(from_node.children.remove(pos))
                } else {
                    None
                }
            };
            if let Some(item) = item_opt {
                let to_node =
                    Self::get_node_mut_by_indices(nodes, &to_indices).ok_or_else(|| {
                        OverseerError::ValidationError(format!("To list not found: {}", to_path))
                    })?;
                if to_node.node_type != "list" {
                    return Err(OverseerError::ValidationError(
                        "move.to is not a list".to_string(),
                    ));
                }
                // Already taken out of the list it came from; a refused press writes nothing, so
                // that is undone with the rest of it.
                Self::refuse_a_key_already_held(to_node, &item, to_path, "move")?;
                let insert_at = at_index.unwrap_or(to_node.children.len());
                let idx = if insert_at > to_node.children.len() {
                    to_node.children.len()
                } else {
                    insert_at
                };
                to_node.children.insert(idx, item);
                // Neither list matches its text any more - see the reorder above. The one
                // the entry left would otherwise be written back with it still inside.
                to_node.source_fingerprint = None;
                if let Some(from_node) = Self::get_node_mut_by_indices(nodes, &from_indices) {
                    from_node.source_fingerprint = None;
                }
                Self::mark_field_explicit_override(nodes, &from_indices);
                Self::mark_field_explicit_override(nodes, &to_indices);
            }
        }
        Ok(())
    }

    fn sort_list(
        nodes: &mut Vec<OverseerNode>,
        owner_path: &[String],
        list_path: &str,
        by_expr: &str,
        order: &str,
        stable: bool,
    ) -> Result<(), OverseerError> {
        // Shape, not value: what this does moves the addresses of everything after
        // it, so nothing the graph knows survives it.
        note_structural();
        let (segments, _explicit_param, anchored) = Self::split_path_and_param(list_path);
        let snapshot = nodes.clone();
        let indices = Self::resolve_target_indices(&snapshot, owner_path, anchored, &segments)
            .ok_or_else(|| {
                OverseerError::ValidationError(format!("List not found: {}", list_path))
            })?;
        let list_node = Self::get_node_mut_by_indices(nodes, &indices).ok_or_else(|| {
            OverseerError::ValidationError(format!("List not found: {}", list_path))
        })?;
        if list_node.node_type != "list" {
            return Err(OverseerError::ValidationError(
                "sort.target is not a list".to_string(),
            ));
        }

        // Prepare evaluation context base for by_expr; we'll bind 'x' to each item
        // For current_node in base context, use the list node snapshot for relative paths
        let list_snapshot = &snapshot[indices[0]]; // root of path's first segment
                                                   // Re-traverse to get the exact list snapshot node reference for context
        let mut cur: &OverseerNode = list_snapshot;
        for idx in &indices[1..] {
            cur = &cur.children[*idx];
        }

        // Every key is worked out before the children are taken, so a refused sort leaves the
        // list as it was. A key that fails is refused rather than sorted as empty: a field left
        // out of an entry reads as its template's default, so a failure means the expression is
        // not a path in the language or the list holds entries of different shapes - a mistake
        // in the document that would otherwise answer ok and sort nothing.
        let mut keys: Vec<OverseerValue> = Vec::with_capacity(cur.children.len());
        for (i, child) in cur.children.iter().enumerate() {
            let base_ctx = EvaluationContext {
                current_node: cur,
                parent_node: None,
                document_root: &snapshot,
                node_path: owner_path.to_vec(),
                var_bindings: std::collections::HashMap::new(),
            };
            let ctx = base_ctx.with_var("x", BoundValue::Node(child));
            let key = FormulaEvaluator::evaluate_formula(by_expr, &ctx).map_err(|e| {
                OverseerError::ValidationError(format!(
                    "sort.by could not be worked out for entry {}: {}",
                    i, e
                ))
            })?;
            keys.push(key);
        }

        let taken = std::mem::take(&mut list_node.children);
        let mut entries: Vec<(OverseerValue, OverseerNode, usize)> = keys
            .into_iter()
            .zip(taken)
            .enumerate()
            .map(|(i, (key, item))| (key, item, i))
            .collect();

        // Choose comparator
        let cmp = |a: &OverseerValue, b: &OverseerValue| Self::compare_overseer_values(a, b);
        if stable {
            if order == "desc" {
                entries.sort_by(|(ka, _ia, _xa), (kb, _ib, _xb)| cmp(kb, ka));
            } else {
                entries.sort_by(|(ka, _ia, _xa), (kb, _ib, _xb)| cmp(ka, kb));
            }
        } else {
            if order == "desc" {
                entries.sort_unstable_by(|(ka, _ia, _xa), (kb, _ib, _xb)| cmp(kb, ka));
            } else {
                entries.sort_unstable_by(|(ka, _ia, _xa), (kb, _ib, _xb)| cmp(ka, kb));
            }
        }

        list_node.children = entries.into_iter().map(|(_k, item, _i)| item).collect();
        // The same entries in a new order still each match their text - see `move_in_list`.
        list_node.source_fingerprint = None;
        Self::mark_field_explicit_override(nodes, &indices);
        Ok(())
    }

    fn compare_overseer_values(a: &OverseerValue, b: &OverseerValue) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (a, b) {
            (OverseerValue::Null, OverseerValue::Null) => Ordering::Equal,
            (OverseerValue::Null, other) => Self::value_to_string(other).cmp(&"".to_string()),
            (other, OverseerValue::Null) => "".to_string().cmp(&Self::value_to_string(other)),
            (OverseerValue::Integer(x), OverseerValue::Integer(y)) => x.cmp(y),
            (OverseerValue::Float(x), OverseerValue::Float(y)) => {
                x.partial_cmp(y).unwrap_or(Ordering::Equal)
            }
            (OverseerValue::Integer(x), OverseerValue::Float(y)) => {
                (*x as f64).partial_cmp(y).unwrap_or(Ordering::Equal)
            }
            (OverseerValue::Float(x), OverseerValue::Integer(y)) => {
                x.partial_cmp(&(*y as f64)).unwrap_or(Ordering::Equal)
            }
            (OverseerValue::String(x), OverseerValue::String(y)) => x.cmp(y),
            (OverseerValue::Boolean(x), OverseerValue::Boolean(y)) => x.cmp(y),
            (OverseerValue::Date(x), OverseerValue::Date(y)) => x.cmp(y), // ISO-8601 strings are lex comparable
            // Mixed types: compare their string representations
            _ => Self::value_to_string(a).cmp(&Self::value_to_string(b)),
        }
    }

    fn value_to_string(v: &OverseerValue) -> String {
        match v {
            OverseerValue::Null => "".to_string(),
            OverseerValue::Integer(i) => i.to_string(),
            OverseerValue::Float(f) => f.to_string(),
            OverseerValue::String(s) => s.clone(),
            OverseerValue::Boolean(b) => b.to_string(),
            OverseerValue::Date(d) => d.clone(),
            OverseerValue::Timestamp(ts) => ts.clone(),
            OverseerValue::Color(c) => format!("{:?}", c),
            OverseerValue::CssSize(s) => format!("{:?}", s),
            OverseerValue::BorderStyle(s) => format!("{:?}", s),
            OverseerValue::Formula(s) => s.clone(),
            OverseerValue::Template(s) => s.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_document;
    use crate::resolver::resolve_document;

    #[test]
    fn test_mount_load_same_document_path() {
        let input = r#"
        div Root {
            div Target { string v = "hello" }
            mount M (source="/Root/Target") { }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        // Initial resolve populates mount defaults
        resolve_document(&mut nodes);
        // Fire implicit load on mount M
        let path = vec!["Root".to_string(), "M".to_string()];
        let res = ActionExecutor::execute_event(&mut nodes, &path, "load");
        assert!(res.is_ok());
        // Verify mount now has embedded Target and status loaded
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let m = root.children.iter().find(|c| c.name == "M").unwrap();
        assert_eq!(m.node_type, "mount");
        assert_eq!(
            m.parameters.get("_mount_status"),
            Some(&OverseerValue::String("loaded".to_string()))
        );
        assert_eq!(m.children.len(), 1);
        let embedded = &m.children[0];
        assert_eq!(embedded.name, "Target");
        let v = embedded.children.iter().find(|c| c.name == "v").unwrap();
        assert_eq!(
            v.parameters.get("value"),
            Some(&OverseerValue::String("hello".to_string()))
        );
    }

    #[test]
    fn test_mount_load_and_unload_external_file() {
        // Create a temporary external .os file with a simple structure
        let temp_dir = std::env::temp_dir();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let file_path = temp_dir.join(format!("overseer_mount_test_{}.os", ts));
        let file_path_str = file_path.to_string_lossy().to_string();
        let ext_content = r#"
div Ext {
  div Data { string v = "hi" }
}
"#;
        std::fs::write(&file_path, ext_content).expect("write temp file");

        // Mount pointing to external file path + internal path
        let source_str = format!("{}/Ext/Data", file_path_str);
        let doc = format!("div Root {{ mount M (source=\"{}\") {{ }} }}", source_str);
        let mut nodes = parse_document(&doc).unwrap().1;
        resolve_document(&mut nodes);
        let path = vec!["Root".to_string(), "M".to_string()];
        let res = ActionExecutor::execute_event(&mut nodes, &path, "load");
        assert!(res.is_ok());
        // Verify loaded
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let m = root.children.iter().find(|c| c.name == "M").unwrap();
        assert_eq!(
            m.parameters.get("_mount_status"),
            Some(&OverseerValue::String("loaded".to_string()))
        );
        assert_eq!(m.children.len(), 1);
        assert_eq!(m.children[0].name, "Data");
        let v = m.children[0]
            .children
            .iter()
            .find(|c| c.name == "v")
            .unwrap();
        assert_eq!(
            v.parameters.get("value"),
            Some(&OverseerValue::String("hi".to_string()))
        );

        // Now unload
        let res2 = ActionExecutor::execute_event(&mut nodes, &path, "unload");
        assert!(res2.is_ok());
        let root2 = nodes.iter().find(|n| n.name == "Root").unwrap();
        let m2 = root2.children.iter().find(|c| c.name == "M").unwrap();
        assert_eq!(
            m2.parameters.get("_mount_status"),
            Some(&OverseerValue::String("unloaded".to_string()))
        );
        assert!(m2.children.is_empty());

        // Cleanup
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_mount_load_missing_file_sets_error_status() {
        // Point to a non-existent file; expect _mount_status = error and _mount_error populated
        let bogus = format!(
            "{}\\\\no_such_dir\\\\no_such_file_{}.os",
            std::env::temp_dir().to_string_lossy(),
            123456789
        );
        let source_str = format!("{}/Ext", bogus);
        let doc = format!("div Root {{ mount M (source=\"{}\") {{ }} }}", source_str);
        let mut nodes = parse_document(&doc).unwrap().1;
        resolve_document(&mut nodes);
        let res =
            ActionExecutor::execute_event(&mut nodes, &vec!["Root".into(), "M".into()], "load");
        assert!(res.is_ok());
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let m = root.children.iter().find(|c| c.name == "M").unwrap();
        assert_eq!(
            m.parameters.get("_mount_status"),
            Some(&OverseerValue::String("error".to_string()))
        );
        let err = m.parameters.get("_mount_error");
        assert!(
            matches!(err, Some(OverseerValue::String(s)) if s.contains("Failed to read mount file"))
        );
    }

    #[test]
    fn test_mount_load_bad_internal_path_sets_error_status() {
        // Create valid temp file but request a bad internal path
        let file_path = std::env::temp_dir().join("overseer_mount_tmp_ok.os");
        let content = "div A { div B { } }";
        std::fs::write(&file_path, content).unwrap();
        let source_str = format!("{}/A/NOPE", file_path.to_string_lossy());
        let doc = format!("div Root {{ mount M (source=\"{}\") {{ }} }}", source_str);
        let mut nodes = parse_document(&doc).unwrap().1;
        resolve_document(&mut nodes);
        let res =
            ActionExecutor::execute_event(&mut nodes, &vec!["Root".into(), "M".into()], "load");
        assert!(res.is_ok());
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let m = root.children.iter().find(|c| c.name == "M").unwrap();
        assert_eq!(
            m.parameters.get("_mount_status"),
            Some(&OverseerValue::String("error".to_string()))
        );
        let err = m.parameters.get("_mount_error");
        assert!(
            matches!(err, Some(OverseerValue::String(s)) if s.contains("segment not found") || s.contains("root not found"))
        );
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_toggle_under_transparent_wrapper_persists() {
        // Template with a transparent unnamed div containing a bool field; list of instances and a button to toggle.
        let script = r#"
        div R {
            div Item { div { bool flag = false } }
            list L (entry=<Item>) { - { } }
            button B { on click { toggle(path="/R/L/Item/flag") } }
        }
        "#;
        let mut nodes = parse_document(script).unwrap().1;
        resolve_document(&mut nodes);
        // Fire click on B -> should toggle the first item's flag to true and persist as explicit override
        let path_btn = vec!["R".into(), "B".into()];
        let res = ActionExecutor::execute_event(&mut nodes, &path_btn, "click");
        assert!(res.is_ok());
        // Verify now true and marked as override
        // Find node (transparency-aware and tolerant of instance suffixes like Item__1)
        fn find<'a>(nodes: &'a [OverseerNode], path: &[&str]) -> Option<&'a OverseerNode> {
            fn eff_name(n: &OverseerNode) -> &str {
                if !n.name.is_empty() {
                    &n.name
                } else {
                    &n.node_type
                }
            }
            fn matches_base(effective: &str, base: &str) -> bool {
                effective == base || effective.starts_with(&format!("{}__", base))
            }
            fn find_from<'a>(node: &'a OverseerNode, segs: &[&str]) -> Option<&'a OverseerNode> {
                if segs.is_empty() {
                    return Some(node);
                }
                let base = segs[0];
                // Try direct children with base-name matching
                for ch in &node.children {
                    if matches_base(eff_name(ch), base) {
                        return find_from(ch, &segs[1..]);
                    }
                }
                // Traverse through transparent wrappers without consuming the segment
                for ch in &node.children {
                    if ch.is_hierarchy_transparent {
                        if let Some(found) = find_from(ch, segs) {
                            return Some(found);
                        }
                    }
                }
                None
            }
            if path.is_empty() {
                return None;
            }
            // Start from roots
            for n in nodes {
                if matches_base(eff_name(n), path[0]) {
                    if let Some(found) = find_from(n, &path[1..]) {
                        return Some(found);
                    }
                }
            }
            None
        }
        let flag = find(&nodes, &["R", "L", "Item", "flag"]).unwrap();
        assert_eq!(
            flag.parameters.get("value"),
            Some(&OverseerValue::Boolean(true))
        );
        assert_eq!(
            flag.parameters.get("_explicit_child_override"),
            Some(&OverseerValue::Boolean(true))
        );
        // Round-trip through serialization + parse + resolve and ensure value stays true
        let ser = crate::file_ops::OverseerFileHandler::serialize_nodes(&nodes).unwrap();
        let (_rem, mut n3) = parse_document(&ser).unwrap();
        resolve_document(&mut n3);
        let flag2 = find(&n3, &["R", "L", "Item", "flag"]).unwrap();
        assert_eq!(
            flag2.parameters.get("value"),
            Some(&OverseerValue::Boolean(true))
        );
    }
    #[test]
    fn test_inc_action_on_click() {
        let input = r#"
        div Root {
            int counter = 1
            button Increment {
                on click { inc(path="../counter", by=2) }
            }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolver::resolve_document(&mut nodes);

        let path = vec!["Root".to_string(), "Increment".to_string()];

        let res = ActionExecutor::execute_event(&mut nodes, &path, "click");
        assert!(res.is_ok());

        // Verify counter incremented to 3
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let counter = root.children.iter().find(|c| c.name == "counter").unwrap();
        assert_eq!(
            counter.parameters.get("value"),
            Some(&OverseerValue::Integer(3))
        );
    }

    #[test]
    fn test_toggle_action() {
        let input = r#"
        div Root {
            bool done = false
            button Toggle {
                on click { toggle(path="../done") }
            }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolver::resolve_document(&mut nodes);
        let path = vec!["Root".to_string(), "Toggle".to_string()];
        let res = ActionExecutor::execute_event(&mut nodes, &path, "click");
        assert!(res.is_ok());
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let done = root.children.iter().find(|c| c.name == "done").unwrap();
        assert_eq!(
            done.parameters.get("value"),
            Some(&OverseerValue::Boolean(true))
        );
    }

    #[test]
    fn test_ensure_in_list_creates_item() {
        let input = r#"
        div Root {
            div Task {
                string id = ""
                string title = ""
            }
            list Tasks (entry=<Task>, key="id") {
            }
            button Add {
                on click { ensure_in_list(list="/Root/Tasks", keyField="id", keyValue="a1", template="<Task>") }
            }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolver::resolve_document(&mut nodes);
        let path = vec!["Root".to_string(), "Add".to_string()];
        let res = ActionExecutor::execute_event(&mut nodes, &path, "click");
        assert!(res.is_ok());
        // Verify a child exists with id == "a1"
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let list = root.children.iter().find(|c| c.name == "Tasks").unwrap();
        assert_eq!(list.node_type, "list");
        assert!(list
            .children
            .iter()
            .any(|it| it.children.iter().any(|f| f.name == "id"
                && f.parameters.get("value") == Some(&OverseerValue::String("a1".to_string())))));
    }

    #[test]
    fn test_ensure_in_list_with_key_precision_day() {
        // Ensure that two timestamps on the same day are treated equal when keyPrecision="day"
        let input = r#"
    div Record (hidden=true) {
            timestamp date = "2024-08-12T10:53:02+00:00"
        }
        list History (entry=<Record>, key="date", keyPrecision="day") {}
        div Owner {
            on mount { ensure_in_list(list="/History", keyField="date", keyValue="2024-08-12", template="<Record>") }
        }
        "#;
        let mut nodes = crate::parser::parse_document(input).unwrap().1;
        // Execute via direct helper calls (bypassing event executor)
        let owner_path: Vec<String> = vec!["Owner".to_string()];
        // Run ensure_in_list via direct call to avoid building a full runtime action executor for tests
        // Instead, simulate call by invoking ensure_in_list
        ActionExecutor::ensure_in_list(
            &mut nodes,
            &owner_path,
            "/History",
            "<Record>",
            "date",
            OverseerValue::String("2024-08-12".to_string()),
            WhereItGoes::Last,
        )
        .unwrap();
        // Now try to ensure with a timestamp on the same day; should no-op (no duplicate)
        ActionExecutor::ensure_in_list(
            &mut nodes,
            &owner_path,
            "/History",
            "<Record>",
            "date",
            OverseerValue::String("2024-08-12T23:10:00Z".to_string()),
            WhereItGoes::Last,
        )
        .unwrap();
        // Verify History has exactly 1 child
        let history = nodes.iter().find(|n| n.name == "History").unwrap();
        assert_eq!(history.children.len(), 1);
    }

    #[test]
    fn test_remove_from_list_by_key() {
        let input = r#"
        div Root {
            div Task {
                string id = ""
                string title = ""
            }
            list Tasks (entry=<Task>, key="id") {
                - Task { id = "x1" title = "Old" }
            }
            button Remove {
                on click { remove(list="/Root/Tasks", keyField="id", keyValue="x1") }
            }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolver::resolve_document(&mut nodes);
        let path = vec!["Root".to_string(), "Remove".to_string()];
        let res = ActionExecutor::execute_event(&mut nodes, &path, "click");
        assert!(res.is_ok());
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let list = root.children.iter().find(|c| c.name == "Tasks").unwrap();
        assert!(list
            .children
            .iter()
            .all(|it| it.children.iter().all(|f| !(f.name == "id"
                && f.parameters.get("value") == Some(&OverseerValue::String("x1".to_string()))))));
    }

    #[test]
    fn test_set_in_list_updates_item_field_by_key() {
        let input = r#"
        div Root {
            div Task { string id = "" string title = "" }
            list Tasks (entry=<Task>, key="id") {
                - Task { id = "a1" title = "Old" }
                - Task { id = "b2" title = "Other" }
            }
            button UpdateA1 { on click { set_in_list(list="/Root/Tasks", keyField="id", keyValue="a1", field="title", value="New Title") } }
        }
        "#;
        let mut nodes = crate::parser::parse_document(input).unwrap().1;
        crate::resolver::resolve_document(&mut nodes);
        let path = vec!["Root".to_string(), "UpdateA1".to_string()];
        let res = ActionExecutor::execute_event(&mut nodes, &path, "click");
        assert!(res.is_ok());
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let list = root.children.iter().find(|c| c.name == "Tasks").unwrap();
        let a1 = list
            .children
            .iter()
            .find(|it| {
                it.children.iter().any(|f| {
                    f.name == "id"
                        && f.parameters.get("value") == Some(&OverseerValue::String("a1".into()))
                })
            })
            .unwrap();
        let title = a1
            .children
            .iter()
            .find(|f| f.name == "title")
            .and_then(|f| f.parameters.get("value"));
        assert_eq!(title, Some(&OverseerValue::String("New Title".to_string())));
    }

    #[test]
    fn test_append_to_template_list() {
        let input = r#"
        div Root {
            div Task { string id = "" string title = "" }
            list Tasks (entry=<Task>, key="id") { }
            button Add { on click { append(list="/Root/Tasks", template="<Task>") { - id = "t1" } } }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolver::resolve_document(&mut nodes);
        let path = vec!["Root".to_string(), "Add".to_string()];
        let res = ActionExecutor::execute_event(&mut nodes, &path, "click");
        assert!(res.is_ok());
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let list = root.children.iter().find(|c| c.name == "Tasks").unwrap();
        assert_eq!(list.node_type, "list");
        assert_eq!(list.children.len(), 1);
        assert_eq!(list.children[0].node_type, "Task");
    }

    #[test]
    fn test_append_to_simple_list() {
        let input = r#"
        div Root {
            list Names (entry=string) { }
            button Add { on click { append(list="/Root/Names", value="Alice") } }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolver::resolve_document(&mut nodes);
        let path = vec!["Root".to_string(), "Add".to_string()];
        let res = ActionExecutor::execute_event(&mut nodes, &path, "click");
        assert!(res.is_ok());
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let list = root.children.iter().find(|c| c.name == "Names").unwrap();
        assert_eq!(list.node_type, "list");
        assert_eq!(list.children.len(), 1);
        assert_eq!(list.children[0].node_type, "string");
        assert_eq!(
            list.children[0].parameters.get("value"),
            Some(&OverseerValue::String("Alice".to_string()))
        );
    }

    #[test]
    fn test_prepend_to_template_list() {
        let input = r#"
        div Root {
            div Task { string id = "" }
            list Tasks (entry=<Task>, key="id") {
                - Task { id = "b" }
            }
            button AddFirst { on click { prepend(list="/Root/Tasks", template="<Task>") { - id = "a" } } }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolver::resolve_document(&mut nodes);
        let path = vec!["Root".to_string(), "AddFirst".to_string()];
        let res = ActionExecutor::execute_event(&mut nodes, &path, "click");
        assert!(res.is_ok());
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let list = root.children.iter().find(|c| c.name == "Tasks").unwrap();
        let first_id = list.children[0]
            .children
            .iter()
            .find(|f| f.name == "id")
            .and_then(|f| f.parameters.get("value"));
        assert_eq!(first_id, Some(&OverseerValue::String("a".to_string())));
    }

    #[test]
    fn test_prepend_to_simple_list() {
        let input = r#"
        div Root {
            list Names (entry=string) {
                - "Bob"
            }
            button AddFirst { on click { prepend(list="/Root/Names", value="Alice") } }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolver::resolve_document(&mut nodes);
        let path = vec!["Root".to_string(), "AddFirst".to_string()];
        let res = ActionExecutor::execute_event(&mut nodes, &path, "click");
        assert!(res.is_ok());
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let list = root.children.iter().find(|c| c.name == "Names").unwrap();
        let first = &list.children[0];
        assert_eq!(first.node_type, "string");
        assert_eq!(
            first.parameters.get("value"),
            Some(&OverseerValue::String("Alice".to_string()))
        );
    }

    #[test]
    fn test_move_within_list_by_key() {
        let input = r#"
        div Root {
            div Task { string id = "" }
            list Tasks (entry=<Task>, key="id") {
                - Task { id = "a" }
                - Task { id = "b" }
                - Task { id = "c" }
            }
            button Move { on click { move(from="/Root/Tasks", keyField="id", keyValue="b", at=0) } }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolver::resolve_document(&mut nodes);
        let path = vec!["Root".to_string(), "Move".to_string()];
        let res = ActionExecutor::execute_event(&mut nodes, &path, "click");
        assert!(res.is_ok());
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let list = root.children.iter().find(|c| c.name == "Tasks").unwrap();
        let ids: Vec<String> = list
            .children
            .iter()
            .map(|it| {
                it.children
                    .iter()
                    .find(|f| f.name == "id")
                    .and_then(|f| f.parameters.get("value"))
                    .and_then(|v| {
                        if let OverseerValue::String(s) = v {
                            Some(s.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default()
            })
            .collect();
        assert_eq!(ids, vec!["b".to_string(), "a".to_string(), "c".to_string()]);
    }

    #[test]
    fn test_sort_list_by_number_desc() {
        let input = r#"
        div Root {
            div Task { string id = "" int priority = 0 }
            list Tasks (entry=<Task>, key="id") {
                - Task { id = "a" priority = 1 }
                - Task { id = "b" priority = 3 }
                - Task { id = "c" priority = 2 }
            }
            button Sort { on click { sort(list="/Root/Tasks", by="$(x/priority)", order="desc") } }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolver::resolve_document(&mut nodes);
        let path = vec!["Root".to_string(), "Sort".to_string()];
        let res = ActionExecutor::execute_event(&mut nodes, &path, "click");
        assert!(res.is_ok());
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let list = root.children.iter().find(|c| c.name == "Tasks").unwrap();
        let ids: Vec<String> = list
            .children
            .iter()
            .map(|it| {
                it.children
                    .iter()
                    .find(|f| f.name == "id")
                    .and_then(|f| f.parameters.get("value"))
                    .and_then(|v| {
                        if let OverseerValue::String(s) = v {
                            Some(s.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default()
            })
            .collect();
        assert_eq!(ids, vec!["b".to_string(), "c".to_string(), "a".to_string()]);
    }


    #[test]
    fn test_inherited_background_not_persisted_after_action_and_save() {
        use crate::file_ops::OverseerFileHandler;
        // Simulate an Exercise with inherited background-color and a Done button that sets a timestamp
        let input = r#"
        div Exercise (background-color=#ffcccc) {
            div History {
                list items(entry=div) {
                    div Entry1 { string when = "2024-01-01" }
                }
            }
            timestamp last_done (mode="elapsed")
            button Done { on click { set_now_ts(path="../last_done") } }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolver::resolve_document(&mut nodes);
        // Click Done
        let path = vec!["Exercise".to_string(), "Done".to_string()];
        let res = ActionExecutor::execute_event(&mut nodes, &path, "click");
        assert!(res.is_ok());
        // Re-resolve after mutation (mimic UI loop)
        resolver::resolve_document(&mut nodes);
        // Serialize and ensure children of History do not persist inherited background-color
        let out = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        // Only the parent Exercise should carry background-color param
        assert!(out.contains("div Exercise (background-color=#ffcccc)"));
        let mentions = out.matches("background-color").count();
        assert_eq!(
            mentions, 1,
            "inherited background-color must not be written on children after actions"
        );
    }





    #[test]
    fn done_button_appends_under_unnamed_wrappers_with_ordinals() {
        let input = r#"
        div Root {
            div (hidden=true) { div ExerciseRecord { int eid = 0 timestamp time = "" } }
            div (hidden=true) {
                div Exercise {
                    int id = 0
                    button done { on click { append(list="/Root/History", template="<ExerciseRecord>") { - eid = $(../id) - time = "t" } } }
                }
            }
            list Exercises (entry=<Exercise>, key="id") {
                - { int id = 1 }
                - { int id = 2 }
            }
            list History (entry=<ExerciseRecord>, key="time") { }
        }
        "#;
        let mut nodes = crate::parser::parse_document(input).unwrap().1;
        crate::resolver::resolve_document(&mut nodes);
        // Click the Done button on the second Exercise instance (name is Exercise__2)
        let path = vec![
            "Root".into(),
            "Exercises".into(),
            "Exercise__2".into(),
            "done".into(),
        ];
        let res = ActionExecutor::execute_event(&mut nodes, &path, "click");
        assert!(res.is_ok());
        // Verify that one History entry was appended
        let root = nodes.iter().find(|n| n.name == "Root").unwrap();
        let hist = root.children.iter().find(|c| c.name == "History").unwrap();
        assert_eq!(hist.node_type, "list");
        assert_eq!(
            hist.children.len(),
            1,
            "History should have one appended item"
        );
        // And the eid should match the clicked item's id (2)
        let item = &hist.children[0];
        let eid = item
            .children
            .iter()
            .find(|f| f.name == "eid")
            .and_then(|f| f.parameters.get("value"))
            .cloned();
        assert_eq!(eid, Some(OverseerValue::Integer(2)));
    }
}

// Additional tests for helpers
#[cfg(test)]
mod tests_clone_from_template {
    use super::*;
    use crate::file_ops::OverseerFileHandler;
    use crate::parser::parse_document;

    #[test]
    fn preserves_original_type_when_cloning_instance_template() {
        // Create a pseudo instance-like template with node_type "ComponentName" but _original_type "div"
        let mut t = OverseerNode {
            name: "ComponentName".to_string(),
            node_type: "ComponentName".to_string(),
            template: None,
            parameters: Default::default(),
            children: vec![],
            is_hierarchy_transparent: false,
            param_order: Vec::new(),
            raw_value_literal: None,
            authored_dash: false,
            child_original_index: None,
            leading_blank_lines: 0,
            source_snapshot: None,
            source_id: None,
            source_fingerprint: None,
        };
        t.parameters.insert(
            "_original_type".to_string(),
            OverseerValue::String("div".to_string()),
        );
        let cloned = ActionExecutor::clone_from_template(&t);
        assert_eq!(
            cloned.parameters.get("_original_type"),
            Some(&OverseerValue::String("div".to_string()))
        );
        // node_type should remain the component name for consistency with instances
        assert_eq!(cloned.node_type, "ComponentName");
    }

    #[test]
    fn append_with_overrides_serializes_as_object_not_primitive() {
        let input = r#"
        div T { int i = 10 int ii = $(2*i) }
list L (entry=<T>, key="i") { }
button B { on click { append (template="<T>", list="/L") { - i = 20 } } }
button C { on click { append (template="<T>", list="/L") { - i = 30 } } }
int x = 50
button D (label="button 3") {
    on click { append (template="<T>", list="/L") { - i = $(x) } }
}
"#;
        let mut nodes = parse_document(input).unwrap().1;
        resolver::resolve_document(&mut nodes);
        // Click B, C, D to append three items
        assert!(ActionExecutor::execute_event(&mut nodes, &vec!["B".into()], "click").is_ok());
        assert!(ActionExecutor::execute_event(&mut nodes, &vec!["C".into()], "click").is_ok());
        assert!(ActionExecutor::execute_event(&mut nodes, &vec!["D".into()], "click").is_ok());
        // Serialize and verify list entries are objects with named field overrides
        let s = OverseerFileHandler::serialize_nodes(&nodes).unwrap();
        assert!(s.contains("list L ("));
        assert!(s.contains("entry=<T>"));
        assert!(s.contains("key=\"i\""));
        // Ensure we do not emit primitive entries like "- 20"/"- 30"/"- 50"
        assert!(
            !s.contains("\n    - 20\n"),
            "Should not serialize primitive '- 20' entries.\n{}",
            s
        );
        assert!(
            !s.contains("\n    - 30\n"),
            "Should not serialize primitive '- 30' entries.\n{}",
            s
        );
        assert!(
            !s.contains("\n    - 50\n"),
            "Should not serialize primitive '- 50' entries.\n{}",
            s
        );
        // Should contain object block entries with named override lines
        // Check entries have object blocks and named overrides without relying on exact whitespace
        let dash_block_count = s.matches("\n        - {").count() + s.matches("\n    - {").count();
        assert!(
            dash_block_count >= 3,
            "Expected at least three '- {{ ... }}' blocks.\n{}",
            s
        );
        assert!(
            s.contains("- i = 20"),
            "Expected an override '- i = 20'.\n{}",
            s
        );
        assert!(
            s.contains("- i = 30"),
            "Expected an override '- i = 30'.\n{}",
            s
        );
        assert!(
            s.contains("- i = 50"),
            "Expected an override '- i = 50'.\n{}",
            s
        );
    }
}

#[cfg(test)]
mod tests_append_naming_and_transparency {
    use super::*;
    use crate::parser::parse_document;

    #[test]
    fn append_assigns_unique_names() {
        let input = r#"div T { int i = 10 int ii = $(2*i) }
list L (entry=<T>) { }
button B { on click { append (template="<T>", list="/L") { - i = 20 } } }
button B2 { on click { append (template="<T>", list="/L") { - i = 25 } } }
"#;
        let mut nodes = parse_document(input).unwrap().1;
        // Click B then B2
        assert!(ActionExecutor::execute_event(&mut nodes, &vec!["B".into()], "click").is_ok());
        assert!(ActionExecutor::execute_event(&mut nodes, &vec!["B2".into()], "click").is_ok());
        // Find list L
        let l = nodes.iter().find(|n| n.name == "L").unwrap();
        assert_eq!(l.children.len(), 2);
        assert_eq!(l.children[0].name, "T__1");
        assert_eq!(l.children[1].name, "T__2");
        // Values should be set on their own children
        let i1 = l.children[0]
            .children
            .iter()
            .find(|c| c.name == "i")
            .unwrap();
        let i2 = l.children[1]
            .children
            .iter()
            .find(|c| c.name == "i")
            .unwrap();
        assert_eq!(
            i1.parameters.get("value"),
            Some(&OverseerValue::Integer(20))
        );
        assert_eq!(
            i2.parameters.get("value"),
            Some(&OverseerValue::Integer(25))
        );
    }

    #[test]
    fn transparent_unnamed_nodes_in_paths() {
        // Unnamed div should be transparent for path resolution when targeting L and buttons
        let input = r#"div (hidden=true) {
    div ExerciseRecord { int x = 1 }
    div ExerciseRecordSample { int y = 2 }
}
list L (entry=<ExerciseRecord>) { }
button Add { on click { append (template="<ExerciseRecord>", list="/L") { - x = 3 } } }
"#;
        let mut nodes = parse_document(input).unwrap().1;
        // Should be able to click Add even though templates are under unnamed div
        assert!(ActionExecutor::execute_event(&mut nodes, &vec!["Add".into()], "click").is_ok());
        let l = nodes.iter().find(|n| n.name == "L").unwrap();
        assert_eq!(l.children.len(), 1);
        let x = l.children[0]
            .children
            .iter()
            .find(|c| c.name == "x")
            .unwrap();
        assert_eq!(x.parameters.get("value"), Some(&OverseerValue::Integer(3)));
    }
}

#[cfg(test)]
mod tests_append_with_inline_overrides {
    use super::*;
    use crate::file_ops::OverseerFileHandler;
    use crate::parser::parse_document;

    #[test]
    fn append_persists_overrides_in_serialization() {
        let input = r#"
div T { string a = "" int b = 0 timestamp c = $(now()) }
list L (entry=<T>) { }
button Add { on click { append (template="<T>", list="/L") { - a = "hello" - b = 42 } } }
"#;
        let mut nodes = parse_document(input).unwrap().1;
    assert!(ActionExecutor::execute_event(&mut nodes, &vec!["Add".into()], "click").is_ok());
        let s = OverseerFileHandler::serialize_nodes(&nodes).unwrap();
        assert!(s.contains("list L ("));
        assert!(s.contains("- {"));
        assert!(s.contains("- a = \"hello\""));
        assert!(s.contains("- b = 42"));
    }

    #[test]
    fn append_with_nested_container_overrides_preserved_on_save() {
        // Mirrors the weight_tracker scenario: list of MealRecord with a nested per_item container.
        // Appending an item with overrides under per_item should preserve the per_item container in serialization.
        let input = r#"
div MealRecord {
    string description = ""
    int amount = 1
    float calories = $(amount*per_item/calories)
    div per_item { float calories = 100 float weight = 50 }
}
list Intake (entry=<MealRecord>) { }
button Add { on click {
    append (list="/Intake", template="<MealRecord>") {
        - description = "coffee"
        - amount = 2
        - per_item {
            - calories = 120
            - weight = 60
        }
    }
} }
"#;
        let mut nodes = parse_document(input).unwrap().1;
        // Click Add
        assert!(ActionExecutor::execute_event(&mut nodes, &vec!["Add".into()], "click").is_ok());
        // Verify structure in memory
        let intake = nodes.iter().find(|n| n.name == "Intake").unwrap();
        assert_eq!(intake.children.len(), 1);
        let item = &intake.children[0];
        // Ensure per_item container exists and has overrides
        let per_item = item
            .children
            .iter()
            .find(|c| c.name == "per_item")
            .expect("per_item missing");
        let cal = per_item
            .children
            .iter()
            .find(|c| c.name == "calories")
            .unwrap();
        assert_eq!(
            cal.parameters.get("value"),
            Some(&OverseerValue::Integer(120))
        );
        let wt = per_item
            .children
            .iter()
            .find(|c| c.name == "weight")
            .unwrap();
        assert_eq!(
            wt.parameters.get("value"),
            Some(&OverseerValue::Integer(60))
        );
        // Serialize and check that the per_item block is emitted with nested overrides
    let s = OverseerFileHandler::serialize_nodes(&nodes).unwrap();
        assert!(s.contains("list Intake (entry=<MealRecord>)"), "{}", s);
        // Must have a nested per_item override block, not flattened fields
        assert!(
            s.contains("per_item {"),
            "expected per_item block in serialization\n{}",
            s
        );
        // Accept either concise "- name = value" overrides or typed field lines inside per_item
        let has_calories = s.contains("- calories = 120") || s.contains("float calories = 120");
        let has_weight = s.contains("- weight = 60") || s.contains("float weight = 60");
        assert!(
            has_calories,
            "missing nested calories override/value in per_item\n{}",
            s
        );
        assert!(
            has_weight,
            "missing nested weight override/value in per_item\n{}",
            s
        );
        // Round-trip parse and ensure structure remains
        let (_rem, mut nodes2) = parse_document(&s).unwrap();
        crate::resolver::resolve_document(&mut nodes2);
        let intake2 = nodes2.iter().find(|n| n.name == "Intake").unwrap();
        let item2 = &intake2.children[0];
        let per_item2 = item2
            .children
            .iter()
            .find(|c| c.name == "per_item")
            .expect("per_item missing after roundtrip");
        let cal2 = per_item2
            .children
            .iter()
            .find(|c| c.name == "calories")
            .unwrap();
        assert_eq!(
            cal2.parameters.get("value"),
            Some(&OverseerValue::Integer(120))
        );
        let wt2 = per_item2
            .children
            .iter()
            .find(|c| c.name == "weight")
            .unwrap();
        assert_eq!(
            wt2.parameters.get("value"),
            Some(&OverseerValue::Integer(60))
        );
    }
}

#[cfg(test)]
mod tests_if_action {
    use super::*;
    use crate::parser::parse_document;
    use crate::resolver::resolve_document;

    #[test]
    fn test_if_cond_true_executes_children() {
        let input = r#"
        div Root {
            int A = 0
            button B { on click {
                if (cond=$(true)) { set(path="/Root/A", mode="value") = 42 }
            } }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let path = vec!["Root".to_string(), "B".to_string()];
        let res = ActionExecutor::execute_event(&mut nodes, &path, "click");
        assert!(res.is_ok());
        // Verify A updated
        fn find<'a>(nodes: &'a [OverseerNode], path: &[&str]) -> Option<&'a OverseerNode> {
            if path.is_empty() {
                return None;
            }
            let mut cur: Option<&OverseerNode> = None;
            for (i, seg) in path.iter().enumerate() {
                let list = if i == 0 {
                    nodes
                } else {
                    &cur.unwrap().children
                };
                cur = list.iter().find(|n| n.name == *seg);
                if cur.is_none() {
                    return None;
                }
            }
            cur
        }
        let a = find(&nodes, &["Root", "A"]).unwrap();
        assert_eq!(a.parameters.get("value"), Some(&OverseerValue::Integer(42)));
    }

    #[test]
    fn test_if_cond_false_skips_children() {
        let input = r#"
        div Root {
            int A = 0
            button B { on click {
                if (cond=$(false)) { set(path="/Root/A", mode="value") = 42 }
            } }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let path = vec!["Root".to_string(), "B".to_string()];
        let res = ActionExecutor::execute_event(&mut nodes, &path, "click");
        assert!(res.is_ok());
        // Verify A unchanged
        fn find<'a>(nodes: &'a [OverseerNode], path: &[&str]) -> Option<&'a OverseerNode> {
            if path.is_empty() {
                return None;
            }
            let mut cur: Option<&OverseerNode> = None;
            for (i, seg) in path.iter().enumerate() {
                let list = if i == 0 {
                    nodes
                } else {
                    &cur.unwrap().children
                };
                cur = list.iter().find(|n| n.name == *seg);
                if cur.is_none() {
                    return None;
                }
            }
            cur
        }
        let a = find(&nodes, &["Root", "A"]).unwrap();
        assert_eq!(a.parameters.get("value"), Some(&OverseerValue::Integer(0)));
    }

    #[test]
    fn test_if_executes_multiple_children_when_true() {
        let input = r#"
        div Root {
            int A = 0
            list L (entry=int) { }
            button Btn { on click {
                if (cond=$(1 == 1)) {
                    append (list="/Root/L", value=7)
                    inc (path="/Root/A", by=5)
                }
            } }
        }
        "#;
        let mut nodes = parse_document(input).unwrap().1;
        resolve_document(&mut nodes);
        let path = vec!["Root".to_string(), "Btn".to_string()];
        let res = ActionExecutor::execute_event(&mut nodes, &path, "click");
        assert!(res.is_ok());

        // Verify A incremented
        fn find<'a>(nodes: &'a [OverseerNode], path: &[&str]) -> Option<&'a OverseerNode> {
            if path.is_empty() {
                return None;
            }
            let mut cur: Option<&OverseerNode> = None;
            for (i, seg) in path.iter().enumerate() {
                let list = if i == 0 {
                    nodes
                } else {
                    &cur.unwrap().children
                };
                cur = list.iter().find(|n| n.name == *seg);
                if cur.is_none() {
                    return None;
                }
            }
            cur
        }
        let a = find(&nodes, &["Root", "A"]).unwrap();
        assert_eq!(a.parameters.get("value"), Some(&OverseerValue::Integer(5)));

        // Verify list L has one item with value 7
        let l = find(&nodes, &["Root", "L"]).unwrap();
        assert_eq!(l.children.len(), 1, "Expected one item appended to list L");
        let entry = &l.children[0];
        let val = entry
            .parameters
            .get("value")
            .cloned()
            .unwrap_or(OverseerValue::Integer(-1));
        assert_eq!(val, OverseerValue::Integer(7));
    }
}
