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
///
/// This one is process-wide, which is only defensible while exactly one document is open.
/// It remains as the fallback for callers that have not said which document they mean;
/// [`DocumentManager::with_document`] establishes the answer explicitly and takes precedence.
static BASE_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

thread_local! {
    /// The document an operation was told it is working on.
    ///
    /// Scoped rather than global so two documents can be resolved at the same time without
    /// one deciding where the other's mounts live - the process-wide answer is a single slot,
    /// and whichever loaded last would win. Resolving is synchronous work, and the scope is
    /// handed a closure rather than a guard, so it cannot be left open across an await and
    /// picked up by unrelated work on the same thread.
    static SCOPED_BASE_DIR: std::cell::RefCell<Option<Option<PathBuf>>> =
        const { std::cell::RefCell::new(None) };
}

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

    /// Run `f` with the directory that document-relative paths resolve against.
    ///
    /// Nested calls stack, so a document that resolves another does not lose its own context
    /// when the inner one finishes.
    pub fn with_document<T>(base: Option<PathBuf>, f: impl FnOnce() -> T) -> T {
        let previous = SCOPED_BASE_DIR.with(|slot| slot.replace(Some(base)));
        let result = f();
        SCOPED_BASE_DIR.with(|slot| *slot.borrow_mut() = previous);
        result
    }

    /// The directory of the document being worked on, if one was named.
    pub fn scoped_base_dir() -> Option<Option<PathBuf>> {
        SCOPED_BASE_DIR.with(|slot| slot.borrow().clone())
    }

    /// Directory to resolve a document-relative path against.
    ///
    /// What the caller said takes precedence over what the process last happened to open.
    pub fn base_dir() -> Option<PathBuf> {
        if let Some(scoped) = Self::scoped_base_dir() {
            return scoped;
        }
        BASE_DIR.lock().ok().and_then(|g| g.clone())
    }

    /// The directory a document path implies, for handing to [`Self::with_document`].
    pub fn dir_of(path: &str) -> Option<PathBuf> {
        let p = PathBuf::from(path);
        let abs = p.canonicalize().unwrap_or(p);
        abs.parent().map(|d| d.to_path_buf())
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
