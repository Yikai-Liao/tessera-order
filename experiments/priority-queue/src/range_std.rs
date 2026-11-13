// SPDX-License-Identifier: BSD-2-Clause
use super::Queue;
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BinaryHeap},
};

const BLOCK: usize = 128;
const POOL_BLOCKS: usize = 32;

#[derive(Default)]
struct Bag {
    blocks: Vec<Vec<u64>>,
    len: usize,
    min: u64,
    max: u64,
}
impl Bag {
    fn push(&mut self, k: u64, pool: &mut Vec<Vec<u64>>) {
        if self.len == 0 {
            self.min = k;
            self.max = k;
        } else {
            self.min = self.min.min(k);
            self.max = self.max.max(k);
        }
        if self.blocks.last().is_none_or(|b| b.len() == BLOCK) {
            self.blocks
                .push(pool.pop().unwrap_or_else(|| Vec::with_capacity(BLOCK)));
        }
        self.blocks.last_mut().unwrap().push(k);
        self.len += 1;
    }
    fn recycle(mut block: Vec<u64>, pool: &mut Vec<Vec<u64>>) {
        block.clear();
        if pool.len() < POOL_BLOCKS {
            pool.push(block);
        }
    }
    fn into_values(self, pool: &mut Vec<Vec<u64>>) -> Vec<u64> {
        let mut values = Vec::with_capacity(self.len);
        for mut block in self.blocks {
            values.append(&mut block);
            Self::recycle(block, pool);
        }
        values
    }
    // Called only for a certified equal-key front. Other bags have no deletions,
    // so their exact extrema remain valid through their entire storage lifetime.
    fn pop_equal(&mut self, pool: &mut Vec<Vec<u64>>) -> Option<u64> {
        let b = self.blocks.last_mut()?;
        let key = b.pop();
        self.len -= 1;
        if b.is_empty() {
            Self::recycle(self.blocks.pop().unwrap(), pool);
        }
        key
    }
}
#[derive(Default)]
enum Front {
    #[default]
    Empty,
    Sorted(Vec<u64>),
    Equal(Bag),
}
impl Front {
    fn peek(&self) -> Option<u64> {
        match self {
            Self::Empty => None,
            Self::Sorted(v) => v.last().copied(),
            Self::Equal(b) => Some(b.min),
        }
    }
    fn pop(&mut self, pool: &mut Vec<Vec<u64>>) -> Option<u64> {
        let value = match self {
            Self::Empty => None,
            Self::Sorted(v) => v.pop(),
            Self::Equal(b) => b.pop_equal(pool),
        };
        let empty = match self {
            Self::Empty => true,
            Self::Sorted(v) => v.is_empty(),
            Self::Equal(b) => b.len == 0,
        };
        if empty {
            *self = Self::Empty;
        }
        value
    }
    fn into_bag(self, pool: &mut Vec<Vec<u64>>) -> Bag {
        match self {
            Self::Empty => Bag::default(),
            Self::Equal(b) => b,
            Self::Sorted(v) => {
                let mut b = Bag::default();
                for k in v {
                    b.push(k, pool)
                }
                b
            }
        }
    }
}

/// Exact scalar min queue with a sorted output tile and delayed high-key bags.
/// The directory owns only nonempty ranges, ordered by their inclusive bounds.
/// Removed bounds need not survive: future keys route to the next bound or a
/// new terminal bag. Each refinement therefore uses current, exact extrema,
/// never a historical prefix mask that future insertions could invalidate.
/// Equal-key payload stability and unwind recovery are not established here.
#[derive(Default)]
pub struct RangeCore<const TILE: usize> {
    back: BTreeMap<u64, Bag>,
    front: Front,
    cutoff: Option<u64>,
    pending: BinaryHeap<Reverse<u64>>,
}
impl<const TILE: usize> RangeCore<TILE> {
    fn append_back(&mut self, k: u64, pool: &mut Vec<Vec<u64>>) {
        if let Some((_, bag)) = self.back.range_mut(k..).next() {
            bag.push(k, pool);
        } else {
            self.back.entry(u64::MAX).or_default().push(k, pool);
        }
    }
    fn prepare(&mut self, mut bag: Bag, pool: &mut Vec<Vec<u64>>) {
        debug_assert!(self.front.peek().is_none() && self.pending.is_empty());
        loop {
            if bag.min == bag.max {
                self.cutoff = Some(bag.max);
                self.front = Front::Equal(bag);
                return;
            }
            if bag.len <= TILE {
                self.cutoff = Some(bag.max);
                let mut values = bag.into_values(pool);
                values.sort_unstable();
                values.reverse();
                self.front = Front::Sorted(values);
                return;
            }
            // Target half a tile per child. Wide variation is consumed in one pass;
            // equal bags above are completed without inspecting all remaining bits.
            let split = super::split::choose(bag.len, bag.min, bag.max, TILE, |index| {
                bag.blocks[index / BLOCK][index % BLOCK]
            });
            let mut parts: Vec<Bag> = (0..split.count()).map(|_| Bag::default()).collect();
            for mut block in bag.blocks {
                for key in block.drain(..) {
                    parts[split.bucket(key)].push(key, pool);
                }
                // Once a source block has been read, its allocation can serve a child.
                Bag::recycle(block, pool);
            }
            let mut nonempty = parts.into_iter().filter(|b| b.len > 0);
            bag = nonempty.next().unwrap();
            for child in nonempty {
                assert!(self.back.insert(child.max, child).is_none());
            }
        }
    }
    fn absorb_pending(&mut self, pool: &mut Vec<Vec<u64>>) {
        let mut bag = std::mem::take(&mut self.front).into_bag(pool);
        for Reverse(key) in std::mem::take(&mut self.pending) {
            bag.push(key, pool);
        }
        self.prepare(bag, pool);
    }
}
impl<const TILE: usize> RangeCore<TILE> {
    pub fn push(&mut self, k: u64, pool: &mut Vec<Vec<u64>>) {
        if self.cutoff.is_some_and(|bound| k <= bound) {
            self.pending.push(Reverse(k));
            if self.pending.len() == TILE {
                self.absorb_pending(pool);
            }
        } else {
            self.append_back(k, pool);
        }
    }
    pub fn pop(&mut self, pool: &mut Vec<Vec<u64>>) -> Option<u64> {
        let pending = self.pending.peek().map(|r| r.0);
        let front = self.front.peek();
        let value = if pending.is_some_and(|p| front.is_none_or(|f| p <= f)) {
            self.pending.pop().map(|r| r.0)
        } else if front.is_some() {
            self.front.pop(pool)
        } else if let Some((_, bag)) = self.back.pop_first() {
            self.prepare(bag, pool);
            self.front.pop(pool)
        } else {
            None
        };
        if self.front.peek().is_none() && self.pending.is_empty() && self.back.is_empty() {
            self.cutoff = None;
        }
        value
    }
}

#[derive(Default)]
pub struct RangeQueue<const TILE: usize> {
    core: RangeCore<TILE>,
    pool: Vec<Vec<u64>>,
}
impl<const TILE: usize> Queue for RangeQueue<TILE> {
    fn push(&mut self, key: u64) {
        self.core.push(key, &mut self.pool);
    }
    fn pop(&mut self) -> Option<u64> {
        self.core.pop(&mut self.pool)
    }
}
