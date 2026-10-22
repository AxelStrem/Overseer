//! The people who may sign in to the server, and the invites, sessions and keys that let them.
//!
//! One JSON file holds it all, written by nobody but the server. It is kept apart from the
//! documents on purpose: the documents are backed up to a repository, and password hashes have
//! no business travelling with them. Losing the file loses nothing written in a document - the
//! people it named join again by invite.
//!
//! Nobody is let in by a secret kept as it was given. A password is kept as its argon2 hash; an
//! invite, a session and a key are random tokens, kept as their SHA-256, which is enough for a
//! value nobody chose and nobody could guess.

use argon2::password_hash::rand_core::{OsRng, RngCore};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// How long an invite may wait to be used.
pub const INVITE_LASTS: Duration = Duration::days(7);
/// How long a sign-in lasts before it has to be made again.
pub const SESSION_LASTS: Duration = Duration::days(90);
/// The shortest password taken. Short enough not to be a nuisance, long enough not to be a PIN.
pub const SHORTEST_PASSWORD: usize = 8;
const LONGEST_USERNAME: usize = 32;
const LONGEST_NAME: usize = 64;
/// What every key begins with, so that one pasted somewhere is recognisable for what it is.
pub const KEY_PREFIX: &str = "osk_";
/// How stale a key's last use may be before it is written down again: often enough to tell a
/// key still in use from one forgotten, seldom enough that a program's every request does not
/// rewrite the file.
pub const KEY_USE_NOTED_EVERY: Duration = Duration::hours(1);

/// Someone who has signed in.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct User {
    pub username: String,
    /// What they are called where a person reads it.
    pub name: String,
    /// Whether they may invite others, and send someone a new password.
    pub admin: bool,
}

/// What an invite will do when it is used.
#[derive(Clone, Debug, PartialEq)]
pub struct Invited {
    /// The username it is for, if it was made for one; anyone may be chosen otherwise.
    pub username: Option<String>,
    /// Whether that account exists already, so that using the invite only sets a new password.
    pub resets: bool,
}

/// A key, as its owner is shown it: never the key itself, which is seen once, when it is made.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct KeyShown {
    pub name: String,
    pub made: DateTime<Utc>,
    /// When it was last used, to within `KEY_USE_NOTED_EVERY`.
    pub used: Option<DateTime<Utc>>,
}

#[derive(Debug)]
pub enum AccountError {
    /// The request was understood and is not allowed: the sentence is for the person who made it.
    Refused(String),
    /// Something went wrong keeping the file.
    Failed(String),
}

impl std::fmt::Display for AccountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AccountError::Refused(said) | AccountError::Failed(said) => f.write_str(said),
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct Store {
    #[serde(default)]
    accounts: Vec<Account>,
    #[serde(default)]
    invites: Vec<Invite>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Account {
    username: String,
    name: String,
    /// The argon2 hash, in the PHC form that carries its own salt and parameters.
    password: String,
    #[serde(default)]
    admin: bool,
    #[serde(default)]
    sessions: Vec<Secret>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    keys: Vec<Key>,
}

/// A key a program uses to be its account, kept until its owner revokes it.
#[derive(Clone, Serialize, Deserialize)]
struct Key {
    name: String,
    hash: String,
    made: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    used: Option<DateTime<Utc>>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Secret {
    /// The SHA-256 of the token, in hex.
    hash: String,
    expires: DateTime<Utc>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Invite {
    hash: String,
    expires: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    #[serde(default)]
    admin: bool,
}

pub struct Accounts {
    path: PathBuf,
    store: Mutex<Store>,
    /// The folder whose `people.os` is kept in step with the accounts - see `people`.
    directory: Option<PathBuf>,
}

fn hashed(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

fn new_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn hash_password(password: &str) -> Result<String, AccountError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|_| AccountError::Failed("the password could not be hashed".into()))
}

fn password_matches(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
        .unwrap_or(false)
}

/// A username as it is kept, or why it cannot be one. It is part of the addresses other people
/// will write, so it is held to what reads the same everywhere: lower case letters, digits, `-`
/// and `_`.
pub fn valid_username(username: &str) -> Result<String, AccountError> {
    let username = username.trim().to_lowercase();
    let fits = !username.is_empty()
        && username.len() <= LONGEST_USERNAME
        && username
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if fits {
        Ok(username)
    } else {
        Err(AccountError::Refused(format!(
            "a username is 1 to {} lower case letters, digits, '-' or '_'",
            LONGEST_USERNAME
        )))
    }
}

fn valid_name(name: &str) -> Result<String, AccountError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > LONGEST_NAME {
        return Err(AccountError::Refused(format!(
            "a display name is 1 to {} characters",
            LONGEST_NAME
        )));
    }
    Ok(name.to_string())
}

fn valid_password(password: &str) -> Result<(), AccountError> {
    if password.chars().count() < SHORTEST_PASSWORD {
        return Err(AccountError::Refused(format!(
            "a password is at least {} characters",
            SHORTEST_PASSWORD
        )));
    }
    Ok(())
}

fn valid_key_name(name: &str) -> Result<String, AccountError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > LONGEST_NAME {
        return Err(AccountError::Refused(format!(
            "a key's name is 1 to {} characters",
            LONGEST_NAME
        )));
    }
    Ok(name.to_string())
}

fn as_user(account: &Account) -> User {
    User {
        username: account.username.clone(),
        name: account.name.clone(),
        admin: account.admin,
    }
}

impl Store {
    fn account(&self, username: &str) -> Option<&Account> {
        self.accounts.iter().find(|a| a.username == username)
    }

    fn account_mut(&mut self, username: &str) -> Option<&mut Account> {
        self.accounts.iter_mut().find(|a| a.username == username)
    }

    /// Forget what has run out, so the file does not grow with every sign-in ever made.
    fn prune(&mut self, now: DateTime<Utc>) {
        self.invites.retain(|i| i.expires > now);
        for account in &mut self.accounts {
            account.sessions.retain(|s| s.expires > now);
        }
    }

    fn open_session(&mut self, username: &str, now: DateTime<Utc>) -> Option<String> {
        let token = new_token();
        let account = self.account_mut(username)?;
        account.sessions.push(Secret {
            hash: hashed(&token),
            expires: now + SESSION_LASTS,
        });
        Some(token)
    }
}

impl Accounts {
    /// The accounts kept in `path`, which need not exist yet: the first one made writes it.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, AccountError> {
        let path = path.into();
        let store = match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| AccountError::Failed(format!("the accounts file does not read: {}", e)))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Store::default(),
            Err(e) => {
                return Err(AccountError::Failed(format!(
                    "the accounts file cannot be read: {}",
                    e.kind()
                )))
            }
        };
        Ok(Self {
            path,
            store: Mutex::new(store),
            directory: None,
        })
    }

    /// Keep `people.os` in `root` saying who has an account and what they are called, from now
    /// on and after every change - see `people`.
    pub fn with_directory(mut self, root: impl Into<PathBuf>) -> Result<Self, AccountError> {
        self.directory = Some(root.into());
        self.write_directory()?;
        Ok(self)
    }

    /// Everyone with an account, as `(username, display name)`, in the order they joined.
    pub fn people(&self) -> Vec<(String, String)> {
        self.read(|store| {
            store
                .accounts
                .iter()
                .map(|a| (a.username.clone(), a.name.clone()))
                .collect()
        })
        .unwrap_or_default()
    }

    fn write_directory(&self) -> Result<(), AccountError> {
        let Some(root) = &self.directory else {
            return Ok(());
        };
        crate::people::write(root, &self.people()).map_err(|e| {
            AccountError::Failed(format!("the directory of people could not be written: {}", e.kind()))
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Change the store and keep the change, or change nothing.
    ///
    /// The change is made on a copy, which replaces what is held only once it is on disk: a
    /// disk that refused it must not leave a session working that the next start forgets, nor
    /// an invite used up that was never written as used.
    fn change<T>(
        &self,
        now: DateTime<Utc>,
        f: impl FnOnce(&mut Store) -> Result<T, AccountError>,
    ) -> Result<T, AccountError> {
        let mut held = self
            .store
            .lock()
            .map_err(|_| AccountError::Failed("the accounts are unavailable".into()))?;
        let mut store = held.clone();
        store.prune(now);
        let outcome = f(&mut store)?;
        let text = serde_json::to_string_pretty(&store)
            .map_err(|e| AccountError::Failed(format!("the accounts could not be written: {}", e)))?;
        // Written beside it and moved into place, so a crash halfway leaves the old file whole.
        let fresh = self.path.with_extension("tmp");
        std::fs::write(&fresh, text)
            .and_then(|_| std::fs::rename(&fresh, &self.path))
            .map_err(|e| {
                AccountError::Failed(format!("the accounts could not be written: {}", e.kind()))
            })?;
        *held = store;
        drop(held);
        // The accounts are kept whatever happens to the directory: it is written again after the
        // next change, and at the next start.
        if let Err(e) = self.write_directory() {
            eprintln!("{}", e);
        }
        Ok(outcome)
    }

    fn read<T>(&self, f: impl FnOnce(&Store) -> T) -> Option<T> {
        self.store.lock().ok().map(|store| f(&store))
    }

    pub fn user(&self, username: &str) -> Option<User> {
        self.read(|s| s.account(username).map(as_user)).flatten()
    }

    /// Make an invite, and give back its token - the only time it is seen. `username` binds it
    /// to that name: to an account that exists, it sends a new password; to one that does not,
    /// it is the only name that may be taken with it.
    pub fn invite(
        &self,
        username: Option<&str>,
        admin: bool,
        now: DateTime<Utc>,
    ) -> Result<String, AccountError> {
        let username = username.map(valid_username).transpose()?;
        let token = new_token();
        self.change(now, |store| {
            store.invites.push(Invite {
                hash: hashed(&token),
                expires: now + INVITE_LASTS,
                username,
                admin,
            });
            Ok(())
        })?;
        Ok(token)
    }

    /// What an invite will do, if it can still be used.
    pub fn invited(&self, token: &str, now: DateTime<Utc>) -> Option<Invited> {
        let hash = hashed(token);
        self.read(|store| {
            let invite = store.invites.iter().find(|i| i.hash == hash && i.expires > now)?;
            Some(Invited {
                username: invite.username.clone(),
                resets: invite
                    .username
                    .as_deref()
                    .is_some_and(|u| store.account(u).is_some()),
            })
        })
        .flatten()
    }

    /// Use an invite: make the account it is for, or set its new password. The invite is used
    /// up, and the person is signed in - the second value is their session's token. A reset
    /// signs every other session of theirs out, since a forgotten password may be a stolen one.
    pub fn join(
        &self,
        token: &str,
        username: &str,
        name: &str,
        password: &str,
        now: DateTime<Utc>,
    ) -> Result<(User, String), AccountError> {
        let hash = hashed(token);
        valid_password(password)?;
        let password_hash = hash_password(password)?;
        self.change(now, |store| {
            let at = store
                .invites
                .iter()
                .position(|i| i.hash == hash)
                .ok_or_else(|| {
                    AccountError::Refused("this invite has been used or has expired".into())
                })?;
            let invite = store.invites.remove(at);
            let username = match invite.username.clone() {
                Some(bound) => bound,
                None => valid_username(username)?,
            };
            match store.account_mut(&username) {
                Some(account) if invite_resets(&invite.username, &username) => {
                    account.password = password_hash;
                    // Keys stay: each was made on purpose for a program, and is revoked on
                    // its own from the account page if it is the one that leaked.
                    account.sessions.clear();
                    account.admin |= invite.admin;
                }
                Some(_) => {
                    return Err(AccountError::Refused(format!(
                        "the username '{}' is taken",
                        username
                    )))
                }
                None => store.accounts.push(Account {
                    username: username.clone(),
                    name: valid_name(name)?,
                    password: password_hash,
                    admin: invite.admin,
                    sessions: Vec::new(),
                    keys: Vec::new(),
                }),
            }
            let session = store
                .open_session(&username, now)
                .ok_or_else(|| AccountError::Failed("the account was not kept".into()))?;
            let user = as_user(store.account(&username).expect("just made"));
            Ok((user, session))
        })
    }

    /// Sign in with a password. A refusal says the same whichever of the two was wrong, so it
    /// does not tell a stranger which usernames exist.
    pub fn sign_in(
        &self,
        username: &str,
        password: &str,
        now: DateTime<Utc>,
    ) -> Result<(User, String), AccountError> {
        let refused = || AccountError::Refused("wrong username or password".into());
        let username = username.trim().to_lowercase();
        let hash = self
            .read(|s| s.account(&username).map(|a| a.password.clone()))
            .flatten();
        // Hashed against something even for a name nobody has, so that the time an answer
        // takes does not say whether the account exists.
        let matches = match &hash {
            Some(hash) => password_matches(password, hash),
            None => {
                let _ = hash_password(password);
                false
            }
        };
        if !matches {
            return Err(refused());
        }
        self.change(now, |store| {
            let session = store.open_session(&username, now).ok_or_else(refused)?;
            Ok((as_user(store.account(&username).ok_or_else(refused)?), session))
        })
    }

    /// Who a session's token belongs to, while it lasts.
    pub fn session(&self, token: &str, now: DateTime<Utc>) -> Option<User> {
        let hash = hashed(token);
        self.read(|store| {
            store
                .accounts
                .iter()
                .find(|a| a.sessions.iter().any(|s| s.hash == hash && s.expires > now))
                .map(as_user)
        })
        .flatten()
    }

    pub fn sign_out(&self, token: &str, now: DateTime<Utc>) -> Result<(), AccountError> {
        let hash = hashed(token);
        self.change(now, |store| {
            for account in &mut store.accounts {
                account.sessions.retain(|s| s.hash != hash);
            }
            Ok(())
        })
    }

    /// Make a key for a program to act as `username` with, and give it back - the only time it
    /// is seen. The name tells it from the others when it is listed or revoked.
    pub fn make_key(&self, username: &str, name: &str, now: DateTime<Utc>) -> Result<String, AccountError> {
        let name = valid_key_name(name)?;
        let token = format!("{}{}", KEY_PREFIX, new_token());
        self.change(now, |store| {
            let account = store
                .account_mut(username)
                .ok_or_else(|| AccountError::Refused("there is no such account".into()))?;
            if account.keys.iter().any(|k| k.name == name) {
                return Err(AccountError::Refused(format!("there is a key named '{}' already", name)));
            }
            account.keys.push(Key {
                name,
                hash: hashed(&token),
                made: now,
                used: None,
            });
            Ok(())
        })?;
        Ok(token)
    }

    /// The keys of an account, in the order they were made.
    pub fn keys(&self, username: &str) -> Vec<KeyShown> {
        self.read(|store| {
            store.account(username).map(|account| {
                account
                    .keys
                    .iter()
                    .map(|k| KeyShown {
                        name: k.name.clone(),
                        made: k.made,
                        used: k.used,
                    })
                    .collect()
            })
        })
        .flatten()
        .unwrap_or_default()
    }

    /// Revoke a key by its name; whether there was one.
    pub fn revoke_key(&self, username: &str, name: &str, now: DateTime<Utc>) -> Result<bool, AccountError> {
        self.change(now, |store| {
            let account = store
                .account_mut(username)
                .ok_or_else(|| AccountError::Refused("there is no such account".into()))?;
            let before = account.keys.len();
            account.keys.retain(|k| k.name != name);
            Ok(account.keys.len() != before)
        })
    }

    /// Who a key belongs to, while it is not revoked. Its use is written down when what was
    /// written last is older than `KEY_USE_NOTED_EVERY`.
    pub fn by_key(&self, token: &str, now: DateTime<Utc>) -> Option<User> {
        let hash = hashed(token);
        let (user, stale) = self
            .read(|store| {
                store.accounts.iter().find_map(|a| {
                    let key = a.keys.iter().find(|k| k.hash == hash)?;
                    let stale = key.used.map_or(true, |used| now - used >= KEY_USE_NOTED_EVERY);
                    Some((as_user(a), stale))
                })
            })
            .flatten()?;
        if stale {
            // A key that works is let in whether or not its use could be written down.
            let noted = self.change(now, |store| {
                for account in &mut store.accounts {
                    for key in account.keys.iter_mut().filter(|k| k.hash == hash) {
                        key.used = Some(now);
                    }
                }
                Ok(())
            });
            if let Err(e) = noted {
                eprintln!("{}", e);
            }
        }
        Some(user)
    }

    /// Change one's own display name, password, or both. A new password needs the current one:
    /// a session left open on someone else's phone must not be enough to take the account.
    pub fn change_own(
        &self,
        username: &str,
        name: Option<&str>,
        password: Option<(&str, &str)>,
        now: DateTime<Utc>,
    ) -> Result<User, AccountError> {
        let name = name.map(valid_name).transpose()?;
        let password_hash = match password {
            Some((current, new)) => {
                let kept = self
                    .read(|s| s.account(username).map(|a| a.password.clone()))
                    .flatten()
                    .ok_or_else(|| AccountError::Refused("there is no such account".into()))?;
                if !password_matches(current, &kept) {
                    return Err(AccountError::Refused("the current password is wrong".into()));
                }
                valid_password(new)?;
                Some(hash_password(new)?)
            }
            None => None,
        };
        self.change(now, |store| {
            let account = store
                .account_mut(username)
                .ok_or_else(|| AccountError::Refused("there is no such account".into()))?;
            if let Some(name) = name {
                account.name = name;
            }
            if let Some(hash) = password_hash {
                account.password = hash;
            }
            Ok(as_user(account))
        })
    }

    /// Make sure the server's administrator can get in: an account of that name is made an
    /// admin, and while there is none, an invite to make it is given back. Earlier invites of
    /// this kind go, so that only the link printed last works.
    pub fn bootstrap(&self, username: &str, now: DateTime<Utc>) -> Result<Option<String>, AccountError> {
        let username = valid_username(username)?;
        if self.user(&username).is_some() {
            if self.user(&username).is_some_and(|u| !u.admin) {
                self.change(now, |store| {
                    if let Some(account) = store.account_mut(&username) {
                        account.admin = true;
                    }
                    Ok(())
                })?;
            }
            return Ok(None);
        }
        let token = new_token();
        self.change(now, |store| {
            store
                .invites
                .retain(|i| !(i.admin && i.username.as_deref() == Some(username.as_str())));
            store.invites.push(Invite {
                hash: hashed(&token),
                expires: now + INVITE_LASTS,
                username: Some(username.clone()),
                admin: true,
            });
            Ok(())
        })?;
        Ok(Some(token))
    }
}

/// Whether an invite bound to `bound` sets a new password for `username`, rather than being
/// refused because the name is taken.
fn invite_resets(bound: &Option<String>, username: &str) -> bool {
    bound.as_deref() == Some(username)
}
