//! What each viewer is looking at, kept out of the documents they are looking at.
//!
//! Which day the food tracker is showing, whether a card is folded, what a filter box holds -
//! all of it is real while somebody is looking and none of it is a fact about anybody. Put in
//! the document, as `mutable="guarded"` fields are today, it is committed by the backup, read
//! by the bot, and fought over by two browser tabs that disagree about which day it is.
//!
//! So it lives here instead: a value per address, per document, per viewer. The document still
//! declares the field and its authored value, and a press still sets it - the value simply goes
//! into this overlay rather than into the text, and the overlay is applied when the document is
//! worked out for that viewer. Nothing is written, which means no undo point either: taking back
//! a write should not take back having looked at yesterday.
//!
//! A viewer with no session is one overlay shared - the bot, a script, anything that is not a
//! page. That is deliberate rather than a fallback: something with no view has no view state,
//! and what it writes to a guarded field should evaporate rather than persist.

use crate::types::OverseerValue;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// How long a viewer's overlay outlives its last use.
///
/// Long enough that a tab left open over lunch is still looking at the day it was, short enough
/// that a month of visits does not accumulate. Nothing is lost when one expires: the document
/// goes back to what it says, which is where a freshly opened page starts anyway.
const KEPT_FOR: Duration = Duration::from_secs(60 * 60 * 6);

struct Overlay {
    values: HashMap<String, OverseerValue>,
    used: Instant,
}

fn store() -> &'static Mutex<HashMap<(String, String), Overlay>> {
    static STORE: OnceLock<Mutex<HashMap<(String, String), Overlay>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn with<T>(body: impl FnOnce(&mut HashMap<(String, String), Overlay>) -> T) -> T {
    let mut held = store().lock().unwrap_or_else(|e| e.into_inner());
    held.retain(|_, overlay| overlay.used.elapsed() < KEPT_FOR);
    body(&mut held)
}

/// Remember what this viewer is looking at.
pub fn set(session: &str, document: &str, address: &str, value: OverseerValue) {
    with(|held| {
        let overlay = held
            .entry((session.to_string(), document.to_string()))
            .or_insert_with(|| Overlay { values: HashMap::new(), used: Instant::now() });
        overlay.values.insert(address.to_string(), value);
        overlay.used = Instant::now();
    });
}

/// What this viewer is looking at, to be applied while the document is worked out.
pub fn overlay(session: &str, document: &str) -> HashMap<String, OverseerValue> {
    with(|held| {
        match held.get_mut(&(session.to_string(), document.to_string())) {
            Some(overlay) => {
                overlay.used = Instant::now();
                overlay.values.clone()
            }
            None => HashMap::new(),
        }
    })
}

/// Forget what a viewer was looking at - a page closed, or a document renamed out from under one.
pub fn forget(session: &str, document: &str) {
    with(|held| {
        held.remove(&(session.to_string(), document.to_string()));
    });
}

/// Forget everything. For tests, and for a server that wants to start from nothing.
pub fn forget_all() {
    with(|held| held.clear());
}
