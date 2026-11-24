//! The server's own count of its heap: how much its allocations hold now, the most they have held,
//! and how much each request made that grow.
//!
//! The process's resident memory says what the server holds, but not when it rose or for whom:
//! the system reports only its peak since start. Counting allocations as they happen gives the
//! peak over any stretch of time - since the last memory line, or while one request ran - so a
//! spike on a host's graph can be put down to a request in the log.
//!
//! `Counting` wraps whatever allocator is in use and is installed by the server binary alone; in
//! the desktop app and in most tests nothing is counted, and `installed` says so.
//!
//! Each allocation adds to a count of its thread's own, which is passed on to the shared count
//! once it has run 64 KB either way: no lock and, most of the time, no atomic operation is taken on
//! the way, so it stays on in production. The bookkeeping of
//! requests takes a lock, but once as a request begins and once as it ends.

use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering::Relaxed};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Bytes the counted allocations hold now.
static HELD: AtomicUsize = AtomicUsize::new(0);
/// The most held since a request last began or ended - see `fold`.
static PEAK_WINDOW: AtomicUsize = AtomicUsize::new(0);
/// The most held since the periodic memory line last read it.
static PEAK_LINE: AtomicUsize = AtomicUsize::new(0);
/// The most held before the periodic line last read `PEAK_LINE`; the peak since start is the
/// larger of the two.
static PEAK_EVER: AtomicUsize = AtomicUsize::new(0);

/// An allocator that counts what the one it wraps hands out and takes back.
pub struct Counting<A>(pub A);

fn raise(peak: &AtomicUsize, now: usize) {
    if now > peak.load(Relaxed) {
        peak.fetch_max(now, Relaxed);
    }
}

/// How far a thread's own count may run before it is added to `HELD`. A locked addition on every
/// allocation made opening a large document several percent slower; batched, the shared count and
/// its peaks are touched once in many allocations, and are off by at most this much a thread.
const BATCH: isize = 64 * 1024;

thread_local! {
    // A plain cell with nothing to drop, so that reaching it from inside the allocator neither
    // allocates nor registers a destructor.
    static PENDING: std::cell::Cell<isize> = const { std::cell::Cell::new(0) };
}

fn add(bytes: isize) {
    if bytes >= 0 {
        let now = HELD.fetch_add(bytes as usize, Relaxed).wrapping_add(bytes as usize);
        raise(&PEAK_WINDOW, now);
        raise(&PEAK_LINE, now);
    } else {
        HELD.fetch_sub(bytes.unsigned_abs(), Relaxed);
    }
}

#[inline]
fn counted(bytes: isize) {
    let due = PENDING
        .try_with(|pending| {
            let now = pending.get() + bytes;
            if now.abs() < BATCH {
                pending.set(now);
                0
            } else {
                pending.set(0);
                now
            }
        })
        // While the thread is being torn down its cell is gone, and the change goes straight on.
        .unwrap_or(bytes);
    if due != 0 {
        add(due);
    }
}

/// Add what this thread has counted and not yet passed on. A thread that ends without it leaves
/// up to `BATCH` uncounted, so the server's runtime calls it as each of its threads stops, and
/// requests call it as they begin and end.
pub fn flush_this_thread() {
    let due = PENDING.try_with(|pending| pending.replace(0)).unwrap_or(0);
    if due != 0 {
        add(due);
    }
}

#[inline]
fn grown(by: usize) {
    counted(by as isize);
}

#[inline]
fn shrunk(by: usize) {
    counted(-(by as isize));
}

// Safety: every call is forwarded to the wrapped allocator unchanged; the counting beside it
// touches nothing but atomics, and allocates nothing.
unsafe impl<A: GlobalAlloc> GlobalAlloc for Counting<A> {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = self.0.alloc(layout);
        if !ptr.is_null() {
            grown(layout.size());
        }
        ptr
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = self.0.alloc_zeroed(layout);
        if !ptr.is_null() {
            grown(layout.size());
        }
        ptr
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.0.dealloc(ptr, layout);
        shrunk(layout.size());
    }

    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let moved = self.0.realloc(ptr, layout, new_size);
        if !moved.is_null() {
            if new_size >= layout.size() {
                grown(new_size - layout.size());
            } else {
                shrunk(layout.size() - new_size);
            }
        }
        moved
    }
}

/// Whether anything is being counted: false unless the binary installed `Counting`.
pub fn installed() -> bool {
    PEAK_LINE.load(Relaxed) > 0 || PEAK_EVER.load(Relaxed) > 0
}

/// Bytes the server's allocations hold now.
pub fn held() -> usize {
    HELD.load(Relaxed)
}

/// What the heap holds, and its peaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Heap {
    pub held: usize,
    pub peak_since_line: usize,
    pub peak_since_start: usize,
}

/// The heap now. `new_line` starts the next stretch for `peak_since_line`, and only the periodic
/// line passes it, so that every periodic line covers the time since the one before.
pub fn heap(new_line: bool) -> Option<Heap> {
    if !installed() {
        return None;
    }
    let held = held();
    let since_line = if new_line { PEAK_LINE.swap(held, Relaxed) } else { PEAK_LINE.load(Relaxed) };
    if new_line {
        PEAK_EVER.fetch_max(since_line, Relaxed);
    }
    let since_start = PEAK_EVER.load(Relaxed).max(since_line);
    Some(Heap { held, peak_since_line: since_line.max(held), peak_since_start: since_start.max(held) })
}

/// A request in flight.
struct InFlight {
    id: u64,
    path: String,
    held_at_start: usize,
    /// The most held at any moment since it began, as far as `fold` has brought it up to date.
    peak: usize,
    /// The paths of the other requests that ran at some time while it did.
    alongside: Vec<String>,
}

static IN_FLIGHT: Mutex<Vec<InFlight>> = Mutex::new(Vec::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

/// Bring every request in flight up to date with the peak since the last begin or end, and start
/// the next stretch from what is held now.
///
/// The allocator raises one peak for everyone, and requests overlap, so it cannot be reset for
/// one of them without losing another's. Folding it into each at every begin and end instead
/// gives each request the most held at any moment while it ran - whoever allocated it - and never
/// a peak from before it began.
fn fold(in_flight: &mut [InFlight]) {
    let peak = PEAK_WINDOW.swap(held(), Relaxed);
    for request in in_flight {
        request.peak = request.peak.max(peak);
    }
}

fn in_flight() -> std::sync::MutexGuard<'static, Vec<InFlight>> {
    IN_FLIGHT.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A request being watched, from `begin` until `end` or until it is dropped.
pub struct Watch {
    id: u64,
    started: Instant,
    ended: bool,
}

/// What a request did to the heap while it ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grown {
    pub path: String,
    pub took: Duration,
    pub held_at_start: usize,
    /// The most the server held at any moment while the request ran, whoever allocated it.
    pub peak: usize,
    /// The paths of the other requests that ran at some time while this one did.
    pub alongside: Vec<String>,
}

impl Grown {
    /// How far the heap rose above what it held when the request began.
    pub fn grew(&self) -> usize {
        self.peak.saturating_sub(self.held_at_start)
    }
}

/// Start watching a request, known in the log by `path`.
pub fn begin(path: &str) -> Watch {
    flush_this_thread();
    let id = NEXT_ID.fetch_add(1, Relaxed);
    let mut requests = in_flight();
    fold(&mut requests);
    let held = held();
    let mut alongside: Vec<String> = Vec::new();
    for other in requests.iter_mut() {
        if !other.alongside.iter().any(|p| p == path) {
            other.alongside.push(path.to_string());
        }
        if !alongside.contains(&other.path) {
            alongside.push(other.path.clone());
        }
    }
    requests.push(InFlight { id, path: path.to_string(), held_at_start: held, peak: held, alongside });
    Watch { id, started: Instant::now(), ended: false }
}

impl Watch {
    /// Stop watching, and say what the request did to the heap.
    pub fn end(mut self) -> Grown {
        self.finish().expect("a request is in flight until it ends")
    }

    fn finish(&mut self) -> Option<Grown> {
        if self.ended {
            return None;
        }
        self.ended = true;
        flush_this_thread();
        let mut requests = in_flight();
        fold(&mut requests);
        let at = requests.iter().position(|r| r.id == self.id)?;
        let request = requests.swap_remove(at);
        Some(Grown {
            path: request.path,
            took: self.started.elapsed(),
            held_at_start: request.held_at_start,
            peak: request.peak,
            alongside: request.alongside,
        })
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.finish();
    }
}
