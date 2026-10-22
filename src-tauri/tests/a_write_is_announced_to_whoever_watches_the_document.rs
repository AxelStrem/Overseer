//! A write is announced to whoever watches the document, and a page catches up with it.
//!
//! A page showed a document until it was reloaded, while the bot wrote to it most of the day. So
//! a page listens for its document and is told the file's version each time it moves - by a write
//! made here, at once, or by one made from outside, at the next look. It then asks for its own
//! catch-up, which is answered the way an edit is: described against the text it holds, worked
//! out as its viewer sees it, and never written.
//!
//!   cargo test --features server --test a_write_is_announced_to_whoever_watches_the_document

use overseer::app_api;
use overseer::delta;
use overseer::types::*;
use std::time::Duration;

const DOCUMENT: &str = r#"tab t (mutable=true) {
    div (hidden=true) {
        div Row (layout="horizontal") {
            string id = ""
            int qty = 0
        }
    }
    string day (mutable="guarded") = "today"
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
    let root = std::env::temp_dir().join(format!("overseer_watch_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("d.os");
    std::fs::write(&file, DOCUMENT).unwrap();
    app_api::forget_baseline();
    overseer::viewstate::forget_all();
    file.to_string_lossy().to_string()
}

/// The bot sets a row's quantity, naming the row by its key as the bot does.
fn the_bot_sets(path: &str, entry: &str, qty: i64) {
    let nodes = app_api::load_document(std::fs::read_to_string(path).unwrap()).unwrap();
    let mut field = overseer::addressing::name_path(&nodes, &format!("t/Rows/[{}]", entry)).expect("no such row");
    field.push("qty".into());
    let w: app_api::ValueWrite = serde_json::from_value(serde_json::json!({
        "node_path": field, "value": OverseerValue::Integer(qty)
    }))
    .unwrap();
    app_api::write_values_at(path, "d.os", vec![w], "bot").expect("the bot's write");
}

fn the_bot_appends(path: &str) {
    app_api::append_entry_at(
        path,
        "d.os",
        vec!["t".into(), "Rows".into()],
        [("id".to_string(), OverseerValue::String("a".into())), ("qty".to_string(), OverseerValue::Integer(5))]
            .into_iter()
            .collect(),
        "bot",
    )
    .expect("the bot's append");
}

fn heard(listening: &mut tokio::sync::broadcast::Receiver<String>, within: Duration) -> Option<String> {
    let until = std::time::Instant::now() + within;
    loop {
        match listening.try_recv() {
            Ok(version) => return Some(version),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty) if std::time::Instant::now() < until => {
                std::thread::sleep(Duration::from_millis(50))
            }
            Err(_) => return None,
        }
    }
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

/// What the page holds once it has taken the answer, as `applyDocumentChanges` or a rebuild does.
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

fn the_file(path: &str) -> Vec<OverseerNode> {
    app_api::forget_baseline();
    app_api::load_document(std::fs::read_to_string(path).unwrap()).unwrap()
}

fn value_at(nodes: &[OverseerNode], names: &[&str]) -> Option<OverseerValue> {
    let (first, rest) = names.split_first()?;
    let node = nodes.iter().find(|n| n.name == *first)?;
    if rest.is_empty() {
        return node.parameters.get("_computed_value").or_else(|| node.parameters.get("value")).cloned();
    }
    // Unnamed layout divs are no step of an address.
    value_at(&node.children, rest).or_else(|| {
        node.children
            .iter()
            .filter(|c| c.name.is_empty() || c.node_type == "div")
            .find_map(|c| value_at(&c.children, rest))
    })
}

#[test]
fn a_write_made_here_is_heard_at_once_with_the_files_new_version() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let path = a_file("here");
    let (now, mut listening) = overseer::watch::listen(std::path::Path::new(&path));
    assert_eq!(now, app_api::version_of(DOCUMENT), "listening does not say the version the file has");

    the_bot_sets(&path, "b", 4);
    let written = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        heard(&mut listening, Duration::from_millis(500)),
        Some(app_api::version_of(&written)),
        "the write was not heard straight away"
    );
    // Writing what the file already says moves nothing, and nothing is heard.
    the_bot_sets(&path, "b", 4);
    assert_eq!(heard(&mut listening, Duration::from_millis(300)), None, "a write that changed nothing was announced");
}

#[test]
fn a_write_made_from_outside_is_heard_at_the_next_look() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let path = a_file("outside");
    let (_, mut listening) = overseer::watch::listen(std::path::Path::new(&path));
    let by_hand = DOCUMENT.replace("- qty = 3", "- qty = 30");
    std::fs::write(&path, &by_hand).unwrap();
    assert_eq!(
        heard(&mut listening, overseer::watch::LOOKED_AT_EVERY * 3),
        Some(app_api::version_of(&by_hand)),
        "a write from outside was never heard"
    );
}

#[test]
fn a_page_behind_the_file_catches_up_to_what_the_file_holds() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let path = a_file("behind");
    let held = app_api::load_document_for(DOCUMENT.to_string(), "d.os", "page").expect("open");
    the_bot_appends(&path);
    the_bot_sets(&path, "c", 7);
    let file_before = std::fs::read_to_string(&path).unwrap();
    let update = {
        let _showing = app_api::describing_for(Some(app_api::version_of(DOCUMENT)));
        app_api::catch_up_at(&path, "d.os", "page").expect("the catch-up")
    };
    assert!(!update.wrote, "a catch-up said it wrote");
    let now = taken(held, update);
    assert_eq!(shown(&now), shown(&the_file(&path)), "the page does not show what the file holds");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), file_before, "a catch-up wrote the file");
}

#[test]
fn a_page_that_holds_the_file_is_told_nothing_changed() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let path = a_file("current");
    let _held = app_api::load_document_for(DOCUMENT.to_string(), "d.os", "page").expect("open");
    let update = {
        let _showing = app_api::describing_for(Some(app_api::version_of(DOCUMENT)));
        app_api::catch_up_at(&path, "d.os", "page").expect("the catch-up")
    };
    assert_eq!(update.changes.as_deref().map(|c| c.len()), Some(0), "nothing moved, but the answer says otherwise");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), DOCUMENT, "a catch-up wrote the file");
}

#[test]
fn what_is_the_viewers_comes_back_as_they_left_it() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let path = a_file("guarded");
    overseer::viewstate::set("page", "d.os", "t/day", OverseerValue::String("yesterday".into()));
    let held = app_api::load_document_for(DOCUMENT.to_string(), "d.os", "page").expect("open");
    assert_eq!(value_at(&held, &["t", "day"]), Some(OverseerValue::String("yesterday".into())));
    the_bot_sets(&path, "c", 9);
    let update = {
        let _showing = app_api::describing_for(Some(app_api::version_of(DOCUMENT)));
        app_api::catch_up_at(&path, "d.os", "page").expect("the catch-up")
    };
    let now = taken(held, update);
    assert_eq!(value_at(&now, &["t", "day"]), Some(OverseerValue::String("yesterday".into())), "the viewer's day was reset");
    assert_eq!(value_at(&now, &["t", "total"]), Some(OverseerValue::Integer(11)), "the bot's write was not caught up with");
    assert!(!std::fs::read_to_string(&path).unwrap().contains("yesterday"), "the viewer's day reached the file");
}

#[cfg(feature = "server")]
mod over_http {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpStream;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    const TOKEN: &str = "the-service-token-of-the-watch-test";

    struct Server {
        child: Child,
        port: u16,
        root: std::path::PathBuf,
    }

    impl Drop for Server {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn write(root: &std::path::Path, name: &str, text: &str) {
        let file = root.join(name);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }

    fn start(port: u16, name: &str) -> Server {
        let dir = std::env::temp_dir().join(format!("overseer-watch-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("documents");
        write(&root, "ap31/sharing.os", "div (hidden=true) {\n    div Share {\n        string document = \"\"\n        string with = \"\"\n        string can = \"read\"\n    }\n}\n\nlist Shares (entry=<Share>) {\n    - {\n        - document = \"recipes.os\"\n        - with = \"everyone\"\n    }\n}\n");
        write(&root, "ap31/weight.os", "tab t (mutable=true) {\n    int kg = 70\n}\n");
        write(&root, "ap31/recipes.os", "string soup = \"lentil\"\n");
        write(&root, "ap31/private.os", "string entry = \"a secret\"\n");
        let mut child = Command::new(env!("CARGO_BIN_EXE_overseer-server"))
            .args([
                "--root",
                root.to_string_lossy().as_ref(),
                "--port",
                &port.to_string(),
                "--accounts",
                dir.join("accounts.json").to_string_lossy().as_ref(),
                "--admin",
                "ap31",
            ])
            .env("OVERSEER_TOKEN", TOKEN)
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
        Server { child, port, root }
    }

    fn bearer() -> String {
        format!("Authorization: Bearer {}", TOKEN)
    }

    /// An open stream of what the server says about one document.
    struct Watching {
        stream: TcpStream,
        status: u16,
        read: String,
    }

    impl Watching {
        fn open(port: u16, document: &str, headers: &[String]) -> Watching {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
            let mut request = format!(
                "GET /v1/watch?document={} HTTP/1.1\r\nHost: 127.0.0.1\r\nAccept: text/event-stream\r\n",
                document
            );
            for header in headers {
                request.push_str(header);
                request.push_str("\r\n");
            }
            request.push_str("\r\n");
            stream.write_all(request.as_bytes()).expect("write");
            stream.set_read_timeout(Some(Duration::from_millis(100))).unwrap();
            let mut watching = Watching { stream, status: 0, read: String::new() };
            let until = Instant::now() + Duration::from_secs(5);
            while !watching.read.contains("\r\n\r\n") && Instant::now() < until {
                watching.more();
            }
            watching.status = watching
                .read
                .split_whitespace()
                .nth(1)
                .and_then(|c| c.parse().ok())
                .unwrap_or(0);
            watching
        }

        fn more(&mut self) {
            let mut buffer = [0u8; 4096];
            match self.stream.read(&mut buffer) {
                Ok(n) => self.read.push_str(&String::from_utf8_lossy(&buffer[..n])),
                Err(_) => {}
            }
        }

        /// The versions announced so far, waiting up to `within` for the `count`th.
        fn versions(&mut self, count: usize, within: Duration) -> Vec<String> {
            let until = Instant::now() + within;
            loop {
                let found: Vec<String> = self
                    .read
                    .split("event: changed")
                    .skip(1)
                    .filter_map(|event| {
                        let data = event.split("data: ").nth(1)?;
                        let version: String = data.chars().take_while(|c| !c.is_whitespace()).collect();
                        (!version.is_empty()).then_some(version)
                    })
                    .collect();
                if found.len() >= count || Instant::now() >= until {
                    return found;
                }
                self.more();
            }
        }
    }

    fn post(port: u16, path: &str, body: &str) -> u16 {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        let request = format!(
            "POST {} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            path,
            bearer(),
            body.len(),
            body
        );
        stream.write_all(request.as_bytes()).expect("write");
        let mut whole = String::new();
        BufReader::new(stream).read_to_string(&mut whole).expect("read");
        whole.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0)
    }

    fn version_of_file(server: &Server, name: &str) -> String {
        overseer::app_api::version_of(&std::fs::read_to_string(server.root.join(name)).unwrap())
    }

    #[test]
    fn a_watcher_hears_a_write_through_the_api_and_one_made_by_hand() {
        let server = start(4851, "heard");
        let mut watching = Watching::open(server.port, "ap31/weight.os", &[bearer()]);
        assert_eq!(watching.status, 200, "{}", watching.read);
        assert!(watching.read.to_lowercase().contains("content-type: text/event-stream"), "{}", watching.read);
        let first = watching.versions(1, Duration::from_secs(5));
        assert_eq!(first, vec![version_of_file(&server, "ap31/weight.os")], "{}", watching.read);

        let wrote = post(server.port, "/v1/node?document=ap31/weight.os&address=t/kg", r#"{"value":{"Integer":71}}"#);
        assert_eq!(wrote, 200);
        let after_the_api = watching.versions(2, Duration::from_secs(2));
        assert_eq!(after_the_api.get(1), Some(&version_of_file(&server, "ap31/weight.os")), "{}", watching.read);

        std::fs::write(server.root.join("ap31/weight.os"), "tab t (mutable=true) {\n    int kg = 72\n}\n").unwrap();
        let by_hand = watching.versions(3, overseer::watch::LOOKED_AT_EVERY * 3);
        assert_eq!(by_hand.get(2), Some(&version_of_file(&server, "ap31/weight.os")), "{}", watching.read);
        assert_eq!(by_hand.len(), 3, "each version once: {}", watching.read);
    }

    #[test]
    fn a_document_one_may_only_read_is_watched_and_one_not_shared_is_refused() {
        let server = start(4852, "refused");
        let shared = Watching::open(server.port, "ap31/recipes.os", &[]);
        assert_eq!(shared.status, 200, "a document shared with everyone could not be watched: {}", shared.read);
        let private = Watching::open(server.port, "ap31/private.os", &[]);
        assert_eq!(private.status, 401, "a stranger was let watch what is not shared: {}", private.read);
        let outside = Watching::open(server.port, "../secret.os", &[bearer()]);
        assert!(outside.status >= 400, "a name outside the root was watched: {}", outside.read);
    }
}
