// SPDX-License-Identifier: BSD-2-Clause

//! Record semantics and immutable shared inputs, independent of sorting adapters.

use super::InputCase;
use crate::case::{Key, Row, generate};
use std::{cmp::Ordering, rc::Rc};

/// Ordering and payload identity for one benchmark record representation.
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
    /// Returns separately allocated payload capacity for one record.
    fn extra_bytes(&self) -> usize {
        0
    }
}
/// Implements benchmark ordering and identity for scalar keys.
macro_rules! key_element {
    ($t:ty) => {
        impl BenchmarkRecord for $t {
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
    pub(super) key: u64,
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
pub(super) struct PreparedRecords<T> {
    original: Vec<T>,
    reference: Vec<T>,
    buffer_bytes: usize,
}
impl<T: BenchmarkRecord> PreparedRecords<T> {
    /// Prepares one immutable input, its reference order and buffer footprint.
    pub(super) fn new(original: Vec<T>) -> Rc<Self> {
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

    /// Returns the original vector for `Vec::clone`/`clone_from` batch restoration.
    pub(super) fn original(&self) -> &Vec<T> {
        &self.original
    }

    /// Returns the immutable stable reference used by output verification.
    pub(super) fn reference(&self) -> &[T] {
        &self.reference
    }

    /// Returns record storage and separately allocated payload bytes.
    pub(super) fn buffer_bytes(&self) -> usize {
        self.buffer_bytes
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
