//! An edit inside an instance that a template holds is saved on the entry it was made in.
//!
//! The writer does not walk into such an instance: the template's own overrides there, such as
//! `- input = $(../../amount)`, carry every mark an edit carries, and walked into they were copied
//! into every record. So an edit leaves a mark of its own, which a template's text never has, and
//! the writer takes only what carries it.

use overseer::app_api;
use overseer::file_ops::OverseerFileHandler;
use overseer::server::DocumentRoot;
use overseer::types::*;

const DOC: &str = r#"tab t (label="T", mutable=true) {
    div (hidden=true) {
        div Scorer (layout="horizontal", margin=0) {
            int input (hidden=true) = 0
            int out (label="Out") = $(input * 2)
        }
        div Row (layout="vertical") {
            int amount = 1

            div (layout="horizontal", margin=0) {
                <Scorer> quality {
                    - input = $(../../amount)
                }
            }
        }
    }

    list rows (entry=<Row>) {
        - {
            - amount = 3
        }
        - {
            - amount = 5
        }
    }
}
"#;

const ADDRESS: &str = "t/rows/Row__2/quality/input";

/// The document as it has to be saved once the second entry's input is 9.
fn expected() -> String {
    DOC.replace(
        "            - amount = 5\n",
        "            - amount = 5\n            - quality {\n                - input = 9\n            }\n",
    )
}

/// Saving consults a process-wide registry of parsed sources, so two of these running at once
/// see each other's document.
static SAVING: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn across_ipc(nodes: &[OverseerNode]) -> Vec<OverseerNode> {
    serde_json::from_str(&serde_json::to_string(nodes).expect("to json")).expect("from json")
}

fn input_of(nodes: &[OverseerNode], entry: usize) -> Option<OverseerValue> {
    let address = format!("t/rows/Row__{}/quality/input", entry);
    overseer::addressing::find(nodes, &address)?.parameters.get("value").cloned()
}

fn edited_from_the_page() -> Vec<OverseerNode> {
    let mut changed = std::collections::HashMap::new();
    changed.insert(ADDRESS.to_string(), OverseerValue::Integer(9));
    app_api::resolve_selective(DOC.to_string(), vec![ADDRESS.to_string()], Some(changed))
        .expect("the edit")
}

#[test]
fn an_unedited_document_saves_as_it_was_written() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = app_api::load_document(DOC.to_string()).unwrap();
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), DOC);
}

#[test]
fn an_edit_from_the_page_is_saved_on_its_entry() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = edited_from_the_page();
    assert_eq!(input_of(&nodes, 2), Some(OverseerValue::Integer(9)), "not edited in memory");
    let saved = OverseerFileHandler::serialize_nodes(&nodes).unwrap();
    assert_eq!(saved, expected());
    let reread = app_api::load_document(saved.clone()).unwrap();
    assert_eq!(input_of(&reread, 2), Some(OverseerValue::Integer(9)), "did not read back");
    let unedited = app_api::load_document(DOC.to_string()).unwrap();
    assert_eq!(input_of(&reread, 1), input_of(&unedited, 1), "the other entry changed");
    assert_eq!(OverseerFileHandler::serialize_nodes(&reread).unwrap(), saved);
}

#[test]
fn an_edit_from_the_page_is_saved_after_the_round_trip_to_it() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = across_ipc(&edited_from_the_page());
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), expected());
}

#[test]
fn a_document_saved_twice_says_the_edit_once() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = edited_from_the_page();
    let once = OverseerFileHandler::serialize_nodes(&nodes).unwrap();
    let twice = OverseerFileHandler::serialize_nodes(&nodes).unwrap();
    assert_eq!(twice, once);
}

#[test]
fn an_edit_by_an_action_is_saved_on_its_entry() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let mut nodes = app_api::load_document(DOC.to_string()).unwrap();
    let path = overseer::addressing::name_path(&nodes, ADDRESS).expect("a path to the field");
    overseer::actions::ActionExecutor::assign_value(
        &mut nodes,
        &format!("/{}", path.join("/")),
        OverseerValue::Integer(9),
    )
    .expect("set the value");
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), expected());
}

#[test]
fn an_edit_through_the_server_is_saved_on_its_entry() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let root = std::env::temp_dir().join(format!("overseer_nested_edit_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("t.os"), DOC).expect("write");
    let service = DocumentRoot::new(&root).expect("open the root");
    service.set_at("t.os", ADDRESS, OverseerValue::Integer(9)).expect("the write was refused");
    let saved = std::fs::read_to_string(root.join("t.os")).expect("read");
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(saved, expected());
}

#[test]
fn an_edit_to_a_nested_override_already_written_changes_only_it() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let written = expected();
    let mut changed = std::collections::HashMap::new();
    changed.insert(ADDRESS.to_string(), OverseerValue::Integer(10));
    let nodes = app_api::resolve_selective(written.clone(), vec![ADDRESS.to_string()], Some(changed))
        .expect("the edit");
    assert_eq!(
        OverseerFileHandler::serialize_nodes(&nodes).unwrap(),
        written.replace("- input = 9", "- input = 10")
    );
}
