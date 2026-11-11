// SPDX-License-Identifier: BSD-2-Clause

//! Record preservation when a key function panics or returns inconsistent
//! keys, for records that own resources and records that hold `&mut`.

use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use tessera_sort::stable_sort_by_key;

struct Owned {
    id: usize,
    text: String,
    edited: Cell<bool>,
    drops: Rc<RefCell<Vec<usize>>>,
}
impl Drop for Owned {
    fn drop(&mut self) {
        self.drops.borrow_mut().push(self.id);
    }
}

fn exercise(len: usize, changing: bool) {
    let drops = Rc::new(RefCell::new(Vec::new()));
    let build = || {
        (0..len)
            .map(|id| Owned {
                id,
                text: format!("record {id}"),
                edited: Cell::new(false),
                drops: drops.clone(),
            })
            .collect::<Vec<_>>()
    };
    let original_key = |id: usize| (id as u64).wrapping_mul(0x9e3779b97f4a7c15);
    let mut probe = build();
    let mut total = 0;
    stable_sort_by_key(&mut probe, |x| {
        total += 1;
        original_key(x.id)
    });
    drop(probe);
    let steps = if cfg!(miri) { 5 } else { 40 };
    for step in 0..steps {
        drops.borrow_mut().clear();
        let mut input = build();
        let at = 1 + step * total / steps;
        let mut calls = 0;
        let result = catch_unwind(AssertUnwindSafe(|| {
            stable_sort_by_key(&mut input, |x| {
                calls += 1;
                x.edited.set(true);
                if !changing && calls == at {
                    std::panic::panic_any(0x71u32);
                }
                if changing && calls >= at {
                    calls as u64 % 7
                } else {
                    original_key(x.id)
                }
            });
        }));
        if !changing {
            assert_eq!(*result.unwrap_err().downcast::<u32>().unwrap(), 0x71);
            assert_eq!(calls, at, "called key after it panicked");
            assert!(input.iter().any(|x| x.edited.get()));
        }
        assert!(drops.borrow().is_empty());
        input.sort_by_key(|x| x.id);
        for (id, x) in input.iter().enumerate() {
            assert_eq!(x.id, id);
            assert_eq!(x.text, format!("record {id}"));
        }
        // Recovered records remain usable with a different key width.
        stable_sort_by_key(&mut input, |x| (x.id as u128) << 64);
        assert!(input.iter().enumerate().all(|(id, x)| x.id == id));
        drop(input);
        drops.borrow_mut().sort_unstable();
        assert_eq!(*drops.borrow(), (0..len).collect::<Vec<_>>());
    }
}

#[test]
fn key_panic_recovery() {
    for len in if cfg!(miri) {
        vec![12, 47, 48, 49, 80]
    } else {
        vec![12, 47, 48, 49, 80, 5000]
    } {
        exercise(len, false);
    }
}

#[test]
fn changing_keys_owned_records() {
    for len in if cfg!(miri) {
        vec![47, 48, 49, 80]
    } else {
        vec![47, 48, 49, 80, 5000]
    } {
        exercise(len, true);
    }
}

#[test]
#[cfg(not(miri))]
fn changing_pivot_keeps_records() {
    let len = 20_000;
    for greater in [false, true] {
        let drops = Rc::new(RefCell::new(Vec::new()));
        let mut records: Vec<_> = (0..len)
            .map(|id| Owned {
                id,
                text: format!("record {id}"),
                edited: Cell::new(false),
                drops: drops.clone(),
            })
            .collect();
        let probe_len = 65_536 / std::mem::size_of::<Owned>();
        let prelude = 4 + probe_len + 1025;
        let mut first_band = vec![false; len];
        let mut calls = 0;
        stable_sort_by_key(&mut records, |record| {
            assert!(
                calls < 2_000_000,
                "equality recursion did not make progress"
            );
            let call = calls;
            calls += 1;
            record.edited.set(true);
            // Reject the order and insertion probes, then expose all key bits.
            if call < 4 {
                return [1, 0, 0, 1][call];
            }
            if call < 4 + probe_len {
                return u64::from(call % 2 == 0);
            }
            if call < prelude {
                return if call % 2 == 0 { 0 } else { u64::MAX };
            }
            // Reject each router gate before its mode sample picks an absent
            // pivot. All records then enter one non-equal band; without the
            // digit fallback, the child scan restores the same candidate mask.
            let offset = call - prelude;
            // The root was already probed. Subsequent children reject order
            // before scanning their remaining candidate bits.
            let phase = offset % (64 + len + 4 + 65);
            if phase < 64 {
                if greater { 0 } else { u64::MAX }
            } else if phase < 64 + len {
                if offset < 64 + len {
                    assert!(!first_band[record.id], "record classified twice");
                    first_band[record.id] = true;
                }
                1
            } else if phase < 64 + len + 4 {
                [1, 0, 0, 1][phase - 64 - len]
            } else if phase.is_multiple_of(2) {
                0
            } else {
                u64::MAX
            }
        });
        // The absent sampled pivot and every classified key were different:
        // the first equality partition really produced an unchanged outer band.
        assert!(first_band.iter().all(|&seen| seen));
        assert!(drops.borrow().is_empty());
        records.sort_by_key(|record| record.id);
        for (id, record) in records.iter().enumerate() {
            assert_eq!(record.id, id);
            assert_eq!(record.text, format!("record {id}"));
            assert!(record.edited.get());
        }
        drop(records);
        drops.borrow_mut().sort_unstable();
        assert_eq!(*drops.borrow(), (0..len).collect::<Vec<_>>());
    }
}

#[test]
fn changing_keys_borrowed_records() {
    struct Borrowed<'a> {
        id: usize,
        value: &'a mut usize,
    }
    let original_key = |id: usize| (id as u64).wrapping_mul(0x9e3779b97f4a7c15);
    for len in [47, 48, 49, if cfg!(miri) { 96 } else { 9000 }] {
        // Match the borrowed record shape so the probe takes the same tile route.
        let mut total = 0;
        {
            let mut backing: Vec<_> = (0..len).collect();
            let mut probe: Vec<_> = backing
                .iter_mut()
                .enumerate()
                .map(|(id, value)| Borrowed { id, value })
                .collect();
            stable_sort_by_key(&mut probe, |record| {
                total += 1;
                original_key(record.id)
            });
        }
        for panic in [false, true] {
            for at in [1, 24, 180, 500, 1200] {
                let mut backing: Vec<_> = (0..len).collect();
                let mut records: Vec<_> = backing
                    .iter_mut()
                    .enumerate()
                    .map(|(id, value)| Borrowed { id, value })
                    .collect();
                let mut calls = 0;
                let result = catch_unwind(AssertUnwindSafe(|| {
                    stable_sort_by_key(&mut records, |x| {
                        calls += 1;
                        if panic && calls == at {
                            std::panic::panic_any(0x72u32);
                        }
                        if calls >= at {
                            calls as u64 % 3
                        } else {
                            original_key(x.id)
                        }
                    });
                }));
                if panic && at <= total {
                    assert_eq!(*result.unwrap_err().downcast::<u32>().unwrap(), 0x72);
                    assert_eq!(calls, at, "called key after it panicked");
                } else {
                    assert!(result.is_ok(), "unexpected sort panic");
                }
                let mut ids: Vec<_> = records.iter().map(|x| x.id).collect();
                ids.sort_unstable();
                assert_eq!(ids, (0..len).collect::<Vec<_>>());
                for record in &mut records {
                    assert_eq!(*record.value, record.id);
                    *record.value += len;
                }
                drop(records);
                assert_eq!(backing, (len..2 * len).collect::<Vec<_>>());
            }
        }
    }
}
