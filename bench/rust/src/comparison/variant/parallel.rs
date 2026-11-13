// SPDX-License-Identifier: BSD-2-Clause

//! Caller-owned Rayon pools prepared outside the timed and counted region.

use super::{BatchKernel, Kernel, PreparedRecords, TesseraAdapter};
use crate::pool::BenchmarkPool;
use rayon::prelude::*;
use std::rc::Rc;

/// Prepares a verified parallel batch with no global pool or timed pool creation.
pub(super) fn prepare<T: TesseraAdapter>(
    input: Rc<PreparedRecords<T>>,
    adapter: &'static str,
    threads: usize,
) -> Box<dyn Kernel> {
    let pool = BenchmarkPool::new(threads);
    Box::new(BatchKernel {
        input,
        buffers: Vec::new(),
        sort: move |records: &mut [T]| {
            pool.install(|| match adapter {
                "tessera-par" => T::tessera_parallel(records),
                "rayon-stable" => records.par_sort_by(T::order),
                _ => unreachable!("validated parallel adapter"),
            })
        },
        stable: true,
        required: None,
        description: "caller-owned pool and public sorting call",
        worker_threads: threads,
    })
}
