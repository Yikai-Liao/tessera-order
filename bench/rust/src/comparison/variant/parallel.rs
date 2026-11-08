// SPDX-License-Identifier: BSD-2-Clause

//! Caller-owned Rayon pools prepared outside the timed and counted region.

use super::{BatchKernel, BenchmarkRecord, Kernel, PreparedRecords};
use rayon::{ThreadPool, ThreadPoolBuilder, prelude::*};
use std::{rc::Rc, thread::JoinHandle};

/// Owns a warmed pool and waits for all workers to exit before the next adapter.
struct Pool {
    pool: Option<ThreadPool>,
    workers: Vec<JoinHandle<()>>,
}
impl Pool {
    /// Starts every worker before timing or allocation counting begins.
    fn new(threads: usize) -> Self {
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

    /// Sorts inside the retained pool, including the dispatch in the timing.
    fn sort<T: BenchmarkRecord>(&self, records: &mut [T], adapter: &str) {
        self.pool.as_ref().unwrap().install(|| match adapter {
            "tessera-par" => T::tessera_parallel(records),
            "rayon-stable" => records.par_sort_by(T::order),
            _ => unreachable!("validated parallel adapter"),
        });
    }
}
impl Drop for Pool {
    fn drop(&mut self) {
        // Rayon signals shutdown on drop but does not join workers. An exit
        // callback still precedes TLS cleanup and OS thread exit, so join the
        // actual threads before the next census or allocation window.
        drop(self.pool.take());
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

/// Prepares a verified parallel batch with no global pool or timed pool creation.
pub(super) fn prepare<T: BenchmarkRecord>(
    input: Rc<PreparedRecords<T>>,
    adapter: &'static str,
    threads: usize,
) -> Box<dyn Kernel> {
    let pool = Pool::new(threads);
    Box::new(BatchKernel {
        input,
        buffers: Vec::new(),
        sort: move |records: &mut [T]| pool.sort(records, adapter),
        stable: true,
        worker_threads: threads,
    })
}
