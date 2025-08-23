use crate::types::{OverseerNode, Result};
use std::path::{Path, PathBuf};

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct DocumentId {
    pub path: PathBuf,
    pub mtime: Option<u64>,
}

#[derive(Default)]
pub struct DocumentManager {}

#[allow(dead_code)]
impl DocumentManager {
    pub fn new() -> Self { Self {} }

    pub fn resolve_path(base: &Path, relative: &str) -> PathBuf {
        let p = Path::new(relative);
        if p.is_absolute() { p.to_path_buf() } else { base.join(p) }
    }

    // Stub: open a document and return parsed nodes (to be wired later)
    pub async fn open_document(_path: &Path) -> Result<Vec<OverseerNode>> {
        Err(crate::types::OverseerError::IoError("open_document not implemented".into()))
    }
}
