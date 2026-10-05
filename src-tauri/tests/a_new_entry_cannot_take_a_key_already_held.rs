//! A new entry in a keyed list is refused when its key is empty or one the list holds already.
//!
//! A list with `key=` is addressed by that field, and two entries holding the same value are two
//! entries at one address: every later remove, change or move by that key is refused, since which
//! one is meant is not clear. The project form checks before it appends, but only because it was
//! written to - the bot, another document's button, or a form that forgets did not. So whichever
//! way an entry arrives, by append, prepend, a copy from a form, the bot's append or a move from
//! another list, the same rule holds, and a refused press writes nothing.

use overseer::server::DocumentRoot;
use overseer::types::OverseerValue;

const DOCUMENT: &str = r#"tab t (label="T", mutable=true) {
    div (hidden=true) {
        div Row (layout="horizontal") {
            string id = ""
            int qty = 0
        }
        div Wrapped (layout="vertical") {
            div (layout="horizontal") {
                string id = ""
                int qty = 0
            }
        }
    }

    list Rows (entry=<Row>, key="id") {
        - {
            - id = "taken"
            - qty = 1
        }
    }

    list Loose (entry=<Row>) {
        - {
            - id = "taken"
            - qty = 1
        }
    }

    list Held (entry=<Wrapped>, key="id") {
        - {
            - id = "taken"
            - qty = 1
        }
    }

    list Elsewhere (entry=<Row>, key="id") {
        - {
            - id = "taken"
            - qty = 2
        }
        - {
            - id = "free"
            - qty = 3
        }
    }

    div Form (layout="horizontal") {
        textbox id = "taken"
        textbox qty = "4"
        button add (label="add") {
            on click {
                append (list="/t/Rows", from="..")
            }
        }
    }

    button append_taken (label="a") {
        on click {
            append (list="/t/Rows") {
                - id = "taken"
            }
        }
    }

    button append_empty (label="a") {
        on click {
            append (list="/t/Rows")
        }
    }

    button append_fresh (label="a") {
        on click {
            append (list="/t/Rows") {
                - id = "fresh"
            }
        }
    }

    button prepend_taken (label="p") {
        on click {
            prepend (list="/t/Rows") {
                - id = "taken"
            }
        }
    }

    button prepend_empty (label="p") {
        on click {
            prepend (list="/t/Rows") {
                - id = "  "
            }
        }
    }

    button append_unkeyed (label="a") {
        on click {
            append (list="/t/Loose") {
                - id = "taken"
            }
        }
    }

    button append_wrapped (label="a") {
        on click {
            append (list="/t/Held") {
                - id = "taken"
            }
        }
    }

    button append_wrapped_fresh (label="a") {
        on click {
            append (list="/t/Held") {
                - id = "fresh"
            }
        }
    }

    button move_taken (label="m") {
        on click {
            move (from="/t/Elsewhere", to="/t/Rows", keyField="id", keyValue="taken")
        }
    }

    button move_free (label="m") {
        on click {
            move (from="/t/Elsewhere", to="/t/Rows", keyField="id", keyValue="free")
        }
    }
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn a_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_key_held_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("d.os"), DOCUMENT).unwrap();
    root
}

fn press(button: &str) -> (Result<(), String>, String) {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root(button);
    overseer::app_api::forget_baseline();
    let service = DocumentRoot::new(&root).unwrap();
    let outcome = service.run_event("d.os", &format!("t/{}", button), "click").map(|_| ()).map_err(|e| format!("{:?}", e));
    (outcome, std::fs::read_to_string(root.join("d.os")).unwrap())
}

/// The bot's way in: an append with these fields, as the server takes it.
fn bot_appends(tag: &str, fields: &[(&str, OverseerValue)]) -> (Result<(), String>, String) {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root(tag);
    overseer::app_api::forget_baseline();
    let service = DocumentRoot::new(&root).unwrap();
    let fields = fields.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
    let outcome = service.append_at("d.os", "t/Rows", &fields).map(|_| ()).map_err(|e| format!("{:?}", e));
    (outcome, std::fs::read_to_string(root.join("d.os")).unwrap())
}

fn refused(button: &str) {
    let (outcome, after) = press(button);
    assert!(outcome.is_err(), "{} was let through:\n{}", button, after);
    assert_eq!(after, DOCUMENT, "the refused press {} wrote something", button);
}

#[test]
fn an_append_with_a_key_already_held_is_refused() {
    refused("append_taken");
}

#[test]
fn an_append_with_no_key_is_refused() {
    refused("append_empty");
}

#[test]
fn a_prepend_with_a_key_already_held_is_refused() {
    refused("prepend_taken");
}

#[test]
fn a_prepend_with_a_blank_key_is_refused() {
    refused("prepend_empty");
}

#[test]
fn a_form_copying_a_key_already_held_is_refused() {
    refused("Form/add");
}

#[test]
fn a_key_inside_the_row_that_lays_it_out_is_still_the_key() {
    refused("append_wrapped");
    let (outcome, after) = press("append_wrapped_fresh");
    assert!(outcome.is_ok(), "{:?}", outcome.err());
    assert!(after.contains("- id = \"fresh\""), "the entry was not written:\n{}", after);
}

#[test]
fn a_move_into_a_list_that_holds_the_key_is_refused() {
    refused("move_taken");
}

// Only that it is let through: a move between two lists does not reach the file at all yet, which
// is older than this check and filed as its own item (`movewrite`).
#[test]
fn a_move_with_a_key_the_list_lacks_is_not_refused() {
    let (outcome, _) = press("move_free");
    assert!(outcome.is_ok(), "{:?}", outcome.err());
}

#[test]
fn an_append_with_a_fresh_key_still_works() {
    let (outcome, after) = press("append_fresh");
    assert!(outcome.is_ok(), "{:?}", outcome.err());
    assert!(after.contains("- id = \"fresh\""), "the entry was not written:\n{}", after);
}

#[test]
fn a_list_without_a_key_takes_a_repeat() {
    let (outcome, after) = press("append_unkeyed");
    assert!(outcome.is_ok(), "{:?}", outcome.err());
    let loose = &after[after.find("list Loose").unwrap()..after.find("list Held").unwrap()];
    assert_eq!(loose.matches("- id = \"taken\"").count(), 2, "the repeat was not written:\n{}", after);
}

#[test]
fn the_bot_cannot_append_a_key_already_held() {
    let (outcome, after) = bot_appends("bot_taken", &[("id", OverseerValue::String("taken".into()))]);
    assert!(outcome.is_err(), "the bot appended a duplicate:\n{}", after);
    assert_eq!(after, DOCUMENT);
}

#[test]
fn the_bot_cannot_append_an_entry_with_no_key() {
    let (outcome, after) = bot_appends("bot_empty", &[("qty", OverseerValue::Integer(5))]);
    assert!(outcome.is_err(), "the bot appended an entry with no key:\n{}", after);
    assert_eq!(after, DOCUMENT);
}

#[test]
fn the_bot_can_append_a_fresh_key() {
    let (outcome, after) = bot_appends("bot_fresh", &[("id", OverseerValue::String("fresh".into()))]);
    assert!(outcome.is_ok(), "{:?}", outcome.err());
    assert!(after.contains("- id = \"fresh\""), "the entry was not written:\n{}", after);
}
