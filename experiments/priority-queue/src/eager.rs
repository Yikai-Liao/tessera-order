// SPDX-License-Identifier: BSD-2-Clause
use super::{Queue, range::RangeCore};

/// Eagerly route a fixed top digit, then lazily prepare only its minimum live
/// child. The outer queue owns one shared bounded block pool; empty children
/// disappear instead of each retaining a private pool or empty directory.
/// This is an exact queue for arbitrary u64 inserts, not a monotone radix heap.
pub struct EagerQueue<const BITS: usize, const TILE: usize> {
    roots: Vec<Option<Box<RangeCore<TILE>>>>,
    counts: Vec<usize>,
    nonempty: Vec<u64>,
    pool: Vec<Vec<u64>>,
}
impl<const BITS: usize, const TILE: usize> Default for EagerQueue<BITS, TILE> {
    fn default() -> Self {
        let n = 1usize << BITS;
        Self {
            roots: (0..n).map(|_| None).collect(),
            counts: vec![0; n],
            nonempty: vec![0; n.div_ceil(64)],
            pool: Vec::new(),
        }
    }
}
impl<const BITS: usize, const TILE: usize> Queue for EagerQueue<BITS, TILE> {
    fn push(&mut self, key: u64) {
        let i = (key >> (64 - BITS)) as usize;
        self.roots[i]
            .get_or_insert_with(Default::default)
            .push(key, &mut self.pool);
        self.counts[i] += 1;
        self.nonempty[i / 64] |= 1u64 << (i % 64);
    }
    fn pop(&mut self) -> Option<u64> {
        let (word, &bits) = self.nonempty.iter().enumerate().find(|(_, x)| **x != 0)?;
        let i = word * 64 + bits.trailing_zeros() as usize;
        let value = self.roots[i].as_mut().unwrap().pop(&mut self.pool);
        assert!(value.is_some());
        self.counts[i] -= 1;
        if self.counts[i] == 0 {
            self.roots[i] = None;
            self.nonempty[word] &= !(1u64 << (i % 64));
        }
        value
    }
}
