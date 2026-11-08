// SPDX-License-Identifier: BSD-2-Clause

//! Record representations, shared inputs and supported sorting adapters.

#[cfg(feature = "rayon")]
mod parallel;

use super::{InputCase, check};
use crate::case::{Key, Row, generate};
use rdst::RadixSort;
use std::{cmp::Ordering, hint::black_box, mem::MaybeUninit, rc::Rc};
use voracious_radix_sort::{RadixSort as VoraciousSort, Radixable};

/// Ordering, identity and sorting operations for one benchmark record representation.
pub(super) trait BenchmarkRecord: Clone + Send + Sync + 'static {
    /// Whether tie order and payload identity are observable.
    const HAS_PAYLOAD: bool = false;
    /// Compares sorting keys in the shared total order.
    fn order(&self, other: &Self) -> Ordering;
    /// Compares every observable bit of two records.
    fn same(&self, other: &Self) -> bool;
    /// Returns the original position encoded in a record payload.
    fn index(&self) -> usize {
        0
    }
    /// Sorts this record representation through Tessera's public API.
    fn tessera(v: &mut [Self]);
    /// Sorts this representation through Tessera's parallel API.
    #[cfg(feature = "rayon")]
    fn tessera_parallel(v: &mut [Self]);
    /// Returns separately allocated payload capacity for one record.
    fn extra_bytes(&self) -> usize {
        0
    }
    /// Sorts this representation with glidesort's default buffer.
    fn glidesort(v: &mut [Self]) {
        glidesort::sort_by(v, Self::order);
    }
    /// Sorts with a heap buffer limited to the supplied scratch bytes.
    fn glidesort_sqrt(records: &mut [Self], scratch_bytes: usize) {
        let mut buffer = Vec::with_capacity(scratch_bytes / size_of::<Self>());
        buffer.resize_with(buffer.capacity(), MaybeUninit::uninit);
        glidesort::sort_with_buffer_by(records, &mut buffer, Self::order);
    }
}
/// Implements benchmark ordering and sorting for scalar keys.
macro_rules! key_element {
    ($t:ty) => {
        impl BenchmarkRecord for $t {
            fn order(&self, o: &Self) -> Ordering {
                Key::order(self, o)
            }
            fn same(&self, o: &Self) -> bool {
                Key::same(self, o)
            }
            fn tessera(v: &mut [Self]) {
                tessera_sort::stable_sort(v);
            }
            #[cfg(feature = "rayon")]
            fn tessera_parallel(v: &mut [Self]) {
                tessera_sort::par_stable_sort(v);
            }
        }
    };
}
key_element!(u32);
key_element!(u64);
key_element!(f64);

/// A 128-byte C-layout record with a key and indexed payload.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, PartialOrd)]
pub(super) struct WideRecord {
    pub key: u64,
    pub payload: [u8; 120],
}
/// A keyed record whose allocated string payload carries its original index.
#[derive(Clone)]
pub(super) struct OwnedRecord {
    key: u64,
    payload: String,
}

/// Implements benchmark operations for copyable keyed records.
macro_rules! record_element {
    ($t:ty, $index:expr, $same:expr) => {
        impl BenchmarkRecord for $t {
            const HAS_PAYLOAD: bool = true;
            fn order(&self, o: &Self) -> Ordering {
                self.key.cmp(&o.key)
            }
            fn same(&self, o: &Self) -> bool {
                $same(self, o)
            }
            fn index(&self) -> usize {
                $index(self)
            }
            fn tessera(v: &mut [Self]) {
                tessera_sort::stable_sort_by_key(v, |r| r.key);
            }
            #[cfg(feature = "rayon")]
            fn tessera_parallel(v: &mut [Self]) {
                tessera_sort::par_stable_sort_by_key(v, |r| r.key);
            }
        }
    };
}
record_element!(
    Row<u32>,
    |r: &Row<u32>| r.row as usize,
    |a: &Row<u32>, b: &Row<u32>| a.key == b.key && a.row == b.row
);
record_element!(
    Row<u64>,
    |r: &Row<u64>| r.row as usize,
    |a: &Row<u64>, b: &Row<u64>| a.key == b.key && a.row == b.row
);
record_element!(
    WideRecord,
    |r: &WideRecord| u64::from_le_bytes(r.payload[..8].try_into().unwrap()) as usize,
    |a: &WideRecord, b: &WideRecord| a == b
);
impl BenchmarkRecord for OwnedRecord {
    const HAS_PAYLOAD: bool = true;
    fn order(&self, other: &Self) -> Ordering {
        self.key.cmp(&other.key)
    }
    fn same(&self, other: &Self) -> bool {
        self.key == other.key && self.payload == other.payload
    }
    fn index(&self) -> usize {
        usize::from_str_radix(&self.payload, 16).unwrap_or(usize::MAX)
    }
    fn tessera(values: &mut [Self]) {
        tessera_sort::stable_sort_by_key(values, |r| r.key);
    }
    #[cfg(feature = "rayon")]
    fn tessera_parallel(values: &mut [Self]) {
        tessera_sort::par_stable_sort_by_key(values, |r| r.key);
    }
    fn extra_bytes(&self) -> usize {
        self.payload.capacity()
    }
}

impl<K: Key> PartialEq for Row<K> {
    fn eq(&self, o: &Self) -> bool {
        self.key.same(&o.key)
    }
}
impl<K: Key> PartialOrd for Row<K> {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.key.order(&o.key))
    }
}
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

/// Immutable original records and their stable reference shared by all adapters.
pub(super) struct PreparedRecords<T> {
    original: Vec<T>,
    reference: Vec<T>,
    buffer_bytes: usize,
}
impl<T: BenchmarkRecord> PreparedRecords<T> {
    /// Prepares one immutable input, its reference order and buffer footprint.
    fn new(original: Vec<T>) -> Rc<Self> {
        let mut reference = original.clone();
        reference.sort_by(T::order);
        let buffer_bytes =
            original.len() * size_of::<T>() + original.iter().map(T::extra_bytes).sum::<usize>();
        Rc::new(Self {
            original,
            reference,
            buffer_bytes,
        })
    }
}
/// A prepared case with its concrete record representation preserved.
pub(super) enum CaseInput {
    U32(Rc<PreparedRecords<u32>>),
    U64(Rc<PreparedRecords<u64>>),
    F64(Rc<PreparedRecords<f64>>),
    Row32(Rc<PreparedRecords<Row<u32>>>),
    Row64(Rc<PreparedRecords<Row<u64>>>),
    WideRecord(Rc<PreparedRecords<WideRecord>>),
    OwnedRecord(Rc<PreparedRecords<OwnedRecord>>),
}
impl CaseInput {
    /// Generates and prepares the concrete record representation of a suite case.
    pub(super) fn new(c: &InputCase) -> Self {
        match c.ty.as_str() {
            "u32" => Self::U32(PreparedRecords::new(generate::<u32>(
                &c.distribution,
                c.len,
            ))),
            "u64" => Self::U64(PreparedRecords::new(generate::<u64>(
                &c.distribution,
                c.len,
            ))),
            "f64" => Self::F64(PreparedRecords::new(generate::<f64>(
                &c.distribution,
                c.len,
            ))),
            "row_u32" => Self::Row32(PreparedRecords::new(
                generate::<u32>(&c.distribution, c.len)
                    .into_iter()
                    .enumerate()
                    .map(|(i, key)| Row { key, row: i as u32 })
                    .collect(),
            )),
            "row_u64" => Self::Row64(PreparedRecords::new(
                generate::<u64>(&c.distribution, c.len)
                    .into_iter()
                    .enumerate()
                    .map(|(i, key)| Row { key, row: i as u32 })
                    .collect(),
            )),
            "record128" => Self::WideRecord(PreparedRecords::new(
                generate::<u64>(&c.distribution, c.len)
                    .into_iter()
                    .enumerate()
                    .map(|(i, key)| {
                        let mut payload = [0u8; 120];
                        for (j, p) in payload.iter_mut().enumerate() {
                            *p = (i.wrapping_mul(31) + j) as u8;
                        }
                        payload[..8].copy_from_slice(&(i as u64).to_le_bytes());
                        WideRecord { key, payload }
                    })
                    .collect(),
            )),
            "owned" => Self::OwnedRecord(PreparedRecords::new(
                generate::<u64>(&c.distribution, c.len)
                    .into_iter()
                    .enumerate()
                    .map(|(i, key)| OwnedRecord {
                        key,
                        payload: format!("{i:016x}"),
                    })
                    .collect(),
            )),
            _ => unreachable!("suite contains an unknown type"),
        }
    }
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
            .resize_with(batch, || self.input.original.clone());
        for v in &mut self.buffers {
            v.clone_from(&self.input.original);
        }
    }
    fn run_batch(&mut self) {
        for v in &mut self.buffers {
            (self.sort)(black_box(v));
        }
    }
    fn check(&self) -> Result<(), String> {
        for v in &self.buffers {
            check::check(v, &self.input.original, &self.input.reference, self.stable)?;
        }
        Ok(())
    }
    fn buffer_bytes(&self) -> usize {
        self.input.buffer_bytes
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
                || (!cfg!(feature = "portable-simd")
                    && (algorithm.adapter == "rust"
                        || (algorithm.adapter == "native" && cfg!(feature = "compare-native"))))
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
fn try_prepare_common<T: BenchmarkRecord>(
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
        "glidesort" => prepare_kernel(input, true, T::glidesort),
        "glidesort-sqrt" => {
            prepare_kernel(input, true, move |v| T::glidesort_sqrt(v, scratch_bytes))
        }
        _ => return None,
    })
}
/// Selects a typed copyable-record adapter without callbacks inside a sort.
macro_rules! copy_adapter {
    ($v:expr,$input:expr,$scratch_bytes:expr,$key:expr,$ty:expr) => {{
        let v = $v;
        let input = $input;
        match v.name {
            "radsort" => Some(prepare_kernel(input, true, |v| {
                radsort::sort_by_key(v, $key)
            })),
            "voracious" => Some(prepare_kernel(input, true, |v| v.voracious_stable_sort())),
            "rdst" => Some(prepare_kernel(input, false, |v| v.radix_sort_unstable())),
            #[cfg(feature = "compare-native")]
            "clausecker" | "fluxsort" | "ips2ra" | "ips4o" | "vqsort" | "x86simdsort"
            | "crumsort" | "duckdb" => $crate::comparison::native::prepare(v, input, $ty),
            _ => try_prepare_common(v, input, $scratch_bytes),
        }
    }};
}
impl Variant {
    /// Prepares an adapter for this input, or declines an unsupported combination.
    pub(super) fn prepare(
        self,
        input: &CaseInput,
        scratch_bytes: usize,
    ) -> Option<Box<dyn Kernel>> {
        if self.name == "voracious" && !matches!(input, CaseInput::U32(_) | CaseInput::U64(_)) {
            return None;
        }
        match input {
            CaseInput::U32(v) => copy_adapter!(self, Rc::clone(v), scratch_bytes, |k| *k, 0),
            CaseInput::U64(v) => copy_adapter!(self, Rc::clone(v), scratch_bytes, |k| *k, 1),
            CaseInput::F64(v) => copy_adapter!(self, Rc::clone(v), scratch_bytes, |k| *k, 2),
            CaseInput::Row32(v) => copy_adapter!(self, Rc::clone(v), scratch_bytes, |r| r.key, 3),
            CaseInput::Row64(v) => copy_adapter!(self, Rc::clone(v), scratch_bytes, |r| r.key, 4),
            CaseInput::WideRecord(v) => {
                copy_adapter!(self, Rc::clone(v), scratch_bytes, |r| r.key, 5)
            }
            CaseInput::OwnedRecord(v) => {
                if self.name == "radsort" {
                    Some(prepare_kernel(Rc::clone(v), true, |v| {
                        radsort::sort_by_key(v, |r| r.key)
                    }))
                } else {
                    try_prepare_common(self, Rc::clone(v), scratch_bytes)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adapters_follow_features() {
        let names: Vec<_> = list_variants().iter().map(|v| v.name).collect();
        for id in ["tessera-par-8", "rayon-stable-8"] {
            assert_eq!(names.contains(&id), cfg!(feature = "rayon"), "{id}");
        }
        assert!(names.contains(&"tessera"));
        assert!(names.contains(&"std-stable"));
        assert_eq!(names.contains(&"radsort"), !cfg!(feature = "portable-simd"));
        assert_eq!(
            names.contains(&"fluxsort"),
            cfg!(feature = "compare-native") && !cfg!(feature = "portable-simd")
        );
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
