// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::command;

mod types;
mod parser;
mod evaluator;
mod file_ops;

use types::*;
use file_ops::FileOperations;
use parser::parse_document;

#[command]
async fn load_overseer_file(path: String) -> Result<String> {
    match FileOperations::read_file(&path).await {
        Ok(content) => Ok(content),
        Err(e) => Err(OverseerError::IoError(format!("Failed to read file: {}", e))),
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
        Ok((_, nodes)) => Ok(nodes),
        Err(e) => Err(OverseerError::ParseError(format!("Parse error: {}", e))),
    }
}

#[command]
async fn find_overseer_files(_directory: String) -> Result<Vec<String>> {
    // For now, return empty vector - this function needs to be implemented
    Ok(vec![])
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            load_overseer_file,
            save_overseer_file,
            parse_overseer_content,
            find_overseer_files
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
