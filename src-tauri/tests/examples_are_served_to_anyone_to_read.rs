//! A server given a folder of examples serves them to anyone at all under `/examples/`, with no
//! token or sign-in, to read and never to change: a press that would write an example is refused,
//! and one that moves only what the document keeps for whoever is looking goes through. The
//! examples are a root of their own, so no name asked for there reaches anyone's documents.
//!
//!   cargo test --features server --test examples_are_served_to_anyone_to_read
#![cfg(feature = "server")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};

const TOKEN: &str = "the-service-token-of-this-test";

const TASKS: &str = r#"tab tasks (mutable=true) {
    int done = 0

    button finish {
        on click {
            inc (path="../done")
        }
    }
}
"#;

const GAME: &str = r#"tab game {
    int moves (mutable="guarded") = 0

    button play {
        on click {
            inc (path="../moves")
        }
    }
}
"#;

struct Server {
    child: Child,
    port: u16,
    examples: std::path::PathBuf,
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

fn start(port: u16, name: &str, with_examples: bool) -> Server {
    let dir = std::env::temp_dir().join(format!("overseer-examples-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let root = dir.join("documents");
    let examples = dir.join("examples");
    let frontend = dir.join("frontend");
    write(&root, "examples/secret.os", "string entry = \"a secret\"\n");
    write(&root, "tasks.os", "string mine = \"my own tasks\"\n");
    write(&examples, "tasks.os", TASKS);
    write(&examples, "games/game.os", GAME);
    write(&frontend, "index.html", "<html><head></head><body></body></html>");
    write(&frontend, "assets/page.js", "console.log('the page')");
    let mut args = vec![
        "--root".to_string(),
        root.to_string_lossy().into_owned(),
        "--frontend".to_string(),
        frontend.to_string_lossy().into_owned(),
        "--port".to_string(),
        port.to_string(),
    ];
    if with_examples {
        args.push("--examples".into());
        args.push(examples.to_string_lossy().into_owned());
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_overseer-server"))
        .args(&args)
        .env("OVERSEER_TOKEN", TOKEN)
        .env_remove("OVERSEER_EXAMPLES")
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
    Server { child, port, examples }
}

struct Answer {
    status: u16,
    body: String,
}

fn ask(port: u16, method: &str, path: &str, headers: &[&str], body: Option<&str>) -> Answer {
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
        Some(body) => request.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
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
    Answer { status, body: body.to_string() }
}

const JSON: &str = "Accept: application/json";
const PAGE: &str = "Accept: text/html";

fn command(server: &Server, cmd: &str, document: &str, args: &str) -> Answer {
    let body = format!(
        r#"{{"document":"{}","session":"a-visitor","args":{}}}"#,
        document, args
    );
    ask(server.port, "POST", &format!("/examples/api/{}", cmd), &[], Some(&body))
}

fn press(server: &Server, document: &str, node_path: &str) -> Answer {
    command(
        server,
        "run_overseer_event",
        document,
        &format!(r#"{{"node_path":{},"event_name":"click"}}"#, node_path),
    )
}

#[test]
fn an_example_opens_with_no_token() {
    let server = start(47901, "opens", true);
    let answer = ask(server.port, "GET", "/examples/games/game.os", &[JSON], None);
    assert_eq!(answer.status, 200, "{}", answer.body);
    assert!(answer.body.contains("moves"), "{}", answer.body);

    let page = ask(server.port, "GET", "/examples/games/game.os", &[PAGE], None);
    assert_eq!(page.status, 200, "{}", page.body);
    assert!(page.body.contains("/__overseer/bridge.js"), "{}", page.body);
    // What the page needs to load, for someone with no token.
    assert_eq!(ask(server.port, "GET", "/__overseer/bridge.js", &[], None).status, 200);
    assert_eq!(ask(server.port, "GET", "/assets/page.js", &[], None).status, 200);

    let access = command(&server, "access", "games/game.os", "{}");
    assert_eq!(access.status, 200, "{}", access.body);
    assert!(access.body.contains(r#""asker":"stranger""#), "{}", access.body);
    assert!(access.body.contains(r#""can":"read""#), "{}", access.body);
}

#[test]
fn a_press_that_would_change_an_example_is_refused() {
    let server = start(47902, "refused", true);
    let answer = press(&server, "tasks.os", r#"["tasks","finish"]"#);
    assert_eq!(answer.status, 403, "{}", answer.body);
    // As a sentence for whoever pressed, which the page shows beside what was pressed.
    assert!(answer.body.contains(r#""refused":"this document is shared with you to read, not to change""#), "{}", answer.body);
    assert_eq!(std::fs::read_to_string(server.examples.join("tasks.os")).unwrap(), TASKS);

    let save = command(
        &server,
        "save_overseer_file",
        "tasks.os",
        r#"{"path":"tasks.os","content":"int done = 5\n"}"#,
    );
    assert_eq!(save.status, 403, "{}", save.body);
    assert_eq!(std::fs::read_to_string(server.examples.join("tasks.os")).unwrap(), TASKS);

    // Nothing that writes by address is served there at all.
    let append = ask(server.port, "POST", "/examples/v1/append?document=tasks.os&address=tasks", &[], Some("{}"));
    assert_ne!(append.status, 200, "{}", append.body);
    let undo = ask(server.port, "POST", "/examples/v1/undo", &[], Some(r#"{"document":"tasks.os"}"#));
    assert_ne!(undo.status, 200, "{}", undo.body);
}

#[test]
fn a_press_that_moves_only_what_the_visitor_sees_goes_through() {
    let server = start(47903, "guarded", true);
    let answer = press(&server, "games/game.os", r#"["game","play"]"#);
    assert_eq!(answer.status, 200, "{}", answer.body);
    assert_eq!(std::fs::read_to_string(server.examples.join("games/game.os")).unwrap(), GAME);
}

#[test]
fn no_name_under_examples_reaches_anyones_documents() {
    let server = start(47904, "apart", true);
    // A person's own folder called `examples` is theirs, behind the token, at /doc/examples/.
    assert_eq!(ask(server.port, "GET", "/doc/examples/secret.os", &[JSON], None).status, 401);
    let token = format!("Authorization: Bearer {}", TOKEN);
    let theirs = ask(server.port, "GET", "/doc/examples/secret.os", &[JSON, &token], None);
    assert_eq!(theirs.status, 200, "{}", theirs.body);
    assert_eq!(ask(server.port, "GET", "/examples/examples/secret.os", &[JSON], None).status, 404);

    let climbing = ask(server.port, "GET", "/examples/%2E%2E/documents/tasks.os", &[JSON], None);
    assert_eq!(climbing.status, 400, "{}", climbing.body);
    assert!(!climbing.body.contains("my own tasks"));
    let source = ask(server.port, "GET", "/examples/v1/source?document=..%2Fdocuments%2Ftasks.os", &[JSON], None);
    assert!(!source.body.contains("my own tasks"), "{}", source.body);

    // The rest stays behind the token.
    assert_eq!(ask(server.port, "GET", "/", &[PAGE], None).status, 401);
    assert_eq!(ask(server.port, "GET", "/documents", &[JSON], None).status, 401);
    assert_eq!(ask(server.port, "GET", "/doc/tasks.os", &[JSON], None).status, 401);
}

#[test]
fn without_the_flag_nothing_is_served_without_a_token() {
    let server = start(47905, "off", false);
    assert_eq!(ask(server.port, "GET", "/examples/tasks.os", &[JSON], None).status, 401);
    assert_eq!(ask(server.port, "GET", "/__overseer/bridge.js", &[], None).status, 401);
    assert_eq!(ask(server.port, "GET", "/assets/page.js", &[], None).status, 401);
    let token = format!("Authorization: Bearer {}", TOKEN);
    assert_eq!(ask(server.port, "GET", "/__overseer/bridge.js", &[&token], None).status, 200);
    assert_ne!(ask(server.port, "GET", "/examples/tasks.os", &[JSON, &token], None).status, 200);
}
