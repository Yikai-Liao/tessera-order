// SPDX-License-Identifier: BSD-2-Clause

//! Required ordered positions, independent of keys, movement and scheduling.
//! A root borrows normalized disjoint intervals; child projection borrows a
//! smaller view and translates its origin, without allocation or normalization.

use core::ops::Range;

/// Positions that still need ordered refinement within one subproblem.
///
/// `None` intervals means All. Otherwise intervals use root coordinates and
/// `base` is this subproblem's root-relative start. Empty views need no work.
/// Only this module constructs views, keeping bounds and endpoint meaning here.
#[derive(Clone, Copy)]
pub(crate) struct Requirement<'a> {
    intervals: Option<&'a [Range<usize>]>,
    base: usize,
}

impl<'a> Requirement<'a> {
    /// Borrows nonempty, sorted, separated intervals bounded by `len`.
    /// Adjacent intervals must already be merged. Validation happens once at
    /// the root; a full interval becomes All, including the empty full input.
    /// Invalid intervals panic before execution can mutate any records.
    // Keep the known full interval visible across the generic entry's crate
    // boundary, so All does not execute validation or projection searches.
    #[inline]
    pub(crate) fn new(len: usize, intervals: &'a [Range<usize>]) -> Self {
        if let [range] = intervals
            && range.start == 0
            && range.end == len
        {
            return Self {
                intervals: None,
                base: 0,
            };
        }
        let mut previous = None;
        for range in intervals {
            assert!(
                range.start < range.end && range.end <= len,
                "invalid required ranks"
            );
            assert!(
                previous.is_none_or(|end| end < range.start),
                "rank intervals must be normalized"
            );
            previous = Some(range.end);
        }
        Self {
            intervals: Some(intervals),
            base: 0,
        }
    }

    /// Projects a final band in this subproblem's coordinates. The caller
    /// supplies a valid slice extent from a ledger or an ordered-run split.
    ///
    /// No overlap means no further work. A covered band becomes All. Other
    /// bands retain borrowed intersections in root coordinates, including
    /// endpoint cuts. Two binary searches cost O(log m) for m parent intervals,
    /// independently of callback order; no mutable traversal cursor is shared.
    #[inline]
    pub(crate) fn project(self, band: Range<usize>) -> Option<Self> {
        if band.is_empty() {
            return None;
        }
        let Some(intervals) = self.intervals else {
            return Some(self);
        };
        let start = self.base + band.start;
        let end = self.base + band.end;
        let first = intervals.partition_point(|range| range.end <= start);
        let remaining = &intervals[first..];
        let count = remaining.partition_point(|range| range.start < end);
        let intervals = &remaining[..count];
        let range = intervals.first()?;
        if range.start <= start && end <= range.end {
            Some(Self {
                intervals: None,
                base: 0,
            })
        } else {
            Some(Self {
                intervals: Some(intervals),
                base: start,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    // The exhaustive coordinate oracle has no unsafe ownership to interpret.
    #[test]
    #[cfg_attr(miri, ignore = "exhaustive rank arithmetic is checked by native tests")]
    fn projections_preserve_positions_and_endpoint_cuts() {
        let len = 8;
        for mask in 0u16..1 << len {
            let mut intervals = Vec::new();
            let mut index = 0;
            while index < len {
                if mask & (1 << index) == 0 {
                    index += 1;
                    continue;
                }
                let start = index;
                while index < len && mask & (1 << index) != 0 {
                    index += 1;
                }
                intervals.push(start..index);
            }
            let required = Requirement::new(len, &intervals);
            for start in 0..=len {
                for end in start..=len {
                    let child = required.project(start..end);
                    assert_eq!(child.is_some(), (start..end).any(|i| mask & (1 << i) != 0));
                    if let Some(child) = child {
                        for index in 0..end - start {
                            assert_eq!(
                                child.project(index..index + 1).is_some(),
                                mask & (1 << (start + index)) != 0,
                            );
                        }
                    }
                }
            }
        }
    }
}
