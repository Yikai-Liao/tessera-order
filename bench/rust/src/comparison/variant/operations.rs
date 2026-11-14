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
        "tessera-unstable" => match operation {
            Operation::TopK { k } => prepare_call(
                input,
                false,
                move |v| T::top_k_unstable(v, k),
                "public unordered top-k; arbitrary equal-key members",
            ),
            Operation::Prefix { prefix_len } => prepare_call(
                input,
                false,
                move |v| T::prefix_unstable(v, prefix_len),
                "public sorted prefix; arbitrary ties",
            ),
            Operation::Select { index } => prepare_call(
                input,
                false,
                move |v| T::select_unstable(v, index),
                "public single selection; arbitrary ties",
            ),
            Operation::RankRange { start, end } => prepare_call(
                input,
                false,
                move |v| T::range_unstable(v, start..end),
                "public sorted rank range; arbitrary ties",
            ),
            Operation::SelectMany { indices } => prepare_call(
                input,
                false,
                move |v| T::many_unstable(v, &indices),
                "public multiple selection including normalization; arbitrary ties",
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
                            "tessera-full" | "tessera-unstable" => {
                                operation.required_ranges().is_some()
                            }
                            "radsort" => !ty.starts_with("reverse_"),
                            "std-unstable" => {
                                !matches!(operation, Operation::Segments { .. })
                                    || matches!(
                                        ty,
                                        "u32"
                                            | "u64"
                                            | "u128"
                                            | "f64"
                                            | "reverse_u64"
                                            | "reverse_f64"
                                    )
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
                                    if matches!(operation, Operation::Argsort {}) {
                                        len * size_of::<usize>()
                                    } else {
                                        0
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
