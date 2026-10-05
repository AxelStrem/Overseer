//! The same text in two folders is two documents.
//!
//! A document's mounts are read relative to its own folder, so two copies of one text, each beside
//! a different neighbour, work out to different things. The worked-out documents were kept by their
//! text alone, and judged fresh against the files of whoever had worked them out - so the second
//! folder was handed the first folder's document, with the first folder's neighbour inside it.
//! Found on 2026-10-05, as `server_writes::a_new_food_keeps_the_figures_it_was_given` failing about
//! one run in seven: every test there copies the same tracker beside a catalogue of its own.

use overseer::server::DocumentRoot;
use overseer::types::*;

const HOST: &str = r#"tab t (label="T", mutable=true) {
    mount CATALOG (hidden=true, lazy=false, mutable=false, source="side.os/s/Things") { }

    int worth = $(/t/CATALOG/Things.map(|x| x/worth).sum())
}
"#;

fn side(worth: i64) -> String {
    format!(
        r#"tab s (label="S", mutable=true) {{
    div (hidden=true) {{
        div Thing (layout="horizontal") {{
            string handle = ""
            int worth = 0
        }}
    }}

    list Things (entry=<Thing>, key="handle") {{
        - {{
            - handle = "a"
            - worth = {}
        }}
    }}
}}
"#,
        worth
    )
}

struct Sandbox(std::path::PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn folder(tag: &str, worth: i64) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_two_folders_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("host.os"), HOST).unwrap();
    std::fs::write(root.join("side.os"), side(worth)).unwrap();
    let documents = DocumentRoot::new(&root).unwrap();
    (Sandbox(root), documents)
}

fn worth(nodes: &[OverseerNode]) -> Option<f64> {
    match overseer::addressing::find(nodes, "t/worth")?.parameters.get("_computed_value") {
        Some(OverseerValue::Integer(i)) => Some(*i as f64),
        Some(OverseerValue::Float(f)) => Some(*f),
        _ => None,
    }
}

#[test]
fn each_folder_sees_its_own_neighbour() {
    let (_one, first) = folder("one", 1);
    let (_two, second) = folder("two", 2);
    assert_eq!(worth(&first.open("host.os").expect("open the first")), Some(1.0));
    assert_eq!(
        worth(&second.open("host.os").expect("open the second")),
        Some(2.0),
        "the second folder was handed the first folder's document"
    );
    // And the first is still its own, though the second was worked out since.
    assert_eq!(worth(&first.open("host.os").expect("reopen the first")), Some(1.0));
}
