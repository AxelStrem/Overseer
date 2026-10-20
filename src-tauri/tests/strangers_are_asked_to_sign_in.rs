//! What a server that keeps accounts answers about documents, by who asks: a stranger is told
//! to sign in to anything not shared with everyone, there or not; a person is told it is not
//! there, shared with them or not; the token reaches everything, and acts for a person with
//! `X-Overseer-As`.
//!
//! The rule itself is tested in `each_persons_documents_are_their_own`; this makes real requests
//! to the real binary.
//!
//!   cargo test --features server --test strangers_are_asked_to_sign_in
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
    let dir = std::env::temp_dir().join(format!("overseer-strangers-{}-{}", name, std::process::id()));
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

    fn location(&self) -> Option<&str> {
        self.headers
            .lines()
            .find_map(|l| l.strip_prefix("location: ").or_else(|| l.strip_prefix("Location: ")))
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

#[test]
fn a_stranger_is_asked_to_sign_in_to_all_but_what_is_public() {
    let server = start(4821, "strangers");
    let port = server.port;
    let public = get(port, "/doc/ap31/recipes.os", &[]);
    assert_eq!(public.status, 200, "{}", public.body);
    assert!(public.body.contains("lentil"));

    let private = get(port, "/doc/ap31/private.os", &[]);
    let nothing = get(port, "/doc/ap31/nothing.os", &[]);
    assert_eq!(private.status, 401);
    assert!(private.body.contains("{\"error\":\"sign in\"}"), "{}", private.body);
    assert_eq!((nothing.status, nothing.body), (private.status, private.body));

    // A browser is sent to sign in to what it may not see, and shown what it may.
    let html = "Accept: text/html".to_string();
    let page = get(port, "/doc/ap31/private.os", &[html.clone()]);
    assert_eq!(page.status, 303);
    assert_eq!(page.location(), Some("/signin?next=/doc/ap31/private.os"));
    assert_ne!(get(port, "/doc/ap31/recipes.os", &[html.clone()]).status, 303);
    assert_eq!(get(port, "/", &[html]).status, 303);
    assert_eq!(get(port, "/documents", &[]).status, 401);
}

#[test]
fn a_person_is_told_what_is_not_shared_is_not_there() {
    let server = start(4822, "people");
    let port = server.port;
    let (ap31, bob) = ap31_and_bob(&server);

    let private = get(port, "/doc/ap31/private.os", &[cookie(&bob)]);
    let nothing = get(port, "/doc/ap31/nothing.os", &[cookie(&bob)]);
    assert_eq!(private.status, 404);
    assert!(private.body.contains("{\"error\":\"no document 'ap31/private.os'\"}"), "{}", private.body);
    assert_eq!(nothing.status, 404);
    assert!(nothing.body.contains("{\"error\":\"no document 'ap31/nothing.os'\"}"), "{}", nothing.body);
    assert_eq!(get(port, "/doc/ap31/private.os", &[cookie(&ap31)]).status, 200);

    // Each person's own under the same name.
    assert!(get(port, "/doc/weight.os", &[cookie(&ap31)]).body.contains("70"));
    assert!(get(port, "/doc/weight.os", &[cookie(&bob)]).body.contains("80"));

    // Shared to read: read, and refused a write.
    assert_eq!(get(port, "/doc/ap31/foods.os", &[cookie(&bob)]).status, 200);
    let write = ask(
        port,
        "POST",
        "/v1/node?document=ap31/foods.os&address=foods/apple",
        &[cookie(&bob)],
        Some(("application/json", "{\"value\":{\"String\":\"Pear\"}}")),
    );
    assert_eq!(write.status, 403, "{}", write.body);

    let listing = get(port, "/documents", &[cookie(&bob)]);
    assert_eq!(listing.status, 200);
    assert!(listing.body.contains("\"mine\":[\"bob/weight.os\"]"), "{}", listing.body);
    assert!(
        listing.body.contains("\"shared\":{\"ap31\":[\"ap31/foods.os\",\"ap31/recipes.os\"]}"),
        "{}",
        listing.body
    );
    assert!(listing.body.contains("\"public\":[\"ap31/recipes.os\"]"), "{}", listing.body);
}

#[test]
fn the_token_reaches_everything_and_acts_for_whoever_it_names() {
    let server = start(4823, "token");
    let port = server.port;
    ap31_and_bob(&server);
    let bearer = format!("Authorization: Bearer {}", TOKEN);
    assert_eq!(get(port, "/doc/ap31/private.os", &[bearer.clone()]).status, 200);
    let all = get(port, "/documents", &[bearer.clone()]);
    assert!(all.body.contains("ap31/sharing.os") && all.body.contains("bob/weight.os"), "{}", all.body);

    let as_bob = "X-Overseer-As: bob".to_string();
    assert!(get(port, "/doc/weight.os", &[bearer.clone(), as_bob.clone()]).body.contains("80"));
    assert_eq!(get(port, "/doc/ap31/private.os", &[bearer, as_bob]).status, 404);
}
