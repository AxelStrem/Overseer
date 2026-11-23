//! A day nobody wrote reads as the calendar's template - through a mount too.
//!
//! The mount brings in the calendar alone; its template stays in a hidden div of its own file.
//! A host reading `FOOD/Days.at(date)` for a day the file never wrote still gets the template's
//! figures, as the file itself does, and through a view of the calendar the same.

use crate::common::serial;
use overseer::server::DocumentRoot;
use overseer::types::*;

const FOOD: &str = r#"int goal = 2000
div (hidden=true) {
    div Day {
        int kcal = 0
        int target = $(goal)
    }
}
tab food (label="Food") {
    calendar Days (entry=<Day>, span="day", at="2026-08-03", layout="day") {
        - 2026-08-03 {
            - kcal = 1500
        }
    }
    calendar Week (of="food/Days", span="week", at="2026-08-03") { }
    int own_unwritten = $(Days.at("2026-08-01")/kcal)
}
"#;

const HOST: &str = r#"tab host (label="Host") {
    mount FOOD (source="food.os/food/Days", lazy=false, mutable=false, hidden=true) { }
    mount WEEK (source="food.os/food/Week", lazy=false, mutable=false, hidden=true) { }
    int written = $(FOOD/Days.at("2026-08-03")/kcal)
    int unwritten = $(FOOD/Days.at("2026-08-01")/kcal)
    int chosen = $(FOOD/Days.at("2026-08-01")/kcal > 0 ? 1 : 2)
    int target = $(FOOD/Days.at("2026-08-01")/target)
    int through_a_view = $(WEEK/Week.at("2026-08-01")/kcal)
    int written_through_a_view = $(WEEK/Week.at("2026-08-03")/kcal)
}
"#;


struct Sandbox(std::path::PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sandbox(tag: &str) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_mounted_calendar_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("food.os"), FOOD).unwrap();
    std::fs::write(root.join("host.os"), HOST).unwrap();
    overseer::app_api::forget_baseline();
    overseer::app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let documents = DocumentRoot::new(&root).unwrap();
    (Sandbox(root), documents)
}

fn number_at(nodes: &[OverseerNode], address: &str) -> Option<f64> {
    let node = overseer::addressing::find(nodes, address)?;
    match node.parameters.get("_computed_value").or_else(|| node.parameters.get("value"))? {
        OverseerValue::Integer(i) => Some(*i as f64),
        OverseerValue::Float(f) => Some(*f),
        _ => None,
    }
}

#[test]
fn the_file_itself_reads_a_day_nobody_wrote_as_its_template() {
    let _serial = serial();
    let (_sandbox, documents) = sandbox("own");
    let food = documents.open("food.os").expect("open");
    assert_eq!(number_at(&food, "food/own_unwritten"), Some(0.0));
}

#[test]
fn a_written_day_reads_as_written_through_a_mount() {
    let _serial = serial();
    let (_sandbox, documents) = sandbox("written");
    let host = documents.open("host.os").expect("open");
    assert_eq!(number_at(&host, "host/written"), Some(1500.0));
}

#[test]
fn a_day_nobody_wrote_reads_as_the_template_in_its_own_file() {
    let _serial = serial();
    let (_sandbox, documents) = sandbox("unwritten");
    let host = documents.open("host.os").expect("open");
    assert_eq!(number_at(&host, "host/unwritten"), Some(0.0));
    assert_eq!(number_at(&host, "host/chosen"), Some(2.0));
}

#[test]
fn a_template_formula_is_worked_out_in_its_own_file() {
    let _serial = serial();
    let (_sandbox, documents) = sandbox("formula");
    let host = documents.open("host.os").expect("open");
    assert_eq!(number_at(&host, "host/target"), Some(2000.0));
}

#[test]
fn a_mounted_view_reads_its_calendars_days_in_its_own_file() {
    let _serial = serial();
    let (_sandbox, documents) = sandbox("view");
    let host = documents.open("host.os").expect("open");
    assert_eq!(number_at(&host, "host/written_through_a_view"), Some(1500.0));
    assert_eq!(number_at(&host, "host/through_a_view"), Some(0.0));
}

// A day written out of the mounted calendar's range is kept as written in its own file until
// something reaches it; reached through the mount, it is worked out there with its template.
const LOG: &str = r#"tab log {
    div (hidden=true) {
        div Meal {
            float got = 0
        }
        div DayRecord {
            float a = 0
            float b = $(a * 2)
            list intake (entry=<Meal>) { }
        }
    }
    calendar Days (entry=<DayRecord>, span="day", at="2026-11-22") {
        - 2026-11-05 {
            - a = 4
            list intake {
                - {
                    - got = 300
                }
                - {
                    - got = 200
                }
            }
        }
        - 2026-11-22 {
            - a = 4
        }
    }
}
"#;

const READER: &str = r#"tab reader {
    mount FOOD (source="log.os/log/Days", lazy=false, mutable=false, hidden=true) { }
    div Day (hidden=true) {
        float a = $(FOOD/Days.at(date())/a)
        float b = $(FOOD/Days.at(date())/b)
        float eaten = $(FOOD/Days.at(date())/intake.map(|x| x/got).sum())
    }
    calendar Days (entry=<Day>, span="month", at="2026-11-24", layout="month") { }
    float old_b = $(FOOD/Days.at("2026-11-05")/b)
    float old_eaten = $(FOOD/Days.at("2026-11-05")/intake.map(|x| x/got).sum())
}
"#;

fn out_of_range_sandbox(tag: &str) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_mounted_calendar_range_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("log.os"), LOG).unwrap();
    std::fs::write(root.join("reader.os"), READER).unwrap();
    overseer::app_api::forget_baseline();
    overseer::app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let documents = DocumentRoot::new(&root).unwrap();
    (Sandbox(root), documents)
}

#[test]
fn a_written_day_out_of_a_mounted_calendars_range_works_out_its_formulas_when_read() {
    let _serial = serial();
    let (_sandbox, documents) = out_of_range_sandbox("formula");
    let reader = documents.open("reader.os").expect("open");
    assert_eq!(number_at(&reader, "reader/old_b"), Some(8.0));
    assert_eq!(number_at(&reader, "reader/Days/2026-11-05/a"), Some(4.0));
    assert_eq!(number_at(&reader, "reader/Days/2026-11-05/b"), Some(8.0));
}

#[test]
fn a_written_day_out_of_a_mounted_calendars_range_reads_its_own_list_when_read() {
    let _serial = serial();
    let (_sandbox, documents) = out_of_range_sandbox("list");
    let reader = documents.open("reader.os").expect("open");
    assert_eq!(number_at(&reader, "reader/old_eaten"), Some(500.0));
    assert_eq!(number_at(&reader, "reader/Days/2026-11-05/eaten"), Some(500.0));
}

#[test]
fn a_written_day_in_a_mounted_calendars_range_reads_as_before() {
    let _serial = serial();
    let (_sandbox, documents) = out_of_range_sandbox("in_range");
    let reader = documents.open("reader.os").expect("open");
    assert_eq!(number_at(&reader, "reader/Days/2026-11-22/b"), Some(8.0));
    assert_eq!(number_at(&reader, "reader/Days/2026-11-21/b"), Some(0.0));
}
