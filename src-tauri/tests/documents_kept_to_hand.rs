//! The documents kept to hand: the list the drawer shows, held in a document of its own.
//!
//! One per place the page runs - the desktop's in its settings folder, naming files anywhere on
//! the computer; the server's in its documents folder, naming documents under it - and both read
//! and written the same way, through the same instructions as any other change.

use overseer::menu;
use overseer::server::DocumentRoot;
use serde_json::json;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

fn a_folder(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_menu_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the folder");
    root
}

fn paths(places: &[menu::Place]) -> Vec<String> {
    places.iter().map(|p| p.path.clone()).collect()
}

#[test]
fn a_list_nobody_has_made_yet_is_made_empty() {
    serialised(|| {
        let file = a_folder("fresh").join("settings").join(menu::FILE);
        assert_eq!(menu::places(&file).expect("read"), vec![]);
        assert!(file.exists(), "the list was not made");
    });
}

#[test]
fn a_document_is_kept_once_in_the_order_it_was_added() {
    serialised(|| {
        let file = a_folder("keep").join(menu::FILE);
        menu::keep(&file, "tasks.os", "Tasks").unwrap();
        menu::keep(&file, "projects/overseer.os", "").unwrap();
        let places = menu::keep(&file, "tasks.os", "Chores").unwrap();
        assert_eq!(paths(&places), vec!["tasks.os", "projects/overseer.os"]);
        assert_eq!(places[0].name, "Chores", "keeping it again with a name renames it");
        assert_eq!(places[1].name, "", "a name nobody gave is left for the drawer to make up");
    });
}

#[test]
fn a_document_is_taken_off_and_nothing_else_is() {
    serialised(|| {
        let file = a_folder("drop").join(menu::FILE);
        menu::keep(&file, "a.os", "A").unwrap();
        menu::keep(&file, "b.os", "B").unwrap();
        menu::keep(&file, "c.os", "C").unwrap();
        assert_eq!(paths(&menu::drop(&file, "b.os").unwrap()), vec!["a.os", "c.os"]);
        assert_eq!(paths(&menu::drop(&file, "not there.os").unwrap()), vec!["a.os", "c.os"]);
    });
}

#[test]
fn a_file_anywhere_on_this_computer_reads_back_as_it_was_given() {
    // The desktop's entries are Windows paths, backslashes and a drive colon and all, written
    // into a quoted string and read out of it again.
    serialised(|| {
        let file = a_folder("windows").join(menu::FILE);
        let here = r"E:\Source\Repos\Secrebot-Docs\personal-stats\documents\projects\black spores.os";
        let there = "/home/someone/notes/diary.os";
        menu::keep(&file, here, "Black Spores").unwrap();
        let places = menu::keep(&file, there, "").unwrap();
        assert_eq!(paths(&places), vec![here.to_string(), there.to_string()]);
        // And from the file alone, as the next run of the app finds it.
        assert_eq!(paths(&menu::places(&file).unwrap()), vec![here.to_string(), there.to_string()]);
        assert_eq!(paths(&menu::drop(&file, here).unwrap()), vec![there.to_string()]);
    });
}

#[test]
fn keeping_one_is_one_step_to_take_back() {
    serialised(|| {
        let folder = a_folder("undo");
        let file = folder.join(menu::FILE);
        menu::places(&file).unwrap();
        menu::keep(&file, "tasks.os", "Tasks").unwrap();
        let before = overseer::undo::take(&folder, menu::FILE).expect("a step to take back");
        assert!(!before.contains("tasks.os"), "{}", before);
    });
}

#[test]
fn the_server_keeps_its_own_list_in_its_own_folder() {
    // Through the door the page uses, with the page's message. Which document the page happens to
    // be showing has nothing to do with it.
    serialised(|| {
        let root = a_folder("server");
        std::fs::write(root.join("tasks.os"), "tab tasks {\n}\n").unwrap();
        let service = DocumentRoot::new(&root).expect("root");
        let ask = |cmd: &str, args: serde_json::Value| {
            service.command_for("alice", Some("tasks.os"), cmd, &args).expect(cmd)
        };
        assert_eq!(ask("menu_places", json!({})), json!([]));
        ask("menu_keep", json!({ "place": "tasks.os", "name": "Tasks" }));
        let kept = ask("menu_keep", json!({ "place": "projects/overseer.os" }));
        assert_eq!(kept, json!([
            { "path": "tasks.os", "name": "Tasks" },
            { "path": "projects/overseer.os", "name": "" },
        ]));
        assert_eq!(ask("menu_drop", json!({ "place": "tasks.os" })), json!([{ "path": "projects/overseer.os", "name": "" }]));
        assert_eq!(ask("menu_location", json!({})), json!(menu::FILE));
        assert!(root.join(menu::FILE).exists());
        assert!(service.list().contains(&menu::FILE.to_string()), "the list is a document like any other");
    });
}

#[test]
fn the_last_one_is_taken_off_too() {
    // Taking the only entry off answered Ok and left the file as it was: the writer replayed the
    // list's block as it was first read, entry and all, because the list now had no children.
    serialised(|| {
        let file = a_folder("last").join(menu::FILE);
        menu::keep(&file, "tasks.os", "Tasks").unwrap();
        assert_eq!(menu::drop(&file, "tasks.os").unwrap(), vec![]);
        assert!(!std::fs::read_to_string(&file).unwrap().contains("tasks.os"));
    });
}
