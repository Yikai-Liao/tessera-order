// SPDX-License-Identifier: BSD-2-Clause

//! Sorting adapters and restorable, verified batch kernels.

#[cfg(feature = "rayon")]
mod parallel;

use super::{
    check,
    record::{BenchmarkRecord, CaseInput, OwnedRecord, PreparedRecords, WideRecord},
};
use crate::case::Row;
use rdst::RadixSort;
use std::{hint::black_box, mem::MaybeUninit, rc::Rc};
use voracious_radix_sort::{RadixSort as VoraciousSort, Radixable};

/// Type-specific Tessera entry points, separate from record identity and ordering.
pub(super) trait TesseraAdapter: BenchmarkRecord {
    /// Sorts scalar keys directly and keyed records through the by-key API.
    fn tessera(records: &mut [Self]);
    /// Uses the same direct/by-key distinction for caller-owned parallel sorting.
    #[cfg(feature = "rayon")]
    fn tessera_parallel(records: &mut [Self]);
}

/// Keeps scalar adapters on Tessera's direct-key entry points.
macro_rules! tessera_keys {
    ($($t:ty),+ $(,)?) => {$(
        impl TesseraAdapter for $t {
            fn tessera(records: &mut [Self]) {
                tessera_sort::stable_sort(records);
            }
            #[cfg(feature = "rayon")]
            fn tessera_parallel(records: &mut [Self]) {
                tessera_sort::par_stable_sort(records);
            }
        }
    )+};
}
tessera_keys!(u32, u64, f64);

/// Keeps keyed-record adapters on Tessera's by-key entry points.
macro_rules! tessera_records {
    ($($t:ty),+ $(,)?) => {$(
        impl TesseraAdapter for $t {
            fn tessera(records: &mut [Self]) {
                tessera_sort::stable_sort_by_key(records, |r| r.key);
            }
            #[cfg(feature = "rayon")]
            fn tessera_parallel(records: &mut [Self]) {
                tessera_sort::par_stable_sort_by_key(records, |r| r.key);
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

/// Restorable batches with one adapter call and output verification per batch.
pub(super) trait Kernel {
    /// Restores every independent buffer to the original input.
    fn restore(&mut self, batch: usize);
    /// Sorts the prepared buffers without restoring or checking them.
    fn run_batch(&mut self);
    /// Verifies all buffers against the shared original and reference.
    fn check(&self) -> Result<(), String>;
    /// Returns the record and owned-payload bytes in one batch buffer.
    fn buffer_bytes(&self) -> usize;
    /// Returns the number of persistent workers owned by this adapter.
    fn worker_threads(&self) -> usize;
}
/// Batch buffers and a monomorphized sort over one shared prepared input.
struct BatchKernel<T, F> {
    input: Rc<PreparedRecords<T>>,
    buffers: Vec<Vec<T>>,
    sort: F,
    stable: bool,
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
            check::check(
                v,
                self.input.original(),
                self.input.reference(),
                self.stable,
            )?;
        }
        Ok(())
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
    Box::new(BatchKernel {
        input,
        buffers: Vec::new(),
        sort,
        stable,
        worker_threads: 0,
    })
}
/// An adapter's public name and stability contract.
#[derive(Clone, Copy)]
pub(super) struct Variant {
    pub name: &'static str,
    pub stable: bool,
    pub adapter: &'static str,
    pub threads: Option<usize>,
}
/// Lists the adapters measured in the selected executable configuration.
pub(super) fn list_variants() -> Vec<Variant> {
    crate::plan::catalog()
        .algorithms
        .iter()
        .filter(|algorithm| {
            algorithm.adapter == "common"
                || (cfg!(feature = "rayon") && algorithm.threads.is_some())
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
    #[cfg(feature = "rayon")]
    if let Some(threads) = v.threads {
        return Some(parallel::prepare(input, v.adapter, threads));
    }
    Some(match v.name {
        "tessera" => prepare_kernel(input, true, T::tessera),
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
            "radsort" => Some(prepare_kernel(input, true, |v| {
                radsort::sort_by_key(v, $key)
            })),
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
    pub(super) fn supports(self, input: &CaseInput) -> bool {
        // Every CaseInput representation implements TesseraAdapter's parallel
        // operations. Preparing one here would warm and discard a real pool.
        (cfg!(feature = "rayon") && self.threads.is_some()) || self.prepare(input, 0).is_some()
    }

    /// Prepares an adapter for this input, or declines an unsupported combination.
    pub(super) fn prepare(
        self,
        input: &CaseInput,
        scratch_bytes: usize,
    ) -> Option<Box<dyn Kernel>> {
        match (self.name, input) {
            ("voracious", input) if !matches!(input, CaseInput::U32(_) | CaseInput::U64(_)) => None,
            ("radsort", CaseInput::OwnedRecord(v)) => {
                Some(prepare_kernel(Rc::clone(v), true, |v| {
                    radsort::sort_by_key(v, |r| r.key)
                }))
            }
            (_, CaseInput::OwnedRecord(v)) => try_prepare_common(self, Rc::clone(v), scratch_bytes),
            (_, CaseInput::U32(v)) => copy_adapter!(self, Rc::clone(v), scratch_bytes, |k| *k),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::InputCase;
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
        for ty in &crate::plan::catalog().comparison.types {
            let input = CaseInput::new(&InputCase {
                ty: ty.clone(),
                distribution: "random".into(),
                len: 32,
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
        let input = CaseInput::F64(PreparedRecords::new(input));
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
