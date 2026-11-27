// SPDX-License-Identifier: BSD-2-Clause

//! Execute numeric work serially or in the selected Rayon pool.

use std::sync::{Arc, Mutex};

static POOL: Mutex<Option<(usize, Arc<rayon::ThreadPool>)>> = Mutex::new(None);

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
            Self::Serial | Self::Global => Ok(f()),
            Self::Fixed(n) => {
                assert!(n >= 2, "fixed pools need at least two threads");
                let pool = {
                    let mut cached = POOL.lock().unwrap_or_else(|e| e.into_inner());
                    if cached.as_ref().is_none_or(|(width, _)| *width != n) {
                        *cached = Some((
                            n,
                            Arc::new(rayon::ThreadPoolBuilder::new().num_threads(n).build()?),
                        ));
                    }
                    Arc::clone(&cached.as_ref().unwrap().1)
                };
                // Work can reenter this cache or run alongside another caller.
                Ok(pool.install(f))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_and_global() {
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
        assert_eq!(
            Threads::Fixed(2).run(rayon::current_num_threads).unwrap(),
            2
        );
        let first = Arc::clone(
            &POOL
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .unwrap()
                .1,
        );
        Threads::Fixed(2)
            .run(|| {
                let cached = POOL
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
            &POOL
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
            POOL.lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .unwrap()
                .0,
            2
        );
    }
}
