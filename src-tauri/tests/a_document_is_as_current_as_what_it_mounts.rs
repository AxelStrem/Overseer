//! A document held worked out is only as current as the files its mounts were read from.
//!
//! The server holds each document worked out, under its own text, and hands it out again while
//! that text is unchanged. But a document worked out has what its mounts brought in inside it - the
//! food tracker has the food catalog - and a food added to the catalog changes the catalog's file,
//! not the tracker's text. So the tracker went on being handed out with the catalog as it had been:
//! a food the bot had just added could not be recorded, and every figure of the meal was an error,
//! until something changed the tracker's own text. Found on 2026-09-28.
//!
//! A mount says now which file it was read from and how that file stood, and a document held is
//! let go once any of them has changed.

use overseer::server::DocumentRoot;
use overseer::types::*;

const HOST: &str = r#"tab t (label="T", mutable=true) {
    mount CATALOG (hidden=true, lazy=false, mutable=false, source="side.os/s/Things") { }

    div (hidden=true) {
        div Use (layout="horizontal") {
            string thing = ""
            float worth = $(/t/CATALOG/Things.filter(|x| x/handle == ../thing).map(|x| x/worth).sum())
        }
    }

    list uses (entry=<Use>) {
        - {
            - thing = "a"
        }
    }
}
"#;

const SIDE: &str = r#"tab s (label="S", mutable=true) {
    div (hidden=true) {
        div Thing (layout="horizontal") {
            string handle = ""
            int worth = 0
        }
    }

    list Things (entry=<Thing>, key="handle") {
        - {
            - handle = "a"
            - worth = 1
        }
    }
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

struct Sandbox(std::path::PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sandbox(tag: &str) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_mounts_current_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("host.os"), HOST).unwrap();
    std::fs::write(root.join("side.os"), SIDE).unwrap();
    overseer::app_api::forget_baseline();
    overseer::app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let documents = DocumentRoot::new(&root).unwrap();
    documents.open("host.os").expect("open");
    (Sandbox(root), documents)
}

fn worth(nodes: &[OverseerNode], address: &str) -> Option<f64> {
    match overseer::addressing::find(nodes, address)?.parameters.get("_computed_value") {
        Some(OverseerValue::Integer(i)) => Some(*i as f64),
        Some(OverseerValue::Float(f)) => Some(*f),
        _ => None,
    }
}

fn a_thing(handle: &str, worth: i64) -> std::collections::HashMap<String, OverseerValue> {
    [
        ("handle".to_string(), OverseerValue::String(handle.to_string())),
        ("worth".to_string(), OverseerValue::Integer(worth)),
    ]
    .into_iter()
    .collect()
}

fn a_use(thing: &str) -> std::collections::HashMap<String, OverseerValue> {
    [("thing".to_string(), OverseerValue::String(thing.to_string()))].into_iter().collect()
}

#[test]
fn what_was_added_to_a_mounted_file_can_be_used_at_once() {
    serialised(|| {
        let (_sandbox, documents) = sandbox("added");
        // Held worked out, as it is after any write.
        documents.append_at("host.os", "t/uses", &a_use("a")).expect("a use of what is there");

        documents.append_at("side.os", "s/Things", &a_thing("b", 5)).expect("a new thing");
        let written = documents.append_at("host.os", "t/uses", &a_use("b")).expect("a use of it");
        let now = documents.open("host.os").expect("reopen");
        let last = written.child_addresses.last().expect("the new use").clone();
        assert_eq!(
            worth(&now, &format!("{}/worth", last)),
            Some(5.0),
            "the host was worked out against the mounted file as it was before"
        );
    });
}

#[test]
fn a_document_opened_after_its_mounted_file_changed_shows_the_change() {
    serialised(|| {
        let (_sandbox, documents) = sandbox("reopen");
        documents.set_at("side.os", "s/Things/[a]/worth", OverseerValue::Integer(7)).expect("a change to the side");
        let now = documents.open("host.os").expect("reopen");
        assert_eq!(worth(&now, "t/uses/Use__1/worth"), Some(7.0), "the host was handed out as it was held");
    });
}

#[test]
fn a_mount_that_has_not_changed_keeps_the_document_held() {
    // Letting go too readily costs a whole working out on every write, which is what holding the
    // document is for - a stamp that moved on every read would do that and nothing would say so.
    serialised(|| {
        let (_sandbox, documents) = sandbox("unchanged");
        documents.append_at("host.os", "t/uses", &a_use("a")).expect("the first write");
        let whole = overseer::resolver::times_worked_out_whole();
        documents.append_at("host.os", "t/uses", &a_use("a")).expect("the second write");
        assert_eq!(
            overseer::resolver::times_worked_out_whole(),
            whole,
            "the document held was let go though nothing it mounts had changed"
        );
    });
}
