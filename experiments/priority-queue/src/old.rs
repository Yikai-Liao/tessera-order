// SPDX-License-Identifier: BSD-2-Clause
// Prior screen algorithms, retained without algorithm changes for paired comparison.
// Preserve the previous screen operation exactly, including its collection idiom.
#![allow(clippy::extend_with_drain)]
use super::Queue;
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BinaryHeap},
};
// Active runs are sorted descending so pop removes their smallest value.
#[derive(Default)]
pub struct Runs {
    buffer: BinaryHeap<Reverse<u64>>,
    runs: Vec<Vec<u64>>,
}
impl Queue for Runs {
    fn push(&mut self, k: u64) {
        self.buffer.push(Reverse(k));
        if self.buffer.len() == 512 {
            let heap = std::mem::take(&mut self.buffer);
            let mut v: Vec<u64> = heap.into_vec().into_iter().map(|Reverse(x)| x).collect();
            tessera_sort::stable_sort(&mut v);
            v.reverse();
            self.runs.push(v);
        }
    }
    fn pop(&mut self) -> Option<u64> {
        let mut best = self.buffer.peek().map(|x| x.0);
        let mut idx = None;
        for (i, r) in self.runs.iter().enumerate() {
            let x = *r.last().unwrap();
            if best.is_none_or(|b| x < b) {
                best = Some(x);
                idx = Some(i)
            }
        }
        if let Some(i) = idx {
            let x = self.runs[i].pop();
            if self.runs[i].is_empty() {
                self.runs.swap_remove(i);
            }
            x
        } else {
            self.buffer.pop().map(|x| x.0)
        }
    }
}
// A prefix leaf owns fixed-capacity blocks until it is prepared for output.
// The sorted output buffer is invalidated by any insertion into this leaf.
const BLOCK: usize = 128;
const LEAF: usize = 512;
#[derive(Default)]
struct Leaf {
    bits: u8,
    blocks: Vec<Vec<u64>>,
    sorted: Option<Vec<u64>>,
    len: usize,
}
impl Leaf {
    fn put(&mut self, k: u64, pool: &mut Vec<Vec<u64>>) {
        if let Some(v) = self.sorted.take() {
            for x in v {
                self.put_block(x, pool)
            }
        }
        self.put_block(k, pool);
        self.len += 1
    }
    fn put_block(&mut self, k: u64, pool: &mut Vec<Vec<u64>>) {
        if self.blocks.last().is_none_or(|b| b.len() == BLOCK) {
            self.blocks
                .push(pool.pop().unwrap_or_else(|| Vec::with_capacity(BLOCK)))
        }
        self.blocks.last_mut().unwrap().push(k)
    }
    fn take_all(&mut self, pool: &mut Vec<Vec<u64>>) -> Vec<u64> {
        let mut v = Vec::with_capacity(self.len);
        if let Some(s) = self.sorted.take() {
            v.extend(s)
        }
        for mut b in self.blocks.drain(..) {
            v.extend(b.drain(..));
            if pool.len() < 32 {
                pool.push(b)
            }
        }
        self.len = 0;
        v
    }
    fn pop(&mut self, pool: &mut Vec<Vec<u64>>) -> Option<u64> {
        if self.len == 0 {
            return None;
        }
        if self.sorted.is_none() {
            let mut v = self.take_all(pool);
            tessera_sort::stable_sort(&mut v);
            v.reverse();
            self.len = v.len();
            self.sorted = Some(v)
        }
        self.len -= 1;
        let value = self.sorted.as_mut().unwrap().pop();
        if self.len == 0 {
            self.sorted = None;
        }
        value
    }
}
pub struct Radix {
    leaves: BTreeMap<u64, Leaf>,
    pool: Vec<Vec<u64>>,
}
impl Default for Radix {
    fn default() -> Self {
        let mut leaves = BTreeMap::new();
        leaves.insert(0, Leaf::default());
        Self {
            leaves,
            pool: Vec::new(),
        }
    }
}
impl Queue for Radix {
    fn push(&mut self, k: u64) {
        self.leaves
            .range_mut(..=k)
            .next_back()
            .unwrap()
            .1
            .put(k, &mut self.pool)
    }
    fn pop(&mut self) -> Option<u64> {
        loop {
            let (&start, leaf) = self.leaves.iter().find(|(_, l)| l.len > 0)?;
            if leaf.len <= LEAF || leaf.bits == 64 {
                return self.leaves.get_mut(&start).unwrap().pop(&mut self.pool);
            }
            let leaf = self.leaves.get_mut(&start).unwrap();
            let bits = leaf.bits + 1;
            let pivot = start | (1u64 << (64 - bits));
            let items = leaf.take_all(&mut self.pool);
            leaf.bits = bits;
            let mut high = Leaf {
                bits,
                ..Leaf::default()
            };
            for k in items {
                if k < pivot {
                    leaf.put(k, &mut self.pool)
                } else {
                    high.put(k, &mut self.pool)
                }
            }
            self.leaves.insert(pivot, high);
        }
    }
}
