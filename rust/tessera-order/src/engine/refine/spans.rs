// SPDX-License-Identifier: BSD-2-Clause

//! A stable permutation of isolated records and runs from one ordered source stream.

use super::{Extract, Requirement, TILE_BYTES, Unsigned, plan};
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

    /// Finish from a certified key boundary or a bounded ordered end. Both
    /// plans describe sparse placements against the original source order.
    pub(super) fn finish_cut<T, E: Extract<T>>(
        records: &mut [T],
        rank: usize,
        extractor: &mut E,
    ) -> bool {
        let pivot = extractor.key(&records[rank]);
        let len = records.len();
        let zero = E::Value::from_usize(0);
        let first_high = if pivot == !zero {
            None
        } else {
            records[..rank]
                .iter()
                .position(|r| extractor.key(r) > pivot)
        };
        let last_low = if pivot == zero {
            None
        } else {
            records[rank + 1..]
                .iter()
                .rposition(|r| extractor.key(r) < pivot)
                .map(|i| rank + 1 + i)
        };
        let limit = Self::exception_limit::<E::Value>(len, size_of::<T>());
        let skipped = match (first_high, last_low) {
            (None, None) => return true,
            (Some(start), None) => prefix_holes(
                (start..len).map(|i| (i, extractor.key(&records[i]) <= pivot)),
                rank + 1 - start,
                limit.min(len - rank - 1),
            ),
            (None, Some(end)) => prefix_holes(
                (0..=end)
                    .rev()
                    .map(|i| (i, extractor.key(&records[i]) >= pivot)),
                end + 1 - rank,
                limit.min(rank),
            )
            .map(|mut holes| {
                holes.reverse();
                holes
            }),
            _ => None,
        };
        let order = if let Some(sources) = skipped {
            let (band, before) = if first_high.is_some() {
                (0..rank + 1, 0)
            } else {
                (rank..len, sources.len())
            };
            Self::around_band(len, &sources, sources.iter().copied(), before, band)
        } else {
            let end = if rank < len / 2 {
                0..rank + 1
            } else {
                rank..len
            };
            if end.len() > limit {
                return false;
            }
            let entries = records
                .iter()
                .enumerate()
                .map(|(i, r)| (extractor.key(r), i));
            Self::ordered_end(len, end, entries, size_of::<T>())
        };
        order.apply(records);
        true
    }

    /// Discover sparse order only when one digit cannot finish the node.
    pub(super) fn for_node<T, E: Extract<T>>(
        records: &[T],
        varying: u128,
        required: Requirement,
        extractor: &mut E,
        record_bytes: usize,
    ) -> Option<Self> {
        let len = records.len();
        if len < plan::KEY_SAMPLE_LEN || plan::complete_digit(varying).is_some() {
            return None;
        }
        let limit = Self::exception_limit::<E::Value>(len, record_bytes);
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
            return Self::dominant(records, extractor, pivot, required, limit);
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
        required: Requirement,
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
        let band = match required {
            Requirement::Nth(i) if !equal.contains(&i) => {
                let end = if i < lower { 0..i + 1 } else { i..len };
                return Some(Self::ordered_end(
                    len,
                    end,
                    entries.into_iter(),
                    size_of::<T>(),
                ));
            }
            Requirement::Nth(i) => {
                if lower != 0 && lower != entries.len() {
                    entries.sort_by_key(|&(key, _)| key >= pivot);
                }
                i..i + 1
            }
            Requirement::All => {
                entries.sort_unstable(); // source position breaks equal-key ties
                equal
            }
        };
        let ordered = entries.into_iter().map(|(_, src)| src);
        Some(Self::around_band(len, &positions, ordered, lower, band))
    }

    /// Gather just one ordered end from a stream containing all its members.
    /// A batch reduction discards a full batch of candidates. When the original
    /// end has no ties outside it, only its slots and the winners' slots move.
    fn ordered_end<V: Unsigned>(
        len: usize,
        end: Range<usize>,
        entries: impl DoubleEndedIterator<Item = (V, usize)>,
        record_bytes: usize,
    ) -> Self {
        let count = end.len();
        let upper = end.start != 0;
        // Complement both fields so upper-end ties prefer later source positions.
        let orient = |(key, src): (V, usize)| if upper { (!key, !src) } else { (key, src) };
        let mut entries = entries.map(orient);
        let mut kept = Vec::with_capacity(2 * count);
        if upper {
            kept.extend(entries.by_ref().rev().take(count));
        } else {
            kept.extend(entries.by_ref().take(count));
        }
        let mut ceiling = *kept.iter().max().unwrap();
        // A sparse stream can omit original end positions. Check for ties only
        // when every end key is present and a record costs more than its entry.
        let original: Vec<_> = if record_bytes > size_of::<(V, usize)>()
            && kept.iter().all(|&e| end.contains(&orient(e).1))
        {
            kept.iter().map(|e| e.0).collect()
        } else {
            Vec::new()
        };
        let first = kept[0].0.to_u128();
        let fixed = !original.iter().fold(0, |v, k| v | (k.to_u128() ^ first))
            & (u128::MAX >> (128 - V::BITS));
        // Each exact lookup costs up to `count` comparisons. Common fixed
        // bits usually reject a key outright. Abandon the proof if matching
        // those bits would cost more than one comparison per input on average.
        let mut lookups = len / count;
        let mut disjoint = !original.is_empty() && fixed != 0;
        for entry in entries {
            if entry < ceiling {
                kept.push(entry);
                if kept.len() == 2 * count {
                    ceiling = *kept.select_nth_unstable(count - 1).1;
                    kept.truncate(count);
                }
            }
            if disjoint && (entry.0.to_u128() ^ first) & fixed == 0 {
                disjoint = lookups != 0 && !original.contains(&entry.0);
                lookups = lookups.saturating_sub(1);
            }
        }
        kept.sort_unstable();
        kept.truncate(count);
        if upper {
            kept.reverse();
        }
        let selected = kept.iter().map(|&e| orient(e).1);
        let mut sources: Vec<_> = selected.clone().collect();
        sources.sort_unstable();
        if disjoint {
            // Winners may cross their peers because (key, source) selects them
            // at the stable end of each tie. The displaced end records travel
            // in source order and have no peers in the untouched slots.
            let displaced = end.clone().filter(|i| sources.binary_search(i).is_err());
            let ordered: Vec<_> = if upper {
                displaced.chain(selected).collect()
            } else {
                selected.chain(displaced).collect()
            };
            sources.extend(end);
            sources.sort_unstable();
            sources.dedup();
            return Self::around_band(len, &sources, ordered.into_iter(), 0, 0..0);
        }
        let (band, before) = if upper {
            (0..end.start, 0)
        } else {
            (count..len, count)
        };
        Self::around_band(len, &sources, selected, before, band)
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

    pub(super) fn write_indices(&self, output: &mut Vec<usize>) {
        output.clear();
        output.reserve(self.len());
        for (_, source) in self.spans() {
            output.extend(source);
        }
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

/// Source positions skipped on the way to the required match.
fn prefix_holes(
    scan: impl Iterator<Item = (usize, bool)>,
    mut needed: usize,
    limit: usize,
) -> Option<Vec<usize>> {
    let mut holes = Vec::new();
    for (source, keep) in scan {
        if keep {
            needed -= 1;
            if needed == 0 {
                return Some(holes);
            }
        } else {
            if holes.len() == limit {
                return None;
            }
            holes.push(source);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::key::ByKey;

    #[test]
    fn bounded_ends_preserve_all_equal_key_positions() {
        let keys = [6u8, 1, 4, 4, 2, 0, 5, 1, 6, 0, 3, 0, 1, 5, 4, 2, 3];
        let len = keys.len();
        let mut ranked: Vec<_> = (0..len).collect();
        ranked.sort_by_key(|&i| keys[i]);
        for count in 1..=len / 2 {
            for end in [0..count, len - count..len] {
                let mut order = Vec::new();
                SourceOrder::ordered_end(len, end.clone(), keys.into_iter().zip(0..len), len)
                    .write_indices(&mut order);
                assert_eq!(order[end.clone()], ranked[end.clone()]);
                order[..end.start].sort_by_key(|&i| keys[i]);
                order[end.end..].sort_by_key(|&i| keys[i]);
                assert_eq!(order, ranked);
            }
        }
    }

    #[test]
    fn disjoint_end_moves_only_the_winners_and_their_slots() {
        let keys = [9u8, 9, 6, 2, 3, 2, 1, 1];
        let len = keys.len();
        for end in [0..2, len - 2..len] {
            let order = SourceOrder::ordered_end(
                len,
                end,
                keys.into_iter().zip(0..len),
                size_of::<[u8; 128]>(),
            );
            // Both pairs change ends. Equal-key background records stay put.
            let mut indices = Vec::new();
            order.write_indices(&mut indices);
            assert_eq!(indices, [6, 7, 2, 3, 4, 5, 0, 1]);
        }
    }

    #[test]
    fn one_sided_cut_places_the_stable_occurrence() {
        let input = [1u8, 6, 3, 7, 3, 4, 3, 6, 3, 9, 3];
        for reverse in [false, true] {
            let keys = if reverse {
                core::array::from_fn(|i| 10 - input[input.len() - 1 - i])
            } else {
                input
            };
            let rank = if reverse { keys.len() - 1 - 4 } else { 4 };
            let mut expected: Vec<_> = (0..keys.len()).collect();
            expected.sort_by_key(|&i| keys[i]);
            let mut ids: Vec<_> = (0..keys.len()).collect();
            let mut records: Vec<_> = ids.iter_mut().collect();
            let mut extract = ByKey(|r: &&mut usize| keys[**r]);
            assert!(SourceOrder::finish_cut(&mut records, rank, &mut extract));
            assert_eq!(*records[rank], expected[rank]);
            records[..rank].sort_by_key(|r| keys[**r]);
            records[rank + 1..].sort_by_key(|r| keys[**r]);
            assert!(records.iter().map(|r| **r).eq(expected));
        }
    }

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
            for required in [
                Requirement::All,
                Requirement::Nth(0),
                Requirement::Nth(18),
                Requirement::Nth(93),
                Requirement::Nth(167),
                Requirement::Nth(len - 1),
            ] {
                // A duplicate owner or invalid overlap in either direction is
                // observable through the unique mutable references under Miri.
                let mut ids: Vec<_> = (0..len).collect();
                let mut records: Vec<_> = ids.iter_mut().collect();
                let mut extract = ByKey(|r: &&mut usize| keys[**r]);
                let order = SourceOrder::for_node(
                    &records,
                    u64::MAX.into(),
                    required,
                    &mut extract,
                    1024, // keep the exception budget below the sample size
                )
                .unwrap();
                order.apply(&mut records);
                if let Requirement::Nth(index) = required {
                    assert_eq!(*records[index], expected[index]);
                    records[..index].sort_by_key(|r| keys[**r]);
                    records[index + 1..].sort_by_key(|r| keys[**r]);
                }
                assert_eq!(records.iter().map(|r| **r).collect::<Vec<_>>(), expected);
            }
        }
    }
}
