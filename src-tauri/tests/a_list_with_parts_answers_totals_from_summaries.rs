//! A total over a windowed list with parts - a count, a sum, the latest date, the parts of one
//! item - is answered from the entries the list holds and a summary of each part it has not read,
//! so the list keeps reading only its newest parts. Every answer is the one reading every part
//! gives. A formula no summary answers still reads every part, and the list says which one did.

use overseer::server::DocumentRoot;
use overseer::types::*;
use std::collections::HashMap;
use std::path::PathBuf;

const TEMPLATE: &str = r#"    div (hidden=true) {
        div Done (layout="horizontal") {
            string handle = ""
            string parent = ""
            string status = "done"
            timestamp done_at = "2026-11-02T10:00:00Z"
            int points = 1
            int twice = $(points * 2)
            bool failed (hidden=$(failed == false), font-color=$(failed ? "red" : "")) = false {
                on change {
                    set (path="../points") = $(../points + 1)
                }
            }
        }
        div Item {
            string handle = ""
            int kids = $(/log/History.filter(|x| x/parent == ../handle && x/status != "cancelled").count())
            int weight = $(/log/History.filter(|x| x/parent == ../handle).map(|x| x/points).sum())
        }
    }
"#;

const TOTALS: &str = r#"    int count (mutable=false) = $(History.count())
    int points (mutable=false) = $(History.sum(|x| x/points))
    int won (mutable=false) = $(History.filter(|x| x/status != "cancelled").map(|x| x/points).sum())
    timestamp latest (mutable=false) = $(History.map(|x| x/done_at).max())
    timestamp earliest (mutable=false) = $(History.map(|x| x/done_at).min())
    float mean (mutable=false) = $(History.avg(|x| x/points))
    string next (mutable=false) = $(fresh_key(Items, History))
"#;

fn host(extra: &str, window: bool) -> String {
    let window = if window { "window=1, " } else { "" };
    format!(
        r#"tab log (label="Log", mutable=true) {{
{TEMPLATE}{extra}
    list Items (entry=<Item>, key="handle") {{
        - {{
            - handle = "p"
        }}
        - {{
            - handle = "q"
        }}
    }}

    list History (entry=<Done>, key="handle", {window}parts=$(|x| week(x/done_at)),
                  sort_by=$(|x| 0 - millis_since_epoch(x/done_at))) {{ }}
}}
"#
    )
}

const OLDEST: &str = r#"list History {
    - {
        - handle = "a"
        - parent = "p"
        - done_at = "2026-10-20T08:00:00Z"
        - points = 3
        - failed = true
    }
    - {
        - handle = "d"
        - parent = "q"
        - status = "cancelled"
        - done_at = "2026-10-21T08:00:00Z"
        - points = 8
    }
}
"#;

const OLDER: &str = r#"list History {
    - {
        - handle = "c"
        - parent = "p"
        - done_at = "2026-10-27T08:00:00Z"
        - points = 4
    }
}
"#;

const THIS_WEEK: &str = r#"list History {
    - {
        - handle = "b"
        - parent = "q"
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

fn sandbox(tag: &str, document: &str) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_partsums_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("log.parts/History")).unwrap();
    std::fs::write(root.join("log.os"), document).unwrap();
    std::fs::write(root.join("log.parts/History/2026-W43.os"), OLDEST).unwrap();
    std::fs::write(root.join("log.parts/History/2026-W44.os"), OLDER).unwrap();
    std::fs::write(root.join("log.parts/History/2026-W45.os"), THIS_WEEK).unwrap();
    overseer::app_api::forget_baseline();
    overseer::app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let documents = DocumentRoot::new(&root).unwrap();
    (Sandbox(root), documents)
}

fn value(nodes: &[OverseerNode], address: &str) -> Option<OverseerValue> {
    let params = &overseer::addressing::find(nodes, address)?.parameters;
    params.get("_computed_value").or_else(|| params.get("value")).cloned()
}

fn values(nodes: &[OverseerNode], addresses: &[&str]) -> Vec<(String, Option<OverseerValue>)> {
    addresses.iter().map(|a| (a.to_string(), value(nodes, a))).collect()
}

fn held(nodes: &[OverseerNode]) -> Vec<String> {
    let list = overseer::addressing::find(nodes, "log/History").expect("the list");
    list.children
        .iter()
        .filter_map(|e| e.children.iter().find(|c| c.name == "handle"))
        .filter_map(|h| match h.parameters.get("value") {
            Some(OverseerValue::String(s)) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

fn said(nodes: &[OverseerNode], key: &str) -> Option<OverseerValue> {
    overseer::addressing::find(nodes, "log/History")?.parameters.get(key).cloned()
}

const ANSWERS: &[&str] = &[
    "log/count",
    "log/points",
    "log/won",
    "log/latest",
    "log/earliest",
    "log/mean",
    "log/next",
    "log/Items/[p]/kids",
    "log/Items/[q]/kids",
    "log/Items/[p]/weight",
    "log/Items/[q]/weight",
];

/// What the document answers with its list read whole, by a document that has no window.
fn read_whole(tag: &str, extra: &str) -> Vec<(String, Option<OverseerValue>)> {
    let (_sandbox, documents) = sandbox(tag, &host(extra, false));
    let nodes = documents.open("log.os").expect("open");
    assert_eq!(held(&nodes), vec!["a", "d", "c", "b"]);
    values(&nodes, ANSWERS)
}

#[test]
fn totals_over_the_parts_not_read_are_those_of_the_list_read_whole() {
    serialised(|| {
        let whole = read_whole("whole", TOTALS);
        let (_sandbox, documents) = sandbox("totals", &host(TOTALS, true));
        let nodes = documents.open("log.os").expect("open");
        assert_eq!(held(&nodes), vec!["b"], "only this week is read");
        assert_eq!(said(&nodes, "_parts_read_whole"), None);
        assert_eq!(values(&nodes, ANSWERS), whole);
        assert_eq!(value(&nodes, "log/count"), Some(OverseerValue::Integer(4)));
        assert_eq!(value(&nodes, "log/won"), Some(OverseerValue::Integer(12)));
        assert_eq!(value(&nodes, "log/Items/[p]/kids"), Some(OverseerValue::Integer(2)));
        assert_eq!(value(&nodes, "log/Items/[q]/weight"), Some(OverseerValue::Integer(13)));
    });
}

#[test]
fn a_part_is_summarised_once_while_it_stands_as_it_did() {
    serialised(|| {
        let (_sandbox, documents) = sandbox("once", &host(TOTALS, true));
        documents.open("log.os").expect("open");
        let made = overseer::partsums::summaries_made();
        // A document worked out again from nothing, as after the server let it go.
        overseer::app_api::forget_baseline();
        overseer::app_api::forget_dependencies();
        let again = DocumentRoot::new(&_sandbox.0).unwrap();
        let nodes = again.open("log.os").expect("open again");
        assert_eq!(overseer::partsums::summaries_made(), made, "no part was summarised again");
        assert_eq!(value(&nodes, "log/points"), Some(OverseerValue::Integer(20)));
    });
}

#[test]
fn a_part_changed_on_disk_is_summarised_again() {
    serialised(|| {
        let (sandbox, documents) = sandbox("changed", &host(TOTALS, true));
        documents.open("log.os").expect("open");
        std::fs::write(sandbox.0.join("log.parts/History/2026-W44.os"), OLDER.replace("points = 4", "points = 40")).unwrap();
        let nodes = documents.open("log.os").expect("open again");
        assert_eq!(held(&nodes), vec!["b"]);
        assert_eq!(value(&nodes, "log/points"), Some(OverseerValue::Integer(56)));
        assert_eq!(value(&nodes, "log/Items/[p]/weight"), Some(OverseerValue::Integer(43)));
    });
}

#[test]
fn writes_reach_the_totals() {
    serialised(|| {
        let (_sandbox, documents) = sandbox("writes", &host(TOTALS, true));
        documents.open("log.os").expect("open");
        documents.set_at("log.os", "log/History/[c]/points", OverseerValue::Integer(7)).expect("a set in a part not read");
        let nodes = documents.open("log.os").expect("open");
        assert_eq!(value(&nodes, "log/points"), Some(OverseerValue::Integer(23)));
        assert_eq!(value(&nodes, "log/Items/[p]/weight"), Some(OverseerValue::Integer(10)));

        let added: HashMap<String, OverseerValue> = [
            ("handle".to_string(), OverseerValue::String("e".to_string())),
            ("parent".to_string(), OverseerValue::String("p".to_string())),
            ("done_at".to_string(), OverseerValue::Timestamp("2026-11-04T12:00:00Z".to_string())),
            ("points".to_string(), OverseerValue::Integer(2)),
        ]
        .into_iter()
        .collect();
        documents.append_at("log.os", "log/History", &added).expect("an append to this week");
        let nodes = documents.open("log.os").expect("open");
        assert_eq!(value(&nodes, "log/count"), Some(OverseerValue::Integer(5)));
        assert_eq!(value(&nodes, "log/Items/[p]/kids"), Some(OverseerValue::Integer(3)));
        assert_eq!(value(&nodes, "log/latest"), Some(OverseerValue::Timestamp("2026-11-04T12:00:00Z".to_string())));

        documents.undo("log.os").expect("undo");
        documents.undo("log.os").expect("undo");
        let nodes = documents.open("log.os").expect("open");
        assert_eq!(value(&nodes, "log/points"), Some(OverseerValue::Integer(20)), "two undos put the totals back");
    });
}

#[test]
fn a_lambda_reading_the_clock_reads_every_part_and_says_so() {
    serialised(|| {
        let extra = "    int old (mutable=false) = $(History.filter(|x| days_since(x/done_at) > 0).count())\n";
        let (_sandbox, documents) = sandbox("clock", &host(extra, true));
        let nodes = documents.open("log.os").expect("open");
        assert_eq!(held(&nodes), vec!["a", "d", "c", "b"]);
        let Some(OverseerValue::String(why)) = said(&nodes, "_parts_read_whole") else {
            panic!("the list says it was read whole");
        };
        assert!(why.contains("days_since(x/done_at)"), "and which formula did it: {}", why);
    });
}

#[test]
fn a_lambda_reading_the_document_reads_every_part() {
    serialised(|| {
        let extra = "    int cap (mutable=false) = 4\n    int big (mutable=false) = $(History.filter(|x| x/points > ../cap).count())\n";
        let (_sandbox, documents) = sandbox("outside", &host(extra, true));
        let nodes = documents.open("log.os").expect("open");
        assert_eq!(held(&nodes), vec!["a", "d", "c", "b"]);
        assert_eq!(value(&nodes, "log/big"), Some(OverseerValue::Integer(2)));
    });
}

#[test]
fn a_field_worked_out_by_a_formula_is_read_from_every_part() {
    serialised(|| {
        let extra = "    int doubled (mutable=false) = $(History.map(|x| x/twice).sum())\n";
        let (_sandbox, documents) = sandbox("formula", &host(extra, true));
        let nodes = documents.open("log.os").expect("open");
        assert_eq!(value(&nodes, "log/doubled"), Some(OverseerValue::Integer(40)));
        assert_eq!(held(&nodes), vec!["a", "d", "c", "b"], "the parts are read once their summary cannot answer");
    });
}

#[test]
fn a_formula_beside_a_fields_value_does_not_stop_its_summary() {
    serialised(|| {
        let extra = "    int kept (mutable=false) = $(History.filter(|x| x/failed == false).count())\n";
        let (_sandbox, documents) = sandbox("beside", &host(extra, true));
        let nodes = documents.open("log.os").expect("open");
        assert_eq!(held(&nodes), vec!["b"], "only this week is read");
        assert_eq!(said(&nodes, "_parts_read_whole"), None);
        assert_eq!(value(&nodes, "log/kept"), Some(OverseerValue::Integer(3)));
    });
}
