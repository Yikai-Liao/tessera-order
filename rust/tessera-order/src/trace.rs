// SPDX-License-Identifier: BSD-2-Clause

//! Observation points for the documentation figures.
//!
//! The block partition reports each step a figure shows through [`emit`].
//! Without the `trace` feature `emit` is an empty inline function: the event
//! closure is never called, so the engine carries no trace code. With the
//! feature, [`partition`] runs one real block partition and returns its steps,
//! which the site's figures replay instead of modelling the engine again.
//!
//! Not part of the public API: events and their meaning may change in any
//! release.

// Without the feature nothing reads the events, and nothing outside the crate
// can name them.
#![cfg_attr(not(feature = "trace"), allow(dead_code, unreachable_pub))]

/// One observable step of a block partition on the calling thread.
///
/// Physical block IDs number the partition's scratch blocks first, then the
/// full blocks of its input slice, as in `block::Partition`. Record positions are
/// indices into the input slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(missing_docs, reason = "each variant documents its fields")]
pub enum Event {
    /// A partition of `len` records into `buckets` buckets, with blocks of
    /// `block_size` records and `scratch_blocks` scratch blocks.
    Begin {
        len: usize,
        block_size: usize,
        buckets: usize,
        scratch_blocks: usize,
    },
    /// `bucket` writes into `physical` next: its first block, or the input
    /// block it links after a full block.
    Writer { bucket: usize, physical: usize },
    /// The record at input position `input` moved to slot `slot` of
    /// `physical`, its bucket's current block.
    Scatter {
        input: usize,
        bucket: usize,
        physical: usize,
        slot: usize,
    },
    /// The ledger's next source: the first `len` records of `physical`, all in
    /// `bucket`. Sources arrive in stable order, so the n-th is source n.
    Source {
        bucket: usize,
        physical: usize,
        len: usize,
    },
    /// `physical` holds no unread source and is now final: protection may
    /// overwrite it.
    Final { physical: usize },
    /// `source` moved from `from`, which a final band is about to overwrite,
    /// to the free block `to`. `from` is now final.
    Relocate {
        source: usize,
        from: usize,
        to: usize,
    },
    /// `source` was copied from `physical` to the input positions from
    /// `start`. `physical` is now final if the copy `overlapped` the block
    /// itself, and free otherwise.
    Transfer {
        source: usize,
        physical: usize,
        start: usize,
        overlapped: bool,
    },
    /// Nonempty `bucket`'s band of `len` records from `start` is final.
    Band {
        bucket: usize,
        start: usize,
        len: usize,
    },
}

#[cfg(feature = "trace")]
std::thread_local! {
    /// Events of the partition that [`partition`] is running on this thread.
    static EVENTS: core::cell::RefCell<Option<alloc::vec::Vec<Event>>> =
        const { core::cell::RefCell::new(None) };
}

/// Records the event `event` builds while [`partition`] runs on this thread.
#[inline(always)]
pub(crate) fn emit(event: impl FnOnce() -> Event) {
    #[cfg(feature = "trace")]
    EVENTS.with(|events| {
        if let Some(events) = events.borrow_mut().as_mut() {
            events.push(event());
        }
    });
    #[cfg(not(feature = "trace"))]
    let _ = event;
}

/// Classifies through the figure's closure.
#[cfg(feature = "trace")]
struct ByFn<F>(F);

#[cfg(feature = "trace")]
impl<F: FnMut(&u32) -> usize> crate::block::Classify<u32> for ByFn<F> {
    fn bucket(&mut self, record: &u32) -> usize {
        (self.0)(record)
    }
}

/// Partitions `keys` stably into the `2^radix` buckets `bucket` assigns, in
/// blocks of `block_size` keys, and returns every step in order.
///
/// This is the engine's serial block partition with the figure's block size
/// instead of the derived one. `bucket` uses only its low `radix` bits.
///
/// # Panics
///
/// Panics if `keys` holds fewer than two keys, `radix` is not in
/// `1..=12`, or `block_size` is zero.
#[cfg(feature = "trace")]
pub fn partition(
    keys: &mut [u32],
    radix: usize,
    block_size: usize,
    bucket: impl FnMut(&u32) -> usize,
) -> alloc::vec::Vec<Event> {
    assert!(keys.len() >= 2, "a partition needs two keys");
    assert!(block_size > 0, "blocks hold at least one key");
    EVENTS.with(|events| *events.borrow_mut() = Some(alloc::vec::Vec::new()));
    let mut buffers = crate::block::Buffers::new();
    crate::block::partition_in_blocks(
        keys,
        radix,
        block_size,
        ByFn(bucket),
        &mut buffers,
        &mut |_, _, _| {},
    );
    EVENTS.with(|events| events.borrow_mut().take().unwrap_or_default())
}

#[cfg(all(test, feature = "trace"))]
mod tests {
    use super::*;

    #[test]
    fn every_record_scatters_once_and_transfers_into_its_band() {
        let mut keys: alloc::vec::Vec<u32> = (0..41).map(|i| (i * 37 + 11) % 40).collect();
        let original = keys.clone();
        let events = partition(&mut keys, 2, 3, |key| (key / 10) as usize);
        let mut expected = original;
        expected.sort_by_key(|key| key / 10);
        assert_eq!(keys, expected);
        let scattered = events
            .iter()
            .filter(|event| matches!(event, Event::Scatter { .. }))
            .count();
        assert_eq!(scattered, keys.len());
        let sources: alloc::vec::Vec<usize> = events
            .iter()
            .filter_map(|event| match event {
                Event::Source { len, .. } => Some(*len),
                _ => None,
            })
            .collect();
        let transferred: usize = events
            .iter()
            .filter_map(|event| match event {
                Event::Transfer { source, .. } => Some(sources[*source]),
                _ => None,
            })
            .sum();
        assert_eq!(transferred, keys.len());
        let bands: usize = events
            .iter()
            .filter_map(|event| match event {
                Event::Band { len, .. } => Some(*len),
                _ => None,
            })
            .sum();
        assert_eq!(bands, keys.len());
    }
}
