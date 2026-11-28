// SPDX-License-Identifier: BSD-2-Clause

//! Caller-owned Rayon pools prepared outside the timed and counted region.

#[cfg(feature = "parallel-operations")]
use super::operations;
use super::{BatchKernel, CaseInput, Kernel, PreparedRecords, TesseraAdapter, Variant};
use crate::{plan::Operation, pool::BenchmarkPool};
use rayon::prelude::*;
use std::rc::Rc;

/// Prepares a verified parallel batch with no global pool or timed pool creation.
fn prepare<T: TesseraAdapter>(
    input: Rc<PreparedRecords<T>>,
    variant: Variant,
    threads: usize,
) -> Box<dyn Kernel> {
    let description = route::<T>(variant.name, input.operation());
    let pool = BenchmarkPool::new(threads);
    #[cfg(feature = "parallel-operations")]
    if matches!(input.operation(), Operation::Argsort {}) {
        return operations::parallel_indices(
            variant,
            input,
            description,
            threads,
            move |source: &[T]| {
                pool.install(|| match variant.name {
                    "tessera" => T::par_argsort(source),
                    "rayon-stable" => {
                        let mut indices: Vec<_> = (0..source.len()).collect();
                        indices.par_sort_by(|a, b| source[*a].order(&source[*b]));
                        indices
                    }
                    _ => unreachable!("validated parallel implementation"),
                })
            },
        );
    }
    let stable = variant.route_stable(input.operation());
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
fn route<T: TesseraAdapter>(implementation: &str, operation: &Operation) -> &'static str {
    match (implementation, operation) {
        ("tessera", Operation::Sort {}) => T::PARALLEL_SORT_ROUTE,
        ("rayon-stable", Operation::Sort {}) => "rayon par_sort_by in a caller-owned pool",
        #[cfg(feature = "parallel-operations")]
        ("tessera", Operation::Argsort {}) => T::PARALLEL_ARGSORT_ROUTE,
        #[cfg(feature = "parallel-operations")]
        ("rayon-stable", Operation::Argsort {}) => {
            "rayon par_sort_by on indices in a caller-owned pool"
        }
        _ => unreachable!("validated parallel implementation and operation"),
    }
}

impl CaseInput {
    /// Names a parallel column's call without starting its pool.
    pub(in crate::comparison) fn parallel_route(&self, implementation: &str) -> &'static str {
        match self {
            #[cfg(feature = "collections")]
            Self::Collection(_) => unreachable!(),
            Self::U32(v) => route::<u32>(implementation, v.operation()),
            Self::U64(v) => route::<u64>(implementation, v.operation()),
            Self::U128(v) => route::<u128>(implementation, v.operation()),
            Self::F64(v) => route::<f64>(implementation, v.operation()),
            Self::ReverseU64(v) => route::<std::cmp::Reverse<u64>>(implementation, v.operation()),
            Self::ReverseF64(v) => route::<std::cmp::Reverse<f64>>(implementation, v.operation()),
            Self::Row32(v) => route::<crate::case::Row<u32>>(implementation, v.operation()),
            Self::Row64(v) => route::<crate::case::Row<u64>>(implementation, v.operation()),
            Self::WideRecord(v) => route::<super::WideRecord>(implementation, v.operation()),
            Self::OwnedRecord(v) => route::<super::OwnedRecord>(implementation, v.operation()),
        }
    }

    /// Prepares an implementation's parallel operation in a `threads`-worker pool.
    pub(super) fn prepare_parallel(&self, variant: Variant, threads: usize) -> Box<dyn Kernel> {
        match self {
            #[cfg(feature = "collections")]
            Self::Collection(_) => unreachable!(),
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
