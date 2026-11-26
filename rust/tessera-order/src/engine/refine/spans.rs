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
        // Bits outside `E::KEY_MASK` are constant across valid keys of `E`,
        // so all-1 / all-0 varying bits prove no key can exceed / precede `pivot`.
        let mask = E::Value::from_u128(E::KEY_MASK);
        let pivot_varying = pivot & mask;
        let first_high = if pivot_varying == mask {
            None
        } else {
            records[..rank]
                .iter()
                .position(|r| extractor.key(r) > pivot)
        };
        let last_low = if pivot_varying == E::Value::from_usize(0) {
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
            (None, Some(end)) if rank == 0 => {
                place_extreme(&mut records[..=end], false, pivot, extractor);
                return true;
            }
            (Some(start), None) if rank == len - 1 => {
                place_extreme(&mut records[start..], true, pivot, extractor);
                return true;
            }
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
            Self::ordered_end(records, end, extractor)
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
        // Radix selection discards most buckets, which beats a complete order.
        if !matches!(required, Requirement::All) {
            return None;
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
            Requirement::Nth(i) if equal.contains(&i) => {
                if lower != 0 && lower != entries.len() {
                    entries.sort_by_key(|&(key, _)| key >= pivot);
                }
                i..i + 1
            }
            _ => {
                entries.sort_unstable(); // source position breaks equal-key ties
                equal
            }
        };
        let ordered = entries.into_iter().map(|(_, src)| src);
        Some(Self::around_band(len, &positions, ordered, lower, band))
    }

    /// Gather just one ordered end from `records`. A batch reduction discards a
    /// full batch of candidates. When the original end has no ties outside it,
    /// only its slots and the winners' slots move.
    fn ordered_end<T, E: Extract<T>>(records: &[T], end: Range<usize>, extractor: &mut E) -> Self {
        let len = records.len();
        let count = end.len();
        let upper = end.start != 0;
        // Complement both fields so upper-end ties prefer later source positions.
        let orient = move |key: E::Value, src: usize| if upper { (!key, !src) } else { (key, src) };
        let mut kept = Vec::with_capacity(2 * count);
        kept.extend(
            records[end.clone()]
                .iter()
                .enumerate()
                .map(|(offset, r)| orient(extractor.key(r), end.start + offset)),
        );
        let mut ceiling = *kept.iter().max().unwrap();
        // Check for ties only when a record costs more than its entry.
        let original: Vec<_> = if size_of::<T>() > size_of::<(E::Value, usize)>() {
            kept.iter().map(|e| e.0).collect()
        } else {
            Vec::new()
        };
        let first = kept[0].0.to_u128();
        let fixed = !original.iter().fold(0, |v, k| v | (k.to_u128() ^ first)) & E::KEY_MASK;
        // Each exact lookup costs up to `count` comparisons. Common fixed
        // bits usually reject a key outright. Abandon the proof if matching
        // those bits would cost more than one comparison per input on average.
        let mut lookups = len / count;
        let mut disjoint = !original.is_empty() && fixed != 0;
        let rest = if upper { 0..end.start } else { count..len };
        for (offset, r) in records[rest.clone()].iter().enumerate() {
            let entry = orient(extractor.key(r), rest.start + offset);
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
        let selected = kept.iter().map(|&(k, s)| orient(k, s).1);
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

/// The cut probe found a strictly better key in this slice. Scanning in
/// source order makes `min_by_key` prefer the first minimum and `max_by_key` the
/// last maximum. A swap is safe when the displaced endpoint has no peers.
fn place_extreme<T, E: Extract<T>>(
    records: &mut [T],
    last: bool,
    anchor: E::Value,
    extractor: &mut E,
) {
    let end = if last { records.len() - 1 } else { 0 };
    let candidates = if last { 0..end } else { 1..records.len() };
    let mut has_peer = false;
    let key = |&i: &usize| {
        let key = extractor.key(&records[i]);
        has_peer |= key == anchor;
        key
    };
    let winner = if last {
        candidates.max_by_key(key)
    } else {
        candidates.min_by_key(key)
    }
    .unwrap();
    if !has_peer {
        records.swap(end, winner);
    } else if last {
        records[winner..].rotate_left(1);
    } else {
        records[..=winner].rotate_right(1);
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
    use crate::engine::key::{ByKey, Direct};
    use alloc::boxed::Box;

    #[test]
    fn bounded_ends_preserve_all_equal_key_positions() {
        let keys = [6u8, 1, 4, 4, 2, 0, 5, 1, 6, 0, 3, 0, 1, 5, 4, 2, 3];
        let wide: Vec<_> = keys.iter().map(|&k| (k, [0u8; 32])).collect();
        let len = keys.len();
        let mut ranked: Vec<_> = (0..len).collect();
        ranked.sort_by_key(|&i| keys[i]);
        for count in 1..=len / 2 {
            for end in [0..count, len - count..len] {
                for order in [
                    SourceOrder::ordered_end(&keys, end.clone(), &mut Direct),
                    SourceOrder::ordered_end(
                        &wide,
                        end.clone(),
                        &mut ByKey(|r: &(u8, [u8; 32])| r.0),
                    ),
                ] {
                    let mut perm: Vec<_> = (0..len).collect();
                    order.apply(&mut perm);
                    assert_eq!(perm[end.clone()], ranked[end.clone()]);
                    perm[..end.start].sort_by_key(|&i| keys[i]);
                    perm[end.end..].sort_by_key(|&i| keys[i]);
                    assert_eq!(perm, ranked);
                }
            }
        }
    }

    #[test]
    fn disjoint_end_moves_only_the_winners_and_their_slots() {
        let keys = [9u8, 9, 6, 2, 3, 2, 1, 1];
        let wide: Vec<_> = keys.iter().map(|&k| (k, [0u8; 128])).collect();
        let len = keys.len();
        for end in [0..2, len - 2..len] {
            let order = SourceOrder::ordered_end(&wide, end, &mut ByKey(|r: &(u8, [u8; 128])| r.0));
            // Both pairs change ends. Equal-key background records stay put.
            let mut indices: Vec<_> = (0..len).collect();
            order.apply(&mut indices);
            assert_eq!(indices, [6, 7, 2, 3, 4, 5, 0, 1]);
        }

        // Wide padded-tuple keys exercise `fixed & E::KEY_MASK` on both ends:
        // disjoint ends move only winners and displaced slots.
        let disjoint_tuples: Vec<_> = keys
            .iter()
            .map(|&k| ((u16::from(k), k), [0u8; 32]))
            .collect();
        for end in [0..2, len - 2..len] {
            let order = SourceOrder::ordered_end(
                &disjoint_tuples,
                end,
                &mut ByKey(|r: &((u16, u8), [u8; 32])| r.0),
            );
            let mut indices: Vec<_> = (0..len).collect();
            order.apply(&mut indices);
            assert_eq!(indices, [6, 7, 2, 3, 4, 5, 0, 1]);
        }

        // Wide padded-tuple ends with outside peers (both with shared effective
        // bits and with complementary all-0/all-1 keys where `fixed == 0`) must
        // preserve equal-key order across the slice.
        let tied_cases: [&[(u16, u8)]; 2] = [
            &[
                (3, 5),
                (u16::MAX, u8::MAX),
                (u16::MAX, u8::MAX),
                (3, 5),
                (3, 5),
                (0, 0),
                (0, 0),
                (3, 5),
            ],
            &[
                (0, 0),
                (u16::MAX, u8::MAX),
                (u16::MAX, u8::MAX),
                (0, 0),
                (0, 0),
                (u16::MAX, u8::MAX),
            ],
        ];
        for tuple_keys in tied_cases {
            let wide_tuples: Vec<_> = tuple_keys.iter().map(|&k| (k, [0u8; 32])).collect();
            let n = tuple_keys.len();
            let mut expected: Vec<_> = (0..n).collect();
            expected.sort_by_key(|&i| tuple_keys[i]);
            for end in [0..1, 0..2, n - 2..n, n - 1..n] {
                let order = SourceOrder::ordered_end(
                    &wide_tuples,
                    end.clone(),
                    &mut ByKey(|r: &((u16, u8), [u8; 32])| r.0),
                );
                let mut perm: Vec<_> = (0..n).collect();
                order.apply(&mut perm);
                assert_eq!(perm[end.clone()], expected[end.clone()]);
                perm[..end.start].sort_by_key(|&i| tuple_keys[i]);
                perm[end.end..].sort_by_key(|&i| tuple_keys[i]);
                assert_eq!(perm, expected);
            }
        }
    }

    #[test]
    fn endpoint_cuts_preserve_the_winner_and_displaced_peers() {
        // The first and last extremes both tie. Each displaced endpoint can
        // cross its peer. Replacing both peers also exercises direct swaps.
        for peer in [5u8, 6] {
            let keys = [5, peer, 9, 1, 9, 1, peer, 5];
            for rank in [0, keys.len() - 1] {
                let mut ids: Vec<_> = (0..keys.len()).collect();
                let mut expected = ids.clone();
                expected.sort_by_key(|&id| keys[id]);
                assert!(SourceOrder::finish_cut(
                    &mut ids,
                    rank,
                    &mut ByKey(|id: &usize| keys[*id])
                ));
                assert_eq!(ids[rank], expected[rank]);
                ids[..rank].sort_by_key(|&id| keys[id]);
                ids[rank + 1..].sort_by_key(|&id| keys[id]);
                assert_eq!(ids, expected);
            }
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

        // Padded tuples and narrow-mask reversed keys (with nonzero constant
        // bits outside `E::KEY_MASK`) resolve stably at both varying extremes.
        let len = 32usize;
        let rank = len - 5;
        let low_rank = 4usize;
        let tuple_max: [(u16, u8); 32] = core::array::from_fn(|i| {
            if i >= rank {
                (u16::MAX, u8::MAX)
            } else {
                (7, 3)
            }
        });
        let mut ids: Vec<_> = (0..len).collect();
        assert!(SourceOrder::finish_cut(
            &mut ids,
            rank,
            &mut ByKey(|&id: &usize| tuple_max[id]),
        ));
        assert_eq!(ids, (0..len).collect::<Vec<_>>());

        let rev_false: [core::cmp::Reverse<bool>; 32] =
            core::array::from_fn(|i| core::cmp::Reverse(i < rank));
        let mut ids: Vec<_> = (0..len).collect();
        assert!(SourceOrder::finish_cut(
            &mut ids,
            rank,
            &mut ByKey(|&id: &usize| rev_false[id]),
        ));
        assert_eq!(ids, (0..len).collect::<Vec<_>>());

        let rev_bool_cut: [core::cmp::Reverse<bool>; 32] =
            core::array::from_fn(|i| core::cmp::Reverse(i != 2));
        let mut ids: Vec<_> = (0..len).collect();
        let mut expected = ids.clone();
        expected.sort_by_key(|&i| rev_bool_cut[i]);
        assert!(SourceOrder::finish_cut(
            &mut ids,
            low_rank,
            &mut ByKey(|&id: &usize| rev_bool_cut[id]),
        ));
        assert_eq!(ids[low_rank], expected[low_rank]);
        ids[..low_rank].sort_by_key(|&i| rev_bool_cut[i]);
        ids[low_rank + 1..].sort_by_key(|&i| rev_bool_cut[i]);
        assert_eq!(ids, expected);

        let rev_nul: [core::cmp::Reverse<char>; 32] =
            core::array::from_fn(|i| core::cmp::Reverse(if i >= rank { '\0' } else { 'a' }));
        let mut ids: Vec<_> = (0..len).collect();
        assert!(SourceOrder::finish_cut(
            &mut ids,
            rank,
            &mut ByKey(|&id: &usize| rev_nul[id]),
        ));
        assert_eq!(ids, (0..len).collect::<Vec<_>>());

        // Padded tuple keys with a mixed non-max suffix (at the effective
        // maximum or one below it) still resolve stably without false skips.
        for suffix_mixed in [
            // Max at `rank`, non-max entries after `rank` -> one-sided cut.
            core::array::from_fn::<_, 32, _>(|i| match i {
                25..=27 | 29 | 31 => (u16::MAX, u8::MAX),
                _ => (7u16, 3u8),
            }),
            // One below max at `rank`, true max in prefix and non-max in suffix
            // -> must scan prefix and fall back to `ordered_end`.
            core::array::from_fn::<_, 32, _>(|i| match i {
                5 => (u16::MAX, u8::MAX),
                27 | 29 | 31 => (u16::MAX, u8::MAX - 1),
                _ => (7u16, 3u8),
            }),
            // One below max across the whole suffix with a true max in prefix
            // -> must scan prefix rather than treating `KEY_MASK - 1` as max.
            core::array::from_fn::<_, 32, _>(|i| match i {
                5 => (u16::MAX, u8::MAX),
                27..=31 => (u16::MAX, u8::MAX - 1),
                _ => (7u16, 3u8),
            }),
        ] {
            let mut ids: Vec<_> = (0..len).collect();
            let mut expected = ids.clone();
            expected.sort_by_key(|&i| suffix_mixed[i]);
            assert!(SourceOrder::finish_cut(
                &mut ids,
                rank,
                &mut ByKey(|&i: &usize| suffix_mixed[i]),
            ));
            assert_eq!(ids[rank], expected[rank]);
            ids[..rank].sort_by_key(|&i| suffix_mixed[i]);
            ids[rank + 1..].sort_by_key(|&i| suffix_mixed[i]);
            assert_eq!(ids, expected);
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
                    (i / 11) as u64 % 2
                } else if i % 7 == 0 {
                    3 + (i / 7) as u64 % 2
                } else {
                    2
                }
            })
            .collect();
        for (keys, mostly_equal) in [(ordered, false), (swapped, false), (dominant, true)] {
            let mut expected: Vec<_> = (0..len).collect();
            expected.sort_by_key(|&i| keys[i]);
            for required in [
                Requirement::All,
                Requirement::Nth(0),
                Requirement::Nth(5),
                Requirement::Nth(18),
                Requirement::Nth(93),
                Requirement::Nth(167),
                Requirement::Nth(180),
                Requirement::Nth(len - 1),
            ] {
                // A duplicated or lost owner is observable through the boxes
                // under Miri. Wide records keep the exception budget below
                // the sample size, so both constructions are exercised.
                let mut records: Vec<_> = (0..len).map(|i| (Box::new(i), [0u8; 1024])).collect();
                let mut extract = ByKey(|r: &(Box<usize>, [u8; 1024])| keys[*r.0]);
                let Some(order) = SourceOrder::for_node(
                    &records,
                    u64::MAX.into(),
                    required,
                    &mut extract,
                    size_of::<(Box<usize>, [u8; 1024])>(),
                ) else {
                    // Selection uses only the dominant-key construction.
                    assert!(!mostly_equal && matches!(required, Requirement::Nth(_)));
                    continue;
                };
                order.apply(&mut records);
                if let Requirement::Nth(index) = required {
                    assert_eq!(*records[index].0, expected[index]);
                    records[..index].sort_by_key(|r| keys[*r.0]);
                    records[index + 1..].sort_by_key(|r| keys[*r.0]);
                }
                assert_eq!(records.iter().map(|r| *r.0).collect::<Vec<_>>(), expected);
            }
        }
    }
}
