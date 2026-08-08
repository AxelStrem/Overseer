//! Serves Overseer documents over HTTP.
//!
//! Run with:
//!
//! ```text
//! cargo run --features server --bin overseer-server -- --root ../examples
//! ```
//!
//! Read-only for now: it answers what documents exist and what one resolves to. It reuses
//! `app_api`, the same code the desktop app runs, so a document cannot mean one thing here
//! and another there.
//!
//! It binds to localhost. There is no authentication, so it should stay that way until there
//! is - a document is personal data, and this one will eventually accept writes.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::body::Body;
use axum::http::header;
use axum::response::Response;
use axum::routing::post;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use axum::{routing::get, Router};
use overseer::server::{refuse_to_start, Access, DocumentRoot, RequestError};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Clone)]
struct Service {
    documents: Arc<DocumentRoot>,
    /// Where the built frontend lives, when one is being served.
    frontend: Option<Arc<std::path::PathBuf>>,
    access: Arc<Access>,
}

/// What lets the frontend run against this server rather than the desktop app.
const BRIDGE: &str = include_str!("../bridge.js");

#[derive(serde::Deserialize)]
struct CommandBody {
    /// The document the caller is working on, so what it mounts can be resolved.
    #[serde(default)]
    document: Option<String>,
    #[serde(default)]
    args: serde_json::Value,
}

async fn command(
    State(service): State<Service>,
    Path(cmd): Path<String>,
    Json(body): Json<CommandBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let documents = service.documents.clone();
    // Resolving is CPU work, long enough on a large document to block the thread it lands on.
    let outcome = tokio::task::spawn_blocking(move || {
        documents.command(body.document.as_deref(), &cmd, &body.args)
    })
    .await
    .map_err(|e| respond(RequestError::Failed(format!("command panicked: {}", e))))?;
    Ok(Json(json!({ "result": outcome.map_err(respond)? })))
}

/// One address, two audiences: the rendered document for a browser, the data for a program.
async fn document_or_page(
    State(service): State<Service>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Response {
    if wants_a_page(&headers) && service.frontend.is_some() {
        return index(State(service)).await;
    }
    match document(State(service), headers, Path(name)).await {
        Ok(json) => json.into_response(),
        Err(err) => err.into_response(),
    }
}

async fn bridge() -> Response {
    Response::builder()
        .header(header::CONTENT_TYPE, "text/javascript; charset=utf-8")
        // The document being viewed is named in the URL, so this must not be reused across
        // documents from cache.
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(BRIDGE))
        .expect("static response")
}

/// The frontend's own index, with the bridge inserted ahead of the application.
///
/// Injected here rather than committed into the page so the built frontend stays exactly what
/// the desktop app ships - one build, serving both.
async fn index(State(service): State<Service>) -> Response {
    let Some(frontend) = service.frontend.as_ref() else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from(
                "No frontend was built. Run `npm run build`, then start with --frontend ../dist
",
            ))
            .expect("static response");
    };
    match std::fs::read_to_string(frontend.join("index.html")) {
        Ok(html) => {
            let tag = "<script src=\"/__overseer/bridge.js\"></script>";
            let injected = match html.find("</head>") {
                Some(at) => format!("{}{}{}", &html[..at], tag, &html[at..]),
                None => format!("{}{}", tag, html),
            };
            Response::builder()
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .body(Body::from(injected))
                .expect("static response")
        }
        Err(e) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(format!("could not read the frontend: {}", e)))
            .expect("static response"),
    }
}


/// The name of the cookie a browser carries once it has presented the token.
const TOKEN_COOKIE: &str = "overseer_token";

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())?
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == TOKEN_COOKIE)
        .map(|(_, value)| value.to_string())
}

/// Refuse anything that has not presented the token.
///
/// Applied to everything, including the frontend's own files: the point is that a stranger who
/// finds the port learns nothing, and serving them the application would be a start.
async fn require_access(
    State(service): State<Service>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let headers = request.headers();
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let cookie = cookie_token(headers);
    if service.access.permits(bearer.as_deref(), cookie.as_deref()) {
        return next.run(request).await;
    }
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": "a token is required" })),
    )
        .into_response()
}

/// Exchange a token in the address for a cookie, so a browser can follow links afterwards.
///
/// This runs before the check above, because presenting the token is how a browser gets in.
async fn sign_in(
    State(service): State<Service>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let presented = params.get("token").map(|s| s.as_str());
    if !service.access.permits(None, presented) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "that is not the token" })),
        )
            .into_response();
    }
    let next = params.get("next").map(|s| s.as_str()).unwrap_or("/");
    // Refuse to bounce anywhere but back into this server.
    let next = if next.starts_with('/') && !next.starts_with("//") { next } else { "/" };
    let cookie = format!(
        // HttpOnly: no part of the page needs to read this, and nothing that cannot read it
        // can leak it. SameSite=Strict: another site must not be able to ride the session.
        "{}={}; Path=/; HttpOnly; SameSite=Strict",
        TOKEN_COOKIE,
        presented.unwrap_or_default()
    );
    Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::LOCATION, next)
        .header(header::SET_COOKIE, cookie)
        .body(Body::empty())
        .expect("static response")
}

/// Report what went wrong without describing the filesystem to whoever asked.
fn respond(error: RequestError) -> (StatusCode, Json<serde_json::Value>) {
    let status = match error {
        RequestError::Rejected(_) => StatusCode::BAD_REQUEST,
        RequestError::NotFound(_) => StatusCode::NOT_FOUND,
        RequestError::Failed(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json(json!({ "error": error.to_string() })))
}

async fn list(State(service): State<Service>) -> impl IntoResponse {
    Json(json!({ "documents": service.documents.list() }))
}

/// Whether the caller is a browser asking to look at something.
///
/// The same URL serves the document either way: a person following a link gets the document
/// rendered, a program asking for data gets the data. Having one address mean two things
/// depending on a header is a small cost against having a link that shows a wall of JSON to
/// anyone who pastes it into a browser.
fn wants_a_page(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|accept| accept.contains("text/html"))
        .unwrap_or(false)
}

async fn document(
    State(service): State<Service>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // Resolving is CPU work rather than IO, and long enough on a large document to block the
    // runtime thread it lands on, so it runs where blocking is expected.
    let documents = service.documents.clone();
    let resolved = tokio::task::spawn_blocking(move || documents.open(&name))
        .await
        .map_err(|e| respond(RequestError::Failed(format!("resolving panicked: {}", e))))?
        .map_err(respond)?;
    Ok(Json(json!({ "nodes": resolved })))
}

#[tokio::main]
async fn main() {
    let mut root = std::path::PathBuf::from("../examples");
    let mut frontend_dir = std::path::PathBuf::from("../dist");
    let mut port: u16 = 4747;
    let mut bind: std::net::IpAddr = std::net::IpAddr::from([127, 0, 0, 1]);
    // Read from the environment or a file rather than an argument: a command line ends up in
    // shell history and in the process list, where a secret has no business being.
    let mut token = std::env::var("OVERSEER_TOKEN").ok().filter(|t| !t.is_empty());
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => {
                if let Some(value) = args.next() {
                    root = std::path::PathBuf::from(value);
                }
            }
            "--bind" => {
                if let Some(value) = args.next() {
                    match value.parse() {
                        Ok(a) => bind = a,
                        Err(_) => {
                            eprintln!("--bind takes an address, got '{}'", value);
                            std::process::exit(2);
                        }
                    }
                }
            }
            "--token-file" => {
                if let Some(value) = args.next() {
                    match std::fs::read_to_string(&value) {
                        Ok(t) => token = Some(t.trim().to_string()).filter(|t| !t.is_empty()),
                        Err(e) => {
                            eprintln!("cannot read the token from '{}': {}", value, e);
                            std::process::exit(2);
                        }
                    }
                }
            }
            "--frontend" => {
                if let Some(value) = args.next() {
                    frontend_dir = std::path::PathBuf::from(value);
                }
            }
            "--port" => {
                if let Some(value) = args.next() {
                    match value.parse() {
                        Ok(p) => port = p,
                        Err(_) => {
                            eprintln!("--port takes a number, got '{}'", value);
                            std::process::exit(2);
                        }
                    }
                }
            }
            other => {
                eprintln!("unknown argument '{}'", other);
                eprintln!(
                    "usage: overseer-server [--root DIR] [--frontend DIR] [--bind ADDR] [--port N] [--token-file PATH]"
                );
                std::process::exit(2);
            }
        }
    }

    let access = match token {
        Some(t) => Access::with_token(t),
        None => Access::unrestricted(),
    };
    if let Some(reason) = refuse_to_start(&bind, &access) {
        eprintln!("{}", reason);
        std::process::exit(2);
    }

    let documents = match DocumentRoot::new(&root) {
        Ok(r) => Arc::new(r),
        Err(e) => {
            eprintln!("cannot serve '{}': {}", root.display(), e);
            std::process::exit(1);
        }
    };
    println!("serving {} documents from {}", documents.list().len(), documents.path().display());

    // Serving the frontend is optional: without a build there is still a usable JSON API.
    let frontend = match frontend_dir.canonicalize() {
        Ok(dir) if dir.join("index.html").exists() => {
            println!("serving the frontend from {}", dir.display());
            Some(Arc::new(dir))
        }
        _ => {
            println!(
                "no frontend at {} - run `npm run build` to view documents in a browser",
                frontend_dir.display()
            );
            None
        }
    };

    let access = Arc::new(access);
    let state = Service {
        documents: documents.clone(),
        frontend: frontend.clone(),
        access: access.clone(),
    };

    let mut app = Router::new()
        .route("/documents", get(list))
        // A name may contain directories, so it is matched to the end of the path.
        .route("/doc/*name", get(document_or_page))
        .route("/api/:cmd", post(command))
        .route("/__overseer/bridge.js", get(bridge))
        .route("/", get(index));
    if let Some(dir) = frontend.as_ref() {
        // Everything else is the frontend's own assets.
        app = app.fallback_service(tower_http::services::ServeDir::new(dir.as_ref()));
    }
    // `layer` rather than `route_layer`: the latter covers routes but not the fallback, which
    // would have left the frontend's own files reachable without a token. A stranger who finds
    // the port should not be handed the application, never mind what it can read.
    let app = app.layer(axum::middleware::from_fn_with_state(
        state.clone(),
        require_access,
    ));
    // Presenting the token is how a browser gets in, so this sits outside that check.
    let app = Router::new()
        .route("/auth", get(sign_in))
        .merge(app)
        .with_state(state);

    let addr = SocketAddr::new(bind, port);
    if access.is_restricted() {
        println!("a token is required; a browser can present it once at /auth?token=...");
    } else {
        println!("no token set, so anything on this machine may read these documents");
    }
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cannot listen on {}: {}", addr, e);
            std::process::exit(1);
        }
    };
    println!("listening on http://{}", addr);
    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("server stopped: {}", e);
        std::process::exit(1);
    }
}
