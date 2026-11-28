// SPDX-License-Identifier: BSD-2-Clause

use super::Collection;
use crate::{
    case::SplitMix64,
    plan::{InputCase, Operation},
};
use blart::{AsBytes, Mapped, NoPrefixesBytes, ToUBE};
use std::{collections::BTreeMap, fmt::Debug};

pub(super) trait IntKey: Ord + Copy + Debug + 'static {
    type Art: AsBytes + NoPrefixesBytes + Ord + Copy;
    const BITS: u32;
    fn from_u64(value: u64) -> Self;
    fn art(self) -> Self::Art;
    fn from_art(key: Self::Art) -> Self;
}
impl IntKey for u32 {
    type Art = Mapped<ToUBE, Self>;
    const BITS: u32 = 32;
    fn from_u64(value: u64) -> Self {
        value as Self
    }
    fn art(self) -> Self::Art {
        Mapped::new(self)
    }
    fn from_art(key: Self::Art) -> Self {
        key.get()
    }
}
impl IntKey for u64 {
    type Art = Mapped<ToUBE, Self>;
    const BITS: u32 = 64;
    fn from_u64(value: u64) -> Self {
        value
    }
    fn art(self) -> Self::Art {
        Mapped::new(self)
    }
    fn from_art(key: Self::Art) -> Self {
        key.get()
    }
}

pub(super) trait Value: Copy + Eq + Debug + 'static {
    fn from_position(position: usize) -> Self;
}
impl Value for u32 {
    fn from_position(position: usize) -> Self {
        position as Self
    }
}
impl Value for u64 {
    fn from_position(position: usize) -> Self {
        position as Self
    }
}
impl Value for () {
    fn from_position(_: usize) -> Self {}
}
impl Value for [u8; 32] {
    fn from_position(position: usize) -> Self {
        let mut rng = SplitMix64(position as u64);
        let mut payload = [0; 32];
        for word in payload.as_chunks_mut::<8>().0 {
            *word = rng.next_u64().to_le_bytes();
        }
        payload
    }
}

#[derive(Clone, Copy)]
pub(super) enum Action<K, V> {
    Get(K),
    Insert(K, V),
    Remove(K),
    Range(K, K),
    Iterate,
    First,
    Last,
    Rank(K),
    Select(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Expected<K, V> {
    Value(Option<V>),
    Entry(Option<(K, V)>),
    Rank(usize),
}

pub(super) struct Fixture<K, V> {
    pub operation: Operation,
    pub original: Vec<(K, V)>,
    pub actions: Vec<Action<K, V>>,
    distribution: String,
    pub final_entries: Option<Vec<(K, V)>>,
    expected: Vec<Expected<K, V>>,
    visited: Vec<(K, V)>,
    initial: Vec<(K, V)>,
    absent: Vec<K>,
}

fn keys<K: IntKey>(distribution: &str, index: usize) -> K {
    let index = index as u64;
    let mask = u64::MAX >> (64 - K::BITS);
    let key = match distribution {
        "dense" => index,
        "clustered" => {
            let cluster = index % 8;
            let span = (mask >> 3) + 1;
            let offset = SplitMix64(100 + cluster).next_u64() % (span / 2);
            cluster * span + offset + index / 8
        }
        "random" => {
            let a = (index ^ 0x19b2_1cfa_4d83_2cc7) & mask;
            let b = ((a ^ (a >> 17)).wrapping_mul(0xbf58_476d_1ce4_e5b9)) & mask;
            let c = ((b ^ (b >> 13)).wrapping_mul(0x94d0_49bb_1331_11eb)) & mask;
            c ^ (c >> 11)
        }
        _ => unreachable!(),
    };
    K::from_u64(key)
}

fn choose(rng: &mut SplitMix64, len: usize, hot_pct: usize) -> usize {
    let hot_len = len.div_ceil(100).max(1);
    let hot = hot_pct > 0 && rng.next_u64() % 100 < hot_pct as u64;
    if hot_pct == 0 || hot_len == len {
        rng.next_u64() as usize % len
    } else if hot {
        rng.next_u64() as usize % hot_len
    } else {
        hot_len + rng.next_u64() as usize % (len - hot_len)
    }
}

impl<K: IntKey, V: Value> Fixture<K, V> {
    pub(super) fn new(case: &InputCase) -> Self {
        let n = case.len;
        let operation = case.operation.clone();
        let mut rng = SplitMix64(1);
        let mut original: Vec<_> = (0..n)
            .map(|i| (keys::<K>(&case.distribution, i), V::from_position(i)))
            .collect();
        let duplicates = operation.collection_input_entries(n) - n;
        if duplicates > 0 {
            original.reserve_exact(duplicates);
            for i in 0..duplicates {
                original.push((
                    original[rng.next_u64() as usize % n].0,
                    V::from_position(n + i),
                ));
            }
        }
        if matches!(operation, Operation::MapBuildInsertAscending {}) {
            original.sort_unstable_by_key(|entry| entry.0);
        } else {
            for i in (1..original.len()).rev() {
                original.swap(i, rng.next_u64() as usize % (i + 1));
            }
        }
        // Sorting by original position inside each key group is independent of adapter insertion.
        let mut positions: Vec<u32> = (0..original.len() as u32).collect();
        positions.sort_unstable_by_key(|&i| (original[i as usize].0, i));
        let mut initial = Vec::with_capacity(n);
        for position in positions {
            let entry = original[position as usize];
            if initial
                .last()
                .is_some_and(|last: &(K, V)| last.0 == entry.0)
            {
                *initial.last_mut().unwrap() = entry;
            } else {
                initial.push(entry);
            }
        }
        let trace_len = n.min(4096);
        let absent = (n..n + trace_len.max(5))
            .map(|i| keys::<K>(&case.distribution, i))
            .collect();
        let mut data = Self {
            operation,
            original,
            actions: Vec::new(),
            distribution: case.distribution.clone(),
            final_entries: None,
            expected: Vec::new(),
            visited: Vec::new(),
            initial,
            absent,
        };
        data.trace(trace_len, &mut rng);
        data
    }

    fn trace(&mut self, count: usize, rng: &mut SplitMix64) {
        let initial = &self.initial;
        let n = initial.len();
        let mut actions = Vec::new();
        let mut final_entries = None;
        let mut expected = Vec::new();
        let mut visited = Vec::new();
        match self.operation {
            Operation::MapBuild {} => {}
            Operation::MapBuildInsertRandom {} | Operation::MapBuildInsertAscending {} => {
                expected.resize(n, Expected::Value(None));
            }
            Operation::MapGetHit { hot_pct } | Operation::MapBuildGet { hot_pct, .. } => {
                let count = match self.operation {
                    Operation::MapBuildGet { .. } => self.operation.cold_queries(n),
                    _ => count,
                };
                for _ in 0..count {
                    let (key, value) = initial[choose(rng, n, hot_pct)];
                    actions.push(Action::Get(key));
                    expected.push(Expected::Value(Some(value)));
                }
            }
            Operation::MapGetMiss { hot_pct } => {
                for _ in 0..count {
                    actions.push(Action::Get(
                        self.absent[choose(rng, self.absent.len(), hot_pct)],
                    ));
                    expected.push(Expected::Value(None));
                }
            }
            Operation::MapInsertNew {} => {
                let mut oracle: BTreeMap<_, _> = initial.iter().copied().collect();
                for (i, &key) in self.absent.iter().take(count).enumerate() {
                    let value = V::from_position(n + i);
                    actions.push(Action::Insert(key, value));
                    expected.push(Expected::Value(oracle.insert(key, value)));
                }
                final_entries = Some(oracle.into_iter().collect());
            }
            Operation::MapInsertOverwrite {} | Operation::MapRemove {} => {
                let mut oracle: BTreeMap<_, _> = initial.iter().copied().collect();
                for (i, &(key, _)) in self.original.iter().take(count).enumerate() {
                    let old = if matches!(self.operation, Operation::MapRemove {}) {
                        actions.push(Action::Remove(key));
                        oracle.remove(&key)
                    } else {
                        let value = V::from_position(n + i);
                        actions.push(Action::Insert(key, value));
                        oracle.insert(key, value)
                    };
                    expected.push(Expected::Value(old));
                }
                final_entries = Some(oracle.into_iter().collect());
            }
            Operation::MapRange { visit_len, hot_pct } => {
                for _ in 0..self.operation.work_items(n) {
                    let start = choose(rng, n - visit_len + 1, hot_pct);
                    let range = &initial[start..start + visit_len];
                    actions.push(Action::Range(range[0].0, range[visit_len - 1].0));
                    visited.extend_from_slice(range);
                }
            }
            Operation::MapIterate {} => actions.push(Action::Iterate),
            Operation::MapFirst {} | Operation::MapLast {} => {
                let (action, entry) = if matches!(self.operation, Operation::MapFirst {}) {
                    (Action::First, initial.first())
                } else {
                    (Action::Last, initial.last())
                };
                actions.resize(count, action);
                expected.resize(count, Expected::Entry(entry.copied()));
            }
            Operation::MapRank { hot_pct } => {
                for _ in 0..count {
                    let key = self.absent[choose(rng, self.absent.len(), hot_pct)];
                    actions.push(Action::Rank(key));
                    expected.push(Expected::Rank(
                        initial.partition_point(|entry| entry.0 < key),
                    ));
                }
            }
            Operation::MapSelect { hot_pct } => {
                for _ in 0..count {
                    let index = choose(rng, n, hot_pct);
                    actions.push(Action::Select(index));
                    expected.push(Expected::Entry(Some(initial[index])));
                }
            }
            Operation::MapMixed { write_pct, hot_pct } => {
                let count = self.operation.work_items(n);
                let mut live = initial.clone();
                let mut oracle: BTreeMap<_, _> = initial.iter().copied().collect();
                for block in 0..count / 20 {
                    for _ in 0..20 - write_pct / 5 {
                        let key = live[choose(rng, n, hot_pct)].0;
                        actions.push(Action::Get(key));
                        expected.push(Expected::Value(oracle.get(&key).copied()));
                    }
                    for pair in 0..write_pct / 10 {
                        let index = choose(rng, n, 0);
                        let (old, _) = live[index];
                        let i = block * (write_pct / 10) + pair;
                        let key = self.absent[i];
                        let value = V::from_position(n + i);
                        actions.push(Action::Remove(old));
                        expected.push(Expected::Value(oracle.remove(&old)));
                        actions.push(Action::Insert(key, value));
                        expected.push(Expected::Value(oracle.insert(key, value)));
                        live[index] = (key, value);
                    }
                }
                final_entries = Some(oracle.into_iter().collect());
            }
            _ => unreachable!(),
        }
        self.actions = actions;
        self.expected = expected;
        self.visited = visited;
        self.final_entries = final_entries;
    }

    pub(super) fn expected_final(&self) -> Option<&[(K, V)]> {
        if self.builds() {
            Some(&self.initial)
        } else {
            self.final_entries.as_deref()
        }
    }

    pub(super) fn builds(&self) -> bool {
        self.single_inserts()
            || matches!(
                self.operation,
                Operation::MapBuild {} | Operation::MapBuildGet { .. }
            )
    }
    pub(super) fn single_inserts(&self) -> bool {
        matches!(
            self.operation,
            Operation::MapBuildInsertRandom {} | Operation::MapBuildInsertAscending {}
        )
    }
    pub(super) fn result_count(&self) -> usize {
        self.expected.len()
    }
    pub(super) fn visit_count(&self) -> usize {
        if matches!(self.operation, Operation::MapIterate {}) {
            self.initial.len()
        } else {
            self.visited.len()
        }
    }
    pub(super) fn check_outputs(
        &self,
        results: &[Expected<K, V>],
        visited: &[(K, V)],
    ) -> Result<(), String> {
        let expected_visits = if matches!(self.operation, Operation::MapIterate {}) {
            &self.initial
        } else {
            &self.visited
        };
        crate::comparison::check::collection_sequence("returned results", &self.expected, results)?;
        crate::comparison::check::collection_sequence("ordered visits", expected_visits, visited)
    }
    pub(super) fn churn(&self) -> Vec<(K, K, V)> {
        let len = self.initial.len();
        let pairs = self.operation.churn_pairs(len);
        if pairs == 0 {
            return Vec::new();
        }
        let mut rng = SplitMix64(29);
        let mut live: Vec<_> = self.initial.iter().map(|&(key, _)| key).collect();
        (len..len + pairs)
            .map(|i| {
                let slot = rng.next_u64() as usize % len;
                let key = keys::<K>(&self.distribution, i);
                let old = std::mem::replace(&mut live[slot], key);
                (old, key, V::from_position(i))
            })
            .collect()
    }

    pub(super) fn check_retained<C: Collection<K, V>>(
        &self,
        collection: &C,
        churn: &[(K, K, V)],
        results: &[(Option<V>, Option<V>)],
    ) -> Result<(), String> {
        if churn.is_empty() {
            return crate::comparison::check::collection_contents(&self.initial, |visit| {
                collection.visit(None, visit)
            });
        }
        let mut oracle: BTreeMap<_, _> = self.initial.iter().copied().collect();
        for (&(old, key, value), &(removed, inserted)) in churn.iter().zip(results) {
            if (oracle.remove(&old), oracle.insert(key, value)) != (removed, inserted) {
                return Err("churn returned incorrect values".into());
            }
        }
        let expected: Vec<_> = oracle.into_iter().collect();
        crate::comparison::check::collection_contents(&expected, |visit| {
            collection.visit(None, visit)
        })
    }
}
