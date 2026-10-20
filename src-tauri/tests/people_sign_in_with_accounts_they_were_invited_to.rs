//! A server that keeps accounts lets people in by the invite, the password and the session it
//! gave them, and the token in as the service, which alone may say whom it acts for.
//!
//! The accounts themselves are tested in `an_account_is_made_by_an_invite_used_once`; this is
//! about whether the server applies them, so it makes real requests to the real binary.
//!
//!   cargo test --features server --test people_sign_in_with_accounts_they_were_invited_to
#![cfg(feature = "server")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};

const TOKEN: &str = "the-service-token-of-this-test";

struct Server {
    child: Child,
    port: u16,
    /// The join link the server printed for its admin.
    bootstrap: String,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start(port: u16, name: &str) -> Server {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples");
    let dir = std::env::temp_dir().join(format!("overseer-signin-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
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
    // Whatever else it says is drained, so that a full pipe never stops it.
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
    /// The value of the session cookie the answer sets.
    fn session(&self) -> Option<String> {
        self.headers.lines().find_map(|l| {
            let value = l.strip_prefix("set-cookie: ").or_else(|| l.strip_prefix("Set-Cookie: "))?;
            let value = value.strip_prefix("overseer_session=")?;
            let token = value.split(';').next()?;
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
    if let Some((kind, body)) = body {
        request.push_str(&format!("Content-Type: {}\r\nContent-Length: {}\r\n\r\n{}", kind, body.len(), body));
    } else {
        request.push_str("\r\n");
    }
    stream.write_all(request.as_bytes()).expect("write");
    let mut whole = String::new();
    BufReader::new(stream).read_to_string(&mut whole).expect("read");
    let (head, body) = whole.split_once("\r\n\r\n").unwrap_or((&whole, ""));
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or_else(|| panic!("no status in '{}'", head));
    Answer {
        status,
        headers: head.to_string(),
        body: body.to_string(),
    }
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

/// Join as the admin by the link printed at start-up, and give back the session.
fn join_admin(server: &Server) -> String {
    let token = invite_token(&server.bootstrap);
    let page = get(server.port, &server.bootstrap, &[]);
    assert_eq!(page.status, 200);
    assert!(page.body.contains("ap31"), "the form names the account it makes");
    let joined = form(
        server.port,
        "/join",
        &format!("invite={}&name=Dmitrii&password=a+good+password", token),
    );
    assert_eq!(joined.status, 303, "{}", joined.body);
    joined.session().expect("joining signs in")
}

#[test]
fn the_admin_joins_by_the_printed_link_once() {
    let server = start(4811, "bootstrap");
    let session = join_admin(&server);
    let me = get(server.port, "/account/me", &[cookie(&session)]);
    assert_eq!(me.status, 200);
    assert!(me.body.contains("\"username\":\"ap31\""), "{}", me.body);
    assert!(me.body.contains("\"name\":\"Dmitrii\""), "{}", me.body);
    assert!(me.body.contains("\"admin\":true"), "{}", me.body);
    assert_eq!(get(server.port, "/documents", &[cookie(&session)]).status, 200);

    let again = form(
        server.port,
        "/join",
        &format!("invite={}&name=Someone&password=another+password", invite_token(&server.bootstrap)),
    );
    assert_eq!(again.status, 410);
    assert!(again.session().is_none());
}

#[test]
fn nothing_is_reachable_without_signing_in() {
    let server = start(4812, "closed");
    assert_eq!(get(server.port, "/documents", &[]).status, 401);
    assert_eq!(get(server.port, "/documents", &[cookie("made-up")]).status, 401);
    // A person following a link is sent to sign in, and back where they were going afterwards.
    let page = get(server.port, "/doc/tasks/tasks.os", &["Accept: text/html".into()]);
    assert_eq!(page.status, 303);
    assert_eq!(page.location(), Some("/signin?next=/doc/tasks/tasks.os"));
    assert_eq!(get(server.port, "/signin", &[]).status, 200);
}

#[test]
fn the_password_signs_in_and_signing_out_ends_it() {
    let server = start(4813, "password");
    join_admin(&server);

    let wrong = form(server.port, "/signin", "username=ap31&password=not+it&next=/");
    assert_eq!(wrong.status, 401);
    assert!(wrong.session().is_none());
    assert!(wrong.body.contains("wrong username or password"));

    let right = form(server.port, "/signin", "username=ap31&password=a+good+password&next=%2Fdoc%2Fx.os");
    assert_eq!(right.status, 303);
    assert_eq!(right.location(), Some("/doc/x.os"));
    let session = right.session().expect("a session cookie");
    assert_eq!(get(server.port, "/documents", &[cookie(&session)]).status, 200);

    let out = ask(server.port, "POST", "/signout", &[cookie(&session)], None);
    assert_eq!(out.status, 303);
    assert_eq!(get(server.port, "/documents", &[cookie(&session)]).status, 401);
}

#[test]
fn an_admin_invites_and_resets_and_nobody_else_may() {
    let server = start(4814, "invites");
    let admin = join_admin(&server);
    let json = "application/json";

    let made = ask(server.port, "POST", "/account/invite", &[cookie(&admin)], Some((json, "{}")));
    assert_eq!(made.status, 200, "{}", made.body);
    let link = made.body.split("\"link\":\"").nth(1).unwrap().split('"').next().unwrap().to_string();
    let joined = form(
        server.port,
        "/join",
        &format!("invite={}&username=alice&name=Alice&password=alice+password", invite_token(&link)),
    );
    let alice = joined.session().expect("Alice is signed in");

    let refused = ask(server.port, "POST", "/account/invite", &[cookie(&alice)], Some((json, "{}")));
    assert_eq!(refused.status, 403);

    let reset = ask(
        server.port,
        "POST",
        "/account/invite",
        &[cookie(&admin)],
        Some((json, "{\"reset\":\"alice\"}")),
    );
    let link = reset.body.split("\"link\":\"").nth(1).unwrap().split('"').next().unwrap().to_string();
    assert!(get(server.port, &link, &[]).body.contains("A new password"));
    let renewed = form(server.port, "/join", &format!("invite={}&password=a+new+one+for+alice", invite_token(&link)));
    assert_eq!(renewed.status, 303);
    assert_eq!(get(server.port, "/documents", &[cookie(&alice)]).status, 401, "a reset signs out");
    assert_eq!(form(server.port, "/signin", "username=alice&password=a+new+one+for+alice").status, 303);
}

#[test]
fn only_the_token_may_act_for_someone() {
    let server = start(4815, "acting");
    let admin = join_admin(&server);
    let bearer = format!("Authorization: Bearer {}", TOKEN);
    let as_admin = "X-Overseer-As: ap31".to_string();

    let me = get(server.port, "/account/me", &[bearer.clone(), as_admin.clone()]);
    assert_eq!(me.status, 200);
    assert!(me.body.contains("\"username\":\"ap31\""), "{}", me.body);
    // The token alone is the service, which is no account.
    assert_eq!(get(server.port, "/account/me", &[bearer.clone()]).status, 400);
    assert_eq!(get(server.port, "/documents", &[bearer.clone()]).status, 200);
    assert_eq!(
        get(server.port, "/documents", &[bearer, "X-Overseer-As: nobody".into()]).status,
        403
    );

    // From anyone else the header changes nothing.
    assert_eq!(get(server.port, "/documents", &[as_admin.clone()]).status, 401);
    let invite = ask(server.port, "POST", "/account/invite", &[cookie(&admin)], Some(("application/json", "{}")));
    let link = invite.body.split("\"link\":\"").nth(1).unwrap().split('"').next().unwrap().to_string();
    let alice = form(
        server.port,
        "/join",
        &format!("invite={}&username=alice&name=Alice&password=alice+password", invite_token(&link)),
    )
    .session()
    .unwrap();
    let still_alice = get(server.port, "/account/me", &[cookie(&alice), as_admin]);
    assert!(still_alice.body.contains("\"username\":\"alice\""), "{}", still_alice.body);
}
