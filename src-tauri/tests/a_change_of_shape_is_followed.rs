//! An entry made or taken out is followed through the dependency graph, not worked out whole.
//!
//! A change of shape used to work the whole document out, because the graph was describing a
//! document that no longer existed: logging a meal cost most of what opening the food tracker
//! does, and marking a task done cost it twice, once between its two actions. What moves is less
//! than it looked. An entry made is new, and only its list has been read; an entry taken out takes
//! its values with it, and what read them is in the graph; and a list naming its entries by place
//! renames every one after the change - a rename, not a change of value. So the graph is asked
//! what the list and the removed entry reach, the entries are named as the text names them and the
//! graph carried along, and what it named is worked out, with every node of an entry just made.
//!
//! Each case checks the answer, that a later edit still reaches everything it should through the
//! renamed entries, and that the document ends up as a fresh open of the file has it.

use overseer::app_api;
use overseer::resolver::times_worked_out_whole;
use overseer::types::*;

const DOCUMENT: &str = r#"tab t (label="T", mutable=true) {
    div (hidden=true) {
        div Meal (layout="horizontal") {
            float grams = 0
            float calories = $(grams * 1.5)
            float share = $(calories * 100 / /t/total)
        }
        div Task (layout="horizontal") {
            string added = ""
            int size = 1
            int of_all = $(/t/open_size)
            button done (label="done") {
                on click {
                    append (list="/t/history") {
                        - added = $(../added)
                        - size = $(../size)
                    }
                    remove (from="/t/open", keyField="added", keyValue=$(../added))
                }
            }
        }
        div Record (layout="horizontal") {
            string added = ""
            int size = 0
        }
    }

    list intake (entry=<Meal>) {
        - {
            - grams = 100
        }
        - {
            - grams = 200
        }
        - {
            - grams = 300
        }
    }
    float total = $(intake.map(|x| x/calories).sum())

    list open (entry=<Task>, key="added") {
        - {
            - added = "a"
            - size = 1
        }
        - {
            - added = "b"
            - size = 2
        }
        - {
            - added = "c"
            - size = 4
        }
    }
    int open_size = $(open.map(|x| x/size).sum())

    list history (entry=<Record>) {
    }
    int done_size = $(history.map(|x| x/size).sum())

    list recent (entry=<Meal>, window=2) {
        - {
            - grams = 1
        }
        - {
            - grams = 2
        }
        - {
            - grams = 3
        }
    }
    float recent_total = $(recent.map(|x| x/grams).sum())

    button first (label="first") {
        on click {
            prepend (list="/t/intake") {
                - grams = 50
            }
        }
    }
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

/// A file holding the document, and the document as the page holds it once opened.
fn opened(tag: &str) -> (String, Vec<OverseerNode>) {
    let root = std::env::temp_dir().join(format!("overseer_shape_followed_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("d.os");
    std::fs::write(&file, DOCUMENT).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    (file.to_string_lossy().to_string(), nodes)
}

/// Take an answer on, the way the page does.
fn take(page: &mut Vec<OverseerNode>, answer: app_api::ResolvedUpdate) {
    if let Some(whole) = answer.nodes {
        *page = whole;
        return;
    }
    overseer::delta::apply(page, answer.changes.expect("an answer describes something"));
}

fn number(page: &[OverseerNode], address: &str) -> Option<f64> {
    match overseer::addressing::find(page, address)?.parameters.get("_computed_value") {
        Some(OverseerValue::Integer(i)) => Some(*i as f64),
        Some(OverseerValue::Float(f)) => Some(*f),
        _ => None,
    }
}

fn name_path(page: &[OverseerNode], address: &str) -> Vec<String> {
    overseer::addressing::name_path(page, address).unwrap_or_else(|| panic!("nothing at {}", address))
}

fn write(path: &str, page: &mut Vec<OverseerNode>, address: &str, value: OverseerValue) {
    let w: app_api::ValueWrite =
        serde_json::from_value(serde_json::json!({ "node_path": name_path(page, address), "value": value })).unwrap();
    take(page, app_api::write_values_at(path, "d.os", vec![w], "s").expect("the write"));
}

fn press(path: &str, page: &mut Vec<OverseerNode>, address: &str) {
    let button = name_path(page, address);
    take(page, app_api::run_event_at(path, "d.os", button, "click".into(), "s", Vec::new(), Vec::new()).expect("the press"));
}

/// Every worked-out value and every name, by address - and what is in view, but nothing under an
/// entry that is not: a fresh open leaves such an entry as written, and one that has just left
/// view is still instantiated. Neither is shown or worked out.
fn worked_out(nodes: &[OverseerNode]) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    let mut hidden: Vec<String> = Vec::new();
    overseer::addressing::walk(nodes, &mut |address, node| {
        if hidden.iter().any(|h| address.starts_with(&format!("{}/", h))) {
            return;
        }
        if overseer::resolver::out_of_view(node) {
            hidden.push(address.to_string());
        }
        out.insert(address.to_string(), node.name.clone());
        for (k, v) in &node.parameters {
            if k.starts_with("_computed_") || k == "_ui_sort_key" || k.ends_with("_out_of_view") {
                out.insert(format!("{}#{}", address, k), format!("{:?}", v));
            }
        }
    });
    out
}

/// What the page holds against the file worked out from nothing. Last in a case: it lets go of
/// what the cache holds.
fn agrees_with_a_fresh_open(path: &str, page: &[OverseerNode]) {
    app_api::forget_baseline();
    app_api::forget_dependencies();
    let fresh = app_api::load_document(std::fs::read_to_string(path).unwrap()).expect("a fresh open");
    let (held, fresh) = (worked_out(page), worked_out(&fresh));
    let differ: Vec<_> = held
        .keys()
        .chain(fresh.keys())
        .filter(|k| held.get(*k) != fresh.get(*k))
        .map(|k| format!("{}: {:?} / fresh {:?}", k, held.get(k), fresh.get(k)))
        .collect();
    assert!(differ.is_empty(), "the page and a fresh open disagree:\n{}", differ.join("\n"));
}

#[test]
fn an_entry_taken_out_of_the_middle_is_followed_and_so_are_the_ones_renamed() {
    serialised(|| {
        let (path, mut page) = opened("middle");
        let whole = times_worked_out_whole();
        let second = name_path(&page, "t/intake/Meal__2");
        take(&mut page, app_api::remove_entry_at(&path, "d.os", second, "s").unwrap());
        assert_eq!(times_worked_out_whole(), whole, "worked out whole");
        assert_eq!(number(&page, "t/total"), Some(600.0));

        // The meal that was third is second now, and the graph has to know it by that name: an edit
        // to it reaches its calories, the total, and the share of every meal.
        write(&path, &mut page, "t/intake/Meal__2/grams", OverseerValue::Float(400.0));
        assert_eq!(times_worked_out_whole(), whole, "worked out whole");
        assert_eq!(number(&page, "t/intake/Meal__2/calories"), Some(600.0));
        assert_eq!(number(&page, "t/total"), Some(750.0));
        assert_eq!(number(&page, "t/intake/Meal__1/share"), Some(20.0));
        agrees_with_a_fresh_open(&path, &page);
    });
}

#[test]
fn an_entry_put_in_front_renames_every_other_and_is_followed() {
    serialised(|| {
        let (path, mut page) = opened("front");
        let whole = times_worked_out_whole();
        press(&path, &mut page, "t/first");
        assert_eq!(times_worked_out_whole(), whole, "worked out whole");
        assert_eq!(number(&page, "t/total"), Some(975.0));
        assert_eq!(number(&page, "t/intake/Meal__1/calories"), Some(75.0), "the new meal, first");

        // The meal that was last is fourth now.
        write(&path, &mut page, "t/intake/Meal__4/grams", OverseerValue::Float(100.0));
        assert_eq!(number(&page, "t/intake/Meal__4/calories"), Some(150.0));
        assert_eq!(number(&page, "t/total"), Some(675.0));
        assert_eq!(times_worked_out_whole(), whole, "worked out whole");
        agrees_with_a_fresh_open(&path, &page);
    });
}

#[test]
fn a_press_that_adds_one_entry_and_takes_out_another_is_followed_both_times() {
    // `done` on a task: into the history, then out of the open list. The second action has to
    // find a document it can read, and that used to be a whole resolve between the two.
    serialised(|| {
        let (path, mut page) = opened("done");
        let whole = times_worked_out_whole();
        press(&path, &mut page, "t/open/[b]/done");
        assert_eq!(times_worked_out_whole(), whole, "worked out whole");
        assert_eq!(number(&page, "t/open_size"), Some(5.0));
        assert_eq!(number(&page, "t/done_size"), Some(2.0));
        assert_eq!(number(&page, "t/open/[c]/of_all"), Some(5.0), "a task renamed kept an old figure");

        // The task that was third is second now: its size reaches the total and every task.
        write(&path, &mut page, "t/open/[c]/size", OverseerValue::Integer(10));
        assert_eq!(number(&page, "t/open_size"), Some(11.0));
        assert_eq!(number(&page, "t/open/[a]/of_all"), Some(11.0));
        assert_eq!(number(&page, "t/open/[c]/of_all"), Some(11.0));

        press(&path, &mut page, "t/open/[a]/done");
        assert_eq!(number(&page, "t/done_size"), Some(3.0));
        assert_eq!(number(&page, "t/open/[c]/of_all"), Some(10.0));
        assert_eq!(times_worked_out_whole(), whole, "worked out whole");
        agrees_with_a_fresh_open(&path, &page);
    });
}

#[test]
fn a_list_showing_part_of_itself_is_worked_out_whole() {
    // Which entries are in view is decided by working the document out, so a change to such a
    // list is not followed - and still comes out right.
    serialised(|| {
        let (path, mut page) = opened("window");
        let whole = times_worked_out_whole();
        let fields = [("grams".to_string(), OverseerValue::Float(4.0))].into_iter().collect();
        take(&mut page, app_api::append_entry_at(&path, "d.os", vec!["t".into(), "recent".into()], fields, "s").unwrap());
        assert!(times_worked_out_whole() > whole, "a windowed list was followed");
        assert_eq!(number(&page, "t/recent_total"), Some(10.0));
        agrees_with_a_fresh_open(&path, &page);
    });
}

#[test]
fn worked_out_again_a_list_showing_part_of_itself_shows_the_same_part() {
    // Worked out a second time, the entries in view had been instantiated and were no longer
    // counted as entries at all: one added never left view, the count of those left out fell
    // short, and one that ought to come back into view stayed out. Every change of shape to such
    // a list is worked out whole, so every one met this.
    serialised(|| {
        let (path, mut page) = opened("window_again");
        let second = name_path(&page, "t/recent/Meal__2");
        take(&mut page, app_api::remove_entry_at(&path, "d.os", second, "s").unwrap());
        let shown = |page: &[OverseerNode]| {
            overseer::addressing::find(page, "t/recent")
                .unwrap()
                .children
                .iter()
                .filter(|entry| !overseer::resolver::out_of_view(entry))
                .count()
        };
        assert_eq!(shown(&page), 2, "the entry after the one taken out did not come into view");
        let fields = [("grams".to_string(), OverseerValue::Float(4.0))].into_iter().collect();
        take(&mut page, app_api::append_entry_at(&path, "d.os", vec!["t".into(), "recent".into()], fields, "s").unwrap());
        assert_eq!(shown(&page), 2, "the entry added did not leave view");
        agrees_with_a_fresh_open(&path, &page);
    });
}

#[test]
fn the_bot_follows_a_change_of_shape_too() {
    serialised(|| {
        let root = std::env::temp_dir().join(format!("overseer_shape_followed_bot_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("d.os"), DOCUMENT).unwrap();
        app_api::forget_baseline();
        app_api::forget_dependencies();
        overseer::viewstate::forget_all();
        let documents = overseer::server::DocumentRoot::new(&root).unwrap();
        documents.open("d.os").expect("open");

        let whole = times_worked_out_whole();
        documents.remove_at("d.os", "t/intake/Meal__1", &Default::default()).expect("the removal");
        let fields = [("grams".to_string(), OverseerValue::Float(20.0))].into_iter().collect();
        documents.append_at("d.os", "t/intake", &fields).expect("the append");
        documents.set_at("d.os", "t/intake/Meal__2/grams", OverseerValue::Float(100.0)).expect("the write");
        assert_eq!(times_worked_out_whole(), whole, "worked out whole");

        let held = documents.open("d.os").expect("reopen");
        assert_eq!(number(&held, "t/total"), Some(480.0));
        let file = root.join("d.os").to_string_lossy().to_string();
        agrees_with_a_fresh_open(&file, &held);
        let _ = std::fs::remove_dir_all(&root);
    });
}
