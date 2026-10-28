//! `column()` and `row()` inside a grid's cell give its place, from 0, as `at()` takes it: every
//! cell is made from one template, and this is how one of them tells itself from the others and
//! reaches its neighbours.

use overseer::app_api;
use overseer::types::*;

const DOC: &str = r#"tab t (label="T") {
    div (hidden=true) {
        div Cell {
            int c = $(column())
            int r = $(row())
            bool marked = false
            div (layout="horizontal") {
                int deeper = $(column() * 10 + row())
            }
            int around = $(Field.cells().filter(|x| x/marked && x/c >= column() - 1 && x/c <= column() + 1 && x/r >= row() - 1 && x/r <= row() + 1 && (x/c != column() || x/r != row())).count())
        }
    }

    grid Field (entry=<Cell>, columns=9, rows=9) {
        - A1 {
            - marked = true
        }
    }

    int outside = $(column())
    int with_argument = $(Field.at(0, 0)/c + column(1))
}
"#;

/// Every cell marked, so the count around a cell is the number of its neighbours.
fn all_marked() -> String {
    DOC.replace("bool marked = false", "bool marked = true")
}

fn value_at(nodes: &[OverseerNode], address: &str) -> OverseerValue {
    let node = overseer::addressing::find(nodes, address).unwrap_or_else(|| panic!("no {address}"));
    node.parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
        .cloned()
        .unwrap_or_else(|| panic!("{address} has no value"))
}

fn int(v: OverseerValue) -> i64 {
    match v {
        OverseerValue::Integer(i) => i,
        OverseerValue::Float(f) if f.fract() == 0.0 => f as i64,
        other => panic!("not a whole number: {other:?}"),
    }
}

fn name(column: usize, row: usize) -> String {
    format!("{}{}", (b'A' + column as u8) as char, row + 1)
}

#[test]
fn every_cell_gets_its_own_place() {
    let nodes = app_api::load_document(DOC.to_string()).expect("load");
    for row in 0..9 {
        for column in 0..9 {
            let cell = format!("t/Field/{}", name(column, row));
            assert_eq!(int(value_at(&nodes, &format!("{cell}/c"))), column as i64, "{cell} column");
            assert_eq!(int(value_at(&nodes, &format!("{cell}/r"))), row as i64, "{cell} row");
            assert_eq!(
                int(value_at(&nodes, &format!("{cell}/deeper"))),
                (column * 10 + row) as i64,
                "{cell}, in a row inside the cell"
            );
        }
    }
}

#[test]
fn a_cell_counts_its_neighbours_at_corners_edges_and_in_the_middle() {
    let nodes = app_api::load_document(all_marked()).expect("load");
    for (cell, neighbours) in [("A1", 3), ("I9", 3), ("A9", 3), ("E1", 5), ("A5", 5), ("I5", 5), ("E5", 8), ("B2", 8)] {
        assert_eq!(int(value_at(&nodes, &format!("t/Field/{cell}/around"))), neighbours, "{cell}");
    }
}

#[test]
fn a_neighbour_count_sees_only_the_cells_around() {
    // Only A1 is marked: B1, A2 and B2 see it, C1 and C3 do not, and A1 does not count itself.
    let nodes = app_api::load_document(DOC.to_string()).expect("load");
    for (cell, count) in [("B1", 1), ("A2", 1), ("B2", 1), ("C1", 0), ("C3", 0), ("A1", 0)] {
        assert_eq!(int(value_at(&nodes, &format!("t/Field/{cell}/around"))), count, "{cell}");
    }
}

#[test]
fn outside_a_cell_or_with_an_argument_it_is_an_error() {
    let nodes = app_api::load_document(DOC.to_string()).expect("load");
    for field in ["t/outside", "t/with_argument"] {
        let text = format!("{:?}", value_at(&nodes, field));
        assert!(text.to_lowercase().contains("error"), "{field} gave {text}");
    }
}
