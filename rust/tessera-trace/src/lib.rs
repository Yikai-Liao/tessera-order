// SPDX-License-Identifier: BSD-2-Clause

//! WebAssembly exports for the documentation figures.
//!
//! The site fills the key buffer, calls [`partition`] and reads the event log:
//! five `u32` words per event, a tag and up to four fields in the order
//! [`Event`] declares them. Unused fields are zero. The docs build compiles this
//! crate for `wasm32-unknown-unknown`; the figures decode the log in
//! `docs/.vitepress/theme/sim/engine.ts`.

use std::cell::RefCell;
use tessera_sort::figures::{self, Event};

/// Words per event in the log.
pub const EVENT_WORDS: usize = 5;

thread_local! {
    static KEYS: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
    static LOG: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
}

/// Encodes one event as its tag and fields.
pub fn encode(event: Event) -> [u32; EVENT_WORDS] {
    let w = |value: usize| u32::try_from(value).expect("figure inputs fit in u32");
    match event {
        Event::Begin {
            len,
            block_size,
            buckets,
            scratch_blocks,
        } => [0, w(len), w(block_size), w(buckets), w(scratch_blocks)],
        Event::Writer { bucket, physical } => [1, w(bucket), w(physical), 0, 0],
        Event::Scatter {
            input,
            bucket,
            physical,
            slot,
        } => [2, w(input), w(bucket), w(physical), w(slot)],
        Event::Source {
            bucket,
            physical,
            len,
        } => [3, w(bucket), w(physical), w(len), 0],
        Event::Final { physical } => [4, w(physical), 0, 0, 0],
        Event::Relocate { source, from, to } => [5, w(source), w(from), w(to), 0],
        Event::Transfer {
            source,
            physical,
            start,
            overlapped,
        } => [6, w(source), w(physical), w(start), u32::from(overlapped)],
        Event::Band { bucket, start, len } => [7, w(bucket), w(start), w(len), 0],
    }
}

/// Resizes the key buffer to `len` keys and returns its address for the
/// caller to fill. The address is valid until the next call.
#[unsafe(no_mangle)]
pub extern "C" fn keys(len: usize) -> *mut u32 {
    KEYS.with(|keys| {
        let mut keys = keys.borrow_mut();
        keys.clear();
        keys.resize(len, 0);
        keys.as_mut_ptr()
    })
}

/// Partitions the key buffer into `2^radix` buckets by `key / divisor`, in
/// blocks of `block_size` keys. Leaves the partitioned keys in the buffer and
/// returns the length of the event log in words.
#[unsafe(no_mangle)]
pub extern "C" fn partition(radix: usize, block_size: usize, divisor: u32) -> usize {
    let events = KEYS.with(|keys| {
        figures::partition(&mut keys.borrow_mut(), radix, block_size, |key| {
            (key / divisor) as usize
        })
    });
    LOG.with(|log| {
        let mut log = log.borrow_mut();
        log.clear();
        log.extend(events.into_iter().flat_map(encode));
        log.len()
    })
}

/// Address of the event log [`partition`] wrote, valid until the next call.
#[unsafe(no_mangle)]
pub extern "C" fn events() -> *const u32 {
    LOG.with(|log| log.borrow().as_ptr())
}
