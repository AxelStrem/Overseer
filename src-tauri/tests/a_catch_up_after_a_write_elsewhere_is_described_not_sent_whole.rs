//! A page that catches up after a write made elsewhere is told what changed, not sent the whole
//! document.
//!
//! A write moves the document held for the file's text onto the text it wrote, so the tree for the
//! text before was gone the moment the file moved. Every other page open on the document still
//! shows that text, and asked to catch up from it - and with nothing to describe the change
//! against, was sent the whole document and drew it again from scratch: 9.4 MB for the project
//! tracker, each time the loop or the bot wrote one field, and every press made meanwhile the
//! same. While a page listens, the tree before is kept for it.
//!
//!   cargo test --features server --test a_catch_up_after_a_write_elsewhere_is_described_not_sent_whole

use overseer::app_api;
use overseer::delta;
use overseer::types::*;

const DOCUMENT: &str = r#"tab t (mutable=true) {
    div (hidden=true) {
        div Row (layout="horizontal") {
            string id = ""
            int qty = 0
        }
    }
    list Rows (entry=<Row>, key="id") {
        - {
            - id = "b"
            - qty = 2
        }
        - {
            - id = "c"
            - qty = 3
        }
    }
    int total = $(Rows.sum(|x| x/qty))
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn a_file(tag: &str) -> String {
    let root = std::env::temp_dir().join(format!("overseer_superseded_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("d.os");
    std::fs::write(&file, DOCUMENT).unwrap();
    app_api::forget_baseline();
    overseer::viewstate::forget_all();
    file.to_string_lossy().to_string()
}

/// The bot sets a row's quantity, naming the row by its key as the bot does - or a page does,
/// saying which text it shows when it is given one.
fn sets(path: &str, session: &str, entry: &str, qty: i64) -> app_api::ResolvedUpdate {
    let nodes = app_api::load_document(std::fs::read_to_string(path).unwrap()).unwrap();
    let mut field = overseer::addressing::name_path(&nodes, &format!("t/Rows/[{}]", entry)).expect("no such row");
    field.push("qty".into());
    let w: app_api::ValueWrite = serde_json::from_value(serde_json::json!({
        "node_path": field, "value": OverseerValue::Integer(qty)
    }))
    .unwrap();
    app_api::write_values_at(path, "d.os", vec![w], session).expect("the write")
}

fn catches_up(path: &str, showing: &str) -> app_api::ResolvedUpdate {
    let _showing = app_api::describing_for(Some(app_api::version_of(showing)));
    app_api::catch_up_at(path, "d.os", "page").expect("the catch-up")
}

/// What the page shows of a document: each node's name and value, and its children.
fn shown(nodes: &[OverseerNode]) -> serde_json::Value {
    serde_json::Value::Array(
        nodes
            .iter()
            .map(|n| {
                let value = n.parameters.get("_computed_value").or_else(|| n.parameters.get("value"));
                serde_json::json!({
                    "name": n.name,
                    "value": serde_json::to_value(value).unwrap(),
                    "children": shown(&n.children),
                })
            })
            .collect(),
    )
}

fn taken(mut held: Vec<OverseerNode>, update: app_api::ResolvedUpdate) -> Vec<OverseerNode> {
    let changes = update.changes.expect("the page was sent the whole document");
    delta::apply(&mut held, changes);
    held
}

fn the_file(path: &str) -> Vec<OverseerNode> {
    app_api::forget_baseline();
    app_api::load_document(std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn a_page_that_listens_catches_up_from_the_text_before_by_its_changes() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let path = a_file("listens");
    let (_, _listening) = overseer::watch::listen(std::path::Path::new(&path));
    let held = app_api::load_document_for(DOCUMENT.to_string(), "d.os", "page").expect("open");
    sets(&path, "bot", "c", 7);

    let update = catches_up(&path, DOCUMENT);
    assert!(update.nodes.is_none(), "the page was sent the whole document");
    let now = taken(held, update);
    assert_eq!(shown(&now), shown(&the_file(&path)), "the page does not show what the file holds");
}

#[test]
fn a_press_from_a_page_not_yet_caught_up_is_answered_by_its_changes() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let path = a_file("press");
    let (_, _listening) = overseer::watch::listen(std::path::Path::new(&path));
    let held = app_api::load_document_for(DOCUMENT.to_string(), "d.os", "page").expect("open");
    sets(&path, "bot", "c", 7);

    let update = {
        let _showing = app_api::describing_for(Some(app_api::version_of(DOCUMENT)));
        sets(&path, "page", "b", 5)
    };
    let now = taken(held, update);
    assert_eq!(shown(&now), shown(&the_file(&path)), "the page does not show what the file holds");
}

#[test]
fn nothing_is_kept_for_a_document_nobody_listens_for() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let path = a_file("alone");
    let _held = app_api::load_document_for(DOCUMENT.to_string(), "d.os", "page").expect("open");
    sets(&path, "bot", "c", 7);

    // Still right, only the long way: nobody was going to ask.
    let update = catches_up(&path, DOCUMENT);
    let whole = update.nodes.expect("a tree was kept that nobody listened for");
    assert_eq!(shown(&whole), shown(&the_file(&path)), "the whole document is not what the file holds");
}

#[cfg(feature = "server")]
mod over_http {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpStream;
    use std::process::{Child, Command, Stdio};
    use std::time::Duration;

    struct Server {
        child: Child,
        port: u16,
    }

    impl Drop for Server {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn start(port: u16) -> Server {
        let root = std::env::temp_dir().join(format!("overseer-superseded-{}", std::process::id())).join("documents");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("ap31")).unwrap();
        std::fs::write(root.join("ap31/d.os"), super::DOCUMENT).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_overseer-server"))
            .args(["--root", root.to_string_lossy().as_ref(), "--port", &port.to_string()])
            .env_remove("OVERSEER_TOKEN")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the server should start");
        let mut out = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        while out.read_line(&mut line).unwrap_or(0) > 0 {
            if line.starts_with("listening on") {
                break;
            }
            line.clear();
        }
        std::thread::spawn(move || {
            let _ = std::io::copy(&mut out, &mut std::io::sink());
        });
        Server { child, port }
    }

    /// The answer's body.
    fn ask(port: u16, method: &str, path: &str, body: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        let request = format!(
            "{} {} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nAccept: application/json\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            method,
            path,
            body.len(),
            body
        );
        stream.write_all(request.as_bytes()).expect("write");
        let mut whole = String::new();
        BufReader::new(stream).read_to_string(&mut whole).expect("read");
        assert!(whole.starts_with("HTTP/1.1 200"), "{}", whole);
        whole.split_once("\r\n\r\n").map(|(_, body)| body.to_string()).unwrap_or_default()
    }

    /// Open the document as a page does, write a row through the API as the bot does, and catch up
    /// from the text the page was shown.
    fn write_and_catch_up(port: u16, qty: i64) -> String {
        let before = std::fs::read_to_string(
            std::env::temp_dir().join(format!("overseer-superseded-{}", std::process::id())).join("documents/ap31/d.os"),
        )
        .unwrap();
        ask(port, "GET", "/doc/ap31/d.os", "");
        ask(port, "POST", "/v1/node?document=ap31/d.os&address=t/Rows/%5Bc%5D/qty", &format!(r#"{{"value":{{"Integer":{}}}}}"#, qty));
        let args = serde_json::json!({
            "document": "ap31/d.os",
            "session": "page",
            "args": { "path": "ap31/d.os", "shown_version": overseer::app_api::version_of(&before) },
        });
        ask(port, "POST", "/api/catch_up_overseer_document", &args.to_string())
    }

    #[test]
    fn a_page_watching_is_described_a_write_through_the_api() {
        let server = start(4861);
        let alone = write_and_catch_up(server.port, 7);
        assert!(alone.contains(r#""changes":null"#), "nobody listened, yet something was kept: {}", alone);

        let mut watching = TcpStream::connect(("127.0.0.1", server.port)).expect("connect");
        watching
            .write_all(b"GET /v1/watch?document=ap31/d.os HTTP/1.1\r\nHost: 127.0.0.1\r\nAccept: text/event-stream\r\n\r\n")
            .unwrap();
        watching.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut first = [0u8; 256];
        let _ = watching.read(&mut first);

        let watched = write_and_catch_up(server.port, 8);
        assert!(watched.contains(r#""changes":["#), "the page was sent the whole document: {}", &watched[..watched.len().min(300)]);
        assert!(watched.contains(r#""nodes":null"#), "{}", &watched[..watched.len().min(300)]);
    }
}
