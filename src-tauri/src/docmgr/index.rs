use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileIndexEntry {
    pub path: PathBuf,
    pub mtime: Option<u64>,
    pub size: Option<u64>,
    pub stats: HashMap<String, serde_json::Value>,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IndexCache {
    pub entries: HashMap<String, FileIndexEntry>,
}

impl IndexCache {
    pub fn new() -> Self { Self { entries: HashMap::new() } }

    pub fn get(&self, key: &str) -> Option<&FileIndexEntry> { self.entries.get(key) }

    pub fn upsert(&mut self, key: String, entry: FileIndexEntry) { self.entries.insert(key, entry); }
}
