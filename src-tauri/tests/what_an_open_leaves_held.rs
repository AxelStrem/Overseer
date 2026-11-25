//! What an open of a real document leaves held, and by whom: a measurement, not a check.
//!
//! The memory line names the caches it knows of, from their own estimates. This opens a document
//! the way the server does, under the server's counting allocator, then lets go of each holder in
//! turn and says how many bytes really went with it - so the part of the heap no figure names can
//! be put down to something.
//!
//! Ignored, because it needs real documents and prints rather than asserts. Run it on a copy:
//!
//! ```text
//! OVERSEER_LEFT_ROOT=C:/copy/ap31 OVERSEER_LEFT_DOC=body_weight.os \
//!     cargo test --release --test what_an_open_leaves_held -- --ignored --nocapture
//! ```

use overseer::heap::{self, Counting};
use std::time::Duration;

#[global_allocator]
static ALLOCATOR: Counting<std::alloc::System> = Counting(std::alloc::System);

fn mb(bytes: isize) -> String {
    format!("{:>7.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

fn held() -> isize {
    heap::flush_this_thread();
    heap::held() as isize
}

/// Run `work` and say how far it brought the heap down.
fn step(name: &str, work: impl FnOnce()) -> isize {
    let before = held();
    work();
    let freed = before - held();
    println!("  {:<46} {}", name, mb(freed));
    freed
}

fn named() -> Vec<(&'static str, usize)> {
    vec![
        ("documents", overseer::document_cache::held().1),
        ("superseded", overseer::document_cache::superseded_held().1),
        ("parts", overseer::parts::read_parts_held().1),
        ("mounted", overseer::actions::mounted_held().1),
        ("bases", overseer::bases::parsed_held().1),
        ("frames", overseer::formula_evaluator::FormulaEvaluator::frames_held().1),
        ("handed out", overseer::app_api::handed_out_held().1),
        ("snapshots", overseer::source_registry::SourceRegistry::held().1),
    ]
}

#[test]
#[ignore]
fn what_an_open_leaves_held() {
    let root = std::env::var("OVERSEER_LEFT_ROOT").expect("OVERSEER_LEFT_ROOT names a copy of the documents");
    let doc = std::env::var("OVERSEER_LEFT_DOC").unwrap_or_else(|_| "body_weight.os".into());
    // What leaves memory would otherwise be written to disk and read back by the next run.
    overseer::document_disk::set_override(Some(None));
    let documents = overseer::server::DocumentRoot::new(&root).expect("a folder of documents");
    std::env::set_current_dir(&root).expect("the documents' folder");

    let start = held();
    for round in 1..=2 {
        let before = held();
        let worker = {
            let documents = documents.clone();
            let doc = doc.clone();
            std::thread::spawn(move || {
                let nodes = documents.open(&doc).expect("opens");
                let answer = serde_json::to_vec(&overseer::types::ForThePage(nodes)).expect("serialises");
                drop(answer);
                heap::flush_this_thread();
                heap::held() as isize
            })
        };
        let on_the_thread = worker.join().expect("the open ran");
        println!("open {} of {}:", round, doc);
        println!("  {:<46} {}", "held after it, its thread still up", mb(on_the_thread - before));
        println!("  {:<46} {}", "held after it, its thread gone", mb(held() - before));
    }

    let after = held() - start;
    let names = named();
    let named_total: usize = names.iter().map(|(_, b)| b).sum();
    println!("held since start: {}", mb(after));
    for (name, bytes) in &names {
        println!("  named {:<40} {}", name, mb(*bytes as isize));
    }
    println!("  {:<46} {}", "named in all", mb(named_total as isize));
    println!("  {:<46} {}", "named by nothing", mb(after - named_total as isize));

    println!("let go of, one at a time:");
    let zero = Duration::ZERO;
    let mut freed = 0;
    freed += step("documents and superseded", || {
        overseer::document_cache::let_go_idle(zero);
    });
    freed += step("parts", || {
        overseer::parts::let_go_of_idle_parts(zero);
    });
    freed += step("mounted", || {
        overseer::actions::let_go_of_idle_mounts(zero);
    });
    freed += step("bases", || {
        overseer::bases::let_go_of_idle_bases(zero);
    });
    freed += step("frames, once their mounts are gone", || {
        overseer::formula_evaluator::FormulaEvaluator::let_go_of_stale_frames(true);
    });
    freed += step("handed out, baseline, dependencies", || {
        overseer::app_api::forget_baseline();
        overseer::app_api::forget_dependencies();
    });
    freed += step("part summaries", overseer::partsums::forget_in_memory);
    freed += step("view state", overseer::viewstate::forget_all);
    freed += step("snapshots", overseer::source_registry::SourceRegistry::forget_all_unused);
    println!("  {:<46} {}", "freed in all", mb(freed));
    println!("  {:<46} {}", "still held", mb(held() - start));
}
