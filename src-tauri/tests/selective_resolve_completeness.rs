//! A selective resolve must produce exactly what a full resolve produces.
//!
//! The frontend used to follow every selective resolve with a second, full one, because
//! dependency-directed updates missed cascades through unnamed wrappers. That cost a
//! second backend pass and two extra transfers of the whole document on every edit. The
//! selective path now resolves values outright, and the frontend trusts its result - so
//! any gap between the two paths would show up as a stale value on screen with nothing
//! left to correct it.

use overseer::app_api;
use overseer::docmgr::manager::DocumentManager;
use overseer::types::*;

// Provenance ids are minted per parse, so they always differ; what matters is whether the
// rendered content differs.
fn params_of(n: &OverseerNode) -> Vec<String> {
    let mut m: Vec<String> = n
        .parameters
        .iter()
        .map(|(k, v)| format!("{}={:?}", k, v))
        .collect();
    m.sort();
    m
}

fn strip(n: &OverseerNode) -> serde_json::Value {
    serde_json::json!({
        "name": n.name,
        "type": format!("{:?}", n.node_type),
        "params": params_of(n),
        "children": n.children.iter().map(strip).collect::<Vec<_>>(),
    })
}

fn walk(a: &serde_json::Value, b: &serde_json::Value, path: &str, out: &mut Vec<String>) {
    if out.len() > 6 { return }
    if a["name"] != b["name"] || a["type"] != b["type"] {
        out.push(format!("{}: {} vs {}", path, a["name"], b["name"]));
        return;
    }
    let here = format!("{}/{}", path, a["name"].as_str().unwrap_or("?"));
    if a["params"] != b["params"] {
        let (pa, pb) = (a["params"].as_array().unwrap(), b["params"].as_array().unwrap());
        for p in pa { if !pb.contains(p) { out.push(format!("{}  only-before: {}", here, p)); } }
        for p in pb { if !pa.contains(p) { out.push(format!("{}  only-after:  {}", here, p)); } }
    }
    let (ca, cb) = (a["children"].as_array().unwrap(), b["children"].as_array().unwrap());
    if ca.len() != cb.len() {
        out.push(format!("{}  child count {} vs {}", here, ca.len(), cb.len()));
        return;
    }
    for (x, y) in ca.iter().zip(cb) { walk(x, y, &here, out); }
}

#[test]
fn selective_resolve_matches_full_resolve() {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/exercise_tracker/exercise.os");
    DocumentManager::set_current_document(Some(p.to_string_lossy().as_ref()));
    let text = std::fs::read_to_string(&p).unwrap();

    // The app applies the edit to its own document and serializes that, so the changed
    // value is already present in the text the backend receives.
    let edited = text.replacen("                    int p4 = 1", "                    int p4 = 2", 1);
    assert_ne!(edited, text, "fixture no longer contains the edited field");
    let field = "exercise_tracker/Exercises/Exercise__1/plates/p4";
    let mut m = std::collections::HashMap::new();
    m.insert(field.to_string(), OverseerValue::Integer(2));

    let selective = app_api::resolve_selective(edited.clone(), vec![field.to_string()], Some(m)).unwrap();
    let full = app_api::load_document(edited).unwrap();

    let (a, b): (Vec<_>, Vec<_>) =
        (selective.iter().map(strip).collect(), full.iter().map(strip).collect());
    let mut out = Vec::new();
    if a.len() != b.len() {
        out.push(format!("root count {} vs {}", a.len(), b.len()));
    }
    for (x, y) in a.iter().zip(&b) {
        walk(x, y, "", &mut out);
    }
    for d in out.iter().take(8) {
        println!("{}", d);
    }
    assert!(
        out.is_empty(),
        "a selective resolve must produce what a full resolve produces - the frontend          relies on it and no longer follows up with a second full resolve ({} differences)",
        out.len()
    );
    DocumentManager::set_current_document(None);
}
