// SPDX-License-Identifier: BSD-2-Clause

//! Caller-owned Rayon pools prepared outside the timed and counted region.

use super::{BatchKernel, CaseInput, Kernel, PreparedRecords, TesseraAdapter};
use crate::pool::BenchmarkPool;
use rayon::prelude::*;
use std::rc::Rc;

/// Prepares a verified parallel batch with no global pool or timed pool creation.
fn prepare<T: TesseraAdapter>(
    input: Rc<PreparedRecords<T>>,
    implementation: &'static str,
    threads: usize,
) -> Box<dyn Kernel> {
    let pool = BenchmarkPool::new(threads);
    Box::new(BatchKernel {
        input,
        buffers: Vec::new(),
        sort: move |records: &mut [T]| {
            pool.install(|| match implementation {
                "tessera" => T::tessera_parallel(records),
                "rayon-stable" => records.par_sort_by(T::order),
                _ => unreachable!("validated parallel implementation"),
            })
        },
        stable: true,
        description: super::PARALLEL_SORT_ROUTE,
        worker_threads: threads,
    })
}

impl CaseInput {
    /// Prepares an implementation's parallel full sort in a `threads`-worker pool.
    pub(super) fn prepare_parallel(
        &self,
        implementation: &'static str,
        threads: usize,
    ) -> Box<dyn Kernel> {
        match self {
            Self::U32(v) => prepare(Rc::clone(v), implementation, threads),
            Self::U64(v) => prepare(Rc::clone(v), implementation, threads),
            Self::U128(v) => prepare(Rc::clone(v), implementation, threads),
            Self::F64(v) => prepare(Rc::clone(v), implementation, threads),
            Self::ReverseU64(v) => prepare(Rc::clone(v), implementation, threads),
            Self::ReverseF64(v) => prepare(Rc::clone(v), implementation, threads),
            Self::Row32(v) => prepare(Rc::clone(v), implementation, threads),
            Self::Row64(v) => prepare(Rc::clone(v), implementation, threads),
            Self::WideRecord(v) => prepare(Rc::clone(v), implementation, threads),
            Self::OwnedRecord(v) => prepare(Rc::clone(v), implementation, threads),
        }
    }
}
