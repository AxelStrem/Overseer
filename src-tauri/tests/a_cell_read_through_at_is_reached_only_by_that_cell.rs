//! A formula that reads one cell of a grid through `at()` reads which cells the grid holds and
//! that cell - not the whole grid.
//!
//! `Board.at(x, y)/alive` used to record a read of the whole board, and a toggle anywhere on it
//! reached every formula that read a neighbour: each step of a game of life worked out every
//! cell's neighbour count again, where only the counts beside a toggled cell can change.

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

const BOARD: &str = r#"
tab t {
    div (hidden=true) {
        div Cell {
            bool alive = false
            int right = $(Board.at((column() + 1) % 3, row())/alive ? 1 : 0)
        }
    }

    grid Board (entry=<Cell>, columns=3, rows=3) { }
    grid Steps (entry=int, columns=3, rows=1) { }

    int centre = $(Board.at(1, 1)/alive ? 1 : 0)
    int second = $(Steps.at(1, 0))
}
"#;

#[test]
fn a_toggle_reaches_what_read_that_cell_and_nothing_else_on_the_grid() {
    let graph = graph_of(BOARD);
    let reached = reached(&graph, "t/Board/C1/alive");
    assert!(reached.contains(&"t/Board/B1/right".to_string()), "{:?}", reached);
    for value in ["t/Board/A1/right", "t/Board/C1/right", "t/Board/B2/right", "t/centre", "t/second"] {
        assert!(!reached.contains(&value.to_string()), "{} reached: {:?}", value, reached);
    }

    let reached = self::reached(&graph, "t/Board/B2/alive");
    for value in ["t/centre", "t/Board/A2/right"] {
        assert!(reached.contains(&value.to_string()), "{} not reached: {:?}", value, reached);
    }
}

#[test]
fn what_a_neighbour_worked_out_does_not_carry_a_toggle_across_the_grid() {
    // `A1/right` reads `B1/alive`, and `C1/right` reads `A1/alive`: a read of the cell `A1`
    // whole would be reached by `A1/right` changing, and from cell to cell, by the whole board.
    let graph = graph_of(BOARD);
    let reached = reached(&graph, "t/Board/B1/alive");
    assert!(reached.contains(&"t/Board/A1/right".to_string()), "{:?}", reached);
    assert!(!reached.contains(&"t/Board/C1/right".to_string()), "{:?}", reached);
}

#[test]
fn a_cell_read_as_a_value_is_reached_by_that_cell_alone() {
    let graph = graph_of(BOARD);
    assert!(reached(&graph, "t/Steps/B1").contains(&"t/second".to_string()));
    assert!(!reached(&graph, "t/Steps/C1").contains(&"t/second".to_string()));
}

#[test]
fn a_grid_made_over_again_reaches_every_cell_read_through_at() {
    // A grid made over names the grid, as a list does when an entry is made or taken out.
    let graph = graph_of(BOARD);
    let reached = reached(&graph, "t/Board");
    for value in ["t/Board/A1/right", "t/Board/C3/right", "t/centre"] {
        assert!(reached.contains(&value.to_string()), "{} not reached: {:?}", value, reached);
    }
    assert!(self::reached(&graph, "t/Steps").contains(&"t/second".to_string()));
}
