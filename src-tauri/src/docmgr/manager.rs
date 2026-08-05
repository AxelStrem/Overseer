use crate::types::{OverseerNode, Result};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct DocumentId {
    pub path: PathBuf,
    pub mtime: Option<u64>,
}

/// Directory of the document currently open in the app.
///
/// A `mount` writes its source as a path relative to the document that declares it, which is
/// the only thing an author can reasonably write - they know where their own files sit, not
/// where the binary was launched from. Resolution therefore needs the document's own
/// directory rather than the process working directory.
static BASE_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

#[derive(Default)]
pub struct DocumentManager {}

impl DocumentManager {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {}
    }

    /// Record the directory of the document being opened. Passing a file path stores its
    /// parent; passing `None` clears it, after which resolution falls back to the process
    /// working directory.
    pub fn set_current_document(path: Option<&str>) {
        let dir = path.and_then(|p| {
            let p = PathBuf::from(p);
            let abs = p.canonicalize().unwrap_or(p);
            abs.parent().map(|d| d.to_path_buf())
        });
        if let Ok(mut guard) = BASE_DIR.lock() {
            *guard = dir;
        }
    }

    /// Directory to resolve a document-relative path against.
    pub fn base_dir() -> Option<PathBuf> {
        BASE_DIR.lock().ok().and_then(|g| g.clone())
    }

    pub fn resolve_path(base: &Path, relative: &str) -> PathBuf {
        let p = Path::new(relative);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            base.join(p)
        }
    }

    /// Resolve a path written inside a document. Absolute paths are used as-is; relative
    /// ones resolve against the open document's directory, falling back to the process
    /// working directory when no document is open (which is the case in most tests).
    pub fn resolve_from_document(relative: &str) -> PathBuf {
        let p = Path::new(relative);
        if p.is_absolute() {
            return p.to_path_buf();
        }
        match Self::base_dir() {
            Some(base) => {
                let candidate = base.join(p);
                if candidate.exists() {
                    candidate
                } else {
                    // Fall back to the working directory so an author who launched the app
                    // from the document's folder is not worse off than before.
                    let cwd = Path::new(relative).to_path_buf();
                    if cwd.exists() {
                        cwd
                    } else {
                        candidate
                    }
                }
            }
            None => p.to_path_buf(),
        }
    }

    // Stub: open a document and return parsed nodes (to be wired later)
    #[allow(dead_code)]
    pub async fn open_document(_path: &Path) -> Result<Vec<OverseerNode>> {
        Err(crate::types::OverseerError::IoError(
            "open_document not implemented".into(),
        ))
    }
}
