// SPDX-License-Identifier: BSD-2-Clause

//! Allocation counters shared with the native memory shims.

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering};
static ACTIVE: AtomicBool = AtomicBool::new(false);
static LIVE: AtomicIsize = AtomicIsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

/// Starts a measurement around one sort with its input already allocated.
pub fn begin() {
    LIVE.store(0, Ordering::Relaxed);
    PEAK.store(0, Ordering::Relaxed);
    ACTIVE.store(true, Ordering::Relaxed);
}
/// Stops the measurement and returns the maximum extra live allocation size.
pub fn end() -> usize {
    ACTIVE.store(false, Ordering::Relaxed);
    PEAK.load(Ordering::Relaxed)
}
/// Records an allocation or deallocation in the active measurement.
#[unsafe(no_mangle)]
pub extern "C" fn bench_allocation_change(bytes: isize) {
    if ACTIVE.load(Ordering::Relaxed) {
        let live = LIVE.fetch_add(bytes, Ordering::Relaxed) + bytes;
        PEAK.fetch_max(live.max(0) as usize, Ordering::Relaxed);
    }
}
