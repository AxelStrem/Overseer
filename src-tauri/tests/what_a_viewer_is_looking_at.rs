//! A field the document marks as the viewer's never reaches the file.
//!
//! Which day the food tracker is showing is real while somebody is looking and is not a fact
//! about anybody. Written into the document, as it was, it is committed by the backup, read by
//! the bot as though it meant something, and fought over by two pages that disagree about which
//! day it is. So it is kept apart: the document declares the field and its authored value, a
//! press still sets it, and the value goes into an overlay applied while the document is worked
//! out for that viewer.
//!
//! Three things have to hold, and each was wrong at some point while this was written:
//!
//!   - The press has to *take*. A second press on "previous day" must start from the day this
//!     viewer is on, or it returns to the same answer forever.
//!   - The file must not move, which also means no undo point and nothing for the backup.
//!   - A real write in the same press must still land. A button that logs a meal *and* moves the
//!     day has to leave the meal behind and take the day away, and the field it leaves behind
//!     must still say what the document authored - not the worked-out value, and not nothing.

use overseer::server::DocumentRoot;
use overseer::types::OverseerValue;

const DOCUMENT: &str = "\
tab day (label=\"Day\", mutable=true) {
    timestamp showing (mutable=\"guarded\") = $(today())
    int recorded = 0

    button back (label=\"back\") {
        on click {
            set (path=\"/day/showing\") = $(date_add_days(../showing, -1))
        }
    }

    button both (label=\"both\") {
        on click {
            set (path=\"/day/showing\") = $(date_add_days(../showing, -1))
            inc (path=\"/day/recorded\", by=1)
        }
    }
}
";

/// The overlay is keyed by viewer and document, and every test here uses `day.os` - so two
/// running at once share a key and wipe each other. Serialised rather than renamed, because the
/// document being the same is part of what these are about.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

fn a_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_view_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("day.os"), DOCUMENT).expect("write");
    overseer::viewstate::forget_all();
    root
}

fn press(service: &DocumentRoot, button: &str) {
    service.run_event("day.os", &format!("day/{}", button), "click").expect("press");
}

fn showing(service: &DocumentRoot) -> String {
    let nodes = service.open("day.os").expect("open");
    let node = overseer::addressing::find(&nodes, "day/showing").expect("no showing field");
    let held = node
        .parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
        .expect("no value");
    match held {
        OverseerValue::Date(d) => d.clone(),
        OverseerValue::Timestamp(t) => t.clone(),
        other => format!("{:?}", other),
    }
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("day.os")).expect("read")
}

#[test]
fn a_press_moves_what_the_viewer_sees() {
    serialised(|| {
        let root = a_root("sees");
        let service = DocumentRoot::new(&root).expect("open the root");
        let before = showing(&service);
        press(&service, "back");
        assert_ne!(showing(&service), before, "the press did not move the day");
    });
}

#[test]
fn pressing_again_goes_further_rather_than_back_to_the_same_answer() {
    serialised(|| {
        // The one that decides whether the overlay is real. Without applying it before the action
        // runs, every press works out `today()` again and lands on the same day forever.
        let root = a_root("further");
        let service = DocumentRoot::new(&root).expect("open the root");
        press(&service, "back");
        let one = showing(&service);
        press(&service, "back");
        let two = showing(&service);
        press(&service, "back");
        let three = showing(&service);
        assert_ne!(one, two, "the second press landed where the first did");
        assert_ne!(two, three, "the third press landed where the second did");
    });
}

#[test]
fn the_file_never_learns_which_day_is_being_looked_at() {
    serialised(|| {
        let root = a_root("file");
        let service = DocumentRoot::new(&root).expect("open the root");
        let before = on_disk(&root);
        press(&service, "back");
        press(&service, "back");
        assert_eq!(on_disk(&root), before, "the viewer's day reached the file");
    });
}

#[test]
fn looking_somewhere_makes_nothing_to_take_back() {
    serialised(|| {
        // Undo is for writes. Taking one back should not take back having looked at yesterday, and
        // the cheapest way to promise that is for looking to make no undo point at all.
        let root = a_root("undo");
        let service = DocumentRoot::new(&root).expect("open the root");
        press(&service, "back");
        press(&service, "back");
        assert_eq!(
            overseer::undo::depth(&root, "day.os"),
            0,
            "looking at a day left something to undo"
        );
    });
}

#[test]
fn a_real_write_in_the_same_press_still_lands() {
    serialised(|| {
        // A press that moves the day *and* records something. The record has to be written and the
        // day must not be.
        let root = a_root("both");
        let service = DocumentRoot::new(&root).expect("open the root");
        let looking = showing(&service);
        press(&service, "both");

        let written = on_disk(&root);
        assert!(written.contains("int recorded = 1"), "the real write did not land:\n{}", written);
        assert_ne!(showing(&service), looking, "the day did not move for the viewer");
    });
}

#[test]
fn what_the_file_keeps_for_a_viewers_field_is_what_the_document_authored() {
    serialised(|| {
        // The subtle half of the case above. Taking the value out must leave the formula the
        // document wrote - not the day it came to, and not an empty field, which would make the
        // next press fail for want of anything to subtract from.
        let root = a_root("authored");
        let service = DocumentRoot::new(&root).expect("open the root");
        press(&service, "both");

        let written = on_disk(&root);
        assert!(
            written.contains("= $(today())"),
            "the authored formula did not survive:\n{}",
            written
        );

        // And the proof that it survived intact: it can still be pressed.
        press(&service, "back");
        press(&service, "back");
    });
}

#[test]
fn two_viewers_are_looking_at_their_own_days() {
    serialised(|| {
        // Kept per viewer, which the document could not do: with the value in the file, two pages
        // share one day and each press moves it under the other.
        let root = a_root("two");
        overseer::viewstate::set(
            "alice",
            "day.os",
            "day/showing",
            OverseerValue::Date("2026-01-01".into()),
        );
        overseer::viewstate::set(
            "bob",
            "day.os",
            "day/showing",
            OverseerValue::Date("2026-06-15".into()),
        );
        let service = DocumentRoot::new(&root).expect("open the root");

        let day_for = |who: &str| {
            let nodes = service.open_for(who, "day.os").expect("open");
            let node = overseer::addressing::find(&nodes, "day/showing").expect("no field");
            // The worked-out value, as everything else in the document sees it: the overlay is
            // applied as an edit, and an edit's answer is the computed one.
            match node
                .parameters
                .get("_computed_value")
                .or_else(|| node.parameters.get("value"))
            {
                Some(OverseerValue::Date(d)) => d.clone(),
                Some(OverseerValue::Timestamp(t)) => t.clone(),
                other => format!("{:?}", other),
            }
        };

        assert_eq!(day_for("alice"), "2026-01-01");
        assert_eq!(day_for("bob"), "2026-06-15");
    });
}
