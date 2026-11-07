// SPDX-License-Identifier: BSD-2-Clause
//! Deterministic native tests of actual producer/consumer and narrow routes.
use crate::test_witness::{self, Hooks};
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering::SeqCst},
};
struct OwnedCell {
    key: u64,
    id: usize,
    text: String,
    visits: Cell<usize>,
    drops: Arc<Vec<AtomicUsize>>,
}
impl Drop for OwnedCell {
    fn drop(&mut self) {
        assert_eq!(self.text, self.id.to_string());
        assert_eq!(
            self.drops[self.id].fetch_add(1, SeqCst),
            0,
            "duplicate record drop"
        );
    }
}
fn value(id: usize, narrow: bool) -> u64 {
    if narrow {
        ((id * 37 + 11) % 16) as u64
    } else {
        (id as u64).wrapping_mul(0x9e3779b97f4a7c15)
    }
}
fn joined(h: &Hooks) {
    assert!(
        h.publication.load(SeqCst) > 0,
        "actual batch publication not witnessed"
    );
    assert!(
        h.consumers_started.load(SeqCst) > 0,
        "spawned consumers not witnessed"
    );
    assert_eq!(
        h.consumers_started.load(SeqCst),
        h.consumers_done.load(SeqCst),
        "scope returned with active consumer"
    );
    assert!(
        h.sender_closed.load(SeqCst) > 0,
        "sender destruction not witnessed"
    );
}
fn check_late(h: &Hooks) {
    joined(h);
    let published = h.publication.load(SeqCst);
    let first_key = h.consumer_first_key.load(SeqCst);
    let moved = h.consumer_first_move.load(SeqCst);
    let panic = h.callback_panic.load(SeqCst);
    assert!(published > 0 && published < first_key && first_key < moved && moved < panic);
    assert!(panic < h.sender_closed.load(SeqCst));
    assert!(h.callbacks_after_publication.load(SeqCst) >= 2);
}
#[cfg(not(miri))]
fn owned_case(narrow: bool, late: bool, key_panic: bool) {
    let len = 65_537;
    let drops = Arc::new((0..len).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>());
    let mut rows: Vec<_> = (0..len)
        .map(|id| OwnedCell {
            key: value(id, narrow),
            id,
            text: id.to_string(),
            visits: Cell::new(0),
            drops: drops.clone(),
        })
        .collect();
    let h = Arc::new(Hooks::new(late, key_panic && !narrow));
    let result = catch_unwind(AssertUnwindSafe(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .unwrap()
            .install(|| {
                test_witness::with_hooks(h.clone(), || {
                    crate::par_stable_sort_by_key(&mut rows, |r| {
                        h.key(key_panic && !narrow, key_panic && narrow);
                        r.visits.set(r.visits.get() + 1);
                        r.key
                    })
                });
            });
    }));
    assert_eq!(result.is_err(), late || key_panic);
    let mut ids: Vec<_> = rows
        .iter()
        .map(|r| {
            assert_eq!(r.text, r.id.to_string());
            r.id
        })
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, (0..len).collect::<Vec<_>>());
    assert!(drops.iter().all(|n| n.load(SeqCst) == 0));
    if narrow {
        assert!(h.single_route.load(SeqCst));
        assert!(h.root_calls.load(SeqCst) > 0);
        assert_eq!(h.consumer_calls.load(SeqCst), 0);
        assert_eq!(h.consumers_started.load(SeqCst), 0);
        assert_eq!(h.root_key_panic.load(SeqCst), key_panic);
    } else if late {
        check_late(&h);
    } else {
        joined(&h);
        if key_panic {
            assert!(h.consumer_first_move.load(SeqCst) > 0);
            assert!(h.consumer_key_panic.load(SeqCst) > h.consumer_first_move.load(SeqCst));
            assert!(!h.root_key_panic.load(SeqCst));
        }
    }
    if !late && !key_panic {
        let actual: Vec<_> = rows.iter().map(|r| (r.key, r.id)).collect();
        let mut expected: Vec<_> = (0..len).map(|id| (value(id, narrow), id)).collect();
        expected.sort_by_key(|r| r.0);
        assert_eq!(actual, expected);
    }
    drop(rows);
    assert!(drops.iter().all(|n| n.load(SeqCst) == 1));
}
#[cfg(not(miri))]
fn exclusive_case(narrow: bool, late: bool, key_panic: bool) {
    let len = 65_537;
    let mut backing: Vec<_> = (0..len).collect();
    let mut rows: Vec<_> = backing
        .iter_mut()
        .enumerate()
        .map(|(id, r)| (value(id, narrow), id, r))
        .collect();
    let h = Arc::new(Hooks::new(late, key_panic && !narrow));
    let result = catch_unwind(AssertUnwindSafe(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .unwrap()
            .install(|| {
                test_witness::with_hooks(h.clone(), || {
                    crate::par_stable_sort_by_key(&mut rows, |r| {
                        h.key(key_panic && !narrow, key_panic && narrow);
                        r.0
                    })
                });
            });
    }));
    assert_eq!(result.is_err(), late || key_panic);
    if narrow {
        assert!(h.single_route.load(SeqCst));
        assert_eq!(h.consumers_started.load(SeqCst), 0);
        assert_eq!(h.root_key_panic.load(SeqCst), key_panic);
    } else if late {
        check_late(&h);
    } else {
        joined(&h);
        if key_panic {
            assert!(h.consumer_key_panic.load(SeqCst) > h.consumer_first_move.load(SeqCst));
            assert!(h.consumer_first_move.load(SeqCst) > 0);
        }
    }
    let actual: Vec<_> = rows.iter().map(|r| (r.0, r.1)).collect();
    if !late && !key_panic {
        let mut expected: Vec<_> = (0..len).map(|id| (value(id, narrow), id)).collect();
        expected.sort_by_key(|r| r.0);
        assert_eq!(actual, expected);
    }
    let mut ids = Vec::new();
    for (_, id, r) in rows {
        assert_eq!(*r, id);
        *r += 1;
        ids.push(id);
    }
    ids.sort_unstable();
    assert_eq!(ids, (0..len).collect::<Vec<_>>());
    assert_eq!(backing, (1..=len).collect::<Vec<_>>());
}
#[test]
#[cfg(not(miri))]
fn delayed_callback_panic_owned_cell_published_consumer_moved() {
    owned_case(false, true, false);
}
#[test]
#[cfg(not(miri))]
fn child_key_panic_owned_cell_after_consumer_move() {
    owned_case(false, false, true);
}
#[test]
#[cfg(not(miri))]
fn combined_owned_cell_success() {
    owned_case(false, false, false);
}
#[test]
#[cfg(not(miri))]
fn delayed_callback_panic_exclusive_published_consumer_moved() {
    exclusive_case(false, true, false);
}
#[test]
#[cfg(not(miri))]
fn child_key_panic_exclusive_after_consumer_move() {
    exclusive_case(false, false, true);
}
#[test]
#[cfg(not(miri))]
fn combined_exclusive_success() {
    exclusive_case(false, false, false);
}
#[test]
#[cfg(not(miri))]
fn single_digit_owned_cell_success() {
    owned_case(true, false, false);
}
#[test]
#[cfg(not(miri))]
fn single_digit_owned_cell_root_panic_after_moves() {
    owned_case(true, false, true);
}
#[test]
#[cfg(not(miri))]
fn single_digit_exclusive_success() {
    exclusive_case(true, false, false);
}
#[test]
#[cfg(not(miri))]
fn single_digit_exclusive_root_panic_after_moves() {
    exclusive_case(true, false, true);
}

#[test]
fn simultaneous_producer_consumer_panic_owned_cell() {
    super::progress_tests::bounded("engine::parallel::safety_gap_tests::simultaneous_producer_consumer_panic_owned_cell", || owned_case(false,true,true));
}
#[test]
fn simultaneous_producer_consumer_panic_exclusive() {
    super::progress_tests::bounded("engine::parallel::safety_gap_tests::simultaneous_producer_consumer_panic_exclusive", || exclusive_case(false,true,true));
}
