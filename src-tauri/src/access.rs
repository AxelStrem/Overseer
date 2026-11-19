//! Who may read and change which document, when a server keeps accounts.
//!
//! The root holds a folder per person, and a person owns what is in theirs. What they let others
//! reach is written in their own `sharing.os`, an ordinary document: a list whose entries say a
//! `document` (named within their folder; several, separated by commas; or a folder, ending in
//! `/`, for everything under it), who it is `with` (usernames, groups the same document defines,
//! or `everyone` for anyone at all, signed in or not) and what they `can` do with it (`read`, or
//! `edit`, which includes reading). Every entry is asked, and any one that allows is enough, so
//! where entries overlap the widest wins. `sharing.os` itself is never shared, whatever it says,
//! so only its owner reads or changes it.
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
    /// Anyone at all, looking at documents served for everyone to read: every file under the
    /// root is theirs to read, and none to change.
    Visitor,
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
            // The templates that ship with Overseer are read by every document a server works
            // out, and a page may open the drawer's base itself - see `drawer` - but nothing a
            // request does changes them for everyone.
            return need == Need::Read || !crate::drawer::in_shipped_library(&file);
        };
        // The directory of people is the server's to write, from its accounts, and nobody's
        // through a document - the token's included, or a name could be changed past the profile.
        if need == Need::Edit && within_root == Path::new(crate::people::FILE) {
            return false;
        }
        if asking.asker == Asker::Anyone {
            return true;
        }
        if asking.asker == Asker::Visitor {
            return need == Need::Read;
        }
        allows(&asking.root, &asking.asker, within_root, need)
    });
    if !allowed {
        REFUSED.with(|r| r.set(true));
    }
    allowed
}

/// Whether whoever is asking may write `part` through `document`: a file of the document's own
/// parts folder, `<doc>.parts/<list>/<part>.os` beside it, is written by whoever may change the
/// document. Anything else is asked of the file itself.
///
/// The part's own file is not opened up by this - `allows` still refuses it by its name to all but
/// its owner, so a part is only ever changed as its document says.
pub fn may_write_part(part: &Path, document: &Path) -> bool {
    let through = (|| {
        let list = part.parent()?;
        let parts = list.parent()?;
        let stem = document.file_stem()?.to_string_lossy();
        // `file_name` is none for `..`, so neither step can climb out of the parts folder.
        part.file_name()?;
        list.file_name()?;
        if parts.file_name()? != std::ffi::OsStr::new(&format!("{}.parts", stem)) {
            return None;
        }
        let beside = parts.parent()?.canonicalize().ok()?;
        Some(beside == document.parent()?.canonicalize().ok()?)
    })()
    .unwrap_or(false);
    if through {
        may(document, Need::Edit)
    } else {
        may(part, Need::Edit)
    }
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
    // A part a list keeps its entries in is read by whoever may read its document - a reader of
    // the text is sent its name with the document's. Only read: it is written through its document,
    // by whoever may change that - see `may_write_part`. Nor does a share of the folder it is in
    // reach it by its own name, for the same reason.
    let host = part_of(&inside);
    let sharing = sharing_of(&root.join(&owner));
    sharing.shares.iter().any(|share| {
        let reached = match &host {
            Some(host) => need == Need::Read && share.documents().iter().any(|entry| covers_host(entry, host)),
            None => share.documents().iter().any(|entry| covers(entry, &document)),
        };
        reached
            && sharing.people(share).iter().any(|who| match asker {
                Asker::Person(me) => who == me || who == EVERYONE,
                Asker::Stranger => who == EVERYONE,
                Asker::Visitor => false,
                Asker::Anyone => true,
            })
            && (need == Need::Read || share.can == "edit")
    })
}

/// The steps of a name an entry gives in `document`, and whether it names a folder: none for one
/// that names nothing - empty, the whole folder, or with a `.` or `..` step, which could otherwise
/// reach outside the sharer's folder.
fn steps(entry: &str) -> Option<(Vec<&str>, bool)> {
    let folder = entry.ends_with('/');
    let mut steps = Vec::new();
    for step in entry.split('/') {
        match step {
            "" => {}
            "." | ".." => return None,
            step => steps.push(step),
        }
    }
    (!steps.is_empty()).then_some((steps, folder))
}

/// Whether a name an entry gives in `document` covers this document, named within its owner's
/// folder: the document itself, or, for a name ending in `/`, every document under that folder.
pub fn covers(entry: &str, document: &str) -> bool {
    let Some((steps, folder)) = steps(entry) else {
        return false;
    };
    let named: Vec<&str> = document.split('/').filter(|s| !s.is_empty()).collect();
    if named.len() < steps.len() || named[..steps.len()] != steps[..] {
        return false;
    }
    if folder {
        named.len() > steps.len()
    } else {
        named.len() == steps.len()
    }
}

/// The same, for a document named without its extension, as its parts folder names it.
fn covers_host(entry: &str, host: &str) -> bool {
    if entry.ends_with('/') {
        return covers(entry, host);
    }
    let Some((mut steps, _)) = steps(entry) else {
        return false;
    };
    let Some(last) = steps.pop() else {
        return false;
    };
    let stem = Path::new(last).with_extension("");
    let stem = stem.to_string_lossy();
    steps.push(&stem);
    covers(&steps.join("/"), host)
}

/// For a file in a document's parts folder, `<doc>.parts/<list>/<part>.os`: the document's name
/// within its owner's folder, without its extension.
fn part_of(inside: &[String]) -> Option<String> {
    let [folder @ .., parts, _list, _part] = inside else {
        return None;
    };
    let stem = parts.strip_suffix(".parts").filter(|s| !s.is_empty())?;
    Some(folder.iter().map(String::as_str).chain([stem]).collect::<Vec<_>>().join("/"))
}

/// What a person's `sharing.os` says before they have shared anything.
pub const SHARING_STARTER: &str = "div (hidden=true) {\r
    div Share (layout=\"horizontal\") {\r
        string document (label=\"Document\") = \"\"\r
        string with (label=\"With\") = \"\"\r
        string can (label=\"Can\") = \"read\"\r
    }\r
\r
    div Group (layout=\"horizontal\") {\r
        string name (label=\"Group\") = \"\"\r
        string members (label=\"Members\") = \"\"\r
    }\r
}\r
\r
tab sharing (label=\"Sharing\", mutable=true) {\r
    text about = \"Each entry shares documents of yours: *document* names one, several separated by commas, or a folder ending in `/` for everything in it, now and later. *With* is a username, a group of yours, or `everyone` for anyone at all, signed in or not, several separated by commas. *Can* is `read` or `edit`; where entries overlap, the widest wins.\"\r
\r
    div adding (layout=\"horizontal\") {\r
        textbox document (placeholder=\"tasks.os, or projects/\") = \"\"\r
        textbox with (placeholder=\"a username, a group, or everyone\") = \"\"\r
        textbox can (placeholder=\"read or edit\") = \"\"\r
\r
        button add (label=\"Share\") {\r
            on click {\r
                append (list=\"/sharing/Shares\", from=\"..\")\r
            }\r
        }\r
    }\r
\r
    list Shares (entry=<Share>, layout=\"vertical\") { }\r
\r
    div grouping (layout=\"horizontal\") {\r
        textbox name (placeholder=\"a group's name\") = \"\"\r
        textbox members (placeholder=\"usernames, separated by commas\") = \"\"\r
\r
        button add (label=\"Add the group\") {\r
            on click {\r
                append (list=\"/sharing/Groups\", from=\"..\")\r
            }\r
        }\r
    }\r
\r
    list Groups (entry=<Group>, key=\"name\", layout=\"vertical\") { }\r
\r
    div (mutable=\"guarded\") {\r
        button reach (label=\"What it comes to\") {\r
            on click {\r
                call (name=\"sharing_reach\", into=\"../reached\")\r
            }\r
        }\r
        text reached = \"\"\r
    }\r
}\r
";

/// Make the `sharing.os` in a person's folder, unless they have one, and answer its file.
pub fn start_sharing(folder: &Path) -> std::io::Result<PathBuf> {
    let file = folder.join(SHARING);
    if !file.exists() {
        std::fs::create_dir_all(folder)?;
        // Written beside it and moved into place, so a reader never meets half a file.
        let fresh = file.with_extension("tmp");
        std::fs::write(&fresh, SHARING_STARTER.as_bytes())?;
        std::fs::rename(&fresh, &file)?;
    }
    Ok(file)
}

/// One entry of a `sharing.os`, as written.
#[derive(Clone, Debug, PartialEq)]
pub struct Share {
    /// One or more documents, separated by commas; a name ending in `/` is a folder.
    pub document: String,
    /// One or more usernames, groups or `everyone`, separated by commas.
    pub with: String,
    pub can: String,
}

impl Share {
    /// The names `document` gives, each as written.
    pub fn documents(&self) -> Vec<&str> {
        names(&self.document)
    }
}

/// The names in a comma-separated field, without the spaces around them.
fn names(field: &str) -> Vec<&str> {
    field.split(',').map(str::trim).filter(|s| !s.is_empty()).collect()
}

/// What a `sharing.os` says: its entries, and the groups of accounts they may name in `with`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sharing {
    pub shares: Vec<Share>,
    /// Each group's members, by username, under the group's name.
    pub groups: std::collections::BTreeMap<String, Vec<String>>,
}

impl Sharing {
    /// Who an entry is with: its usernames and `everyone`, and the members of each group it names.
    /// A group's name is the sharer's own and stands for its members even where an account has the
    /// same name, so an account made later under that name gets nothing by it.
    pub fn people(&self, share: &Share) -> Vec<String> {
        let mut people = Vec::new();
        for name in names(&share.with) {
            match self.groups.get(name) {
                Some(members) => people.extend(members.iter().cloned()),
                None => people.push(name.to_string()),
            }
        }
        people.sort();
        people.dedup();
        people
    }
}

type Stamp = Option<(std::time::SystemTime, u64)>;

static SHARED: LazyLock<Mutex<std::collections::HashMap<PathBuf, (Stamp, Sharing)>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

/// What the owner of this folder shares.
pub fn shared_by(folder: &Path) -> Vec<Share> {
    sharing_of(folder).shares
}

/// What the owner of this folder shares, with the groups its entries may name, read again only
/// once their `sharing.os` has changed.
pub fn sharing_of(folder: &Path) -> Sharing {
    let file = folder.join(SHARING);
    let stamp = std::fs::metadata(&file)
        .ok()
        .and_then(|m| m.modified().ok().map(|t| (t, m.len())));
    if let Ok(held) = SHARED.lock() {
        if let Some((then, sharing)) = held.get(&file) {
            if *then == stamp {
                return sharing.clone();
            }
        }
    }
    let sharing = std::fs::read_to_string(&file)
        .ok()
        .map(|text| read_sharing(&text))
        .unwrap_or_default();
    if let Ok(mut held) = SHARED.lock() {
        held.insert(file, (stamp, sharing.clone()));
    }
    sharing
}

/// The entries of a `sharing.os`, worked out as any document is, so an entry that leaves `can`
/// out has the template's.
pub fn read_shares(text: &str) -> Vec<Share> {
    read_sharing(text).shares
}

/// The entries and the groups of a `sharing.os`: an entry of any of its lists with a `document`
/// and a `with` is a share, and one with a `name` and `members` a group.
pub fn read_sharing(text: &str) -> Sharing {
    let Ok((_, mut nodes)) = crate::parser::parse_document(text) else {
        return Sharing::default();
    };
    crate::environment::without(|| crate::resolver::resolve_document(&mut nodes));
    let mut sharing = Sharing::default();
    collect(&nodes, &mut sharing);
    sharing
}

fn collect(nodes: &[crate::types::OverseerNode], out: &mut Sharing) {
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
                    // A name written from the top of the folder, `/projects/`, is named within it.
                    let document = names(&document)
                        .into_iter()
                        .map(|name| name.trim_start_matches('/'))
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.shares.push(Share {
                        document,
                        with: with.to_lowercase(),
                        can: field("can").unwrap_or_else(|| "read".into()).to_lowercase(),
                    });
                } else if let (Some(name), Some(members)) = (field("name"), field("members")) {
                    let name = name.to_lowercase();
                    if !name.is_empty() {
                        // A member is an account: never another group, nor everyone.
                        let members = members.to_lowercase();
                        let members = names(&members)
                            .into_iter()
                            .filter(|member| *member != EVERYONE)
                            .map(str::to_string)
                            .collect();
                        out.groups.insert(name, members);
                    }
                }
            }
        } else {
            collect(&node.children, out);
        }
    }
}
