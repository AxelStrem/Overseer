//! Who may read and change which document, when a server keeps accounts.
//!
//! The root holds a folder per person, and a person owns what is in theirs. What they let others
//! reach is written in their own `sharing.os`, an ordinary document: a list whose entries say a
//! `document` (named within their folder), who it is `with` (a username, or `everyone` for anyone
//! at all, signed in or not) and what they `can` do with it (`read`, or `edit`, which includes
//! reading). `sharing.os` itself is never shared, whatever it says, so only its owner reads or
//! changes it.
//!
//! The question is asked of every file a request reaches - the document named, what it mounts and
//! the bases its templates come from - so it is asked where those are read, inside the engine,
//! rather than only where a request arrives. Whoever serves the request says who is asking with
//! [`within`]; outside that, as in the desktop app, everything may be read and changed.

use std::cell::{Cell, RefCell};
use std::path::{Component, Path, PathBuf};
use std::sync::{LazyLock, Mutex};

/// The file in a person's folder that says what they share.
pub const SHARING: &str = "sharing.os";

/// Who shares with anyone at all, in `with`.
pub const EVERYONE: &str = "everyone";

/// What a write to a document shared only to be read is refused with.
pub const READ_ONLY: &str = "this document is shared with you to read, not to change";

/// Who a request comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum Asker {
    /// The server's own token, or no server at all: every file is open to it.
    Anyone,
    /// Someone with an account, by username.
    Person(String),
    /// Nobody signed in, who reaches only what is shared with everyone.
    Stranger,
}

/// What is wanted of a file.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Need {
    Read,
    Edit,
}

struct Asking {
    asker: Asker,
    /// The folder the people's folders are in.
    root: PathBuf,
}

thread_local! {
    static ASKING: RefCell<Option<Asking>> = const { RefCell::new(None) };
    /// Whether anything was refused since the request began - see `refused_any`.
    static REFUSED: Cell<bool> = const { Cell::new(false) };
}

/// Work as `asker`, whose documents are in folders under `root`.
pub fn within<T>(asker: Asker, root: &Path, f: impl FnOnce() -> T) -> T {
    let previous = ASKING.with(|held| {
        held.replace(Some(Asking {
            asker,
            root: root.to_path_buf(),
        }))
    });
    let refused_before = REFUSED.with(|r| r.replace(false));
    let result = f();
    ASKING.with(|held| *held.borrow_mut() = previous);
    REFUSED.with(|r| r.set(refused_before));
    result
}

/// Who is asking, where a server said; `Anyone` otherwise.
pub fn asker() -> Asker {
    ASKING.with(|held| {
        held.borrow()
            .as_ref()
            .map_or(Asker::Anyone, |asking| asking.asker.clone())
    })
}

/// The folder the people's folders are in, where a server said.
pub fn root() -> Option<PathBuf> {
    ASKING.with(|held| held.borrow().as_ref().map(|asking| asking.root.clone()))
}

/// Whether something this request reached was kept from it. A document worked out without a
/// file its asker may not read is not the document as its owner sees it, and is not kept for
/// anyone else to be handed - see `document_cache`.
pub fn refused_any() -> bool {
    REFUSED.with(Cell::get)
}

/// Whether whoever is asking may do `need` with this file.
///
/// A file that is not there is allowed: reading it fails as a missing file does, which is
/// what a refusal must look like anyway. So is one outside the root - the templates that ship
/// with Overseer, a theme - which belongs to no one.
pub fn may(file: &Path, need: Need) -> bool {
    let allowed = ASKING.with(|held| {
        let held = held.borrow();
        let Some(asking) = held.as_ref() else {
            return true;
        };
        let Ok(file) = file.canonicalize() else {
            return true;
        };
        let Ok(within_root) = file.strip_prefix(&asking.root) else {
            return true;
        };
        // The directory of people is the server's to write, from its accounts, and nobody's
        // through a document - the token's included, or a name could be changed past the profile.
        if need == Need::Edit && within_root == Path::new(crate::people::FILE) {
            return false;
        }
        if asking.asker == Asker::Anyone {
            return true;
        }
        allows(&asking.root, &asking.asker, within_root, need)
    });
    if !allowed {
        REFUSED.with(|r| r.set(true));
    }
    allowed
}

/// The same, for a path as the engine keeps it.
pub fn may_read(file: &str) -> bool {
    may(Path::new(file), Need::Read)
}

/// The rule itself, for a file named relative to the root.
pub fn allows(root: &Path, asker: &Asker, relative: &Path, need: Need) -> bool {
    let mut parts = relative.components().filter_map(|c| match c {
        Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
        _ => None,
    });
    let Some(owner) = parts.next() else {
        return false;
    };
    let inside: Vec<String> = parts.collect();
    // A file at the root itself - the server's own settings, its journal - is no one's. The
    // directory of people and the settings a stranger is drawn in are read by everyone.
    if inside.is_empty() {
        return need == Need::Read
            && (owner == crate::people::FILE || owner == crate::environment::FILE);
    }
    if let Asker::Person(me) = asker {
        if *me == owner {
            return true;
        }
    }
    let document = inside.join("/");
    if document == SHARING {
        return false;
    }
    shared_by(&root.join(&owner)).iter().any(|share| {
        share.document == document
            && match asker {
                Asker::Person(me) => share.with == *me || share.with == EVERYONE,
                Asker::Stranger => share.with == EVERYONE,
                Asker::Anyone => true,
            }
            && (need == Need::Read || share.can == "edit")
    })
}

/// One entry of a `sharing.os`.
#[derive(Clone, Debug, PartialEq)]
pub struct Share {
    pub document: String,
    pub with: String,
    pub can: String,
}

type Stamp = Option<(std::time::SystemTime, u64)>;

static SHARED: LazyLock<Mutex<std::collections::HashMap<PathBuf, (Stamp, Vec<Share>)>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

/// What the owner of this folder shares, read again only once their `sharing.os` has changed.
pub fn shared_by(folder: &Path) -> Vec<Share> {
    let file = folder.join(SHARING);
    let stamp = std::fs::metadata(&file)
        .ok()
        .and_then(|m| m.modified().ok().map(|t| (t, m.len())));
    if let Ok(held) = SHARED.lock() {
        if let Some((then, shares)) = held.get(&file) {
            if *then == stamp {
                return shares.clone();
            }
        }
    }
    let shares = std::fs::read_to_string(&file)
        .ok()
        .map(|text| read_shares(&text))
        .unwrap_or_default();
    if let Ok(mut held) = SHARED.lock() {
        held.insert(file, (stamp, shares.clone()));
    }
    shares
}

/// The entries of a `sharing.os`, worked out as any document is, so an entry that leaves `can`
/// out has the template's.
pub fn read_shares(text: &str) -> Vec<Share> {
    let Ok((_, mut nodes)) = crate::parser::parse_document(text) else {
        return Vec::new();
    };
    crate::environment::without(|| crate::resolver::resolve_document(&mut nodes));
    let mut shares = Vec::new();
    collect(&nodes, &mut shares);
    shares
}

fn collect(nodes: &[crate::types::OverseerNode], out: &mut Vec<Share>) {
    for node in nodes {
        if node.node_type == "list" {
            for (_, entry) in crate::addressing::effective_children(node) {
                let field = |name: &str| {
                    crate::addressing::effective_children(entry)
                        .into_iter()
                        .map(|(_, child)| child)
                        .find(|child| child.name == name)
                        .and_then(|child| {
                            child
                                .parameters
                                .get("_computed_value")
                                .or_else(|| child.parameters.get("value"))
                                .cloned()
                        })
                        .and_then(|value| match value {
                            crate::types::OverseerValue::String(s) => Some(s.trim().to_string()),
                            _ => None,
                        })
                };
                if let (Some(document), Some(with)) = (field("document"), field("with")) {
                    out.push(Share {
                        document: document.trim_start_matches('/').to_string(),
                        with: with.to_lowercase(),
                        can: field("can").unwrap_or_else(|| "read".into()).to_lowercase(),
                    });
                }
            }
        } else {
            collect(&node.children, out);
        }
    }
}
