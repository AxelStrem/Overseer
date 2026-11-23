//! A project tracker made from the library's template, with half a year of History in weekly
//! parts, answers every figure of its Stats tab as it would with its History read whole - and,
//! once the parts are summarised, opens without parsing a part its window does not draw: not
//! again on the same server, not on one that has just started with the summaries on disk, and
//! not after an item is finished into this week. A part changed on disk is summarised again
//! alone.

use crate::common::serial;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::server::DocumentRoot;
use overseer::types::{OverseerNode, OverseerValue};
use std::collections::HashMap;
use std::path::PathBuf;

const WEEKS: i64 = 30;
const PER_WEEK: i64 = 20;
const NOW: &str = "2026-11-18T12:00:00Z";
const TRACKER: &str = "ann/projects/p.os";

const PEOPLE: &str = "div (hidden=true) {\n    div Person (layout=\"horizontal\") {\n        string tag = \"\"\n        string name = \"\"\n        string colour = \"#888888\"\n    }\n}\n\nlist People (entry=<Person>, key=\"tag\") {\n    - {\n        - tag = \"ann\"\n        - name = \"Ann\"\n    }\n    - {\n        - tag = \"bob\"\n        - name = \"Bob\"\n    }\n}\n";

const OWNERS: [&str; 3] = ["ann", "bob", ""];
const AUTHORS: [&str; 4] = ["ann", "agent_ann", "bob", ""];
const COMPLEXITIES: [&str; 5] = ["unassigned", "trivial", "low", "medium", "high"];
const LABELS: [&str; 4] = ["ui", "dsl", "perf", "docs"];

fn now() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(NOW).unwrap().with_timezone(&chrono::Utc)
}

/// The `n`th entry finished `week` weeks ago, written as a part or the document writes it.
fn entry(week: i64, n: i64) -> String {
    let at = now() - chrono::Duration::days(7 * week) - chrono::Duration::hours(n);
    let i = (week * PER_WEEK + n) as usize;
    let mut text = format!(
        "        - {{\n            - finished_at = \"{}\"\n            - title = \"Item {}\"\n            - handle = \"h{}\"\n",
        at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        i,
        i
    );
    if i % 7 == 3 {
        text += "            - status = \"cancelled\"\n";
    }
    if !OWNERS[i % 3].is_empty() {
        text += &format!("            - owner = \"{}\"\n", OWNERS[i % 3]);
    }
    if i % 5 != 0 {
        text += &format!("            - complexity = \"{}\"\n", COMPLEXITIES[i % 5]);
    }
    let (first, second) = (LABELS[i % 4], LABELS[(i / 4) % 4]);
    let labels = if first == second { first.to_string() } else { format!("{}, {}", first, second) };
    text += &format!("            - labels = \"{}\"\n", labels);
    if i % 4 == 1 {
        text += "            - milestones = \"alpha\"\n";
    }
    text += &format!("            - points = {}\n", 1 + i % 8);
    if !AUTHORS[i % 4].is_empty() {
        text += &format!(
            "            div Talk {{\n                list Comments {{\n                    - {{\n                        - author = \"owner\"\n                        - body = \"asked\"\n                    }}\n                    - {{\n                        - author = \"{}\"\n                        - body = \"done\"\n                    }}\n                }}\n            }}\n",
            AUTHORS[i % 4]
        );
    }
    if i % 2 == 0 {
        text += &format!(
            "            div Agents {{\n                list Work {{\n                    - {{\n                        - at = \"{}\"\n                        - cost = {}.5\n                        - read = {}\n                        - minutes = 12.5\n                    }}\n                }}\n            }}\n",
            at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            i % 3,
            i % 40
        );
    }
    text + "        }\n"
}

fn week_name(week: i64) -> String {
    let iso = chrono::Datelike::iso_week(&(now() - chrono::Duration::days(7 * week)));
    format!("{:04}-W{:02}", iso.year(), iso.week())
}

fn part(week: i64) -> String {
    let entries: String = (0..PER_WEEK).map(|n| entry(week, n)).collect();
    format!("list History {{\n{}}}\n", entries)
}

/// The tracker, with its History written in it rather than in parts when `history` is given.
fn tracker(history: &str) -> String {
    format!(
        "<projects/project_template.os/project> project (label=\"P\") {{
    - name = \"P\"
    mount people (source=\"../../people.os/People\", hidden=true, lazy=false, mutable=false) {{ }}
    - Labels {{
        - {{
            - tag = \"ui\"
        }}
        - {{
            - tag = \"dsl\"
        }}
        - {{
            - tag = \"perf\"
        }}
        - {{
            - tag = \"docs\"
        }}
    }}
    - Milestones {{
        - {{
            - tag = \"alpha\"
        }}
    }}
    - Items {{
        - {{
            - added = \"2026-10-01T10:00:00+00:00\"
            - handle = \"brace\"
            - owner = \"ann\"
            - title = \"A brace\"
            - labels = \"ui\"
            - points = 3
        }}
        - {{
            - added = \"2026-10-01T10:00:00+00:00\"
            - handle = \"spare\"
            - owner = \"bob\"
            - title = \"Reopened\"
            - points = 2
        }}
    }}
    - History {{
{}    }}
}}
",
        history
    )
}

struct Sandbox(PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
        FormulaEvaluator::set_time_override(None);
        overseer::document_disk::set_override(None);
    }
}

impl Sandbox {
    fn documents(&self) -> PathBuf {
        self.0.join("documents")
    }

    fn part_file(&self, week: i64) -> PathBuf {
        self.documents().join(format!("ann/projects/p.parts/History/{}.os", week_name(week)))
    }
}

/// The tracker and its people in a folder of their own, with a disk cache beside them; History in
/// weekly parts when `parted`, else written whole in the tracker.
fn sandbox(tag: &str, parted: bool) -> Sandbox {
    let root = std::env::temp_dir().join(format!("overseer_largetracker_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let sandbox = Sandbox(root);
    let documents = sandbox.documents();
    std::fs::create_dir_all(documents.join("ann/projects/p.parts/History")).unwrap();
    std::fs::write(documents.join("people.os"), PEOPLE).unwrap();
    if parted {
        std::fs::write(documents.join(TRACKER), tracker("")).unwrap();
        for week in 0..WEEKS {
            std::fs::write(sandbox.part_file(week), part(week)).unwrap();
        }
    } else {
        let whole: String = (0..WEEKS).flat_map(|w| (0..PER_WEEK).map(move |n| entry(w, n))).collect();
        std::fs::write(documents.join(TRACKER), tracker(&whole)).unwrap();
    }
    overseer::document_disk::set_override(Some(Some((sandbox.0.join("cache"), 256 * 1024 * 1024))));
    FormulaEvaluator::set_time_override(Some(now()));
    sandbox
}

/// The document worked out again from nothing, as after the server let it go: the summaries
/// still held in memory unless `restart`, when nothing is but what is on disk.
fn reopen(sandbox: &Sandbox, restart: bool) -> (DocumentRoot, Vec<OverseerNode>) {
    overseer::document_disk::settled();
    if restart {
        overseer::partsums::forget_in_memory();
        overseer::parts::let_go_of_idle_parts(std::time::Duration::ZERO);
    }
    overseer::app_api::forget_baseline();
    overseer::app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let documents = DocumentRoot::new(&sandbox.documents()).unwrap();
    let nodes = documents.open(TRACKER).expect("open");
    (documents, nodes)
}

/// Every value the document worked out outside its History, by the names leading to it.
fn worked_out(nodes: &[OverseerNode]) -> Vec<(String, String)> {
    fn walk(nodes: &[OverseerNode], at: &str, out: &mut Vec<(String, String)>) {
        for node in nodes {
            if node.name == "History" {
                continue;
            }
            let here = if node.name.is_empty() { at.to_string() } else { format!("{}/{}", at, node.name) };
            let mut keys: Vec<_> = node.parameters.keys().filter(|k| k.starts_with("_computed_")).collect();
            keys.sort();
            for key in keys {
                out.push((format!("{} {}", here, key), format!("{:?}", node.parameters[key])));
            }
            walk(&node.children, &here, out);
        }
    }
    let mut out = Vec::new();
    walk(nodes, "", &mut out);
    out
}

fn value(nodes: &[OverseerNode], address: &str) -> Option<OverseerValue> {
    let params = &overseer::addressing::find(nodes, address)?.parameters;
    params.get("_computed_value").or_else(|| params.get("value")).cloned()
}

fn figures(nodes: &[OverseerNode], address: &str) -> serde_json::Value {
    match overseer::addressing::find(nodes, address).and_then(|n| n.parameters.get("_computed_bars")) {
        Some(OverseerValue::String(s)) => serde_json::from_str(s).expect("figures are JSON"),
        other => panic!("no figures at {address}: {other:?}"),
    }
}

/// The amounts a plot over a list gives, by label.
fn amounts(nodes: &[OverseerNode], address: &str) -> HashMap<String, f64> {
    figures(nodes, address)
        .as_array()
        .expect("a list of figures")
        .iter()
        .map(|f| (f["label"].as_str().unwrap_or_default().to_string(), f["amount"].as_f64().unwrap_or(f64::NAN)))
        .collect()
}

fn history_read_whole(nodes: &[OverseerNode]) -> Option<OverseerValue> {
    overseer::addressing::find(nodes, "project/History")?.parameters.get("_parts_read_whole").cloned()
}

#[test]
fn every_stat_of_a_tracker_in_parts_is_what_its_history_read_whole_gives() {
    let _serial = serial();
    let whole = {
        let sandbox = sandbox("whole", false);
        let (_documents, nodes) = reopen(&sandbox, true);
        worked_out(&nodes)
    };
    let sandbox = sandbox("parted", true);
    let (_documents, nodes) = reopen(&sandbox, true);
    let parted = worked_out(&nodes);
    assert!(whole.len() > 100, "the template's figures are there: {}", whole.len());
    let differing: Vec<_> = whole.iter().zip(&parted).filter(|(a, b)| a != b).take(5).collect();
    assert!(differing.is_empty() && whole.len() == parted.len(), "{:#?}", differing);
}

#[test]
fn the_closed_work_charts_count_the_finished_items() {
    let _serial = serial();
    let sandbox = sandbox("closed", true);
    let (_documents, nodes) = reopen(&sandbox, true);
    let all = WEEKS * PER_WEEK;
    let cancelled = (0..all).filter(|i| i % 7 == 3).count() as f64;
    let finished = all as f64 - cancelled;

    let statuses = amounts(&nodes, "project/statuses/each");
    assert_eq!(statuses["finished"], finished);
    assert_eq!(statuses["cancelled"], cancelled);

    let thinking = amounts(&nodes, "project/closed_complexities/each");
    assert_eq!(thinking.values().sum::<f64>(), finished);
    let low = (0..all).filter(|i| i % 7 != 3 && i % 5 == 2).count() as f64;
    assert_eq!(thinking["low"], low);

    let owners = amounts(&nodes, "project/closed_owners/each");
    let anns = (0..all).filter(|i| i % 7 != 3 && i % 3 == 0).count() as f64;
    assert_eq!(owners["Ann"], anns);
    let nobody = value(&nodes, "project/closed_owners/nobody");
    let nobodys = (0..all).filter(|i| i % 7 != 3 && i % 3 == 2).count() as f64;
    let nobody_amount = overseer::addressing::find(&nodes, "project/closed_owners/nobody")
        .and_then(|n| n.parameters.get("_computed_amount").cloned());
    assert!(
        matches!(nobody_amount, Some(OverseerValue::Integer(n)) if n as f64 == nobodys),
        "{:?} {:?}",
        nobody,
        nobody_amount
    );

    let last = amounts(&nodes, "project/closed_last_authors/each");
    let bobs = (0..all).filter(|i| i % 7 != 3 && i % 4 == 2).count() as f64;
    assert_eq!(last["Bob"], bobs);
    let agents = (0..all).filter(|i| i % 7 != 3 && i % 4 == 1).count() as i64;
    let agents_amount = overseer::addressing::find(&nodes, "project/closed_last_authors/agents")
        .and_then(|n| n.parameters.get("_computed_amount").cloned());
    assert_eq!(agents_amount, Some(OverseerValue::Integer(agents)));
}

#[test]
fn an_open_with_the_summaries_made_parses_only_the_parts_its_window_draws() {
    let _serial = serial();
    let sandbox = sandbox("cached", true);
    let made = overseer::partsums::summaries_made();
    let parsed = overseer::parts::parts_parsed();
    let (_documents, nodes) = reopen(&sandbox, true);
    let first = worked_out(&nodes);
    assert_eq!(history_read_whole(&nodes), None, "History is not read whole for any formula");
    assert!(overseer::partsums::summaries_made() > made, "the first open summarises the old parts");
    assert!(overseer::parts::parts_parsed() - parsed >= (WEEKS - 2) as usize, "and parses each of them once");

    let made = overseer::partsums::summaries_made();
    let parsed = overseer::parts::parts_parsed();
    let (_documents, nodes) = reopen(&sandbox, false);
    assert_eq!(overseer::partsums::summaries_made(), made, "the same server summarises nothing again");
    assert_eq!(overseer::parts::parts_parsed(), parsed, "and parses no part again");
    assert_eq!(worked_out(&nodes), first);

    let (_documents, nodes) = reopen(&sandbox, true);
    assert_eq!(overseer::partsums::summaries_made(), made, "a server just started reads the summaries from disk");
    assert!(
        overseer::parts::parts_parsed() - parsed <= 2,
        "and parses only the parts the window draws: {}",
        overseer::parts::parts_parsed() - parsed
    );
    assert_eq!(worked_out(&nodes), first);
}

#[test]
fn an_item_finished_into_this_week_summarises_no_older_part_again() {
    let _serial = serial();
    let sandbox = sandbox("append", true);
    let (documents, nodes) = reopen(&sandbox, true);
    let finished = value(&nodes, "project/tasks_finished");
    let points = value(&nodes, "project/points_finished");

    let made = overseer::partsums::summaries_made();
    let parsed = overseer::parts::parts_parsed();
    let fields: HashMap<String, OverseerValue> = [
        ("handle", OverseerValue::String("fresh".into())),
        ("title", OverseerValue::String("Fresh".into())),
        ("finished_at", OverseerValue::String(NOW.into())),
        ("points", OverseerValue::Integer(5)),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    documents.append_at(TRACKER, "project/History", &fields).expect("append");
    let nodes = documents.open(TRACKER).expect("open after the append");
    assert!(std::fs::read_to_string(sandbox.part_file(0)).unwrap().contains("\"fresh\""));
    assert_eq!(overseer::partsums::summaries_made(), made, "no older part is summarised again");
    assert!(overseer::parts::parts_parsed() - parsed <= 2, "nor parsed: {}", overseer::parts::parts_parsed() - parsed);
    let plus = |v: Option<OverseerValue>, n: i64| match v {
        Some(OverseerValue::Integer(i)) => Some(OverseerValue::Integer(i + n)),
        other => panic!("{:?}", other),
    };
    assert_eq!(value(&nodes, "project/tasks_finished"), plus(finished, 1));
    assert_eq!(value(&nodes, "project/points_finished"), plus(points, 5));

    let (_documents, again) = reopen(&sandbox, true);
    assert_eq!(overseer::partsums::summaries_made(), made, "nor after a restart");
    assert_eq!(worked_out(&again), worked_out(&nodes));
}

#[test]
fn an_old_part_changed_on_disk_is_the_only_one_summarised_again() {
    let _serial = serial();
    let sandbox = sandbox("edit", true);
    reopen(&sandbox, true);

    let made = overseer::partsums::summaries_made();
    std::fs::write(sandbox.part_file(10), part(10).replace("- points = ", "- points = 1")).unwrap();
    let (_documents, nodes) = reopen(&sandbox, false);
    let one = overseer::partsums::summaries_made() - made;
    assert!(one > 0, "the changed part is summarised again");

    let made = overseer::partsums::summaries_made();
    let parsed = overseer::parts::parts_parsed();
    std::fs::write(sandbox.part_file(20), part(20).replace("- points = ", "- points = 1")).unwrap();
    let (_documents, after) = reopen(&sandbox, true);
    assert_eq!(overseer::partsums::summaries_made() - made, one, "after a restart too, that part alone");
    assert!(overseer::parts::parts_parsed() - parsed <= 3, "{}", overseer::parts::parts_parsed() - parsed);
    assert_ne!(value(&nodes, "project/points_finished"), value(&after, "project/points_finished"));
}
