// SPDX-License-Identifier: BSD-2-Clause
use super::Queue;
use super::insert_buffer::InsertBuffer;
const NONE: usize = usize::MAX;

/// Delayed sorted runs with a tournament of run heads. Compaction optionally
/// keeps one run at each binary carry level; the ordinary variant keeps every
/// nonempty run. Both compare their winner with a bounded insert-buffer heap.
#[derive(Default)]
pub struct BufferedRunQueue<const TILE: usize, const COMPACT: bool> {
    buffer: InsertBuffer,
    runs: Vec<Vec<u64>>,
    tree: Vec<usize>,
    leaves: usize,
    free: Vec<usize>,
}
impl<const TILE: usize, const COMPACT: bool> BufferedRunQueue<TILE, COMPACT> {
    fn winner(&self, a: usize, b: usize) -> usize {
        if a == NONE {
            return b;
        }
        if b == NONE {
            return a;
        }
        if self.runs[a].last().unwrap() <= self.runs[b].last().unwrap() {
            a
        } else {
            b
        }
    }
    fn update(&mut self, id: usize) {
        if self.runs.len() > self.leaves {
            self.leaves = self.runs.len().next_power_of_two();
            self.tree = vec![NONE; self.leaves * 2];
            for (i, run) in self.runs.iter().enumerate() {
                if !run.is_empty() {
                    self.tree[self.leaves + i] = i;
                }
            }
            for i in (1..self.leaves).rev() {
                self.tree[i] = self.winner(self.tree[2 * i], self.tree[2 * i + 1]);
            }
        } else {
            let mut i = self.leaves + id;
            self.tree[i] = if self.runs[id].is_empty() { NONE } else { id };
            while i > 1 {
                i /= 2;
                self.tree[i] = self.winner(self.tree[2 * i], self.tree[2 * i + 1]);
            }
        }
    }
    fn merge(a: Vec<u64>, b: Vec<u64>) -> Vec<u64> {
        let mut merged = Vec::with_capacity(a.len() + b.len());
        let (mut i, mut j) = (0, 0);
        while i < a.len() && j < b.len() {
            if a[i] >= b[j] {
                merged.push(a[i]);
                i += 1;
            } else {
                merged.push(b[j]);
                j += 1;
            }
        }
        merged.extend_from_slice(&a[i..]);
        merged.extend_from_slice(&b[j..]);
        merged
    }
    fn flush(&mut self) {
        let mut values = std::mem::take(&mut self.buffer).into_values();
        tessera_sort::stable_sort(&mut values);
        values.reverse();
        if COMPACT {
            let mut level = 0;
            loop {
                if level == self.runs.len() {
                    self.runs.push(Vec::new());
                }
                if self.runs[level].is_empty() {
                    self.runs[level] = values;
                    self.update(level);
                    break;
                }
                let old = std::mem::take(&mut self.runs[level]);
                self.update(level);
                values = Self::merge(old, values);
                level += 1;
            }
        } else {
            let id = self.free.pop().unwrap_or_else(|| {
                self.runs.push(Vec::new());
                self.runs.len() - 1
            });
            self.runs[id] = values;
            self.update(id);
        }
    }
}
impl<const TILE: usize, const COMPACT: bool> Queue for BufferedRunQueue<TILE, COMPACT> {
    fn push(&mut self, k: u64) {
        self.buffer.push(k);
        if self.buffer.len() == TILE {
            self.flush();
        }
    }
    fn pop(&mut self) -> Option<u64> {
        let id = self.tree.get(1).copied().unwrap_or(NONE);
        if self
            .buffer
            .peek()
            .is_some_and(|key| id == NONE || key <= *self.runs[id].last().unwrap())
        {
            return self.buffer.pop();
        }
        if id == NONE {
            return None;
        }
        let key = self.runs[id].pop();
        if self.runs[id].is_empty() {
            self.runs[id] = Vec::new();
            if !COMPACT {
                self.free.push(id);
            }
        }
        if self.runs[id].capacity() > 128 && self.runs[id].len() * 4 < self.runs[id].capacity() {
            self.runs[id].shrink_to_fit();
        }
        self.update(id);
        key
    }
}
