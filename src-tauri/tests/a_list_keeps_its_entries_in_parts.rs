//! A list with `parts` keeps each entry in the file its key names, beside the document, and the
//! document holds none of them.
//!
//! A history grows without end and most of it is never read again, but held in the document it is
//! read, written and backed up with every change. In parts, `History` is still one list - read and
//! written at the same addresses - while each week of it is a file of its own.

use overseer::app_api;
use overseer::server::DocumentRoot;
use overseer::types::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const HOST: &str = r#"tab log (label="Log", mutable=true) {
    div (hidden=true) {
        div Done (layout="horizontal") {
            string handle = ""
            timestamp done_at = "2026-11-02T10:00:00Z"
            int points = 1
            int twice = $(points * 2)
        }
    }

    int total (mutable=false) = $(History.sum(|x| x/points))

    list History (entry=<Done>, key="handle", parts=$(|x| week(x/done_at))) { }

    button redo (label="Move a to this week") {
        on click {
            set_in_list (list="../History", keyField="handle", keyValue="a", field="done_at") = "2026-11-03T09:00:00Z"
        }
    }
}
"#;

const OLD_WEEK: &str = r#"list History {
    - {
        - handle = "a"
        - done_at = "2026-10-27T08:00:00Z"
        - points = 3
    }
}
"#;

const THIS_WEEK: &str = r#"list History {
    - {
        - handle = "b"
        - points = 5
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

struct Sandbox(PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sandbox(tag: &str, host: &str) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_parts_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("log.parts/History")).unwrap();
    std::fs::write(root.join("log.os"), host).unwrap();
    std::fs::write(root.join("log.parts/History/2026-W44.os"), OLD_WEEK).unwrap();
    std::fs::write(root.join("log.parts/History/2026-W45.os"), THIS_WEEK).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let documents = DocumentRoot::new(&root).unwrap();
    (Sandbox(root), documents)
}

fn on_disk(root: &Path, name: &str) -> String {
    std::fs::read_to_string(root.join(name)).unwrap_or_else(|_| "(no file)".to_string())
}

fn value(nodes: &[OverseerNode], address: &str) -> Option<OverseerValue> {
    let params = &overseer::addressing::find(nodes, address)?.parameters;
    params.get("_computed_value").or_else(|| params.get("value")).cloned()
}

fn done(handle: &str, at: &str, points: i64) -> HashMap<String, OverseerValue> {
    [
        ("handle".to_string(), OverseerValue::String(handle.to_string())),
        ("done_at".to_string(), OverseerValue::Timestamp(at.to_string())),
        ("points".to_string(), OverseerValue::Integer(points)),
    ]
    .into_iter()
    .collect()
}

#[test]
fn a_list_with_parts_opens_with_every_parts_entries() {
    serialised(|| {
        let (_sandbox, documents) = sandbox("open", HOST);
        let nodes = documents.open("log.os").expect("open");
        assert_eq!(value(&nodes, "log/History/[a]/twice"), Some(OverseerValue::Integer(6)), "an entry of a part is made from the template");
        assert_eq!(value(&nodes, "log/History/[b]/points"), Some(OverseerValue::Integer(5)));
        assert_eq!(value(&nodes, "log/total"), Some(OverseerValue::Integer(8)), "formulas read every part");
        let list = overseer::addressing::find(&nodes, "log/History").expect("the list");
        let handles: Vec<_> = list
            .children
            .iter()
            .filter_map(|e| e.children.iter().find(|c| c.name == "handle"))
            .filter_map(|h| h.parameters.get("value").cloned())
            .collect();
        assert_eq!(
            handles,
            vec![OverseerValue::String("a".into()), OverseerValue::String("b".into())],
            "parts come in the order of their keys"
        );
    });
}

#[test]
fn an_append_goes_to_the_part_its_key_names() {
    serialised(|| {
        let (sandbox, documents) = sandbox("append", HOST);
        documents.append_at("log.os", "log/History", &done("c", "2026-11-04T12:00:00Z", 2)).expect("the append");
        assert_eq!(on_disk(&sandbox.0, "log.os"), HOST, "the document holds none of its entries");
        assert_eq!(on_disk(&sandbox.0, "log.parts/History/2026-W44.os"), OLD_WEEK, "a part the write did not reach is left as it was");
        assert_eq!(
            on_disk(&sandbox.0, "log.parts/History/2026-W45.os"),
            "list History {\n    - {\n        - handle = \"b\"\n        - points = 5\n    }\n    - {\n        - handle = \"c\"\n        - done_at = \"2026-11-04T12:00:00Z\"\n        - points = 2\n    }\n}\n",
        );
        let nodes = documents.open("log.os").expect("reopen");
        assert_eq!(value(&nodes, "log/total"), Some(OverseerValue::Integer(10)));
    });
}

#[test]
fn an_append_to_a_new_period_makes_its_part_and_undo_takes_it_away() {
    serialised(|| {
        let (sandbox, documents) = sandbox("new", HOST);
        documents.append_at("log.os", "log/History", &done("d", "2026-11-10T12:00:00Z", 1)).expect("the append");
        assert!(
            on_disk(&sandbox.0, "log.parts/History/2026-W46.os").contains("- handle = \"d\""),
            "a new week starts a new file"
        );
        documents.undo("log.os").expect("undo");
        assert!(!sandbox.0.join("log.parts/History/2026-W46.os").exists(), "undo takes away the part it made");
        assert_eq!(on_disk(&sandbox.0, "log.os"), HOST);
        let nodes = documents.open("log.os").expect("reopen");
        assert!(overseer::addressing::find(&nodes, "log/History/[d]").is_none());
    });
}

#[test]
fn an_entry_whose_key_changes_moves_to_the_other_part_in_one_step() {
    serialised(|| {
        let (sandbox, documents) = sandbox("move", HOST);
        documents.run_event("log.os", "log/redo", "click").expect("the press");
        let old_week = on_disk(&sandbox.0, "log.parts/History/2026-W44.os");
        let this_week = on_disk(&sandbox.0, "log.parts/History/2026-W45.os");
        assert!(!old_week.contains("\"a\""), "it left the old week: {}", old_week);
        assert_eq!(old_week, "list History {
}
", "it left the old week");
        assert_eq!(
            this_week,
            "list History {
    - {
        - handle = \"a\"
        - done_at = \"2026-11-03T09:00:00Z\"
        - points = 3
    }
    - {
        - handle = \"b\"
        - points = 5
    }
}
",
            "and is in this one, in the list's order"
        );
        assert_eq!(on_disk(&sandbox.0, "log.os"), HOST);

        documents.undo("log.os").expect("undo");
        assert_eq!(on_disk(&sandbox.0, "log.parts/History/2026-W44.os"), OLD_WEEK, "one undo puts both back");
        assert_eq!(on_disk(&sandbox.0, "log.parts/History/2026-W45.os"), THIS_WEEK);
    });
}

#[test]
fn a_set_inside_an_entry_writes_only_its_part() {
    serialised(|| {
        let (sandbox, documents) = sandbox("set", HOST);
        documents.set_at("log.os", "log/History/[b]/points", OverseerValue::Integer(7)).expect("the set");
        assert_eq!(on_disk(&sandbox.0, "log.parts/History/2026-W45.os"), THIS_WEEK.replace("5", "7"));
        assert_eq!(on_disk(&sandbox.0, "log.parts/History/2026-W44.os"), OLD_WEEK);
        assert_eq!(on_disk(&sandbox.0, "log.os"), HOST);
        let nodes = documents.open("log.os").expect("reopen");
        assert_eq!(value(&nodes, "log/total"), Some(OverseerValue::Integer(10)));
    });
}

#[test]
fn entries_the_document_held_go_to_their_parts_when_the_list_is_written() {
    serialised(|| {
        let held = HOST.replace(
            "parts=$(|x| week(x/done_at))) { }",
            "parts=$(|x| week(x/done_at))) {\n        - {\n            - handle = \"e\"\n            - done_at = \"2026-10-28T08:00:00Z\"\n        }\n    }",
        );
        let (sandbox, documents) = sandbox("held", &held);
        let nodes = documents.open("log.os").expect("open");
        assert_eq!(value(&nodes, "log/total"), Some(OverseerValue::Integer(9)), "the document's own entries and the parts' are one list");
        documents.set_at("log.os", "log/History/[b]/points", OverseerValue::Integer(6)).expect("a write to the list");
        assert_eq!(
            on_disk(&sandbox.0, "log.os"),
            HOST.replace("parts=$(|x| week(x/done_at))) { }", "parts=$(|x| week(x/done_at))) {
    }"),
            "the document's entry left it, as a last entry taken out of a list does"
        );
        assert_eq!(
            on_disk(&sandbox.0, "log.parts/History/2026-W44.os"),
            OLD_WEEK.replace("list History {
", "list History {
    - {
        - handle = \"e\"
        - done_at = \"2026-10-28T08:00:00Z\"
    }
"),
            "for its part, in the list's order: the document's own entries came first"
        );
    });
}

#[test]
fn an_entry_with_no_usable_key_stays_in_the_document() {
    serialised(|| {
        let (sandbox, documents) = sandbox("nokey", HOST);
        let mut odd = done("f", "", 1);
        odd.insert("done_at".to_string(), OverseerValue::String("not a moment".to_string()));
        documents.append_at("log.os", "log/History", &odd).expect("the append");
        assert!(on_disk(&sandbox.0, "log.os").contains("- handle = \"f\""), "it stays where it can be seen");
        let nodes = documents.open("log.os").expect("reopen");
        let list = overseer::addressing::find(&nodes, "log/History").expect("the list");
        assert!(
            matches!(list.parameters.get(overseer::parts::PROBLEM), Some(OverseerValue::String(why)) if why.contains("in the document itself")),
            "and the list says why: {:?}",
            list.parameters.get(overseer::parts::PROBLEM)
        );
    });
}

#[test]
fn a_part_changed_on_disk_is_read_again() {
    serialised(|| {
        let (sandbox, documents) = sandbox("stale", HOST);
        documents.open("log.os").expect("held worked out");
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(sandbox.0.join("log.parts/History/2026-W45.os"), THIS_WEEK.replace("5", "50")).unwrap();
        let nodes = documents.open("log.os").expect("reopen");
        assert_eq!(value(&nodes, "log/total"), Some(OverseerValue::Integer(53)));
    });
}

#[test]
fn a_write_to_one_entry_leaves_the_rest_of_its_part_as_written() {
    serialised(|| {
        let (sandbox, documents) = sandbox("layout", HOST);
        let laid_out = "list History {\n    - {\n        - handle = \"b\"\n        - points = 5\n    }\n    - { - handle = \"h\"\n        - points = 2 }\n}\n";
        std::fs::write(sandbox.0.join("log.parts/History/2026-W45.os"), laid_out).unwrap();
        documents.open("log.os").expect("held worked out");
        documents.set_at("log.os", "log/History/[h]/points", OverseerValue::Integer(4)).expect("the first set");
        documents.set_at("log.os", "log/History/[h]/points", OverseerValue::Integer(3)).expect("a second, from what the first left");
        assert_eq!(
            on_disk(&sandbox.0, "log.parts/History/2026-W45.os"),
            laid_out.replace("- points = 2 }", "- points = 3 }"),
            "the other entry is as it was, and this one as it was written but for its points"
        );
        assert_eq!(on_disk(&sandbox.0, "log.parts/History/2026-W44.os"), OLD_WEEK);
    });
}

#[test]
fn a_part_that_cannot_be_read_is_never_written_over() {
    serialised(|| {
        let (sandbox, documents) = sandbox("broken", HOST);
        let broken = "list History {\n<<<<<<< ours\n    - {\n        - handle = \"x\"\n";
        std::fs::write(sandbox.0.join("log.parts/History/2026-W46.os"), broken).unwrap();
        let nodes = documents.open("log.os").expect("the document still opens");
        let list = overseer::addressing::find(&nodes, "log/History").expect("the list");
        assert!(
            matches!(list.parameters.get(overseer::parts::PROBLEM), Some(OverseerValue::String(why)) if why.contains("2026-W46")),
            "and the list names the part: {:?}",
            list.parameters.get(overseer::parts::PROBLEM)
        );
        documents.append_at("log.os", "log/History", &done("d", "2026-11-10T12:00:00Z", 1)).expect("the append");
        assert_eq!(on_disk(&sandbox.0, "log.parts/History/2026-W46.os"), broken, "what it holds is not lost");
        assert!(on_disk(&sandbox.0, "log.os").contains("- handle = \"d\""), "the new entry waits in the document");
    });
}

#[test]
fn an_entry_whose_key_stops_naming_a_part_comes_back_into_the_document() {
    serialised(|| {
        let (sandbox, documents) = sandbox("comesback", HOST);
        documents
            .set_at("log.os", "log/History/[a]/done_at", OverseerValue::String("not a moment".into()))
            .expect("the set");
        let host = on_disk(&sandbox.0, "log.os");
        assert!(
            host.contains("- handle = \"a\"") && host.contains("- done_at = \"not a moment\"") && host.contains("- points = 3"),
            "it is written into the document whole: {}",
            host
        );
        assert_eq!(on_disk(&sandbox.0, "log.parts/History/2026-W44.os"), "list History {\n}\n", "and has left its part");
        let nodes = documents.open("log.os").expect("reopen");
        assert_eq!(value(&nodes, "log/total"), Some(OverseerValue::Integer(8)), "nothing is lost on the way");
    });
}

#[test]
fn parts_are_not_listed_as_documents() {
    serialised(|| {
        let (_sandbox, documents) = sandbox("listed", HOST);
        assert_eq!(documents.list(), vec!["log.os".to_string()]);
    });
}

#[test]
fn a_part_that_appears_on_disk_is_read_in() {
    serialised(|| {
        let (sandbox, documents) = sandbox("appears", HOST);
        documents.open("log.os").expect("held worked out");
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(sandbox.0.join("log.parts/History/2026-W46.os"), THIS_WEEK.replace("\"b\"", "\"g\"")).unwrap();
        let nodes = documents.open("log.os").expect("reopen");
        assert_eq!(value(&nodes, "log/total"), Some(OverseerValue::Integer(13)), "a new week brought by a sync is read");
    });
}

#[test]
fn a_document_saved_untouched_leaves_every_file_as_it_was() {
    serialised(|| {
        let (sandbox, documents) = sandbox("same", HOST);
        documents.run_event("log.os", "log/redo", "click").expect("the press");
        documents.undo("log.os").expect("undo");
        assert_eq!(on_disk(&sandbox.0, "log.os"), HOST);
        assert_eq!(on_disk(&sandbox.0, "log.parts/History/2026-W44.os"), OLD_WEEK);
        assert_eq!(on_disk(&sandbox.0, "log.parts/History/2026-W45.os"), THIS_WEEK);
    });
}

#[test]
fn the_period_functions_name_the_day_week_month_and_year() {
    let text = r#"div d {
    string a = $(day("2026-11-03"))
    string b = $(week("2026-11-03"))
    string c = $(month("2026-11-03"))
    string e = $(year("2026-11-03"))
    string f = $(week("2026-12-31"))
    string g = $(week("2027-01-04"))
    string h = $(week("2027-01-01"))
}
"#;
    let nodes = app_api::load_document(text.to_string()).expect("load");
    let said = |name: &str| value(&nodes, &format!("d/{}", name));
    assert_eq!(said("a"), Some(OverseerValue::String("2026-11-03".into())));
    assert_eq!(said("b"), Some(OverseerValue::String("2026-W45".into())));
    assert_eq!(said("c"), Some(OverseerValue::String("2026-11".into())));
    assert_eq!(said("e"), Some(OverseerValue::String("2026".into())));
    assert_eq!(said("f"), Some(OverseerValue::String("2026-W53".into())));
    assert_eq!(said("g"), Some(OverseerValue::String("2027-W01".into())));
    assert_eq!(said("h"), Some(OverseerValue::String("2026-W53".into())), "a week belongs to the year of its Thursday");
}

const BASE: &str = r#"div (hidden=true) {
    div Done (layout="horizontal") {
        string handle = ""
        timestamp done_at = "2026-11-02T10:00:00Z"
        int points = 1
    }
    tab log (label="Log", mutable=true) {
        string name = ""
        int total (mutable=false) = $(History.sum(|x| x/points))
        list History (entry=<Done>, key="handle", parts=$(|x| week(x/done_at))) { }
    }
}
"#;

const INSTANCE: &str = r#"<base.os/log> log {
    - name = "Mine"
}
"#;

#[test]
fn a_list_a_template_gives_keeps_its_entries_in_the_instances_parts() {
    serialised(|| {
        let (sandbox, documents) = sandbox("instance", INSTANCE);
        std::fs::write(sandbox.0.join("base.os"), BASE).unwrap();
        let nodes = documents.open("log.os").expect("open");
        assert_eq!(value(&nodes, "log/total"), Some(OverseerValue::Integer(8)), "the instance reads its own parts");
        documents.append_at("log.os", "log/History", &done("c", "2026-11-04T12:00:00Z", 2)).expect("the append");
        assert_eq!(on_disk(&sandbox.0, "log.os"), INSTANCE, "the instance holds none of the entries");
        assert!(on_disk(&sandbox.0, "log.parts/History/2026-W45.os").contains("- handle = \"c\""));
        assert_eq!(on_disk(&sandbox.0, "base.os"), BASE);
        let nodes = documents.open("log.os").expect("reopen");
        assert_eq!(value(&nodes, "log/total"), Some(OverseerValue::Integer(10)));
    });
}

#[test]
fn entries_an_instance_held_go_to_their_parts() {
    serialised(|| {
        let held = "<base.os/log> log {\n    - name = \"Mine\"\n    - History {\n        - {\n            - handle = \"e\"\n            - done_at = \"2026-10-28T08:00:00Z\"\n        }\n    }\n}\n";
        let (sandbox, documents) = sandbox("instance_held", held);
        std::fs::write(sandbox.0.join("base.os"), BASE).unwrap();
        let nodes = documents.open("log.os").expect("open");
        assert_eq!(value(&nodes, "log/total"), Some(OverseerValue::Integer(9)));
        documents.set_at("log.os", "log/History/[b]/points", OverseerValue::Integer(6)).expect("a write to the list");
        let now = on_disk(&sandbox.0, "log.os");
        assert_eq!(
            now,
            INSTANCE.replace("    - name = \"Mine\"\n", "    - name = \"Mine\"\n    - History {\n    }\n"),
            "the instance's entry left it, as a last entry taken out of a list does"
        );
        assert!(on_disk(&sandbox.0, "log.parts/History/2026-W44.os").contains("- handle = \"e\""));
        let nodes = documents.open("log.os").expect("reopen");
        assert_eq!(value(&nodes, "log/total"), Some(OverseerValue::Integer(10)));
    });
}

#[test]
fn the_source_of_a_document_names_its_parts_and_each_reads_as_its_text() {
    serialised(|| {
        let (sandbox, documents) = sandbox("source", HOST);
        let source = documents.source("log.os").expect("the host's source");
        assert_eq!(source.text, HOST);
        let named = source.parts.get("History").expect("History's parts").clone();
        assert_eq!(
            named,
            vec!["log.parts/History/2026-W44.os".to_string(), "log.parts/History/2026-W45.os".to_string()],
            "in the order of their keys"
        );
        assert_eq!(documents.source(&named[0]).expect("the old week").text, OLD_WEEK);
        assert_eq!(documents.source(&named[1]).expect("this week").text, THIS_WEEK);
        assert!(documents.source(&named[0]).unwrap().parts.is_empty(), "a part has no parts of its own");

        std::fs::write(sandbox.0.join("log.parts/History/2026-W46.os"), THIS_WEEK).unwrap();
        let again = documents.source("log.os").expect("read again");
        assert_eq!(again.version, source.version, "the host's text is as it was");
        assert_ne!(again.tag, source.tag, "but a new part is not hidden by an answer of 'unchanged'");
        assert_eq!(again.parts["History"].len(), 3);
    });
}

#[test]
fn the_source_of_a_document_without_parts_names_none() {
    serialised(|| {
        let (sandbox, documents) = sandbox("noparts", HOST);
        std::fs::write(sandbox.0.join("plain.os"), "div d {\n    int a = 1\n}\n").unwrap();
        let source = documents.source("plain.os").expect("plain's source");
        assert!(source.parts.is_empty());
        assert_eq!(source.tag, source.version);
        assert_eq!(serde_json::to_value(&source).unwrap()["parts"], serde_json::json!({}));
    });
}
