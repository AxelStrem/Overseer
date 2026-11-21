//! A document keeps plain files beside it, in `tools/<its name>/`, through the real server:
//! whoever may read the document reads them, only whoever may change it keeps or removes them,
//! and no path reaches outside that folder.
//!
//!   cargo test --features server --test it a_document_keeps_plain_files_beside_it::

use crate::common::shared;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

const SHARING: &str = r#"div (hidden=true) {
    div Share {
        string document = ""
        string with = ""
        string can = "read"
    }
}

list Shares (entry=<Share>) {
    - {
        - document = "p.project.os"
        - with = "bob"
    }
    - {
        - document = "edited.os"
        - with = "bob"
        - can = "edit"
    }
}
"#;

struct Server {
    child: Child,
    port: u16,
    bootstrap: String,
    root: PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn write(root: &Path, name: &str, text: &[u8]) {
    let file = root.join(name);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

fn start(port: u16, name: &str) -> Server {
    let dir = std::env::temp_dir().join(format!("overseer-files-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let root = dir.join("documents");
    write(&root, "ap31/sharing.os", SHARING.as_bytes());
    write(&root, "ap31/p.project.os", b"string description = \"P\"\n");
    write(&root, "ap31/edited.os", b"string note = \"\"\n");
    write(&root, "ap31/private.os", b"string entry = \"a secret\"\n");
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
        .env("OVERSEER_TOKEN", "the-service-token-of-this-test")
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
    Server { child, port, bootstrap, root }
}

struct Answer {
    status: u16,
    headers: String,
    body: Vec<u8>,
}

impl Answer {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.lines().find_map(|l| {
            let (key, value) = l.split_once(": ")?;
            key.eq_ignore_ascii_case(name).then_some(value)
        })
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    fn session(&self) -> Option<String> {
        let value = self.header("set-cookie")?;
        let token = value.strip_prefix("overseer_session=")?.split(';').next()?;
        (!token.is_empty()).then(|| token.to_string())
    }
}

fn ask(port: u16, method: &str, path: &str, headers: &[String], body: Option<(&str, &[u8])>) -> Answer {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let mut request = format!("{} {} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n", method, path).into_bytes();
    for header in headers {
        request.extend_from_slice(header.as_bytes());
        request.extend_from_slice(b"\r\n");
    }
    match body {
        Some((kind, body)) => {
            request.extend_from_slice(format!("Content-Type: {}\r\nContent-Length: {}\r\n\r\n", kind, body.len()).as_bytes());
            request.extend_from_slice(body);
        }
        None => request.extend_from_slice(b"\r\n"),
    }
    // A server that refuses a body before reading it may close the connection while it is
    // still being sent; what it answered is still there to read.
    let _ = stream.write_all(&request);
    let mut whole = Vec::new();
    let _ = BufReader::new(stream).read_to_end(&mut whole);
    let split = whole.windows(4).position(|w| w == b"\r\n\r\n").unwrap_or(whole.len());
    let head = String::from_utf8_lossy(&whole[..split]).into_owned();
    let mut body = whole.get(split + 4..).unwrap_or_default().to_vec();
    if head.to_ascii_lowercase().contains("transfer-encoding: chunked") {
        body = unchunked(&body);
    }
    let status = head.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    Answer { status, headers: head, body }
}

fn unchunked(mut rest: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    while let Some(end) = rest.windows(2).position(|w| w == b"\r\n") {
        let size = usize::from_str_radix(String::from_utf8_lossy(&rest[..end]).trim(), 16).unwrap_or(0);
        if size == 0 {
            break;
        }
        body.extend_from_slice(&rest[end + 2..end + 2 + size]);
        rest = &rest[end + 2 + size + 2..];
    }
    body
}

fn form(port: u16, path: &str, body: &str) -> Answer {
    ask(port, "POST", path, &[], Some(("application/x-www-form-urlencoded", body.as_bytes())))
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
    let made = ask(server.port, "POST", "/account/invite", &[cookie(&ap31)], Some(("application/json", b"{}")));
    let text = made.text();
    let link = text.split("\"link\":\"").nth(1).unwrap().split('"').next().unwrap().to_string();
    let bob = form(
        server.port,
        "/join",
        &format!("invite={}&username=bob&name=Bob&password=bobs+password", invite_token(&link)),
    )
    .session()
    .expect("bob joins");
    (ap31, bob)
}

fn file(doc: &str, path: &str) -> String {
    format!("/v1/file?doc={}&path={}", doc, path)
}

fn put(port: u16, session: &str, doc: &str, path: &str, bytes: &[u8]) -> Answer {
    ask(port, "PUT", &file(doc, path), &[cookie(session)], Some(("application/octet-stream", bytes)))
}

fn read(port: u16, session: &str, doc: &str, path: &str) -> Answer {
    ask(port, "GET", &file(doc, path), &[cookie(session)], None)
}

fn remove(port: u16, session: &str, doc: &str, path: &str) -> Answer {
    ask(port, "DELETE", &file(doc, path), &[cookie(session)], None)
}

/// A link to a folder, where this system lets a test make one.
fn link_folder(target: &Path, link: &Path) -> bool {
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    #[cfg(unix)]
    return std::os::unix::fs::symlink(target, link).is_ok();
    // Without the right to make links, a junction does as well and needs none.
    #[cfg(windows)]
    return std::os::windows::fs::symlink_dir(target, link).is_ok()
        || Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link.to_string_lossy().replace('/', "\\"))
            .arg(target.to_string_lossy().replace('/', "\\"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
}

const SCRIPT: &[u8] = b"print('drive')\n\xff\x00 not text\n";

fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|b| format!("{:02x}", b)).collect()
}

#[test]
fn the_owner_keeps_reads_and_removes_a_file_beside_a_document() {
    let _shared = shared();
    let server = start(4891, "owner");
    let port = server.port;
    let (ap31, _) = ap31_and_bob(&server);

    let kept = put(port, &ap31, "p.project.os", "tests/drive.py", SCRIPT);
    assert_eq!(kept.status, 200, "{}", kept.text());
    let sha = kept.text().split("\"sha256\":\"").nth(1).unwrap().split('"').next().unwrap().to_string();
    assert_eq!(sha, sha256(SCRIPT), "{}", kept.text());
    assert_eq!(
        std::fs::read(server.root.join("ap31/tools/p.project/tests/drive.py")).unwrap(),
        SCRIPT,
        "kept in the document's own folder of files, byte for byte"
    );

    let got = read(port, &ap31, "p.project.os", "tests/drive.py");
    assert_eq!(got.status, 200, "{}", got.text());
    assert_eq!(got.body, SCRIPT);
    assert_eq!(got.header("etag"), Some(format!("\"{}\"", sha).as_str()));
    let held = ask(
        port,
        "GET",
        &file("p.project.os", "tests/drive.py"),
        &[cookie(&ap31), format!("If-None-Match: \"{}\"", sha)],
        None,
    );
    assert_eq!(held.status, 304);

    // Kept again, it is replaced.
    assert_eq!(put(port, &ap31, "p.project.os", "tests/drive.py", b"v2").status, 200);
    assert_eq!(read(port, &ap31, "p.project.os", "tests/drive.py").body, b"v2");

    let removed = remove(port, &ap31, "p.project.os", "tests/drive.py");
    assert_eq!(removed.status, 200, "{}", removed.text());
    assert_eq!(read(port, &ap31, "p.project.os", "tests/drive.py").status, 404);
    assert_eq!(remove(port, &ap31, "p.project.os", "tests/drive.py").status, 404);
}

#[test]
fn a_read_share_reads_the_files_and_only_an_edit_share_changes_them() {
    let _shared = shared();
    let server = start(4892, "shares");
    let port = server.port;
    let (ap31, bob) = ap31_and_bob(&server);
    assert_eq!(put(port, &ap31, "p.project.os", "drive.py", SCRIPT).status, 200);
    assert_eq!(put(port, &ap31, "private.os", "notes.txt", b"secret").status, 200);

    let got = read(port, &bob, "ap31/p.project.os", "drive.py");
    assert_eq!(got.status, 200, "{}", got.text());
    assert_eq!(got.body, SCRIPT);

    // Read-only: refused, and said so.
    assert_eq!(put(port, &bob, "ap31/p.project.os", "drive.py", b"mine").status, 403);
    assert_eq!(put(port, &bob, "ap31/p.project.os", "new.py", b"mine").status, 403);
    assert_eq!(remove(port, &bob, "ap31/p.project.os", "drive.py").status, 403);
    assert_eq!(read(port, &ap31, "p.project.os", "drive.py").body, SCRIPT);
    assert!(!server.root.join("ap31/tools/p.project/new.py").exists());

    // Not shared: as if there were no such document, to read and to write.
    assert_eq!(read(port, &bob, "ap31/private.os", "notes.txt").status, 404);
    assert_eq!(put(port, &bob, "ap31/private.os", "notes.txt", b"mine").status, 404);
    assert_eq!(remove(port, &bob, "ap31/private.os", "notes.txt").status, 404);
    assert_eq!(read(port, &ap31, "private.os", "notes.txt").body, b"secret");

    // A stranger is asked to sign in.
    assert_eq!(ask(port, "GET", &file("ap31/p.project.os", "drive.py"), &[], None).status, 401);

    // Shared to edit, the files are theirs to change too.
    assert_eq!(put(port, &bob, "ap31/edited.os", "a.json", b"{}").status, 200);
    assert_eq!(read(port, &ap31, "edited.os", "a.json").body, b"{}");
}

#[test]
fn no_path_leads_outside_the_documents_files() {
    let _shared = shared();
    let server = start(4893, "paths");
    let port = server.port;
    let (ap31, _) = ap31_and_bob(&server);
    for path in ["../sharing.os", "../../ap31/sharing.os", "a/../../p.project.os", "/etc/hosts", "C:/Windows/win.ini", "C:x", ""] {
        assert_eq!(read(port, &ap31, "p.project.os", path).status, 400, "reading '{}'", path);
        assert_eq!(put(port, &ap31, "p.project.os", path, b"x").status, 400, "keeping '{}'", path);
        assert_eq!(remove(port, &ap31, "p.project.os", path).status, 400, "removing '{}'", path);
    }
    assert_eq!(std::fs::read_to_string(server.root.join("ap31/sharing.os")).unwrap(), SHARING);

    // A document's own text is no file beside it.
    assert_eq!(read(port, &ap31, "p.project.os", "p.project.os").status, 404);

    // A link inside the folder that points out of it is followed neither to read nor to keep.
    let outside = server.root.join("outside");
    write(&outside, "secret.txt", b"secret");
    if link_folder(&outside, &server.root.join("ap31/tools/p.project/out")) {
        let got = read(port, &ap31, "p.project.os", "out/secret.txt");
        assert_eq!(got.status, 404, "{}", got.text());
        assert_eq!(put(port, &ap31, "p.project.os", "out/planted.txt", b"x").status, 400);
        assert_eq!(put(port, &ap31, "p.project.os", "out/deeper/planted.txt", b"x").status, 400);
        assert!(!outside.join("planted.txt").exists());
        assert!(!outside.join("deeper").exists());
        assert_eq!(remove(port, &ap31, "p.project.os", "out/secret.txt").status, 404);
        assert!(outside.join("secret.txt").exists());
    } else {
        eprintln!("no folder link could be made here; the link cases were skipped");
    }
}

#[test]
fn a_file_over_the_cap_is_neither_kept_nor_handed_out() {
    let _shared = shared();
    let server = start(4894, "cap");
    let port = server.port;
    let (ap31, _) = ap31_and_bob(&server);
    let most = overseer::app_api::FILE_MOST_BYTES as usize;
    assert_eq!(put(port, &ap31, "p.project.os", "big.bin", &vec![7u8; most]).status, 200);
    let over = put(port, &ap31, "p.project.os", "huge.bin", &vec![7u8; most + 1]);
    assert_eq!(over.status, 413, "{}", over.text());
    assert!(!server.root.join("ap31/tools/p.project/huge.bin").exists());

    // Put there by hand, it is still not handed out.
    write(&server.root, "ap31/tools/p.project/huge.bin", &vec![7u8; most + 1]);
    assert_eq!(read(port, &ap31, "p.project.os", "huge.bin").status, 400);
    assert_eq!(read(port, &ap31, "p.project.os", "big.bin").body.len(), most);
    assert_eq!(read(port, &ap31, "p.project.os", "missing.bin").status, 404);
}
