// SPDX-License-Identifier: BSD-2-Clause

//! Binary predicate adaptation to complete stable distribution.

use crate::engine::{ByKey, distribute};

/// Moves the records matching `predicate` before the others, keeping input
/// order on both sides, and returns the matching count.
///
/// C++ `std::stable_partition`: `values[..count]` holds the matching records
/// and `values[count..]` the others, so the count is the slice's
/// `partition_point` for the predicate afterwards. Calls `predicate` exactly
/// once per record, in input order, also for zero-sized records. Records need
/// not be `Clone`, `Send` or `Sync`.
///
/// # Panics
/// A predicate panic propagates after every record is restored to the slice,
/// in unspecified order. Callback side effects are not rolled back.
///
/// # Memory
/// O(√n) auxiliary space for a fixed record size, plus bounded tile storage.
/// All private storage is released on return or unwind.
///
/// # Examples
/// ```
/// let mut rows = [(2, "a"), (1, "b"), (4, "c"), (3, "d")];
/// let count = tessera_order::partition(&mut rows, |row| row.0 % 2 == 0);
/// assert_eq!(count, 2);
/// assert_eq!(rows, [(2, "a"), (4, "c"), (1, "b"), (3, "d")]);
/// ```
pub fn partition<T, F: FnMut(&T) -> bool>(values: &mut [T], mut predicate: F) -> usize {
    if values.len() < 2 || size_of::<T>() == 0 {
        // No movement is needed, but every predicate call is still owed.
        return values.iter().filter(|record| predicate(record)).count();
    }
    distribute(values, 1, ByKey(|record: &T| u8::from(!predicate(record))))[0]
}
