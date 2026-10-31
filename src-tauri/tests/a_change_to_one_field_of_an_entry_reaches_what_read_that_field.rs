//! What reads a list through a lambda reads which entries it holds and, of each entry, what the
//! lambda read - not everything in the list.
//!
//! `cells().filter(|x| x/mine).count()` used to record a read of the whole board, and a change to
//! any value in any cell reached it: flagging one cell of the minesweeper board worked out every
//! cell's mine and neighbour count again. The read of each entry's field is now recorded as a
//! read of that field of every entry, `Field/*/mine`, and the board itself as which cells it
//! holds, `Field#_entries`. What reads an entry any other way - its own value, a lookup by key,
//! a chain that hands the entries on - still reads the list whole.

use overseer::dependencies::{self, Graph};
use overseer::parser;
use overseer::resolver;

fn graph_of(source: &str) -> Graph {
    let (_rest, mut nodes) = parser::parse_document(source).expect("parse");
    dependencies::start_recording();
    resolver::resolve_document(&mut nodes);
    dependencies::take_recording()
}

fn reached(graph: &Graph, changed: &str) -> Vec<String> {
    graph.nodes_to_work_out_again(&[changed.to_string()])
}

const SHOP: &str = r#"
tab shop {
    div (hidden=true) {
        div Line {
            string item = ""
            float price = 0
            int qty = 1
            string note = ""
        }
    }

    list Lines (entry=<Line>, key="item") {
        - {
            - item = "bread"
            - price = 100
        }
        - {
            - item = "milk"
            - price = 50
        }
    }

    float total = $(Lines.map(|x| x/price * x/qty).sum())
    int dear = $(Lines.filter(|x| x/price > 60).count())
    int lines = $(Lines.count())
    float milk = $(Lines.find("milk")/price)
    float first = $(Lines.map(|x| x/price).first())
    float looked_up = $(Lines.filter(|x| x/item == "milk").map(|x| x/price).sum())
}
"#;

#[test]
fn a_change_to_a_field_no_lambda_read_reaches_nothing_that_ran_over_the_list() {
    let graph = graph_of(SHOP);
    let reached = reached(&graph, "shop/Lines/bread/note");
    for value in ["shop/total", "shop/dear", "shop/lines", "shop/looked_up", "shop/first"] {
        assert!(!reached.contains(&value.to_string()), "{} reached: {:?}", value, reached);
    }
}

#[test]
fn a_change_to_a_field_a_lambda_read_reaches_what_ran_over_the_list() {
    let graph = graph_of(SHOP);
    let reached = reached(&graph, "shop/Lines/bread/price");
    for value in ["shop/total", "shop/dear", "shop/first", "shop/looked_up"] {
        assert!(reached.contains(&value.to_string()), "{} not reached: {:?}", value, reached);
    }
    assert!(!reached.contains(&"shop/lines".to_string()), "{:?}", reached);
    let qty = self::reached(&graph, "shop/Lines/milk/qty");
    assert!(qty.contains(&"shop/total".to_string()), "{:?}", qty);
    assert!(!qty.contains(&"shop/dear".to_string()), "{:?}", qty);
}

#[test]
fn an_entry_made_or_taken_out_reaches_what_ran_over_the_list() {
    // An entry made or taken out names the list - see `app_api::follow_the_shape`.
    let graph = graph_of(SHOP);
    let reached = reached(&graph, "shop/Lines");
    for value in ["shop/total", "shop/dear", "shop/lines", "shop/milk", "shop/first", "shop/looked_up"] {
        assert!(reached.contains(&value.to_string()), "{} not reached: {:?}", value, reached);
    }
}

#[test]
fn a_lookup_by_key_still_reads_the_list_whole() {
    let graph = graph_of(SHOP);
    assert!(reached(&graph, "shop/Lines/bread/note").contains(&"shop/milk".to_string()));
}

#[test]
fn an_entry_read_as_a_value_is_read_through_its_fields() {
    let doc = r#"
tab t {
    list Numbers {
        int a = 1
        int b = 2
    }
    int total = $(Numbers.map(|x| x).sum())
}
"#;
    let graph = graph_of(doc);
    assert!(reached(&graph, "t/Numbers/b").contains(&"t/total".to_string()));
}

#[test]
fn a_list_inside_an_entry_is_reached_from_a_change_inside_it() {
    let doc = r#"
tab t {
    div (hidden=true) {
        div Part {
            float kcal = 0
        }
        div Meal {
            string name = ""
            list parts (entry=<Part>) {
            }
        }
    }
    list Meals (entry=<Meal>, key="name") {
        - {
            - name = "lunch"
            - parts {
                - {
                    - kcal = 300
                }
            }
        }
    }
    float total = $(Meals.map(|m| m/parts.map(|p| p/kcal).sum()).sum())
}
"#;
    let graph = graph_of(doc);
    let values = graph.values();
    let part = values
        .iter()
        .find_map(|v| v.strip_suffix("/kcal#_computed_value").filter(|p| p.starts_with("t/Meals/lunch/parts/")))
        .map(|p| format!("{}/kcal", p));
    // The part may hold a literal, and then is no value of its own: name it from the template.
    let part = part.unwrap_or_else(|| "t/Meals/lunch/parts/Part__1/kcal".to_string());
    assert!(
        reached(&graph, &part).contains(&"t/total".to_string()),
        "{} does not reach the total: {:?}",
        part,
        graph.reads("t/total#_computed_value")
    );
}

#[test]
fn flagging_a_cell_reaches_its_own_face_and_not_every_cell_on_the_board() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../examples/games/minesweeper.os"
    ))
    .expect("minesweeper");
    let graph = graph_of(&source);
    // The template's own `mine`, worked out outside the board, reads no cells; only the board's
    // cells are compared, in a fixed order rather than the graph's.
    let mut mines: Vec<String> = graph
        .values()
        .into_iter()
        .filter_map(|v| v.strip_suffix("/mine#_computed_value"))
        .filter(|v| v.starts_with("game/Field/"))
        .map(str::to_string)
        .collect();
    mines.sort();
    assert!(mines.len() > 1, "no cells found: {:?}", graph.values().iter().take(20).collect::<Vec<_>>());
    let cell = &mines[0];
    let other = &mines[1];

    let reached = reached(&graph, &format!("{}/flagged", cell));
    assert!(reached.contains(&format!("{}/face", cell)), "{:?}", reached);
    assert!(reached.iter().any(|v| v.ends_with("flags_left")), "{:?}", reached);
    for value in [format!("{}/mine", other), format!("{}/around", other), format!("{}/mine", cell)] {
        assert!(!reached.contains(&value), "{} reached by a flag: {} values", value, reached.len());
    }

    // Opening a cell is read by every cell's neighbour count only through its mine, which an
    // opened cell does not change.
    let opened = self::reached(&graph, &format!("{}/opened", cell));
    assert!(!opened.contains(&format!("{}/around", other)), "{:?}", opened.len());
    let roll = self::reached(&graph, &format!("{}/roll", cell));
    assert!(roll.contains(&format!("{}/mine", other)), "a roll does not reach the other mines");
}
