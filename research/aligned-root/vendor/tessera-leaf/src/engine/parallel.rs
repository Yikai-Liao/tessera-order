// SPDX-License-Identifier: BSD-2-Clause

//! Parallel completion of fully materialized, disjoint radix buckets.

use super::*;

// A task needs several tiles to amortize scheduling and independent scratch.
const MIN_TASK_BYTES: usize = 256 * 1024;
// At 512 KiB, the first measured schedule did not consistently beat the
// sequential call; keep at least sixteen tiles before paying for task setup.
const MIN_PARALLEL_BYTES: usize = 1024 * 1024;

/// Selects the serial entry point before adapting a user key to a shared borrow.
#[inline]
pub(crate) fn use_serial<T>(len: usize) -> bool {
    len < 2
        || core::mem::size_of::<T>() == 0
        || len.saturating_mul(core::mem::size_of::<T>()) < MIN_PARALLEL_BYTES
        || rayon::current_num_threads() == 1
}

pub(crate) fn sort<T: Send, E: Extract<T> + Clone + Send, const CACHED: bool>(
    arr: &mut [T],
    mut key: E,
) {
    let record_bytes = core::mem::size_of::<T>();
    let bytes = arr.len().saturating_mul(record_bytes);
    if arr.len() < 2
        || record_bytes == 0
        || bytes < MIN_PARALLEL_BYTES
        || rayon::current_num_threads() == 1
    {
        super::sort::<T, E, CACHED>(arr, &mut key, None);
        return;
    }
    let tile_bytes = TILE_BYTES
        .max(record_bytes)
        .min(MAX_TILE_RECORDS.saturating_mul(record_bytes));
    let Some(varying) = prepare_sort(arr, &mut key, tile_bytes) else {
        return;
    };
    // Keep 128-bit keys on the existing 64-bit kernels when only one half varies.
    if E::Value::BITS > 64 {
        if varying >> 64 == 0 {
            sort_prepared::<T, _, CACHED>(arr, Narrow::<E, 0>(key), varying, tile_bytes);
            return;
        }
        if varying & ((1u128 << 64) - 1) == 0 {
            sort_prepared::<T, _, CACHED>(arr, Narrow::<E, 64>(key), varying >> 64, tile_bytes);
            return;
        }
    }
    sort_prepared::<T, E, CACHED>(arr, key, varying, tile_bytes);
}

#[derive(Clone)]
struct Narrow<E, const SHIFT: usize>(E);
impl<T, E: Extract<T>, const SHIFT: usize> Extract<T> for Narrow<E, SHIFT> {
    type Value = u64;
    #[inline]
    fn key(&mut self, record: &T) -> u64 {
        (self.0.key(record).to_u128() >> SHIFT) as u64
    }
}

fn sort_prepared<T: Send, E: Extract<T> + Clone + Send, const CACHED: bool>(
    arr: &mut [T],
    mut key: E,
    varying: u128,
    tile_bytes: usize,
) {
    let bytes = arr.len().saturating_mul(core::mem::size_of::<T>());
    #[cfg(test)]
    let hooks = crate::test_witness::current();
    let mut workspace = Workspace::new();
    let low = varying.trailing_zeros() as usize;
    let top = bit_len(varying);
    // A single digit has no unfinished children. Preserve the adaptive serial
    // equality and outlier routes until there is useful independent work.
    let mut keys = [KeyValue::<T, E>::from_usize(0); MODE_SAMPLE_LEN];
    sample_keys(arr, &mut keys, &mut key);
    let equal = sample_mode(&mut keys);
    let bit = KeyValue::<T, E>::from_usize(1) << KeyValue::<T, E>::from_usize(top - 1);
    if top - low <= SPLIT_BITS {
        #[cfg(test)]
        crate::test_witness::single_route(&hooks);
        block::partition_digit_parallel_with_visit(
            arr,
            top - low,
            low,
            key,
            workspace.block_buffers(),
            rayon::current_num_threads().min((bytes / MIN_TASK_BYTES).max(1)),
            &mut |_| {},
        );
        return;
    }
    if equal.is_some() || outlier_run(arr, &keys, bit, &mut key).is_some() {
        Sorter::<T, E, CACHED> {
            workspace: &mut workspace,
            key: &mut key,
            tile_bytes,
        }
        .sort_node(arr, varying);
        return;
    }
    let digit = choose_split_digit(arr, top, tile_bytes, &mut key);
    let digit = if let Some(cap) = std::env::var("TESSERA_PIPELINE_ROOT_BITS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
    {
        top_window(varying, cap.clamp(1, digit.width)).0
    } else {
        digit
    };
    let candidate = varying & !digit.mask::<E::Value>().to_u128();
    // Prototype: child work starts as soon as compact has retired its source
    // blocks. Jobs only own materialized, pairwise disjoint bucket intervals.
    let worker_count = rayon::current_num_threads().min((bytes / MIN_TASK_BYTES).max(1));
    if worker_count < 2 {
        Sorter::<T, E, CACHED> {
            workspace: &mut workspace,
            key: &mut key,
            tile_bytes,
        }
        .sort_node(arr, varying);
        return;
    }
    let mut batch = Vec::new();
    let mut batch_records = 0usize;
    let batch_bytes = std::env::var("TESSERA_PIPELINE_BATCH_BYTES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(128 * 1024);
    rayon::scope(|scope| {
        // The sender lives inside the scope body. A producer panic therefore
        // disconnects the channel before scope waits for consumers to finish.
        #[cfg(test)]
        let _publishing_witness = crate::test_witness::PublishingGuard::new();
        #[cfg(test)]
        let _sender_witness = crate::test_witness::SenderGuard::new(hooks.clone());
        let (sender, receiver) = std::sync::mpsc::channel();
        let receiver = std::sync::Arc::new(std::sync::Mutex::new(receiver));
        let mut launched = false;
        block::partition_digit_parallel_with_visit(
            arr,
            digit.width,
            digit.shift,
            key.clone(),
            workspace.block_buffers(),
            worker_count,
            &mut |child| {
                let child_key = &key;
                batch_records += child.len();
                batch.push(Materialized {
                    pointer: child.as_mut_ptr(),
                    len: child.len(),
                });
                if batch_records.saturating_mul(core::mem::size_of::<T>()) >= batch_bytes {
                    sender.send(core::mem::take(&mut batch)).unwrap();
                    #[cfg(test)]
                    crate::test_witness::published(&hooks);
                    batch_records = 0;
                    if !launched {
                        // Finish potentially reentrant generic Clone code
                        // before publishing any consumer that waits for us.
                        let keys: Vec<_> = (1..worker_count).map(|_| child_key.clone()).collect();
                        launched = true;
                        #[cfg(test)]
                        crate::test_witness::publication_started();
                        for mut child_key in keys {
                            let receiver = receiver.clone();
                            #[cfg(test)]
                            let consumer_hooks = hooks.clone();
                            scope.spawn(move |_| {
                                #[cfg(test)]
                                let _consumer_witness =
                                    crate::test_witness::PhaseGuard::consumer(consumer_hooks);
                                consume_queue::<T, E, CACHED>(
                                    &receiver,
                                    candidate,
                                    &mut child_key,
                                    tile_bytes,
                                );
                            });
                        }
                    }
                }
                #[cfg(test)]
                crate::test_witness::callback_finished(&hooks);
            },
        );
        if batch_records != 0 {
            sender.send(core::mem::take(&mut batch)).unwrap();
        }
        // Disconnect before this producer joins the consumers. No job blocks
        // on the availability of a particular worker or holds a queue lock
        // across a user key call, including reentrant Rayon operations.
        drop(sender);
        #[cfg(test)]
        crate::test_witness::publication_closed();
        consume_queue::<T, E, CACHED>(&receiver, candidate, &mut key, tile_bytes);
    });
}

fn consume_queue<T, E: Extract<T>, const CACHED: bool>(
    receiver: &std::sync::Mutex<std::sync::mpsc::Receiver<Vec<Materialized<T>>>>,
    candidate: u128,
    key: &mut E,
    tile_bytes: usize,
) {
    let mut workspace = Workspace::new();
    loop {
        #[cfg(test)]
        let queue_witness = crate::test_witness::QueueGuard::new();
        let next = receiver
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .try_recv();
        #[cfg(test)]
        drop(queue_witness);
        match next {
            Ok(buckets) => {
                consume_buckets::<T, E, CACHED>(buckets, candidate, key, &mut workspace, tile_bytes)
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                std::thread::yield_now();
            }
        }
    }
}

fn consume_buckets<T, E: Extract<T>, const CACHED: bool>(
    buckets: Vec<Materialized<T>>,
    candidate: u128,
    key: &mut E,
    workspace: &mut Workspace<T>,
    tile_bytes: usize,
) {
    for source in buckets {
        // SAFETY: compact retired these records before publishing the task;
        // tasks cover disjoint buckets and scope joins before the input borrow
        // ends. No task aliases root scratch or another job's workspace.
        let child = unsafe { source.slice() };
        #[cfg(test)]
        crate::test_witness::new_bucket();
        if child.len() > 1 {
            let varying = scan_varying_bits(child, candidate, key);
            Sorter::<T, E, CACHED> {
                workspace,
                key,
                tile_bytes,
            }
            .sort_node(child, varying);
        }
    }
}

struct Materialized<T> {
    pointer: *mut T,
    len: usize,
}

// SAFETY: ownership of a materialized disjoint interval is transferred once
// into its scoped job. T only needs Send; no shared T references are created.
unsafe impl<T: Send> Send for Materialized<T> {}

impl<T> Materialized<T> {
    /// # Safety
    /// The interval must be fully initialized, uniquely owned by this task,
    /// and remain allocated for the lifetime selected by the scoped caller.
    unsafe fn slice<'a>(self) -> &'a mut [T] {
        // SAFETY: the scoped caller guarantees this interval's unique ownership.
        unsafe { core::slice::from_raw_parts_mut(self.pointer, self.len) }
    }
}

/// Keeps the serial adaptive checks without changing the sequential entry point.
fn prepare_sort<T, E: Extract<T>>(arr: &mut [T], key: &mut E, tile_bytes: usize) -> Option<u128> {
    let record_bytes = core::mem::size_of::<T>();
    let sampled = probe_order(arr, key);
    match sampled {
        Order::Ascending => return None,
        Order::Descending => {
            reverse_stably(arr, key);
            return None;
        }
        Order::Flat | Order::Unsorted => {}
    }
    if sampled == Order::Unsorted {
        let probe = (tile_bytes / record_bytes).max(2).min(arr.len());
        let swaps = if probe == arr.len() {
            budgeted_insertion_sort(arr, probe, |record| key.key(record))
        } else {
            // No retained workspace is exposed by the parallel API, so these
            // Copy keys can use a typed temporary without raw scratch casts.
            let mut keys: Vec<_> = arr[..probe].iter().map(|record| key.key(record)).collect();
            match budgeted_insertion_sort(&mut keys, probe, |&key| key) {
                Some(0) => None,
                Some(swaps) => budgeted_insertion_sort(arr, swaps * arr.len() / probe, |record| {
                    key.key(record)
                }),
                None => None,
            }
        };
        if swaps.is_some() {
            return None;
        }
    }
    let varying = block::varying_key_bits(arr, key);
    if varying == 0 {
        return None;
    }
    if sampled == Order::Flat {
        match confirm_order(arr, key) {
            Order::Ascending => return None,
            Order::Descending => {
                reverse_stably(arr, key);
                return None;
            }
            _ => {}
        }
    }
    Some(varying)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delayed_pipeline_consumption_preserves_owned_borrows() {
        let mut values: Vec<_> = (0..293u64)
            .map(|i| ((i * 73) % 64, alloc::format!("value {i}")))
            .collect();
        let mut expected = values.clone();
        expected.sort_by_key(|row| row.0);
        let mut pending = Vec::new();
        let mut root = Workspace::new();
        let mut worker = Workspace::new();
        let mut key = crate::key::ByKey(|row: &(u64, alloc::string::String)| row.0);
        block::partition_digit_with_ends(
            &mut values,
            2,
            4,
            &mut key,
            root.block_buffers(),
            &mut |child, key| {
                consume_buckets::<_, _, true>(
                    core::mem::take(&mut pending),
                    15,
                    key,
                    &mut worker,
                    TILE_BYTES,
                );
                pending.push(Materialized {
                    pointer: child.as_mut_ptr(),
                    len: child.len(),
                });
            },
        );
        consume_buckets::<_, _, true>(pending, 15, &mut key, &mut worker, TILE_BYTES);
        assert_eq!(values, expected);
    }
    #[test]
    fn delayed_pipeline_consumption_preserves_exclusive_records() {
        let mut payloads: Vec<_> = (0..293u64).collect();
        let mut values: Vec<_> = payloads
            .iter_mut()
            .map(|payload| (((*payload * 73) % 64), payload))
            .collect();
        let mut expected: Vec<_> = values.iter().map(|row| (row.0, *row.1)).collect();
        expected.sort_by_key(|row| row.0);
        let mut pending = Vec::new();
        let mut root = Workspace::new();
        let mut worker = Workspace::new();
        let mut key = crate::key::ByKey(|row: &(u64, &mut u64)| row.0);
        block::partition_digit_with_ends(
            &mut values,
            2,
            4,
            &mut key,
            root.block_buffers(),
            &mut |child, key| {
                consume_buckets::<_, _, true>(
                    core::mem::take(&mut pending),
                    15,
                    key,
                    &mut worker,
                    TILE_BYTES,
                );
                pending.push(Materialized {
                    pointer: child.as_mut_ptr(),
                    len: child.len(),
                });
            },
        );
        consume_buckets::<_, _, true>(pending, 15, &mut key, &mut worker, TILE_BYTES);
        let actual: Vec<_> = values.iter().map(|row| (row.0, *row.1)).collect();
        assert_eq!(actual, expected);
    }

    #[test]
    #[cfg(not(miri))]
    fn reentrant_rayon_keys_do_not_hold_worker_locks() {
        let mut values: Vec<_> = (0..131_073u64)
            .map(|i| i.wrapping_mul(0x9e3779b97f4a7c15))
            .collect();
        let mut expected = values.clone();
        expected.sort();
        let calls = std::sync::atomic::AtomicUsize::new(0);
        rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .unwrap()
            .install(|| {
                crate::par_stable_sort_by_key(&mut values, |key| {
                    let call = calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if call % 1024 == 0 {
                        rayon::join(|| (), || ());
                    }
                    *key
                });
            });
        assert_eq!(values, expected);
    }

    #[test]
    #[cfg(not(miri))]
    fn producer_clone_panic_preserves_records() {
        struct PanickingClone(std::sync::Arc<std::sync::atomic::AtomicUsize>);
        impl Clone for PanickingClone {
            fn clone(&self) -> Self {
                if self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed) == 1 {
                    panic!("producer clone panic");
                }
                Self(self.0.clone())
            }
        }
        impl Extract<u64> for PanickingClone {
            type Value = u64;
            fn key(&mut self, record: &u64) -> u64 {
                *record
            }
        }
        let mut values: Vec<_> = (0..131_073u64)
            .map(|i| i.wrapping_mul(0x9e3779b97f4a7c15))
            .collect();
        let mut expected = values.clone();
        expected.sort();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(4)
                .build()
                .unwrap()
                .install(|| {
                    sort::<_, _, false>(
                        &mut values,
                        PanickingClone(std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0))),
                    );
                });
        }));
        assert!(result.is_err());
        values.sort();
        assert_eq!(values, expected);
    }
    #[test]
    #[cfg(not(miri))]
    fn producer_unwind_disconnects_started_consumers() {
        let mut values: Vec<_> = (0..1021u64)
            .map(|i| i.wrapping_mul(0x9e3779b97f4a7c15))
            .collect();
        let mut expected = values.clone();
        expected.sort();
        let started = std::sync::atomic::AtomicBool::new(false);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(2)
                .build()
                .unwrap()
                .install(|| {
                    rayon::scope(|scope| {
                        let (sender, receiver) = std::sync::mpsc::channel();
                        let receiver = std::sync::Arc::new(std::sync::Mutex::new(receiver));
                        sender
                            .send(alloc::vec![Materialized {
                                pointer: values.as_mut_ptr(),
                                len: values.len(),
                            }])
                            .unwrap();
                        let started_ref = &started;
                        scope.spawn(move |_| {
                            started_ref.store(true, std::sync::atomic::Ordering::Release);
                            consume_queue::<_, _, false>(
                                &receiver,
                                u128::MAX,
                                &mut crate::key::Direct,
                                TILE_BYTES,
                            );
                        });
                        while !started.load(std::sync::atomic::Ordering::Acquire) {
                            std::thread::yield_now();
                        }
                        panic!("producer after worker start");
                    });
                });
        }));
        assert!(result.is_err());
        assert!(started.load(std::sync::atomic::Ordering::Acquire));
        values.sort();
        assert_eq!(values, expected);
    }
}

#[cfg(all(test, not(miri)))]
mod safety_gap_tests;

#[cfg(all(test, not(miri)))]
mod progress_tests;
