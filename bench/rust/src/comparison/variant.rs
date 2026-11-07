// SPDX-License-Identifier: BSD-2-Clause

use super::{check, suite::Case};
use crate::case::{Key, Row, generate};
use rdst::RadixSort;
use std::{cmp::Ordering, hint::black_box, mem::MaybeUninit, rc::Rc};
use voracious_radix_sort::{RadixSort as VoraciousSort, Radixable};

pub(super) trait Element: Clone + 'static {
    const HAS_PAYLOAD: bool = false;
    fn order(&self, other: &Self) -> Ordering;
    fn same(&self, other: &Self) -> bool;
    fn index(&self) -> usize {
        0
    }
    fn tessera(v: &mut [Self]);
    fn extra_bytes(&self) -> usize {
        0
    }
    fn glidesort(v: &mut [Self]) {
        glidesort::sort_by(v, Self::order);
    }
    fn glidesort_sqrt(v: &mut [Self], bytes: usize) {
        let mut scratch = Vec::with_capacity(bytes / size_of::<Self>());
        scratch.resize_with(scratch.capacity(), MaybeUninit::uninit);
        glidesort::sort_with_buffer_by(v, &mut scratch, Self::order);
    }
}
macro_rules! key_element {
    ($t:ty) => {
        impl Element for $t {
            fn order(&self, o: &Self) -> Ordering {
                Key::order(self, o)
            }
            fn same(&self, o: &Self) -> bool {
                Key::same(self, o)
            }
            fn tessera(v: &mut [Self]) {
                tessera_sort::stable_sort(v);
            }
        }
    };
}
key_element!(u32);
key_element!(u64);
key_element!(f64);

#[repr(C)]
#[derive(Clone, Copy, PartialEq, PartialOrd)]
pub(super) struct Record {
    pub key: u64,
    pub payload: [u8; 120],
}
#[derive(Clone)]
pub(super) struct Owned {
    key: u64,
    payload: String,
}

macro_rules! record_element {
    ($t:ty, $index:expr, $same:expr) => {
        impl Element for $t {
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
    Record,
    |r: &Record| u64::from_le_bytes(r.payload[..8].try_into().unwrap()) as usize,
    |a: &Record, b: &Record| a == b
);
impl Element for Owned {
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
radix_record!(Record, u64);

pub(super) struct Inputs<T> {
    original: Vec<T>,
    reference: Vec<T>,
    bytes: usize,
}
impl<T: Element> Inputs<T> {
    fn new(original: Vec<T>) -> Rc<Self> {
        let mut reference = original.clone();
        reference.sort_by(T::order);
        let bytes =
            original.len() * size_of::<T>() + original.iter().map(T::extra_bytes).sum::<usize>();
        Rc::new(Self {
            original,
            reference,
            bytes,
        })
    }
}
pub(super) enum Input {
    U32(Rc<Inputs<u32>>),
    U64(Rc<Inputs<u64>>),
    F64(Rc<Inputs<f64>>),
    Row32(Rc<Inputs<Row<u32>>>),
    Row64(Rc<Inputs<Row<u64>>>),
    Record(Rc<Inputs<Record>>),
    Owned(Rc<Inputs<Owned>>),
}
impl Input {
    pub(super) fn new(c: &Case) -> Self {
        match c.ty.as_str() {
            "u32" => Self::U32(Inputs::new(generate::<u32>(&c.distribution, c.len))),
            "u64" => Self::U64(Inputs::new(generate::<u64>(&c.distribution, c.len))),
            "f64" => Self::F64(Inputs::new(generate::<f64>(&c.distribution, c.len))),
            "row_u32" => Self::Row32(Inputs::new(
                generate::<u32>(&c.distribution, c.len)
                    .into_iter()
                    .enumerate()
                    .map(|(i, key)| Row { key, row: i as u32 })
                    .collect(),
            )),
            "row_u64" => Self::Row64(Inputs::new(
                generate::<u64>(&c.distribution, c.len)
                    .into_iter()
                    .enumerate()
                    .map(|(i, key)| Row { key, row: i as u32 })
                    .collect(),
            )),
            "record128" => Self::Record(Inputs::new(
                generate::<u64>(&c.distribution, c.len)
                    .into_iter()
                    .enumerate()
                    .map(|(i, key)| {
                        let mut payload = [0u8; 120];
                        for (j, p) in payload.iter_mut().enumerate() {
                            *p = (i.wrapping_mul(31) + j) as u8;
                        }
                        payload[..8].copy_from_slice(&(i as u64).to_le_bytes());
                        Record { key, payload }
                    })
                    .collect(),
            )),
            "owned" => Self::Owned(Inputs::new(
                generate::<u64>(&c.distribution, c.len)
                    .into_iter()
                    .enumerate()
                    .map(|(i, key)| Owned {
                        key,
                        payload: format!("{i:016x}"),
                    })
                    .collect(),
            )),
            _ => unreachable!("suite contains an unknown type"),
        }
    }
}

pub(super) trait Kernel {
    fn restore(&mut self, batch: usize);
    fn run_batch(&mut self);
    fn check(&self) -> Result<(), String>;
    fn bytes(&self) -> usize;
}
struct Buffers<T, F> {
    input: Rc<Inputs<T>>,
    buffers: Vec<Vec<T>>,
    sort: F,
    stable: bool,
}
impl<T: Element, F: FnMut(&mut [T])> Kernel for Buffers<T, F> {
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
    fn bytes(&self) -> usize {
        self.input.bytes
    }
}
pub(super) fn kernel<T: Element>(
    input: Rc<Inputs<T>>,
    stable: bool,
    sort: impl FnMut(&mut [T]) + 'static,
) -> Box<dyn Kernel> {
    Box::new(Buffers {
        input,
        buffers: Vec::new(),
        sort,
        stable,
    })
}
#[derive(Clone, Copy)]
pub(super) struct Variant {
    pub name: &'static str,
    pub stable: bool,
}
pub(super) fn variants() -> Vec<Variant> {
    let mut v = vec![
        Variant {
            name: "tessera",
            stable: true,
        },
        Variant {
            name: "std-stable",
            stable: true,
        },
    ];
    if !cfg!(feature = "portable-simd") {
        for (name, stable) in [
            ("radsort", true),
            ("voracious", true),
            ("glidesort", true),
            ("glidesort-sqrt", true),
            ("std-unstable", false),
            ("rdst", false),
        ] {
            v.push(Variant { name, stable });
        }
        #[cfg(feature = "compare-native")]
        for (name, stable) in [
            ("clausecker", true),
            ("fluxsort", true),
            ("ips2ra", false),
            ("ips4o", false),
            ("vqsort", false),
            ("x86simdsort", false),
            ("crumsort", false),
        ] {
            v.push(Variant { name, stable });
        }
    }
    v
}
fn base<T: Element>(v: Variant, input: Rc<Inputs<T>>, scratch: usize) -> Option<Box<dyn Kernel>> {
    Some(match v.name {
        "tessera" => kernel(input, true, T::tessera),
        "std-stable" => kernel(input, true, |v| v.sort_by(T::order)),
        "std-unstable" => kernel(input, false, |v| v.sort_unstable_by(T::order)),
        "glidesort" => kernel(input, true, T::glidesort),
        "glidesort-sqrt" => kernel(input, true, move |v| T::glidesort_sqrt(v, scratch)),
        _ => return None,
    })
}
macro_rules! copy_adapter {
    ($v:expr,$input:expr,$scratch:expr,$key:expr,$ty:expr) => {{
        let v = $v;
        let input = $input;
        match v.name {
            "radsort" => Some(kernel(input, true, |v| radsort::sort_by_key(v, $key))),
            "voracious" => Some(kernel(input, true, |v| v.voracious_stable_sort())),
            "rdst" => Some(kernel(input, false, |v| v.radix_sort_unstable())),
            #[cfg(feature = "compare-native")]
            "clausecker" | "fluxsort" | "ips2ra" | "ips4o" | "vqsort" | "x86simdsort"
            | "crumsort" => $crate::comparison::native::prepare(v, input, $ty),
            _ => base(v, input, $scratch),
        }
    }};
}
impl Variant {
    pub(super) fn prepare(self, input: &Input, scratch: usize) -> Option<Box<dyn Kernel>> {
        if self.name == "voracious" && !matches!(input, Input::U32(_) | Input::U64(_)) {
            return None;
        }
        match input {
            Input::U32(v) => copy_adapter!(self, Rc::clone(v), scratch, |k| *k, 0),
            Input::U64(v) => copy_adapter!(self, Rc::clone(v), scratch, |k| *k, 1),
            Input::F64(v) => copy_adapter!(self, Rc::clone(v), scratch, |k| *k, 2),
            Input::Row32(v) => copy_adapter!(self, Rc::clone(v), scratch, |r| r.key, 3),
            Input::Row64(v) => copy_adapter!(self, Rc::clone(v), scratch, |r| r.key, 4),
            Input::Record(v) => copy_adapter!(self, Rc::clone(v), scratch, |r| r.key, 5),
            Input::Owned(v) => {
                if self.name == "radsort" {
                    Some(kernel(Rc::clone(v), true, |v| {
                        radsort::sort_by_key(v, |r| r.key)
                    }))
                } else {
                    base(self, Rc::clone(v), scratch)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bad_sort_is_caught_and_restore_replaces_every_buffer() {
        let input = Inputs::new(vec![3u64, 1, 2]);
        let mut k = kernel(Rc::clone(&input), true, |v| {
            v.fill(0);
        });
        k.restore(3);
        k.run_batch();
        assert!(k.check().is_err());
        let mut k = Buffers {
            input,
            buffers: vec![],
            sort: |v: &mut [u64]| {
                assert_eq!(v, [3, 1, 2]);
                v.sort();
            },
            stable: true,
        };
        for _ in 0..9 {
            k.restore(3);
            k.run_batch();
            k.check().unwrap();
        }
    }
    #[test]
    fn float_adapters_preserve_total_order_and_nan_payloads() {
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
        let input = Input::F64(Inputs::new(input));
        for v in variants() {
            let Some(mut k) = v.prepare(&input, 32768) else {
                continue;
            };
            k.restore(2);
            k.run_batch();
            k.check().unwrap_or_else(|e| panic!("{}: {e}", v.name));
        }
    }
}
