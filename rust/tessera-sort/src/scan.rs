// SPDX-License-Identifier: BSD-2-Clause

//! Scalar adjacent-pair fallback for short scans and unsupported vector widths.

use crate::RadixKey;

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
