// SPDX-License-Identifier: BSD-2-Clause
//! Test-only phase witnesses. The frozen algorithms are unchanged.
use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Phase {
    None,
    Root,
    Consumer,
}
thread_local! {
    static PHASE: Cell<Phase> = const { Cell::new(Phase::None) };
    static MOVED: Cell<bool> = const { Cell::new(false) };
    static HOOKS: RefCell<Option<Arc<Hooks>>> = const { RefCell::new(None) };
}
#[derive(Default)]
pub(crate) struct Hooks {
    pub late_callback_panic: bool,
    pub wait_for_child_panic: bool,
    pub progress_target: usize,
    pub progress_started: AtomicUsize,
    pub publication_count: AtomicUsize,
    pub sequence: AtomicUsize,
    pub publication: AtomicUsize,
    pub callbacks_after_publication: AtomicUsize,
    pub callback_panic: AtomicUsize,
    pub consumer_first_key: AtomicUsize,
    pub consumer_first_move: AtomicUsize,
    pub consumer_key_panic: AtomicUsize,
    pub sender_closed: AtomicUsize,
    pub consumer_finished: AtomicUsize,
    pub consumers_started: AtomicUsize,
    pub consumers_done: AtomicUsize,
    pub single_route: AtomicBool,
    pub root_key_panic: AtomicBool,
    pub root_calls: AtomicUsize,
    pub consumer_calls: AtomicUsize,
    signal: Mutex<()>,
    ready: Condvar,
}
impl Hooks {
    pub(crate) fn new(late_callback_panic: bool, wait_for_child_panic: bool) -> Self {
        Self {
            late_callback_panic,
            wait_for_child_panic,
            ..Self::default()
        }
    }
    fn tick(&self) -> usize {
        self.sequence.fetch_add(1, SeqCst) + 1
    }
    fn once(&self, target: &AtomicUsize) {
        if target.load(SeqCst) == 0 {
            let _ = target.compare_exchange(0, self.tick(), SeqCst, SeqCst);
        }
    }
    pub(crate) fn key(&self, fail_consumer: bool, fail_root: bool) {
        match phase() {
            Phase::Consumer => {
                self.consumer_calls.fetch_add(1, SeqCst);
                self.once(&self.consumer_first_key);
                if has_moved()
                    && fail_consumer
                    && self
                        .consumer_key_panic
                        .compare_exchange(0, self.tick(), SeqCst, SeqCst)
                        .is_ok()
                {
                    {
                        let _lock = self
                            .signal
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        self.ready.notify_all();
                    }
                    panic!("phase-gated consumer key panic after live record moves");
                }
            }
            Phase::Root => {
                self.root_calls.fetch_add(1, SeqCst);
                if has_moved() && fail_root && !self.root_key_panic.swap(true, SeqCst) {
                    panic!("phase-gated root key panic after completed scatter block");
                }
            }
            Phase::None => {}
        }
    }
}
pub(crate) fn current() -> Option<Arc<Hooks>> {
    HOOKS.with(|h| h.borrow().clone())
}
pub(crate) fn phase() -> Phase {
    PHASE.get()
}
pub(crate) fn has_moved() -> bool {
    MOVED.get()
}
pub(crate) fn with_hooks<R>(hooks: Arc<Hooks>, run: impl FnOnce() -> R) -> R {
    let previous = HOOKS.with(|h| h.replace(Some(hooks)));
    struct Reset(Option<Arc<Hooks>>);
    impl Drop for Reset {
        fn drop(&mut self) {
            HOOKS.with(|h| h.replace(self.0.take()));
        }
    }
    let _reset = Reset(previous);
    run()
}
pub(crate) struct PhaseGuard {
    phase: Phase,
    moved: bool,
    hooks: Option<Arc<Hooks>>,
    consumer: bool,
}
impl PhaseGuard {
    pub(crate) fn root() -> Self {
        Self::enter(Phase::Root, current(), false)
    }
    pub(crate) fn consumer(hooks: Option<Arc<Hooks>>) -> Self {
        Self::enter(Phase::Consumer, hooks, true)
    }
    fn enter(phase: Phase, hooks: Option<Arc<Hooks>>, consumer: bool) -> Self {
        if consumer {
            if let Some(h) = &hooks {
                h.consumers_started.fetch_add(1, SeqCst);
            }
        }
        let old_phase = PHASE.replace(phase);
        let old_moved = MOVED.replace(false);
        let old_hooks = HOOKS.with(|h| h.replace(hooks));
        Self {
            phase: old_phase,
            moved: old_moved,
            hooks: old_hooks,
            consumer,
        }
    }
}
impl Drop for PhaseGuard {
    fn drop(&mut self) {
        if self.consumer {
            if let Some(h) = current() {
                h.consumers_done.fetch_add(1, SeqCst);
                h.consumer_finished.store(h.tick(), SeqCst);
            }
        }
        PHASE.set(self.phase);
        MOVED.set(self.moved);
        HOOKS.with(|h| h.replace(self.hooks.take()));
    }
}
pub(crate) fn moved() {
    MOVED.set(true);
    if phase() == Phase::Consumer {
        if let Some(h) = current() {
            if h.consumer_first_move.load(SeqCst) == 0 {
                let _lock = h
                    .signal
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                h.once(&h.consumer_first_move);
                h.ready.notify_all();
            }
        }
    }
}
pub(crate) fn published(h: &Option<Arc<Hooks>>) {
    if let Some(h) = h {
        h.once(&h.publication);
        h.publication_count.fetch_add(1, SeqCst);
    }
}
pub(crate) fn callback_finished(h: &Option<Arc<Hooks>>) {
    let Some(h) = h else { return };
    if h.progress_target > 0 && h.publication_count.load(SeqCst) >= h.progress_target {
        let lock = h
            .signal
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (_lock, timeout) = h
            .ready
            .wait_timeout_while(lock, Duration::from_secs(20), |_| {
                h.progress_started.load(SeqCst) < h.progress_target
            })
            .unwrap();
        assert!(
            !timeout.timed_out(),
            "distinct consumers did not reach progress rendezvous"
        );
    }
    if (!h.late_callback_panic && !h.wait_for_child_panic) || h.publication.load(SeqCst) == 0 {
        return;
    }
    if h.callbacks_after_publication.fetch_add(1, SeqCst) == 0 {
        return;
    }
    // The second callback after publication is an intentional test rendezvous.
    // Root scatter/permutation have joined. Consumers have independent work,
    // no producer or receiver lock is held, and this is an OS wait, not a Rayon
    // yield that could recursively enter a receiver waiting for this sender.
    let lock = h
        .signal
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_lock, timeout) = h
        .ready
        .wait_timeout_while(lock, Duration::from_secs(20), |_| {
            if h.wait_for_child_panic {
                h.consumer_key_panic.load(SeqCst) == 0
            } else {
                h.consumer_first_move.load(SeqCst) == 0
            }
        })
        .unwrap();
    assert!(
        !timeout.timed_out(),
        "consumer failed to reach move witness within bounded test rendezvous"
    );
    assert!(h.consumer_first_key.load(SeqCst) > 0);
    if h.late_callback_panic {
        h.callback_panic.store(h.tick(), SeqCst);
        panic!("delayed producer callback panic after published child moved records");
    }
}
pub(crate) fn single_route(h: &Option<Arc<Hooks>>) {
    if let Some(h) = h {
        h.single_route.store(true, SeqCst);
    }
}
pub(crate) struct SenderGuard(Option<Arc<Hooks>>);
impl SenderGuard {
    pub(crate) fn new(h: Option<Arc<Hooks>>) -> Self {
        Self(h)
    }
}
impl Drop for SenderGuard {
    fn drop(&mut self) {
        if let Some(h) = &self.0 {
            h.sender_closed.store(h.tick(), SeqCst);
        }
    }
}

pub(crate) fn new_bucket() {
    if phase() == Phase::Consumer {
        MOVED.set(false);
    }
}

thread_local! {
    static PUBLISHING: Cell<bool> = const { Cell::new(false) };
    static QUEUE_LOCKED: Cell<bool> = const { Cell::new(false) };
}
pub(crate) struct PublishingGuard(bool);
impl PublishingGuard {
    pub(crate) fn new() -> Self {
        Self(PUBLISHING.replace(false))
    }
}
impl Drop for PublishingGuard {
    fn drop(&mut self) {
        PUBLISHING.set(self.0);
    }
}
pub(crate) fn publication_started() {
    PUBLISHING.set(true);
}
pub(crate) fn publication_closed() {
    PUBLISHING.set(false);
}
pub(crate) struct QueueGuard(bool);
impl QueueGuard {
    pub(crate) fn new() -> Self {
        Self(QUEUE_LOCKED.replace(true))
    }
}
impl Drop for QueueGuard {
    fn drop(&mut self) {
        QUEUE_LOCKED.set(self.0);
    }
}
pub(crate) fn assert_user_allowed() {
    assert!(
        !PUBLISHING.get(),
        "producer invoked user key while published consumers can wait"
    );
    assert!(!QUEUE_LOCKED.get(), "user key invoked under receiver lock");
}
impl Hooks {
    pub(crate) fn progress(target: usize) -> Self {
        Self {
            progress_target: target,
            ..Self::default()
        }
    }
    pub(crate) fn progress_arrive(&self) {
        let lock = self
            .signal
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.progress_started.fetch_add(1, SeqCst);
        self.ready.notify_all();
        let (_lock, timeout) = self
            .ready
            .wait_timeout_while(lock, Duration::from_secs(20), |_| {
                self.progress_started.load(SeqCst) < self.progress_target
            })
            .unwrap();
        assert!(
            !timeout.timed_out(),
            "nested consumers did not meet at progress rendezvous"
        );
    }
}
