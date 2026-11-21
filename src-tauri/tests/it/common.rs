//! What the tests share.

use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};

/// What the crate keeps for the whole process - the snapshots of every parse, the documents held
/// worked out, the view state, the folder of the current document, the overrides of its caches -
/// is one for every module, as they run in one process.
///
/// A test that resets or sets any of it, or relies on nobody else touching it while it runs, holds
/// it on its own: `serial`. Every other test shares it: `shared`, so that one that resets what the
/// rest rely on - `SourceRegistry::reset` takes the snapshots a save writes from - waits until
/// none is running.
static PROCESS: RwLock<()> = RwLock::new(());

/// Holds the process alone until the guard is dropped.
pub fn serial() -> RwLockWriteGuard<'static, ()> {
    PROCESS.write()
}

/// Holds the process beside the other tests sharing it, until the guard is dropped.
///
/// Taken even while a test waits to hold it alone. A lock that makes the shared wait behind it
/// leaves every thread but one idle for as long as the slowest shared test runs, and the run takes
/// twice as long; the tests waiting for it alone get it once the shared ones are through.
pub fn shared() -> RwLockReadGuard<'static, ()> {
    PROCESS.read_recursive()
}

/// Runs `body` holding the process alone.
pub fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let _guard = serial();
    body()
}
