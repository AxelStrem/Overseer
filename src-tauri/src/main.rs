// Prevents additional console window on Windows in release, DO NOT REMOVE!!
// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::command;

mod actions;
mod addressing;
mod delta;
mod app_api;
mod dependencies;
mod document_cache;
mod docmgr;
mod file_ops;
mod formula_evaluator;
mod parser;
pub mod resolver;
mod source_registry;
mod types;

use actions::ActionExecutor;
use file_ops::FileOperations;
use types::*;

#[command]
async fn load_overseer_file(path: String) -> Result<String> {
    // Record the document's directory so a mount's relative source resolves against the
    // document that declares it.
    // The app has one document open, so recording it here is enough; anything serving several
    // at once names the document per operation instead, through
    // DocumentManager::with_document, which takes precedence over this.
    //
    // Changing the process working directory used to happen here as well. Resolution has not
    // depended on it for some time, and it is exactly the kind of process-wide state that
    // makes a second open document behave unpredictably, so it no longer does.
    docmgr::manager::DocumentManager::set_current_document(Some(&path));
    match FileOperations::read_file(&path).await {
        Ok(content) => Ok(content),
        Err(e) => Err(OverseerError::IoError(format!(
            "Failed to read file: {}",
            e
        ))),
    }
}

// Save a regenerated document provided by the caller. We canonicalize via the serializer so
// snapshot-driven trivia (comments/whitespace) is reapplied without needing the original text.
#[command]
async fn save_overseer_file_with_original(
    path: String,
    regenerated: String,
    _original: String,
) -> Result<()> {
    let regenerated_canonical = app_api::canonicalize_document(&regenerated);
    match FileOperations::write_file(&path, &regenerated_canonical).await {
        Ok(_) => Ok(()),
        Err(e) => Err(OverseerError::IoError(format!(
            "Failed to save file: {}",
            e
        ))),
    }
}

#[command]
async fn save_overseer_file(path: String, content: String) -> Result<()> {
    let regenerated = app_api::canonicalize_document(&content);
    match FileOperations::write_file(&path, &regenerated).await {
        Ok(_) => Ok(()),
        Err(e) => Err(OverseerError::IoError(format!(
            "Failed to save file: {}",
            e
        ))),
    }
}


/// Save a document the caller holds as text, restoring guarded fields to what was authored.
#[command]
async fn save_overseer_file_from_text(
    path: String,
    content: String,
    guarded: Option<Vec<app_api::GuardedRevert>>,
) -> Result<()> {
    let text = app_api::save_document_from_text(content, guarded.unwrap_or_default())?;
    FileOperations::write_file(&path, &text)
        .await
        .map_err(|e| OverseerError::IoError(format!("Failed to save file: {}", e)))
}

#[command]
async fn serialize_overseer_nodes(nodes: Vec<OverseerNode>) -> Result<String> {
    match file_ops::OverseerFileHandler::serialize_nodes(&nodes) {
        Ok(content) => Ok(content),
        Err(e) => Err(OverseerError::SerializationError(format!(
            "Failed to serialize nodes: {}",
            e
        ))),
    }
}

/// `serialize_overseer_nodes` with the document delivered as raw bytes.
///
/// Passing the document as a normal command argument makes Tauri hand it to the webview's
/// JSON IPC, which on a large document costs seconds - far more than the serialization it
/// asks for, and more than receiving the same document back, which travels a different
/// route. Taking the bytes directly avoids that path. The caller sends UTF-8 JSON; a JSON
/// body is still accepted so an older caller keeps working.
#[command]
fn serialize_overseer_nodes_raw(request: tauri::ipc::Request<'_>) -> Result<String> {
    let nodes: Vec<OverseerNode> = match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) => serde_json::from_slice(bytes)
            .map_err(|e| OverseerError::SerializationError(format!("Invalid node bytes: {}", e)))?,
        tauri::ipc::InvokeBody::Json(value) => serde_json::from_value(value.clone())
            .map_err(|e| OverseerError::SerializationError(format!("Invalid node json: {}", e)))?,
    };
    file_ops::OverseerFileHandler::serialize_nodes(&nodes).map_err(|e| {
        OverseerError::SerializationError(format!("Failed to serialize nodes: {}", e))
    })
}

#[command]
async fn parse_overseer_content(content: String) -> Result<Vec<OverseerNode>> {
    // The same call the server makes. This used to record where the server did not, because the
    // recording cost seventy per cent of an open and only an editor got that back; it now costs
    // almost nothing and both want the graph. See `app_api::recording_is_on`.
    app_api::load_document(content)
}

#[command]
async fn parse_overseer_content_selective(
    content: String,
    changed_fields: Vec<String>,
    changed_field_values: Option<std::collections::HashMap<String, OverseerValue>>,
) -> Result<Vec<OverseerNode>> {
    app_api::resolve_selective(content, changed_fields, changed_field_values)
}


/// `parse_overseer_content_selective`, also returning the serialized document.
///
/// The caller keeps that text and sends it back for the next edit, instead of uploading the
/// document each time.
#[command]
async fn parse_overseer_content_selective_with_text(
    content: String,
    changed_fields: Vec<String>,
    changed_field_values: Option<std::collections::HashMap<String, OverseerValue>>,
) -> Result<app_api::ResolvedDocument> {
    app_api::resolve_selective_with_text(content, changed_fields, changed_field_values)
}


/// Resolve an edit and answer with what changed, when that is possible.
///
/// The answer is the whole document today: ~14 MB for a large one, over an IPC that manages a
/// couple of MB per second, and then rebuilt element by element. A field edit changes a couple
/// of nodes, and this says so instead.
#[command]
async fn parse_overseer_content_selective_update(
    content: String,
    changed_fields: Vec<String>,
    changed_field_values: Option<std::collections::HashMap<String, OverseerValue>>,
) -> Result<app_api::ResolvedUpdate> {
    app_api::resolve_selective_update(content, changed_fields, changed_field_values)
}

#[command]
async fn find_overseer_files(_directory: String) -> Result<Vec<String>> {
    // For now, return empty vector - this function needs to be implemented
    Ok(vec![])
}

#[command]
async fn execute_overseer_event(
    mut nodes: Vec<OverseerNode>,
    node_path: Vec<String>,
    event_name: String,
) -> Result<Vec<OverseerNode>> {
    #[cfg(feature = "debug-resolver")]
    eprintln!(
        "[TAURI] execute_overseer_event event='{}' path={:?}",
        event_name, node_path
    );
    // Execute actions for the event; this will mutate nodes and re-resolve once
    ActionExecutor::execute_event(&mut nodes, &node_path, &event_name)?;
    Ok(nodes)
}


/// `execute_overseer_event` driven by the document's text rather than the document.
#[command]
async fn execute_overseer_event_with_text(
    content: String,
    node_path: Vec<String>,
    event_name: String,
) -> Result<app_api::ResolvedDocument> {
    app_api::execute_event_on_text(content, node_path, event_name)
}

/// `scheduler_tick` driven by the document's text.

/// `execute_overseer_event_with_text`, answering with what changed where possible.
#[command]
async fn execute_overseer_event_update(
    content: String,
    node_path: Vec<String>,
    event_name: String,
) -> Result<app_api::ResolvedUpdate> {
    app_api::execute_event_update(content, node_path, event_name)
}

#[command]
async fn scheduler_tick_with_text(content: String) -> Result<app_api::ResolvedDocument> {
    app_api::tick_on_text(content)
}

/// `get_next_timer_due_ms` driven by the document's text.
#[command]
async fn get_next_timer_due_ms_from_text(content: String) -> Result<Option<i64>> {
    app_api::next_due_ms_on_text(content)
}

#[command]
async fn scheduler_tick(mut nodes: Vec<OverseerNode>) -> Result<Vec<OverseerNode>> {
    // Run a timer sweep; this may mutate the document and re-resolve inside
    match ActionExecutor::tick(&mut nodes) {
        Ok(()) => Ok(nodes),
        Err(e) => {
            eprintln!("[TAURI] scheduler_tick error: {}", e);
            Err(e)
        }
    }
}

#[command]
async fn get_next_timer_due_ms(nodes: Vec<OverseerNode>) -> Result<Option<i64>> {
    Ok(ActionExecutor::next_due_ms(&nodes))
}

fn main() {
    // Install a panic hook to surface detailed errors in the terminal during development
    std::panic::set_hook(Box::new(|info| {
        eprintln!("\n================= RUST PANIC =================");
        eprintln!("{}", info);
        if let Ok(bt) = std::env::var("RUST_BACKTRACE") {
            if bt != "0" {
                eprintln!("Backtrace enabled (set RUST_BACKTRACE=1)");
            }
        }
        eprintln!("============================================\n");
    }));
    // Set WebView2 fixed version path with debug cache folder
    let debug_cache_folder = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("webview2_debug");

    std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", &debug_cache_folder);

    // Point at a bundled fixed-version WebView2 runtime, but only if one is actually there.
    //
    // Setting this unconditionally pins the app to a path that exists for exactly one build
    // profile: the runtime sits beside the executable, so a `target/debug` build finds it and
    // a `target/release` build does not, failing with "could not find WebView2 runtime" on a
    // machine that has WebView2 installed - because naming a folder here means the installed
    // one is never consulted. The runtime is 565 MB, so copying it per profile is not the
    // answer; finding it where it already lives is.
    const RUNTIME_DIR: &str = "Microsoft.WebView2.FixedVersionRuntime.138.0.3351.95.x64";
    let bundled_runtime = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("webview2").join(RUNTIME_DIR)))
        .filter(|path| path.exists())
        .or_else(|| {
            // Anything built by cargo can use the copy kept in the source tree, whichever
            // profile it was built with.
            let in_source_tree = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("webview2")
                .join(RUNTIME_DIR);
            in_source_tree.exists().then_some(in_source_tree)
        });

    match bundled_runtime {
        Some(path) => std::env::set_var("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", path),
        // Nothing bundled: leave the variable alone so the system-installed runtime is used.
        None => std::env::remove_var("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER"),
    }

    match tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            load_overseer_file,
            save_overseer_file,
            save_overseer_file_from_text,
            save_overseer_file_with_original,
            serialize_overseer_nodes,
            serialize_overseer_nodes_raw,
            parse_overseer_content,
            parse_overseer_content_selective,
            parse_overseer_content_selective_with_text,
            parse_overseer_content_selective_update,
            find_overseer_files,
            execute_overseer_event,
            execute_overseer_event_with_text,
            execute_overseer_event_update,
            scheduler_tick_with_text,
            get_next_timer_due_ms_from_text,
            scheduler_tick,
            get_next_timer_due_ms
        ])
        .run(tauri::generate_context!())
    {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Failed to start Overseer application: {}", e);
            eprintln!("This might be due to WebView2 runtime issues.");
            eprintln!("Please try:");
            eprintln!("1. Installing the latest WebView2 runtime from Microsoft");
            eprintln!("2. Running as administrator");
            eprintln!("3. Temporarily disabling antivirus/Windows Defender");
            eprintln!("4. Running from the development environment with `npm run tauri dev`");
            std::process::exit(1);
        }
    }
}
