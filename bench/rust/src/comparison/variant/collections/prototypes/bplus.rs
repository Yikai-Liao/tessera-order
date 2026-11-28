// SPDX-License-Identifier: BSD-2-Clause

use super::super::{Collection, IntKey, Value};
use std::mem::{replace, size_of, swap};

const NODE_PAYLOAD_BYTES: usize = 1024;
const MIN_LEAF_CAPACITY: usize = 16;

pub(in super::super) const fn leaf_capacity<K, V>() -> usize {
    let entries = NODE_PAYLOAD_BYTES / (size_of::<K>() + size_of::<V>());
    if entries < MIN_LEAF_CAPACITY {
        MIN_LEAF_CAPACITY
    } else {
        entries
    }
}

// Non-root nodes are at least half full, except the rightmost leaf after packing or appending.
const fn min_fill(capacity: usize) -> usize {
    capacity / 2
}

fn bound<K: Copy>(keys: &[K], before: impl Fn(K) -> bool) -> usize {
    let mut base = 0;
    let mut len = keys.len();
    while len > 1 {
        let half = len / 2;
        base += usize::from(before(keys[base + half - 1])) * half;
        len -= half;
    }
    base + usize::from(keys.get(base).is_some_and(|&key| before(key)))
}

pub(in super::super) struct BPlus<K, V, const CAP: usize> {
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
    children: Vec<Child<K, V, CAP>>,
}

struct Child<K, V, const CAP: usize> {
    node: Node<K, V, CAP>,
    count: usize,
}

impl<K: Copy + Ord, V: Copy, const CAP: usize> Leaf<K, V, CAP> {
    fn single(key: K, value: V) -> Self {
        Self {
            keys: [key; CAP],
            values: [value; CAP],
            len: 1,
        }
    }

    fn lower_bound(&self, key: K) -> usize {
        bound(&self.keys[..self.len], |other| other < key)
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

    fn insert_into_pair(left: &mut Self, right: &mut Self, key: K, value: V) {
        let target = if key <= left.keys[left.len - 1] {
            left
        } else {
            right
        };
        target.insert(target.lower_bound(key), key, value);
    }

    fn split_insert(&mut self, key: K, value: V) -> Node<K, V, CAP> {
        let mut right = Box::new(Self::single(key, value));
        if key < self.keys[CAP - 1] {
            right.len = 0;
            Self::balance(self, &mut right, min_fill(CAP));
            Self::insert_into_pair(self, &mut right, key, value);
        }
        Node::Leaf(right)
    }
}

impl<K: Copy + Ord, V: Copy, const CAP: usize> Branch<K, V, CAP> {
    const CAPACITY: usize = NODE_PAYLOAD_BYTES / (size_of::<K>() + size_of::<Child<K, V, CAP>>());

    fn new() -> Self {
        Self {
            keys: Vec::with_capacity(Self::CAPACITY + 1),
            children: Vec::with_capacity(Self::CAPACITY + 1),
        }
    }

    fn push(&mut self, node: Node<K, V, CAP>) {
        self.keys.push(node.max_key());
        let count = node.count();
        self.children.push(Child { node, count });
    }

    fn child_for(&self, key: K) -> usize {
        bound(&self.keys, |other| other < key).min(self.keys.len() - 1)
    }

    fn child_at(&self, mut index: usize) -> Option<(usize, usize)> {
        for (slot, child) in self.children.iter().enumerate() {
            if index < child.count {
                return Some((slot, index));
            }
            index -= child.count;
        }
        None
    }

    fn refresh(&mut self, slot: usize) {
        let child = &mut self.children[slot];
        self.keys[slot] = child.node.max_key();
        child.count = child.node.count();
    }

    fn grow_child(
        &mut self,
        slot: usize,
        change: isize,
        right: Option<Node<K, V, CAP>>,
    ) -> Option<Node<K, V, CAP>> {
        self.keys[slot] = self.children[slot].node.max_key();
        self.children[slot].count = self.children[slot].count.wrapping_add_signed(change);
        let right = right?;
        let count = right.count();
        self.children[slot].count -= count;
        self.keys.insert(slot + 1, right.max_key());
        self.children.insert(slot + 1, Child { node: right, count });
        if self.children.len() <= Self::CAPACITY {
            return None;
        }
        let mut right = Box::new(Self::new());
        let midpoint = min_fill(Self::CAPACITY);
        right.keys.extend(self.keys.drain(midpoint..));
        right.children.extend(self.children.drain(midpoint..));
        Some(Node::Branch(right))
    }

    fn remove_child(&mut self, slot: usize) {
        self.keys.remove(slot);
        self.children.remove(slot);
    }

    fn repair(&mut self, slot: usize) {
        if self.children.len() < 2 {
            return;
        }
        let left_slot = slot.saturating_sub(1);
        let (left, right) = self.children.split_at_mut(left_slot + 1);
        let left = &mut left[left_slot].node;
        let right = &mut right[0].node;
        let total = left.slots() + right.slots();
        let merged = total <= left.capacity();
        let left_len = if merged { total } else { total / 2 };
        match (left, right) {
            (Node::Leaf(left), Node::Leaf(right)) => Leaf::balance(left, right, left_len),
            (Node::Branch(left), Node::Branch(right)) => {
                if left_len < left.children.len() {
                    right.keys.splice(..0, left.keys.drain(left_len..));
                    right.children.splice(..0, left.children.drain(left_len..));
                } else {
                    let moved = left_len - left.children.len();
                    left.keys.extend(right.keys.drain(..moved));
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

impl<K: Copy + Ord, V: Copy, const CAP: usize> Node<K, V, CAP> {
    fn max_key(&self) -> K {
        match self {
            Self::Leaf(leaf) => leaf.keys[leaf.len - 1],
            Self::Branch(branch) => *branch.keys.last().unwrap(),
        }
    }

    fn count(&self) -> usize {
        match self {
            Self::Leaf(leaf) => leaf.len,
            Self::Branch(branch) => branch.children.iter().map(|child| child.count).sum(),
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

    fn leaf_for(&self, key: K) -> &Leaf<K, V, CAP> {
        match self {
            Self::Leaf(leaf) => leaf,
            Self::Branch(branch) => branch.children[branch.child_for(key)].node.leaf_for(key),
        }
    }

    fn leaf_at(&self, index: usize) -> Option<(&Leaf<K, V, CAP>, usize)> {
        match self {
            Self::Leaf(leaf) => (index < leaf.len).then_some((leaf, index)),
            Self::Branch(branch) => {
                let (slot, offset) = branch.child_at(index)?;
                branch.children[slot].node.leaf_at(offset)
            }
        }
    }

    fn rank(&self, key: K) -> usize {
        match self {
            Self::Leaf(leaf) => leaf.lower_bound(key),
            Self::Branch(branch) => {
                let slot = branch.child_for(key);
                let prefix: usize = branch.children[..slot]
                    .iter()
                    .map(|child| child.count)
                    .sum();
                prefix + branch.children[slot].node.rank(key)
            }
        }
    }

    fn endpoint(&self, last: bool) -> Option<(K, V)> {
        match self {
            Self::Leaf(leaf) => leaf.entry(if last { leaf.len - 1 } else { 0 }),
            Self::Branch(branch) => {
                let slot = if last { branch.children.len() - 1 } else { 0 };
                branch.children[slot].node.endpoint(last)
            }
        }
    }

    // An overflow returns the full leaf's starting rank without changing the tree.
    fn try_insert(&mut self, key: K, value: V) -> Result<Option<V>, usize> {
        match self {
            Self::Leaf(leaf) => {
                let index = leaf.lower_bound(key);
                if index < leaf.len && leaf.keys[index] == key {
                    return Ok(Some(replace(&mut leaf.values[index], value)));
                }
                if leaf.len == CAP {
                    return Err(0);
                }
                leaf.insert(index, key, value);
                Ok(None)
            }
            Self::Branch(branch) => {
                let slot = branch.child_for(key);
                match branch.children[slot].node.try_insert(key, value) {
                    Ok(old) => {
                        if old.is_none() {
                            branch.grow_child(slot, 1, None);
                        }
                        Ok(old)
                    }
                    Err(start) => Err(start
                        + branch.children[..slot]
                            .iter()
                            .map(|c| c.count)
                            .sum::<usize>()),
                }
            }
        }
    }

    fn grow_at(
        &mut self,
        index: usize,
        change: isize,
        edit: impl FnOnce(&mut Leaf<K, V, CAP>) -> Option<Self>,
    ) -> Option<Self> {
        match self {
            Self::Leaf(leaf) => edit(leaf),
            Self::Branch(branch) => {
                let (slot, offset) = branch.child_at(index).unwrap();
                let right = branch.children[slot].node.grow_at(offset, change, edit);
                branch.grow_child(slot, change, right)
            }
        }
    }

    fn remove(&mut self, key: K) -> Option<V> {
        match self {
            Self::Leaf(leaf) => {
                let index = leaf.lower_bound(key);
                (index < leaf.len && leaf.keys[index] == key).then(|| leaf.remove(index).1)
            }
            Self::Branch(branch) => {
                let slot = branch.child_for(key);
                let child = &mut branch.children[slot];
                let value = child.node.remove(key)?;
                let slots = child.node.slots();
                if slots == 0 {
                    branch.remove_child(slot);
                } else {
                    branch.keys[slot] = child.node.max_key();
                    child.count -= 1;
                    if slots < min_fill(child.node.capacity()) {
                        branch.repair(slot);
                    }
                }
                Some(value)
            }
        }
    }

    fn visit(&self, lo: Option<K>, hi: Option<K>, visit: &mut impl FnMut((K, V))) {
        match self {
            Self::Leaf(leaf) => {
                let start = lo.map_or(0, |key| leaf.lower_bound(key));
                let end = hi.map_or(leaf.len, |key| bound(&leaf.keys[..leaf.len], |k| k <= key));
                leaf.keys[start..end]
                    .iter()
                    .copied()
                    .zip(leaf.values[start..end].iter().copied())
                    .for_each(visit);
            }
            Self::Branch(branch) => {
                let first = lo.map_or(0, |key| branch.child_for(key));
                let last = hi.map_or(branch.children.len() - 1, |key| branch.child_for(key));
                for slot in first..=last {
                    let lo = if slot == first { lo } else { None };
                    let hi = if slot == last { hi } else { None };
                    branch.children[slot].node.visit(lo, hi, visit);
                }
            }
        }
    }
}

impl<K: Copy + Ord, V: Copy, const CAP: usize> BPlus<K, V, CAP> {
    fn split_full_leaf(
        root: &mut Node<K, V, CAP>,
        start: usize,
        key: K,
        value: V,
    ) -> Option<Node<K, V, CAP>> {
        let right = root.leaf_at(start + CAP);
        let leaf = root.leaf_at(start).unwrap().0;
        if (start == 0 && right.is_none()) || key > leaf.keys[CAP - 1] {
            return root.grow_at(start, 1, |leaf| Some(leaf.split_insert(key, value)));
        }

        // Split two full leaves into three. Move the sibling's entries out first,
        // then use the target's new starting rank to finish the split.
        let moved = CAP / 3;
        let kept = CAP - moved;
        let mut middle = Box::new(Leaf::single(key, value));
        middle.len = 0;
        let target = if start > 0 {
            root.grow_at(start - 1, -(moved as isize), |leaf| {
                Leaf::balance(leaf, &mut middle, kept);
                None
            });
            start - moved
        } else {
            root.grow_at(CAP, -(moved as isize), |leaf| {
                Leaf::balance(&mut middle, leaf, moved);
                None
            });
            start
        };
        root.grow_at(target, moved as isize + 1, |leaf| {
            if start > 0 {
                Leaf::balance(&mut middle, leaf, kept);
                swap(leaf, &mut middle);
            } else {
                Leaf::balance(leaf, &mut middle, kept);
            }
            Leaf::insert_into_pair(leaf, &mut middle, key, value);
            Some(Node::Leaf(middle))
        })
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
                    let mut branch = Box::new(Branch::new());
                    for node in nodes.by_ref().take(base + usize::from(group < extra)) {
                        branch.push(node);
                    }
                    Node::Branch(branch)
                })
                .collect();
        }
        Self { root: level.pop() }
    }
}

impl<K: IntKey, V: Value, const CAP: usize> Collection<K, V> for BPlus<K, V, CAP> {
    const NAME: &'static str = "Counted B+ tree";
    const BUILD: &'static str = "Tessera sort_by_key + packed B+ tree";
    const ENDPOINTS: &'static str = "Counted B+ tree edge descent";
    const ORDER_STATISTICS: bool = true;

    fn empty() -> Self {
        Self { root: None }
    }

    fn build(entries: Vec<(K, V)>) -> Self {
        entries.into()
    }

    fn get(&self, key: K) -> Option<V> {
        let leaf = self.root.as_ref()?.leaf_for(key);
        let index = leaf.lower_bound(key);
        (index < leaf.len && leaf.keys[index] == key).then(|| leaf.values[index])
    }

    fn insert(&mut self, key: K, value: V) -> Option<V> {
        let Some(root) = &mut self.root else {
            self.root = Some(Node::Leaf(Box::new(Leaf::single(key, value))));
            return None;
        };
        let start = match root.try_insert(key, value) {
            Ok(old) => return old,
            Err(start) => start,
        };
        let left = start
            .checked_sub(1)
            .filter(|&index| root.leaf_at(index).unwrap().0.len < CAP);
        let right = start + CAP;
        let neighbor = left.map(|index| (index, true)).or_else(|| {
            root.leaf_at(right)
                .filter(|(leaf, _)| leaf.len < CAP)
                .map(|_| (right, false))
        });
        if let Some((neighbor, to_left)) = neighbor {
            let mut entry = (key, value);
            root.grow_at(start, 0, |leaf| {
                let index = leaf.lower_bound(key);
                if to_left && index > 0 {
                    entry = leaf.remove(0);
                    leaf.insert(index - 1, key, value);
                } else if !to_left && index < CAP {
                    entry = leaf.remove(CAP - 1);
                    leaf.insert(index, key, value);
                }
                None
            });
            root.grow_at(neighbor, 1, |leaf| {
                let index = if to_left { leaf.len } else { 0 };
                leaf.insert(index, entry.0, entry.1);
                None
            });
        } else if let Some(right) = Self::split_full_leaf(root, start, key, value) {
            let mut branch = Box::new(Branch::new());
            branch.push(self.root.take().unwrap());
            branch.push(right);
            self.root = Some(Node::Branch(branch));
        }
        None
    }

    fn remove(&mut self, key: K) -> Option<V> {
        let root = self.root.as_mut()?;
        let value = root.remove(key)?;
        match root {
            Node::Branch(branch) if branch.children.len() == 1 => {
                *root = branch.children.pop().unwrap().node;
            }
            _ if root.slots() == 0 => self.root = None,
            _ => {}
        }
        Some(value)
    }

    fn visit(&self, bounds: Option<(K, K)>, mut visit: impl FnMut((K, V))) {
        let (lo, hi) = bounds.map_or((None, None), |(lo, hi)| (Some(lo), Some(hi)));
        if let Some(root) = &self.root {
            root.visit(lo, hi, &mut visit);
        }
    }

    fn endpoint(&self, last: bool) -> Option<(K, V)> {
        self.root.as_ref()?.endpoint(last)
    }

    fn rank(&self, key: K) -> usize {
        self.root.as_ref().map_or(0, |root| root.rank(key))
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
                assert!(branch.keys.windows(2).all(|keys| keys[0] < keys[1]));
                assert!(!root || slots >= 2);
                let mut total = 0;
                let mut height = 0;
                for (slot, child) in branch.children.iter().enumerate() {
                    let (count, child_height) =
                        check_node(&child.node, false, rightmost && slot + 1 == slots, leaves);
                    assert_eq!(child.count, count, "child subtree count at slot {slot}");
                    assert_eq!(branch.keys[slot], child.node.max_key());
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
