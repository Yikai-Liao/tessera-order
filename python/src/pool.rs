// SPDX-License-Identifier: BSD-2-Clause

//! Execute numeric work serially or in the selected Rayon pool.

use std::ptr;
#[cfg(unix)]
use std::sync::atomic::AtomicU32;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Arc, Mutex};

struct ProcessPool {
    pid: u32,
    cache: Mutex<Option<(usize, Arc<rayon::ThreadPool>)>>,
}

static PROCESS_POOL: AtomicPtr<ProcessPool> = AtomicPtr::new(ptr::null_mut());
#[cfg(unix)]
static GLOBAL_PID: AtomicU32 = AtomicU32::new(0);

fn process_cache() -> &'static Mutex<Option<(usize, Arc<rayon::ThreadPool>)>> {
    let pid = std::process::id();
    loop {
        let ptr = PROCESS_POOL.load(Ordering::Acquire);
        if !ptr.is_null() {
            // SAFETY: PROCESS_POOL stores only null or intentionally leaked
            // Box<ProcessPool> pointers valid for the rest of the process.
            let pool = unsafe { &*ptr };
            if pool.pid == pid {
                return &pool.cache;
            }
        }
        let fresh = Box::into_raw(Box::new(ProcessPool {
            pid,
            cache: Mutex::new(None),
        }));
        match PROCESS_POOL.compare_exchange(ptr, fresh, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => {
                // Intentionally leak any inherited parent ProcessPool: neither
                // its mutex nor its Rayon ThreadPool may be locked or dropped
                // in a forked child where parent threads no longer exist.
                // SAFETY: fresh was just allocated and published for the process lifetime.
                return unsafe { &(*fresh).cache };
            }
            Err(_) => {
                // SAFETY: CAS failed, so fresh was never exposed to another thread.
                unsafe { drop(Box::from_raw(fresh)) };
            }
        }
    }
}

fn run_in_pool<R: Send>(
    width: usize,
    f: impl FnOnce() -> R + Send,
) -> Result<R, rayon::ThreadPoolBuildError> {
    assert!(width >= 1, "pools need at least one thread");
    let pool = {
        let mut cached = process_cache().lock().unwrap_or_else(|e| e.into_inner());
        if cached.as_ref().is_none_or(|(n, _)| *n != width) {
            *cached = Some((
                width,
                Arc::new(rayon::ThreadPoolBuilder::new().num_threads(width).build()?),
            ));
        }
        Arc::clone(&cached.as_ref().unwrap().1)
    };
    // Work can reenter this cache or run alongside another caller.
    Ok(pool.install(f))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Threads {
    Serial,
    Global,
    /// A cached pool of exactly this width; callers must supply at least two.
    Fixed(usize),
}

impl Threads {
    /// Runs `f`, returning a pool construction error before executing any work.
    pub(crate) fn run<R: Send>(
        &self,
        f: impl FnOnce() -> R + Send,
    ) -> Result<R, rayon::ThreadPoolBuildError> {
        match *self {
            Self::Serial => Ok(f()),
            Self::Global => {
                #[cfg(unix)]
                {
                    let pid = std::process::id();
                    let owner = GLOBAL_PID
                        .compare_exchange(0, pid, Ordering::Relaxed, Ordering::Relaxed)
                        .unwrap_or_else(|owner| owner);
                    if owner != 0 && owner != pid {
                        let width = std::env::var("RAYON_NUM_THREADS")
                            .ok()
                            .and_then(|value| value.parse::<usize>().ok())
                            .filter(|&n| n > 0)
                            .unwrap_or_else(|| {
                                std::thread::available_parallelism().map_or(1, usize::from)
                            });
                        return run_in_pool(width.max(1), f);
                    }
                }
                Ok(f())
            }
            Self::Fixed(n) => {
                assert!(n >= 2, "fixed pools need at least two threads");
                run_in_pool(n, f)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn serial_and_global() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let caller = std::thread::current().id();
        assert_eq!(
            Threads::Serial.run(|| std::thread::current().id()).unwrap(),
            caller
        );
        assert_eq!(
            Threads::Global.run(rayon::current_num_threads).unwrap(),
            rayon::current_num_threads()
        );
    }

    #[test]
    fn cache_reuse_replacement_and_unlocked_work() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(
            Threads::Fixed(2).run(rayon::current_num_threads).unwrap(),
            2
        );
        let first = Arc::clone(
            &process_cache()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .unwrap()
                .1,
        );
        Threads::Fixed(2)
            .run(|| {
                let cached = process_cache()
                    .try_lock()
                    .expect("install must release the cache lock");
                assert!(Arc::ptr_eq(&first, &cached.as_ref().unwrap().1));
            })
            .unwrap();
        assert_eq!(
            Threads::Fixed(3).run(rayon::current_num_threads).unwrap(),
            3
        );
        let replacement = Arc::clone(
            &process_cache()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .unwrap()
                .1,
        );
        assert!(!Arc::ptr_eq(&first, &replacement));
        assert_eq!(first.install(rayon::current_num_threads), 2);
        // A running call keeps its pool alive while another call replaces it.
        assert_eq!(
            Threads::Fixed(3)
                .run(|| Threads::Fixed(2).run(rayon::current_num_threads).unwrap())
                .unwrap(),
            2
        );
        assert_eq!(
            process_cache()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .unwrap()
                .0,
            2
        );
    }

    #[cfg(unix)]
    #[test]
    fn post_fork_skips_locked_parent_cache_and_global_pool() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved_pool = PROCESS_POOL.load(Ordering::Acquire);
        let saved_global = GLOBAL_PID.load(Ordering::Acquire);
        // Simulate a forked child inheriting a locked parent cache mutex and
        // an already-initialized parent Rayon global pool.
        let parent: &'static ProcessPool = Box::leak(Box::new(ProcessPool {
            pid: std::process::id().wrapping_add(1),
            cache: Mutex::new(None),
        }));
        let held = parent.cache.lock().unwrap();
        PROCESS_POOL.store(ptr::from_ref(parent).cast_mut(), Ordering::Release);
        GLOBAL_PID.store(std::process::id().wrapping_add(1), Ordering::Release);

        let fixed = Threads::Fixed(2).run(rayon::current_num_threads).unwrap();
        let global = Threads::Global.run(rayon::current_num_threads).unwrap();

        PROCESS_POOL.store(saved_pool, Ordering::Release);
        GLOBAL_PID.store(saved_global, Ordering::Release);
        drop(held);

        assert_eq!(fixed, 2);
        assert!(global >= 1);
    }
}
