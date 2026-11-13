// SPDX-License-Identifier: BSD-2-Clause

//! Caller-owned Rayon pools prepared outside the timed and counted region.

use super::{KernelVisitor, PreparedRecords, TesseraAdapter};
use crate::pool::BenchmarkPool;
use rayon::prelude::*;
use std::rc::Rc;

/// Prepares a verified parallel batch with no global pool or timed pool creation.
pub(super) fn prepare<T: TesseraAdapter, V: KernelVisitor>(
    input: Rc<PreparedRecords<T>>,
    adapter: &'static str,
    threads: usize,
    visitor: V,
) -> V::Output {
    let pool = BenchmarkPool::new(threads);
    visitor.visit(input, true, threads, move |records: &mut [T]| {
        pool.install(|| match adapter {
            "tessera-par" => T::tessera_parallel(records),
            "rayon-stable" => records.par_sort_by(T::order),
            _ => unreachable!("validated parallel adapter"),
        })
    })
}
