// SPDX-License-Identifier: BSD-2-Clause

//! Scalar adjacent-pair scans for callbacks, short inputs and unsupported widths.

use crate::RadixKey;
use crate::key::Extract;

/// Returns the first descending pair, retaining the previous extracted key so
/// a user callback runs once per scanned record. The callback may panic or
/// change its answer; no records move during this shared scan.
#[inline]
pub(crate) fn by_key<T, E: Extract<T>>(records: &[T], extractor: &mut E) -> Option<usize> {
    let first = records.first()?;
    let mut previous = extractor.key(first);
    records.windows(2).position(|pair| {
        let current = extractor.key(&pair[1]);
        let inverted = previous > current;
        previous = current;
        inverted
    })
}

/// Returns the offset of the first adjacent pair whose encoded keys descend,
/// scanning one pair at a time.
#[inline(always)]
pub(crate) fn scalar<K: RadixKey>(keys: &[K]) -> Option<usize> {
    let mut keys = keys.iter();
    let first = keys.next()?;
    let mut previous = first.encode();
    for (index, key) in keys.enumerate() {
        let current = key.encode();
        if previous > current {
            return Some(index);
        }
        previous = current;
    }
    None
}
