//! The parsed parts of a history and the mounted files worked out are kept only within a limit,
//! and a document read after they were let go reads the same as before.
//!
//! Unbounded, they held every part and every mounted file the server had ever read, for as long as
//! it ran, outside the budget `OVERSEER_CACHE_MB` sets - a footprint that only ever grew.

use overseer::server::DocumentRoot;
use overseer::types::*;
use std::path::PathBuf;

const HOST: &str = r#"tab log (label="Log", mutable=true) {
    div (hidden=true) {
        div Done {
            string handle = ""
            timestamp done_at = "2026-11-02T10:00:00Z"
            int points = 1
        }
    }

    int total (mutable=false) = $(History.sum(|x| x/points))
    int first_price (mutable=false) = $(First/shop/price)
    int second_price (mutable=false) = $(Second/shop/price)

    mount First (lazy=false, mutable=false, hidden=true, source="first.os/shop") { }
    mount Second (lazy=false, mutable=false, hidden=true, source="second.os/shop") { }

    list History (entry=<Done>, key="handle", parts=$(|x| week(x/done_at))) { }
}
"#;

fn week(handle: &str, at: &str, points: i64) -> String {
    format!(
        "list History {{\n    - {{\n        - handle = \"{}\"\n        - done_at = \"{}\"\n        - points = {}\n    }}\n}}\n",
        handle, at, points
    )
}

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Sandbox(PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        overseer::bounded::set_limit_override(None);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sandbox(tag: &str) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_bounded_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("log.parts/History")).unwrap();
    std::fs::write(root.join("log.os"), HOST).unwrap();
    std::fs::write(root.join("log.parts/History/2026-W43.os"), week("a", "2026-10-20T08:00:00Z", 3)).unwrap();
    std::fs::write(root.join("log.parts/History/2026-W44.os"), week("b", "2026-10-27T08:00:00Z", 5)).unwrap();
    std::fs::write(root.join("log.parts/History/2026-W45.os"), week("c", "2026-11-03T08:00:00Z", 7)).unwrap();
    std::fs::write(root.join("first.os"), "div shop {\n    int price = 11\n}\n").unwrap();
    std::fs::write(root.join("second.os"), "div shop {\n    int price = 13\n}\n").unwrap();
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

fn read_all(documents: &DocumentRoot) -> Vec<Option<OverseerValue>> {
    let nodes = documents.open("log.os").expect("open");
    ["log/total", "log/first_price", "log/second_price", "log/History/[a]/points"]
        .iter()
        .map(|address| value(&nodes, address))
        .collect()
}

#[test]
fn parts_and_mounts_are_let_go_beyond_the_limit_and_read_again_the_same() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_sandbox, documents) = sandbox("small");
    let expected = vec![
        Some(OverseerValue::Integer(15)),
        Some(OverseerValue::Integer(11)),
        Some(OverseerValue::Integer(13)),
        Some(OverseerValue::Integer(3)),
    ];

    // Each cache may hold only the one thing last put in.
    overseer::bounded::set_limit_override(Some(1));
    assert_eq!(read_all(&documents), expected);
    assert!(overseer::parts::read_parts_held().0 <= 1, "parts held: {:?}", overseer::parts::read_parts_held());
    assert!(overseer::actions::mounted_held().0 <= 1, "mounts held: {:?}", overseer::actions::mounted_held());

    // Worked out again from scratch, with what was let go read from the files once more.
    overseer::app_api::forget_baseline();
    overseer::app_api::forget_dependencies();
    overseer::document_cache::forget_nodes();
    assert_eq!(read_all(&documents), expected, "a document read after its parts and mounts were let go");

    // With room, all of them stay.
    overseer::bounded::set_limit_override(Some(64 * 1024 * 1024));
    overseer::app_api::forget_baseline();
    overseer::app_api::forget_dependencies();
    overseer::document_cache::forget_nodes();
    assert_eq!(read_all(&documents), expected);
    assert!(overseer::parts::read_parts_held().0 >= 3, "parts held: {:?}", overseer::parts::read_parts_held());
    assert!(overseer::actions::mounted_held().0 >= 2, "mounts held: {:?}", overseer::actions::mounted_held());
}
