//! What the page is told of whoever is looking at a document - who they are, and whether the
//! document is theirs to change or only to read - and the sharing document it opens for them.
//!
//!   cargo test --features server --test a_page_is_told_what_its_viewer_may_do
#![cfg(feature = "server")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};

const TOKEN: &str = "the-service-token-of-this-test";

const SHARING: &str = r#"div (hidden=true) {
    div Share {
        string document = ""
        string with = ""
        string can = "read"
    }
}

list Shares (entry=<Share>) {
    - {
        - document = "foods.os"
        - with = "bob"
    }
    - {
        - document = "recipes.os"
        - with = "everyone"
    }
}
"#;

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
    let dir = std::env::temp_dir().join(format!("overseer-page-access-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let root = dir.join("documents");
    write(&root, "ap31/sharing.os", SHARING);
    write(&root, "ap31/weight.os", "int kg = 70\n");
    write(&root, "ap31/foods.os", "tab foods (mutable=true) {\n    string apple = \"Apple\"\n}\n");
    write(&root, "ap31/recipes.os", "string soup = \"lentil\"\n");
    write(&root, "ap31/private.os", "string entry = \"a secret\"\n");
    write(&root, "bob/weight.os", "int kg = 80\n");
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
}

fn ask(port: u16, method: &str, path: &str, headers: &[String], body: Option<(&str, &str)>) -> Answer {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let mut request = format!(
        "{} {} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n",
        method, path
    );
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
    let mut whole = String::new();
    BufReader::new(stream).read_to_string(&mut whole).expect("read");
    let (head, body) = whole.split_once("\r\n\r\n").unwrap_or((&whole, ""));
    let status = head.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    Answer { status, headers: head.to_string(), body: body.to_string() }
}

fn get(port: u16, path: &str, headers: &[String]) -> Answer {
    ask(port, "GET", path, headers, None)
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

/// `ap31` by the printed link, and `bob` by an invite of theirs; both signed in.
fn ap31_and_bob(server: &Server) -> (String, String) {
    let ap31 = form(
        server.port,
        "/join",
        &format!("invite={}&name=Dmitrii&password=a+good+password", invite_token(&server.bootstrap)),
    )
    .session()
    .expect("ap31 joins");
    let made = ask(server.port, "POST", "/account/invite", &[cookie(&ap31)], Some(("application/json", "{}")));
    let link = made.body.split("\"link\":\"").nth(1).unwrap().split('"').next().unwrap().to_string();
    let bob = form(
        server.port,
        "/join",
        &format!("invite={}&username=bob&name=Bob&password=bobs+password", invite_token(&link)),
    )
    .session()
    .expect("bob joins");
    (ap31, bob)
}

fn access(port: u16, document: &str, headers: &[String]) -> Answer {
    let body = format!("{{\"document\":\"{}\",\"args\":{{}}}}", document);
    ask(port, "POST", "/api/access", headers, Some(("application/json", &body)))
}

#[test]
fn each_viewer_is_told_whether_they_may_change_the_document() {
    let server = start(4831, "access");
    let port = server.port;
    let (ap31, bob) = ap31_and_bob(&server);

    let own = access(port, "ap31/foods.os", &[cookie(&ap31)]);
    assert_eq!(own.status, 200, "{}", own.body);
    assert!(own.body.contains("\"asker\":\"person\""), "{}", own.body);
    assert!(own.body.contains("\"can\":\"edit\""), "{}", own.body);
    assert!(own.body.contains("\"username\":\"ap31\"") && own.body.contains("\"admin\":true"), "{}", own.body);

    let shared = access(port, "ap31/foods.os", &[cookie(&bob)]);
    assert!(shared.body.contains("\"can\":\"read\""), "{}", shared.body);
    assert!(shared.body.contains("\"name\":\"Bob\"") && shared.body.contains("\"admin\":false"), "{}", shared.body);

    let hidden = access(port, "ap31/private.os", &[cookie(&bob)]);
    assert_eq!(hidden.status, 404, "{}", hidden.body);

    let stranger = access(port, "ap31/recipes.os", &[]);
    assert_eq!(stranger.status, 200, "{}", stranger.body);
    assert!(stranger.body.contains("\"asker\":\"stranger\""), "{}", stranger.body);
    assert!(stranger.body.contains("\"can\":\"read\""), "{}", stranger.body);
    assert!(stranger.body.contains("\"person\":null"), "{}", stranger.body);

    let token = access(port, "ap31/private.os", &[format!("Authorization: Bearer {}", TOKEN)]);
    assert!(token.body.contains("\"asker\":\"anyone\"") && token.body.contains("\"can\":\"edit\""), "{}", token.body);
}

#[test]
fn a_person_without_a_sharing_document_is_given_one_to_fill_in() {
    let server = start(4832, "sharing");
    let port = server.port;
    let (ap31, bob) = ap31_and_bob(&server);

    let made = ask(port, "POST", "/account/sharing", &[cookie(&bob)], Some(("application/json", "{}")));
    assert_eq!(made.status, 200, "{}", made.body);
    assert!(made.body.contains("\"document\":\"bob/sharing.os\""), "{}", made.body);
    let page = get(port, "/doc/bob/sharing.os", &[cookie(&bob)]);
    assert_eq!(page.status, 200, "{}", page.body);
    assert!(page.body.contains("Shares"), "{}", page.body);
    assert_eq!(get(port, "/doc/bob/sharing.os", &[cookie(&ap31)]).status, 404);

    // One there already is left as it is.
    let again = ask(port, "POST", "/account/sharing", &[cookie(&ap31)], Some(("application/json", "{}")));
    assert!(again.body.contains("\"document\":\"ap31/sharing.os\""), "{}", again.body);
    assert!(get(port, "/doc/ap31/sharing.os", &[cookie(&ap31)]).body.contains("recipes.os"));

    assert_eq!(ask(port, "POST", "/account/sharing", &[], Some(("application/json", "{}"))).status, 401);
}

#[test]
fn the_starter_sharing_document_shares_nothing() {
    assert!(overseer::access::read_shares(overseer::access::SHARING_STARTER).is_empty());
}
