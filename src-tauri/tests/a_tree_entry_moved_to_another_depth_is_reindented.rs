//! An entry dragged to another depth of a tree is written at the depth it lands at: every line
//! of it, its own nested lists included, moves in or out by as much as its first line did.

use overseer::server::DocumentRoot;
use serde_json::json;

const DOCUMENT: &str = r#"div (hidden=true) {
    div Place {
        string name = ""
        list Places (entry=<Place>, key="name") { }
    }
}

tab t (mutable=true) {
    list Places (entry=<Place>, key="name", tree="Places", movable="anywhere") {
        - {
            - name = "Food"
            - path = "food.os"
        }
        - {
            - name = "Projects"
            list Places {
                - {
                    - name = "Spores"
                    list Places {
                        - {
                            - name = "Spore A"
                        }
                    }
                }
            }
        }
    }
}
"#;

const FOOD_IN_PROJECTS: &str = r#"div (hidden=true) {
    div Place {
        string name = ""
        list Places (entry=<Place>, key="name") { }
    }
}

tab t (mutable=true) {
    list Places (entry=<Place>, key="name", tree="Places", movable="anywhere") {
        - {
            - name = "Projects"
            list Places {
                - {
                    - name = "Food"
                    - path = "food.os"
                }
                - {
                    - name = "Spores"
                    list Places {
                        - {
                            - name = "Spore A"
                        }
                    }
                }
            }
        }
    }
}
"#;

const SPORES_AT_THE_TOP: &str = r#"div (hidden=true) {
    div Place {
        string name = ""
        list Places (entry=<Place>, key="name") { }
    }
}

tab t (mutable=true) {
    list Places (entry=<Place>, key="name", tree="Places", movable="anywhere") {
        - {
            - name = "Food"
            - path = "food.os"
        }
        - {
            - name = "Projects"
            list Places {
            }
        }
        - {
            - name = "Spores"
            list Places {
                - {
                    - name = "Spore A"
                }
            }
        }
    }
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

const A_STRING_OVER_TWO_LINES: &str = "- path = \"food.os\n        and more\"";

fn a_root(tag: &str) -> std::path::PathBuf {
    a_root_with(tag, DOCUMENT)
}

fn a_root_with(tag: &str, document: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_tree_reindent_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("d.os"), document).expect("write");
    overseer::viewstate::forget_all();
    root
}

fn move_to(service: &DocumentRoot, entry: &str, to_list: &str, before: Option<&str>) {
    let nodes = service.open("d.os").expect("open");
    let names = |address: &str| {
        overseer::addressing::name_path(&nodes, address)
            .unwrap_or_else(|| panic!("no such address: {}", address))
    };
    let args = json!({
        "entry_path": names(entry),
        "to_list_path": names(to_list),
        "before": before.map(names),
    });
    service
        .command_for("alice", Some("d.os"), "move_overseer_entry", &args)
        .expect("the move");
}

#[test]
fn an_entry_moved_one_level_in_is_indented_one_level_more() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("in");
    let service = DocumentRoot::new(&root).expect("open the root");
    move_to(&service, "t/Places/[Food]", "t/Places/[Projects]/Places", Some("t/Places/[Projects]/Places/[Spores]"));
    assert_eq!(std::fs::read_to_string(root.join("d.os")).unwrap(), FOOD_IN_PROJECTS);
}

#[test]
fn an_entry_moved_one_level_out_is_indented_one_level_less() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("out");
    let service = DocumentRoot::new(&root).expect("open the root");
    move_to(&service, "t/Places/[Projects]/Places/[Spores]", "t/Places", None);
    assert_eq!(std::fs::read_to_string(root.join("d.os")).unwrap(), SPORES_AT_THE_TOP);
}

#[test]
fn a_string_over_several_lines_keeps_its_value_when_its_entry_moves() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root_with("string", &DOCUMENT.replace("- path = \"food.os\"", A_STRING_OVER_TWO_LINES));
    let service = DocumentRoot::new(&root).expect("open the root");
    move_to(&service, "t/Places/[Food]", "t/Places/[Projects]/Places", Some("t/Places/[Projects]/Places/[Spores]"));
    let expected = FOOD_IN_PROJECTS.replace(
        "                    - path = \"food.os\"",
        &format!("                    {}", A_STRING_OVER_TWO_LINES),
    );
    assert_eq!(std::fs::read_to_string(root.join("d.os")).unwrap(), expected);
}

const AN_EMPTY_LIST_DEEPER: &str = r#"div (hidden=true) {
    div Meal {
        string name = ""
    }
}

tab t (mutable=true) {
    list Dishes (entry=<Meal>, key="name", movable=true) {
        - {
            - name = "Pasta"
        }
        - {
            - name = "Soup"
        }
    }
    div (layout="horizontal") {
        list Tuesday (entry=<Meal>, key="name", movable=true) { }
    }
}
"#;

const PASTA_ON_TUESDAY: &str = r#"div (hidden=true) {
    div Meal {
        string name = ""
    }
}

tab t (mutable=true) {
    list Dishes (entry=<Meal>, key="name", movable=true) {
        - {
            - name = "Soup"
        }
    }
    div (layout="horizontal") {
        list Tuesday (entry=<Meal>, key="name", movable=true) {
            - {
                - name = "Pasta"
            }
        }
    }
}
"#;

#[test]
fn an_entry_moved_into_an_empty_list_one_level_in_is_indented_one_level_more() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root_with("empty", AN_EMPTY_LIST_DEEPER);
    let service = DocumentRoot::new(&root).expect("open the root");
    move_to(&service, "t/Dishes/[Pasta]", "t/Tuesday", None);
    assert_eq!(std::fs::read_to_string(root.join("d.os")).unwrap(), PASTA_ON_TUESDAY);
}
