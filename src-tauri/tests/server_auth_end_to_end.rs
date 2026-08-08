//! Nothing this server exposes may be reached without the token.
//!
//! The rules themselves are unit-tested; this is about whether they are actually *applied*,
//! which is a property of how the server is wired rather than of the rules. They were not, in
//! one case: the check covered the routes but not the fallback, leaving the frontend's own
//! files - the whole application - downloadable by anyone who found the port. That is the kind
//! of gap only an end-to-end request finds, so this makes real ones.
//!
//! Runs only with the `server` feature, since that is what builds the binary:
//!   cargo test --features server --test server_auth_end_to_end
#![cfg(feature = "server")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};

const TOKEN: &str = "an-entirely-secret-token";

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
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples");
    let child = Command::new(env!("CARGO_BIN_EXE_overseer-server"))
        .args([
            "--root",
            root.to_string_lossy().as_ref(),
            "--port",
            &port.to_string(),
        ])
        .env("OVERSEER_TOKEN", TOKEN)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the server should start");
    let server = Server { child, port };
    for _ in 0..100 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return server;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("the server never began listening on {}", port);
}

/// The status of a bare GET, optionally presenting the token.
fn get(port: u16, path: &str, credential: Option<&str>) -> u16 {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let auth = credential
        .map(|c| format!("{}\r\n", c))
        .unwrap_or_default();
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n{}\r\n",
        path, auth
    );
    stream.write_all(request.as_bytes()).expect("write");
    let mut reader = BufReader::new(stream);
    let mut status = String::new();
    reader.read_line(&mut status).expect("read");
    let mut rest = Vec::new();
    let _ = reader.read_to_end(&mut rest);
    status
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or_else(|| panic!("no status in '{}'", status.trim()))
}

#[test]
fn nothing_is_reachable_without_the_token() {
    let server = start(4801);
    // Every kind of thing this server serves: data, a document, the application itself.
    for path in [
        "/documents",
        "/doc/weight_tracker/tracker_v2.os",
        "/assets/nonexistent.js",
        "/",
        "/__overseer/bridge.js",
    ] {
        assert_eq!(
            get(server.port, path, None),
            401,
            "'{}' was served without a token",
            path
        );
    }
}

#[test]
fn the_token_opens_the_same_doors() {
    let server = start(4802);
    let bearer = format!("Authorization: Bearer {}", TOKEN);
    assert_eq!(get(server.port, "/documents", Some(&bearer)), 200);
    let cookie = format!("Cookie: overseer_token={}", TOKEN);
    assert_eq!(get(server.port, "/documents", Some(&cookie)), 200);
}

#[test]
fn a_wrong_token_opens_nothing() {
    let server = start(4803);
    let bearer = format!("Authorization: Bearer {}x", TOKEN);
    assert_eq!(get(server.port, "/documents", Some(&bearer)), 401);
}
