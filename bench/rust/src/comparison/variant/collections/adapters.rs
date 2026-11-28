// SPDX-License-Identifier: BSD-2-Clause

use super::input::{IntKey, Value};
use crate::plan::Operation;
use std::collections::{BTreeMap, BTreeSet};

pub(super) trait Collection<K: Copy, V: Copy>: Sized + 'static {
    const NAME: &'static str;
    const BUILD: &'static str;
    const ENDPOINTS: &'static str;
    const ORDER_STATISTICS: bool = false;
    const CHURN: &'static str = "per-pair remove then insert";

    fn route(operation: &Operation) -> Option<&'static str> {
        match operation {
            Operation::MapBuild {} | Operation::MapBuildGet { .. } => Some(Self::BUILD),
            Operation::MapFirst {} | Operation::MapLast {} => Some(Self::ENDPOINTS),
            Operation::MapRank { .. } | Operation::MapSelect { .. } if !Self::ORDER_STATISTICS => {
                None
            }
            _ => Some(Self::NAME),
        }
    }
    fn empty() -> Self;
    fn build(entries: Vec<(K, V)>) -> Self;
    fn get(&self, key: K) -> Option<V>;
    fn insert(&mut self, key: K, value: V) -> Option<V>;
    fn remove(&mut self, key: K) -> Option<V>;
    fn churn(&mut self, trace: &[(K, K, V)], results: &mut [(Option<V>, Option<V>)]) {
        for (&(old, key, value), result) in trace.iter().zip(results) {
            *result = (self.remove(old), self.insert(key, value));
        }
    }
    fn visit(&self, bounds: Option<(K, K)>, visit: impl FnMut((K, V)));
    fn endpoint(&self, last: bool) -> Option<(K, V)>;
    fn rank(&self, _key: K) -> usize {
        unreachable!()
    }
    fn select(&self, _index: usize) -> Option<(K, V)> {
        unreachable!()
    }
}

macro_rules! ordered_map {
    ($adapter:ident, $map:ident, $name:literal, {$($rank:item)*} $(,)?) => {
        pub(super) struct $adapter<K: Ord,V>($map<K,V>);
        impl<K: IntKey, V: Value> Collection<K,V> for $adapter<K,V> {
            const NAME: &'static str = $name;
            const BUILD: &'static str = concat!($name, "::from_iter");
            const ENDPOINTS: &'static str = concat!($name, "::first_key_value / last_key_value");
            fn empty() -> Self { Self($map::new()) }
            fn build(entries: Vec<(K,V)>) -> Self { Self(entries.into_iter().collect()) }
            fn get(&self, key: K) -> Option<V> { self.0.get(&key).copied() }
            fn insert(&mut self, key: K, value: V) -> Option<V> { self.0.insert(key, value) }
            fn remove(&mut self, key: K) -> Option<V> { self.0.remove(&key) }
            fn visit(&self, bounds: Option<(K,K)>, mut visit: impl FnMut((K,V))) {
                if let Some((lo,hi)) = bounds {
                    self.0.range(lo..=hi).for_each(|(&k,&v)| visit((k,v)));
                } else {
                    self.0.iter().for_each(|(&k,&v)| visit((k,v)));
                }
            }
            fn endpoint(&self, last: bool) -> Option<(K,V)> {
                if last { self.0.last_key_value() } else { self.0.first_key_value() }.map(|(&k,&v)| (k,v))
            }
            $($rank)*
        }
    };
}
use indexset::{BTreeMap as IndexMap, BTreeSet as IndexSet};
ordered_map!(StdMap, BTreeMap, "std::collections::BTreeMap", {});
ordered_map!(IndexedMap, IndexMap, "indexset::BTreeMap", {
    const ORDER_STATISTICS: bool = true;
    fn rank(&self, key: K) -> usize {
        self.0.rank(&key)
    }
    fn select(&self, index: usize) -> Option<(K, V)> {
        self.0.get_index(index).map(|(&k, &v)| (k, v))
    }
});

macro_rules! ordered_set {
    ($adapter:ident, $set:ident, $key:ty, $name:literal, $encode:expr, $decode:expr, {$($rank:item)*} $(,)?) => {
        pub(super) struct $adapter($set<$key>);
        impl Collection<u64,()> for $adapter {
            const NAME: &'static str = $name;
            const BUILD: &'static str = concat!($name, "::from_iter");
            const ENDPOINTS: &'static str = concat!($name, "::first / last");
            fn empty() -> Self { Self($set::new()) }
            fn build(entries: Vec<(u64,())>) -> Self { Self(entries.into_iter().map(|(k, ())| $encode(k)).collect()) }
            fn get(&self, key: u64) -> Option<()> { self.0.get(&$encode(key)).map(|_| ()) }
            fn insert(&mut self, key: u64, (): ()) -> Option<()> { (!self.0.insert($encode(key))).then_some(()) }
            fn remove(&mut self, key: u64) -> Option<()> { self.0.remove(&$encode(key)).then_some(()) }
            fn visit(&self, bounds: Option<(u64,u64)>, mut visit: impl FnMut((u64,()))) {
                if let Some((lo,hi)) = bounds {
                    self.0.range($encode(lo)..=$encode(hi)).for_each(|&k| visit(($decode(k),())));
                } else {
                    self.0.iter().for_each(|&k| visit(($decode(k),())));
                }
            }
            fn endpoint(&self, last: bool) -> Option<(u64,())> {
                if last { self.0.last() } else { self.0.first() }.map(|&k| ($decode(k),()))
            }
            $($rank)*
        }
    };
}
use blart::TreeSet;
ordered_set!(
    StdSet,
    BTreeSet,
    u64,
    "std::collections::BTreeSet",
    std::convert::identity,
    std::convert::identity,
    {},
);
ordered_set!(
    IndexedSet,
    IndexSet,
    u64,
    "indexset::BTreeSet",
    std::convert::identity,
    std::convert::identity,
    {
        const ORDER_STATISTICS: bool = true;
        fn rank(&self, key: u64) -> usize {
            self.0.rank(&key)
        }
        fn select(&self, index: usize) -> Option<(u64, ())> {
            self.0.get_index(index).map(|&k| (k, ()))
        }
    }
);
ordered_set!(
    ArtSet,
    TreeSet,
    <u64 as IntKey>::Art,
    "blart::TreeSet (big-endian keys)",
    u64::art,
    u64::from_art,
    {},
);

pub(super) struct ArtMap<K: IntKey, V>(blart::TreeMap<K::Art, V>);
impl<K: IntKey, V: Value> Collection<K, V> for ArtMap<K, V> {
    const NAME: &'static str = "blart::TreeMap (big-endian keys)";
    const BUILD: &'static str = "blart::TreeMap::from_iter (big-endian keys)";
    const ENDPOINTS: &'static str = "blart::TreeMap::first_key_value / last_key_value";
    fn empty() -> Self {
        Self(blart::TreeMap::new())
    }
    fn build(entries: Vec<(K, V)>) -> Self {
        Self(entries.into_iter().map(|(k, v)| (k.art(), v)).collect())
    }
    fn get(&self, key: K) -> Option<V> {
        self.0.get(&key.art()).copied()
    }
    fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.0.insert(key.art(), value)
    }
    fn remove(&mut self, key: K) -> Option<V> {
        self.0.remove(&key.art())
    }
    fn visit(&self, bounds: Option<(K, K)>, mut visit: impl FnMut((K, V))) {
        if let Some((lo, hi)) = bounds {
            self.0
                .range(lo.art()..=hi.art())
                .for_each(|(&k, &v)| visit((K::from_art(k), v)));
        } else {
            self.0
                .iter()
                .for_each(|(&k, &v)| visit((K::from_art(k), v)));
        }
    }
    fn endpoint(&self, last: bool) -> Option<(K, V)> {
        if last {
            self.0.last_key_value()
        } else {
            self.0.first_key_value()
        }
        .map(|(&k, &v)| (K::from_art(k), v))
    }
}

pub(super) struct SortedVec<K, V>(Vec<(K, V)>);
fn sorted_last<K: Ord + Copy, V: Copy>(mut entries: Vec<(K, V)>) -> Vec<(K, V)> {
    entries.sort_by_key(|entry| entry.0);
    entries.dedup_by(|later, kept| {
        if later.0 != kept.0 {
            return false;
        }
        *kept = *later;
        true
    });
    entries
}
impl<K: IntKey, V: Value> Collection<K, V> for SortedVec<K, V> {
    const NAME: &'static str = "sorted Vec (binary search)";
    const BUILD: &'static str = "Vec::sort_by_key + last-write deduplication";
    const ENDPOINTS: &'static str = "Vec::first / last";
    const CHURN: &'static str = "sort churn events by key and merge";
    const ORDER_STATISTICS: bool = true;
    fn empty() -> Self {
        Self(Vec::new())
    }
    fn build(entries: Vec<(K, V)>) -> Self {
        Self(sorted_last(entries))
    }
    fn get(&self, key: K) -> Option<V> {
        self.0
            .binary_search_by_key(&key, |e| e.0)
            .ok()
            .map(|i| self.0[i].1)
    }
    fn insert(&mut self, key: K, value: V) -> Option<V> {
        match self.0.binary_search_by_key(&key, |e| e.0) {
            Ok(i) => Some(std::mem::replace(&mut self.0[i].1, value)),
            Err(i) => {
                self.0.insert(i, (key, value));
                None
            }
        }
    }
    fn remove(&mut self, key: K) -> Option<V> {
        self.0
            .binary_search_by_key(&key, |e| e.0)
            .ok()
            .map(|i| self.0.remove(i).1)
    }
    fn churn(&mut self, trace: &[(K, K, V)], results: &mut [(Option<V>, Option<V>)]) {
        let event_key = |event: u32| {
            let (old, key, _) = trace[event as usize / 2];
            if event.is_multiple_of(2) { old } else { key }
        };
        // Even events remove, odd events insert. The event number preserves each key's call order.
        let mut events: Vec<_> = (0..u32::try_from(2 * trace.len()).unwrap()).collect();
        events.sort_unstable_by_key(|&event| (event_key(event), event));

        let mut merged = Vec::with_capacity(self.0.capacity());
        let mut entries = self.0.iter().copied().peekable();
        for group in events.chunk_by(|&a, &b| event_key(a) == event_key(b)) {
            let key = event_key(group[0]);
            while let Some(entry) = entries.next_if(|entry| entry.0 < key) {
                merged.push(entry);
            }
            let mut value = entries.next_if(|entry| entry.0 == key).map(|entry| entry.1);
            for &event in group {
                let pair = event as usize / 2;
                if event.is_multiple_of(2) {
                    results[pair].0 = value.take();
                } else {
                    results[pair].1 = value.replace(trace[pair].2);
                }
            }
            if let Some(value) = value {
                merged.push((key, value));
            }
        }
        merged.extend(entries);
        self.0 = merged;
    }
    fn visit(&self, bounds: Option<(K, K)>, visit: impl FnMut((K, V))) {
        let entries = if let Some((lo, hi)) = bounds {
            &self.0[self.rank(lo)..self.0.partition_point(|entry| entry.0 <= hi)]
        } else {
            &self.0[..]
        };
        entries.iter().copied().for_each(visit);
    }
    fn endpoint(&self, last: bool) -> Option<(K, V)> {
        if last { self.0.last() } else { self.0.first() }.copied()
    }
    fn rank(&self, key: K) -> usize {
        self.0.partition_point(|entry| entry.0 < key)
    }
    fn select(&self, index: usize) -> Option<(K, V)> {
        self.0.get(index).copied()
    }
}

pub(super) struct DbMap<K, V>(index_db::BPlusTree<K, V>);
impl<K: IntKey, V: Value> Collection<K, V> for DbMap<K, V> {
    const NAME: &'static str = "index_db::BPlusTree";
    const BUILD: &'static str = "stable sort + last-write deduplication + BPlusTree::from_sorted";
    const ENDPOINTS: &'static str = "index_db::BPlusTree::iter().next / next_back";
    fn empty() -> Self {
        Self(index_db::BPlusTree::new())
    }
    fn build(entries: Vec<(K, V)>) -> Self {
        Self(index_db::BPlusTree::from_sorted(sorted_last(entries)))
    }
    fn get(&self, key: K) -> Option<V> {
        self.0.get(&key).copied()
    }
    fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.0.insert(key, value)
    }
    fn remove(&mut self, key: K) -> Option<V> {
        self.0.remove(&key)
    }
    fn visit(&self, bounds: Option<(K, K)>, mut visit: impl FnMut((K, V))) {
        if let Some((lo, hi)) = bounds {
            self.0.range(lo..=hi).for_each(|(&k, &v)| visit((k, v)));
        } else {
            self.0.iter().for_each(|(&k, &v)| visit((k, v)));
        }
    }
    fn endpoint(&self, last: bool) -> Option<(K, V)> {
        let mut iter = self.0.iter();
        if last { iter.next_back() } else { iter.next() }.map(|(&k, &v)| (k, v))
    }
}

pub(super) struct LazyMap<K: tessera_order::RadixKey, V>(tessera_order::TesseraMap<K, V>);

impl<K: IntKey, V: Value> Collection<K, V> for LazyMap<K, V> {
    const NAME: &'static str = "TesseraMap";
    const BUILD: &'static str = "TesseraMap::from(Vec)";
    const ENDPOINTS: &'static str = "TesseraMap::first / last";

    fn route(operation: &Operation) -> Option<&'static str> {
        match operation {
            Operation::MapRank { .. } => None,
            Operation::MapSelect { .. } => Some("TesseraMap::select_nth"),
            Operation::MapBuild {} | Operation::MapBuildGet { .. } => Some(Self::BUILD),
            Operation::MapFirst {} | Operation::MapLast {} => Some(Self::ENDPOINTS),
            _ => Some(Self::NAME),
        }
    }

    fn empty() -> Self {
        Self(tessera_order::TesseraMap::new())
    }
    fn build(entries: Vec<(K, V)>) -> Self {
        Self(entries.into())
    }
    fn get(&self, key: K) -> Option<V> {
        self.0.get(key).copied()
    }
    fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.0.insert(key, value)
    }
    fn remove(&mut self, key: K) -> Option<V> {
        self.0.remove(key)
    }
    fn visit(&self, bounds: Option<(K, K)>, mut visit: impl FnMut((K, V))) {
        if let Some((lo, hi)) = bounds {
            self.0.range(lo..=hi, |k, &v| visit((k, v)));
        } else {
            self.0.range(.., |k, &v| visit((k, v)));
        }
    }
    fn endpoint(&self, last: bool) -> Option<(K, V)> {
        if last { self.0.last() } else { self.0.first() }.map(|(k, &v)| (k, v))
    }
    fn select(&self, index: usize) -> Option<(K, V)> {
        self.0.select_nth(index).map(|(k, &v)| (k, v))
    }
}

pub(super) struct LazySet(tessera_order::TesseraSet<u64>);

impl Collection<u64, ()> for LazySet {
    const NAME: &'static str = "TesseraSet";
    const BUILD: &'static str = "TesseraSet::from(Vec)";
    const ENDPOINTS: &'static str = "TesseraSet::first / last";

    fn route(operation: &Operation) -> Option<&'static str> {
        match operation {
            Operation::MapRank { .. } => None,
            Operation::MapSelect { .. } => Some("TesseraSet::select_nth"),
            Operation::MapBuild {} | Operation::MapBuildGet { .. } => Some(Self::BUILD),
            Operation::MapFirst {} | Operation::MapLast {} => Some(Self::ENDPOINTS),
            _ => Some(Self::NAME),
        }
    }

    fn empty() -> Self {
        Self(tessera_order::TesseraSet::new())
    }
    fn build(entries: Vec<(u64, ())>) -> Self {
        Self(
            entries
                .into_iter()
                .map(|(k, ())| k)
                .collect::<Vec<_>>()
                .into(),
        )
    }
    fn get(&self, key: u64) -> Option<()> {
        self.0.contains(key).then_some(())
    }
    fn insert(&mut self, key: u64, (): ()) -> Option<()> {
        (!self.0.insert(key)).then_some(())
    }
    fn remove(&mut self, key: u64) -> Option<()> {
        self.0.remove(key).then_some(())
    }
    fn visit(&self, bounds: Option<(u64, u64)>, mut visit: impl FnMut((u64, ()))) {
        if let Some((lo, hi)) = bounds {
            self.0.range(lo..=hi, |k| visit((k, ())));
        } else {
            self.0.range(.., |k| visit((k, ())));
        }
    }
    fn endpoint(&self, last: bool) -> Option<(u64, ())> {
        if last { self.0.last() } else { self.0.first() }.map(|k| (k, ()))
    }
    fn select(&self, index: usize) -> Option<(u64, ())> {
        self.0.select_nth(index).map(|k| (k, ()))
    }
}
