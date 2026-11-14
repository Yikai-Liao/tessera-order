// SPDX-License-Identifier: BSD-2-Clause

//! Complete serial operation adapters. In-place calls share `BatchKernel`;
//! index outputs own a distinct kernel because their immutable source, result
//! allocation and restoration lifetime differ. Capability decisions live here
//! and in `Variant::prepare`, not in CLI or report dictionaries.

use super::{
    BenchmarkRecord, Kernel, PreparedRecords, TesseraAdapter, Variant, glidesort, prepare_call,
};
use crate::plan::Operation;
use std::{hint::black_box, rc::Rc};

struct IndexKernel<T, F> {
    input: Rc<PreparedRecords<T>>,
    reference: Vec<usize>,
    outputs: Vec<Vec<usize>>,
    call: F,
}

impl<T: BenchmarkRecord, F: FnMut(&[T]) -> Vec<usize>> Kernel for IndexKernel<T, F> {
    fn restore(&mut self, batch: usize) {
        // The public result is created inside every timed call, never reused as
        // free scratch. Discard earlier outputs outside timing and allocation.
        self.outputs.clear();
        self.outputs.resize_with(batch, Vec::new);
    }
    fn run_batch(&mut self) {
        for output in &mut self.outputs {
            *output = (self.call)(black_box(self.input.original()));
        }
    }
    fn check(&self) -> Result<(), String> {
        if self.outputs.iter().all(|output| *output == self.reference) {
            Ok(())
        } else {
            Err("index output differs from the exact stable index permutation".into())
        }
    }
    fn buffer_bytes(&self) -> usize {
        self.input.original().len() * size_of::<usize>()
    }
    fn worker_threads(&self) -> usize {
        0
    }
    fn output_bytes(&self) -> usize {
        self.outputs
            .first()
            .map_or(0, |v| v.capacity() * size_of::<usize>())
    }
    fn description(&self) -> &'static str {
        "index construction and stable index sorting"
    }
}

/// Prepares immutable source and independent stable index reference once.
pub(super) fn indices<T: BenchmarkRecord>(
    input: Rc<PreparedRecords<T>>,
    call: impl FnMut(&[T]) -> Vec<usize> + 'static,
) -> Box<dyn Kernel> {
    let mut reference: Vec<_> = (0..input.original().len()).collect();
    reference.sort_by(|&a, &b| input.original()[a].order(&input.original()[b]));
    Box::new(IndexKernel {
        input,
        reference,
        outputs: Vec::new(),
        call,
    })
}

/// Outputs with different allocation contracts; the binary cut is a scalar.
#[derive(Debug, PartialEq, Eq)]
enum Boundaries {
    Cut(usize),
    Offsets(Vec<usize>),
}
impl Boundaries {
    fn new(operation: &Operation) -> Self {
        match operation {
            Operation::Partition {} => Self::Cut(0),
            Operation::PartitionBuckets { bucket_count } => {
                Self::Offsets(vec![0; bucket_count + 1])
            }
            _ => unreachable!("category operation"),
        }
    }
    /// Fills the fresh result from `new`; each offsets table is initialized once.
    fn count<T: BenchmarkRecord>(&mut self, values: &[T]) {
        match self {
            Self::Cut(count) => *count = values.iter().filter(|r| r.category(2) == 0).count(),
            Self::Offsets(offsets) => {
                let bucket_count = offsets.len() - 1;
                for record in values {
                    offsets[record.category(bucket_count) + 1] += 1;
                }
                for bucket in 0..bucket_count {
                    offsets[bucket + 1] += offsets[bucket];
                }
            }
        }
    }
    fn heap_bytes(&self) -> usize {
        match self {
            Self::Cut(_) => 0,
            Self::Offsets(offsets) => offsets.capacity() * size_of::<usize>(),
        }
    }
}

/// Independent histogram output, also used after sorting-based references.
fn boundaries<T: BenchmarkRecord>(values: &[T], operation: &Operation) -> Boundaries {
    let mut output = Boundaries::new(operation);
    output.count(values);
    output
}

/// Category calls mutate records and return boundaries. Output must survive
/// until checking, but must not become reusable workspace for the next call.
struct CategoryKernel<T, F> {
    input: Rc<PreparedRecords<T>>,
    buffers: Vec<Vec<T>>,
    outputs: Vec<Boundaries>,
    expected: Boundaries,
    call: F,
    description: &'static str,
}
impl<T: BenchmarkRecord, F: FnMut(&mut [T]) -> Boundaries> Kernel for CategoryKernel<T, F> {
    fn restore(&mut self, batch: usize) {
        self.outputs.clear();
        self.outputs.resize_with(batch, || Boundaries::Cut(0));
        self.buffers
            .resize_with(batch, || self.input.original().clone());
        for values in &mut self.buffers {
            values.clone_from(self.input.original());
        }
    }
    fn run_batch(&mut self) {
        for (values, output) in self.buffers.iter_mut().zip(&mut self.outputs) {
            *output = (self.call)(black_box(values));
        }
    }
    fn check(&self) -> Result<(), String> {
        for (values, output) in self.buffers.iter().zip(&self.outputs) {
            super::super::check::check(
                values,
                self.input.original(),
                self.input.reference(),
                true,
            )?;
            if *output != self.expected {
                return Err("category boundaries differ from the independent histogram".into());
            }
        }
        Ok(())
    }
    fn buffer_bytes(&self) -> usize {
        self.input.buffer_bytes() + self.expected.heap_bytes()
    }
    fn worker_threads(&self) -> usize {
        0
    }
    fn output_bytes(&self) -> usize {
        self.outputs.first().map_or(0, Boundaries::heap_bytes)
    }
    fn description(&self) -> &'static str {
        self.description
    }
}

/// Complete category routes. Sorting references order by category, never by
/// the original key; they include boundary construction and are labelled as
/// compositions, not direct output-buffer partition algorithms.
pub(super) fn categories<T: BenchmarkRecord>(
    name: &'static str,
    input: Rc<PreparedRecords<T>>,
) -> Option<Box<dyn Kernel>> {
    let operation = input.operation().clone();
    let bucket_count = operation.bucket_count()?;
    let description = match name {
        "tessera" if bucket_count <= tessera_order::MAX_PARTITION_BUCKETS => {
            "public one-shot stable category partition including boundary output"
        }
        "tessera-full" => "Tessera by-key category sorting plus boundary construction",
        "tessera-cached" => "Tessera cached-key category sorting plus boundary construction",
        "std-stable" => "standard stable category sorting plus boundary construction",
        "glidesort" => "glidesort stable category sorting plus boundary construction",
        "radsort" => "radsort stable category sorting plus boundary construction",
        _ => return None,
    };
    Some(Box::new(CategoryKernel {
        expected: boundaries(input.original(), &operation),
        input,
        buffers: Vec::new(),
        outputs: Vec::new(),
        description,
        call: move |values: &mut [T]| {
            if name == "tessera" {
                return if matches!(operation, Operation::Partition {}) {
                    Boundaries::Cut(tessera_order::stable_partition(values, |r| {
                        r.category(2) == 0
                    }))
                } else {
                    Boundaries::Offsets(tessera_order::stable_partition_by_bucket(
                        values,
                        bucket_count,
                        |r| r.category(bucket_count),
                    ))
                };
            }
            // Keep mandatory output live while sorting scratch is live too,
            // as in the public partition. Peak minus output then measures
            // workspace rather than subtracting a later, disjoint allocation.
            let mut output = Boundaries::new(&operation);
            if matches!(operation, Operation::Partition {}) {
                // The predicate entry also supplies a literal two. Keep the
                // same classifier specialization rather than charging only
                // sorting references for a runtime category-count division.
                sort_by_category(name, values, 2, |r| r.category(2));
            } else {
                sort_by_category(name, values, bucket_count, |r| r.category(bucket_count));
            }
            output.count(values);
            output
        },
    }))
}

/// Sorts by the supplied category, sharing its literal or runtime fan-out
/// specialization across all sorting references. The cached-key route takes
/// the narrowest unsigned key holding `bucket_count` categories, as a caller
/// would write it, since that type sets the size of its key cache.
fn sort_by_category<T: BenchmarkRecord>(
    name: &str,
    values: &mut [T],
    bucket_count: usize,
    category: impl Fn(&T) -> usize,
) {
    match name {
        "tessera-full" => tessera_order::stable_sort_by_key(values, category),
        "tessera-cached" if bucket_count <= 1 << u8::BITS => {
            tessera_order::sort_by_cached_key(values, |r| category(r) as u8);
        }
        "tessera-cached" => tessera_order::sort_by_cached_key(values, |r| category(r) as u16),
        "std-stable" => values.sort_by_key(category),
        "glidesort" => glidesort::sort_by(values, |a, b| category(a).cmp(&category(b))),
        "radsort" => radsort::sort_by_key(values, category),
        _ => unreachable!(),
    }
}

/// Gives a complete stable sorter the requested operation's complete contract.
/// These are explicit reference compositions, not missing-API fallbacks.
pub(super) fn reference<T: BenchmarkRecord>(
    input: Rc<PreparedRecords<T>>,
    mut sort: impl FnMut(&mut [T]) + 'static,
) -> Box<dyn Kernel> {
    let operation = input.operation().clone();
    let offsets = operation.offsets(input.original().len());
    let description = if offsets.is_some() {
        "independent stable sorting of every segment"
    } else {
        "complete stable sorting reference for required ranks"
    };
    prepare_call(
        input,
        true,
        move |values| {
            operation
                .validate(values.len())
                .expect("validated operation");
            if operation.is_noop(values.len()) {
                return;
            }
            if let Some(offsets) = &offsets {
                for ends in offsets.windows(2) {
                    sort(&mut values[ends[0]..ends[1]]);
                }
            } else {
                sort(values);
            }
        },
        description,
    )
}

/// Prepares typed public Tessera entries or equivalent Rust compositions.
pub(super) fn prepare<T: TesseraAdapter>(
    v: Variant,
    input: Rc<PreparedRecords<T>>,
) -> Option<Box<dyn Kernel>> {
    let operation = input.operation().clone();
    if operation.bucket_count().is_some() {
        return categories(v.name, input);
    }
    if matches!(operation, Operation::Argsort {}) {
        return Some(match v.name {
            "tessera" => indices(input, T::argsort),
            "std-stable" | "glidesort" | "std-unstable" => indices(input, move |source| {
                let mut indices: Vec<_> = (0..source.len()).collect();
                let order = |a: &usize, b: &usize| source[*a].order(&source[*b]);
                match v.name {
                    "std-stable" => indices.sort_by(order),
                    "glidesort" => glidesort::sort_by(&mut indices, order),
                    // Original index is an explicit tie-breaker, so this still
                    // returns the exact stable permutation for payload records.
                    _ => indices.sort_unstable_by(|a, b| order(a, b).then(a.cmp(b))),
                }
                indices
            }),
            _ => return None,
        });
    }
    Some(match v.name {
        "tessera" => match operation {
            Operation::Segments { .. } => {
                let offsets = operation.offsets(input.original().len()).unwrap();
                prepare_call(
                    input,
                    true,
                    move |values| T::segments(values, &offsets),
                    "public segmented sort with invocation workspace reuse",
                )
            }
            Operation::TopK { k } => prepare_call(
                input,
                true,
                move |values| T::top_k(values, k),
                "public stable-membership unordered top-k",
            ),
            Operation::Prefix { prefix_len } => prepare_call(
                input,
                true,
                move |v| T::prefix(v, prefix_len),
                "public stable prefix sort",
            ),
            Operation::Select { index } => prepare_call(
                input,
                true,
                move |v| T::select(v, index),
                "public stable single selection",
            ),
            Operation::RankRange { start, end } => prepare_call(
                input,
                true,
                move |v| T::rank_range(v, start..end),
                "public stable sorted rank range",
            ),
            Operation::SelectMany { indices } => prepare_call(
                input,
                true,
                move |v| T::select_many(v, &indices),
                "public stable multiple selection including normalization",
            ),
            _ => return None,
        },

        "tessera-full" if operation.required_ranges().is_some() => reference(input, T::tessera),
        "std-stable" => reference(input, |v| v.sort_by(T::order)),
        "glidesort" => reference(input, glidesort::<T>),
        "std-unstable" if !T::HAS_PAYLOAD || !matches!(operation, Operation::Segments { .. }) => {
            let offsets = operation.offsets(input.original().len());
            prepare_call(
                input,
                false,
                move |values| {
                    if let Some(offsets) = &offsets {
                        for ends in offsets.windows(2) {
                            values[ends[0]..ends[1]].sort_unstable_by(T::order);
                        }
                    } else {
                        partial_unstable(values, &operation);
                    }
                },
                "slice unstable selection/sorting; arbitrary equal-key members",
            )
        }
        _ => return None,
    })
}

/// Complete comparison-based partial operations under the weak tie contract.
fn partial_unstable<T: BenchmarkRecord>(values: &mut [T], operation: &Operation) {
    match operation {
        Operation::TopK { k } => {
            if *k > 0 && *k < values.len() {
                values.select_nth_unstable_by(*k, T::order);
            }
        }
        Operation::Prefix { prefix_len } => {
            if *prefix_len == 0 {
                return;
            }
            if *prefix_len < values.len() {
                values.select_nth_unstable_by(*prefix_len, T::order);
            }
            values[..*prefix_len].sort_unstable_by(T::order);
        }
        Operation::Select { index } => {
            values.select_nth_unstable_by(*index, T::order);
        }
        Operation::RankRange { start, end } => {
            if start == end {
                return;
            }
            if *start > 0 {
                values.select_nth_unstable_by(*start, T::order);
            }
            let tail = &mut values[*start..];
            let count = end - start;
            if count < tail.len() {
                tail.select_nth_unstable_by(count, T::order);
            }
            tail[..count].sort_unstable_by(T::order);
        }
        Operation::SelectMany { indices } => {
            let mut ranks = indices.clone();
            ranks.sort_unstable();
            ranks.dedup();
            select_many(values, &ranks, 0);
        }
        _ => unreachable!("prepared partial operation"),
    }
}

fn select_many<T: BenchmarkRecord>(values: &mut [T], ranks: &[usize], base: usize) {
    if ranks.is_empty() {
        return;
    }
    let middle = ranks.len() / 2;
    let index = ranks[middle] - base;
    let (left, _, right) = values.select_nth_unstable_by(index, T::order);
    select_many(left, &ranks[..middle], base);
    select_many(right, &ranks[middle + 1..], base + index + 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        comparison::{CaseInput, list_variants},
        plan::InputCase,
    };

    #[test]
    fn complete_operations_restore_validate_and_decline_unsupported_tasks() {
        let len = 4097;
        let operations = [
            Operation::Argsort {},
            Operation::Partition {},
            Operation::PartitionBuckets { bucket_count: 1 },
            Operation::PartitionBuckets { bucket_count: 3 },
            Operation::PartitionBuckets { bucket_count: 16 },
            Operation::TopK { k: 0 },
            Operation::TopK { k: 513 },
            Operation::TopK { k: len },
            Operation::Segments { segment_len: 73 },
            Operation::Prefix { prefix_len: 0 },
            Operation::Prefix { prefix_len: 513 },
            Operation::Prefix { prefix_len: len },
            Operation::Select { index: len / 2 },
            Operation::RankRange {
                start: 513,
                end: 1537,
            },
            Operation::RankRange { start: 19, end: 19 },
            Operation::SelectMany {
                indices: vec![len - 1, 0, 513, 513, 514],
            },
            Operation::SelectMany { indices: vec![] },
        ];
        for ty in crate::plan::catalog().types.iter().map(|ty| ty.id.as_str()) {
            for distribution in ["random", "d16"] {
                for operation in &operations {
                    let input = CaseInput::new(&InputCase {
                        ty: ty.into(),
                        distribution: distribution.into(),
                        len,
                        operation: operation.clone(),
                    });
                    for variant in list_variants() {
                        // An independent capability oracle protects admission,
                        // including no silent full-sort substitution.
                        let expected = match variant.name {
                            "tessera" | "std-stable" | "glidesort" => true,
                            "tessera-full" => {
                                operation.required_ranges().is_some()
                                    || operation.bucket_count().is_some()
                            }
                            "tessera-cached" => operation.bucket_count().is_some(),
                            "radsort" => {
                                operation.bucket_count().is_some() || !ty.starts_with("reverse_")
                            }
                            "std-unstable" => {
                                operation.bucket_count().is_none()
                                    && (!matches!(operation, Operation::Segments { .. })
                                        || matches!(
                                            ty,
                                            "u32"
                                                | "u64"
                                                | "u128"
                                                | "f64"
                                                | "reverse_u64"
                                                | "reverse_f64"
                                        ))
                            }
                            _ => false,
                        };
                        let prepared = variant.prepare(&input, 0);
                        assert_eq!(
                            prepared.is_some(),
                            expected,
                            "{ty}/{operation:?}/{}",
                            variant.name
                        );
                        if let Some(mut kernel) = prepared {
                            for batch in [2, 1] {
                                kernel.restore(batch);
                                kernel.run_batch();
                                kernel.check().unwrap_or_else(|error| {
                                    panic!(
                                        "{ty}/{distribution}/{operation:?}/{}: {error}",
                                        variant.name
                                    )
                                });
                                assert_eq!(
                                    kernel.output_bytes(),
                                    match operation {
                                        Operation::Argsort {} => len * size_of::<usize>(),
                                        Operation::PartitionBuckets { bucket_count } =>
                                            (bucket_count + 1) * size_of::<usize>(),
                                        _ => 0,
                                    }
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn category_output_counts_toward_the_prepared_memory_cap() {
        let CaseInput::U32(input) = CaseInput::new(&InputCase {
            ty: "u32".into(),
            distribution: "random".into(),
            len: 1,
            operation: Operation::PartitionBuckets {
                bucket_count: tessera_order::MAX_PARTITION_BUCKETS,
            },
        }) else {
            unreachable!()
        };
        let retained = (tessera_order::MAX_PARTITION_BUCKETS + 1) * size_of::<usize>();
        let mut kernel = categories("tessera", input).unwrap();
        assert_eq!(kernel.buffer_bytes(), size_of::<u32>() + retained);
        kernel.restore(3);
        kernel.run_batch();
        kernel.check().unwrap();
        assert_eq!(kernel.output_bytes(), retained);
    }

    #[test]
    fn category_checker_rejects_key_sorting_and_corrupt_boundaries() {
        let CaseInput::U64(input) = CaseInput::new(&InputCase {
            ty: "u64".into(),
            distribution: "random".into(),
            len: 128,
            operation: Operation::PartitionBuckets { bucket_count: 7 },
        }) else {
            unreachable!()
        };
        for corrupt_boundary in [false, true] {
            let operation = input.operation().clone();
            let mut wrong = CategoryKernel {
                expected: boundaries(input.original(), &operation),
                input: input.clone(),
                buffers: Vec::new(),
                outputs: Vec::new(),
                description: "intentionally wrong partition",
                call: move |values: &mut [u64]| {
                    if corrupt_boundary {
                        values.sort_by_key(|r| r.category(7));
                    } else {
                        values.sort();
                    }
                    let Boundaries::Offsets(mut offsets) = boundaries(values, &operation) else {
                        unreachable!()
                    };
                    if corrupt_boundary {
                        offsets[1] += 1;
                    }
                    Boundaries::Offsets(offsets)
                },
            };
            wrong.restore(2);
            wrong.run_batch();
            assert!(wrong.check().is_err());
        }
    }

    #[test]
    fn no_op_checkers_require_original_order() {
        let len = 128;
        for operation in [
            Operation::TopK { k: 0 },
            Operation::TopK { k: len },
            Operation::Prefix { prefix_len: 0 },
            Operation::RankRange { start: 19, end: 19 },
            Operation::SelectMany { indices: vec![] },
        ] {
            let CaseInput::U64(input) = CaseInput::new(&InputCase {
                ty: "u64".into(),
                distribution: "random".into(),
                len,
                operation,
            }) else {
                unreachable!()
            };
            let mut correct = prepare_call(input.clone(), true, |_| {}, "no-op");
            correct.restore(1);
            correct.run_batch();
            assert!(correct.check().is_ok());
            let mut wrong = prepare_call(input, true, |v| v.sort(), "incorrect no-op");
            wrong.restore(1);
            wrong.run_batch();
            assert!(wrong.check().is_err());
        }
    }

    #[test]
    fn output_checkers_reject_wrong_permutations_and_cross_segment_moves() {
        let case = InputCase {
            ty: "u64".into(),
            distribution: "random".into(),
            len: 128,
            operation: Operation::Argsort {},
        };
        let CaseInput::U64(input) = CaseInput::new(&case) else {
            unreachable!()
        };
        let mut wrong = indices(input, |source| vec![0; source.len()]);
        wrong.restore(1);
        wrong.run_batch();
        assert!(wrong.check().is_err());
        let CaseInput::U64(input) = CaseInput::new(&InputCase {
            operation: Operation::Segments { segment_len: 17 },
            ..case
        }) else {
            unreachable!()
        };
        let mut wrong = prepare_call(input, true, |v| v.sort(), "intentionally wrong full sort");
        wrong.restore(1);
        wrong.run_batch();
        assert!(wrong.check().is_err());
    }
}
