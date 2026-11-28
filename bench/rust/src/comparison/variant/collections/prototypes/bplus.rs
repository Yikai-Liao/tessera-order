// SPDX-License-Identifier: BSD-2-Clause

use super::super::{Collection, IntKey, Value};
use fearless_simd::{Level, Simd, dispatch, prelude::*};
use std::mem::{replace, size_of};

const LEAF_PAYLOAD_BYTES: usize = 960;
const BRANCH_PAYLOAD_BYTES: usize = 1024;

pub(in super::super) const fn leaf_capacity<K, V>() -> usize {
    LEAF_PAYLOAD_BYTES / (size_of::<K>() + size_of::<V>())
}

// Non-root nodes are at least half full, except the rightmost leaf after packing or appending.
const fn min_fill(capacity: usize) -> usize {
    capacity / 2
}

#[inline(always)]
fn count_lt<S: Simd, K: IntKey>(simd: S, keys: &[K], key: K) -> usize {
    let lanes = K::lanes::<S>();
    let mut rest = keys;
    let mut count = 0;
    if lanes > 1 {
        let target = K::splat_encoded(simd, key);
        while rest.len() >= lanes {
            let (block, tail) = rest.split_at(lanes);
            count += K::load_encoded(simd, block)
                .simd_lt(target)
                .to_bitmask()
                .count_ones() as usize;
            rest = tail;
        }
    }
    count + rest.iter().filter(|&&k| k < key).count()
}

#[inline(always)]
fn lower_bound<S: Simd, K: IntKey>(simd: S, keys: &[K], key: K) -> usize {
    const BLOCK: usize = 16;
    if keys.len() < BLOCK {
        return count_lt(simd, keys, key);
    }
    let last = keys.len() - BLOCK;
    let (blocks, _) = keys[..keys.len() - 1].as_chunks::<BLOCK>();
    let block = blocks.partition_point(|keys| keys[BLOCK - 1] < key);
    // The last window may overlap its predecessor. All keys before its start are smaller.
    let start = (block * BLOCK).min(last);
    start + count_lt(simd, &keys[start..start + BLOCK], key)
}

pub(in super::super) struct BPlus<K, V, const CAP: usize> {
    level: Level,
    root: Option<Node<K, V, CAP>>,
}

enum Node<K, V, const CAP: usize> {
    Leaf(Box<Leaf<K, V, CAP>>),
    Branch(Box<Branch<K, V, CAP>>),
}

struct Leaf<K, V, const CAP: usize> {
    keys: [K; CAP],
    values: [V; CAP],
    len: usize,
}

struct Branch<K, V, const CAP: usize> {
    // Each separator is the inclusive maximum of its child.
    keys: Vec<K>,
    children: Vec<Node<K, V, CAP>>,
    counts: Vec<usize>,
}

enum Insert<V, N> {
    Done(Option<V>),
    FullLeaf(usize),
    Split(N),
}

impl<K: IntKey, V: Copy, const CAP: usize> Leaf<K, V, CAP> {
    fn single(key: K, value: V) -> Self {
        Self {
            keys: [key; CAP],
            values: [value; CAP],
            len: 1,
        }
    }

    fn lower_bound(&self, simd: impl Simd, key: K) -> usize {
        lower_bound(simd, &self.keys[..self.len], key)
    }

    fn entry(&self, index: usize) -> Option<(K, V)> {
        (index < self.len).then(|| (self.keys[index], self.values[index]))
    }

    fn insert(&mut self, index: usize, key: K, value: V) {
        self.keys.copy_within(index..self.len, index + 1);
        self.values.copy_within(index..self.len, index + 1);
        self.keys[index] = key;
        self.values[index] = value;
        self.len += 1;
    }

    fn remove(&mut self, index: usize) -> (K, V) {
        let entry = (self.keys[index], self.values[index]);
        self.keys.copy_within(index + 1..self.len, index);
        self.values.copy_within(index + 1..self.len, index);
        self.len -= 1;
        entry
    }

    fn balance(left: &mut Self, right: &mut Self, left_len: usize) {
        if left_len < left.len {
            let moved = left.len - left_len;
            right.keys.copy_within(..right.len, moved);
            right.values.copy_within(..right.len, moved);
            right.keys[..moved].copy_from_slice(&left.keys[left_len..left.len]);
            right.values[..moved].copy_from_slice(&left.values[left_len..left.len]);
            right.len += moved;
        } else {
            let moved = left_len - left.len;
            left.keys[left.len..left_len].copy_from_slice(&right.keys[..moved]);
            left.values[left.len..left_len].copy_from_slice(&right.values[..moved]);
            right.keys.copy_within(moved..right.len, 0);
            right.values.copy_within(moved..right.len, 0);
            right.len -= moved;
        }
        left.len = left_len;
    }

    fn insert_into_pair(simd: impl Simd, left: &mut Self, right: &mut Self, key: K, value: V) {
        let target = if key <= left.keys[left.len - 1] {
            left
        } else {
            right
        };
        target.insert(target.lower_bound(simd, key), key, value);
    }

    fn split_insert(&mut self, simd: impl Simd, key: K, value: V) -> Node<K, V, CAP> {
        let mut right = Box::new(Self::single(key, value));
        if key < self.keys[CAP - 1] {
            right.len = 0;
            Self::balance(self, &mut right, min_fill(CAP));
            Self::insert_into_pair(simd, self, &mut right, key, value);
        }
        Node::Leaf(right)
    }
}

impl<K: IntKey, V: Copy, const CAP: usize> Branch<K, V, CAP> {
    // One temporary slot stages a child before a full-branch split.
    const CAPACITY: usize = BRANCH_PAYLOAD_BYTES
        / (size_of::<K>() + size_of::<Node<K, V, CAP>>() + size_of::<usize>())
        - 1;

    fn with_capacity(slots: usize) -> Self {
        Self {
            keys: Vec::with_capacity(slots),
            counts: Vec::with_capacity(slots),
            children: Vec::with_capacity(slots),
        }
    }

    fn push(&mut self, node: Node<K, V, CAP>) {
        self.keys.push(node.max_key());
        let count = node.count();
        self.counts.push(count);
        self.children.push(node);
    }

    fn child_for(&self, simd: impl Simd, key: K) -> usize {
        lower_bound(simd, &self.keys[..self.keys.len() - 1], key)
    }

    fn child_at(&self, mut index: usize) -> Option<(usize, usize)> {
        for (slot, &count) in self.counts.iter().enumerate() {
            if index < count {
                return Some((slot, index));
            }
            index -= count;
        }
        None
    }

    fn refresh(&mut self, slot: usize) {
        self.keys[slot] = self.children[slot].max_key();
        self.counts[slot] = self.children[slot].count();
    }

    fn insert_child(&mut self, slot: usize, node: Node<K, V, CAP>) -> Option<Node<K, V, CAP>> {
        self.keys.insert(slot, node.max_key());
        self.counts.insert(slot, node.count());
        self.children.insert(slot, node);
        if self.children.len() <= Self::CAPACITY {
            return None;
        }
        let mut right = Box::new(Self::with_capacity(Self::CAPACITY + 1));
        let midpoint = min_fill(Self::CAPACITY);
        right.keys.extend(self.keys.drain(midpoint..));
        right.counts.extend(self.counts.drain(midpoint..));
        right.children.extend(self.children.drain(midpoint..));
        Some(Node::Branch(right))
    }

    fn remove_child(&mut self, slot: usize) {
        self.keys.remove(slot);
        self.counts.remove(slot);
        self.children.remove(slot);
    }

    fn repair(&mut self, slot: usize) {
        if self.children.len() < 2 {
            return;
        }
        let left_slot = slot.saturating_sub(1);
        let (left, right) = self.children.split_at_mut(left_slot + 1);
        let left = &mut left[left_slot];
        let right = &mut right[0];
        let total = left.slots() + right.slots();
        let merged = total <= left.capacity();
        let left_len = if merged { total } else { total / 2 };
        match (left, right) {
            (Node::Leaf(left), Node::Leaf(right)) => Leaf::balance(left, right, left_len),
            (Node::Branch(left), Node::Branch(right)) => {
                if left_len < left.children.len() {
                    right.keys.splice(..0, left.keys.drain(left_len..));
                    right.counts.splice(..0, left.counts.drain(left_len..));
                    right.children.splice(..0, left.children.drain(left_len..));
                } else {
                    let moved = left_len - left.children.len();
                    left.keys.extend(right.keys.drain(..moved));
                    left.counts.extend(right.counts.drain(..moved));
                    left.children.extend(right.children.drain(..moved));
                }
            }
            _ => unreachable!(),
        }
        self.refresh(left_slot);
        if merged {
            self.remove_child(left_slot + 1);
        } else {
            self.refresh(left_slot + 1);
        }
    }
}

impl<K: IntKey, V: Copy, const CAP: usize> Node<K, V, CAP> {
    fn max_key(&self) -> K {
        match self {
            Self::Leaf(leaf) => leaf.keys[leaf.len - 1],
            Self::Branch(branch) => *branch.keys.last().unwrap(),
        }
    }

    fn count(&self) -> usize {
        match self {
            Self::Leaf(leaf) => leaf.len,
            Self::Branch(branch) => branch.counts.iter().sum(),
        }
    }

    fn slots(&self) -> usize {
        match self {
            Self::Leaf(leaf) => leaf.len,
            Self::Branch(branch) => branch.children.len(),
        }
    }

    fn capacity(&self) -> usize {
        match self {
            Self::Leaf(_) => CAP,
            Self::Branch(_) => Branch::<K, V, CAP>::CAPACITY,
        }
    }

    fn leaf_for(&self, simd: impl Simd, key: K) -> &Leaf<K, V, CAP> {
        match self {
            Self::Leaf(leaf) => leaf,
            Self::Branch(branch) => {
                branch.children[branch.child_for(simd, key)].leaf_for(simd, key)
            }
        }
    }

    fn leaf_at(&self, index: usize) -> Option<(&Leaf<K, V, CAP>, usize)> {
        match self {
            Self::Leaf(leaf) => (index < leaf.len).then_some((leaf, index)),
            Self::Branch(branch) => {
                let (slot, offset) = branch.child_at(index)?;
                branch.children[slot].leaf_at(offset)
            }
        }
    }

    fn rank(&self, simd: impl Simd, key: K) -> usize {
        match self {
            Self::Leaf(leaf) => leaf.lower_bound(simd, key),
            Self::Branch(branch) => {
                let slot = branch.child_for(simd, key);
                let prefix: usize = branch.counts[..slot].iter().sum();
                prefix + branch.children[slot].rank(simd, key)
            }
        }
    }

    fn endpoint(&self, last: bool) -> Option<(K, V)> {
        match self {
            Self::Leaf(leaf) => leaf.entry(if last { leaf.len - 1 } else { 0 }),
            Self::Branch(branch) => {
                let slot = if last { branch.children.len() - 1 } else { 0 };
                branch.children[slot].endpoint(last)
            }
        }
    }

    fn insert(&mut self, simd: impl Simd, key: K, value: V) -> Insert<V, Self> {
        match self {
            Self::Leaf(leaf) => {
                let index = leaf.lower_bound(simd, key);
                if index < leaf.len && leaf.keys[index] == key {
                    return Insert::Done(Some(replace(&mut leaf.values[index], value)));
                }
                if leaf.len == CAP {
                    return Insert::FullLeaf(index);
                }
                leaf.insert(index, key, value);
                Insert::Done(None)
            }
            Self::Branch(branch) => {
                let slot = branch.child_for(simd, key);
                let split = match branch.children[slot].insert(simd, key, value) {
                    Insert::Done(old) => {
                        if old.is_none() {
                            branch.counts[slot] += 1;
                            if key > branch.keys[slot] {
                                branch.keys[slot] = key;
                            }
                        }
                        return Insert::Done(old);
                    }
                    Insert::Split(right) => {
                        branch.refresh(slot);
                        branch.insert_child(slot + 1, right)
                    }
                    Insert::FullLeaf(index) => branch.insert_full(slot, index, key, value),
                };
                split.map_or(Insert::Done(None), Insert::Split)
            }
        }
    }

    fn remove(&mut self, simd: impl Simd, key: K) -> Option<V> {
        match self {
            Self::Leaf(leaf) => {
                let index = leaf.lower_bound(simd, key);
                (index < leaf.len && leaf.keys[index] == key).then(|| leaf.remove(index).1)
            }
            Self::Branch(branch) => {
                let slot = branch.child_for(simd, key);
                let child = &mut branch.children[slot];
                let value = child.remove(simd, key)?;
                let slots = child.slots();
                if slots == 0 {
                    branch.remove_child(slot);
                } else {
                    if key == branch.keys[slot] {
                        branch.keys[slot] = child.max_key();
                    }
                    branch.counts[slot] -= 1;
                    if slots < min_fill(child.capacity()) {
                        branch.repair(slot);
                    }
                }
                Some(value)
            }
        }
    }

    fn visit(&self, simd: impl Simd, lo: Option<K>, hi: Option<K>, visit: &mut impl FnMut((K, V))) {
        match self {
            Self::Leaf(leaf) => {
                let start = lo.map_or(0, |key| leaf.lower_bound(simd, key));
                let end = hi.map_or(leaf.len, |key| {
                    leaf.keys[..leaf.len].partition_point(|&k| k <= key)
                });
                leaf.keys[start..end]
                    .iter()
                    .copied()
                    .zip(leaf.values[start..end].iter().copied())
                    .for_each(visit);
            }
            Self::Branch(branch) => {
                let first = lo.map_or(0, |key| branch.child_for(simd, key));
                let last = hi.map_or(branch.children.len() - 1, |key| branch.child_for(simd, key));
                for slot in first..=last {
                    let lo = if slot == first { lo } else { None };
                    let hi = if slot == last { hi } else { None };
                    branch.children[slot].visit(simd, lo, hi, visit);
                }
            }
        }
    }
}

impl<K: IntKey, V: Copy, const CAP: usize> Branch<K, V, CAP> {
    fn insert_full(
        &mut self,
        slot: usize,
        index: usize,
        key: K,
        value: V,
    ) -> Option<Node<K, V, CAP>> {
        if index == CAP {
            return self.insert_child(slot + 1, Node::Leaf(Box::new(Leaf::single(key, value))));
        }
        let left_slot =
            if slot > 0 && (self.counts[slot - 1] < CAP || slot + 1 == self.children.len()) {
                slot - 1
            } else {
                slot
            };
        let (before, after) = self.children.split_at_mut(left_slot + 1);
        let (Node::Leaf(left), Node::Leaf(right)) = (&mut before[left_slot], &mut after[0]) else {
            unreachable!()
        };
        let total = left.len + right.len;
        if total < 2 * CAP {
            if slot == left_slot {
                let (k, v) = left.remove(CAP - 1);
                right.insert(0, k, v);
                left.insert(index, key, value);
            } else if index == 0 {
                left.insert(left.len, key, value);
            } else {
                let (k, v) = right.remove(0);
                left.insert(left.len, k, v);
                right.insert(index - 1, key, value);
            }
            self.refresh(left_slot);
            self.refresh(left_slot + 1);
            return None;
        }
        let moved = CAP / 3;
        let mut middle = Box::new(Leaf::single(key, value));
        middle.len = 0;
        Leaf::balance(&mut middle, right, moved);
        Leaf::balance(left, &mut middle, CAP - moved);
        let left_index = if slot == left_slot {
            index
        } else {
            CAP + index
        };
        if left_index < left.len {
            left.insert(left_index, key, value);
        } else if left_index < left.len + middle.len {
            middle.insert(left_index - left.len, key, value);
        } else {
            right.insert(left_index - left.len - middle.len, key, value);
        }
        self.refresh(left_slot);
        self.refresh(left_slot + 1);
        self.insert_child(left_slot + 1, Node::Leaf(middle))
    }
}

impl<K: IntKey, V: Value, const CAP: usize> From<Vec<(K, V)>> for BPlus<K, V, CAP> {
    fn from(mut entries: Vec<(K, V)>) -> Self {
        tessera_order::sort_by_key(&mut entries, |entry| entry.0);
        let mut entries = entries.into_iter().peekable();
        let mut level = Vec::new();
        while let Some((key, value)) = entries.next() {
            let mut leaf = Box::new(Leaf::single(key, value));
            loop {
                let last = leaf.len - 1;
                if let Some((_, value)) = entries.next_if(|entry| entry.0 == leaf.keys[last]) {
                    leaf.values[last] = value;
                } else if leaf.len == CAP {
                    break;
                } else if let Some((key, value)) = entries.next() {
                    leaf.keys[leaf.len] = key;
                    leaf.values[leaf.len] = value;
                    leaf.len += 1;
                } else {
                    break;
                }
            }
            level.push(Node::Leaf(leaf));
        }
        while level.len() > 1 {
            let groups = level.len().div_ceil(Branch::<K, V, CAP>::CAPACITY);
            let (base, extra) = (level.len() / groups, level.len() % groups);
            let mut nodes = level.into_iter();
            level = (0..groups)
                .map(|group| {
                    let slots = base + usize::from(group < extra);
                    let mut branch = Box::new(Branch::with_capacity(slots + 1));
                    for node in nodes.by_ref().take(slots) {
                        branch.push(node);
                    }
                    Node::Branch(branch)
                })
                .collect();
        }
        Self {
            level: Level::new(),
            root: level.pop(),
        }
    }
}

impl<K: IntKey, V: Value, const CAP: usize> Collection<K, V> for BPlus<K, V, CAP> {
    const NAME: &'static str = "Counted B+ tree";
    const BUILD: &'static str = "Tessera sort_by_key + packed B+ tree";
    const ENDPOINTS: &'static str = "Counted B+ tree edge descent";
    const ORDER_STATISTICS: bool = true;

    fn empty() -> Self {
        Self {
            level: Level::new(),
            root: None,
        }
    }

    fn build(entries: Vec<(K, V)>) -> Self {
        entries.into()
    }

    fn get(&self, key: K) -> Option<V> {
        let root = self.root.as_ref()?;
        dispatch!(self.level, simd => {
            let leaf = root.leaf_for(simd, key);
            let index = leaf.lower_bound(simd, key);
            (index < leaf.len && leaf.keys[index] == key).then(|| leaf.values[index])
        })
    }

    fn insert(&mut self, key: K, value: V) -> Option<V> {
        let Some(root) = &mut self.root else {
            self.root = Some(Node::Leaf(Box::new(Leaf::single(key, value))));
            return None;
        };
        let right = match dispatch!(self.level, simd => root.insert(simd, key, value)) {
            Insert::Done(old) => return old,
            Insert::Split(right) => right,
            Insert::FullLeaf(_) => {
                let Node::Leaf(leaf) = root else {
                    unreachable!()
                };
                dispatch!(self.level, simd => leaf.split_insert(simd, key, value))
            }
        };
        let mut branch = Box::new(Branch::with_capacity(Branch::<K, V, CAP>::CAPACITY + 1));
        branch.push(self.root.take().unwrap());
        branch.push(right);
        self.root = Some(Node::Branch(branch));
        None
    }

    fn remove(&mut self, key: K) -> Option<V> {
        let root = self.root.as_mut()?;
        let value = dispatch!(self.level, simd => root.remove(simd, key))?;
        match root {
            Node::Branch(branch) if branch.children.len() == 1 => {
                *root = branch.children.pop().unwrap();
            }
            _ if root.slots() == 0 => self.root = None,
            _ => {}
        }
        Some(value)
    }

    fn visit(&self, bounds: Option<(K, K)>, mut visit: impl FnMut((K, V))) {
        let (lo, hi) = bounds.map_or((None, None), |(lo, hi)| (Some(lo), Some(hi)));
        if let Some(root) = &self.root {
            dispatch!(self.level, simd => root.visit(simd, lo, hi, &mut visit));
        }
    }

    fn endpoint(&self, last: bool) -> Option<(K, V)> {
        self.root.as_ref()?.endpoint(last)
    }

    fn rank(&self, key: K) -> usize {
        self.root.as_ref().map_or(
            0,
            |root| dispatch!(self.level, simd => root.rank(simd, key)),
        )
    }

    fn select(&self, index: usize) -> Option<(K, V)> {
        let (leaf, offset) = self.root.as_ref()?.leaf_at(index)?;
        leaf.entry(offset)
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::input::Fixture;
    use super::*;
    use crate::{
        case::SplitMix64,
        plan::{InputCase, Operation},
    };
    use std::collections::BTreeMap;

    const CAP: usize = leaf_capacity::<u64, u64>();
    type Tree = BPlus<u64, u64, CAP>;

    fn check_node<K: IntKey, V: Value, const C: usize>(
        node: &Node<K, V, C>,
        root: bool,
        rightmost: bool,
        leaves: &mut Vec<usize>,
    ) -> (usize, usize) {
        let slots = node.slots();
        assert!(slots > 0 && slots <= node.capacity());
        if !root && !(rightmost && matches!(node, Node::Leaf(_))) {
            assert!(slots >= min_fill(node.capacity()));
        }
        match node {
            Node::Leaf(leaf) => {
                assert!(
                    leaf.keys[..leaf.len]
                        .windows(2)
                        .all(|keys| keys[0] < keys[1])
                );
                leaves.push(slots);
                (slots, 1)
            }
            Node::Branch(branch) => {
                assert_eq!(branch.keys.len(), slots);
                assert_eq!(branch.counts.len(), slots);
                assert!(branch.keys.windows(2).all(|keys| keys[0] < keys[1]));
                assert!(!root || slots >= 2);
                let mut total = 0;
                let mut height = 0;
                for (slot, child) in branch.children.iter().enumerate() {
                    let (count, child_height) =
                        check_node(child, false, rightmost && slot + 1 == slots, leaves);
                    assert_eq!(
                        branch.counts[slot], count,
                        "child subtree count at slot {slot}"
                    );
                    assert_eq!(branch.keys[slot], child.max_key());
                    assert!(slot == 0 || height == child_height);
                    height = child_height;
                    total += count;
                }
                (total, height + 1)
            }
        }
    }

    fn contents(tree: &Tree, model: &BTreeMap<u64, u64>) -> usize {
        let mut actual = Vec::new();
        tree.visit(None, |entry| actual.push(entry));
        assert!(actual.into_iter().eq(model.iter().map(|(&k, &v)| (k, v))));
        tree.root.as_ref().map_or(0, |root| {
            let (count, height) = check_node(root, true, true, &mut Vec::new());
            assert_eq!(count, model.len());
            height
        })
    }

    fn check_lower_bounds<K: IntKey>(high_bit: u64) {
        let keys: Vec<_> = (0..128)
            .map(|i| K::from_u64(high_bit - 64 + (i / 2) * 2))
            .collect();
        dispatch!(Level::new(), simd => {
            for len in 0..=keys.len() {
                let keys = &keys[..len];
                for raw in (high_bit - 65..=high_bit + 65).chain([0, u64::MAX]) {
                    let key = K::from_u64(raw);
                    assert_eq!(lower_bound(simd, keys, key), keys.partition_point(|&k| k < key),
                        "key {key:?}, length {len}");
                }
            }
        });
    }

    #[test]
    fn lower_bounds_u32() {
        check_lower_bounds::<u32>(1 << 31);
    }

    #[test]
    fn lower_bounds_u64() {
        check_lower_bounds::<u64>(1 << 63);
    }

    #[test]
    fn randomized_differential() {
        let fanout = Branch::<u64, u64, CAP>::CAPACITY;
        for size in [0, CAP + 1, CAP * (fanout + 4)] {
            let mut rng = SplitMix64(41);
            let domain = (4 * size).max(4 * CAP) as u64;
            let input: Vec<_> = (0..size)
                .map(|i| (rng.next_u64() % domain, i as u64))
                .collect();
            let mut tree = Tree::build(input.clone());
            let mut model: BTreeMap<_, _> = input.into_iter().collect();
            let mut height = contents(&tree, &model);
            for step in 0..size.max(CAP) * 3 {
                let key = rng.next_u64() % domain;
                if rng.next_u64().is_multiple_of(2) {
                    let value = rng.next_u64();
                    assert_eq!(tree.insert(key, value), model.insert(key, value));
                } else {
                    assert_eq!(tree.remove(key), model.remove(&key));
                }
                let query = rng.next_u64() % (domain + 1);
                let hi = query + rng.next_u64() % (2 * CAP as u64);
                let index = (rng.next_u64() as usize) % (model.len() + 2);
                assert_eq!(tree.get(query), model.get(&query).copied());
                assert_eq!(
                    tree.rank(query),
                    model.range(..query).count(),
                    "rank at step {step}, size {size}"
                );
                assert_eq!(
                    tree.select(index),
                    model.iter().nth(index).map(|(&k, &v)| (k, v))
                );
                assert_eq!(
                    tree.endpoint(false),
                    model.first_key_value().map(|(&k, &v)| (k, v))
                );
                assert_eq!(
                    tree.endpoint(true),
                    model.last_key_value().map(|(&k, &v)| (k, v))
                );
                let mut range = Vec::new();
                tree.visit(Some((query, hi)), |entry| range.push(entry));
                assert!(
                    range
                        .into_iter()
                        .eq(model.range(query..=hi).map(|(&k, &v)| (k, v)))
                );
                if step.is_multiple_of(CAP) {
                    height = height.max(contents(&tree, &model));
                }
            }
            if size > CAP * fanout {
                assert!(height >= 3);
            }
            let mut keys: Vec<_> = model.keys().copied().collect();
            while !keys.is_empty() {
                let slot = rng.next_u64() as usize % keys.len();
                let key = keys.swap_remove(slot);
                assert_eq!(tree.remove(key), model.remove(&key));
                if keys.len().is_multiple_of(CAP) {
                    contents(&tree, &model);
                }
            }
            assert!(tree.root.is_none());
        }
    }

    #[test]
    fn ascending_and_bulk_pack_leaves() {
        let size = CAP * (Branch::<u64, u64, CAP>::CAPACITY + 2) + 3;
        let mut ascending = Tree::empty();
        for key in 0..size as u64 {
            assert_eq!(ascending.insert(key, key + 1), None);
        }
        let entries = (0..size as u64)
            .rev()
            .flat_map(|key| [(key, key), (key, key + 1)])
            .collect();
        let bulk = Tree::build(entries);
        for tree in [ascending, bulk] {
            let mut leaves = Vec::new();
            let (count, _) = check_node(tree.root.as_ref().unwrap(), true, true, &mut leaves);
            assert_eq!(count, size);
            assert_eq!(leaves.pop(), Some(size % CAP));
            assert!(leaves.iter().all(|&len| len == CAP));
            for key in 0..size as u64 {
                assert_eq!(tree.select(key as usize), Some((key, key + 1)));
            }
        }
    }

    fn check_churn<K: IntKey, V: Value, const C: usize>(shape: &str) {
        const ENTRIES: usize = 65_536;
        const MIN_AVERAGE_FILL: f64 = 0.75;
        let fixture = Fixture::<K, V>::new(&InputCase {
            ty: shape.into(),
            distribution: "random".into(),
            len: ENTRIES,
            operation: Operation::MapMixed {
                write_pct: 10,
                hot_pct: 0,
            },
        });
        let mut tree = BPlus::<_, _, C>::build(fixture.original.clone());
        let churn = fixture.churn();
        assert_eq!(churn.len(), ENTRIES);
        let mut results = vec![(None, None); churn.len()];
        tree.churn(&churn, &mut results);
        fixture.check_retained(&tree, &churn, &results).unwrap();
        let mut leaves = Vec::new();
        let (entries, _) = check_node(tree.root.as_ref().unwrap(), true, true, &mut leaves);
        let average = entries as f64 / (leaves.len() * C) as f64;
        println!(
            "{shape}: {:.2}% leaf fill ({} leaves, {C} entries per leaf, branch fan-out {})",
            average * 100.0,
            leaves.len(),
            Branch::<K, V, C>::CAPACITY
        );
        assert!(
            average > MIN_AVERAGE_FILL,
            "{shape}: average fill {average:.4}"
        );
    }

    #[test]
    fn random_replacements_keep_leaves_dense() {
        check_churn::<u32, u32, { leaf_capacity::<u32, u32>() }>("map_u32_u32");
        check_churn::<u64, u64, CAP>("map_u64_u64");
        check_churn::<u64, [u8; 32], { leaf_capacity::<u64, [u8; 32]>() }>("map_u64_payload32");
        check_churn::<u64, (), { leaf_capacity::<u64, ()>() }>("set_u64");
    }
}
