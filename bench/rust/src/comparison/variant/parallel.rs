// SPDX-License-Identifier: BSD-2-Clause

//! Caller-owned Rayon pools prepared outside the timed and counted region.

use super::{BatchKernel, CaseInput, Kernel, PreparedRecords, TesseraAdapter, Variant};
use crate::pool::BenchmarkPool;
use rayon::prelude::*;
use std::rc::Rc;

/// Prepares a verified parallel batch with no global pool or timed pool creation.
fn prepare<T: TesseraAdapter>(
    input: Rc<PreparedRecords<T>>,
    variant: Variant,
    threads: usize,
) -> Box<dyn Kernel> {
    let description = route::<T>(variant.name);
    let stable = variant.route_stable(input.operation());
    let pool = BenchmarkPool::new(threads);
    Box::new(BatchKernel {
        input,
        buffers: Vec::new(),
        sort: move |records: &mut [T]| {
            pool.install(|| match variant.name {
                "tessera" => T::tessera_parallel(records),
                "rayon-stable" => records.par_sort_by(T::order),
                _ => unreachable!("validated parallel implementation"),
            })
        },
        stable,
        description,
        worker_threads: threads,
    })
}

/// The public call an implementation's parallel column makes for `T`.
fn route<T: TesseraAdapter>(implementation: &str) -> &'static str {
    match implementation {
        "tessera" => T::PARALLEL_SORT_ROUTE,
        "rayon-stable" => "rayon par_sort_by in a caller-owned pool",
        _ => unreachable!("validated parallel implementation"),
    }
}

impl CaseInput {
    /// Names a parallel column's call without starting its pool.
    pub(in crate::comparison) fn parallel_route(&self, implementation: &str) -> &'static str {
        match self {
            Self::U32(_) => route::<u32>(implementation),
            Self::U64(_) => route::<u64>(implementation),
            Self::U128(_) => route::<u128>(implementation),
            Self::F64(_) => route::<f64>(implementation),
            Self::ReverseU64(_) => route::<std::cmp::Reverse<u64>>(implementation),
            Self::ReverseF64(_) => route::<std::cmp::Reverse<f64>>(implementation),
            Self::Row32(_) => route::<crate::case::Row<u32>>(implementation),
            Self::Row64(_) => route::<crate::case::Row<u64>>(implementation),
            Self::WideRecord(_) => route::<super::WideRecord>(implementation),
            Self::OwnedRecord(_) => route::<super::OwnedRecord>(implementation),
        }
    }

    /// Prepares an implementation's parallel full sort in a `threads`-worker pool.
    pub(super) fn prepare_parallel(&self, variant: Variant, threads: usize) -> Box<dyn Kernel> {
        match self {
            Self::U32(v) => prepare(Rc::clone(v), variant, threads),
            Self::U64(v) => prepare(Rc::clone(v), variant, threads),
            Self::U128(v) => prepare(Rc::clone(v), variant, threads),
            Self::F64(v) => prepare(Rc::clone(v), variant, threads),
            Self::ReverseU64(v) => prepare(Rc::clone(v), variant, threads),
            Self::ReverseF64(v) => prepare(Rc::clone(v), variant, threads),
            Self::Row32(v) => prepare(Rc::clone(v), variant, threads),
            Self::Row64(v) => prepare(Rc::clone(v), variant, threads),
            Self::WideRecord(v) => prepare(Rc::clone(v), variant, threads),
            Self::OwnedRecord(v) => prepare(Rc::clone(v), variant, threads),
        }
    }
}
