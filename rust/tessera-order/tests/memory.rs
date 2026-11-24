// SPDX-License-Identifier: BSD-2-Clause

//! Per-call scratch allocation and release, measured with a counting allocator.

// Miri keeps its own allocator; the other suites cover record handling there.
#![cfg(not(miri))]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
#[cfg(feature = "rayon")]
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering},
};
use tessera_order::{sort, sort_by_key};

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
    static BYTES: Cell<isize> = const { Cell::new(0) };
    static PEAK: Cell<isize> = const { Cell::new(0) };
    #[cfg(feature = "rayon")]
    static POOL_THREAD: Cell<bool> = const { Cell::new(false) };
}
#[cfg(feature = "rayon")]
static GLOBAL_ENABLED: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "rayon")]
static GLOBAL_ALLOCS: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "rayon")]
static GLOBAL_BYTES: AtomicIsize = AtomicIsize::new(0);
#[cfg(feature = "rayon")]
static GLOBAL_LOCK: Mutex<()> = Mutex::new(());

#[cfg(feature = "rayon")]
fn lock_global() -> std::sync::MutexGuard<'static, ()> {
    GLOBAL_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

struct Counting;
#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn record(bytes: isize, allocation: bool) {
    #[cfg(feature = "rayon")]
    if GLOBAL_ENABLED.load(Ordering::Relaxed) && POOL_THREAD.try_with(Cell::get).unwrap_or(false) {
        GLOBAL_BYTES.fetch_add(bytes, Ordering::Relaxed);
        if allocation {
            GLOBAL_ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
    }
    if ENABLED.try_with(Cell::get).unwrap_or(false) {
        BYTES.with(|total| {
            let live = total.get() + bytes;
            total.set(live);
            PEAK.with(|peak| peak.set(peak.get().max(live)));
        });
        if allocation {
            ALLOCS.with(|total| total.set(total.get() + 1));
        }
    }
}

// SAFETY: all allocation operations forward the same pointer and Layout to
// System. Thread-local bookkeeping neither allocates nor changes storage.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplies a valid allocation layout.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            record(layout.size() as isize, true);
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record(-(layout.size() as isize), false);
        // SAFETY: ptr/layout came from this forwarding allocator.
        unsafe {
            System.dealloc(ptr, layout);
        }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: the original pointer/layout and requested size come from
        // GlobalAlloc's caller and are passed unchanged to System.
        let next = unsafe { System.realloc(ptr, layout, size) };
        if !next.is_null() {
            record(size as isize - layout.size() as isize, true);
        }
        next
    }
}

fn measure(f: impl FnOnce()) -> (usize, isize) {
    measure_peak(f).0
}

/// Measures allocation count, net bytes and the peak live size of one call.
fn measure_peak(f: impl FnOnce()) -> ((usize, isize), isize) {
    ALLOCS.with(|x| x.set(0));
    BYTES.with(|x| x.set(0));
    PEAK.with(|x| x.set(0));
    ENABLED.with(|x| x.set(true));
    f();
    ENABLED.with(|x| x.set(false));
    (
        (ALLOCS.with(Cell::get), BYTES.with(Cell::get)),
        PEAK.with(Cell::get),
    )
}

#[test]
fn scratch_released_after_sort() {
    #[cfg(feature = "rayon")]
    let _guard = lock_global();
    let mut input: Vec<_> = (0..20000u64)
        .map(|id| (id.wrapping_mul(0x9e3779b97f4a7c15), id))
        .collect();
    let (allocations, bytes) = measure(|| sort_by_key(&mut input, |x| x.0));
    assert!(allocations > 0);
    assert_eq!(bytes, 0);
    assert!(input.is_sorted_by_key(|x| x.0));

    let mut keys: Vec<_> = (0..20000u64)
        .map(|id| id.wrapping_mul(0x9e3779b97f4a7c15))
        .collect();
    let (allocations, bytes) = measure(|| sort(&mut keys));
    assert!(allocations > 0);
    assert_eq!(bytes, 0);
    assert!(keys.is_sorted());
}

#[test]
fn bucket_ordered_input_keeps_scratch_in_one_tile() {
    #[cfg(feature = "rayon")]
    let _guard = lock_global();
    // A sorted body with a run of keys above it is already in bucket order at
    // the top bit, so splitting there needs no block partition. The run sits
    // either side of the body: behind it after a rotation. The wide digit the
    // partition would choose reserves 2 * 2^12 blocks of records, which is
    // 8 MiB for these keys.
    let len = 65_536usize;
    let run = len / 128;
    let above: Vec<u64> = (0..run as u64).rev().map(|id| 1 << 63 | id).collect();
    let body: Vec<u64> = (0..(len - run) as u64).collect();
    for keys in [
        body.iter().chain(&above).copied().collect::<Vec<_>>(),
        above.iter().chain(&body).copied().collect::<Vec<_>>(),
    ] {
        let mut keys = keys;
        let ((allocations, net), peak) = measure_peak(|| sort(&mut keys));
        assert!(keys.is_sorted());
        assert!(allocations > 0);
        assert_eq!(net, 0);
        assert!(peak <= 128 * 1024, "peak scratch {peak}");
    }
}

#[test]
fn ordered_inputs_do_not_allocate() {
    #[cfg(feature = "rayon")]
    let _guard = lock_global();
    for len in [0, 1, 48, 49, 8193, 20000] {
        for constant in [false, true] {
            let mut keys: Vec<_> = (0..len as u64)
                .map(|id| if constant { 7 } else { id })
                .collect();
            assert_eq!(measure(|| sort(&mut keys)), (0, 0));
            let mut records: Vec<_> = keys.iter().copied().enumerate().collect();
            assert_eq!(measure(|| sort_by_key(&mut records, |x| x.1)), (0, 0));
        }
    }
}

#[cfg(feature = "rayon")]
fn measure_pool_threads(include_caller: bool, f: impl FnOnce()) -> (usize, isize) {
    let _guard = lock_global();
    let prev = POOL_THREAD.with(|member| member.replace(include_caller));
    GLOBAL_ALLOCS.store(0, Ordering::SeqCst);
    GLOBAL_BYTES.store(0, Ordering::SeqCst);
    GLOBAL_ENABLED.store(true, Ordering::SeqCst);
    f();
    GLOBAL_ENABLED.store(false, Ordering::SeqCst);
    POOL_THREAD.with(|member| member.set(prev));
    (
        GLOBAL_ALLOCS.load(Ordering::SeqCst),
        GLOBAL_BYTES.load(Ordering::SeqCst),
    )
}

#[cfg(feature = "rayon")]
#[test]
fn parallel_ordered_and_constant_inputs_do_not_allocate() {
    use rayon::prelude::*;
    use tessera_order::par_sort;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .start_handler(|_| POOL_THREAD.with(|member| member.set(true)))
        .build()
        .unwrap();
    let len = 131_073usize;
    let mut sorted: Vec<u64> = (0..len as u64).collect();
    let mut reversed: Vec<u64> = (0..len as u64).rev().collect();
    let mut constant: Vec<u64> = vec![7u64; len];
    let mut full_width_asc = vec![u64::MAX; len];
    full_width_asc[0] = 0;
    // Warm every worker thread and Rayon's chunk iterator state outside
    // measurement so one-time pool setup is not charged to the sort call.
    let mut warmup = vec![0u64; len];
    pool.install(|| {
        warmup
            .par_chunks_mut(len.div_ceil(4))
            .for_each(|chunk| chunk[0] = 1);
    });
    // Exercise the same counter helper used by the sort assertions below while
    // leaving the calling test thread untracked so Rayon's caller-side job Vec
    // is excluded and all four pool workers must be counted individually.
    let origin = std::thread::current().id();
    let (worker_allocs, worker_net) = measure_pool_threads(false, || {
        pool.broadcast(|ctx| {
            assert_ne!(std::thread::current().id(), origin);
            let boxed = Box::new(ctx.index());
            std::hint::black_box(boxed);
        });
    });
    assert_eq!(worker_allocs, 4);
    assert_eq!(worker_net, 0);

    pool.install(|| {
        assert_eq!(measure_pool_threads(true, || par_sort(&mut sorted)), (0, 0));
        assert_eq!(
            measure_pool_threads(true, || par_sort(&mut reversed)),
            (0, 0)
        );
        assert_eq!(
            measure_pool_threads(true, || par_sort(&mut constant)),
            (0, 0)
        );
        assert_eq!(
            measure_pool_threads(true, || par_sort(&mut full_width_asc)),
            (0, 0)
        );
    });
    assert!(sorted.is_sorted());
    assert!(reversed.is_sorted());
    assert!(full_width_asc.is_sorted());
}
