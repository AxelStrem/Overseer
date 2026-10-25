//! A write to a file a document mounts is announced to whoever watches the document, and a page
//! catches up with it.
//!
//! A page heard each write to its own document's file, but a document that mounts another shows
//! that file's content too - the food tracker shows the catalogue - and a food the bot added to the
//! catalogue changed nothing the page was told about. It stayed as it was until its next edit or a
//! reload. Now what a document's mounts read is announced beside its version, and a page that says
//! which it was shown is answered with what changed since.

use overseer::app_api;
use overseer::delta;
use overseer::server::DocumentRoot;
use overseer::types::*;
use overseer::watch::Heard;
use std::time::Duration;

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

struct Sandbox {
    root: std::path::PathBuf,
    documents: DocumentRoot,
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Sandbox {
    fn new(tag: &str) -> Sandbox {
        let root = std::env::temp_dir().join(format!("overseer_watch_mounts_{}_{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("host.os"), HOST).unwrap();
        std::fs::write(root.join("side.os"), SIDE).unwrap();
        app_api::forget_baseline();
        app_api::forget_dependencies();
        overseer::viewstate::forget_all();
        let documents = DocumentRoot::new(&root).unwrap();
        Sandbox { root, documents }
    }

    /// The document as a page opens it, which is also what tells the watch what its mounts read.
    fn open(&self, session: &str, name: &str) -> Vec<OverseerNode> {
        let text = std::fs::read_to_string(self.root.join(name)).unwrap();
        let nodes = self
            .documents
            .command_for(session, Some(name), "parse_overseer_content", &serde_json::json!({ "content": text }))
            .expect("open");
        serde_json::from_value(nodes).expect("a document")
    }

    /// A page's catch-up, saying which text it holds and what its mounts read when it was shown -
    /// made where the server makes it, with its settings and in the document's folder.
    fn catch_up(&self, session: &str, name: &str, read_from: Option<&str>) -> app_api::ResolvedUpdate {
        let text = std::fs::read_to_string(self.root.join(name)).unwrap();
        let root = self.documents.path().to_path_buf();
        overseer::environment::with_file(self.documents.settings_file(), || {
            overseer::docmgr::manager::DocumentManager::with_document(Some(root.clone()), || {
                let _showing = app_api::describing_for(Some(app_api::version_of(&text)));
                let _reading = app_api::reading_for(read_from.map(str::to_string));
                let path = root.join(name).to_string_lossy().to_string();
                app_api::catch_up_at(&path, name, session).expect("the catch-up")
            })
        })
    }

    /// The bot sets what a thing in the catalogue is worth, through the server.
    fn the_bot_sets_worth(&self, worth: i64) {
        self.documents
            .set_at("side.os", "s/Things/[a]/worth", OverseerValue::Integer(worth))
            .expect("the bot's write");
    }

    fn listen(&self, name: &str) -> (Heard, tokio::sync::broadcast::Receiver<Heard>) {
        overseer::watch::listen(&self.root.join(name))
    }
}

fn heard(listening: &mut tokio::sync::broadcast::Receiver<Heard>, within: Duration) -> Option<Heard> {
    let until = std::time::Instant::now() + within;
    loop {
        match listening.try_recv() {
            Ok(heard) => return Some(heard),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty) if std::time::Instant::now() < until => {
                std::thread::sleep(Duration::from_millis(50))
            }
            Err(_) => return None,
        }
    }
}

fn taken(mut held: Vec<OverseerNode>, update: app_api::ResolvedUpdate) -> Vec<OverseerNode> {
    match (update.changes, update.nodes) {
        (Some(changes), _) => {
            delta::apply(&mut held, changes);
            held
        }
        (None, Some(nodes)) => nodes,
        (None, None) => panic!("an answer with neither changes nor a document"),
    }
}

/// What the page shows of a document: each node's name and value, and its children.
fn as_drawn(nodes: &[OverseerNode]) -> serde_json::Value {
    serde_json::Value::Array(
        nodes
            .iter()
            .map(|n| {
                let value = n.parameters.get("_computed_value").or_else(|| n.parameters.get("value"));
                serde_json::json!({
                    "name": n.name,
                    "value": serde_json::to_value(value).unwrap(),
                    "children": as_drawn(&n.children),
                })
            })
            .collect(),
    )
}

fn worth(nodes: &[OverseerNode]) -> Option<f64> {
    match overseer::addressing::find(nodes, "t/uses/Use__1/worth")?.parameters.get("_computed_value") {
        Some(OverseerValue::Integer(i)) => Some(*i as f64),
        Some(OverseerValue::Float(f)) => Some(*f),
        _ => None,
    }
}

#[test]
fn a_write_made_here_to_a_mounted_file_is_heard_at_once() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = Sandbox::new("here");
    sandbox.open("page", "host.os");
    let (now, mut listening) = sandbox.listen("host.os");
    assert_eq!(now.version, app_api::version_of(HOST));
    assert!(!now.read_from.is_empty(), "listening does not say what the document's mounts read");

    sandbox.the_bot_sets_worth(70);
    let after = heard(&mut listening, Duration::from_millis(500)).expect("the write to the mounted file was not heard");
    assert_eq!(after.version, app_api::version_of(HOST), "the document's own file did not move");
    assert_ne!(after.read_from, now.read_from, "what the mounts read was announced as it was");
}

#[test]
fn a_write_made_from_outside_to_a_mounted_file_is_heard_at_the_next_look() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = Sandbox::new("outside");
    sandbox.open("page", "host.os");
    let (now, mut listening) = sandbox.listen("host.os");
    std::fs::write(sandbox.root.join("side.os"), SIDE.replace("- worth = 1", "- worth = 70")).unwrap();
    let after = heard(&mut listening, overseer::watch::LOOKED_AT_EVERY * 3).expect("a write from outside was never heard");
    assert_ne!(after.read_from, now.read_from);
}

#[test]
fn a_page_behind_what_its_document_mounts_is_told_what_changed() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = Sandbox::new("behind");
    let held = sandbox.open("page", "host.os");
    assert_eq!(worth(&held), Some(1.0));
    let (shown, mut listening) = sandbox.listen("host.os");
    sandbox.the_bot_sets_worth(70);
    let after = heard(&mut listening, Duration::from_millis(500)).expect("not heard");

    let update = sandbox.catch_up("page", "host.os", Some(&shown.read_from));
    assert!(update.changes.as_ref().is_some_and(|c| !c.is_empty()), "the page was not told what changed");
    assert_eq!(update.read_from, after.read_from, "the answer and the watch name what the mounts read differently");
    let now = taken(held, update);
    assert_eq!(worth(&now), Some(70.0), "the page does not show what the mounted file holds");
    assert_eq!(as_drawn(&now), as_drawn(&sandbox.open("check", "host.os")), "the page does not show what opening the document gives");
}

#[test]
fn a_page_that_holds_what_its_document_mounts_is_told_nothing_changed() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = Sandbox::new("current");
    sandbox.open("page", "host.os");
    let (shown, _listening) = sandbox.listen("host.os");
    let update = sandbox.catch_up("page", "host.os", Some(&shown.read_from));
    assert_eq!(update.changes.as_deref().map(|c| c.len()), Some(0), "nothing moved, but the answer says otherwise");
    assert_eq!(update.read_from, shown.read_from);
}

#[test]
fn a_page_is_told_what_changed_though_another_caught_up_first() {
    // The document held is the newer one by the time this page asks, so it cannot be what the
    // answer is described against: that answered "nothing changed", and the page stayed stale.
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = Sandbox::new("second");
    let held = sandbox.open("page", "host.os");
    let (shown, _listening) = sandbox.listen("host.os");
    sandbox.the_bot_sets_worth(70);
    let other = sandbox.open("other", "host.os");
    assert_eq!(worth(&other), Some(70.0));

    let update = sandbox.catch_up("page", "host.os", Some(&shown.read_from));
    assert!(update.changes.as_ref().is_some_and(|c| !c.is_empty()), "the page was not told what changed");
    assert_eq!(worth(&taken(held, update)), Some(70.0), "the page does not show what the mounted file holds");
}

#[test]
fn a_document_that_mounts_nothing_says_nothing_about_it() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = Sandbox::new("nothing");
    sandbox.open("page", "side.os");
    let (now, _listening) = sandbox.listen("side.os");
    assert_eq!(now.read_from, "");
    assert_eq!(sandbox.catch_up("page", "side.os", None).read_from, "");
}
