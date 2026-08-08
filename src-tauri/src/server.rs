//! Serving documents over HTTP.
//!
//! The logic lives here rather than in the request handlers so it can be tested without a
//! socket, and so the handlers stay thin enough to read. Everything here is about *which*
//! document, and whether the caller is allowed to ask for it; what a document is remains
//! [`crate::app_api`]'s business, shared with the desktop app so the two cannot drift.

use crate::app_api;
use crate::docmgr::manager::DocumentManager;
use crate::types::*;
use std::path::{Component, Path, PathBuf};

/// The directory documents are served from. Nothing outside it is reachable.
#[derive(Clone, Debug)]
pub struct DocumentRoot {
    root: PathBuf,
}

#[derive(Debug, PartialEq)]
pub enum RequestError {
    /// The name is not one this server will look up, regardless of what is on disk.
    Rejected(String),
    NotFound(String),
    Failed(String),
}

impl std::fmt::Display for RequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestError::Rejected(m) | RequestError::NotFound(m) | RequestError::Failed(m) => {
                write!(f, "{}", m)
            }
        }
    }
}

impl DocumentRoot {
    pub fn new(root: impl AsRef<Path>) -> std::io::Result<Self> {
        Ok(Self {
            root: root.as_ref().canonicalize()?,
        })
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    /// The file a requested name refers to.
    ///
    /// A name arrives from the network, so it is checked rather than trusted. Parent
    /// components are refused outright instead of being normalized away - `a/../../b` can be
    /// made to look harmless by normalizing, and there is no legitimate reason to write it.
    /// The resolved path is then confirmed to still sit under the root, which is what catches
    /// a symlink pointing somewhere else entirely.
    pub fn resolve(&self, name: &str) -> std::result::Result<PathBuf, RequestError> {
        if name.is_empty() {
            return Err(RequestError::Rejected("no document named".into()));
        }
        let relative = Path::new(name);
        if relative.is_absolute() {
            return Err(RequestError::Rejected(format!(
                "'{}' is an absolute path; documents are named relative to the root",
                name
            )));
        }
        for component in relative.components() {
            match component {
                Component::Normal(_) => {}
                _ => {
                    return Err(RequestError::Rejected(format!(
                        "'{}' contains a path component that is not a name",
                        name
                    )))
                }
            }
        }
        if relative.extension().and_then(|e| e.to_str()) != Some("os") {
            return Err(RequestError::Rejected(format!(
                "'{}' is not an .os document",
                name
            )));
        }

        let candidate = self.root.join(relative);
        let resolved = candidate
            .canonicalize()
            .map_err(|_| RequestError::NotFound(format!("no document '{}'", name)))?;
        if !resolved.starts_with(&self.root) {
            // Reported as missing rather than refused: which paths exist outside the root is
            // not something a caller needs to learn from the error it gets back.
            return Err(RequestError::NotFound(format!("no document '{}'", name)));
        }
        Ok(resolved)
    }

    /// Every document under the root, named as a caller would ask for it.
    pub fn list(&self) -> Vec<String> {
        fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, root, out);
                } else if path.extension().and_then(|e| e.to_str()) == Some("os") {
                    if let Ok(relative) = path.strip_prefix(root) {
                        out.push(relative.to_string_lossy().replace('\\', "/"));
                    }
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.root, &self.root, &mut out);
        out.sort();
        out
    }

    /// Read and resolve a document, the way opening it in the app would.
    pub fn open(&self, name: &str) -> std::result::Result<Vec<OverseerNode>, RequestError> {
        let path = self.resolve(name)?;
        let text = std::fs::read_to_string(&path)
            .map_err(|e| RequestError::Failed(format!("could not read '{}': {}", name, e)))?;
        // Naming the document is what lets its mounts resolve against its own directory, and
        // what keeps two documents being served at once from deciding for each other.
        let dir = path.parent().map(|d| d.to_path_buf());
        DocumentManager::with_document(dir, || app_api::load_document(text))
            .map_err(|e| RequestError::Failed(format!("could not resolve '{}': {:?}", name, e)))
    }
}
