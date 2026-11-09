// SPDX-License-Identifier: BSD-2-Clause

//! Warmed, caller-owned pools with synchronous worker shutdown.

use rayon::{ThreadPool, ThreadPoolBuilder};
use std::thread::JoinHandle;

/// Owns benchmark workers outside sort timing and allocation measurement.
pub struct BenchmarkPool {
    pool: Option<ThreadPool>,
    workers: Vec<JoinHandle<()>>,
}

impl BenchmarkPool {
    /// Starts and warms exactly `threads` workers.
    ///
    /// # Panics
    ///
    /// Panics if Rayon cannot create the pool.
    pub fn new(threads: usize) -> Self {
        let mut workers = Vec::with_capacity(threads);
        let pool = ThreadPoolBuilder::new()
            .num_threads(threads)
            .spawn_handler(|worker| {
                workers.push(std::thread::spawn(move || worker.run()));
                Ok(())
            })
            .build()
            .expect("create benchmark Rayon pool");
        pool.broadcast(|_| {});
        Self {
            pool: Some(pool),
            workers,
        }
    }

    /// Includes dispatch into the warmed pool in the caller's measured region.
    ///
    /// # Panics
    ///
    /// Propagates a panic from `work` to the caller.
    pub fn install<R: Send>(&self, work: impl FnOnce() -> R + Send) -> R {
        self.pool.as_ref().unwrap().install(work)
    }
}

impl Drop for BenchmarkPool {
    fn drop(&mut self) {
        // Rayon shutdown alone does not wait for worker/TLS cleanup. Linux may
        // still expose a retiring task entry briefly after these joins return.
        drop(self.pool.take());
        for worker in self.workers.drain(..) {
            worker.join().expect("join benchmark worker");
        }
    }
}
