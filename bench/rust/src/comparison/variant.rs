// SPDX-License-Identifier: BSD-2-Clause

//! Operation adapters and restorable, verified batch kernels.

#[cfg(feature = "operations")]
mod operations;
#[cfg(feature = "rayon")]
mod parallel;

use super::{
    check,
    record::{BenchmarkRecord, CaseInput, OwnedRecord, PreparedRecords, WideRecord},
};
use crate::case::Row;
#[cfg(any(test, feature = "operations"))]
use crate::plan::Operation;
use rdst::RadixSort;
use std::{cmp::Reverse, hint::black_box, mem::MaybeUninit, rc::Rc};
use voracious_radix_sort::{RadixSort as VoraciousSort, Radixable};

/// Type-specific Tessera entry points, separate from record identity and ordering.
pub(super) trait TesseraAdapter: BenchmarkRecord {
    /// Sorts scalar keys directly and keyed records through the by-key API.
    fn tessera(records: &mut [Self]);
    /// Current serial slice operations; historical sorting adapters omit these APIs.
    #[cfg(feature = "operations")]
    fn argsort(records: &[Self]) -> Vec<usize>;
    #[cfg(feature = "operations")]
    fn segments(records: &mut [Self], offsets: &[usize]);
    #[cfg(feature = "operations")]
    fn prefix(records: &mut [Self], len: usize);
    #[cfg(feature = "operations")]
    fn top_k(records: &mut [Self], k: usize);
    #[cfg(feature = "operations")]
    fn select(records: &mut [Self], index: usize);
    #[cfg(feature = "operations")]
    fn rank_range(records: &mut [Self], ranks: std::ops::Range<usize>);
    #[cfg(feature = "operations")]
    fn select_many(records: &mut [Self], indices: &[usize]);
    #[cfg(feature = "operations")]
    fn prefix_unstable(records: &mut [Self], len: usize);
    #[cfg(feature = "operations")]
    fn top_k_unstable(records: &mut [Self], k: usize);
    #[cfg(feature = "operations")]
    fn select_unstable(records: &mut [Self], index: usize);
    #[cfg(feature = "operations")]
    fn range_unstable(records: &mut [Self], ranks: std::ops::Range<usize>);
    #[cfg(feature = "operations")]
    fn many_unstable(records: &mut [Self], indices: &[usize]);
    /// Calls the complete serial API under the weak tie contract.
    #[cfg(feature = "operations")]
    fn tessera_unstable(records: &mut [Self]);
    /// Sorts through the independently named baseline crate.
    #[cfg(tessera_bench_revisions)]
    fn baseline(records: &mut [Self]);
    /// Sorts through the second baseline placement.
    #[cfg(tessera_bench_revisions)]
    fn baseline_copy(records: &mut [Self]);
    /// Sorts through the second current placement.
    #[cfg(tessera_bench_revisions)]
    fn current_copy(records: &mut [Self]);
    /// Reports distinct relative adapter addresses, independent of process ASLR.
    #[cfg(tessera_bench_revisions)]
    fn revision_entries() -> Vec<String> {
        let entries = [
            Self::tessera as *const () as usize,
            Self::baseline as *const () as usize,
            Self::baseline_copy as *const () as usize,
            Self::current_copy as *const () as usize,
        ];
        assert!(
            (0..entries.len()).all(|i| !entries[..i].contains(&entries[i])),
            "revision adapters were folded together"
        );
        entries
            .iter()
            .map(|entry| format!("{:#x}", entry.wrapping_sub(entries[0])))
            .collect()
    }
    /// Uses the same direct/by-key distinction for caller-owned parallel sorting.
    #[cfg(feature = "rayon")]
    fn tessera_parallel(records: &mut [Self]);
}

/// Calls the same public API with identical record and key types in every copy.
#[cfg(tessera_bench_revisions)]
macro_rules! revision_sorts {
    ($api:ident $(, $key:expr)?) => {
        #[inline(never)]
        fn baseline(records: &mut [Self]) { revision_baseline::$api(records $(, $key)?); }
        #[inline(never)]
        fn baseline_copy(records: &mut [Self]) { revision_baseline_copy::$api(records $(, $key)?); }
        #[inline(never)]
        fn current_copy(records: &mut [Self]) { revision_current_copy::$api(records $(, $key)?); }
    };
}

/// Supplies only the direct/by-key API distinction; operation behavior stays
/// in the library and kernel preparation owns the workload/output shape.
macro_rules! slice_operations {
    ($argsort:ident, $segments:ident, $prefix:ident, $top_k:ident, $select:ident, $range:ident, $many:ident $(, $key:expr)?) => {
        #[cfg(feature = "operations")]
        fn argsort(records: &[Self]) -> Vec<usize> { tessera_order::$argsort(records $(, $key)?) }
        #[cfg(feature = "operations")]
        fn segments(records: &mut [Self], offsets: &[usize]) { tessera_order::$segments(records, offsets $(, $key)?); }
        #[cfg(feature = "operations")]
        fn prefix(records: &mut [Self], len: usize) { tessera_order::$prefix(records, len $(, $key)?); }
        #[cfg(feature = "operations")]
        fn top_k(records: &mut [Self], k: usize) { tessera_order::$top_k(records, k $(, $key)?); }
        #[cfg(feature = "operations")]
        fn select(records: &mut [Self], index: usize) { tessera_order::$select(records, index $(, $key)?); }
        #[cfg(feature = "operations")]
        fn rank_range(records: &mut [Self], ranks: std::ops::Range<usize>) { tessera_order::$range(records, ranks $(, $key)?); }
        #[cfg(feature = "operations")]
        fn select_many(records: &mut [Self], indices: &[usize]) { tessera_order::$many(records, indices $(, $key)?); }
    };
}

/// Supplies direct/by-key API pairs without owning algorithm decisions.
macro_rules! unstable_rank_operations {
    ($prefix:ident, $top_k:ident, $select:ident, $range:ident, $many:ident $(, $key:expr)?) => {
        #[cfg(feature = "operations")]
        fn prefix_unstable(records: &mut [Self], len: usize) { tessera_order::$prefix(records, len $(, $key)?); }
        #[cfg(feature = "operations")]
        fn top_k_unstable(records: &mut [Self], k: usize) { tessera_order::$top_k(records, k $(, $key)?); }
        #[cfg(feature = "operations")]
        fn select_unstable(records: &mut [Self], index: usize) { tessera_order::$select(records, index $(, $key)?); }
        #[cfg(feature = "operations")]
        fn range_unstable(records: &mut [Self], ranks: std::ops::Range<usize>) { tessera_order::$range(records, ranks $(, $key)?); }
        #[cfg(feature = "operations")]
        fn many_unstable(records: &mut [Self], indices: &[usize]) { tessera_order::$many(records, indices $(, $key)?); }
    };
}

/// Keeps scalar adapters on Tessera's direct-key entry points.
macro_rules! tessera_keys {
    ($($t:ty),+ $(,)?) => {$(
        impl TesseraAdapter for $t {
            #[cfg_attr(tessera_bench_revisions, inline(never))]
            fn tessera(records: &mut [Self]) {
                tessera_order::stable_sort(records);
            }
            #[cfg(feature = "operations")]
            fn tessera_unstable(records: &mut [Self]) {
                tessera_order::unstable_sort(records);
            }
            slice_operations!(stable_argsort, stable_sort_segments, stable_sort_prefix, stable_top_k, stable_select_nth, stable_sort_rank_range, stable_select_nth_many);
            unstable_rank_operations!(unstable_sort_prefix, unstable_top_k, unstable_select_nth, unstable_sort_rank_range, unstable_select_nth_many);
            #[cfg(tessera_bench_revisions)]
            revision_sorts!(stable_sort);
            #[cfg(feature = "rayon")]
            fn tessera_parallel(records: &mut [Self]) {
                tessera_order::par_stable_sort(records);
            }
        }
    )+};
}
tessera_keys!(u32, u64, u128, f64, Reverse<u64>, Reverse<f64>);

/// Keeps keyed-record adapters on Tessera's by-key entry points.
macro_rules! tessera_records {
    ($($t:ty),+ $(,)?) => {$(
        impl TesseraAdapter for $t {
            #[cfg_attr(tessera_bench_revisions, inline(never))]
            fn tessera(records: &mut [Self]) {
                tessera_order::stable_sort_by_key(records, |r| r.key);
            }
            #[cfg(feature = "operations")]
            fn tessera_unstable(records: &mut [Self]) {
                tessera_order::unstable_sort_by_key(records, |r| r.key);
            }
            slice_operations!(stable_argsort_by_key, stable_sort_segments_by_key, stable_sort_prefix_by_key, stable_top_k_by_key, stable_select_nth_by_key, stable_sort_rank_range_by_key, stable_select_nth_many_by_key, |r| r.key);
            unstable_rank_operations!(unstable_sort_prefix_by_key, unstable_top_k_by_key, unstable_select_nth_by_key, unstable_sort_rank_range_by_key, unstable_select_nth_many_by_key, |r| r.key);
            #[cfg(tessera_bench_revisions)]
            revision_sorts!(stable_sort_by_key, |r| r.key);
            #[cfg(feature = "rayon")]
            fn tessera_parallel(records: &mut [Self]) {
                tessera_order::par_stable_sort_by_key(records, |r| r.key);
            }
        }
    )+};
}
tessera_records!(Row<u32>, Row<u64>, WideRecord, OwnedRecord);

/// Connects keyed records to the external radix-crate interfaces.
macro_rules! radix_record {
    ($t:ty, $key:ty) => {
        impl rdst::RadixKey for $t {
            const LEVELS: usize = size_of::<$key>();
            fn get_level(&self, level: usize) -> u8 {
                (self.key >> (8 * level)) as u8
            }
        }
        impl Radixable<$key> for $t {
            type Key = $key;
            fn key(&self) -> $key {
                self.key
            }
        }
    };
}
radix_record!(Row<u32>, u32);
radix_record!(Row<u64>, u64);
radix_record!(WideRecord, u64);

/// Sorts records with glidesort's default buffer.
fn glidesort<T: BenchmarkRecord>(records: &mut [T]) {
    glidesort::sort_by(records, T::order);
}

/// Sorts records with a heap buffer limited to the supplied scratch bytes.
fn glidesort_sqrt<T: BenchmarkRecord>(records: &mut [T], scratch_bytes: usize) {
    let mut buffer = Vec::with_capacity(scratch_bytes / size_of::<T>());
    buffer.resize_with(buffer.capacity(), MaybeUninit::uninit);
    glidesort::sort_with_buffer_by(records, &mut buffer, T::order);
}

/// Restorable batches of complete calls with operation-specific verification.
pub trait Kernel {
    /// Restores every independent buffer to the original input.
    fn restore(&mut self, batch: usize);
    /// Executes complete calls without restoring or checking the prepared buffers.
    fn run_batch(&mut self);
    /// Verifies all buffers against the shared original and reference.
    fn check(&self) -> Result<(), String>;
    /// Restores independent inputs, times only operation calls, then checks results and workers.
    /// Returns an error for invalid output or threads outside the owned pool.
    fn measure_batch(&mut self, batch: usize) -> Result<std::time::Duration, String> {
        self.restore(batch);
        let start = std::time::Instant::now();
        self.run_batch();
        let elapsed = start.elapsed();
        self.check()?;
        super::host::check_threads(self.worker_threads())?;
        Ok(elapsed)
    }
    /// Returns the record and owned-payload bytes in one batch buffer.
    fn buffer_bytes(&self) -> usize;
    /// Returns the number of persistent workers owned by this adapter.
    fn worker_threads(&self) -> usize;
    /// Mandatory output allocation retained through verification, per call.
    fn output_bytes(&self) -> usize {
        0
    }
    /// Exact implementation route, retained with non-sort capability evidence.
    fn description(&self) -> &'static str;
}
/// Batch buffers and a monomorphized in-place call over one shared input.
struct BatchKernel<T, F> {
    input: Rc<PreparedRecords<T>>,
    buffers: Vec<Vec<T>>,
    sort: F,
    stable: bool,
    required: Option<Vec<std::ops::Range<usize>>>,
    description: &'static str,
    worker_threads: usize,
}
impl<T: BenchmarkRecord, F: FnMut(&mut [T])> Kernel for BatchKernel<T, F> {
    fn restore(&mut self, batch: usize) {
        self.buffers
            .resize_with(batch, || self.input.original().clone());
        for v in &mut self.buffers {
            v.clone_from(self.input.original());
        }
    }
    fn run_batch(&mut self) {
        for v in &mut self.buffers {
            (self.sort)(black_box(v));
        }
    }
    fn check(&self) -> Result<(), String> {
        for v in &self.buffers {
            if self.input.operation().is_noop(self.input.original().len()) {
                // No-op contracts require original order, not just membership.
                check::check(v, self.input.original(), self.input.original(), true)?;
            } else if let Some(ranges) = &self.required {
                check::refinement(
                    v,
                    self.input.original(),
                    self.input.reference(),
                    ranges,
                    self.stable,
                )?;
            } else {
                check::check(
                    v,
                    self.input.original(),
                    self.input.reference(),
                    self.stable,
                )?;
            }
        }
        Ok(())
    }
    fn description(&self) -> &'static str {
        self.description
    }
    fn buffer_bytes(&self) -> usize {
        self.input.buffer_bytes()
    }
    fn worker_threads(&self) -> usize {
        self.worker_threads
    }
}
/// Creates a typed batch kernel without cloning its shared reference.
pub(super) fn prepare_kernel<T: BenchmarkRecord>(
    input: Rc<PreparedRecords<T>>,
    stable: bool,
    sort: impl FnMut(&mut [T]) + 'static,
) -> Box<dyn Kernel> {
    prepare_call(input, stable, sort, "complete sorting call")
}

/// Prepares one complete in-place call with its result checker and route evidence.
fn prepare_call<T: BenchmarkRecord>(
    input: Rc<PreparedRecords<T>>,
    stable: bool,
    sort: impl FnMut(&mut [T]) + 'static,
    description: &'static str,
) -> Box<dyn Kernel> {
    Box::new(BatchKernel {
        description,
        required: input.operation().required_ranges(),
        input,
        buffers: Vec::new(),
        sort,
        stable,
        worker_threads: 0,
    })
}
/// Adapts radix extraction to index output or an in-place operation.
fn prepare_radsort<T: BenchmarkRecord, K: radsort::Key>(
    input: Rc<PreparedRecords<T>>,
    key: impl Fn(&T) -> K + Copy + 'static,
) -> Box<dyn Kernel> {
    #[cfg(feature = "operations")]
    if !input.operation().is_sort() {
        if matches!(input.operation(), Operation::Argsort {}) {
            return operations::indices(input, move |source| {
                let mut indices: Vec<_> = (0..source.len()).collect();
                radsort::sort_by_key(&mut indices, |&index| key(&source[index]));
                indices
            });
        }
        return operations::reference(input, move |values| radsort::sort_by_key(values, key));
    }
    prepare_kernel(input, true, move |values| radsort::sort_by_key(values, key))
}

/// An adapter's public name and stability contract.
#[derive(Clone, Copy)]
pub struct Variant {
    /// Catalog algorithm identity.
    pub name: &'static str,
    /// Whether equal keys retain input order.
    pub stable: bool,
    /// Catalog adapter family.
    pub adapter: &'static str,
    /// Owned worker count, absent for serial adapters.
    pub threads: Option<usize>,
}
/// Lists the adapters measured in the selected executable configuration.
pub fn list_variants() -> Vec<Variant> {
    crate::plan::catalog()
        .algorithms
        .iter()
        .filter(|algorithm| {
            algorithm.adapter == "common"
                || (matches!(algorithm.adapter.as_str(), "reference" | "unstable")
                    && cfg!(feature = "operations"))
                || (cfg!(feature = "rayon") && algorithm.threads.is_some())
                || (algorithm.adapter == "revision" && cfg!(tessera_bench_revisions))
                || algorithm.adapter == "rust"
                || (algorithm.adapter == "native" && cfg!(feature = "compare-native"))
        })
        .map(|algorithm| Variant {
            name: &algorithm.id,
            stable: algorithm.stable,
            adapter: &algorithm.adapter,
            threads: algorithm.threads,
        })
        .collect()
}
/// Prepares the standard or Tessera adapter for any benchmark record type.
fn try_prepare_common<T: TesseraAdapter>(
    v: Variant,
    input: Rc<PreparedRecords<T>>,
    scratch_bytes: usize,
) -> Option<Box<dyn Kernel>> {
    if !input.operation().is_sort() {
        #[cfg(feature = "operations")]
        return operations::prepare(v, input);
        #[cfg(not(feature = "operations"))]
        return None;
    }
    #[cfg(feature = "rayon")]
    if let Some(threads) = v.threads {
        return Some(parallel::prepare(input, v.adapter, threads));
    }
    Some(match v.name {
        "tessera" => prepare_kernel(input, true, T::tessera),
        #[cfg(feature = "operations")]
        "tessera-unstable" => prepare_kernel(input, false, T::tessera_unstable),
        #[cfg(tessera_bench_revisions)]
        "tessera-baseline" => prepare_kernel(input, true, T::baseline),
        #[cfg(tessera_bench_revisions)]
        "tessera-baseline-copy" => prepare_kernel(input, true, T::baseline_copy),
        #[cfg(tessera_bench_revisions)]
        "tessera-copy" => prepare_kernel(input, true, T::current_copy),
        "std-stable" => prepare_kernel(input, true, |v| v.sort_by(T::order)),
        "std-unstable" => prepare_kernel(input, false, |v| v.sort_unstable_by(T::order)),
        "glidesort" => prepare_kernel(input, true, glidesort::<T>),
        "glidesort-sqrt" => prepare_kernel(input, true, move |v| glidesort_sqrt(v, scratch_bytes)),
        _ => return None,
    })
}
/// Selects a typed copyable-record adapter without callbacks inside a sort.
macro_rules! copy_adapter {
    ($v:expr,$input:expr,$scratch_bytes:expr,$key:expr) => {{
        let v = $v;
        let input = $input;
        match v.name {
            "radsort" => Some(prepare_radsort(input, $key)),
            "voracious" => Some(prepare_kernel(input, true, |v| v.voracious_stable_sort())),
            "rdst" => Some(prepare_kernel(input, false, |v| v.radix_sort_unstable())),
            #[cfg(feature = "compare-native")]
            _ if v.adapter == "native" => $crate::comparison::native::prepare(v, input),
            _ => try_prepare_common(v, input, $scratch_bytes),
        }
    }};
}
impl Variant {
    /// Checks capabilities without starting workers for a parallel adapter.
    pub fn supports(self, input: &CaseInput) -> bool {
        // Every CaseInput representation implements TesseraAdapter's parallel
        // operations. Preparing one here would warm and discard a real pool.
        (input.operation().is_sort() && cfg!(feature = "rayon") && self.threads.is_some())
            || self.prepare(input, 0).is_some()
    }

    /// Prepares an adapter for this input, or declines an unsupported combination.
    pub fn prepare(self, input: &CaseInput, scratch_bytes: usize) -> Option<Box<dyn Kernel>> {
        if input.operation().is_sort() {
            if self.adapter == "reference" {
                return None;
            }
        } else if !cfg!(feature = "operations")
            || !matches!(
                self.name,
                "tessera"
                    | "tessera-full"
                    | "tessera-unstable"
                    | "std-stable"
                    | "std-unstable"
                    | "glidesort"
                    | "radsort"
            )
        {
            return None;
        }
        #[cfg(feature = "compare-native")]
        {
            static LAYOUTS: std::sync::OnceLock<()> = std::sync::OnceLock::new();
            LAYOUTS.get_or_init(super::native::check_layouts);
        }
        match (self.name, input) {
            ("voracious", input) if !matches!(input, CaseInput::U32(_) | CaseInput::U64(_)) => None,
            ("radsort", CaseInput::OwnedRecord(v)) => {
                Some(prepare_radsort(Rc::clone(v), |r| r.key))
            }
            // External radix and native adapters do not accept Reverse keys.
            // Common comparison sorts use BenchmarkRecord's reversed order;
            // Tessera keeps the direct-key API in every revision copy.
            (_, CaseInput::ReverseU64(v)) => try_prepare_common(self, Rc::clone(v), scratch_bytes),
            (_, CaseInput::ReverseF64(v)) => try_prepare_common(self, Rc::clone(v), scratch_bytes),
            (_, CaseInput::OwnedRecord(v)) => try_prepare_common(self, Rc::clone(v), scratch_bytes),
            (_, CaseInput::U32(v)) => copy_adapter!(self, Rc::clone(v), scratch_bytes, |k| *k),
            (_, CaseInput::U128(input)) => match self.name {
                "radsort" if input.operation().is_sort() => {
                    Some(prepare_kernel(Rc::clone(input), true, radsort::sort))
                }
                "radsort" => Some(prepare_radsort(Rc::clone(input), |key| *key)),
                "rdst" => Some(prepare_kernel(Rc::clone(input), false, |v| {
                    v.radix_sort_unstable()
                })),
                _ => try_prepare_common(self, Rc::clone(input), scratch_bytes),
            },
            (_, CaseInput::U64(v)) => copy_adapter!(self, Rc::clone(v), scratch_bytes, |k| *k),
            (_, CaseInput::F64(v)) => copy_adapter!(self, Rc::clone(v), scratch_bytes, |k| *k),
            (_, CaseInput::Row32(v)) => copy_adapter!(self, Rc::clone(v), scratch_bytes, |r| r.key),
            (_, CaseInput::Row64(v)) => copy_adapter!(self, Rc::clone(v), scratch_bytes, |r| r.key),
            (_, CaseInput::WideRecord(v)) => {
                copy_adapter!(self, Rc::clone(v), scratch_bytes, |r| r.key)
            }
        }
    }
}

/// Adapter placement belongs to the sorting boundary, not record semantics.
#[cfg(tessera_bench_revisions)]
impl CaseInput {
    /// Records offsets for current, baseline A, baseline B and current B in order.
    pub(super) fn revision_entries(&self) -> Vec<String> {
        match self {
            Self::U32(_) => u32::revision_entries(),
            Self::U64(_) => u64::revision_entries(),
            Self::U128(_) => u128::revision_entries(),
            Self::F64(_) => f64::revision_entries(),
            Self::ReverseU64(_) => Reverse::<u64>::revision_entries(),
            Self::ReverseF64(_) => Reverse::<f64>::revision_entries(),
            Self::Row32(_) => Row::<u32>::revision_entries(),
            Self::Row64(_) => Row::<u64>::revision_entries(),
            Self::WideRecord(_) => WideRecord::revision_entries(),
            Self::OwnedRecord(_) => OwnedRecord::revision_entries(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::InputCase;
    #[cfg(tessera_bench_revisions)]
    #[test]
    fn revisions_preserve_records() {
        for ty in crate::plan::catalog().types.iter().map(|ty| &ty.id) {
            for distribution in ["random", "equal", "reverse", "d16"] {
                let input = CaseInput::new(&InputCase {
                    ty: ty.clone(),
                    distribution: distribution.into(),
                    len: 1000,
                    operation: Operation::Sort {},
                });
                assert_eq!(input.revision_entries().len(), 4);
                for variant in list_variants()
                    .into_iter()
                    .filter(|v| v.name == "tessera" || v.adapter == "revision")
                {
                    let mut kernel = variant.prepare(&input, 0).unwrap();
                    kernel.restore(2);
                    kernel.run_batch();
                    kernel.check().unwrap_or_else(|error| {
                        panic!("{ty}/{distribution}/{}: {error}", variant.name)
                    });
                }
            }
        }
    }

    #[test]
    fn adapters_follow_features() {
        let names: Vec<_> = list_variants().iter().map(|v| v.name).collect();
        for id in ["tessera-par-8", "rayon-stable-8"] {
            assert_eq!(names.contains(&id), cfg!(feature = "rayon"), "{id}");
        }
        assert!(names.contains(&"tessera"));
        assert!(names.contains(&"std-stable"));
        assert!(names.contains(&"radsort"));
        assert_eq!(
            names.contains(&"fluxsort"),
            cfg!(feature = "compare-native")
        );
    }

    #[test]
    fn capabilities_match_preparation() {
        for ty in crate::plan::catalog().types.iter().map(|ty| &ty.id) {
            let input = CaseInput::new(&InputCase {
                ty: ty.clone(),
                distribution: "random".into(),
                len: 32,
                operation: Operation::Sort {},
            });
            for variant in list_variants() {
                assert_eq!(
                    variant.supports(&input),
                    variant.prepare(&input, 0).is_some(),
                    "{} / {ty}",
                    variant.name,
                );
            }
        }
    }

    #[test]
    fn reverse_adapters_preserve_inputs() {
        for ty in ["reverse_u64", "reverse_f64"] {
            for distribution in ["random", "sorted", "sorted99", "far_swaps", "equal"] {
                for len in [63, 64, 1000] {
                    let input = CaseInput::new(&InputCase {
                        ty: ty.into(),
                        distribution: distribution.into(),
                        len,
                        operation: Operation::Sort {},
                    });
                    for variant in list_variants().into_iter().filter(|v| v.threads.is_none()) {
                        let kernel = variant.prepare(&input, 32768);
                        let supported = ["common", "revision"].contains(&variant.adapter)
                            || [
                                "glidesort",
                                "glidesort-sqrt",
                                "std-unstable",
                                "tessera-unstable",
                            ]
                            .contains(&variant.name);
                        assert_eq!(kernel.is_some(), supported, "{ty}/{}", variant.name);
                        if let Some(mut kernel) = kernel {
                            kernel.restore(2);
                            kernel.run_batch();
                            kernel.check().unwrap_or_else(|error| {
                                panic!("{ty}/{distribution}/{len}/{}: {error}", variant.name)
                            });
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn batches_restore_shared_input() {
        let input = PreparedRecords::new(vec![3u64, 1, 2]);
        let mut k = prepare_kernel(Rc::clone(&input), true, |v| {
            v.fill(0);
        });
        k.restore(3);
        k.run_batch();
        assert!(k.check().is_err());
        let mut k = BatchKernel {
            input,
            buffers: vec![],
            sort: |v: &mut [u64]| {
                assert_eq!(v, [3, 1, 2]);
                v.sort();
            },
            stable: true,
            required: None,
            description: "test fixture",
            worker_threads: 0,
        };
        for _ in 0..9 {
            k.restore(3);
            k.run_batch();
            k.check().unwrap();
        }
    }
    #[test]
    fn float_payloads_preserve_order() {
        let bits = [
            0xfff8_0000_0000_0002u64,
            0xfff8_0000_0000_0001,
            0xfff0_0000_0000_0001,
            0xfff0_0000_0000_0000,
            0x8000_0000_0000_0001,
            0x8000_0000_0000_0000,
            0,
            1,
            0x7ff0_0000_0000_0000,
            0x7ff0_0000_0000_0001,
            0x7ff8_0000_0000_0001,
            0x7ff8_0000_0000_0002,
        ];
        let input: Vec<_> = bits
            .into_iter()
            .rev()
            .cycle()
            .take(1000)
            .map(f64::from_bits)
            .collect();
        let reversed = input.iter().copied().map(Reverse).collect();
        for input in [
            CaseInput::F64(PreparedRecords::new(input)),
            CaseInput::ReverseF64(PreparedRecords::new(reversed)),
        ] {
            for v in list_variants() {
                let Some(mut k) = v.prepare(&input, 32768) else {
                    continue;
                };
                k.restore(2);
                k.run_batch();
                k.check().unwrap_or_else(|e| panic!("{}: {e}", v.name));
            }
        }
    }
}
