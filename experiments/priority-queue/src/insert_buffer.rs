// SPDX-License-Identifier: BSD-2-Clause
use std::{cmp::Reverse, collections::BinaryHeap};

/// Append without maintaining order until a pop actually needs this buffer.
/// Heapify once at that boundary, then maintain the heap until the next flush
/// or complete drain. A pop never manufactures a tiny persistent sorted run.
pub enum InsertBuffer {
    Unordered {
        values: Vec<Reverse<u64>>,
        min: Option<u64>,
    },
    Heap(BinaryHeap<Reverse<u64>>),
}
impl Default for InsertBuffer {
    fn default() -> Self {
        Self::Unordered {
            values: Vec::new(),
            min: None,
        }
    }
}
impl InsertBuffer {
    pub fn len(&self) -> usize {
        match self {
            Self::Unordered { values, .. } => values.len(),
            Self::Heap(h) => h.len(),
        }
    }
    pub fn peek(&self) -> Option<u64> {
        match self {
            Self::Unordered { min, .. } => *min,
            Self::Heap(h) => h.peek().map(|r| r.0),
        }
    }
    pub fn push(&mut self, key: u64) {
        match self {
            Self::Unordered { values, min } => {
                values.push(Reverse(key));
                *min = Some(min.map_or(key, |m| m.min(key)));
            }
            Self::Heap(h) => h.push(Reverse(key)),
        }
    }
    pub fn pop(&mut self) -> Option<u64> {
        if let Self::Unordered { values, min } = self {
            if values.len() <= 1 {
                *min = None;
                return values.pop().map(|r| r.0);
            }
            *self = Self::Heap(BinaryHeap::from(std::mem::take(values)));
        }
        let Self::Heap(h) = self else { unreachable!() };
        let key = h.pop().map(|r| r.0);
        if h.is_empty() {
            let values = std::mem::take(h).into_vec();
            *self = Self::Unordered { values, min: None };
        }
        key
    }
    pub fn into_values(self) -> Vec<u64> {
        let v = match self {
            Self::Unordered { values, .. } => values,
            Self::Heap(h) => h.into_vec(),
        };
        v.into_iter().map(|r| r.0).collect()
    }
}
