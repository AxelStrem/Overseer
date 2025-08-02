// Prevents additional console window on Windows in release, DO NOT REMOVE!!
// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::command;

mod types;
mod parser;
mod evaluator;
mod file_ops;
pub mod resolver;

use types::*;
use file_ops::FileOperations;
use parser::parse_document;

#[command]
async fn load_overseer_file(path: String) -> Result<String> {
    match FileOperations::read_file(&path).await {
        Ok(content) => {
            Ok(content)
        },
        Err(e) => {
            Err(OverseerError::IoError(format!("Failed to read file: {}", e)))
        }
    }
}

#[command]
async fn save_overseer_file(path: String, content: String) -> Result<()> {
    match FileOperations::write_file(&path, &content).await {
        Ok(_) => Ok(()),
        Err(e) => Err(OverseerError::IoError(format!("Failed to save file: {}", e))),
    }
}

#[command]
async fn parse_overseer_content(content: String) -> Result<Vec<OverseerNode>> {
    match parse_document(&content) {
        Ok((remaining, mut nodes)) => {
            resolver::resolve_document(&mut nodes);
            Ok(nodes)
        },
        Err(e) => {
            Err(OverseerError::ParseError(format!("Parse error: {}", e)))
        }
    }
}

#[command]
async fn find_overseer_files(_directory: String) -> Result<Vec<String>> {
    // For now, return empty vector - this function needs to be implemented
    Ok(vec![])
}

fn main() {
    // Set WebView2 fixed version path with debug cache folder
    let debug_cache_folder = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("webview2_debug");
    
    std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", &debug_cache_folder);
    
    std::env::set_var("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", 
        std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("webview2")
            .join("Microsoft.WebView2.FixedVersionRuntime.138.0.3351.95.x64"));

    match tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            load_overseer_file,
            save_overseer_file,
            parse_overseer_content,
            find_overseer_files
        ])
        .run(tauri::generate_context!()) {
        Ok(_) => {},
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
