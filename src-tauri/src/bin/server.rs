//! Serves Overseer documents over HTTP.
//!
//! Run with:
//!
//! ```text
//! cargo run --features server --bin overseer-server -- --root ../examples
//! ```
//!
//! It answers what documents exist and what one resolves to, and writes them. It reuses
//! `app_api`, the same code the desktop app runs, so a document cannot mean one thing here
//! and another there.
//!
//! It binds to localhost unless told otherwise, and beyond it refuses to start without a token
//! (`OVERSEER_TOKEN` or `--token-file`) unless `--open` says that everyone who can reach it may
//! read and write the documents. Every flag has an environment variable too, for a container;
//! DEPLOYING.md lists them.
//!
//! Given an accounts file (`OVERSEER_ACCOUNTS` or `--accounts`), people sign in to it as
//! themselves too - see `overseer::accounts`. The token goes on meaning the service: whoever
//! holds it may do anything, and may say whom it acts for.
//!
//! Given a folder of examples (`OVERSEER_EXAMPLES` or `--examples`), it serves those to anyone at
//! all under `/examples/`, with no token or sign-in, to read and never to change.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::body::Body;
use axum::http::header;
use axum::response::Response;
use axum::routing::post;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use axum::{routing::get, Router};
use axum::Extension;
use overseer::access::Asker;
use overseer::accounts::{AccountError, Accounts, User};
use overseer::server::{
    from_environment, is_loopback, is_switched_on, refuse_to_start, Access, DocumentRoot,
    RequestError,
};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Clone)]
struct Service {
    documents: Arc<DocumentRoot>,
    /// Where the built frontend lives, when one is being served.
    frontend: Option<Arc<std::path::PathBuf>>,
    access: Arc<Access>,
    /// The people who may sign in, when the server keeps any.
    accounts: Option<Arc<Accounts>>,
}

/// Who a request comes from, once the access check has let it in.
#[derive(Clone)]
enum Caller {
    /// Holds the server's token, or the server asks for none: everything is open to it.
    Service,
    /// Someone signed in, or the service acting for them.
    Person(User),
    /// Nobody signed in, on a server that keeps accounts: what is shared with everyone is
    /// theirs to read, and nothing else - see `overseer::access`.
    Stranger,
    /// Anyone at all, looking at the examples: theirs to read, and none of them to change.
    Visitor,
}

impl Caller {
    fn asker(&self) -> Asker {
        match self {
            Caller::Service => Asker::Anyone,
            Caller::Person(user) => Asker::Person(user.username.clone()),
            Caller::Stranger => Asker::Stranger,
            Caller::Visitor => Asker::Visitor,
        }
    }
}

/// Every request to the examples comes from a visitor, whoever sent it: nothing there is theirs
/// to change, the token's holder included.
async fn as_visitor(mut request: axum::extract::Request, next: axum::middleware::Next) -> Response {
    request.extensions_mut().insert(Caller::Visitor);
    next.run(request).await
}

/// What a stranger is told of anything not shared with everyone, there or not.
fn sign_in_first() -> (StatusCode, Json<serde_json::Value>) {
    respond(RequestError::SignIn)
}

/// What lets the frontend run against this server rather than the desktop app.
const BRIDGE: &str = include_str!("../bridge.js");

/// The most a command's body may weigh: room for the largest image kept, in base64, and the
/// JSON around it.
const API_MOST_BYTES: usize = (overseer::app_api::IMAGE_MOST_BYTES as usize) / 3 * 4 + 1024 * 1024;

#[derive(serde::Deserialize)]
struct CommandBody {
    /// The document the caller is working on, so what it mounts can be resolved.
    #[serde(default)]
    document: Option<String>,
    /// Which viewer is asking, so what they are looking at is kept apart from what everyone
    /// else is. Absent means a caller with no view of its own - a script, or the bot.
    #[serde(default)]
    session: Option<String>,
    /// The scheme the page is in, light or dark, which a `mode` of "system" stands for - see
    /// `overseer::environment`. Absent for a caller with no page, which is drawn light.
    #[serde(default)]
    scheme: Option<String>,
    #[serde(default)]
    args: serde_json::Value,
}

async fn command(
    State(service): State<Service>,
    Extension(caller): Extension<Caller>,
    Path(cmd): Path<String>,
    Json(body): Json<CommandBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let documents = service.documents.clone();
    let person = match &caller {
        Caller::Person(user) => Some(user.clone()),
        _ => None,
    };
    let asked = cmd.clone();
    // What a visitor is looking at is kept apart from what the same tab looked at in someone's
    // documents, where a document of the same name may be quite another.
    let session = match (&caller, body.session) {
        (Caller::Visitor, Some(session)) => Some(format!("examples:{}", session)),
        (_, session) => session,
    };
    // Resolving is CPU work, long enough on a large document to block the thread it lands on.
    let outcome = tokio::task::spawn_blocking(move || {
        documents.as_asker(caller.asker(), || {
            overseer::environment::seen_as(body.scheme.as_deref(), || {
                documents.command_for(
                    session.as_deref().unwrap_or(""),
                    body.document.as_deref(),
                    &cmd,
                    &body.args,
                )
            })
        })
    })
    .await
    .map_err(|e| respond(RequestError::Failed(format!("command panicked: {}", e))))?;
    let mut result = outcome.map_err(respond)?;
    // Who the person is lives in the accounts, which the documents know nothing of.
    if asked == "access" {
        if let Some(answer) = result.as_object_mut() {
            answer.insert("person".into(), json!(person));
        }
    }
    Ok(Json(json!({ "result": result })))
}

/// One address, two audiences: the rendered document for a browser, the data for a program.
async fn document_or_page(
    State(service): State<Service>,
    Extension(caller): Extension<Caller>,
    headers: HeaderMap,
    uri: axum::http::Uri,
    Path(name): Path<String>,
) -> Response {
    // A stranger following a link to what is not shared with everyone is better served by the
    // way in than by a page that cannot open it.
    if wants_a_page(&headers) && matches!(caller, Caller::Stranger) {
        let documents = service.documents.clone();
        let asked = name.clone();
        let public = tokio::task::spawn_blocking(move || {
            documents.as_asker(Asker::Stranger, || documents.resolve(&asked).is_ok())
        })
        .await
        .unwrap_or(false);
        if !public {
            return to_sign_in(&uri);
        }
    }
    if wants_a_page(&headers) && service.frontend.is_some() {
        return index(State(service), Extension(caller), uri).await;
    }
    match document(State(service), Extension(caller), Path(name)).await {
        Ok(json) => json.into_response(),
        Err(err) => err.into_response(),
    }
}

/// The same for an example: the page for a browser, the document for a program.
async fn example_or_page(
    State(service): State<Service>,
    headers: HeaderMap,
    uri: axum::http::Uri,
    Path(name): Path<String>,
) -> Response {
    if wants_a_page(&headers) && service.frontend.is_some() {
        return index(State(service), Extension(Caller::Visitor), uri).await;
    }
    match document(State(service), Extension(Caller::Visitor), Path(name)).await {
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
async fn index(
    State(service): State<Service>,
    Extension(caller): Extension<Caller>,
    uri: axum::http::Uri,
) -> Response {
    // The list of documents is no stranger's: what is shared with everyone is reached by its link.
    if matches!(caller, Caller::Stranger) && uri.path() == "/" {
        return to_sign_in(&uri);
    }
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

/// The name of the cookie a browser carries once someone has signed in.
const SESSION_COOKIE: &str = "overseer_session";

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    cookie_named(headers, TOKEN_COOKIE)
}

fn cookie_named(headers: &HeaderMap, wanted: &str) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())?
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == wanted)
        .map(|(_, value)| value.to_string())
}

/// Let in what has presented the token or signed in, and say who it is; refuse the rest.
///
/// Applied to everything, including the frontend's own files: the point is that a stranger who
/// finds the port learns nothing, and serving them the application would be a start.
async fn require_access(
    State(service): State<Service>,
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let headers = request.headers();
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let cookie = cookie_token(headers);
    let caller = if service.access.is_restricted()
        && service.access.permits(bearer.as_deref(), cookie.as_deref())
    {
        // Only the token may say whom it acts for: from anyone else the header would be a way
        // to become someone without their password.
        match acting_for(headers) {
            None => Some(Caller::Service),
            Some(username) => match service.accounts.as_ref().and_then(|a| a.user(&username)) {
                Some(user) => Some(Caller::Person(user)),
                None => {
                    return (
                        StatusCode::FORBIDDEN,
                        Json(json!({ "error": "there is no account of that name" })),
                    )
                        .into_response()
                }
            },
        }
    } else if let Some(accounts) = service.accounts.as_ref() {
        // A program presents a key of the account it acts as. A key that is not one - revoked,
        // or mistyped - is refused outright rather than let in as a stranger, so the program
        // learns of it instead of quietly seeing only what is public.
        if let Some(key) = bearer.as_deref().and_then(bearer_secret) {
            return match accounts.by_key(key, chrono::Utc::now()) {
                Some(user) => {
                    request.extensions_mut().insert(Caller::Person(user));
                    next.run(request).await
                }
                None => (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": "the key is not known" })),
                )
                    .into_response(),
            };
        }
        // Nobody signed in is still let through, as a stranger: each document says whether it
        // is shared with everyone, and the page has to load for one that is to be drawn.
        Some(
            cookie_named(headers, SESSION_COOKIE)
                .and_then(|token| accounts.session(&token, chrono::Utc::now()))
                .map_or(Caller::Stranger, Caller::Person),
        )
    } else if !service.access.is_restricted() {
        Some(Caller::Service)
    } else {
        None
    };
    if let Some(caller) = caller {
        request.extensions_mut().insert(caller);
        return next.run(request).await;
    }
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": "a token is required" })),
    )
        .into_response()
}

/// Send a browser to sign in, and back here afterwards.
fn to_sign_in(uri: &axum::http::Uri) -> Response {
    let here = uri
        .path_and_query()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| "/".into());
    redirect(&format!("/signin?next={}", escaped_for_query(&here)), None)
}

/// The account the token says it acts for, in `X-Overseer-As`.
/// What an `Authorization: Bearer ...` header presents.
fn bearer_secret(value: &str) -> Option<&str> {
    value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
        .map(str::trim)
        .filter(|v| !v.is_empty())
}

fn acting_for(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-overseer-as")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim().to_lowercase())
        .filter(|v| !v.is_empty())
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
    let next = staying_here(params.get("next").map(|s| s.as_str()));
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


/// Refuse to bounce anywhere but back into this server.
fn staying_here(next: Option<&str>) -> &str {
    match next {
        Some(next) if next.starts_with('/') && !next.starts_with("//") => next,
        _ => "/",
    }
}

/// Text as it may stand in a query string, every character but the plainest escaped.
fn escaped_for_query(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                (b as char).to_string()
            }
            _ => format!("%{:02X}", b),
        })
        .collect()
}

fn escaped_for_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn redirect(to: &str, cookie: Option<String>) -> Response {
    let mut response = Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::LOCATION, to);
    if let Some(cookie) = cookie {
        response = response.header(header::SET_COOKIE, cookie);
    }
    response.body(Body::empty()).expect("static response")
}

/// The cookie that keeps someone signed in, or with no token, the one that ends it.
///
/// SameSite=Lax rather than the token cookie's Strict: a link to a document, followed from a
/// message, should open it rather than ask to sign in again. Secure when the request came over
/// HTTPS, as it does through Railway's proxy, and not otherwise, so that a server on a laptop
/// can still be signed in to.
fn session_cookie(headers: &HeaderMap, token: Option<&str>) -> String {
    let secure = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("https"));
    let lasts = match token {
        Some(_) => overseer::accounts::SESSION_LASTS.num_seconds(),
        None => 0,
    };
    format!(
        "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
        SESSION_COOKIE,
        token.unwrap_or_default(),
        lasts,
        if secure { "; Secure" } else { "" }
    )
}

/// A page of its own, needing nothing else from the server: it is shown to people who cannot
/// yet be given anything else, the application's own styles included.
fn form_page(status: StatusCode, title: &str, said: Option<&str>, body: &str) -> Response {
    let said = said
        .map(|s| format!("<p class=\"said\">{}</p>", escaped_for_html(s)))
        .unwrap_or_default();
    let html = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<title>{title} - Overseer</title><style>\
body{{font-family:system-ui,sans-serif;background:#f1f5f9;color:#1e293b;display:flex;\
justify-content:center;padding:12vh 16px;margin:0}}\
main{{background:#fff;border-radius:12px;padding:28px;width:100%;max-width:340px;\
box-shadow:0 1px 3px #0002}}\
h1{{font-size:20px;margin:0 0 18px}}label{{display:block;font-size:13px;margin:12px 0 4px}}\
input{{box-sizing:border-box;width:100%;padding:8px 10px;font:inherit;border:1px solid #cbd5e1;\
border-radius:6px}}button{{margin-top:20px;width:100%;padding:9px;font:inherit;border:0;\
border-radius:6px;background:#2563eb;color:#fff;cursor:pointer}}\
.said{{color:#b91c1c;font-size:14px}}.fixed{{font-weight:600}}\
@media (prefers-color-scheme:dark){{body{{background:#0f172a;color:#e2e8f0}}\
main{{background:#1e293b}}input{{background:#0f172a;color:inherit;border-color:#334155}}}}\
</style></head><body><main><h1>{title}</h1>{said}{body}</main></body></html>",
        title = escaped_for_html(title),
        said = said,
        body = body,
    );
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(html))
        .expect("static response")
}

fn no_accounts() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "this server keeps no accounts" })),
    )
        .into_response()
}

type Fields = std::collections::HashMap<String, String>;

fn field<'a>(fields: &'a Fields, name: &str) -> &'a str {
    fields.get(name).map(|s| s.as_str()).unwrap_or("")
}

fn sign_in_form(status: StatusCode, next: &str, said: Option<&str>) -> Response {
    form_page(
        status,
        "Sign in",
        said,
        &format!(
            "<form method=\"post\" action=\"/signin\">\
<input type=\"hidden\" name=\"next\" value=\"{next}\">\
<label for=\"username\">Username</label>\
<input id=\"username\" name=\"username\" autocomplete=\"username\" autocapitalize=\"none\" required autofocus>\
<label for=\"password\">Password</label>\
<input id=\"password\" name=\"password\" type=\"password\" autocomplete=\"current-password\" required>\
<button>Sign in</button></form>",
            next = escaped_for_html(next)
        ),
    )
}

async fn sign_in_page(
    State(service): State<Service>,
    axum::extract::Query(params): axum::extract::Query<Fields>,
) -> Response {
    if service.accounts.is_none() {
        return no_accounts();
    }
    sign_in_form(StatusCode::OK, staying_here(params.get("next").map(|s| s.as_str())), None)
}

// Hashing a password takes a few tens of milliseconds of one core. That is done where the
// request is, rather than on the blocking pool, which is two threads wide and may be in the
// middle of a long resolve: a sign-in should not wait half a minute behind someone's tracker.
async fn sign_in_with_password(
    State(service): State<Service>,
    headers: HeaderMap,
    axum::extract::Form(fields): axum::extract::Form<Fields>,
) -> Response {
    let Some(accounts) = service.accounts.as_ref() else {
        return no_accounts();
    };
    let next = staying_here(fields.get("next").map(|s| s.as_str()));
    match accounts.sign_in(field(&fields, "username"), field(&fields, "password"), chrono::Utc::now()) {
        Ok((_, token)) => redirect(next, Some(session_cookie(&headers, Some(&token)))),
        Err(AccountError::Refused(said)) => sign_in_form(StatusCode::UNAUTHORIZED, next, Some(&said)),
        Err(AccountError::Failed(said)) => {
            sign_in_form(StatusCode::INTERNAL_SERVER_ERROR, next, Some(&said))
        }
    }
}

async fn sign_out(State(service): State<Service>, headers: HeaderMap) -> Response {
    if let (Some(accounts), Some(token)) =
        (service.accounts.as_ref(), cookie_named(&headers, SESSION_COOKIE))
    {
        let _ = accounts.sign_out(&token, chrono::Utc::now());
    }
    redirect("/signin", Some(session_cookie(&headers, None)))
}

fn join_form(
    status: StatusCode,
    invite: &str,
    invited: &overseer::accounts::Invited,
    said: Option<&str>,
) -> Response {
    let (title, who) = match (&invited.username, invited.resets) {
        (Some(username), true) => (
            "A new password",
            format!("<p>For <span class=\"fixed\">{}</span></p>", escaped_for_html(username)),
        ),
        (Some(username), false) => (
            "Join",
            format!(
                "<p>As <span class=\"fixed\">{}</span></p>\
<label for=\"name\">Display name</label><input id=\"name\" name=\"name\" required>",
                escaped_for_html(username)
            ),
        ),
        (None, _) => (
            "Join",
            "<label for=\"username\">Username</label>\
<input id=\"username\" name=\"username\" autocomplete=\"username\" autocapitalize=\"none\" \
pattern=\"[a-z0-9_\\-]+\" title=\"lower case letters, digits, - and _\" required autofocus>\
<label for=\"name\">Display name</label><input id=\"name\" name=\"name\" required>"
                .to_string(),
        ),
    };
    form_page(
        status,
        title,
        said,
        &format!(
            "<form method=\"post\" action=\"/join\">\
<input type=\"hidden\" name=\"invite\" value=\"{invite}\">{who}\
<label for=\"password\">Password</label>\
<input id=\"password\" name=\"password\" type=\"password\" autocomplete=\"new-password\" minlength=\"{shortest}\" required>\
<button>{title}</button></form>",
            invite = escaped_for_html(invite),
            who = who,
            shortest = overseer::accounts::SHORTEST_PASSWORD,
            title = if invited.resets { "Set the password" } else { "Join" },
        ),
    )
}

fn invite_gone() -> Response {
    form_page(
        StatusCode::GONE,
        "This invite cannot be used",
        Some("It has been used already, or it has expired. Ask for a new one."),
        "",
    )
}

async fn join_page(
    State(service): State<Service>,
    axum::extract::Query(params): axum::extract::Query<Fields>,
) -> Response {
    let Some(accounts) = service.accounts.as_ref() else {
        return no_accounts();
    };
    let invite = field(&params, "invite");
    match accounts.invited(invite, chrono::Utc::now()) {
        Some(invited) => join_form(StatusCode::OK, invite, &invited, None),
        None => invite_gone(),
    }
}

async fn join_with_invite(
    State(service): State<Service>,
    headers: HeaderMap,
    axum::extract::Form(fields): axum::extract::Form<Fields>,
) -> Response {
    let Some(accounts) = service.accounts.as_ref() else {
        return no_accounts();
    };
    let invite = field(&fields, "invite");
    let now = chrono::Utc::now();
    let Some(invited) = accounts.invited(invite, now) else {
        return invite_gone();
    };
    match accounts.join(
        invite,
        field(&fields, "username"),
        field(&fields, "name"),
        field(&fields, "password"),
        now,
    ) {
        Ok((_, token)) => redirect("/", Some(session_cookie(&headers, Some(&token)))),
        Err(AccountError::Refused(said)) => {
            join_form(StatusCode::BAD_REQUEST, invite, &invited, Some(&said))
        }
        Err(AccountError::Failed(said)) => {
            join_form(StatusCode::INTERNAL_SERVER_ERROR, invite, &invited, Some(&said))
        }
    }
}

fn account_refusal(error: AccountError) -> (StatusCode, Json<serde_json::Value>) {
    match error {
        AccountError::Refused(said) => (StatusCode::BAD_REQUEST, Json(json!({ "error": said }))),
        AccountError::Failed(said) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": said })))
        }
    }
}

fn signed_in(caller: &Caller) -> Result<&User, (StatusCode, Json<serde_json::Value>)> {
    match caller {
        Caller::Person(user) => Ok(user),
        Caller::Service => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "this is the server's token, not an account" })),
        )),
        Caller::Stranger | Caller::Visitor => Err(sign_in_first()),
    }
}

/// Who is signed in.
async fn me(Extension(caller): Extension<Caller>) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    Ok(Json(json!(signed_in(&caller)?)))
}

#[derive(serde::Deserialize)]
struct OwnChange {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    password: Option<String>,
    /// The password now, which a new one needs.
    #[serde(default)]
    current: Option<String>,
}

/// Change one's own display name or password.
async fn change_me(
    State(service): State<Service>,
    Extension(caller): Extension<Caller>,
    Json(body): Json<OwnChange>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let accounts = service.accounts.as_ref().ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({ "error": "this server keeps no accounts" })))
    })?;
    let user = signed_in(&caller)?;
    let password = body
        .password
        .as_deref()
        .map(|new| (body.current.as_deref().unwrap_or(""), new));
    let changed = accounts
        .change_own(&user.username, body.name.as_deref(), password, chrono::Utc::now())
        .map_err(account_refusal)?;
    Ok(Json(json!(changed)))
}

fn keeps_accounts(service: &Service) -> Result<&Accounts, (StatusCode, Json<serde_json::Value>)> {
    service.accounts.as_deref().ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({ "error": "this server keeps no accounts" })))
    })
}

/// One's own keys, by name, when each was made and last used.
async fn keys(
    State(service): State<Service>,
    Extension(caller): Extension<Caller>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let accounts = keeps_accounts(&service)?;
    let user = signed_in(&caller)?;
    Ok(Json(json!({ "keys": accounts.keys(&user.username) })))
}

#[derive(serde::Deserialize)]
struct KeyAsked {
    name: String,
}

/// Make a key for a program to act as oneself with, and answer it - the only time it is shown.
async fn make_key(
    State(service): State<Service>,
    Extension(caller): Extension<Caller>,
    Json(body): Json<KeyAsked>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let accounts = keeps_accounts(&service)?;
    let user = signed_in(&caller)?;
    let key = accounts
        .make_key(&user.username, &body.name, chrono::Utc::now())
        .map_err(account_refusal)?;
    Ok(Json(json!({ "name": body.name.trim(), "key": key })))
}

/// Revoke one of one's own keys.
async fn revoke_key(
    State(service): State<Service>,
    Extension(caller): Extension<Caller>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let accounts = keeps_accounts(&service)?;
    let user = signed_in(&caller)?;
    if !accounts
        .revoke_key(&user.username, &name, chrono::Utc::now())
        .map_err(account_refusal)?
    {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "there is no key of that name" })),
        ));
    }
    Ok(Json(json!({ "revoked": name })))
}

/// Make one's own `sharing.os`, unless there is one, and answer its full name.
async fn start_sharing(
    State(service): State<Service>,
    Extension(caller): Extension<Caller>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let user = signed_in(&caller)?;
    let folder = service.documents.path().join(&user.username);
    overseer::access::start_sharing(&folder)
        .map_err(|e| respond(RequestError::Failed(format!("could not make the sharing document: {:?}", e.kind()))))?;
    Ok(Json(json!({
        "document": format!("{}/{}", user.username, overseer::access::SHARING),
    })))
}

#[derive(serde::Deserialize)]
struct InviteAsked {
    /// The account to send a new password to; absent, an invite for someone new.
    #[serde(default)]
    reset: Option<String>,
}

/// Make an invite, or a link that sets a new password. Only an admin, or the token, may.
async fn invite(
    State(service): State<Service>,
    Extension(caller): Extension<Caller>,
    Json(body): Json<InviteAsked>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let accounts = service.accounts.as_ref().ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({ "error": "this server keeps no accounts" })))
    })?;
    match &caller {
        Caller::Person(user) if !user.admin => {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "only an admin may invite" })),
            ));
        }
        Caller::Stranger => return Err(sign_in_first()),
        _ => {}
    }
    if let Some(username) = body.reset.as_deref() {
        if accounts.user(&username.trim().to_lowercase()).is_none() {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "there is no account of that name" })),
            ));
        }
    }
    let now = chrono::Utc::now();
    let token = accounts
        .invite(body.reset.as_deref(), false, now)
        .map_err(account_refusal)?;
    Ok(Json(json!({
        "link": format!("/join?invite={}", token),
        "expires": (now + overseer::accounts::INVITE_LASTS).to_rfc3339(),
    })))
}

/// Which node an operation is about.
///
/// Both a document name and an address contain slashes, so neither belongs in the path -
/// query parameters escape them properly and leave the address readable in a log.
#[derive(serde::Deserialize)]
struct Target {
    document: String,
    address: String,
}

#[derive(serde::Deserialize)]
struct WriteBody {
    /// Fields to set on the new entry, for an append.
    #[serde(default)]
    fields: std::collections::HashMap<String, overseer::types::OverseerValue>,
    /// The value, for a set.
    #[serde(default)]
    value: Option<overseer::types::OverseerValue>,
}

/// Read the subtree at an address: a catalogue, a day, one food's portion weight.
async fn read_at(
    State(service): State<Service>,
    Extension(caller): Extension<Caller>,
    axum::extract::Query(target): axum::extract::Query<Target>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let documents = service.documents.clone();
    let view = tokio::task::spawn_blocking(move || {
        documents.as_asker(caller.asker(), || documents.read_at(&target.document, &target.address))
    })
        .await
        .map_err(|e| respond(RequestError::Failed(format!("reading panicked: {}", e))))?
        .map_err(respond)?;
    // The view itself, not wrapped again: it already carries the node, its address and the
    // addresses of its children, and a caller reaching through two "node" keys to find the
    // first one is a sign the shape is wrong.
    Ok(Json(json!(view)))
}

/// A document's text as it is written - see `DocumentRoot::source`.
async fn read_source(
    State(service): State<Service>,
    Extension(caller): Extension<Caller>,
    axum::extract::Query(which): axum::extract::Query<Which>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let documents = service.documents.clone();
    let source = tokio::task::spawn_blocking(move || {
        documents.as_asker(caller.asker(), || documents.source(&which.document))
    })
        .await
        .map_err(|e| respond(RequestError::Failed(format!("reading panicked: {}", e))))?
        .map_err(respond)?;
    Ok(Json(json!(source)))
}

/// Append an entry to the list at an address.
async fn append_at(
    State(service): State<Service>,
    Extension(caller): Extension<Caller>,
    axum::extract::Query(target): axum::extract::Query<Target>,
    Json(body): Json<WriteBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let documents = service.documents.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        documents.as_asker(caller.asker(), || {
            documents.append_at(&target.document, &target.address, &body.fields)
        })
    })
        .await
        .map_err(|e| respond(RequestError::Failed(format!("writing panicked: {}", e))))?
        .map_err(respond)?;
    Ok(Json(json!(outcome)))
}

/// Set the value at an address.
async fn set_at(
    State(service): State<Service>,
    Extension(caller): Extension<Caller>,
    axum::extract::Query(target): axum::extract::Query<Target>,
    Json(body): Json<WriteBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let Some(value) = body.value else {
        return Err(respond(RequestError::Rejected("'value' is required".into())));
    };
    let documents = service.documents.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        documents.as_asker(caller.asker(), || documents.set_at(&target.document, &target.address, value))
    })
        .await
        .map_err(|e| respond(RequestError::Failed(format!("writing panicked: {}", e))))?
        .map_err(respond)?;
    Ok(Json(json!(outcome)))
}


/// Remove the entry at an address.
async fn remove_at(
    State(service): State<Service>,
    Extension(caller): Extension<Caller>,
    axum::extract::Query(target): axum::extract::Query<Target>,
    // Optional, and optional on purpose: a list with a key addresses its entries by that key
    // and cannot go stale, so insisting would be noise. Sent, it is checked - see `remove_at`.
    body: Option<Json<WriteBody>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let documents = service.documents.clone();
    let expect = body.map(|Json(b)| b.fields).unwrap_or_default();
    let outcome = tokio::task::spawn_blocking(move || {
        documents.as_asker(caller.asker(), || {
            documents.remove_at(&target.document, &target.address, &expect)
        })
    })
        .await
        .map_err(|e| respond(RequestError::Failed(format!("writing panicked: {}", e))))?
        .map_err(respond)?;
    Ok(Json(json!(outcome)))
}


#[derive(serde::Deserialize)]
struct EventBody {
    event: String,
}

/// Press something the document declares - a button's `on click`, say.
async fn run_event(
    State(service): State<Service>,
    Extension(caller): Extension<Caller>,
    axum::extract::Query(target): axum::extract::Query<Target>,
    Json(body): Json<EventBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let documents = service.documents.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        documents.as_asker(caller.asker(), || {
            documents.run_event(&target.document, &target.address, &body.event)
        })
    })
    .await
    .map_err(|e| respond(RequestError::Failed(format!("the event panicked: {}", e))))?
    .map_err(respond)?;
    Ok(Json(json!(outcome)))
}

/// Take back the last write to a document.
async fn undo(
    State(service): State<Service>,
    Extension(caller): Extension<Caller>,
    axum::extract::Query(which): axum::extract::Query<Which>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let documents = service.documents.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        documents.as_asker(caller.asker(), || documents.undo(&which.document))
    })
        .await
        .map_err(|e| respond(RequestError::Failed(format!("the undo panicked: {}", e))))?
        .map_err(respond)?;
    Ok(Json(outcome))
}

#[derive(serde::Deserialize)]
struct Which {
    document: String,
}

/// Hear each time a document's file changes, as its version - see `overseer::watch`.
///
/// One `changed` event with the version it has now, then one per version after. Only versions:
/// what a page shows depends on who is looking, so each asks for its own catch-up. Reading is
/// enough to listen, since a document shared only to be read still moves.
async fn watch(
    State(service): State<Service>,
    Extension(caller): Extension<Caller>,
    axum::extract::Query(which): axum::extract::Query<Which>,
) -> Result<
    axum::response::sse::Sse<
        impl futures_util::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>,
    >,
    (StatusCode, Json<serde_json::Value>),
> {
    use axum::response::sse::{Event, KeepAlive, Sse};
    use futures_util::StreamExt;
    use tokio::sync::broadcast::error::RecvError;
    let documents = service.documents.clone();
    let (now, listening) = tokio::task::spawn_blocking(move || {
        documents.as_asker(caller.asker(), || {
            documents.resolve(&which.document).map(|path| overseer::watch::listen(&path))
        })
    })
    .await
    .map_err(|e| respond(RequestError::Failed(format!("watching panicked: {}", e))))?
    .map_err(respond)?;
    // What the document's mounts read goes as an event of its own, ahead of the version, so a
    // page that knows only `changed` goes on reading the version alone - see `watch::Heard`.
    let said = |heard: overseer::watch::Heard| {
        let mut events = Vec::new();
        if !heard.read_from.is_empty() {
            events.push(Ok(Event::default().event("read").data(heard.read_from)));
        }
        events.push(Ok(Event::default().event("changed").data(heard.version)));
        events
    };
    let stream = futures_util::stream::unfold(
        (Some(now), listening),
        move |(first, mut listening)| async move {
            if let Some(heard) = first {
                return Some((said(heard), (None, listening)));
            }
            loop {
                match listening.recv().await {
                    Ok(heard) => return Some((said(heard), (None, listening))),
                    // Versions missed only mean the next one is newer still.
                    Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => return None,
                }
            }
        },
    )
    .flat_map(futures_util::stream::iter);
    // Kept alive, or a proxy in front of the server closes a quiet stream.
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// Report what went wrong without describing the filesystem to whoever asked.
///
/// A press the document refused says so in `refused` as well as `error`, so a caller can tell
/// the document's own sentence, meant for whoever pressed, from something that went wrong.
fn respond(error: RequestError) -> (StatusCode, Json<serde_json::Value>) {
    let status = match error {
        RequestError::Rejected(_) => StatusCode::BAD_REQUEST,
        RequestError::NotFound(_) => StatusCode::NOT_FOUND,
        RequestError::Failed(_) => StatusCode::INTERNAL_SERVER_ERROR,
        RequestError::Refused(_) => StatusCode::UNPROCESSABLE_ENTITY,
        RequestError::SignIn => StatusCode::UNAUTHORIZED,
        RequestError::ReadOnly(_) => StatusCode::FORBIDDEN,
    };
    match error {
        // A press on a document only to be read is refused in a sentence for whoever pressed
        // too: without it the page takes the long way, runs the press itself, and its save is
        // refused instead, which it can only answer with an error in place of the document.
        RequestError::Refused(said) | RequestError::ReadOnly(said) => {
            (status, Json(json!({ "error": said, "refused": said })))
        }
        other => (status, Json(json!({ "error": other.to_string() }))),
    }
}

/// What whoever asks can reach: `documents`, every one by its full name, and the same grouped
/// into `mine`, `shared` (by owner) and `public` - see `DocumentRoot::listing`.
async fn list(
    State(service): State<Service>,
    Extension(caller): Extension<Caller>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if matches!(caller, Caller::Stranger) {
        return Err(sign_in_first());
    }
    let documents = service.documents.clone();
    let listing = tokio::task::spawn_blocking(move || {
        documents.as_asker(caller.asker(), || documents.listing())
    })
    .await
    .map_err(|e| respond(RequestError::Failed(format!("listing panicked: {}", e))))?;
    Ok(Json(json!(listing)))
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
    Extension(caller): Extension<Caller>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // Resolving is CPU work rather than IO, and long enough on a large document to block the
    // runtime thread it lands on, so it runs where blocking is expected.
    let documents = service.documents.clone();
    let resolved = tokio::task::spawn_blocking(move || {
        documents.as_asker(caller.asker(), || documents.open(&name))
    })
        .await
        .map_err(|e| respond(RequestError::Failed(format!("resolving panicked: {}", e))))?
        .map_err(respond)?;
    // As the page has it, without what only writing the document back needs - see `ForThePage`.
    Ok(Json(json!({ "nodes": overseer::types::ForThePage(resolved) })))
}

/// Hand back to the operating system the memory a request stopped needing.
///
/// Freeing memory inside a process does not return it: the allocator keeps it for the next
/// request, which is usually right and is wrong here. Resolving one large document peaks at
/// around a hundred and sixty megabytes, and the next request may be half an hour away - so
/// without this the service sits on the high-water mark of everything it has ever done, and a
/// host that bills by memory held bills for all of it.
///
/// glibc only, which is what the container runs; everywhere else this is nothing. Declared
/// directly rather than through a crate, since it is one symbol and adding a dependency to the
/// server for it would be the larger change.
///
/// Rate-limited because it walks the arenas: worth doing after a resolve that took forty
/// seconds, not after each of a hundred small reads while someone scrolls a page.
#[cfg(target_env = "gnu")]
fn release_free_memory() {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    extern "C" {
        fn malloc_trim(pad: usize) -> i32;
    }

    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    const NOT_MORE_OFTEN_THAN: Duration = Duration::from_secs(2);

    if let Ok(mut last) = LAST.lock() {
        let now = Instant::now();
        if last.map(|then| now.duration_since(then) < NOT_MORE_OFTEN_THAN) == Some(true) {
            return;
        }
        *last = Some(now);
    } else {
        return;
    }

    // Safety: no arguments to get wrong, no pointers involved, and it is safe to call at any
    // time from any thread - it takes the allocator's own locks.
    unsafe {
        malloc_trim(0);
    }
}

#[cfg(not(target_env = "gnu"))]
fn release_free_memory() {}

/// Runs after every response, so nothing has to remember to ask.
async fn give_back_memory(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let response = next.run(request).await;
    release_free_memory();
    response
}

fn main() {
    // Tokio's blocking pool is five hundred threads by default, and every document operation
    // runs on it. That is the wrong shape for this service twice over.
    //
    // The work is serialised behind the documents anyway - one bot, one reader - so the extra
    // threads buy no throughput. And under glibc each thread that allocates is given a heap
    // arena of its own, which is never handed back: a resolve peaking at a hundred and sixty
    // megabytes on each of a dozen different threads leaves a dozen high-water marks, and the
    // service sits on their sum. That is what turned a two-hundred-megabyte peak into two
    // gigabytes held, and why it arrived in steps rather than climbing.
    //
    // Two rather than one so that a slow resolve cannot block a quick read behind it.
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(2)
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("cannot start: {}", e);
            std::process::exit(1);
        }
    };
    runtime.block_on(serve());
}

async fn serve() {
    let mut root: Option<String> = None;
    let mut frontend_dir: Option<String> = None;
    let mut port: Option<String> = None;
    let mut bind: Option<String> = None;
    let mut open = false;
    let mut accounts_file: Option<String> = None;
    let mut admin: Option<String> = None;
    let mut examples_dir: Option<String> = None;
    // Read from the environment or a file rather than an argument: a command line ends up in
    // shell history and in the process list, where a secret has no business being.
    let mut token = std::env::var("OVERSEER_TOKEN").ok().filter(|t| !t.is_empty());
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => root = args.next(),
            "--bind" => bind = args.next(),
            "--port" => port = args.next(),
            "--frontend" => frontend_dir = args.next(),
            "--open" => open = true,
            "--accounts" => accounts_file = args.next(),
            "--admin" => admin = args.next(),
            "--examples" => examples_dir = args.next(),
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
            other => {
                eprintln!("unknown argument '{}'", other);
                eprintln!(
                    "usage: overseer-server [--root DIR] [--frontend DIR] [--bind ADDR] [--port N] [--token-file PATH] [--accounts PATH] [--admin NAME] [--examples DIR] [--open]"
                );
                std::process::exit(2);
            }
        }
    }

    // A flag wins over the environment, so a script that passes one is not overridden by
    // whatever its host happens to set. `PORT` comes last: it is what a host such as Railway
    // hands every container, and a port asked for by name is the more deliberate of the two.
    fn env(name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
    fn setting(
        flag: Option<String>,
        what: &'static str,
        names: &[&'static str],
    ) -> Option<(&'static str, String)> {
        match flag {
            Some(value) => Some((what, value)),
            None => from_environment(names, env),
        }
    }
    let root = setting(root, "--root", &["OVERSEER_ROOT"])
        .map_or_else(|| "../examples".to_string(), |(_, v)| v);
    let root = std::path::PathBuf::from(root);
    let frontend_dir = setting(frontend_dir, "--frontend", &["OVERSEER_FRONTEND"])
        .map_or_else(|| "../dist".to_string(), |(_, v)| v);
    let frontend_dir = std::path::PathBuf::from(frontend_dir);
    let bind: std::net::IpAddr = match setting(bind, "--bind", &["OVERSEER_BIND"]) {
        None => std::net::IpAddr::from([127, 0, 0, 1]),
        Some((from, value)) => match value.parse() {
            Ok(a) => a,
            Err(_) => {
                eprintln!("{} takes an address, got '{}'", from, value);
                std::process::exit(2);
            }
        },
    };
    let port: u16 = match setting(port, "--port", &["OVERSEER_PORT", "PORT"]) {
        None => 4747,
        Some((from, value)) => match value.parse() {
            Ok(p) => p,
            Err(_) => {
                eprintln!("{} takes a number, got '{}'", from, value);
                std::process::exit(2);
            }
        },
    };
    if !open {
        open = from_environment(&["OVERSEER_OPEN"], env).is_some_and(|(_, v)| is_switched_on(&v));
    }

    let accounts = match setting(accounts_file, "--accounts", &["OVERSEER_ACCOUNTS"]) {
        None => None,
        // The documents name people by username, and draw their names from the directory.
        Some((from, path)) => match Accounts::open(&path).and_then(|a| a.with_directory(&root)) {
            Ok(accounts) => Some(Arc::new(accounts)),
            Err(e) => {
                eprintln!("cannot use the accounts that {} names: {}", from, e);
                std::process::exit(1);
            }
        },
    };
    let admin = setting(admin, "--admin", &["OVERSEER_ADMIN"]);

    let access = match token {
        Some(t) => Access::with_token(t),
        None => Access::unrestricted(),
    };
    // Accounts keep strangers out as a token does: nobody gets in without signing in.
    if let Some(reason) = refuse_to_start(&bind, &access, open || accounts.is_some()) {
        eprintln!("{}", reason);
        std::process::exit(2);
    }
    // Only open matters when it is what let the server start: with a token, or on loopback, it
    // changes nothing and a warning would only teach people to ignore warnings.
    let serving_open =
        open && !access.is_restricted() && accounts.is_none() && !is_loopback(&bind);

    match (&accounts, admin) {
        (Some(accounts), Some((from, username))) => {
            match accounts.bootstrap(&username, chrono::Utc::now()) {
                Ok(Some(invite)) => println!(
                    "there is no account '{}' yet; to make it, open /join?invite={} within 7 days",
                    username.trim().to_lowercase(),
                    invite
                ),
                Ok(None) => {}
                Err(e) => {
                    eprintln!("{} names no account that can be made: {}", from, e);
                    std::process::exit(2);
                }
            }
        }
        (None, Some((from, _))) => {
            eprintln!("{} needs accounts to be kept: set OVERSEER_ACCOUNTS or --accounts", from);
            std::process::exit(2);
        }
        _ => {}
    }

    let documents = match DocumentRoot::new(&root) {
        // With accounts, each person's documents are in a folder named for them.
        Ok(r) => Arc::new(match accounts.clone() {
            Some(accounts) => r.with_accounts(move |name| accounts.user(name).is_some()),
            None => r,
        }),
        Err(e) => {
            eprintln!("cannot serve '{}': {}", root.display(), e);
            std::process::exit(1);
        }
    };
    println!("serving {} documents from {}", documents.list().len(), documents.path().display());

    // Unset is off: nothing is served without a token or sign-in unless this names a folder.
    let examples = match setting(examples_dir, "--examples", &["OVERSEER_EXAMPLES"]) {
        None => None,
        Some((from, dir)) => match DocumentRoot::new(&dir) {
            Ok(r) => {
                println!(
                    "serving {} examples from {} to anyone, at /examples/",
                    r.as_asker(Asker::Visitor, || r.list().len()),
                    r.path().display()
                );
                Some(Arc::new(r))
            }
            Err(e) => {
                eprintln!("cannot serve the examples {} names: {}", from, e);
                std::process::exit(1);
            }
        },
    };

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
        accounts: accounts.clone(),
    };

    let mut app = Router::new()
        .route("/documents", get(list))
        // A name may contain directories, so it is matched to the end of the path.
        .route("/doc/*name", get(document_or_page))
        // A pasted image travels as base64 inside the JSON, a third larger than the image, and
        // axum refuses bodies over 2 MB by default; this lets one as large as is kept through.
        .route(
            "/api/:cmd",
            post(command).layer(axum::extract::DefaultBodyLimit::max(API_MOST_BYTES)),
        )
        // The document API: a subtree is named by an address that survives a resolve.
        .route("/v1/node", get(read_at).post(set_at))
        .route("/v1/append", post(append_at))
        .route("/v1/remove", post(remove_at))
        .route("/v1/event", post(run_event))
        .route("/v1/undo", post(undo))
        .route("/v1/watch", get(watch))
        .route("/v1/source", get(read_source))
        .route("/account/me", get(me).post(change_me))
        .route("/account/invite", post(invite))
        .route("/account/sharing", post(start_sharing))
        .route("/account/keys", get(keys).post(make_key))
        .route("/account/keys/:name", axum::routing::delete(revoke_key))
        .route("/", get(index));
    if examples.is_none() {
        app = app.route("/__overseer/bridge.js", get(bridge));
    }
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
    // Outside the access check, so a refused request gives its memory back too.
    let app = app.layer(axum::middleware::from_fn(give_back_memory));
    // Presenting the token is how a browser gets in, so this sits outside that check.
    //
    // And everything compressed on the way out, for whoever asks - a browser always does, and so
    // does the bot's client. An answer is the same few parameter names over and over: opening the
    // food tracker is 4.9 MB of JSON and 239 KB gzipped, which on a phone is the difference
    // between seconds of transfer and none worth mentioning. The default level: the fastest one
    // saves the server about 20 ms on the largest answer and sends 300 KB more for it.
    let app = Router::new()
        .route("/auth", get(sign_in))
        // The ways in for people, which have to be reachable before anyone has signed in.
        .route("/signin", get(sign_in_page).post(sign_in_with_password))
        .route("/signout", get(sign_out).post(sign_out))
        .route("/join", get(join_page).post(join_with_invite))
        .merge(app);
    // The examples, for anyone at all: outside the access check, and a root of their own, so no
    // name asked for there can reach anyone's documents - a person's own folder called
    // `examples` is still theirs, at /doc/examples/. Only what the page reads with is here;
    // nothing that writes but a press, which a visitor's asker refuses unless it moves only what
    // the document keeps for whoever is looking.
    //
    // The page has to load for someone with no token, so its own files and the bridge are
    // served to anyone too. They are the open-source build, and once the examples are public
    // there is nothing in them a stranger could learn from; the list of documents, /doc/ and
    // everything that reads or writes them stay behind the check.
    let app = match examples {
        None => app,
        Some(examples) => {
            let visiting = Service {
                documents: examples,
                frontend: frontend.clone(),
                access: access.clone(),
                accounts: None,
            };
            let examples_app = Router::new()
                .route(
                    "/api/:cmd",
                    post(command).layer(axum::extract::DefaultBodyLimit::max(API_MOST_BYTES)),
                )
                .route("/v1/node", get(read_at))
                .route("/v1/watch", get(watch))
                .route("/v1/source", get(read_source))
                .route("/*name", get(example_or_page))
                .layer(axum::middleware::from_fn(as_visitor))
                .with_state(visiting);
            let mut app = app
                .nest("/examples", examples_app)
                .route("/__overseer/bridge.js", get(bridge));
            if let Some(dir) = frontend.as_ref() {
                for part in ["assets", "icons"] {
                    app = app.nest_service(
                        &format!("/{}", part),
                        tower_http::services::ServeDir::new(dir.join(part)),
                    );
                }
            }
            app
        }
    };
    let app = app
        .with_state(state)
        .layer(tower_http::compression::CompressionLayer::new());

    let addr = SocketAddr::new(bind, port);
    if let Some(accounts) = accounts.as_ref() {
        println!(
            "people sign in at /signin, with the accounts kept in {}",
            accounts.path().display()
        );
    }
    if access.is_restricted() {
        println!("a token is required; a browser can present it once at /auth?token=...");
    } else if accounts.is_some() {
        println!("no token set, so only people signed in may read these documents");
    } else if serving_open {
        println!(
            "warning: serving open with no token - anyone who can reach {} may read and change these documents",
            addr
        );
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
