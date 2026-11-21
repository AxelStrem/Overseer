//! A calendar's history grows a day at a time and is read a few days at a time. A written day
//! that neither the calendar's range nor any view's holds is kept as it was written - no template
//! copied onto it, no formula of it worked out - until something reaches it: `at(date)`, a total
//! over the days, a press or a server read naming it. Then it is worked out as any day in view.

use crate::common::{serial, shared};
use overseer::app_api;
use overseer::server::DocumentRoot;
use overseer::types::*;

const DAYS: i64 = 100;

fn day_name(n: i64) -> String {
    let first = chrono::NaiveDate::from_ymd_opt(2026, 7, 1).unwrap();
    (first + chrono::Duration::days(n)).format("%Y-%m-%d").to_string()
}

/// Day `n` holds two meals, of `n` and `2n` calories ten times over.
fn doc() -> String {
    let mut days = String::new();
    for n in 0..DAYS {
        days.push_str(&format!(
            "        - {} {{\n            list intake {{\n                - {{\n                    - calories = {}\n                }}\n                - {{\n                    - calories = {}\n                }}\n            }}\n        }}\n",
            day_name(n),
            n * 10,
            n * 20
        ));
    }
    format!(
        r#"tab t (label="T", mutable=true) {{
    div (hidden=true) {{
        div Meal {{
            string name = ""
            int calories = 0
            int kj = $(calories * 4)
            string size = $(calories > 500 ? "big" : "small")
        }}
        div Day {{
            string note = ""
            list intake (entry=<Meal>) {{ }}
            int eaten = $(intake.sum(|m| m/calories))
            int kj = $(intake.sum(|m| m/kj))
            int big = $(intake.filter(|m| m/size == "big").count())
        }}
    }}

    calendar Days (entry=<Day>, from="2026-11-02", to="2026-11-08") {{
{days}    }}

    int old = $(Days.at("{old}")/eaten)
    int shown = $(Days.cells().count())

    button note_old (label="note") {{
        on click {{
            set (path="../Days/{noted}/note") = "away"
        }}
    }}
}}
"#,
        days = days,
        old = day_name(45),
        noted = day_name(3),
    )
}

fn reads(node: &OverseerNode) -> Option<OverseerValue> {
    node.parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
        .cloned()
}

fn holds_worked_out_values(node: &OverseerNode) -> bool {
    node.parameters.contains_key("_computed_value") || node.children.iter().any(holds_worked_out_values)
}

#[test]
fn a_written_day_out_of_every_range_is_kept_as_written() {
    let _shared = shared();
    let nodes = app_api::load_document(doc()).expect("resolve");
    let days = overseer::addressing::find(&nodes, "t/Days").expect("the calendar");
    let kept: Vec<&OverseerNode> = days.children.iter().filter(|d| overseer::resolver::not_worked_out(d)).collect();
    // All but the day `old` reads, which its formula reached.
    assert_eq!(kept.len() as i64, DAYS - 1, "days kept as written");
    for day in &kept {
        assert!(day.children.iter().all(|c| c.name != "eaten"), "{} had its template copied onto it", day.name);
        assert!(!holds_worked_out_values(day), "{} had formulas worked out", day.name);
    }
    let shown = overseer::addressing::find(&nodes, "t/Days/2026-11-03/eaten").expect("a day in view");
    assert_eq!(reads(shown), Some(OverseerValue::Integer(0)));
}

#[test]
fn at_reaches_a_day_kept_as_written() {
    let _shared = shared();
    let nodes = app_api::load_document(doc()).expect("resolve");
    let old = overseer::addressing::find(&nodes, "t/old").expect("old");
    assert_eq!(reads(old), Some(OverseerValue::Integer(45 * 30)));
    let reached = overseer::addressing::find(&nodes, &format!("t/Days/{}/kj", day_name(45))).expect("the day reached");
    assert_eq!(reads(reached), Some(OverseerValue::Integer(45 * 30 * 4)));
    let shown = overseer::addressing::find(&nodes, "t/shown").expect("shown");
    assert_eq!(reads(shown), Some(OverseerValue::Integer(7)), "a reached day came into view");
}

#[test]
fn a_document_with_days_kept_as_written_saves_as_it_was_read() {
    let _shared = shared();
    let text = doc();
    let nodes = app_api::load_document(text.clone()).expect("resolve");
    let again = overseer::file_ops::OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
    assert_eq!(again, text, "the document moved when nothing changed");
}


fn a_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_kept_days_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("t.os"), doc()).unwrap();
    app_api::forget_baseline();
    overseer::viewstate::forget_all();
    root
}

#[test]
fn the_server_reads_and_writes_a_day_kept_as_written() {
    let _guard = serial();
    let root = a_root("server");
    let service = DocumentRoot::new(&root).expect("open the root");
    let eaten = service.read_at("t.os", &format!("t/Days/{}/eaten", day_name(10))).expect("the read was refused");
    assert_eq!(reads(&eaten.node), Some(OverseerValue::Integer(300)));
    assert_eq!(std::fs::read_to_string(root.join("t.os")).unwrap(), doc(), "a read wrote the file");

    let mut meal = std::collections::HashMap::new();
    meal.insert("calories".to_string(), OverseerValue::Integer(7));
    service
        .append_at("t.os", &format!("t/Days/{}/intake", day_name(20)), &meal)
        .expect("the append was refused");
    let text = std::fs::read_to_string(root.join("t.os")).unwrap();
    assert_eq!(text.matches(&format!("- {} {{", day_name(20))).count(), 1, "the day was written twice:\n{}", text);
    assert_eq!(text.matches("- calories = 7\n").count(), 1, "the meal was not written:\n{}", text);
    let eaten = service.read_at("t.os", &format!("t/Days/{}/eaten", day_name(20))).expect("the read was refused");
    assert_eq!(reads(&eaten.node), Some(OverseerValue::Integer(607)));
    assert_eq!(text.lines().count(), doc().lines().count() + 3, "more than the meal changed:\n{}", text);
}

#[test]
fn a_press_writes_to_a_day_kept_as_written() {
    let _guard = serial();
    let root = a_root("press");
    let file = root.join("t.os");
    let path = file.to_string_lossy().to_string();
    let nodes = app_api::load_document(doc()).expect("open");
    let node_path = overseer::addressing::name_path(&nodes, "t/note_old").expect("the button");
    app_api::run_event_at(&path, "t.os", node_path, "click".into(), "s", Vec::new(), Vec::new())
        .expect("the press was refused");
    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(text.matches("- note = \"away\"").count(), 1, "the note was not written:\n{}", text);
    assert_eq!(text.matches(&format!("- {} {{", day_name(3))).count(), 1, "the day was written twice:\n{}", text);
    assert_eq!(text.lines().count(), doc().lines().count() + 1, "more than the note changed:\n{}", text);
}
