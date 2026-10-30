//! The accounts people sign in to the server with: made by invite, kept in a file, and only ever
//! opened by the password or a session that has not run out.
//!
//!   cargo test --features server --test an_account_is_made_by_an_invite_used_once
#![cfg(feature = "server")]

use chrono::{Duration, Utc};
use overseer::accounts::{AccountError, Accounts, INVITE_LASTS, KEY_PREFIX, KEY_USE_NOTED_EVERY, SESSION_LASTS};
use std::path::PathBuf;

fn fresh_file(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("overseer-accounts-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("accounts.json")
}

fn refused<T: std::fmt::Debug>(outcome: Result<T, AccountError>) -> String {
    match outcome {
        Err(AccountError::Refused(said)) => said,
        other => panic!("expected a refusal, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn a_display_name_counts_the_text_it_shows() {
    let accounts = Accounts::open(fresh_file("shown")).unwrap();
    let now = Utc::now();
    let invite = accounts.invite(None, false, now).unwrap();
    accounts.join(&invite, "alice", "Alice", "a good password", now).unwrap();
    let coloured = |letters: usize| "<color=#e11d48 | a>".repeat(letters);

    // Sixty letters, each of its own colour: over a thousand characters written, sixty shown.
    let sixty = coloured(50) + &"b".repeat(10);
    assert!(sixty.chars().count() > 64);
    assert_eq!(accounts.change_own("alice", Some(&sixty), None, None, now).unwrap().name, sixty);
    let shown = refused(accounts.change_own("alice", Some(&("x".repeat(65))), None, None, now));
    assert!(shown.contains("shows"), "{}", shown);
    let written = refused(accounts.change_own("alice", Some(&coloured(60)), None, None, now));
    assert!(written.contains("as written"), "{}", written);
}

#[test]
fn a_chip_colour_is_a_hex_colour_or_the_default() {
    let accounts = Accounts::open(fresh_file("colour")).unwrap();
    let now = Utc::now();
    let invite = accounts.invite(None, false, now).unwrap();
    accounts.join(&invite, "alice", "Alice", "a good password", now).unwrap();
    assert_eq!(accounts.user("alice").unwrap().colour, None);
    let set = accounts.change_own("alice", None, Some("#E11D48"), None, now).unwrap();
    assert_eq!(set.colour.as_deref(), Some("#e11d48"));
    assert_eq!(accounts.change_own("alice", None, Some("#abc"), None, now).unwrap().colour.as_deref(), Some("#abc"));
    for bad in ["red", "#12345", "#ggg", "url(x)"] {
        refused(accounts.change_own("alice", None, Some(bad), None, now));
    }
    assert_eq!(accounts.user("alice").unwrap().colour.as_deref(), Some("#abc"), "a refusal changes nothing");
    assert_eq!(accounts.change_own("alice", None, Some(""), None, now).unwrap().colour, None);
}

#[test]
fn an_invite_makes_one_account_and_is_used_up() {
    let accounts = Accounts::open(fresh_file("once")).unwrap();
    let now = Utc::now();
    let invite = accounts.invite(None, false, now).unwrap();
    let (user, session) = accounts.join(&invite, "Alice", "Alice B", "a good password", now).unwrap();
    assert_eq!(user.username, "alice");
    assert_eq!(user.name, "Alice B");
    assert!(!user.admin);
    assert_eq!(accounts.session(&session, now).unwrap().username, "alice");

    let again = refused(accounts.join(&invite, "bob", "Bob", "a good password", now));
    assert!(again.contains("used"), "{}", again);
}

#[test]
fn an_invite_that_has_waited_too_long_is_refused() {
    let accounts = Accounts::open(fresh_file("expired")).unwrap();
    let then = Utc::now() - INVITE_LASTS - Duration::minutes(1);
    let invite = accounts.invite(None, false, then).unwrap();
    assert!(accounts.invited(&invite, Utc::now()).is_none());
    refused(accounts.join(&invite, "alice", "Alice", "a good password", Utc::now()));
}

#[test]
fn a_username_is_plain_and_not_taken_twice() {
    let accounts = Accounts::open(fresh_file("names")).unwrap();
    let now = Utc::now();
    let first = accounts.invite(None, false, now).unwrap();
    refused(accounts.join(&first, "al ice", "Alice", "a good password", now));
    // A refused join leaves the invite as it was.
    accounts.join(&first, "alice", "Alice", "a good password", now).unwrap();
    let second = accounts.invite(None, false, now).unwrap();
    let taken = refused(accounts.join(&second, "ALICE", "Someone", "a good password", now));
    assert!(taken.contains("taken"), "{}", taken);
    refused(accounts.join(&second, "bob", "Bob", "short", now));
}

#[test]
fn the_password_signs_in_and_nothing_else_does() {
    let accounts = Accounts::open(fresh_file("password")).unwrap();
    let now = Utc::now();
    let invite = accounts.invite(None, false, now).unwrap();
    accounts.join(&invite, "alice", "Alice", "a good password", now).unwrap();

    let (user, session) = accounts.sign_in("Alice", "a good password", now).unwrap();
    assert_eq!(user.username, "alice");
    assert!(accounts.session(&session, now).is_some());
    let wrong = refused(accounts.sign_in("alice", "a bad password", now));
    let nobody = refused(accounts.sign_in("nobody", "a good password", now));
    assert_eq!(wrong, nobody, "a refusal must not say which of the two was wrong");
}

#[test]
fn a_session_ends_when_signed_out_or_run_out() {
    let accounts = Accounts::open(fresh_file("sessions")).unwrap();
    let now = Utc::now();
    let invite = accounts.invite(None, false, now).unwrap();
    let (_, session) = accounts.join(&invite, "alice", "Alice", "a good password", now).unwrap();
    assert!(accounts.session(&session, now + SESSION_LASTS + Duration::minutes(1)).is_none());
    accounts.sign_out(&session, now).unwrap();
    assert!(accounts.session(&session, now).is_none());
}

#[test]
fn a_reset_sets_a_new_password_and_signs_out_everywhere() {
    let accounts = Accounts::open(fresh_file("reset")).unwrap();
    let now = Utc::now();
    let invite = accounts.invite(None, false, now).unwrap();
    let (_, old_session) = accounts.join(&invite, "alice", "Alice", "a good password", now).unwrap();

    let reset = accounts.invite(Some("alice"), false, now).unwrap();
    assert!(accounts.invited(&reset, now).unwrap().resets);
    let (user, _) = accounts.join(&reset, "", "", "a new password", now).unwrap();
    assert_eq!(user.name, "Alice", "a reset keeps the display name");
    assert!(accounts.session(&old_session, now).is_none());
    refused(accounts.sign_in("alice", "a good password", now));
    accounts.sign_in("alice", "a new password", now).unwrap();
}

#[test]
fn the_admin_is_invited_until_the_account_exists() {
    let file = fresh_file("bootstrap");
    let accounts = Accounts::open(&file).unwrap();
    let now = Utc::now();
    let earlier = accounts.bootstrap("ap31", now).unwrap().unwrap();
    let invite = accounts.bootstrap("ap31", now).unwrap().unwrap();
    assert!(accounts.invited(&earlier, now).is_none(), "only the last link printed works");
    let invited = accounts.invited(&invite, now).unwrap();
    assert_eq!(invited.username.as_deref(), Some("ap31"));
    assert!(!invited.resets);

    // The name is the invite's, whatever the form sends.
    let (user, _) = accounts.join(&invite, "someone", "Dmitrii", "a good password", now).unwrap();
    assert_eq!(user.username, "ap31");
    assert!(user.admin);
    assert!(accounts.bootstrap("ap31", now).unwrap().is_none());

    // And all of it is kept: a server started again knows the account.
    let reopened = Accounts::open(&file).unwrap();
    assert!(reopened.user("ap31").unwrap().admin);
    reopened.sign_in("ap31", "a good password", now).unwrap();
}

#[test]
fn a_person_changes_their_own_name_and_password() {
    let accounts = Accounts::open(fresh_file("own")).unwrap();
    let now = Utc::now();
    let invite = accounts.invite(None, false, now).unwrap();
    accounts.join(&invite, "alice", "Alice", "a good password", now).unwrap();

    let renamed = accounts.change_own("alice", Some("Alice C"), None, None, now).unwrap();
    assert_eq!(renamed.name, "Alice C");
    refused(accounts.change_own("alice", None, None, Some(("not it", "a new password")), now));
    accounts
        .change_own("alice", None, None, Some(("a good password", "a new password")), now)
        .unwrap();
    accounts.sign_in("alice", "a new password", now).unwrap();
}

#[test]
fn a_key_is_its_account_until_revoked() {
    let file = fresh_file("keys");
    let accounts = Accounts::open(&file).unwrap();
    let now = Utc::now();
    let invite = accounts.invite(None, false, now).unwrap();
    accounts.join(&invite, "alice", "Alice", "a good password", now).unwrap();

    let key = accounts.make_key("alice", " desktop bot ", now).unwrap();
    assert!(key.starts_with(KEY_PREFIX), "{}", key);
    assert_eq!(accounts.by_key(&key, now).unwrap().username, "alice");
    assert!(accounts.by_key("osk_made-up", now).is_none());
    assert!(accounts.session(&key, now).is_none(), "a key is no session");
    assert!(refused(accounts.make_key("alice", "desktop bot", now)).contains("already"));
    refused(accounts.make_key("alice", "  ", now));

    let listed = accounts.keys("alice");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "desktop bot");
    assert_eq!(listed[0].used, Some(now));
    assert!(!std::fs::read_to_string(&file).unwrap().contains(&key), "only its hash is kept");

    // A use soon after is not written down again; one later is.
    let soon = now + Duration::minutes(5);
    accounts.by_key(&key, soon).unwrap();
    assert_eq!(accounts.keys("alice")[0].used, Some(now));
    let later = now + KEY_USE_NOTED_EVERY + Duration::minutes(1);
    accounts.by_key(&key, later).unwrap();
    assert_eq!(accounts.keys("alice")[0].used, Some(later));

    // A reset of the password leaves keys alone; revoking ends one.
    let reset = accounts.invite(Some("alice"), false, now).unwrap();
    accounts.join(&reset, "alice", "", "a new password", now).unwrap();
    assert!(accounts.by_key(&key, now).is_some());
    assert!(Accounts::open(&file).unwrap().by_key(&key, now).is_some(), "keys are kept");
    assert!(accounts.revoke_key("alice", "desktop bot", now).unwrap());
    assert!(!accounts.revoke_key("alice", "desktop bot", now).unwrap());
    assert!(accounts.by_key(&key, now).is_none());
    assert!(accounts.keys("alice").is_empty());
}

#[test]
fn an_accounts_file_from_before_keys_still_opens() {
    let file = fresh_file("before-keys");
    std::fs::write(
        &file,
        r#"{"accounts":[{"username":"ap31","name":"Dmitrii","password":"x","admin":true,"sessions":[]}],"invites":[]}"#,
    )
    .unwrap();
    let accounts = Accounts::open(&file).unwrap();
    assert!(accounts.keys("ap31").is_empty());
    accounts.make_key("ap31", "bot", Utc::now()).unwrap();
}
