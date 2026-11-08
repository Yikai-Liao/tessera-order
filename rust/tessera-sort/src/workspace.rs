// SPDX-License-Identifier: BSD-2-Clause

//! Reusable scratch memory.

use crate::key::{ByKey, Direct};
use crate::{RadixKey, block, engine};
use alloc::vec::Vec;
use core::mem::MaybeUninit;

/// Scratch memory kept between sorts of `T` records.
///
/// The free functions allocate scratch during each call and free it before
/// returning. A workspace keeps that capacity instead and allocates again
/// when a later call needs more scratch. Required capacity depends on input
/// length, distribution, key width and the sorting path, not length alone.
/// Its methods behave exactly like the free functions of the same name and
/// produce the same results.
///
/// A workspace keeps capacity, never records: each call starts from empty
/// scratch, and a workspace stays usable after a key function panics. Sorting
/// takes `&mut self`, so use one workspace per thread. The workspace is typed
/// by the record, not the key, so one workspace serves every key type.
///
/// # Memory
///
/// The workspace keeps the largest capacity it has used until
/// [`shrink_to_fit`](Self::shrink_to_fit) releases it. A key-function panic
/// may drop part of the block-partition capacity; the next call allocates it
/// again. [`retained_bytes`](Self::retained_bytes) reports the heap capacity.
///
/// # Examples
///
/// ```
/// use tessera_sort::Workspace;
///
/// let batches = vec![vec![(3u32, "a"), (1, "b"), (3, "c")], vec![(2, "d"), (0, "e")]];
/// let mut workspace = Workspace::new();
/// for mut batch in batches {
///     workspace.stable_sort_by_key(&mut batch, |row| row.0);
///     assert!(batch.is_sorted_by_key(|row| row.0));
/// }
///
/// // Cap the memory a long-lived workspace keeps.
/// if workspace.retained_bytes() > 4 << 20 {
///     workspace.shrink_to_fit();
/// }
/// ```
pub struct Workspace<T> {
    /// Tile scratch records, followed by cached bucket IDs that do not fit in
    /// the cursor table.
    pub(crate) tile: Vec<MaybeUninit<T>>,
    /// Encoded keys of the insertion probe window.
    pub(crate) probe: Vec<MaybeUninit<u128>>,
    /// Block-partition buffers, created when a node first needs them.
    pub(crate) block: Option<block::Buffers<T>>,
}

impl<T> Workspace<T> {
    /// Creates an empty workspace. Does not allocate.
    pub fn new() -> Self {
        Self {
            tile: Vec::new(),
            probe: Vec::new(),
            block: None,
        }
    }

    pub(crate) fn block_buffers(&mut self) -> &mut block::Buffers<T> {
        self.block.get_or_insert_with(block::Buffers::new)
    }

    /// Sorts keys in ascending order, keeping equal keys in input order.
    ///
    /// Same as [`crate::stable_sort`], with scratch from this workspace.
    ///
    /// # Examples
    ///
    /// ```
    /// let mut workspace = tessera_sort::Workspace::new();
    /// for mut batch in [vec![3u64, 1, 2], vec![9, 8]] {
    ///     workspace.stable_sort(&mut batch);
    ///     assert!(batch.is_sorted());
    /// }
    /// ```
    pub fn stable_sort(&mut self, values: &mut [T])
    where
        T: RadixKey,
    {
        engine::sort::<T, _, false>(values, &mut Direct, Some(self));
    }

    /// Sorts records by a key, keeping records with equal keys in input order.
    ///
    /// Same as [`crate::stable_sort_by_key`], with scratch from this workspace.
    ///
    /// # Panics
    ///
    /// If `key` panics, the panic propagates after every record has been moved
    /// back into `values`, in an unspecified order. The workspace stays usable.
    /// Without the `std` feature, a panic during block partitioning aborts;
    /// panics elsewhere may unwind with every record preserved.
    ///
    /// # Examples
    ///
    /// ```
    /// let mut workspace = tessera_sort::Workspace::new();
    /// let mut rows = vec![(2u8, "b"), (1, "a"), (2, "c")];
    /// workspace.stable_sort_by_key(&mut rows, |row| row.0);
    /// assert_eq!(rows, [(1, "a"), (2, "b"), (2, "c")]);
    /// ```
    pub fn stable_sort_by_key<K: RadixKey, F: FnMut(&T) -> K>(&mut self, values: &mut [T], key: F) {
        engine::sort::<T, _, true>(values, &mut ByKey(key), Some(self));
    }

    /// Returns the heap capacity this workspace keeps, in bytes.
    ///
    /// Counts record scratch, cached bucket IDs, order-probe keys and block
    /// partition metadata. Stack storage is not included.
    pub fn retained_bytes(&self) -> usize {
        self.tile.capacity() * core::mem::size_of::<T>()
            + self.probe.capacity() * core::mem::size_of::<u128>()
            + self
                .block
                .as_ref()
                .map_or(0, block::Buffers::retained_bytes)
    }

    /// Frees all retained memory. The workspace stays usable.
    pub fn shrink_to_fit(&mut self) {
        *self = Self::new();
    }
}

impl<T> Default for Workspace<T> {
    fn default() -> Self {
        Self::new()
    }
}
