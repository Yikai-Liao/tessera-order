// SPDX-License-Identifier: BSD-2-Clause

//! Record ownership at the parallel packing boundary.

use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::ByKey;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    /// Exercises packing independently of production routing cutoffs.
    fn finish_records<T: Send, E: Extract<T> + Clone + Send>(
        records: &mut [T],
        key: E,
        block_size: usize,
        shards: usize,
        finish: &(dyn Fn(&mut [T]) + Sync),
    ) {
        partition_impl_with_visit(
            records,
            3,
            5,
            key,
            &mut Buffers::new(),
            Config {
                shards,
                block_size: Some(block_size),
                parallel: !cfg!(miri),
                finish: Some(BoundedFinish {
                    limit: usize::MAX,
                    sort: finish,
                }),
            },
            Some(&mut |_| panic!("packing finisher unexpectedly bypassed")),
        );
    }

    #[test]
    fn sparse_buckets_keep_directory_ends() {
        for shards in [2, 5] {
            let mut records: Vec<_> = (0..97u32)
                .map(|id| (32 + (id % 2) * 128 + id % 16, id.to_string()))
                .collect();
            let mut expected = records.clone();
            expected.sort_by_key(|record| record.0);
            // Only buckets 1 and 5 are populated: empty leading, interior and
            // trailing buckets must not change the next bucket's source range.
            finish_records(
                &mut records,
                ByKey(|record: &(u32, String)| record.0),
                5,
                shards,
                &|records| records.sort_by_key(|record| record.0),
            );
            assert_eq!(records, expected);
        }
    }

    #[test]
    fn callback_panic_keeps_owners() {
        /// An owned payload whose destructor detects loss or duplication.
        struct Owned {
            key: u32,
            id: usize,
            text: String,
            drops: Arc<Vec<AtomicUsize>>,
        }
        impl Drop for Owned {
            fn drop(&mut self) {
                assert_eq!(self.text, self.id.to_string());
                self.drops[self.id].fetch_add(1, Ordering::Relaxed);
            }
        }
        for fail in [None, Some(0), Some(31), Some(96)] {
            let len = 97;
            let drops = Arc::new((0..len).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>());
            let mut records: Vec<_> = (0..len)
                .map(|id| Owned {
                    key: ((id * 7919 + 73) % 256) as u32,
                    id,
                    text: id.to_string(),
                    drops: drops.clone(),
                })
                .collect();
            let mut expected: Vec<_> = records
                .iter()
                .map(|record| (record.key, record.id))
                .collect();
            expected.sort_by_key(|record| record.0);
            let result = unwind::catch(|| {
                finish_records(
                    &mut records,
                    ByKey(|record: &Owned| record.key),
                    5,
                    3,
                    &|records| {
                        records.sort_by_key(|record| {
                            assert_ne!(Some(record.id), fail, "injected finishing panic");
                            record.key
                        })
                    },
                )
            });
            assert_eq!(result.is_err(), fail.is_some());
            if fail.is_none() {
                assert_eq!(
                    records
                        .iter()
                        .map(|record| (record.key, record.id))
                        .collect::<Vec<_>>(),
                    expected
                );
            }
            let mut ids: Vec<_> = records.iter().map(|record| record.id).collect();
            ids.sort_unstable();
            assert_eq!(ids, (0..len).collect::<Vec<_>>());
            assert!(drops.iter().all(|count| count.load(Ordering::Relaxed) == 0));
            drop(records);
            assert!(drops.iter().all(|count| count.load(Ordering::Relaxed) == 1));
        }
    }

    #[test]
    fn changing_keys_keep_borrows() {
        let mut values: Vec<_> = (0..97).collect();
        let mut records: Vec<_> = values.iter_mut().enumerate().collect();
        let calls = AtomicUsize::new(0);
        finish_records(
            &mut records,
            ByKey(|record: &(usize, &mut usize)| ((record.0 * 7919 + 73) % 256) as u32),
            5,
            3,
            &|records| {
                crate::stable_sort_by_key(records, |record| {
                    (calls.fetch_add(1, Ordering::Relaxed) ^ record.0) as u32
                });
            },
        );
        let mut ids = Vec::new();
        for (id, value) in records {
            assert_eq!(id, *value);
            *value += 1;
            ids.push(id);
        }
        ids.sort_unstable();
        assert_eq!(ids, (0..97).collect::<Vec<_>>());
        assert_eq!(values, (1..=97).collect::<Vec<_>>());
    }

    #[test]
    fn oversized_buckets_finish_in_place() {
        let mut records: Vec<_> = (0..97)
            .map(|id| (((id * 7919 + 73) % 256) as u32, id))
            .collect();
        let mut expected = records.clone();
        expected.sort_by_key(|record| record.0);
        let visits = AtomicUsize::new(0);
        partition_impl_with_visit(
            &mut records,
            3,
            5,
            ByKey(|record: &(u32, usize)| record.0),
            &mut Buffers::new(),
            Config {
                shards: 3,
                block_size: Some(5),
                parallel: !cfg!(miri),
                finish: Some(BoundedFinish {
                    limit: 0,
                    sort: &|records| {
                        visits.fetch_add(1, Ordering::Relaxed);
                        records.sort_by_key(|record| record.0);
                    },
                }),
            },
            Some(&mut |_| panic!("packing must finish oversized buckets directly")),
        );
        assert!(visits.load(Ordering::Relaxed) > 0);
        assert_eq!(records, expected);
    }
}
