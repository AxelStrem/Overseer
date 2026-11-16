//! `GET /v1/source` names the documents a document inherits from, and each is read by that name
//! as any document's text is: one under the root by its own name, one found in a library as
//! `library:<its path there>`. A library is read only from its own folder, and only to read - for a
//! program that reads a project as text and needs the template's stages, as the agentic loop does.
//!
//!   cargo test --features server --test a_document_names_the_library_bases_it_inherits
#![cfg(feature = "server")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};

const TOKEN: &str = "the-service-token-of-this-test";

const SHARING: &str = "div (hidden=true) {\r\n    div Share {\r\n        string document = \"\"\r\n        string with = \"\"\r\n        string can = \"read\"\r\n    }\r\n}\r\n\r\nlist Shares (entry=<Share>) {\r\n    - {\r\n        - document = \"projects/tracker.os\"\r\n        - with = \"bob\"\r\n    }\r\n    - {\r\n        - document = \"deck.os\"\r\n        - with = \"bob\"\r\n    }\r\n    - {\r\n        - document = \"card.os\"\r\n        - with = \"bob\"\r\n    }\r\n}\r\n";

const TRACKER: &str = "<projects/project_template.os/project> project {\r\n    - name = \"A tracker\"\r\n}\r\n";
// A base beside the document, itself made from the library's template.
const CARD: &str = "<projects/project_template.os/project> Card { }\r\n";
const DECK: &str = "<card.os/Card> deck { }\r\n";

struct Server {
    child: Child,
    port: u16,
    bootstrap: String,
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

fn folder(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("overseer-bases-{}-{}", name, std::process::id()))
}

/// The library is a copy of the one that ships, so a test may change its template.
fn template_in(name: &str) -> std::path::PathBuf {
    folder(name).join("library/projects/project_template.os")
}

fn start(port: u16, name: &str) -> Server {
    let dir = folder(name);
    let _ = std::fs::remove_dir_all(&dir);
    let root = dir.join("documents");
    let shipped = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../library/projects/project_template.os");
    write(&dir.join("library"), "projects/project_template.os", &std::fs::read_to_string(shipped).unwrap());
    write(&dir.join("library"), "projects/README.md", "not a document\r\n");
    write(&dir, "secret.os", "string entry = \"beside the library\"\r\n");
    write(&root, "ap31/sharing.os", SHARING);
    write(&root, "ap31/projects/tracker.os", TRACKER);
    write(&root, "ap31/private.os", TRACKER);
    write(&root, "ap31/card.os", CARD);
    write(&root, "ap31/deck.os", DECK);
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
        .env("OVERSEER_LIBRARY", dir.join("library"))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the server should start");
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut bootstrap = String::new();
    let mut line = String::new();
    while out.read_line(&mut line).unwrap_or(0) > 0 {
        if let Some(at) = line.find("/join?invite=") {
            bootstrap = line[at..].split_whitespace().next().unwrap().to_string();
        }
        if line.starts_with("listening on") {
            break;
        }
        line.clear();
    }
    std::thread::spawn(move || {
        let _ = std::io::copy(&mut out, &mut std::io::sink());
    });
    assert!(!bootstrap.is_empty(), "the server printed no join link for its admin");
    Server { child, port, bootstrap }
}

struct Answer {
    status: u16,
    headers: String,
    body: String,
}

impl Answer {
    fn session(&self) -> Option<String> {
        self.headers.lines().find_map(|l| {
            let value = l.strip_prefix("set-cookie: ").or_else(|| l.strip_prefix("Set-Cookie: "))?;
            let token = value.strip_prefix("overseer_session=")?.split(';').next()?;
            (!token.is_empty()).then(|| token.to_string())
        })
    }

    fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).unwrap_or_else(|_| panic!("not JSON: {}", self.body))
    }
}

fn ask(port: u16, method: &str, path: &str, headers: &[String], body: Option<(&str, &str)>) -> Answer {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let mut request = format!("{} {} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n", method, path);
    for header in headers {
        request.push_str(header);
        request.push_str("\r\n");
    }
    match body {
        Some((kind, body)) => request.push_str(&format!(
            "Content-Type: {}\r\nContent-Length: {}\r\n\r\n{}",
            kind,
            body.len(),
            body
        )),
        None => request.push_str("\r\n"),
    }
    stream.write_all(request.as_bytes()).expect("write");
    let mut whole = Vec::new();
    BufReader::new(stream).read_to_end(&mut whole).expect("read");
    let whole = String::from_utf8_lossy(&whole).to_string();
    let (head, body) = whole.split_once("\r\n\r\n").unwrap_or((&whole, ""));
    let status = head.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    // Small answers come whole; a chunked one is undone so the body is the JSON alone.
    let body = if head.to_ascii_lowercase().contains("transfer-encoding: chunked") {
        let mut out = String::new();
        let mut rest = body;
        while let Some((size, after)) = rest.split_once("\r\n") {
            let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
            if n == 0 {
                break;
            }
            out.push_str(&after[..n]);
            rest = &after[n + 2..];
        }
        out
    } else {
        body.to_string()
    };
    Answer { status, headers: head.to_string(), body }
}

fn form(port: u16, path: &str, body: &str) -> Answer {
    ask(port, "POST", path, &[], Some(("application/x-www-form-urlencoded", body)))
}

fn cookie(session: &str) -> String {
    format!("Cookie: overseer_session={}", session)
}

fn invite_token(link: &str) -> &str {
    link.split("invite=").nth(1).unwrap()
}

fn json(body: &str) -> Option<(&str, &str)> {
    Some(("application/json", body))
}

/// `ap31` by the printed link, `bob` by ap31's invite, and a key of bob's - a program acting as
/// bob, as the loop acts as its agent's account.
fn bobs_key(server: &Server) -> String {
    let ap31 = form(
        server.port,
        "/join",
        &format!("invite={}&name=Dmitrii&password=a+good+password", invite_token(&server.bootstrap)),
    )
    .session()
    .expect("ap31 joins");
    let made = ask(server.port, "POST", "/account/invite", &[cookie(&ap31)], json("{}"));
    let link = made.json()["link"].as_str().unwrap().to_string();
    let bob = form(
        server.port,
        "/join",
        &format!("invite={}&username=bob&name=Bob&password=bobs+password", invite_token(&link)),
    )
    .session()
    .expect("bob joins");
    let key = ask(server.port, "POST", "/account/keys", &[cookie(&bob)], json("{\"name\":\"loop\"}"));
    assert_eq!(key.status, 200, "{}", key.body);
    format!("Authorization: Bearer {}", key.json()["key"].as_str().unwrap())
}

fn source(port: u16, document: &str, headers: &[String]) -> Answer {
    ask(port, "GET", &format!("/v1/source?document={}", document), headers, None)
}

const TEMPLATE: &str = "library:projects/project_template.os";

#[test]
fn a_reader_of_a_tracker_reads_the_template_it_inherits() {
    let server = start(4881, "read");
    let port = server.port;
    let key = bobs_key(&server);
    let tracker = source(port, "ap31/projects/tracker.os", &[key.clone()]);
    assert_eq!(tracker.status, 200, "{}", tracker.body);
    assert_eq!(tracker.json()["bases"], serde_json::json!([TEMPLATE]));

    let base = source(port, TEMPLATE, &[key]);
    assert_eq!(base.status, 200, "{}", base.body);
    let read = base.json();
    assert_eq!(read["document"], TEMPLATE, "named as it was asked for, not in the reader's folder");
    assert_eq!(read["text"].as_str().unwrap(), std::fs::read_to_string(template_in("read")).unwrap());
    assert!(read["text"].as_str().unwrap().contains("int template_version"), "the template says its version");
    assert_eq!(read["bases"], serde_json::json!([]));
}

#[test]
fn a_document_not_shared_names_no_bases_and_a_stranger_reads_no_library() {
    let server = start(4882, "refused");
    let port = server.port;
    let key = bobs_key(&server);
    let private = source(port, "ap31/private.os", &[key]);
    assert_eq!(private.status, 404, "{}", private.body);
    assert!(!private.body.contains("library:"), "{}", private.body);
    // The templates are published with Overseer, so an account reads one whatever it may read
    // of the documents made from it; someone not signed in is asked to sign in first.
    assert_eq!(source(port, TEMPLATE, &[]).status, 401);
}

#[test]
fn nothing_outside_the_library_is_read_by_a_library_name() {
    let server = start(4883, "outside");
    let port = server.port;
    let key = bobs_key(&server);
    for name in ["library:../secret.os", "library:projects/../../secret.os", "library:/secret.os"] {
        assert_eq!(source(port, name, &[key.clone()]).status, 404, "{}", name);
    }
    let absolute = format!("library:{}", folder("outside").join("secret.os").to_string_lossy().replace('\\', "/"));
    assert_eq!(source(port, &absolute, &[key.clone()]).status, 404, "{}", absolute);
    assert_eq!(source(port, "library:projects/missing.os", &[key.clone()]).status, 404);
    assert_eq!(source(port, "library:projects/README.md", &[key.clone()]).status, 400);
    // Written to through no route: it is the library's, not the reader's.
    let set = ask(
        port,
        "POST",
        &format!("/v1/node?document={}&address=project/name", TEMPLATE),
        &[key],
        json("{\"value\":{\"String\":\"Mine\"}}"),
    );
    assert_ne!(set.status, 200, "{}", set.body);
    assert!(!std::fs::read_to_string(template_in("outside")).unwrap().contains("Mine"));
}

#[test]
fn the_templates_version_changes_with_its_file() {
    let server = start(4884, "version");
    let port = server.port;
    let key = bobs_key(&server);
    let first = source(port, TEMPLATE, &[key.clone()]);
    assert_eq!(first.status, 200, "{}", first.body);
    let version = first.json()["version"].as_str().unwrap().to_string();
    let path = format!("/v1/source?document={}&since={}", TEMPLATE, version);
    assert_eq!(ask(port, "GET", &path, &[key.clone()], None).status, 304);

    let file = template_in("version");
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, text.replace("template_version (hidden=true) = 1", "template_version (hidden=true) = 2")).unwrap();
    let after = ask(port, "GET", &path, &[key], None);
    assert_eq!(after.status, 200, "{}", after.body);
    assert_ne!(after.json()["version"].as_str().unwrap(), version);
    assert!(after.json()["text"].as_str().unwrap().contains("template_version (hidden=true) = 2"));
}

#[test]
fn a_base_beside_the_document_is_named_as_a_document_with_what_it_inherits() {
    let server = start(4885, "beside");
    let port = server.port;
    let key = bobs_key(&server);
    let deck = source(port, "ap31/deck.os", &[key]);
    assert_eq!(deck.status, 200, "{}", deck.body);
    assert_eq!(deck.json()["bases"], serde_json::json!(["ap31/card.os", TEMPLATE]));
}
