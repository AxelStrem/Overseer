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
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use axum::{routing::get, Router};
use overseer::server::{DocumentRoot, RequestError};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Clone)]
struct Service {
    documents: Arc<DocumentRoot>,
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

async fn document(
    State(service): State<Service>,
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
    let mut port: u16 = 4747;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => {
                if let Some(value) = args.next() {
                    root = std::path::PathBuf::from(value);
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
                eprintln!("usage: overseer-server [--root DIR] [--port N]");
                std::process::exit(2);
            }
        }
    }

    let documents = match DocumentRoot::new(&root) {
        Ok(r) => Arc::new(r),
        Err(e) => {
            eprintln!("cannot serve '{}': {}", root.display(), e);
            std::process::exit(1);
        }
    };
    println!("serving {} documents from {}", documents.list().len(), documents.path().display());

    let app = Router::new()
        .route("/documents", get(list))
        // A name may contain directories, so it is matched to the end of the path.
        .route("/doc/*name", get(document))
        .with_state(Service { documents });

    // Localhost only: see the note at the top of this file.
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
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
