//! The heap the server counts says how far each request made it grow - see `overseer::heap`.

use overseer::heap::{self, Counting};
use std::hint::black_box;
use std::sync::Mutex;

#[global_allocator]
static ALLOCATOR: Counting<std::alloc::System> = Counting(std::alloc::System);

const MB: usize = 1024 * 1024;

/// The heap is one for the whole process, so the tests here take turns.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn turn() -> std::sync::MutexGuard<'static, ()> {
    ONE_AT_A_TIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn build_and_drop(bytes: usize) {
    let value = black_box(vec![7u8; bytes]);
    drop(value);
}

/// Near enough: the test harness allocates a little on its own threads meanwhile.
fn near(bytes: usize, expected: usize) -> bool {
    bytes.abs_diff(expected) < 4 * MB
}

#[test]
fn a_request_that_builds_a_large_value_grew_by_about_its_size() {
    let _turn = turn();
    let watch = heap::begin("/doc/large");
    build_and_drop(100 * MB);
    let grown = watch.end();
    assert!(near(grown.grew(), 100 * MB), "grew {} MB", grown.grew() / MB);
    assert!(grown.alongside.is_empty());
    assert!(heap::installed());
}

#[test]
fn a_request_that_allocates_nothing_much_grew_by_nothing_much() {
    let _turn = turn();
    build_and_drop(50 * MB);
    let watch = heap::begin("/doc/small");
    let grown = watch.end();
    assert!(near(grown.grew(), 0), "grew {} MB", grown.grew() / MB);
}

#[test]
fn overlapping_requests_each_count_what_was_held_while_they_ran_and_name_each_other() {
    let _turn = turn();
    let first = heap::begin("/doc/first");
    build_and_drop(60 * MB);
    let second = heap::begin("/v1/event");
    build_and_drop(30 * MB);
    let first = first.end();
    let third = heap::begin("/doc/third");
    let second = second.end();
    let third = third.end();

    // The first saw its own sixty and the second's thirty; the second only its thirty, the
    // sixty having gone before it began; the third began after both were dropped.
    assert!(near(first.grew(), 60 * MB), "first grew {} MB", first.grew() / MB);
    assert!(near(second.grew(), 30 * MB), "second grew {} MB", second.grew() / MB);
    assert!(near(third.grew(), 0), "third grew {} MB", third.grew() / MB);

    assert_eq!(first.alongside, vec!["/v1/event"]);
    assert_eq!(second.alongside, vec!["/doc/first", "/doc/third"]);
    assert_eq!(third.alongside, vec!["/v1/event"]);
}

#[test]
fn the_periodic_line_starts_the_next_peak_since_the_last_line() {
    let _turn = turn();
    build_and_drop(80 * MB);
    let before = heap::heap(true).unwrap();
    assert!(before.peak_since_line >= before.held + 80 * MB - 4 * MB);
    let after = heap::heap(true).unwrap();
    assert!(near(after.peak_since_line, after.held), "{:?}", after);
    assert!(after.peak_since_start >= before.peak_since_line);
}

#[test]
fn a_request_dropped_without_ending_is_forgotten() {
    let _turn = turn();
    drop(heap::begin("/doc/abandoned"));
    let grown = heap::begin("/doc/next").end();
    assert!(grown.alongside.is_empty(), "{:?}", grown.alongside);
}
