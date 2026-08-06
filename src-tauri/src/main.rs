// Prevents additional console window on Windows in release, DO NOT REMOVE!!
// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::command;

mod actions;
mod app_api;
mod dependency_tracker;
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
    docmgr::manager::DocumentManager::set_current_document(Some(&path));
    // Also set the process working directory, as earlier versions relied on. This is kept
    // for compatibility only - resolution no longer depends on it, and it should go once
    // multiple open documents make a single process-wide directory meaningless.
    if let Ok(p) = std::path::PathBuf::from(&path).canonicalize() {
        if let Some(parent) = p.parent() {
            let _ = std::env::set_current_dir(parent);
        }
    }
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

#[command]
async fn parse_overseer_content(content: String) -> Result<Vec<OverseerNode>> {
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
            save_overseer_file_with_original,
            serialize_overseer_nodes,
            parse_overseer_content,
            parse_overseer_content_selective,
            find_overseer_files,
            execute_overseer_event,
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
