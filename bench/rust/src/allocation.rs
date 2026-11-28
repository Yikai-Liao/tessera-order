// SPDX-License-Identifier: BSD-2-Clause

//! Allocation counters shared with the native memory shims.

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering};
static ACTIVE: AtomicBool = AtomicBool::new(false);
static LIVE_BYTES: AtomicIsize = AtomicIsize::new(0);
static PEAK_BYTES: AtomicUsize = AtomicUsize::new(0);

/// Starts a measurement around one sort with its input already allocated.
pub fn begin() {
    LIVE_BYTES.store(0, Ordering::Relaxed);
    PEAK_BYTES.store(0, Ordering::Relaxed);
    ACTIVE.store(true, Ordering::Relaxed);
}
/// Stops the measurement and returns the maximum extra live allocation size.
pub fn end() -> usize {
    ACTIVE.store(false, Ordering::Relaxed);
    PEAK_BYTES.load(Ordering::Relaxed)
}
/// Reads the live bytes before the measured collection is dropped.
///
/// # Panics
/// Panics if the measurement frees more bytes than it allocated.
pub fn live_bytes() -> usize {
    LIVE_BYTES
        .load(Ordering::Relaxed)
        .try_into()
        .expect("negative retained bytes")
}
/// Records an allocation or deallocation in the active measurement.
#[unsafe(no_mangle)]
pub extern "C" fn bench_allocation_change(bytes: isize) {
    if ACTIVE.load(Ordering::Relaxed) {
        let live = LIVE_BYTES.fetch_add(bytes, Ordering::Relaxed) + bytes;
        PEAK_BYTES.fetch_max(live.max(0) as usize, Ordering::Relaxed);
    }
}
