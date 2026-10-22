//! A key acts as the account that made it, through the real server: it reaches what that
//! person may, writes what they may, and nothing once it is revoked. Only a signed-in person
//! makes, lists and revokes keys.
//!
//! The keys themselves are tested in `an_account_is_made_by_an_invite_used_once`.
//!
//!   cargo test --features server --test a_key_acts_as_its_account
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
    let dir = std::env::temp_dir().join(format!("overseer-keys-{}-{}", name, std::process::id()));
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

fn json(body: &str) -> Option<(&str, &str)> {
    Some(("application/json", body))
}

fn bearer(key: &str) -> String {
    format!("Authorization: Bearer {}", key)
}

/// Make a key on `session`'s account, and give it back.
fn make_key(port: u16, session: &str, name: &str) -> String {
    let made = ask(port, "POST", "/account/keys", &[cookie(session)], json(&format!("{{\"name\":\"{}\"}}", name)));
    assert_eq!(made.status, 200, "{}", made.body);
    made.body.split("\"key\":\"").nth(1).unwrap().split('"').next().unwrap().to_string()
}

#[test]
fn a_key_reaches_what_its_account_may_and_no_more() {
    let server = start(4861, "reach");
    let port = server.port;
    let (ap31, bob) = ap31_and_bob(&server);
    let ap31_key = bearer(&make_key(port, &ap31, "desktop bot"));
    let bob_key = bearer(&make_key(port, &bob, "agent"));

    let me = get(port, "/account/me", &[bob_key.clone()]);
    assert!(me.body.contains("\"username\":\"bob\""), "{}", me.body);
    assert!(get(port, "/doc/weight.os", &[bob_key.clone()]).body.contains("80"));
    assert_eq!(get(port, "/doc/ap31/foods.os", &[bob_key.clone()]).status, 200);
    assert_eq!(get(port, "/doc/ap31/private.os", &[bob_key.clone()]).status, 404);
    assert_eq!(get(port, "/doc/ap31/private.os", &[ap31_key.clone()]).status, 200);

    // Writes are refused where the account is refused them, and made where it is not.
    let set = |key: &str| {
        ask(
            port,
            "POST",
            "/v1/node?document=ap31/foods.os&address=foods/apple",
            &[key.to_string()],
            json("{\"value\":{\"String\":\"Pear\"}}"),
        )
    };
    assert_eq!(set(&bob_key).status, 403);
    let written = set(&ap31_key);
    assert_eq!(written.status, 200, "{}", written.body);
    assert!(get(port, "/doc/ap31/foods.os", &[ap31_key.clone()]).body.contains("Pear"));

    // Only the token may act for someone else.
    let still_bob = get(port, "/account/me", &[bob_key, "X-Overseer-As: ap31".into()]);
    assert!(still_bob.body.contains("\"username\":\"bob\""), "{}", still_bob.body);
}

#[test]
fn a_key_that_is_not_one_is_refused_outright() {
    let server = start(4862, "unknown");
    let port = server.port;
    let (ap31, _) = ap31_and_bob(&server);
    // Not let in as a stranger to what is public, either: the program has to learn its key is
    // wrong.
    let made_up = get(port, "/doc/ap31/recipes.os", &[bearer("osk_made-up")]);
    assert_eq!(made_up.status, 401);
    assert!(made_up.body.contains("the key is not known"), "{}", made_up.body);

    let key = make_key(port, &ap31, "desktop bot");
    assert_eq!(get(port, "/documents", &[bearer(&key)]).status, 200);
    let revoked = ask(port, "DELETE", "/account/keys/desktop%20bot", &[cookie(&ap31)], None);
    assert_eq!(revoked.status, 200, "{}", revoked.body);
    assert_eq!(get(port, "/documents", &[bearer(&key)]).status, 401);
    assert_eq!(ask(port, "DELETE", "/account/keys/desktop%20bot", &[cookie(&ap31)], None).status, 404);
}

#[test]
fn keys_are_listed_by_name_and_only_their_owner_handles_them() {
    let server = start(4863, "listed");
    let port = server.port;
    let (ap31, bob) = ap31_and_bob(&server);
    let key = make_key(port, &ap31, "desktop bot");
    let again = ask(port, "POST", "/account/keys", &[cookie(&ap31)], json("{\"name\":\"desktop bot\"}"));
    assert_eq!(again.status, 400, "{}", again.body);

    get(port, "/documents", &[bearer(&key)]);
    let listed = get(port, "/account/keys", &[cookie(&ap31)]);
    assert_eq!(listed.status, 200);
    assert!(listed.body.contains("\"name\":\"desktop bot\""), "{}", listed.body);
    assert!(listed.body.contains("\"used\":\""), "a use is noted: {}", listed.body);
    assert!(!listed.body.contains(&key), "the key is never shown again");
    assert!(get(port, "/account/keys", &[cookie(&bob)]).body.contains("\"keys\":[]"));

    // Bob cannot revoke ap31's key: Bob has none of that name.
    assert_eq!(ask(port, "DELETE", "/account/keys/desktop%20bot", &[cookie(&bob)], None).status, 404);
    assert_eq!(get(port, "/documents", &[bearer(&key)]).status, 200);

    // A stranger, and the token that is no account, make none.
    assert_eq!(ask(port, "POST", "/account/keys", &[], json("{\"name\":\"x\"}")).status, 401);
    let token = format!("Authorization: Bearer {}", TOKEN);
    assert_eq!(ask(port, "POST", "/account/keys", &[token], json("{\"name\":\"x\"}")).status, 400);
}
