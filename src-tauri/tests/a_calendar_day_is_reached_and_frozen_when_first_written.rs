//! Any day of a calendar can be read and written over the server, as an action can set it: a day
//! out of view nobody wrote answers as the template has it, and a write to it is kept, in date
//! order. A day is made when it is first written - by a press or by the server - and a field of
//! it marked `freeze=true` is written in then, as in a new entry of a list.

use overseer::server::DocumentRoot;
use overseer::types::*;
use serde_json::json;

const DOC: &str = r#"tab t (label="T", mutable=true) {
    int standing = 1800

    div (hidden=true) {
        div Meal {
            string name = ""
            int calories = 0
        }
        div Day {
            string note = ""
            list intake (entry=<Meal>) { }
            int eaten = $(intake.sum(|m| m/calories))
            div targets {
                int target (freeze=true, fallback=$(/t/standing)) = null
            }
        }
    }

    calendar Days (entry=<Day>, from="2026-11-02", to="2026-11-08") {
        - 2026-10-20 {
            list intake {
                - {
                    - name = "soup"
                    - calories = 300
                }
                - {
                    - name = "bread"
                    - calories = 150
                }
            }
        }
    }

    button log (label="log") {
        on click {
            append (list="../Days/2026-11-05/intake") {
                - name = "toast"
                - calories = 200
            }
        }
    }
}
"#;

static SAVING: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn a_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_calendar_writes_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("t.os"), DOC).unwrap();
    overseer::app_api::forget_baseline();
    overseer::viewstate::forget_all();
    root
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("t.os")).unwrap()
}

fn a_meal(name: &str, calories: i64) -> std::collections::HashMap<String, OverseerValue> {
    let mut fields = std::collections::HashMap::new();
    fields.insert("name".to_string(), OverseerValue::String(name.to_string()));
    fields.insert("calories".to_string(), OverseerValue::Integer(calories));
    fields
}

/// What a node reads: its value worked out, else its fallback when it states none.
fn reads(node: &OverseerNode) -> Option<OverseerValue> {
    let held = node.parameters.get("_computed_value").or_else(|| node.parameters.get("value"));
    match held {
        None | Some(OverseerValue::Null) => node.parameters.get("_computed_fallback").cloned(),
        some => some.cloned(),
    }
}

/// A day out of view is worked out only when something reaches it - here, the address read.
fn target_of(text: &str, day: &str) -> Option<OverseerValue> {
    let address = format!("t/Days/{}/targets/target", day);
    let _reached = overseer::resolver::keeping_in_view(&address);
    let nodes = overseer::app_api::load_document(text.to_string()).expect("resolve");
    let node = overseer::addressing::find(&nodes, &address)?;
    reads(node)
}

#[test]
fn the_server_appends_to_a_day_out_of_view_nobody_wrote() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("append");
    let service = DocumentRoot::new(&root).expect("open the root");
    service
        .append_at("t.os", "t/Days/2026-10-25/intake", &a_meal("dinner", 700))
        .expect("the append was refused");
    let text = on_disk(&root);
    let october = text.find("- 2026-10-20").expect("the written day");
    let new = text.find("- 2026-10-25").unwrap_or_else(|| panic!("the day was not written:\n{}", text));
    assert!(october < new, "the day is out of date order:\n{}", text);
    assert!(text.contains("- name = \"dinner\""), "the meal was not written:\n{}", text);
    let eaten = service.read_at("t.os", "t/Days/2026-10-25/eaten").expect("the day");
    assert_eq!(reads(&eaten.node), Some(OverseerValue::Integer(700)));
}

#[test]
fn the_server_sets_a_field_of_a_day_out_of_view_nobody_wrote() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("set");
    let service = DocumentRoot::new(&root).expect("open the root");
    service
        .set_at("t.os", "t/Days/2026-10-26/note", OverseerValue::String("away".into()))
        .expect("the set was refused");
    let text = on_disk(&root);
    assert!(text.contains("- 2026-10-26"), "the day was not written:\n{}", text);
    assert!(text.contains("- note = \"away\""), "the note was not written:\n{}", text);
}

#[test]
fn the_server_reads_a_day_out_of_view_nobody_wrote_as_the_template_has_it() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("read");
    let service = DocumentRoot::new(&root).expect("open the root");
    let eaten = service.read_at("t.os", "t/Days/2026-10-27/eaten").expect("the read was refused");
    assert_eq!(reads(&eaten.node), Some(OverseerValue::Integer(0)));
    let target = service.read_at("t.os", "t/Days/2026-10-27/targets/target").expect("the read was refused");
    assert_eq!(reads(&target.node), Some(OverseerValue::Integer(1800)));
    let written = service.read_at("t.os", "t/Days/2026-10-20/eaten").expect("the read was refused");
    assert_eq!(reads(&written.node), Some(OverseerValue::Integer(450)));
    assert_eq!(on_disk(&root), DOC, "a read wrote the file");
}

#[test]
fn the_server_removes_from_a_written_day_out_of_view() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("remove");
    let service = DocumentRoot::new(&root).expect("open the root");
    service
        .remove_at("t.os", "t/Days/2026-10-20/intake/Meal__1", &std::collections::HashMap::new())
        .expect("the remove was refused");
    let text = on_disk(&root);
    assert!(!text.contains("soup"), "the meal is still there:\n{}", text);
    assert!(text.contains("bread"), "the other meal went too:\n{}", text);
}

#[test]
fn a_day_a_press_makes_freezes_its_fields() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("press");
    let service = DocumentRoot::new(&root).expect("open the root");
    let nodes = service.open("t.os").expect("open");
    let path = overseer::addressing::name_path(&nodes, "t/log").expect("the button");
    service
        .command(Some("t.os"), "run_overseer_event", &json!({ "node_path": path, "event_name": "click" }))
        .expect("the press was refused");
    let text = on_disk(&root);
    assert!(text.contains("- 2026-11-05"), "the day was not written:\n{}", text);
    assert!(text.contains("- target = 1800"), "the day did not freeze its target:\n{}", text);
    assert!(text.contains("- name = \"toast\""), "the meal was not written:\n{}", text);
    let nodes = overseer::app_api::load_document(text.clone()).expect("resolve");
    let again = overseer::file_ops::OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
    assert_eq!(again, text, "the document moved when nothing changed");
}

#[test]
fn a_day_the_server_makes_freezes_its_fields_and_keeps_them() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("frozen");
    let service = DocumentRoot::new(&root).expect("open the root");
    service
        .append_at("t.os", "t/Days/2026-10-25/intake", &a_meal("dinner", 700))
        .expect("the append was refused");
    assert!(on_disk(&root).contains("- target = 1800"), "the day did not freeze its target:\n{}", on_disk(&root));

    service.set_at("t.os", "t/standing", OverseerValue::Integer(2200)).expect("raise the standing figure");
    service
        .append_at("t.os", "t/Days/2026-10-25/intake", &a_meal("pudding", 300))
        .expect("the second append was refused");
    service
        .append_at("t.os", "t/Days/2026-11-04/intake", &a_meal("lunch", 500))
        .expect("the append in view was refused");
    let text = on_disk(&root);
    assert_eq!(target_of(&text, "2026-10-25"), Some(OverseerValue::Integer(1800)), "the day followed the standing figure:\n{}", text);
    assert_eq!(text.matches("- target =").count(), 2, "a day froze twice, or one never did:\n{}", text);
    assert_eq!(target_of(&text, "2026-11-04"), Some(OverseerValue::Integer(2200)), "the newer day did not take the figure as it stood:\n{}", text);
    assert_eq!(target_of(&text, "2026-11-03"), Some(OverseerValue::Integer(2200)), "an unwritten day stopped following");
}
