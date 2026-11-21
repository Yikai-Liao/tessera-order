// SPDX-License-Identifier: BSD-2-Clause

//! A stable permutation of isolated records and runs from one ordered source stream.

use super::{Extract, TILE_BYTES, plan};
use alloc::vec::Vec;
use core::ops::Range;

struct Span {
    source: usize,
    end: usize,
}

pub(super) struct SourceOrder(Vec<Span>);

impl SourceOrder {
    fn exception_limit<V>(len: usize, record_bytes: usize) -> usize {
        // A tile of metadata per square root of the number of input tiles.
        // Charge both construction and the worst-case split in the output:
        // one outlier can interrupt a source run and split a destination run.
        let entry_bytes = size_of::<(V, usize)>()
            + size_of::<Range<usize>>()
            + 3 * (size_of::<Span>() + record_bytes);
        let tile = (TILE_BYTES / entry_bytes).max(1);
        tile * len.div_ceil(tile).isqrt()
    }

    /// Discover sparse order only when one digit cannot finish the node.
    pub(super) fn for_node<T, E: Extract<T>>(
        records: &[T],
        varying: u128,
        extractor: &mut E,
    ) -> Option<Self> {
        let len = records.len();
        if len < plan::KEY_SAMPLE_LEN || plan::complete_digit(varying).is_some() {
            return None;
        }
        let limit = Self::exception_limit::<E::Value>(len, size_of::<T>());
        // Sample admission only. Both constructions enforce the actual limit.
        let allowance = limit.saturating_mul(plan::KEY_SAMPLE_LEN).div_ceil(len);
        let pivot = extractor.key(&records[len / 2]);
        let (mut inversions, mut exceptions) = (0, 0);
        for j in 0..plan::KEY_SAMPLE_LEN {
            let i = plan::sample_index(len, j).min(len - 2);
            let key = extractor.key(&records[i]);
            exceptions += usize::from(key != pivot);
            inversions += usize::from(key > extractor.key(&records[i + 1]));
            if inversions > allowance && exceptions > allowance {
                return None;
            }
        }
        if exceptions <= allowance {
            return Self::dominant(records, extractor, pivot, limit);
        }
        let mut runs: Vec<Range<usize>> = Vec::new();
        let mut removed = Vec::new();
        let mut index = 0;
        // Each disjoint inverted pair costs at least one deletion. Scan each
        // surviving run before writing its span to keep metadata off the hot loop.
        while index < len {
            let mut previous = extractor.key(&records[index]);
            if let Some(last) = runs.last_mut() {
                let source = last.end - 1;
                let held = extractor.key(&records[source]);
                if held > previous {
                    if removed.len() + 2 > limit {
                        return None;
                    }
                    removed.extend([(held, source), (previous, index)]);
                    last.end -= 1;
                    if last.start == last.end {
                        runs.pop();
                    }
                    index += 1;
                    continue;
                }
            }
            let start = index;
            index += 1;
            index += records[index..]
                .iter()
                .position(|r| {
                    let next = extractor.key(r);
                    core::mem::replace(&mut previous, next) > next
                })
                .unwrap_or(len - index);
            // A changing key can erase an inversion between its two probes.
            // Keep adjacent source intervals coalesced even in that case.
            if let Some(last) = runs.last_mut()
                && last.end == start
            {
                last.end = index;
            } else {
                runs.push(start..index);
            }
        }
        removed.sort_unstable();
        Some(Self::merge(
            runs.into_iter(),
            removed.into_iter(),
            |run, entry, _| {
                // An exception lies outside this entire source run. One position
                // comparison therefore breaks every equal-key tie in the run.
                let mut precedes = |r: &T| (extractor.key(r), run.start) < entry;
                let slice = &records[run.clone()];
                if !precedes(&slice[0]) {
                    0
                } else if precedes(slice.last().unwrap()) {
                    slice.len()
                } else {
                    slice.partition_point(precedes)
                }
            },
        ))
    }

    fn dominant<T, E: Extract<T>>(
        records: &[T],
        extractor: &mut E,
        pivot: E::Value,
        limit: usize,
    ) -> Option<Self> {
        let mut positions = Vec::new();
        let mut entries = Vec::new();
        let mut lower = 0;
        for (source, record) in records.iter().enumerate() {
            let key = extractor.key(record);
            if key != pivot {
                if entries.len() == limit {
                    return None;
                }
                lower += usize::from(key < pivot);
                positions.push(source);
                entries.push((key, source));
            }
        }
        let len = records.len();
        let equal = lower..len - (entries.len() - lower);
        entries.sort_unstable(); // source position breaks equal-key ties
        let ordered = entries.into_iter().map(|(_, src)| src);
        Some(Self::around_band(len, &positions, ordered, lower, equal))
    }

    /// Clamp exceptional slots to the two sides of the reserved background band.
    fn around_band(
        len: usize,
        sources: &[usize],
        ordered: impl Iterator<Item = usize>,
        before: usize,
        band: Range<usize>,
    ) -> Self {
        let slots = sources.iter().enumerate().map(|(i, &slot)| {
            if i < before {
                slot.min(band.start - (before - i))
            } else {
                slot.max(band.end + (i - before))
            }
        });
        let mut start = 0;
        let runs = sources.iter().copied().chain([len]).map(|end| {
            let run = start..end;
            start = end + 1;
            run
        });
        Self::merge(runs, slots.zip(ordered), |run, (slot, _), written| {
            (slot - written).min(run.len())
        })
    }

    /// Merge exceptional sources into disjoint source runs. `prefix` gives the
    /// number of records in the current run that precede the next exception.
    fn merge<K: Copy>(
        runs: impl Iterator<Item = Range<usize>>,
        exceptions: impl Iterator<Item = (K, usize)>,
        mut prefix: impl FnMut(&Range<usize>, (K, usize), usize) -> usize,
    ) -> Self {
        let mut order = Self(Vec::with_capacity(
            runs.size_hint().0 + exceptions.size_hint().0,
        ));
        let mut runs = runs.filter(|r| !r.is_empty()).peekable();
        for entry in exceptions {
            while let Some(run) = runs.peek_mut() {
                let n = prefix(run, entry, order.len());
                if n != 0 {
                    order.push(run.start..run.start + n);
                    run.start += n;
                }
                if run.start != run.end {
                    break;
                }
                runs.next();
            }
            order.push(entry.1..entry.1 + 1);
        }
        for run in runs {
            order.push(run);
        }
        order
    }

    fn len(&self) -> usize {
        self.0.last().map_or(0, |s| s.end)
    }

    fn push(&mut self, range: Range<usize>) {
        let start = self.len();
        self.0.push(Span {
            source: range.start,
            end: start + range.len(),
        });
    }

    fn spans(&self) -> impl DoubleEndedIterator<Item = (usize, Range<usize>)> + '_ {
        self.0.iter().enumerate().map(|(i, s)| {
            let start = if i == 0 { 0 } else { self.0[i - 1].end };
            (start, s.source..s.source + (s.end - start))
        })
    }

    /// Gather isolated records first. Every longer span comes from the same
    /// monotone subsequence, so all its source and destination ranges have
    /// the same relative order. Move each such span at most once.
    pub(super) fn apply<T>(&self, records: &mut [T]) {
        use core::{mem::MaybeUninit, ptr};
        assert_eq!(records.len(), self.len());
        let singles = || {
            self.spans()
                .filter(|(dest, r)| r.len() == 1 && *dest != r.start)
        };
        let mut saved = Vec::<MaybeUninit<T>>::with_capacity(singles().count());
        let scratch = saved.as_mut_ptr().cast::<T>();
        let data = records.as_mut_ptr();
        // SAFETY: the spans partition both source and destination. All key
        // calls, rank arithmetic and allocation are complete before movement.
        // Scratch has one distinct slot for every singleton. Between copies
        // we access only raw pointers and immutable span metadata. No user
        // code runs while a record is held in scratch.
        unsafe {
            for (slot, (_, src)) in singles().enumerate() {
                ptr::copy_nonoverlapping(data.add(src.start), scratch.add(slot), 1);
            }
            // A left-moving span ends before every later source. Every earlier
            // right-moving source also ends before this span's destination.
            for (dest, src) in self.spans() {
                if src.len() > 1 && dest < src.start {
                    ptr::copy(data.add(src.start), data.add(dest), src.len());
                }
            }
            // The symmetric backward traversal protects unread right movers.
            for (dest, src) in self.spans().rev() {
                if src.len() > 1 && dest > src.start {
                    ptr::copy(data.add(src.start), data.add(dest), src.len());
                }
            }
            for (slot, (dest, _)) in singles().enumerate() {
                ptr::copy_nonoverlapping(scratch.add(slot), data.add(dest), 1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::key::ByKey;
    use alloc::boxed::Box;

    #[test]
    fn stable_projection_of_sparse_exceptions_and_ordered_runs() {
        let len = 193;
        let ordered: Vec<_> = (0..len).map(|i| i as u64 / 3).collect();
        let mut swapped = ordered.clone();
        swapped[..len / 2].rotate_right(1);
        swapped[len / 2..].rotate_left(1);
        let dominant: Vec<_> = (0..len)
            .map(|i| {
                if i % 11 == 0 {
                    0
                } else if i % 7 == 0 {
                    4
                } else {
                    2
                }
            })
            .collect();
        for keys in [ordered, swapped, dominant] {
            let mut expected: Vec<_> = (0..len).collect();
            expected.sort_by_key(|&i| keys[i]);
            // A duplicated or lost owner is observable through the boxes
            // under Miri. Wide records keep the exception budget below the
            // sample size, so both constructions are exercised.
            let mut records: Vec<_> = (0..len).map(|i| (Box::new(i), [0u8; 1024])).collect();
            let mut extract = ByKey(|r: &(Box<usize>, [u8; 1024])| keys[*r.0]);
            let order = SourceOrder::for_node(&records, u64::MAX.into(), &mut extract).unwrap();
            order.apply(&mut records);
            assert_eq!(records.iter().map(|r| *r.0).collect::<Vec<_>>(), expected);
        }
    }
}
