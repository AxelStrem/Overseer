//! `GET /v1/source` gives a document's text as it is written, and its version, to whoever may
//! read the document - for a program that reads documents with a parser of its own, as the
//! agentic loop's tracker scripts do. It reaches what a read reaches and nothing more.
//!
//!   cargo test --features server --test a_documents_text_is_read_as_written
#![cfg(feature = "server")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};

const TOKEN: &str = "the-service-token-of-this-test";

const SHARING: &str = "div (hidden=true) {\r\n    div Share {\r\n        string document = \"\"\r\n        string with = \"\"\r\n        string can = \"read\"\r\n    }\r\n}\r\n\r\nlist Shares (entry=<Share>) {\r\n    - {\r\n        - document = \"foods.os\"\r\n        - with = \"bob\"\r\n        - can = \"edit\"\r\n    }\r\n}\r\n";

// CRLF, a comment and a field left to its template: what a parser of the text has to see as it
// is, and what a resolved read would not give back.
const FOODS: &str = "// the catalogue\r\ntab foods (mutable=true) {\r\n    string apple = \"Apple\"\r\n}\r\n";

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

fn start(port: u16, name: &str) -> Server {
    let dir = std::env::temp_dir().join(format!("overseer-source-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let root = dir.join("documents");
    write(&root, "ap31/sharing.os", SHARING);
    write(&root, "ap31/foods.os", FOODS);
    write(&root, "ap31/private.os", "string entry = \"a secret\"\r\n");
    write(&root, "bob/weight.os", "int kg = 80\r\n");
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

#[test]
fn the_text_comes_as_it_is_written_with_its_version() {
    let server = start(4871, "text");
    let key = bobs_key(&server);
    let answer = source(server.port, "ap31/foods.os", &[key.clone()]);
    assert_eq!(answer.status, 200, "{}", answer.body);
    let read = answer.json();
    assert_eq!(read["text"].as_str().unwrap(), FOODS, "the bytes as written, CRLF and comment included");
    assert_eq!(read["document"], "ap31/foods.os");
    let version = read["version"].as_str().unwrap().to_string();
    assert!(!version.is_empty());

    // A write through the API is in the next read, under a new version.
    let set = ask(
        server.port,
        "POST",
        "/v1/node?document=ap31/foods.os&address=foods/apple",
        &[key.clone()],
        json("{\"value\":{\"String\":\"Pear\"}}"),
    );
    assert_eq!(set.status, 200, "{}", set.body);
    let again = source(server.port, "ap31/foods.os", &[key]).json();
    assert!(again["text"].as_str().unwrap().contains("string apple = \"Pear\""), "{}", again["text"]);
    assert!(again["text"].as_str().unwrap().starts_with("// the catalogue\r\n"), "the layout is kept");
    assert_ne!(again["version"].as_str().unwrap(), version);
}

#[test]
fn it_reaches_what_a_read_reaches_and_no_more() {
    let server = start(4872, "reach");
    let port = server.port;
    let key = bobs_key(&server);
    // A name of bob's own is in bob's folder, and answered by its full name.
    let own = source(port, "weight.os", &[key.clone()]);
    assert_eq!(own.status, 200, "{}", own.body);
    assert_eq!(own.json()["document"], "bob/weight.os");
    // Not shared with bob: told it is not there, as any read is.
    assert_eq!(source(port, "ap31/private.os", &[key.clone()]).status, 404);
    // A stranger is asked to sign in, and a name that is no document is refused.
    assert_eq!(source(port, "ap31/foods.os", &[]).status, 401);
    assert_eq!(source(port, "ap31/foods.txt", &[key]).status, 400);
}
