// SPDX-License-Identifier: BSD-2-Clause
//! Bounded tests for cooperating nested Rayon work, not arbitrary user blocking.
use crate::test_witness::{self, Hooks, Phase};
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering::SeqCst},
};
use std::time::{Duration, Instant};

pub(crate) fn bounded(name: &str, run: impl FnOnce()) {
    if std::env::var("TESSERA_PROGRESS_CHILD").as_deref() == Ok(name) {
        run();
        return;
    }
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env("TESSERA_PROGRESS_CHILD", name)
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(35);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "bounded child failed: {status}");
            return;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("nested public sort exceeded 35-second subprocess deadline: {name}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn key(id: usize) -> u64 {
    (id as u64).wrapping_mul(0x9e3779b97f4a7c15)
}
struct Nested {
    hooks: Arc<Hooks>,
    workers: AtomicUsize,
    claimed: AtomicUsize,
    active: AtomicUsize,
    peak: AtomicUsize,
    completed: AtomicUsize,
}
impl Nested {
    fn new(target: usize) -> Self {
        Self {
            hooks: Arc::new(Hooks::progress(target)),
            workers: AtomicUsize::new(0),
            claimed: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
            completed: AtomicUsize::new(0),
        }
    }
    fn invoke(&self, fail: bool) {
        test_witness::assert_user_allowed();
        if test_witness::phase() != Phase::Consumer {
            return;
        }
        let worker = rayon::current_thread_index().unwrap();
        if self.workers.fetch_or(1 << worker, SeqCst) & (1 << worker) != 0 {
            return;
        }
        if self.claimed.fetch_add(1, SeqCst) >= self.hooks.progress_target {
            return;
        }
        let active = self.active.fetch_add(1, SeqCst) + 1;
        self.peak.fetch_max(active, SeqCst);
        self.hooks.progress_arrive();
        // Isolate the nested invocation's instrumentation from the outer test's
        // rendezvous. These public calls still use the same fixed Rayon pool.
        test_witness::with_hooks(Arc::new(Hooks::default()), || {
            let mut direct: Vec<_> = (0..131_073).map(key).collect();
            crate::par_stable_sort(&mut direct);
            assert!(direct.windows(2).all(|w| w[0] <= w[1]));
            let (left, right) = rayon::join(
                || {
                    let mut rows: Vec<_> = (0..65_537).map(|id| (key(id), id)).collect();
                    crate::par_stable_sort_by_key(&mut rows, |r| {
                        test_witness::assert_user_allowed();
                        r.0
                    });
                    assert!(rows.windows(2).all(|w| w[0].0 <= w[1].0));
                    rows.len()
                },
                || {
                    let (a, b) = rayon::join(|| 17, || 25);
                    a + b
                },
            );
            assert_eq!(left, 65_537);
            assert_eq!(right, 42);
        });
        self.completed.fetch_add(1, SeqCst);
        self.active.fetch_sub(1, SeqCst);
        if fail {
            rayon::join(|| panic!("nested join key panic"), || ());
        }
    }
    fn verify(&self) {
        assert!(self.workers.load(SeqCst).count_ones() as usize >= self.hooks.progress_target);
        assert_eq!(self.completed.load(SeqCst), self.hooks.progress_target);
        assert_eq!(self.active.load(SeqCst), 0);
        assert!(self.peak.load(SeqCst) >= self.hooks.progress_target);
        assert_eq!(
            self.hooks.consumers_started.load(SeqCst),
            self.hooks.consumers_done.load(SeqCst)
        );
        assert!(self.hooks.sender_closed.load(SeqCst) > 0);
    }
}
struct Owned {
    key: u64,
    id: usize,
    text: String,
    visits: Cell<usize>,
    drops: Arc<Vec<AtomicUsize>>,
}
impl Drop for Owned {
    fn drop(&mut self) {
        assert_eq!(self.text, self.id.to_string());
        assert_eq!(self.drops[self.id].fetch_add(1, SeqCst), 0);
    }
}
fn owned(threads: usize, fail: bool) {
    let n = 131_073;
    let drops = Arc::new((0..n).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>());
    let mut rows: Vec<_> = (0..n)
        .map(|id| Owned {
            key: key(id),
            id,
            text: id.to_string(),
            visits: Cell::new(0),
            drops: drops.clone(),
        })
        .collect();
    let nested = Nested::new((threads - 1).min(2));
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .unwrap();
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        pool.install(|| {
            test_witness::with_hooks(nested.hooks.clone(), || {
                crate::par_stable_sort_by_key(&mut rows, |r| {
                    nested.invoke(fail);
                    r.visits.set(r.visits.get() + 1);
                    r.key
                });
            })
        })
    }));
    assert_eq!(outcome.is_err(), fail);
    nested.verify();
    let mut ids: Vec<_> = rows
        .iter()
        .map(|r| {
            assert_eq!(r.text, r.id.to_string());
            r.id
        })
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, (0..n).collect::<Vec<_>>());
    if !fail {
        assert!(
            rows.windows(2)
                .all(|w| (w[0].key, w[0].id) <= (w[1].key, w[1].id))
        );
    }
    assert!(drops.iter().all(|x| x.load(SeqCst) == 0));
    drop(rows);
    assert!(drops.iter().all(|x| x.load(SeqCst) == 1));
}
fn exclusive(threads: usize) {
    let n = 131_073;
    let mut backing: Vec<_> = (0..n).collect();
    let mut rows: Vec<_> = backing
        .iter_mut()
        .enumerate()
        .map(|(id, r)| (key(id), id, r))
        .collect();
    let nested = Nested::new((threads - 1).min(2));
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .unwrap()
        .install(|| {
            test_witness::with_hooks(nested.hooks.clone(), || {
                crate::par_stable_sort_by_key(&mut rows, |r| {
                    nested.invoke(false);
                    r.0
                });
            })
        });
    nested.verify();
    assert!(
        rows.windows(2)
            .all(|w| (w[0].0, w[0].1) <= (w[1].0, w[1].1))
    );
    for (_, id, r) in rows {
        assert_eq!(*r, id);
        *r += 1;
    }
    assert_eq!(backing, (1..=n).collect::<Vec<_>>());
}
#[test]
fn nested_public_sorts_and_joins_owned_cell() {
    bounded(
        "engine::parallel::progress_tests::nested_public_sorts_and_joins_owned_cell",
        || owned(4, false),
    );
}
#[test]
fn nested_public_sorts_and_joins_exclusive() {
    bounded(
        "engine::parallel::progress_tests::nested_public_sorts_and_joins_exclusive",
        || exclusive(4),
    );
}
#[test]
fn nested_join_panics_restore_owned_cell() {
    bounded(
        "engine::parallel::progress_tests::nested_join_panics_restore_owned_cell",
        || owned(4, true),
    );
}

#[test]
fn nested_public_sorts_and_joins_owned_cell_p2() {
    bounded(
        "engine::parallel::progress_tests::nested_public_sorts_and_joins_owned_cell_p2",
        || owned(2, false),
    );
}
#[test]
fn nested_public_sorts_and_joins_owned_cell_p3() {
    bounded(
        "engine::parallel::progress_tests::nested_public_sorts_and_joins_owned_cell_p3",
        || owned(3, false),
    );
}
#[test]
fn nested_public_sorts_and_joins_exclusive_p2() {
    bounded(
        "engine::parallel::progress_tests::nested_public_sorts_and_joins_exclusive_p2",
        || exclusive(2),
    );
}
#[test]
fn nested_public_sorts_and_joins_exclusive_p3() {
    bounded(
        "engine::parallel::progress_tests::nested_public_sorts_and_joins_exclusive_p3",
        || exclusive(3),
    );
}
#[test]
fn nested_join_panics_restore_owned_cell_p2() {
    bounded(
        "engine::parallel::progress_tests::nested_join_panics_restore_owned_cell_p2",
        || owned(2, true),
    );
}
#[test]
fn nested_join_panics_restore_owned_cell_p3() {
    bounded(
        "engine::parallel::progress_tests::nested_join_panics_restore_owned_cell_p3",
        || owned(3, true),
    );
}
