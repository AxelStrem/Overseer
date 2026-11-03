//! A list that keeps its entries in files of their own: `list History (..., parts=$(|x| week(x/finished_at)))`.
//!
//! A history grows without end, and most of it is never read again, but held in the document it is
//! read, written and backed up on every change. With `parts`, each entry lives in the file its key
//! names - `overseer.parts/History/2026-W45.os` beside `overseer.os` - and none in the document
//! itself. The list is still one list: its entries are addressed, read and written exactly as
//! though the document held them.
//!
//! There is nothing to rotate. An entry goes to the part its key names whenever the list is
//! written, so a new week simply starts a new file, and an entry whose key changes moves from one
//! file to the other in the same write.
//!
//! A part holds `list History { - {...} }` and nothing else: the entries as the document's writer
//! writes them, against the document's template, so what an entry leaves out is still its
//! template's. Nothing works a part out on its own; it is only ever read into its document.

use crate::types::{OverseerNode, OverseerValue};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

/// On an entry: the key of the part it was read from, or is to be written to. Never written.
pub const PART: &str = "_part";

/// On a list with parts: the folder its parts are kept in. Says the parts have been read in, so
/// a tree worked out again does not read them twice.
pub const FOLDER: &str = "_parts_folder";

/// On a list with parts: every part read, as lines of `file`, a tab, and how it stood then - see
/// `actions::mount_stamp`. What a document held worked out is only as current as.
pub const READ: &str = "_parts_read";

/// On a list with parts: why one of them could not be had, or an entry has no part to go to.
pub const PROBLEM: &str = "_parts_problem";

/// On a list with parts: the parts not read, oldest first, one key a line - see `bring_in`.
pub const UNREAD: &str = "_parts_unread";

/// On a list with parts: how many parts it has not read, for the page's note under the list.
pub const UNREAD_COUNT: &str = "_parts_left_unread";

/// On a list with parts not read: its address, as what reaches past it names it.
pub const ADDRESS: &str = "_parts_address";

/// On a list with parts not read: the lines of `READ` for those parts, oldest first.
pub const UNREAD_STAMPS: &str = "_parts_unread_stamps";

/// The folder a document's list keeps its parts in: beside the document, under its name.
pub fn folder_for(document: &Path, list: &str) -> Option<PathBuf> {
    let stem = document.file_stem()?.to_string_lossy().into_owned();
    Some(document.parent()?.join(format!("{}.parts", stem)).join(list))
}

/// Whether a folder is where a document keeps its lists' parts.
pub fn is_a_parts_folder(folder: &Path) -> bool {
    folder.extension().is_some_and(|e| e == "parts")
}

/// Whether a list keeps its entries in parts, and with which key.
pub fn key_of(list: &OverseerNode) -> Option<&str> {
    if list.node_type != "list" {
        return None;
    }
    match list.parameters.get("parts") {
        Some(OverseerValue::Formula(s)) | Some(OverseerValue::String(s)) if !s.trim().is_empty() => Some(s.as_str()),
        _ => None,
    }
}

/// Whether a key can name a file: anything a folder on any system can hold, and not a step out
/// of the folder.
pub fn usable(key: &str) -> bool {
    !key.is_empty()
        && !key.starts_with('.')
        && key.len() <= 120
        && !key.chars().any(|c| c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
}

/// A part's text as it was last read, parsed and not worked out, under how the file stood.
///
/// Parsed once for as long as the file stands as it did. Every write reads its document again, and
/// a history in weekly parts is otherwise parsed in full for each.
static READ_PARTS: LazyLock<Mutex<HashMap<PathBuf, (String, String, Vec<OverseerNode>)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// The text of a part and its nodes, parsed. `None` when it is not there, or does not parse as
/// a whole: the parser passes over what it cannot read and the document's writer keeps that text,
/// but a part is written from its entries, and what was passed over would be lost.
fn read(file: &Path) -> Option<(String, Vec<OverseerNode>)> {
    let stamp = crate::actions::mount_stamp(&file.to_string_lossy());
    if let Ok(held) = READ_PARTS.lock() {
        if let Some((was, text, nodes)) = held.get(file) {
            if *was == stamp {
                return Some((text.clone(), nodes.clone()));
            }
        }
    }
    let text = std::fs::read_to_string(file).ok()?;
    let (rest, nodes) = crate::parser::parse_document(&text).ok()?;
    if !rest.trim().is_empty() || passed_over(&nodes) {
        return None;
    }
    if let Ok(mut held) = READ_PARTS.lock() {
        held.insert(file.to_path_buf(), (stamp, text.clone(), nodes.clone()));
    }
    Some((text, nodes))
}

fn passed_over(nodes: &[OverseerNode]) -> bool {
    nodes
        .iter()
        .any(|n| n.parameters.contains_key(crate::parser::PARSE_ERROR) || passed_over(&n.children))
}

/// The list a part holds: the first one in it.
fn list_in(nodes: &mut [OverseerNode]) -> Option<&mut OverseerNode> {
    nodes.iter_mut().find(|n| n.node_type == "list")
}

/// The parts kept in a folder, in the order of their keys.
fn parts_in(folder: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<(String, PathBuf)> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("os"))
        .filter_map(|p| Some((p.file_stem()?.to_string_lossy().into_owned(), p)))
        .collect();
    found.sort();
    found
}

/// Every list of a document that keeps its entries in parts, by its name, with the file names of
/// its parts in the order of their keys - for a program that reads the parts as text.
///
/// Read from the folders beside the document, not from the document: `parts=` is often on a
/// template's list, and the folder is named by the list in any case. A folder with no part in it
/// is left out.
pub fn parts_of(document: &Path) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut found = std::collections::BTreeMap::new();
    let Some(folder) = document.file_stem().and_then(|stem| {
        Some(document.parent()?.join(format!("{}.parts", stem.to_string_lossy())))
    }) else {
        return found;
    };
    let Ok(lists) = std::fs::read_dir(&folder) else {
        return found;
    };
    for list in lists.flatten().filter(|e| e.path().is_dir()) {
        let files: Vec<String> = parts_in(&list.path())
            .into_iter()
            .filter_map(|(_, file)| Some(file.file_name()?.to_string_lossy().into_owned()))
            .collect();
        if !files.is_empty() {
            found.insert(list.file_name().to_string_lossy().into_owned(), files);
        }
    }
    found
}

/// Bring a list's parts in, after the entries the document holds itself, before any of them is
/// made from the template - so an entry read from a part is made exactly as one written in the
/// document is.
///
/// `wanted` is how many entries the list has to hold for what will be drawn of it, `None` for all
/// of them. A list with a window reads its parts from the newest back, and stops once it holds
/// that many: a part nothing reaches is never parsed. The parts left are said on the list -
/// `UNREAD` - and a tree worked out again reads what more it now needs of them, the rest when
/// something reaches past what it holds.
///
/// A document worked out with no file to say where it is - a text handed over on its own - has no
/// parts to read, and its list holds only what the text does.
pub fn bring_in(list: &mut OverseerNode, address: &str, wanted: Option<usize>) {
    bring_in_choosing(list, address, |_key, held| wanted.is_none_or(|wanted| held < wanted));
}

/// Read in the parts a write is about to put entries into, that the list has not read: a part is
/// written from the entries the list holds of it, and one not read would lose the rest.
///
/// Read as entries out of view that nothing has worked out, as a window leaves them: the tree they
/// come into has been worked out already, and they are not drawn.
fn bring_in_for_a_write(list: &mut OverseerNode, keys: &[String]) {
    let Some(address) = address_of_unread(list).map(str::to_string) else { return };
    bring_in_choosing(list, &address, |key, _held| keys.iter().any(|k| k == key));
    // Every entry of these parts the list holds was just read: none was, while they were unread.
    for entry in list.children.iter_mut() {
        if matches!(entry.parameters.get(PART), Some(OverseerValue::String(k)) if keys.contains(k)) {
            entry.parameters.insert(crate::resolver::OUT_OF_VIEW.to_string(), OverseerValue::Boolean(true));
            entry.parameters.insert(crate::resolver::NOT_WORKED_OUT.to_string(), OverseerValue::String(address.clone()));
        }
    }
}

fn bring_in_choosing(list: &mut OverseerNode, address: &str, wants: impl Fn(&str, usize) -> bool) {
    if key_of(list).is_none() {
        return;
    }
    let fresh = !list.parameters.contains_key(FOLDER);
    let (folder, mut read_lines, mut problems, unread): (PathBuf, Vec<String>, Vec<String>, Vec<(String, PathBuf)>) = if fresh {
        let Some(document) = crate::docmgr::manager::DocumentManager::document_file() else {
            return;
        };
        let Some(folder) = folder_for(&document, &list.name) else { return };
        let mut problems = Vec::new();
        // Entries the document holds itself go to their parts the next time the list is written -
        // unless the part cannot be worked out, which that write says. Until then, it is said here.
        let own = list.children.iter().filter(|c| is_an_entry(c)).count();
        if own > 0 {
            problems.push(format!(
                "{} {} in the document itself rather than in a part",
                own,
                if own == 1 { "entry is" } else { "entries are" }
            ));
        }
        // The folder too: a part that appears in it - a new week, brought by a sync - is in none of
        // the files read, and is noticed by the folder having changed.
        let parts = parts_in(&folder);
        (folder.clone(), vec![stamp_line(&folder)], problems, parts)
    } else {
        let left = unread_of(list);
        if left.is_empty() {
            return;
        }
        let Some(OverseerValue::String(folder)) = list.parameters.get(FOLDER) else { return };
        let folder = PathBuf::from(folder);
        let lines = match list.parameters.get(READ) {
            Some(OverseerValue::String(lines)) => lines.lines().map(str::to_string).collect(),
            _ => Vec::new(),
        };
        let problems = match list.parameters.get(PROBLEM) {
            Some(OverseerValue::String(said)) => said.split("; ").map(str::to_string).collect(),
            _ => Vec::new(),
        };
        let parts = left.into_iter().map(|key| (key.clone(), folder.join(format!("{}.os", key)))).collect();
        (folder, lines, problems, parts)
    };
    // Newest first, as many as the window wants: what is read is each part whole, so the list may
    // hold a few more than it draws.
    let mut held = list.children.iter().filter(|c| is_an_entry(c)).count();
    let mut to_read: Vec<(String, PathBuf, Vec<OverseerNode>)> = Vec::new();
    let mut left: Vec<String> = Vec::new();
    let mut left_stamps: Vec<String> = Vec::new();
    for (key, file) in unread.into_iter().rev() {
        if !wants(&key, held) {
            // How a part left unread stands is what its summary is kept under, and what the
            // document worked out from that summary is only as current as - see `partsums`.
            let line = stamp_line(&file);
            read_lines.push(line.clone());
            left_stamps.push(line);
            left.push(key);
            continue;
        }
        // Taken before the file is read, so a write landing in between leaves the stamp older
        // than what was read, and the next look finds it stale - never the other way round.
        read_lines.push(stamp_line(&file));
        let Some((_text, mut nodes)) = read(&file) else {
            problems.push(format!("the part '{}' could not be read", key));
            continue;
        };
        let Some(part) = list_in(&mut nodes) else {
            problems.push(format!("the part '{}' holds no list", key));
            continue;
        };
        let entries = std::mem::take(&mut part.children);
        held += entries.len();
        to_read.push((key, file, entries));
    }
    // Oldest first again, each where its key puts it among the parts the list holds already.
    for (key, _file, entries) in to_read.into_iter().rev() {
        let at = list
            .children
            .iter()
            .position(|c| matches!(c.parameters.get(PART), Some(OverseerValue::String(k)) if k.as_str() > key.as_str()))
            .unwrap_or(list.children.len());
        let entries = entries.into_iter().map(|mut entry| {
            entry.parameters.insert(PART.to_string(), OverseerValue::String(key.clone()));
            entry
        });
        list.children.splice(at..at, entries);
    }
    left.reverse();
    left_stamps.reverse();
    list.parameters.insert(FOLDER.to_string(), OverseerValue::String(folder.to_string_lossy().into_owned()));
    list.parameters.insert(READ.to_string(), OverseerValue::String(read_lines.join("\n")));
    if problems.is_empty() {
        list.parameters.remove(PROBLEM);
    } else {
        list.parameters.insert(PROBLEM.to_string(), OverseerValue::String(problems.join("; ")));
    }
    set_unread(list, address, &left, &left_stamps);
}

/// The parts of a list not read yet, oldest first - see `bring_in`.
pub fn unread_of(list: &OverseerNode) -> Vec<String> {
    match list.parameters.get(UNREAD) {
        Some(OverseerValue::String(keys)) => keys.lines().map(str::to_string).collect(),
        _ => Vec::new(),
    }
}

/// Whether a list has parts it has not read, and so does not hold every entry it has.
pub fn leaves_parts_unread(list: &OverseerNode) -> bool {
    list.parameters.contains_key(UNREAD)
}

fn set_unread(list: &mut OverseerNode, address: &str, keys: &[String], stamps: &[String]) {
    if keys.is_empty() {
        for said in [UNREAD, UNREAD_COUNT, ADDRESS, UNREAD_STAMPS] {
            list.parameters.remove(said);
        }
        return;
    }
    list.parameters.insert(UNREAD.to_string(), OverseerValue::String(keys.join("\n")));
    list.parameters.insert(UNREAD_COUNT.to_string(), OverseerValue::Integer(keys.len() as i64));
    list.parameters.insert(ADDRESS.to_string(), OverseerValue::String(address.to_string()));
    list.parameters.insert(UNREAD_STAMPS.to_string(), OverseerValue::String(stamps.join("\n")));
}

/// The parts of a list not read yet, oldest first, each as its file and how it stood when the
/// list was read - see `UNREAD_STAMPS`.
pub fn unread_stamps(list: &OverseerNode) -> impl Iterator<Item = (&str, &str)> {
    let lines = match list.parameters.get(UNREAD_STAMPS) {
        Some(OverseerValue::String(lines)) if leaves_parts_unread(list) => lines.as_str(),
        _ => "",
    };
    lines.lines().filter_map(|line| line.split_once('\t'))
}

/// The entries a part holds, parsed and not made from their template. `None` when it cannot be
/// read, as for `bring_in`.
pub fn entries_in(file: &Path) -> Option<Vec<OverseerNode>> {
    let (_text, mut nodes) = read(file)?;
    Some(std::mem::take(&mut list_in(&mut nodes)?.children))
}

/// The address a list with parts not read was worked out at, for what reaches past what it holds
/// to name it - see `resolver::note_missed_under`.
pub fn address_of_unread(list: &OverseerNode) -> Option<&str> {
    match list.parameters.get(ADDRESS) {
        Some(OverseerValue::String(address)) if leaves_parts_unread(list) => Some(address.as_str()),
        _ => None,
    }
}

/// A file or folder read, and how it stood, as a line of `READ`.
fn stamp_line(path: &Path) -> String {
    let named = path.to_string_lossy().into_owned();
    let stamp = crate::actions::mount_stamp(&named);
    format!("{}\t{}", named, stamp)
}

/// Every part file a document's lists were read from, and how each stood then - beside what its
/// mounts were read from, in `actions::mounts_held`.
pub fn held(node: &OverseerNode, out: &mut Vec<(String, String)>) {
    if let Some(OverseerValue::String(lines)) = node.parameters.get(READ) {
        for line in lines.lines() {
            if let Some((file, stamp)) = line.split_once('\t') {
                if !out.iter().any(|(f, s)| f == file && s == stamp) {
                    out.push((file.to_string(), stamp.to_string()));
                }
            }
        }
    }
}

/// The entries a part file holds: everything of the list but what its template gave. An entry
/// made from its template takes the template's type, so it is not told by its type.
fn is_an_entry(child: &OverseerNode) -> bool {
    !crate::resolver::is_a_given_entry(child)
}

/// Every list with parts a write may have changed, by where it is in the tree.
fn lists_with_parts(nodes: &[OverseerNode]) -> Vec<Vec<usize>> {
    fn walk(nodes: &[OverseerNode], trail: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        for (at, node) in nodes.iter().enumerate() {
            trail.push(at);
            if node.node_type == "mount" {
                // A mounted file's lists are that file's, and it was read without its parts.
            } else {
                if node.parameters.contains_key(FOLDER) && key_of(node).is_some() {
                    out.push(trail.clone());
                }
                walk(&node.children, trail, out);
            }
            trail.pop();
        }
    }
    let mut out = Vec::new();
    walk(nodes, &mut Vec::new(), &mut out);
    out
}

fn at<'a>(nodes: &'a [OverseerNode], indices: &[usize]) -> Option<&'a OverseerNode> {
    let (first, rest) = indices.split_first()?;
    let mut node = nodes.get(*first)?;
    for i in rest {
        node = node.children.get(*i)?;
    }
    Some(node)
}

fn at_mut<'a>(nodes: &'a mut [OverseerNode], indices: &[usize]) -> Option<&'a mut OverseerNode> {
    let (first, rest) = indices.split_first()?;
    let mut node = nodes.get_mut(*first)?;
    for i in rest {
        node = node.children.get_mut(*i)?;
    }
    Some(node)
}

/// What a write left in a document's parts: each part file whose text it changed, with the text
/// that file should now say, as a write through a mount is said - see `actions::mounts_written`.
///
/// Each entry of a list the write reached is given the part its key names now. Those are left out
/// of the document's own text by the writer, so this is what puts an entry in its file - a new one,
/// one whose key changed, or one the document held before the list had parts. An entry whose key
/// cannot be worked out, or cannot name a file, stays in the document, and the list says why.
///
/// Which lists a write reached is said by the report, as for mounts; every one when the shape moved
/// in a way the report cannot say, or when there is no report. Called once the document is
/// settled, so a key reads what the write left.
pub fn written(nodes: &mut Vec<OverseerNode>, report: Option<&crate::actions::Changed>) -> Vec<crate::actions::MountWrite> {
    let lists = lists_with_parts(nodes);
    if lists.is_empty() {
        return Vec::new();
    }
    let every_one = report.is_none_or(|changed| changed.shape_unknown);
    // Two lists of one name keep their parts in one folder, and each reads the other's entries as
    // its own. Neither is written to its parts until one of them is renamed.
    let folders: Vec<Option<OverseerValue>> =
        lists.iter().map(|place| at(nodes, place).and_then(|l| l.parameters.get(FOLDER).cloned())).collect();
    let mut out = Vec::new();
    for (place, indices) in lists.into_iter().enumerate() {
        if folders.iter().enumerate().any(|(other, f)| other != place && *f == folders[place]) {
            if let Some(list) = at_mut(nodes, &indices) {
                let why = format!("another list named '{}' keeps its parts in the same folder", list.name);
                list.parameters.insert(PROBLEM.to_string(), OverseerValue::String(why));
            }
            continue;
        }
        let address = crate::actions::ActionExecutor::address_of(nodes, &indices);
        if !every_one {
            let under = |reached: &str| reached == address || reached.starts_with(&format!("{}/", address));
            let changed = report.expect("a report, or every list is looked at");
            if !changed.fields.iter().any(|f| under(f)) && !changed.shapes.iter().any(|s| under(s.list())) {
                continue;
            }
        }
        let Some(keys) = keys_of(nodes, &indices, &address) else { continue };
        // A part the list has not read, that an entry is now to go into, is read in first: it is
        // written from the entries the list holds of it.
        let into_unread: Vec<String> = match at(nodes, &indices) {
            Some(list) if leaves_parts_unread(list) => {
                let unread = unread_of(list);
                let mut hit: Vec<String> = keys
                    .iter()
                    .filter_map(|k| k.as_ref().and_then(|k| k.as_ref().ok()))
                    .filter(|k| unread.contains(k))
                    .cloned()
                    .collect();
                hit.sort();
                hit.dedup();
                hit
            }
            _ => Vec::new(),
        };
        let keys = if into_unread.is_empty() {
            keys
        } else {
            let Some(list) = at_mut(nodes, &indices) else { continue };
            bring_in_for_a_write(list, &into_unread);
            let Some(keys) = keys_of(nodes, &indices, &address) else { continue };
            keys
        };
        let Some(list) = at_mut(nodes, &indices) else { continue };
        let Some(OverseerValue::String(folder)) = list.parameters.get(FOLDER).cloned() else { continue };
        let folder = PathBuf::from(folder);
        let mut problems = Vec::new();
        // Whether an entry the document's own text held leaves it for a part, or one from a part
        // stays in it. The writer gives back a list as it read it while every entry it read still
        // says what it did, so it has to be told.
        let mut the_documents_entries_moved = false;
        // The parts there were, and the parts there are now: one left with no entries is written
        // empty rather than taken away, so that undo only ever has a file's text to put back.
        //
        // Each entry with the part it was read from, if any: what ties it to that part's text. Where
        // it was read is said by `source_id` as well, but a document held on disk between writes
        // comes back with its parses numbered afresh, and no longer matches the part's own.
        let mut groups: Vec<(String, Vec<(usize, Option<String>)>)> =
            {
                // A part the list has not read is no part of this write: it holds none of its entries.
                let unread = unread_of(list);
                parts_in(&folder).into_iter().filter(|(key, _)| !unread.contains(key)).map(|(key, _)| (key, Vec::new())).collect()
            };
        let reached = entries_reached(list, &address, report.filter(|_| !every_one));
        for (at, (child, key)) in list.children.iter_mut().zip(keys).enumerate() {
            match key {
                None => {}
                Some(Ok(key)) => {
                    // Read from the document's text: one an action has just made was never in it.
                    the_documents_entries_moved |= !child.parameters.contains_key(PART) && child.source_snapshot.is_some();
                    let came_from = match child.parameters.insert(PART.to_string(), OverseerValue::String(key.clone())) {
                        Some(OverseerValue::String(was)) => Some(was),
                        _ => None,
                    };
                    let entries = match groups.iter_mut().find(|(k, _)| *k == key) {
                        Some((_, entries)) => entries,
                        None => {
                            groups.push((key, Vec::new()));
                            &mut groups.last_mut().expect("just pushed").1
                        }
                    };
                    entries.push((at, came_from));
                }
                Some(Err(why)) => {
                    // One read from a part is written into the document as a new entry: where it
                    // was read is the part's text.
                    if child.parameters.remove(PART).is_some() {
                        forget_where_it_was_read(child);
                        the_documents_entries_moved = true;
                    }
                    let named = child.name.clone();
                    problems.push(format!("'{}' stays in the document: {}", named, why));
                }
            }
        }
        if problems.is_empty() {
            list.parameters.remove(PROBLEM);
        } else {
            list.parameters.insert(PROBLEM.to_string(), OverseerValue::String(problems.join("; ")));
        }
        if the_documents_entries_moved {
            list.source_fingerprint = None;
        }
        let name = list.name.clone();
        for (key, members) in groups {
            let file = folder.join(format!("{}.os", key));
            // A part holding just the entries it was read with, none of which the write reached,
            // is left alone without being written out again to be compared: an append reaches a
            // week, and a history is a great many of them.
            //
            // The same entries are those read from it, as many as it holds: one gone from it, by
            // a move or a removal, leaves fewer. A reorder is a change of shape the report cannot
            // follow, which reaches every entry.
            let same_entries = members.iter().all(|(_, from)| from.as_deref() == Some(key.as_str()))
                && entries_read(&file) == Some(members.len());
            if same_entries && !reached.every && !members.iter().any(|(at, _)| reached.entries[*at]) {
                continue;
            }
            // A part that is there but cannot be read - a merge left half-done, say - is never
            // written over: what it holds would be lost. What would have gone into it stays in the
            // document until it reads again.
            if file.exists() && read(&file).is_none_or(|(_, mut nodes)| list_in(&mut nodes).is_none()) {
                for (at, from) in &members {
                    let entry = &mut list.children[*at];
                    entry.parameters.remove(PART);
                    if from.is_some() {
                        forget_where_it_was_read(entry);
                        list.source_fingerprint = None;
                    }
                }
                if !members.is_empty() {
                    let why = format!("the part '{}' could not be read, so what belongs in it stays in the document", key);
                    let said = match list.parameters.get(PROBLEM) {
                        Some(OverseerValue::String(before)) => format!("{}; {}", before, why),
                        _ => why,
                    };
                    list.parameters.insert(PROBLEM.to_string(), OverseerValue::String(said));
                }
                continue;
            }
            let entries: Vec<OverseerNode> = members
                .iter()
                .map(|(at, from)| {
                    let mut entry = list.children[*at].clone();
                    entry.parameters.remove(PART);
                    // An entry that comes from another file - the document, or another part - is
                    // written as a new one. Where it was read is that file's text, and means
                    // nothing in this one.
                    if from.as_deref() != Some(key.as_str()) {
                        forget_where_it_was_read(&mut entry);
                    }
                    entry
                })
                .collect();
            let (was, mut part) = match read(&file) {
                Some((text, nodes)) => (text, nodes),
                None => {
                    let fresh = format!("list {} {{\n}}\n", name);
                    let (_rest, nodes) = match crate::parser::parse_document(&fresh) {
                        Ok(parsed) => parsed,
                        Err(_) => continue,
                    };
                    (String::new(), nodes)
                }
            };
            let Some(held) = list_in(&mut part) else { continue };
            let emptied = entries.is_empty();
            // The list is written again unless it holds exactly the entries it was read with: the
            // writer gives a list back as it read it while each entry left says what it did.
            if !same_entries {
                held.source_fingerprint = None;
            }
            held.children = entries;
            // A list emptied is written as one made empty: the writer, given a list it read with
            // entries and no entries left, puts back the text it read.
            let text = if emptied {
                format!("list {} {{\n}}\n", name)
            } else {
                match crate::file_ops::OverseerFileHandler::serialize_nodes(&part) {
                    Ok(text) => text,
                    Err(_) => continue,
                }
            };
            if text == was || (was.is_empty() && held_nothing(&part)) {
                continue;
            }
            out.push(crate::actions::MountWrite {
                mount: name.clone(),
                file: file.to_string_lossy().into_owned(),
                text,
                writable: true,
            });
        }
    }
    out
}

/// Each entry's key, worked out where the entry is, as `sort_by` is: `None` for what is no entry,
/// and why not for an entry whose key cannot be had.
///
/// An entry out of view that nothing has worked out keeps the part it was read from. It holds only
/// what it was written with, so a key reading a field its template gives would come out wrong,
/// and nothing has changed it: whatever reaches it works it out first.
fn keys_of(nodes: &[OverseerNode], indices: &[usize], address: &str) -> Option<Vec<Option<Result<String, String>>>> {
    let list = at(nodes, indices)?;
    let lambda = key_of(list)?;
    let trail: Vec<String> = address.split('/').map(|s| s.to_string()).collect();
    Some(
        list.children
            .iter()
            .map(|child| {
                if !is_an_entry(child) {
                    return None;
                }
                if crate::resolver::not_worked_out(child) {
                    if let Some(OverseerValue::String(part)) = child.parameters.get(PART) {
                        return Some(Ok(part.clone()));
                    }
                }
                let mut path = trail.clone();
                path.push(child.name.clone());
                let context = crate::formula_evaluator::EvaluationContext::new_with_current_and_parent(
                    child,
                    Some(list),
                    path,
                    nodes,
                );
                Some(
                    match crate::formula_evaluator::FormulaEvaluator::evaluate_lambda_on_item(lambda, &context, child) {
                        Ok(value) => {
                            let key = plain(&value);
                            if usable(&key) {
                                Ok(key)
                            } else {
                                Err(format!("the part '{}' cannot name a file", key))
                            }
                        }
                        Err(e) => Err(format!("its part could not be worked out: {}", e)),
                    },
                )
            })
            .collect(),
    )
}

/// Which entries of a list a write reached, by the report: a field under one, by its name or its
/// key. `every` when there is no report to go by, or it names something under the list that is no
/// entry it can tell.
struct Reached {
    every: bool,
    entries: Vec<bool>,
}

fn entries_reached(list: &OverseerNode, address: &str, report: Option<&crate::actions::Changed>) -> Reached {
    let mut reached = Reached { every: report.is_none(), entries: vec![false; list.children.len()] };
    let Some(changed) = report else { return reached };
    let key_field = match list.parameters.get("key") {
        Some(OverseerValue::String(k)) => Some(k.as_str()),
        _ => None,
    };
    let keys: Vec<Option<String>> = list
        .children
        .iter()
        .map(|entry| {
            let field = entry.children.iter().find(|c| Some(c.name.as_str()) == key_field)?;
            let value = field.parameters.get("_computed_value").or_else(|| field.parameters.get("value"))?;
            Some(format!("[{}]", plain(value)))
        })
        .collect();
    let within = format!("{}/", address);
    for field in &changed.fields {
        let Some(rest) = field.strip_prefix(&within) else {
            if field == address {
                reached.every = true;
            }
            continue;
        };
        let step = rest.split('/').next().unwrap_or_default();
        let mut found = false;
        for (at, entry) in list.children.iter().enumerate() {
            if entry.name == step || keys[at].as_deref() == Some(step) {
                reached.entries[at] = true;
                found = true;
            }
        }
        reached.every |= !found;
    }
    reached
}

/// How many entries a part was last read with - without the copy `read` makes.
fn entries_read(file: &Path) -> Option<usize> {
    let stamp = crate::actions::mount_stamp(&file.to_string_lossy());
    let count = |nodes: &[OverseerNode]| nodes.iter().find(|n| n.node_type == "list").map(|l| l.children.len());
    if let Ok(held) = READ_PARTS.lock() {
        if let Some((was, _text, nodes)) = held.get(file) {
            if *was == stamp {
                return count(nodes);
            }
        }
    }
    read(file).and_then(|(_text, nodes)| count(&nodes))
}

/// Let go of where a node and everything in it were read, so it is written from what it holds.
fn forget_where_it_was_read(node: &mut OverseerNode) {
    node.source_snapshot = None;
    node.source_id = None;
    node.source_fingerprint = None;
    for child in node.children.iter_mut() {
        forget_where_it_was_read(child);
    }
}

/// Whether a part about to be made holds nothing, and so need not be made.
fn held_nothing(part: &[OverseerNode]) -> bool {
    part.iter().find(|n| n.node_type == "list").is_some_and(|l| l.children.is_empty())
}

/// A key as text: what a formula gave, written the way it would be read.
fn plain(value: &OverseerValue) -> String {
    match value {
        OverseerValue::String(s) | OverseerValue::Date(s) | OverseerValue::Timestamp(s) => s.clone(),
        OverseerValue::Integer(n) => n.to_string(),
        OverseerValue::Float(f) if f.fract() == 0.0 => format!("{}", *f as i64),
        OverseerValue::Float(f) => f.to_string(),
        OverseerValue::Boolean(b) => b.to_string(),
        _ => String::new(),
    }
}

/// Say on each list that its parts now stand as they were just written, so the document held
/// worked out is not let go for files it wrote itself.
pub fn restamp(nodes: &mut [OverseerNode], wrote: &[crate::undo::Along]) {
    if wrote.is_empty() {
        return;
    }
    fn walk(nodes: &mut [OverseerNode], wrote: &[crate::undo::Along]) {
        for node in nodes.iter_mut() {
            if node.node_type == "mount" {
                continue;
            }
            if let Some(OverseerValue::String(folder)) = node.parameters.get(FOLDER).cloned() {
                let folder = PathBuf::from(folder);
                if wrote.iter().any(|w| Path::new(&w.file).parent() == Some(folder.as_path())) {
                    let lines: Vec<String> = std::iter::once(stamp_line(&folder))
                        .chain(parts_in(&folder).into_iter().map(|(_, file)| stamp_line(&file)))
                        .collect();
                    node.parameters.insert(READ.to_string(), OverseerValue::String(lines.join("\n")));
                }
            }
            walk(&mut node.children, wrote);
        }
    }
    walk(nodes, wrote);
}

/// Whether an entry lives in a part, and so is no part of its document's own text.
pub fn lives_in_a_part(entry: &OverseerNode) -> bool {
    entry.parameters.contains_key(PART)
}

/// Whether anything in the tree lives in a part.
pub fn holds_one(nodes: &[OverseerNode]) -> bool {
    nodes.iter().any(|node| lives_in_a_part(node) || holds_one(&node.children))
}

/// Take out of a tree about to be written as its document's text every entry a part holds.
///
/// A list left with nothing of the document's own is no edit of the document's. An append marks
/// the list it went into as written here, and in an instance of another file's template that made
/// the writer give the instance a `list History` it never said.
pub fn leave_out(nodes: &mut Vec<OverseerNode>) {
    for node in nodes.iter_mut() {
        if node.node_type == "list" && key_of(node).is_some() && node.children.iter().any(lives_in_a_part) {
            node.children.retain(|entry| !lives_in_a_part(entry));
            if node.children.is_empty() {
                for mark in ["_override_present", "_explicit_child_override", crate::file_ops::EDITED_HERE] {
                    node.parameters.remove(mark);
                }
            }
        }
        leave_out(&mut node.children);
    }
}
