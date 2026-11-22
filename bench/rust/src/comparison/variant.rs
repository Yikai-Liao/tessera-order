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
use crate::plan::{Execution, Operation};
use rdst::RadixSort;
use std::{cmp::Reverse, hint::black_box, mem::MaybeUninit, rc::Rc};
use voracious_radix_sort::{RadixSort as VoraciousSort, Radixable};

/// Type-specific Tessera entry points, separate from record identity and ordering.
pub(super) trait TesseraAdapter: BenchmarkRecord {
    /// The public call [`Self::tessera`] and the revision copies make.
    const SORT_ROUTE: &'static str;
    /// The public call [`Self::tessera_parallel`] makes.
    #[cfg(feature = "rayon")]
    const PARALLEL_SORT_ROUTE: &'static str;
    /// Sorts scalar keys directly and keyed records through the by-key API.
    fn tessera(records: &mut [Self]);
    /// Current serial slice operations; historical sorting adapters omit these APIs.
    #[cfg(feature = "operations")]
    fn argsort(records: &[Self]) -> Vec<usize>;
    #[cfg(feature = "operations")]
    fn sort_index_rows(records: &[Self]) -> Vec<usize>;
    #[cfg(feature = "operations")]
    fn select_nth(records: &mut [Self], index: usize);

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
/// in the library and kernel preparation owns the operation and output layout.
macro_rules! slice_operations {
    ($argsort:ident, $select:ident $(, $key:expr)?) => {
        #[cfg(feature = "operations")]
        fn argsort(records: &[Self]) -> Vec<usize> { tessera_order::$argsort(records $(, $key)?) }
        #[cfg(feature = "operations")]
        fn select_nth(records: &mut [Self], index: usize) { tessera_order::$select(records, index $(, $key)?); }
    };
}

/// Keeps scalar adapters on Tessera's direct-key entry points.
///
/// Sorting adapters also build against baseline libraries, which before 0.2
/// offer only the deprecated `stable_` names. Switch them to `sort` and
/// `par_sort` once every compared baseline has those names.
macro_rules! tessera_keys {
    ($($t:ty),+ $(,)?) => {$(
        #[allow(deprecated)]
        impl TesseraAdapter for $t {
            const SORT_ROUTE: &'static str = "tessera_order::stable_sort";
            #[cfg(feature = "rayon")]
            const PARALLEL_SORT_ROUTE: &'static str = "tessera_order::par_stable_sort in a caller-owned pool";
            #[cfg_attr(tessera_bench_revisions, inline(never))]
            fn tessera(records: &mut [Self]) {
                tessera_order::stable_sort(records);
            }

            slice_operations!(argsort, select_nth);
            #[cfg(feature = "operations")]
            fn sort_index_rows(records: &[Self]) -> Vec<usize> {
                operations::sort_index_rows(records, |r| *r)
            }

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

/// Keeps keyed-record adapters on Tessera's by-key entry points, with the
/// baseline naming constraint of [`tessera_keys`].
macro_rules! tessera_records {
    ($($t:ty),+ $(,)?) => {$(
        #[allow(deprecated)]
        impl TesseraAdapter for $t {
            const SORT_ROUTE: &'static str = "tessera_order::stable_sort_by_key";
            #[cfg(feature = "rayon")]
            const PARALLEL_SORT_ROUTE: &'static str =
                "tessera_order::par_stable_sort_by_key in a caller-owned pool";
            #[cfg_attr(tessera_bench_revisions, inline(never))]
            fn tessera(records: &mut [Self]) {
                tessera_order::stable_sort_by_key(records, |r| r.key);
            }

            slice_operations!(argsort_by_key, select_nth_by_key, |r| r.key);
            #[cfg(feature = "operations")]
            fn sort_index_rows(records: &[Self]) -> Vec<usize> {
                operations::sort_index_rows(records, |r| r.key)
            }

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
    /// Per-call retained batch bytes: mutable records/payloads and mandatory
    /// output. Shared originals/references and transient workspace are separate.
    fn buffer_bytes(&self) -> usize;
    /// Returns the number of persistent workers owned by this adapter.
    fn worker_threads(&self) -> usize;
    /// Mandatory output allocation retained through verification, per call.
    fn output_bytes(&self) -> usize {
        0
    }
    /// Exact implementation route, retained in every case's capability evidence.
    fn description(&self) -> &'static str;
}
/// Batch buffers and a monomorphized in-place call over one shared input.
struct BatchKernel<T, F> {
    input: Rc<PreparedRecords<T>>,
    buffers: Vec<Vec<T>>,
    sort: F,
    stable: bool,
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
            if let Operation::SelectNth { index } = *self.input.operation() {
                check::selection(
                    v,
                    self.input.original(),
                    self.input.reference(),
                    index,
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
/// Adapts radix extraction to index output or an in-place operation.
fn prepare_radsort<T: BenchmarkRecord, K: radsort::Key>(
    variant: Variant,
    input: Rc<PreparedRecords<T>>,
    key: impl Fn(&T) -> K + Copy + 'static,
) -> Box<dyn Kernel> {
    #[cfg(feature = "operations")]
    match *input.operation() {
        Operation::PartitionBuckets { bucket_count } => {
            return operations::categories(variant, input, bucket_count)
                .expect("category sorting supported");
        }
        Operation::Argsort {} => {
            return operations::indices(
                variant,
                input,
                "radsort::sort_by_key on indices",
                move |source| {
                    let mut indices: Vec<_> = (0..source.len()).collect();
                    radsort::sort_by_key(&mut indices, |&index| key(&source[index]));
                    indices
                },
            );
        }
        Operation::SelectNth { .. } => {
            return operations::selection_reference(variant, input, move |values| {
                radsort::sort_by_key(values, key);
            });
        }
        Operation::Sort {} => {}
    }
    variant.prepare_call(input, "radsort::sort_by_key", move |values| {
        radsort::sort_by_key(values, key);
    })
}

/// A catalog implementation and its executable adapter.
#[derive(Clone, Copy)]
pub struct Variant {
    /// Catalog implementation identity.
    pub name: &'static str,
    implementation: &'static crate::plan::Implementation,
    /// Catalog adapter family.
    pub adapter: &'static str,
    /// The catalog's parallel full-sort entry point, if any.
    pub parallel_sort: Option<crate::plan::ParallelSort>,
}
/// Lists the adapters measured in the selected executable configuration.
pub fn list_variants() -> Vec<Variant> {
    crate::plan::catalog()
        .implementations
        .iter()
        .filter(|implementation| {
            implementation.adapter == "common"
                || (implementation.adapter == "reference" && cfg!(feature = "operations"))
                || (implementation.adapter == "rayon" && cfg!(feature = "rayon"))
                || (implementation.adapter == "revision" && cfg!(tessera_bench_revisions))
                || implementation.adapter == "rust"
                || (implementation.adapter == "native" && cfg!(feature = "compare-native"))
        })
        .map(|implementation| Variant {
            name: &implementation.id,
            implementation,
            adapter: &implementation.adapter,
            parallel_sort: implementation.parallel,
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
    Some(match v.name {
        "tessera" | "tessera-full" => v.prepare_call(input, T::SORT_ROUTE, T::tessera),

        #[cfg(tessera_bench_revisions)]
        "tessera-baseline" => v.prepare_call(input, T::SORT_ROUTE, T::baseline),
        #[cfg(tessera_bench_revisions)]
        "tessera-baseline-copy" => v.prepare_call(input, T::SORT_ROUTE, T::baseline_copy),
        #[cfg(tessera_bench_revisions)]
        "tessera-copy" => v.prepare_call(input, T::SORT_ROUTE, T::current_copy),
        "std-stable" => v.prepare_call(input, "slice::sort_by", |v| v.sort_by(T::order)),
        "std-unstable" => v.prepare_call(input, "slice::sort_unstable_by", |v| {
            v.sort_unstable_by(T::order);
        }),
        "glidesort" => v.prepare_call(input, "glidesort::sort_by", glidesort::<T>),
        "glidesort-sqrt" => v.prepare_call(input, "glidesort::sort_with_buffer_by", move |v| {
            glidesort_sqrt(v, scratch_bytes);
        }),
        _ => return None,
    })
}
/// Selects a typed copyable-record adapter without callbacks inside a sort.
macro_rules! copy_adapter {
    ($v:expr,$input:expr,$scratch_bytes:expr,$key:expr) => {{
        let v = $v;
        let input = $input;
        match v.name {
            "radsort" => Some(prepare_radsort(v, input, $key)),
            "voracious" => Some(v.prepare_call(input, "voracious_stable_sort", |v| {
                v.voracious_stable_sort();
            })),
            "rdst" => Some(v.prepare_call(input, "rdst radix_sort_unstable", |v| {
                v.radix_sort_unstable();
            })),
            #[cfg(feature = "compare-native")]
            _ if v.adapter == "native" => $crate::comparison::native::prepare(v, input),
            _ => try_prepare_common(v, input, $scratch_bytes),
        }
    }};
}
impl Variant {
    /// The tie-order guarantee used for verification and published metadata.
    pub(super) fn route_stable(self, operation: &Operation) -> bool {
        self.implementation.route_stable(operation)
    }

    /// Prepares an in-place call with the route's verification contract.
    pub(super) fn prepare_call<T: BenchmarkRecord>(
        self,
        input: Rc<PreparedRecords<T>>,
        description: &'static str,
        sort: impl FnMut(&mut [T]) + 'static,
    ) -> Box<dyn Kernel> {
        let stable = self.route_stable(input.operation());
        Box::new(BatchKernel {
            description,
            input,
            buffers: Vec::new(),
            sort,
            stable,
            worker_threads: 0,
        })
    }

    /// Whether this adapter calls a parallel entry point for the operation.
    /// The catalog lists which implementations have a parallel full sort.
    fn parallel(self, operation: &Operation) -> bool {
        cfg!(feature = "rayon") && operation.is_sort() && self.parallel_sort.is_some()
    }

    /// Checks capabilities without starting workers for a parallel execution.
    pub fn supports(self, input: &CaseInput, execution: Execution) -> bool {
        // Every CaseInput representation implements TesseraAdapter's parallel
        // operations. Preparing one here would warm and discard a real pool.
        match execution {
            Execution::Serial => self.prepare(input, execution, 0).is_some(),
            Execution::Threads(_) => self.parallel(input.operation()),
        }
    }

    /// The exact call a supported column makes, retained in every manifest
    /// and printed by `list`. Naming a parallel route starts no pool.
    pub(super) fn route(self, input: &CaseInput, execution: Execution) -> &'static str {
        match execution {
            Execution::Serial => self
                .prepare(input, execution, 0)
                .expect("route of a supported column")
                .description(),
            #[cfg(feature = "rayon")]
            Execution::Threads(_) => input.parallel_route(self.name),
            #[cfg(not(feature = "rayon"))]
            Execution::Threads(_) => unreachable!("no parallel column without Rayon"),
        }
    }

    /// Prepares an adapter for this input and execution, or declines an
    /// unsupported combination. A parallel execution starts and warms its
    /// N-worker pool here, outside every timed and counted region.
    pub fn prepare(
        self,
        input: &CaseInput,
        execution: Execution,
        scratch_bytes: usize,
    ) -> Option<Box<dyn Kernel>> {
        if let Some(threads) = execution.threads() {
            if !self.parallel(input.operation()) {
                return None;
            }
            #[cfg(feature = "rayon")]
            return Some(input.prepare_parallel(self, threads));
            #[cfg(not(feature = "rayon"))]
            unreachable!("parallel execution without the rayon feature: {threads}");
        }
        if input.operation().is_sort() {
            if self.parallel_sort == Some(crate::plan::ParallelSort::Only) {
                return None;
            }
        } else if !cfg!(feature = "operations")
            || !matches!(
                self.name,
                "tessera"
                    | "tessera-full"
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
                Some(prepare_radsort(self, Rc::clone(v), |r| r.key))
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
                    Some(self.prepare_call(Rc::clone(input), "radsort::sort", radsort::sort))
                }
                "radsort" => Some(prepare_radsort(self, Rc::clone(input), |key| *key)),
                "rdst" => Some(self.prepare_call(
                    Rc::clone(input),
                    "rdst radix_sort_unstable",
                    |v| v.radix_sort_unstable(),
                )),
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
                    let mut kernel = variant.prepare(&input, Execution::Serial, 0).unwrap();
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
        assert_eq!(names.contains(&"rayon-stable"), cfg!(feature = "rayon"));
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
                for execution in [Execution::Serial, "t1".parse().unwrap()] {
                    assert_eq!(
                        variant.supports(&input, execution),
                        variant.prepare(&input, execution, 0).is_some(),
                        "{} / {ty} / {execution}",
                        variant.name,
                    );
                }
                // Only the full sorts of Tessera and Rayon have parallel entry points.
                assert_eq!(
                    variant.supports(&input, "t2".parse().unwrap()),
                    cfg!(feature = "rayon") && ["tessera", "rayon-stable"].contains(&variant.name),
                    "{} / {ty}",
                    variant.name,
                );
            }
        }
    }

    /// An opponent that supports a non-sort operation names its call in the catalog's
    /// `routes`, so a figure never shows a sort's label on an index or selection row; a
    /// named route is one the adapter takes. Tessera's own labels name its calls.
    #[cfg(feature = "operations")]
    #[test]
    fn catalog_routes_name_supported_operations() {
        let operations = [
            Operation::Argsort {},
            Operation::SelectNth { index: 0 },
            Operation::PartitionBuckets { bucket_count: 16 },
        ];
        for variant in list_variants()
            .into_iter()
            .filter(|v| !v.name.starts_with("tessera"))
        {
            for operation in &operations {
                let kind = operation.kind();
                let supported = crate::plan::catalog().types.iter().any(|ty| {
                    variant.supports(
                        &CaseInput::new(&InputCase {
                            ty: ty.id.clone(),
                            distribution: "random".into(),
                            len: 32,
                            operation: operation.clone(),
                        }),
                        Execution::Serial,
                    )
                });
                assert_eq!(
                    variant.implementation.routes.contains_key(kind),
                    supported,
                    "{} / {kind}: catalog route and adapter support differ",
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
                    for variant in list_variants() {
                        let kernel = variant.prepare(&input, Execution::Serial, 32768);
                        let supported = ["common", "revision"].contains(&variant.adapter)
                            || [
                                "tessera-full",
                                "glidesort",
                                "glidesort-sqrt",
                                "std-unstable",
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
        let variant = list_variants()
            .into_iter()
            .find(|v| v.name == "tessera")
            .unwrap();
        let mut k = variant.prepare_call(Rc::clone(&input), "test fixture", |v| {
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
                let Some(mut k) = v.prepare(&input, Execution::Serial, 32768) else {
                    continue;
                };
                k.restore(2);
                k.run_batch();
                k.check().unwrap_or_else(|e| panic!("{}: {e}", v.name));
            }
        }
    }
}
