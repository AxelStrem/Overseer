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
    /// did, every address after the change may mean something else, so the graph is no use and
    /// the whole document is worked out again.
    pub structural: bool,
    /// Writes the document said belong to whoever is looking rather than to the document -
    /// `mutable="guarded"`, in force here or anywhere above. They are named and their values
    /// carried out rather than written down, and the caller decides where a viewer's state
    /// lives. The document is left exactly as it was.
    pub view_state: Vec<(String, OverseerValue)>,
}

thread_local! {
    /// Whether whoever asked is going to work the document out afterwards - see
    /// `caller_will_settle_it`.
    static CALLER_SETTLES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
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
    REPORT.with(|r| *r.borrow_mut() = Some(Changed::default()));
}

/// Stop, and take what was noted. `None` when nobody asked.
pub fn take_report() -> Option<Changed> {
    REPORT.with(|r| r.borrow_mut().take())
}

/// One value moved, at this address.
fn note_field(address: String) {
    REPORT.with(|r| {
        if let Some(changed) = r.borrow_mut().as_mut() {
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
        "string" | "text" | "textbox" | "tags" => Some(OverseerValue::String(text.clone())),
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

/// The document's shape moved, so no address can be trusted to still mean what it did.
fn note_structural() {
    REPORT.with(|r| {
        if let Some(changed) = r.borrow_mut().as_mut() {
            changed.structural = true;
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
        // descend
        let mut cur: &OverseerNode = root;
        for idx in indices.iter().skip(1) {
            if *idx >= cur.children.len() {
                break;
            }
            let child = &cur.children[*idx];
            let base = eff(child);
            let prior = cur
                .children
                .iter()
                .take(*idx)
                .filter(|c| eff(c) == base)
                .count();
            let mut seg = base.to_string();
            if prior > 0 {
                seg = format!("{}#{}", seg, prior);
            }
            out.push(seg);
            cur = child;
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
        // Build an internal, disambiguated path for evaluation contexts
        let owner_eval_path = Self::build_disambiguated_path(&nodes.clone(), &owner_indices);

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
                    if Self::action_changes_structure(&action.node_type) && at < last {
                        resolver::resolve_document(nodes);
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
        match action.node_type.as_str() {
            "if" => {
                // if(cond=...) { <actions...> }
                // Evaluate cond in owner's context (defaults to false if missing)
                let snapshot = nodes.clone();
                let cond_val = match action.parameters.get("cond") {
                    Some(v) => Self::evaluate_in_context(v, owner_path, &snapshot)?,
                    None => OverseerValue::Boolean(false),
                };
                if Self::to_bool(&cond_val) {
                    for child in &action.children {
                        let res = Self::execute_action(nodes, owner_indices, owner_path, child);
                        if let Err(e) = res {
                            return Err(e);
                        }
                        resolver::resolve_document(nodes);
                    }
                }
                Ok(())
            }
            "load_mount" => {
                Self::execute_load_mount(nodes, owner_indices, owner_path, action)?;
                Ok(())
            }
            "unload_mount" => {
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
                        let snapshot = nodes.clone();
                        let ctx = EvaluationContext::new(owner_path.to_vec(), &snapshot);
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
                        let snapshot = nodes.clone();
                        let ctx = EvaluationContext::new(owner_path.to_vec(), &snapshot);
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
                        let snapshot = nodes.clone();
                        let ctx = EvaluationContext::new(owner_path.to_vec(), &snapshot);
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
                // Evaluate keyValue and value in the owner's context (if provided as formulas)
                let snapshot = nodes.clone();
                let key_value = match action.parameters.get("keyValue") {
                    Some(v) => Self::evaluate_in_context(v, owner_path, &snapshot)?,
                    None => {
                        return Err(OverseerError::ValidationError(
                            "set_in_list.keyValue required".to_string(),
                        ))
                    }
                };
                let new_value = match action.parameters.get("value") {
                    Some(v) => Self::evaluate_in_context(v, owner_path, &snapshot)?,
                    None => {
                        return Err(OverseerError::ValidationError(
                            "set_in_list.value required".to_string(),
                        ))
                    }
                };
                // Resolve list by path using snapshot for path calculation, then mutate on nodes
                let (segments, _explicit_param, anchored) = Self::split_path_and_param(&list_path);
                let indices = match Self::resolve_target_indices(
                    &snapshot, owner_path, anchored, &segments,
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
                if let Some(item) = list_node.children.iter_mut().find(|it| {
                    Self::get_field_value(it, &effective_key_field)
                        .map_or(false, |v| Self::value_equals(v, &key_value))
                }) {
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
                        let snapshot = nodes.clone();
                        let ctx = EvaluationContext::new(owner_path.to_vec(), &snapshot);
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
        map: &std::collections::HashMap<String, OverseerValue>,
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
        map: &std::collections::HashMap<String, OverseerValue>,
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
        params: &'a std::collections::HashMap<String, OverseerValue>,
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
            parameters: std::collections::HashMap::new(),
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
            parameters: std::collections::HashMap::new(),
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

        let stamp = std::fs::metadata(path)
            .ok()
            .and_then(|m| m.modified().ok().map(|t| (t, m.len())));

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
        let loaded_roots: Vec<OverseerNode> = if let Some(fp) = file_path_opt {
            // A mount's source is written relative to the document that declares it, not to
            // wherever the process happens to be running from.
            let fp = crate::docmgr::manager::DocumentManager::resolve_from_document(&fp)
                .to_string_lossy()
                .to_string();
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
            node.source_snapshot = Some(NodeSourceSnapshot::synthetic_from_template(
                &existing_snapshot,
            ));
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
                    let mut m = std::collections::HashMap::new();
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

    fn get_field_value<'a>(item: &'a OverseerNode, field: &'a str) -> Option<&'a OverseerValue> {
        item.children
            .iter()
            .find(|c| c.name == field)
            .and_then(|c| c.parameters.get("value"))
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
        // Shape, not value: what this does moves the addresses of everything after
        // it, so nothing the graph knows survives it.
        note_structural();
        let (segments, _explicit_param, anchored) = Self::split_path_and_param(list_path);
        // Clone nodes snapshot for immutable searches to avoid aliasing
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
        // rather than just "done" is what lets one instruction mean "make sure of this entry and
        // then write into it" - the same sentence whether the entry was a preview a moment ago or
        // has been in the file for a month.
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
        let template_def = Self::find_node_by_name(&snapshot, tn).ok_or_else(|| {
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
        let made = new_item.name.clone();
        match goes {
            WhereItGoes::First => list_node.children.insert(0, new_item),
            WhereItGoes::Last => list_node.children.push(new_item),
        }
        // The list's text no longer matches what was parsed from it, so it has to be written out
        // again rather than replayed - the same reason removing an entry clears this.
        list_node.source_fingerprint = None;
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

    fn remove_from_list(
        nodes: &mut Vec<OverseerNode>,
        owner_path: &[String],
        list_path: &str,
        key_field: &str,
        key_value: &OverseerValue,
    ) -> Result<(), OverseerError> {
        // Shape, not value: what this does moves the addresses of everything after
        // it, so nothing the graph knows survives it.
        note_structural();
        let (segments, _explicit_param, anchored) = Self::split_path_and_param(list_path);
        let indices = Self::resolve_target_indices(&nodes, owner_path, anchored, &segments)
            .ok_or_else(|| {
                OverseerError::ValidationError(format!("List not found: {}", list_path))
            })?;
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

        if let Some(pos) = list_node.children.iter().position(|it| {
            Self::get_field_value(it, &effective_key_field).map_or(false, |v| {
                Self::value_equals_with_key_precision(&list_node, v, key_value)
            })
        }) {
            list_node.children.remove(pos);
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
        let (segments, _explicit_param, anchored) = Self::split_path_and_param(entry_path);
        let indices = Self::resolve_target_indices(&nodes, &[], anchored, &segments)
            .ok_or_else(|| {
                OverseerError::ValidationError(format!("Nothing at: {}", entry_path))
            })?;

        let (last, parent_indices) = indices
            .split_last()
            .ok_or_else(|| OverseerError::ValidationError("Nothing to remove".to_string()))?;

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

        parent.children.remove(*last);
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
        Self::set_value(nodes, &[], &[], target, value)
    }

    /// The overrides a copy from `from` makes, for an entry made from this template.
    ///
    /// `append (list=..., from="..")` fills the new entry from another node - a div of textboxes,
    /// in the case it was made for, so a form can add a list entry in one press without the
    /// action naming each field. The entry is still made from the list's template; the source only
    /// supplies values. A field is matched by its name, and by the names of the divs it sits in,
    /// with unnamed wrappers ignored on both sides, since those arrange rather than mean anything.
    /// The template's type is kept where the two differ and the value converted to it where it can
    /// be - text into a number, a date, a flag - and left out where it cannot, or where there is
    /// nothing in it, so the template's default stands. A field the action's own block names is
    /// the block's: `- added = $(now())` beside a copy says what the form does not.
    ///
    /// The textboxes read here are noted, so the page can empty them once the press is done.
    fn overrides_copied_from(
        snapshot: &Vec<OverseerNode>,
        owner_path: &[String],
        from: &OverseerValue,
        template: &OverseerNode,
        block: &[OverseerNode],
    ) -> Result<Vec<OverseerNode>, OverseerError> {
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
        let source = Self::resolve_target_indices(snapshot, owner_path, anchored, &segments)
            .and_then(|at| Self::get_node_ref_by_indices(snapshot, &at))
            .ok_or_else(|| OverseerError::ValidationError(format!("Source not found: {}", from_path)))?;

        const HOLDS_A_VALUE: [&str; 10] =
            ["string", "text", "textbox", "int", "float", "bool", "checkbox", "date", "timestamp", "tags"];
        fn is_wrapper(node: &OverseerNode) -> bool {
            node.is_hierarchy_transparent && (node.name.is_empty() || node.name == node.node_type)
        }
        /// Every field under a node, by the names that lead to it.
        fn fields<'a>(node: &'a OverseerNode, trail: &mut Vec<String>, out: &mut Vec<(String, &'a OverseerNode)>) {
            for child in &node.children {
                if HOLDS_A_VALUE.contains(&child.node_type.as_str()) {
                    if !child.name.is_empty() {
                        trail.push(child.name.clone());
                        out.push((trail.join("/"), child));
                        trail.pop();
                    }
                } else if child.node_type == "list" || child.node_type == "on" || child.node_type == "button" {
                    // What a form holds besides its fields: the button that sends it, and lists,
                    // which are not a value to copy.
                } else if is_wrapper(child) {
                    fields(child, trail, out);
                } else if !child.name.is_empty() {
                    trail.push(child.name.clone());
                    fields(child, trail, out);
                    trail.pop();
                }
            }
        }
        let mut wanted = Vec::new();
        fields(template, &mut Vec::new(), &mut wanted);
        let mut offered = Vec::new();
        fields(source, &mut Vec::new(), &mut offered);

        let said_by_the_block: Vec<&str> = block.iter().map(|o| o.name.as_str()).collect();
        let mut values = std::collections::HashMap::new();
        for (path, field) in offered {
            if let Some(OverseerValue::String(key)) = field.parameters.get(TYPED_FROM) {
                note_emptied(key);
            }
            let first = path.split('/').next().unwrap_or("");
            if said_by_the_block.contains(&first) {
                continue;
            }
            let Some((_, into)) = wanted.iter().find(|(p, _)| *p == path) else { continue };
            let value = field
                .parameters
                .get("_computed_value")
                .or_else(|| field.parameters.get("value"));
            if let Some(v) = value.and_then(|v| converted(v, &into.node_type)) {
                values.insert(path, v);
            }
        }
        Ok(crate::app_api::entry_overrides(&values))
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
                "append.target is not a list".to_string(),
            ));
        }

        let style_guide = Self::derive_list_entry_style(list_node, false);

        // Determine entry type
        if let Some(entry) = list_node.parameters.get("entry") {
            match entry {
                OverseerValue::Template(t) => {
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
                    let template_def = Self::find_node_by_name(&snapshot, &chosen_template_name)
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
                    // Assign a unique instance name (e.g., T__1, T__2) to avoid duplicate sibling names.
                    // Using current length+1 reflects the creation order and matches resolver's reload naming convention.
                    let ordinal = list_node.children.len() + 1;
                    new_item.name = format!("{}__{}", template_def.name, ordinal);
                    // Apply evaluated overrides from action block, after whatever a copy supplies
                    let mut all = match from {
                        Some(from) => Self::overrides_copied_from(&snapshot, owner_path, from, template_def, overrides)?,
                        None => Vec::new(),
                    };
                    all.extend(overrides.iter().cloned());
                    Self::apply_overrides_evaluated(
                        &mut new_item,
                        &all,
                        owner_path,
                        &snapshot,
                    )?;
                    Self::apply_list_entry_style(&mut new_item, &style_guide);
                    Self::mark_the_entry_as_new(&mut new_item);
                    list_node.children.push(new_item);
                    Self::harmonize_list_entry_spacing(list_node, &style_guide);
                    // Mark this list field as explicitly overridden so mutations persist on template instances
                    Self::mark_field_explicit_override(nodes, &indices);
                }
                OverseerValue::String(type_name) => {
                    // Simple type list requires a value
                    let val = value_opt.ok_or_else(|| {
                        OverseerError::ValidationError(
                            "append.value required for simple list".to_string(),
                        )
                    })?;
                    let mut item = OverseerNode {
                        name: format!("{}__{}", type_name, list_node.children.len() + 1),
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
                    Self::apply_list_entry_style(&mut item, &style_guide);
                    Self::mark_the_entry_as_new(&mut item);
                    list_node.children.push(item);
                    Self::harmonize_list_entry_spacing(list_node, &style_guide);
                    // Mark this list field as explicitly overridden so mutations persist on template instances
                    Self::mark_field_explicit_override(nodes, &indices);
                }
                _ => {
                    return Err(OverseerError::ValidationError(
                        "append: unsupported entry type".to_string(),
                    ))
                }
            }
        } else {
            return Err(OverseerError::ValidationError(
                "append: list has no entry parameter".to_string(),
            ));
        }
        Ok(())
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
                "prepend.target is not a list".to_string(),
            ));
        }

        let style_guide = Self::derive_list_entry_style(list_node, true);

        if let Some(entry) = list_node.parameters.get("entry") {
            match entry {
                OverseerValue::Template(t) => {
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
                    let template_def = Self::find_node_by_name(&snapshot, &chosen_template_name)
                        .ok_or_else(|| {
                            OverseerError::ValidationError(format!(
                                "Template not found: {}",
                                chosen_template_name
                            ))
                        })?;
                    let mut new_item = Self::clone_from_template(template_def);
                    if let Some(OverseerValue::String(parent_eff)) =
                        list_node.parameters.get("_effective_layout")
                    {
                        let opp = Self::opposite_layout(parent_eff);
                        new_item
                            .parameters
                            .insert("_effective_layout".to_string(), OverseerValue::String(opp));
                    }
                    // Name as if appended to the front: use current length+1 to maintain unique names
                    let ordinal = list_node.children.len() + 1;
                    new_item.name = format!("{}__{}", template_def.name, ordinal);
                    let mut all = match from {
                        Some(from) => Self::overrides_copied_from(&snapshot, owner_path, from, template_def, overrides)?,
                        None => Vec::new(),
                    };
                    all.extend(overrides.iter().cloned());
                    Self::apply_overrides_evaluated(
                        &mut new_item,
                        &all,
                        owner_path,
                        &snapshot,
                    )?;
                    Self::apply_list_entry_style(&mut new_item, &style_guide);
                    Self::mark_the_entry_as_new(&mut new_item);
                    list_node.children.insert(0, new_item);
                    Self::harmonize_list_entry_spacing(list_node, &style_guide);
                    // Mark this list field as explicitly overridden so mutations persist on template instances
                    Self::mark_field_explicit_override(nodes, &indices);
                }
                OverseerValue::String(type_name) => {
                    let val = value_opt.ok_or_else(|| {
                        OverseerError::ValidationError(
                            "prepend.value required for simple list".to_string(),
                        )
                    })?;
                    let mut item = OverseerNode {
                        name: format!("{}__{}", type_name, list_node.children.len() + 1),
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
                    Self::apply_list_entry_style(&mut item, &style_guide);
                    Self::mark_the_entry_as_new(&mut item);
                    list_node.children.insert(0, item);
                    Self::harmonize_list_entry_spacing(list_node, &style_guide);
                    // Mark this list field as explicitly overridden so mutations persist on template instances
                    Self::mark_field_explicit_override(nodes, &indices);
                }
                _ => {
                    return Err(OverseerError::ValidationError(
                        "prepend: unsupported entry type".to_string(),
                    ))
                }
            }
        } else {
            return Err(OverseerError::ValidationError(
                "prepend: list has no entry parameter".to_string(),
            ));
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
        match node.source_snapshot.as_mut() {
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
            if let Some(pos) = list_node.children.iter().position(|it| {
                Self::get_field_value(it, &effective_key_field)
                    .map_or(false, |v| Self::value_equals(v, key_value))
            }) {
                let item = list_node.children.remove(pos);
                let insert_at = at_index.unwrap_or(list_node.children.len());
                let idx = if insert_at > list_node.children.len() {
                    list_node.children.len()
                } else {
                    insert_at
                };
                list_node.children.insert(idx, item);
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
                if let Some(pos) = from_node.children.iter().position(|it| {
                    Self::get_field_value(it, &effective_key_field)
                        .map_or(false, |v| Self::value_equals(v, key_value))
                }) {
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
                let insert_at = at_index.unwrap_or(to_node.children.len());
                let idx = if insert_at > to_node.children.len() {
                    to_node.children.len()
                } else {
                    insert_at
                };
                to_node.children.insert(idx, item);
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

        // Take children to reorder
        let children_snapshot = cur.children.clone();
        let taken = std::mem::take(&mut list_node.children);
        let mut entries: Vec<(OverseerValue, OverseerNode, usize)> =
            Vec::with_capacity(taken.len());

        for (i, item) in taken.into_iter().enumerate() {
            // Build context with x bound to snapshot item
            let base_ctx = EvaluationContext {
                current_node: cur,
                parent_node: None,
                document_root: &snapshot,
                node_path: owner_path.to_vec(),
                var_bindings: std::collections::HashMap::new(),
            };
            let ctx = base_ctx.with_var("x", BoundValue::Node(&children_snapshot[i]));
            let key = match FormulaEvaluator::evaluate_formula(by_expr, &ctx) {
                Ok(v) => v,
                Err(_) => OverseerValue::String(String::new()),
            };
            entries.push((key, item, i));
        }

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
            button Add { on click { append(list="/Root/Tasks", template="<Task>") } }
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
list L (entry=<T>, key="id") { }
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
        assert!(s.contains("key=\"id\""));
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
