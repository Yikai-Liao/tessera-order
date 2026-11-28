// SPDX-License-Identifier: BSD-2-Clause

//! Record semantics and immutable shared inputs, independent of sorting adapters.

use super::InputCase;
use crate::case::{Key, Row, generate};
use crate::plan::Operation;
use std::{
    cmp::{Ordering, Reverse},
    rc::Rc,
};

use tessera_order::RadixKey;

/// Reproducible finite categories, with no sorting-specific interpretation.
fn category<K: RadixKey>(key: K, bucket_count: usize) -> usize
where
    K::Encoded: Into<u128>,
{
    (key.encode().into() % bucket_count as u128) as usize
}

/// Ordering and payload identity for one benchmark record representation.
pub trait BenchmarkRecord: Clone + Send + Sync + 'static {
    /// Whether tie order and payload identity are observable.
    const HAS_PAYLOAD: bool = false;
    /// Encoded-key category shared by every partition adapter and its reference.
    fn category(&self, bucket_count: usize) -> usize;
    /// Compares sorting keys in the shared total order.
    fn order(&self, other: &Self) -> Ordering;
    /// Compares every observable bit of two records.
    fn same(&self, other: &Self) -> bool;
    /// Returns the original position encoded in a record payload.
    fn index(&self) -> usize {
        0
    }
    /// Returns separately allocated payload capacity for one record.
    fn extra_bytes(&self) -> usize {
        0
    }
}
/// Implements benchmark ordering and identity for scalar keys.
macro_rules! key_element {
    ($t:ty) => {
        impl BenchmarkRecord for $t {
            fn category(&self, bucket_count: usize) -> usize {
                category(*self, bucket_count)
            }
            fn order(&self, o: &Self) -> Ordering {
                Key::order(self, o)
            }
            fn same(&self, o: &Self) -> bool {
                Key::same(self, o)
            }
        }
    };
}
key_element!(u32);
key_element!(u64);
key_element!(u128);
key_element!(f64);

impl<K: Key + Send + Sync + 'static> BenchmarkRecord for Reverse<K>
where
    K::Encoded: Into<u128>,
{
    fn category(&self, bucket_count: usize) -> usize {
        category(*self, bucket_count)
    }
    fn order(&self, other: &Self) -> Ordering {
        other.0.order(&self.0)
    }
    fn same(&self, other: &Self) -> bool {
        self.0.same(&other.0)
    }
}

pub(super) use crate::case::Wide as WideRecord;
/// A keyed record whose allocated string payload carries its original index.
#[derive(Clone)]
pub struct OwnedRecord {
    pub(super) key: u64,
    payload: String,
}

/// Implements benchmark operations for copyable keyed records.
macro_rules! record_element {
    ($t:ty, $index:expr, $same:expr) => {
        impl BenchmarkRecord for $t {
            const HAS_PAYLOAD: bool = true;
            fn category(&self, bucket_count: usize) -> usize {
                category(self.key, bucket_count)
            }
            fn order(&self, o: &Self) -> Ordering {
                self.key.cmp(&o.key)
            }
            fn same(&self, o: &Self) -> bool {
                $same(self, o)
            }
            fn index(&self) -> usize {
                $index(self)
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
    fn category(&self, bucket_count: usize) -> usize {
        category(self.key, bucket_count)
    }
    fn order(&self, other: &Self) -> Ordering {
        self.key.cmp(&other.key)
    }
    fn same(&self, other: &Self) -> bool {
        self.key == other.key && self.payload == other.payload
    }
    fn index(&self) -> usize {
        usize::from_str_radix(&self.payload, 16).unwrap_or(usize::MAX)
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
/// Immutable original records and their stable reference shared by all adapters.
pub struct PreparedRecords<T> {
    operation: Operation,
    original: Vec<T>,
    reference: Vec<T>,
    buffer_bytes: usize,
}
impl<T: BenchmarkRecord> PreparedRecords<T> {
    /// Prepares one immutable input, its reference order and buffer footprint.
    pub fn new(original: Vec<T>) -> Rc<Self> {
        Self::prepare(original, Operation::Sort {})
    }

    fn prepare(original: Vec<T>, operation: Operation) -> Rc<Self> {
        let mut reference = original.clone();
        if let Operation::PartitionBuckets { bucket_count } = operation {
            reference.sort_by_key(|record| record.category(bucket_count));
        } else {
            reference.sort_by(T::order);
        }
        let buffer_bytes =
            original.len() * size_of::<T>() + original.iter().map(T::extra_bytes).sum::<usize>();
        Rc::new(Self {
            operation,
            original,
            reference,
            buffer_bytes,
        })
    }

    /// Prepares a validated operation without retaining per-call workspace.
    fn for_case(case: &InputCase, original: Vec<T>) -> Rc<Self> {
        case.operation
            .validate(original.len())
            .expect("validated operation");
        Self::prepare(original, case.operation.clone())
    }

    /// Returns the result contract for this immutable input.
    pub(super) fn operation(&self) -> &Operation {
        &self.operation
    }

    /// Returns the original vector for `Vec::clone`/`clone_from` batch restoration.
    pub fn original(&self) -> &Vec<T> {
        &self.original
    }

    /// Returns the immutable stable reference used by output verification.
    pub fn reference(&self) -> &[T] {
        &self.reference
    }

    /// Returns record storage and separately allocated payload bytes.
    pub fn buffer_bytes(&self) -> usize {
        self.buffer_bytes
    }
}
/// A prepared case with its concrete record representation preserved.
#[allow(missing_docs)]
pub enum CaseInput {
    U32(Rc<PreparedRecords<u32>>),
    U64(Rc<PreparedRecords<u64>>),
    U128(Rc<PreparedRecords<u128>>),
    F64(Rc<PreparedRecords<f64>>),
    ReverseU64(Rc<PreparedRecords<Reverse<u64>>>),
    ReverseF64(Rc<PreparedRecords<Reverse<f64>>>),
    Row32(Rc<PreparedRecords<Row<u32>>>),
    Row64(Rc<PreparedRecords<Row<u64>>>),
    WideRecord(Rc<PreparedRecords<WideRecord>>),
    OwnedRecord(Rc<PreparedRecords<OwnedRecord>>),
}
impl CaseInput {
    /// Generates and prepares the concrete record representation of a plan case.
    pub fn new(c: &InputCase) -> Self {
        match c.ty.as_str() {
            "u32" => Self::U32(PreparedRecords::for_case(
                c,
                generate::<u32>(&c.distribution, c.len),
            )),
            "u64" => Self::U64(PreparedRecords::for_case(
                c,
                generate::<u64>(&c.distribution, c.len),
            )),
            "u128" => Self::U128(PreparedRecords::for_case(
                c,
                generate::<u128>(&c.distribution, c.len),
            )),
            "f64" => Self::F64(PreparedRecords::for_case(
                c,
                generate::<f64>(&c.distribution, c.len),
            )),
            // Complementing integers and flipping float signs complements their
            // encoded keys. Reverse complements them again, preserving the base
            // generator's encoded-order geometry, including sorted inputs.
            "reverse_u64" => Self::ReverseU64(PreparedRecords::for_case(
                c,
                generate::<u64>(&c.distribution, c.len)
                    .into_iter()
                    .map(|key| Reverse(!key))
                    .collect(),
            )),
            "reverse_f64" => Self::ReverseF64(PreparedRecords::for_case(
                c,
                generate::<f64>(&c.distribution, c.len)
                    .into_iter()
                    .map(|key| Reverse(-key))
                    .collect(),
            )),
            "row_u32" => Self::Row32(PreparedRecords::for_case(
                c,
                generate::<u32>(&c.distribution, c.len)
                    .into_iter()
                    .enumerate()
                    .map(|(i, key)| Row { key, row: i as u32 })
                    .collect(),
            )),
            "row_u64" => Self::Row64(PreparedRecords::for_case(
                c,
                generate::<u64>(&c.distribution, c.len)
                    .into_iter()
                    .enumerate()
                    .map(|(i, key)| Row { key, row: i as u32 })
                    .collect(),
            )),
            // These are distinct inputs, not harness-specific representations.
            "rec128_u64" => Self::WideRecord(PreparedRecords::for_case(
                c,
                generate::<u64>(&c.distribution, c.len)
                    .into_iter()
                    .enumerate()
                    .map(|(index, key)| WideRecord::new(key, index))
                    .collect(),
            )),
            "record128" => Self::WideRecord(PreparedRecords::for_case(
                c,
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
            "owned" => Self::OwnedRecord(PreparedRecords::for_case(
                c,
                generate::<u64>(&c.distribution, c.len)
                    .into_iter()
                    .enumerate()
                    .map(|(i, key)| OwnedRecord {
                        key,
                        payload: format!("{i:016x}"),
                    })
                    .collect(),
            )),
            _ => unreachable!("plan contains an unknown type"),
        }
    }

    /// Result contract shared by every typed representation.
    pub(super) fn operation(&self) -> &Operation {
        match self {
            Self::U32(v) => v.operation(),
            Self::U64(v) => v.operation(),
            Self::U128(v) => v.operation(),
            Self::F64(v) => v.operation(),
            Self::ReverseU64(v) => v.operation(),
            Self::ReverseF64(v) => v.operation(),
            Self::Row32(v) => v.operation(),
            Self::Row64(v) => v.operation(),
            Self::WideRecord(v) => v.operation(),
            Self::OwnedRecord(v) => v.operation(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tessera_order::RadixKey;

    #[test]
    fn reverse_inputs_preserve_encoded_geometry() {
        // Both representations must exercise the same disorder and variation,
        // not turn the sorted scan control into a descending fast path.
        for distribution in [
            "random",
            "sorted",
            "sorted99",
            "far_swaps",
            "skew99",
            "finite_signed",
        ] {
            for ty in ["reverse_u64", "reverse_f64"] {
                let case = InputCase {
                    ty: ty.into(),
                    distribution: distribution.into(),
                    len: 1000,
                    operation: Operation::Sort {},
                };
                match CaseInput::new(&case) {
                    CaseInput::ReverseU64(input) => {
                        for (reverse, base) in input
                            .original()
                            .iter()
                            .zip(generate::<u64>(distribution, case.len))
                        {
                            assert_eq!(reverse.encode(), base.encode(), "{ty}/{distribution}");
                        }
                    }
                    CaseInput::ReverseF64(input) => {
                        for (reverse, base) in input
                            .original()
                            .iter()
                            .zip(generate::<f64>(distribution, case.len))
                        {
                            assert_eq!(reverse.encode(), base.encode(), "{ty}/{distribution}");
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
    }

    #[test]
    fn reverse_float_identity_is_bit_exact() {
        let negative_zero = Reverse(-0.0f64);
        let positive_zero = Reverse(0.0f64);
        assert!(negative_zero.order(&positive_zero).is_gt());
        assert!(!negative_zero.same(&positive_zero));
        let nan = Reverse(f64::from_bits(0x7ff8_0000_0000_0001));
        let other_nan = Reverse(f64::from_bits(0x7ff8_0000_0000_0002));
        assert!(nan.same(&nan));
        assert!(!nan.same(&other_nan));
        assert!(nan.order(&other_nan).is_gt());
    }
}
/// Hashes logical record fields with their canonical little-endian encoding.
pub(super) fn input_digest(input: &CaseInput) -> u64 {
    fn digest<T: BenchmarkRecord>(
        records: &PreparedRecords<T>,
        bytes: impl Fn(&T, &mut Vec<u8>),
    ) -> u64 {
        let mut record = Vec::with_capacity(128);
        let mut hash = 0xcbf29ce484222325u64;
        for value in records.original() {
            record.clear();
            bytes(value, &mut record);
            for &byte in &record {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
            }
        }
        hash
    }
    match input {
        CaseInput::U32(v) => digest(v, |v, b| b.extend(v.to_le_bytes())),
        CaseInput::U64(v) => digest(v, |v, b| b.extend(v.to_le_bytes())),
        CaseInput::U128(v) => digest(v, |v, b| b.extend(v.to_le_bytes())),
        CaseInput::F64(v) => digest(v, |v, b| b.extend(v.to_bits().to_le_bytes())),
        CaseInput::ReverseU64(v) => digest(v, |v, b| b.extend(v.0.to_le_bytes())),
        CaseInput::ReverseF64(v) => digest(v, |v, b| b.extend(v.0.to_bits().to_le_bytes())),
        CaseInput::Row32(v) => digest(v, |v, b| {
            b.extend(v.key.to_le_bytes());
            b.extend(v.row.to_le_bytes());
        }),
        CaseInput::Row64(v) => digest(v, |v, b| {
            b.extend(v.key.to_le_bytes());
            b.extend(v.row.to_le_bytes());
        }),
        CaseInput::WideRecord(v) => digest(v, |v, b| {
            b.extend(v.key.to_le_bytes());
            b.extend(v.payload);
        }),
        CaseInput::OwnedRecord(v) => digest(v, |v, b| {
            b.extend(v.key.to_le_bytes());
            b.extend((v.payload.len() as u64).to_le_bytes());
            b.extend(v.payload.as_bytes());
        }),
    }
}

#[cfg(test)]
mod identity_tests {
    use super::input_digest;
    use crate::plan::{InputCase, Operation, catalog};
    use serde_json::{Value, json};

    #[test]
    fn benchmark_input_vectors() {
        // All generator branches, both sides of the sorted-tail boundary,
        // and records long enough to exercise run transitions.
        let lengths = [127, 4097];
        let mut types = serde_json::Map::new();
        for ty in &catalog().types {
            let mut distributions = serde_json::Map::new();
            for distribution in &catalog().distributions {
                let digests = lengths.map(|len| {
                    let case = InputCase {
                        ty: ty.id.clone(),
                        distribution: distribution.id.clone(),
                        len,
                        operation: Operation::default(),
                    };
                    format!("{:016x}", input_digest(&super::CaseInput::new(&case)))
                });
                distributions.insert(distribution.id.clone(), json!(digests));
            }
            types.insert(
                ty.id.clone(),
                json!({
                    "record_bytes": crate::plan::record_bytes(&ty.id),
                    "distributions": distributions,
                }),
            );
        }
        let actual = json!({"schema": 2, "seed": 1, "lengths": lengths, "types": types});
        if let Some(path) = std::env::var_os("TESSERA_WRITE_INPUT_VECTORS") {
            // Keep one distribution's digests per line so generator changes are reviewable.
            use std::io::Write;
            let mut file = std::fs::File::create(path).unwrap();
            writeln!(
                file,
                "{{\"schema\":2,\"seed\":1,\"lengths\":{},\"types\":{{",
                actual["lengths"]
            )
            .unwrap();
            let types = actual["types"].as_object().unwrap();
            for (index, (ty, spec)) in types.iter().enumerate() {
                writeln!(
                    file,
                    "  {}:{{\"record_bytes\":{},\"distributions\":{{",
                    json!(ty),
                    spec["record_bytes"]
                )
                .unwrap();
                let distributions = spec["distributions"].as_object().unwrap();
                for (index, (distribution, digests)) in distributions.iter().enumerate() {
                    let comma = if index + 1 == distributions.len() {
                        ""
                    } else {
                        ","
                    };
                    writeln!(file, "    {}:{digests}{comma}", json!(distribution)).unwrap();
                }
                let comma = if index + 1 == types.len() { "" } else { "," };
                writeln!(file, "  }}}}{comma}").unwrap();
            }
            writeln!(file, "}}}}").unwrap();
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../conformance/vectors/benchmark-inputs.json");
        let expected: Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(actual, expected);
    }
}
