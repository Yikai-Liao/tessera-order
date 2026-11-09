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

#[cfg(feature = "std")]
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

// A separate process is needed because a panic in the no-std movement guard
// must abort even when the host supplies an unwinder.
#[cfg(all(not(feature = "std"), not(miri)))]
#[test]
fn no_std_block_panic_aborts() {
    const CHILD: &str = "TESSERA_NO_STD_PANIC_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let build = || {
            (0..8192usize)
                .map(|id| {
                    let key = if id < 64 {
                        0
                    } else {
                        id.wrapping_mul(73) as u8
                    };
                    (key, format!("record {id}"))
                })
                .collect::<Vec<_>>()
        };
        let mut probe = build();
        let mut total = 0;
        let mut key = |x: &(u8, String)| {
            total += 1;
            x.0
        };
        stable_sort_by_key(&mut probe, &mut key);
        drop(probe);

        let mut input = build();
        let mut calls = 0;
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let mut key = |x: &(u8, String)| {
                calls += 1;
                // The middle of this narrow-key sort is the block scatter,
                // after consumed input blocks have become output blocks.
                assert_ne!(calls, total / 2, "no-std block panic probe");
                x.0
            };
            stable_sort_by_key(&mut input, &mut key);
        }));
        // If unwinding escapes the guard, report a successful child exit so
        // the parent fails. Do not drop possibly duplicated owning records.
        std::process::exit(0);
    }

    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "no_std_block_panic_aborts", "--nocapture"])
        .env(CHILD, "1")
        .env("RUST_BACKTRACE", "0")
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "panic escaped the guard");
    assert!(stderr.contains("no-std block panic probe"), "{stderr}");
    assert!(
        stderr.contains("cannot unwind through record movement without the std feature"),
        "{stderr}"
    );
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(output.status.signal(), Some(6), "{stderr}");
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
            // Each mode sample picks an absent pivot; all records enter one
            // non-equal band, and its scan restores the same candidate mask.
            let phase = (call - prelude) % (64 + len + 65);
            if phase < 64 {
                if greater { 0 } else { u64::MAX }
            } else if phase < 64 + len {
                1
            } else if phase % 2 == 0 {
                0
            } else {
                u64::MAX
            }
        });
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
            if panic && !cfg!(feature = "std") {
                continue;
            }
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
