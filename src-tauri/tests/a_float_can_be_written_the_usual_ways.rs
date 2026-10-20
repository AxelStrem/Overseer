//! A float can be written the usual ways: `1.`, `.5`, `2e3`, `1.5E-7`, `inf`, `-inf`, `nan`.
//!
//! The parser knew only `1` and `1.5`. `1e3` came out as the number 1 followed by a node named
//! `e3`, `1.` as the integer 1 with a stray point, and `inf` as a word. In formulas it was the
//! other way: they read `inf` wherever a name began with it, so `$(info + 1)` was an error.
//!
//! A number also holds the type of the field it is written in, so `float x = 1` is a float, and
//! a float is written back in a form that reads as the same float - `1e20` rather than twenty-one
//! digits too long for a whole number.

use overseer::app_api;
use overseer::file_ops::OverseerFileHandler;
use overseer::types::{OverseerNode, OverseerValue};

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

const DOC: &str = r#"
div (hidden=true) {
    div Item {
        string name = ""
        float price = 0
    }
}

tab sizes (mutable=true) {
    float whole = 1
    float pointed = 1.
    float half = .5
    float below = -.25
    float big = 2e3
    float tiny = 1.5E-7
    float up = inf
    float down = -inf
    float unknown = nan
    int thousand = 1e3
    int part = 2.5
    string word = information
    string path = inf/x
    int info = 3
    int nancy = 4
    int next = $(info + nancy + 1)
    float sum = $(1e3 + .5 + 1.)
    float far = 0

    list Items (entry=<Item>, key="name") {
        - {
            - name = "tea"
            - price = 2
        }
        - {
            - name = "air"
            - price = inf
        }
    }

    button grow (label="Grow") {
        on click {
            set (path="../far") = $(1e20)
        }
    }
    button endless (label="Endless") {
        on click {
            set (path="../far") = $(-inf)
        }
    }
}
"#;

fn loaded(text: &str) -> Vec<OverseerNode> {
    overseer::source_registry::SourceRegistry::reset();
    app_api::load_document(text.to_string()).expect("load")
}

fn value(nodes: &[OverseerNode], address: &str) -> OverseerValue {
    let node = overseer::addressing::find(nodes, address).unwrap_or_else(|| panic!("nothing at {}", address));
    node.parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
        .cloned()
        .unwrap_or_else(|| panic!("{} has no value", address))
}

fn float(nodes: &[OverseerNode], address: &str) -> f64 {
    match value(nodes, address) {
        OverseerValue::Float(f) => f,
        other => panic!("{} reads {:?}", address, other),
    }
}

#[test]
fn each_shape_reads_as_the_float_it_says() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = loaded(DOC);
    assert_eq!(float(&nodes, "sizes/pointed"), 1.0);
    assert_eq!(float(&nodes, "sizes/half"), 0.5);
    assert_eq!(float(&nodes, "sizes/below"), -0.25);
    assert_eq!(float(&nodes, "sizes/big"), 2000.0);
    assert_eq!(float(&nodes, "sizes/tiny"), 1.5e-7);
    assert_eq!(float(&nodes, "sizes/up"), f64::INFINITY);
    assert_eq!(float(&nodes, "sizes/down"), f64::NEG_INFINITY);
    assert!(float(&nodes, "sizes/unknown").is_nan());
    assert_eq!(float(&nodes, "sizes/Items/[air]/price"), f64::INFINITY);
}

#[test]
fn nothing_after_a_number_is_left_over_as_a_node() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = loaded(DOC);
    let sizes = overseer::addressing::find(&nodes, "sizes").expect("sizes");
    let names: Vec<&str> = sizes.children.iter().map(|child| child.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "whole", "pointed", "half", "below", "big", "tiny", "up", "down", "unknown", "thousand", "part",
            "word", "path", "info", "nancy", "next", "sum", "far", "Items", "grow", "endless",
        ]
    );
}

#[test]
fn a_number_holds_the_type_of_its_field() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = loaded(DOC);
    assert_eq!(value(&nodes, "sizes/whole"), OverseerValue::Float(1.0));
    assert_eq!(value(&nodes, "sizes/Items/[tea]/price"), OverseerValue::Float(2.0));
    assert_eq!(value(&nodes, "sizes/thousand"), OverseerValue::Integer(1000));
    assert_eq!(value(&nodes, "sizes/part"), OverseerValue::Float(2.5));
}

#[test]
fn a_word_that_only_starts_like_one_is_a_word() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = loaded(DOC);
    assert_eq!(value(&nodes, "sizes/word"), OverseerValue::String("information".into()));
    assert_eq!(value(&nodes, "sizes/path"), OverseerValue::String("inf/x".into()));
}

#[test]
fn formulas_read_the_same_shapes_and_names_that_begin_with_them() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = loaded(DOC);
    assert_eq!(value(&nodes, "sizes/next"), OverseerValue::Integer(8));
    assert_eq!(float(&nodes, "sizes/sum"), 1001.5);
}

#[test]
fn the_document_comes_back_as_it_was_written() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let text = DOC.trim_start();
    let nodes = loaded(text);
    // Through JSON, as the page has it: infinity and NaN must survive the trip.
    let across: Vec<OverseerNode> =
        serde_json::from_str(&serde_json::to_string(&nodes).expect("json")).expect("json");
    let saved = app_api::canonicalize_document(&OverseerFileHandler::serialize_nodes(&across).expect("save"));
    assert_eq!(saved, text);
}

#[test]
fn infinity_and_nan_reach_the_page_as_words() {
    let json = serde_json::to_string(&[
        OverseerValue::Float(f64::INFINITY),
        OverseerValue::Float(f64::NEG_INFINITY),
        OverseerValue::Float(f64::NAN),
        OverseerValue::Float(1.5),
    ])
    .expect("json");
    assert_eq!(json, r#"[{"Float":"inf"},{"Float":"-inf"},{"Float":"nan"},{"Float":1.5}]"#);
    let back: Vec<OverseerValue> = serde_json::from_str(&json).expect("json");
    assert_eq!(back[0], OverseerValue::Float(f64::INFINITY));
    assert_eq!(back[1], OverseerValue::Float(f64::NEG_INFINITY));
    assert!(matches!(back[2], OverseerValue::Float(f) if f.is_nan()));
}

/// The document in a file of its own, pressed and read back from the disk.
fn pressed_and_reopened(button: &str) -> (String, Vec<OverseerNode>) {
    let root = std::env::temp_dir().join(format!("overseer_float_{}_{}", button, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("f.os");
    std::fs::write(&file, DOC.trim_start()).unwrap();
    let file = file.to_string_lossy().to_string();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let address = format!("sizes/{}", button);
    let path = overseer::addressing::name_path(&loaded(DOC.trim_start()), &address).expect("the button");
    app_api::run_event_at(&file, "f.os", path, "click".into(), "s", Vec::new(), Vec::new()).expect("the press");
    let text = std::fs::read_to_string(&file).unwrap();
    let nodes = loaded(&text);
    (text, nodes)
}

#[test]
fn a_float_too_large_to_write_out_is_written_with_an_exponent() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (text, nodes) = pressed_and_reopened("grow");
    assert!(text.contains("float far = 1e20"), "{}", text);
    assert_eq!(float(&nodes, "sizes/far"), 1e20);
}

#[test]
fn an_infinity_set_by_a_press_is_written_as_one() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (text, nodes) = pressed_and_reopened("endless");
    assert!(text.contains("float far = -inf"), "{}", text);
    assert_eq!(float(&nodes, "sizes/far"), f64::NEG_INFINITY);
}
