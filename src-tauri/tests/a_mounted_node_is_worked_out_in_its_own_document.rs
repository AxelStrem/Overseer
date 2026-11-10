//! A mounted node's formulas are worked out in the document they come from.
//!
//! A day of the food tracker reads a flag at its document's top, a figure elsewhere in its own
//! tab, and the food catalogue through a mount of its own. Mounted into another document, those
//! are still that file's: its absolute paths, the names above the mounted node, and its own
//! mounts - read relative to its own folder. Only the contexts come through the host.

use overseer::server::DocumentRoot;
use overseer::types::*;

const HOST: &str = r#"context accent = "host blue"
tab weight (label="Weight") {
    mount CAL (lazy=false, mutable=false, hidden=true, source="inner/side.os/s/History") { }
    float monday = $(CAL/History.find("mon")/kcal)
}
"#;

const SIDE: &str = r#"bool flag = true
div (hidden=true) {
    div Day {
        string day = ""
        string food = ""
        bool shown = $(flag)
        float standing = $(/s/standing/x)
        float kcal = $(CAT/Catalog.filter(|f| f/handle == ../food)/kcal)
        string seen = $(accent)
    }
}
tab s (label="S") {
    div standing {
        float x = 7
    }
    mount CAT (lazy=false, mutable=false, hidden=true, source="cat.os/c/Catalog") { }
    list History (entry=<Day>, key="day") {
        - {
            - day = "mon"
            - food = "apple"
        }
    }
}
"#;

const CATALOGUE: &str = r#"div c {
    list Catalog (key="handle") {
        - {
            string handle = "apple"
            float kcal = 52
        }
    }
}
"#;

const LOOP_A: &str = r#"tab a (label="A") {
    mount B (lazy=false, mutable=false, source="b.os/b") { }
}
"#;

const LOOP_B: &str = r#"tab b (label="B") {
    mount A (lazy=false, mutable=false, source="a.os/a") { }
    float x = 1
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Sandbox(std::path::PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sandbox(tag: &str, files: &[(&str, &str)]) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_own_document_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (name, text) in files {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    overseer::app_api::forget_baseline();
    overseer::app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let documents = DocumentRoot::new(&root).unwrap();
    (Sandbox(root), documents)
}

fn value_at(nodes: &[OverseerNode], address: &str) -> Option<OverseerValue> {
    let node = overseer::addressing::find(nodes, address)?;
    node.parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
        .cloned()
}

fn number_at(nodes: &[OverseerNode], address: &str) -> Option<f64> {
    match value_at(nodes, address)? {
        OverseerValue::Integer(i) => Some(i as f64),
        OverseerValue::Float(f) => Some(f),
        _ => None,
    }
}

fn host_files() -> Vec<(&'static str, &'static str)> {
    vec![("host.os", HOST), ("inner/side.os", SIDE), ("inner/cat.os", CATALOGUE)]
}

#[test]
fn a_name_above_the_mounted_node_is_looked_for_in_its_own_document() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_sandbox, documents) = sandbox("bare", &host_files());
    let host = documents.open("host.os").expect("open");
    assert_eq!(
        value_at(&host, "weight/CAL/History/[mon]/shown"),
        Some(OverseerValue::Boolean(true))
    );
}

#[test]
fn an_absolute_path_starts_from_the_mounted_documents_own_top() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_sandbox, documents) = sandbox("absolute", &host_files());
    let host = documents.open("host.os").expect("open");
    assert_eq!(number_at(&host, "weight/CAL/History/[mon]/standing"), Some(7.0));
}

#[test]
fn the_mounted_documents_own_mount_is_brought_in_from_its_own_folder() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_sandbox, documents) = sandbox("nested", &host_files());
    let host = documents.open("host.os").expect("open");
    assert_eq!(number_at(&host, "weight/CAL/History/[mon]/kcal"), Some(52.0));
    assert_eq!(number_at(&host, "weight/monday"), Some(52.0));
}

#[test]
fn the_contexts_still_come_through_the_host() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_sandbox, documents) = sandbox("contexts", &host_files());
    let host = documents.open("host.os").expect("open");
    assert_eq!(
        value_at(&host, "weight/CAL/History/[mon]/seen"),
        Some(OverseerValue::String("host blue".to_string()))
    );
}

#[test]
fn a_change_to_the_nested_mounts_file_reaches_the_host() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = sandbox("stale", &host_files());
    let host = documents.open("host.os").expect("open");
    assert_eq!(number_at(&host, "weight/monday"), Some(52.0));
    // A different length, so the file's stamp changes even within the clock's resolution.
    std::fs::write(sandbox.0.join("inner/cat.os"), CATALOGUE.replace("52", "520")).unwrap();
    let host = documents.open("host.os").expect("reopen");
    assert_eq!(number_at(&host, "weight/monday"), Some(520.0));
}

#[test]
fn two_documents_that_mount_each_other_open() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_sandbox, documents) = sandbox("cycle", &[("a.os", LOOP_A), ("b.os", LOOP_B)]);
    let a = documents.open("a.os").expect("open");
    assert_eq!(number_at(&a, "a/B/b/x"), Some(1.0));
}

const WINDOWED_HOST: &str = r#"tab weight (label="Weight") {
    mount CAL (lazy=false, mutable=false, hidden=true, source="side.os/s/History") { }
    float first = $(CAL/History.find("d1")/kcal)
    float last = $(CAL/History.find("d3")/kcal)
}
"#;

const WINDOWED_SIDE: &str = r#"div (hidden=true) {
    div Day {
        string day = ""
        float grams = 0
        float kcal = $(grams * /s/per_gram)
    }
}
tab s (label="S") {
    float per_gram = 2
    list History (entry=<Day>, key="day", window=1) {
        - {
            - day = "d1"
            - grams = 10
        }
        - {
            - day = "d2"
            - grams = 20
        }
        - {
            - day = "d3"
            - grams = 30
        }
    }
}
"#;

#[test]
fn an_entry_a_mounted_window_leaves_out_is_worked_out_in_its_own_document_when_read() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_sandbox, documents) = sandbox("window", &[("host.os", WINDOWED_HOST), ("side.os", WINDOWED_SIDE)]);
    let host = documents.open("host.os").expect("open");
    assert_eq!(number_at(&host, "weight/first"), Some(20.0));
    assert_eq!(number_at(&host, "weight/last"), Some(60.0));
}
