// SPDX-License-Identifier: BSD-2-Clause

//! Complete operation adapters. Selection uses the in-place batch kernel.
//! Argsort (serial and parallel) and category grouping retain their allocated
//! outputs until verification, then discard them before the next timed batch.

use super::{BenchmarkRecord, Kernel, PreparedRecords, TesseraAdapter, Variant, glidesort};
use crate::plan::Operation;
use std::{hint::black_box, rc::Rc};

struct IndexKernel<T, F> {
    input: Rc<PreparedRecords<T>>,
    reference: Vec<usize>,
    outputs: Vec<Vec<usize>>,
    call: F,
    description: &'static str,
    worker_threads: usize,
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
        self.worker_threads
    }
    fn output_bytes(&self) -> usize {
        self.outputs
            .first()
            .map_or(0, |v| v.capacity() * size_of::<usize>())
    }
    fn description(&self) -> &'static str {
        self.description
    }
}

/// Prepares immutable source and independent stable index reference once.
pub(in crate::comparison) fn indices<T: BenchmarkRecord>(
    variant: Variant,
    input: Rc<PreparedRecords<T>>,
    description: &'static str,
    call: impl FnMut(&[T]) -> Vec<usize> + 'static,
) -> Box<dyn Kernel> {
    indices_with_workers(variant, input, description, 0, call)
}

/// Prepares a parallel argsort kernel whose caller-owned pool runs `worker_threads` workers.
#[cfg(feature = "parallel-operations")]
pub(super) fn parallel_indices<T: BenchmarkRecord>(
    variant: Variant,
    input: Rc<PreparedRecords<T>>,
    description: &'static str,
    worker_threads: usize,
    call: impl FnMut(&[T]) -> Vec<usize> + 'static,
) -> Box<dyn Kernel> {
    indices_with_workers(variant, input, description, worker_threads, call)
}

fn indices_with_workers<T: BenchmarkRecord>(
    variant: Variant,
    input: Rc<PreparedRecords<T>>,
    description: &'static str,
    worker_threads: usize,
    call: impl FnMut(&[T]) -> Vec<usize> + 'static,
) -> Box<dyn Kernel> {
    assert!(
        variant.route_stable(input.operation()),
        "{} argsort must guarantee the stable index permutation",
        variant.name,
    );
    let mut reference: Vec<_> = (0..input.original().len()).collect();
    reference.sort_by(|&a, &b| input.original()[a].order(&input.original()[b]));
    Box::new(IndexKernel {
        input,
        reference,
        outputs: Vec::new(),
        call,
        description,
        worker_threads,
    })
}

/// Materializes keys beside their source positions, stably sorts the rows,
/// and returns a compact vector of source indices.
pub(super) fn sort_index_rows<T, K: tessera_order::RadixKey>(
    source: &[T],
    key: impl Fn(&T) -> K,
) -> Vec<usize> {
    let mut rows: Vec<_> = source
        .iter()
        .enumerate()
        .map(|(i, r)| (key(r), i))
        .collect();
    tessera_order::sort_by_key(&mut rows, |&(key, _)| key);
    rows.iter().map(|&(_, index)| index).collect()
}

/// Finds boundaries from category transitions in the sorted reference,
/// independently of the histogram used by the timed call.
fn reference_offsets<T: BenchmarkRecord>(reference: &[T], bucket_count: usize) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(bucket_count + 1);
    for (index, record) in reference.iter().enumerate() {
        let category = record.category(bucket_count);
        if category >= offsets.len() {
            // Skipped categories are empty and start at the same position.
            offsets.resize(category + 1, index);
        }
    }
    offsets.resize(bucket_count + 1, reference.len());
    offsets
}

/// Category calls mutate records and return boundaries. Output must survive
/// until checking, but must not become reusable workspace for the next call.
struct CategoryKernel<T, F> {
    input: Rc<PreparedRecords<T>>,
    buffers: Vec<Vec<T>>,
    outputs: Vec<Vec<usize>>,
    expected: Vec<usize>,
    call: F,
    description: &'static str,
}
impl<T: BenchmarkRecord, F: FnMut(&mut [T]) -> Vec<usize>> Kernel for CategoryKernel<T, F> {
    fn restore(&mut self, batch: usize) {
        self.outputs.clear();
        self.outputs.resize_with(batch, Vec::new);
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
                return Err("category offsets differ from the stable grouping reference".into());
            }
        }
        Ok(())
    }
    fn buffer_bytes(&self) -> usize {
        self.input.buffer_bytes() + size_of_val(self.expected.as_slice())
    }
    fn worker_threads(&self) -> usize {
        0
    }
    fn output_bytes(&self) -> usize {
        self.outputs
            .first()
            .map_or(0, |offsets| offsets.capacity() * size_of::<usize>())
    }
    fn description(&self) -> &'static str {
        self.description
    }
}

/// Complete category routes. Sorting references order by category, never by
/// the original key; they include boundary construction and are labelled as
/// compositions, not direct output-buffer partition algorithms.
pub(super) fn categories<T: BenchmarkRecord>(
    variant: Variant,
    input: Rc<PreparedRecords<T>>,
    bucket_count: usize,
) -> Option<Box<dyn Kernel>> {
    let name = variant.name;
    let description = match name {
        "tessera" => "public cached-key category sorting plus boundary construction",
        "tessera-full" => "Tessera by-key category sorting plus boundary construction",
        "std-stable" => "standard stable category sorting plus boundary construction",
        "glidesort" => "glidesort stable category sorting plus boundary construction",
        "radsort" => "radsort stable category sorting plus boundary construction",
        _ => return None,
    };
    assert!(
        variant.route_stable(input.operation()),
        "{name} bucket partition must guarantee stable grouping",
    );
    Some(Box::new(CategoryKernel {
        expected: reference_offsets(input.reference(), bucket_count),
        input,
        buffers: Vec::new(),
        outputs: Vec::new(),
        description,
        call: move |values: &mut [T]| {
            // Keep the result live during sorting so peak minus output
            // measures sorting workspace, not disjoint allocations.
            let mut offsets = vec![0; bucket_count + 1];
            sort_by_category(name, values, bucket_count);
            for record in values {
                offsets[record.category(bucket_count) + 1] += 1;
            }
            for bucket in 0..bucket_count {
                offsets[bucket + 1] += offsets[bucket];
            }
            offsets
        },
    }))
}

/// The cached-key route uses the narrowest unsigned key holding every
/// category, since that type sets the size of its key cache.
fn sort_by_category<T: BenchmarkRecord>(name: &str, values: &mut [T], bucket_count: usize) {
    let category = |r: &T| r.category(bucket_count);
    match name {
        "tessera-full" => tessera_order::sort_by_key(values, category),
        "tessera" if bucket_count <= 1 << u8::BITS => {
            tessera_order::sort_by_cached_key(values, |r| category(r) as u8);
        }
        "tessera" if bucket_count <= 1 << u16::BITS => {
            tessera_order::sort_by_cached_key(values, |r| category(r) as u16);
        }
        "tessera" if u32::try_from(bucket_count - 1).is_ok() => {
            tessera_order::sort_by_cached_key(values, |r| category(r) as u32);
        }
        "tessera" => tessera_order::sort_by_cached_key(values, category),
        "std-stable" => values.sort_by_key(category),
        "glidesort" => glidesort::sort_by(values, |a, b| category(a).cmp(&category(b))),
        "radsort" => radsort::sort_by_key(values, category),
        _ => unreachable!(),
    }
}

/// Full stable sorting satisfies the selected identity and both memberships.
pub(super) fn selection_reference<T: BenchmarkRecord>(
    variant: Variant,
    input: Rc<PreparedRecords<T>>,
    sort: impl FnMut(&mut [T]) + 'static,
) -> Box<dyn Kernel> {
    variant.prepare_call(
        input,
        "complete stable sorting reference for required ranks",
        sort,
    )
}

/// Prepares typed public Tessera entries or equivalent Rust compositions.
pub(super) fn prepare<T: TesseraAdapter>(
    v: Variant,
    input: Rc<PreparedRecords<T>>,
) -> Option<Box<dyn Kernel>> {
    match *input.operation() {
        Operation::Sort {} => None,
        Operation::PartitionBuckets { bucket_count } => categories(v, input, bucket_count),
        Operation::Argsort {} => Some(match v.name {
            "tessera" => indices(v, input, "public stable argsort", T::argsort),
            "tessera-full" => indices(
                v,
                input,
                "Tessera stable sort of (key, index) rows and index extraction",
                T::sort_index_rows,
            ),
            "std-stable" | "glidesort" | "std-unstable" => indices(
                v,
                input,
                "index construction and stable index sorting",
                move |source| {
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
                },
            ),
            _ => return None,
        }),
        Operation::SelectNth { index } => Some(match v.name {
            "tessera" => v.prepare_call(input, "public stable single selection", move |values| {
                T::select_nth(values, index)
            }),
            "tessera-full" => selection_reference(v, input, T::tessera),
            "std-stable" => selection_reference(v, input, |values| values.sort_by(T::order)),
            "glidesort" => selection_reference(v, input, glidesort::<T>),
            "std-unstable" => v.prepare_call(
                input,
                "slice unstable single selection; arbitrary equal-key members",
                move |values| {
                    values.select_nth_unstable_by(index, T::order);
                },
            ),
            _ => return None,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        comparison::{CaseInput, list_variants},
        plan::{Execution, InputCase},
    };

    fn variant(name: &str) -> Variant {
        list_variants()
            .into_iter()
            .find(|v| v.name == name)
            .unwrap()
    }

    #[test]
    fn std_unstable_argsort_verifies_exact_tie_order() {
        let variant = variant("std-unstable");
        let operation = Operation::Argsort {};
        assert!(variant.route_stable(&operation));
        assert!(!variant.route_stable(&Operation::Sort {}));
        assert!(!variant.route_stable(&Operation::SelectNth { index: 0 }));

        let input = CaseInput::new(&InputCase {
            ty: "row_u64".into(),
            distribution: "equal".into(),
            len: 64,
            operation,
        });
        let mut kernel = variant.prepare(&input, Execution::Serial, 0).unwrap();
        kernel.restore(1);
        kernel.run_batch();
        kernel.check().unwrap();

        let CaseInput::Row64(records) = input else {
            unreachable!();
        };
        let mut reordered = indices(variant, records, "reversed test adapter", |source| {
            (0..source.len()).rev().collect()
        });
        reordered.restore(1);
        reordered.run_batch();
        assert!(reordered.check().is_err());
    }

    #[test]
    fn complete_operations_restore_validate_and_decline_unsupported_tasks() {
        let len = 4097;
        let operations = [
            Operation::Argsort {},
            Operation::PartitionBuckets { bucket_count: 1 },
            Operation::PartitionBuckets { bucket_count: 3 },
            Operation::PartitionBuckets { bucket_count: 257 },
            Operation::SelectNth { index: 0 },
            Operation::SelectNth { index: len / 2 },
            Operation::SelectNth { index: len - 1 },
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
                    let category_grouping = matches!(operation, Operation::PartitionBuckets { .. });
                    let selection = matches!(operation, Operation::SelectNth { .. });
                    for variant in list_variants() {
                        // An independent capability oracle protects admission,
                        // including no silent full-sort substitution.
                        let expected = match variant.name {
                            "tessera" | "tessera-full" | "std-stable" | "glidesort" => true,
                            "radsort" => category_grouping || !ty.starts_with("reverse_"),
                            "std-unstable" => !category_grouping,
                            "vqsort" => matches!(ty, "u32" | "u64") && selection,
                            "x86simdsort" => matches!(ty, "u32" | "u64") && !category_grouping,
                            _ => false,
                        };
                        let prepared = variant.prepare(&input, crate::plan::Execution::Serial, 0);
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
        let bucket_count = 70_000; // above u16 keys
        let CaseInput::U32(input) = CaseInput::new(&InputCase {
            ty: "u32".into(),
            distribution: "random".into(),
            len: 1,
            operation: Operation::PartitionBuckets { bucket_count },
        }) else {
            unreachable!()
        };
        let retained = (bucket_count + 1) * size_of::<usize>();
        let mut kernel = categories(variant("tessera"), input, bucket_count).unwrap();
        assert_eq!(kernel.buffer_bytes(), size_of::<u32>() + retained);
        kernel.restore(3);
        kernel.run_batch();
        kernel.check().unwrap();
        assert_eq!(kernel.output_bytes(), retained);
    }

    #[test]
    fn category_checker_rejects_key_sorting_and_corrupt_boundaries() {
        let bucket_count = 7;
        let CaseInput::U64(input) = CaseInput::new(&InputCase {
            ty: "u64".into(),
            distribution: "random".into(),
            len: 128,
            operation: Operation::PartitionBuckets { bucket_count },
        }) else {
            unreachable!()
        };
        for corrupt_boundary in [false, true] {
            let expected = reference_offsets(input.reference(), bucket_count);
            let mut offsets = expected.clone();
            if corrupt_boundary {
                offsets[1] += 1;
            }
            let mut wrong = CategoryKernel {
                expected,
                input: input.clone(),
                buffers: Vec::new(),
                outputs: Vec::new(),
                description: "intentionally wrong partition",
                call: move |values: &mut [u64]| {
                    if corrupt_boundary {
                        values.sort_by_key(|r| r.category(bucket_count));
                    } else {
                        values.sort();
                    }
                    offsets.clone()
                },
            };
            wrong.restore(2);
            wrong.run_batch();
            assert!(wrong.check().is_err());
        }
    }

    #[test]
    fn argsort_checker_rejects_wrong_permutations() {
        let case = InputCase {
            ty: "u64".into(),
            distribution: "random".into(),
            len: 128,
            operation: Operation::Argsort {},
        };
        let CaseInput::U64(input) = CaseInput::new(&case) else {
            unreachable!()
        };
        let mut wrong = indices(
            variant("tessera"),
            input,
            "invalid test adapter",
            |source| vec![0; source.len()],
        );
        wrong.restore(1);
        wrong.run_batch();
        assert!(wrong.check().is_err());
    }

    #[cfg(feature = "parallel-operations")]
    #[test]
    fn parallel_argsort_validates_exact_stable_permutation() {
        let len = 4097;
        for ty in ["u64", "row_u64", "rec128_u64", "owned", "f64"] {
            for distribution in ["random", "d16", "equal"] {
                let input = CaseInput::new(&InputCase {
                    ty: ty.into(),
                    distribution: distribution.into(),
                    len,
                    operation: Operation::Argsort {},
                });
                let threads: Execution = "t2".parse().unwrap();
                for variant in list_variants() {
                    let expected = matches!(variant.name, "tessera" | "rayon-stable");
                    assert_eq!(
                        variant.supports(&input, threads),
                        expected,
                        "{ty}/{distribution}/{}",
                        variant.name
                    );
                    let prepared = variant.prepare(&input, threads, 0);
                    assert_eq!(
                        prepared.is_some(),
                        expected,
                        "{ty}/{distribution}/{}",
                        variant.name
                    );
                    if let Some(mut kernel) = prepared {
                        assert_eq!(kernel.worker_threads(), 2);
                        kernel.restore(2);
                        kernel.run_batch();
                        kernel.check().unwrap_or_else(|error| {
                            panic!("{ty}/{distribution}/{}: {error}", variant.name)
                        });
                        assert_eq!(kernel.output_bytes(), len * size_of::<usize>());
                    }
                }
            }
        }
        let select = CaseInput::new(&InputCase {
            ty: "u64".into(),
            distribution: "random".into(),
            len,
            operation: Operation::SelectNth { index: 0 },
        });
        for variant in list_variants() {
            assert!(!variant.supports(&select, "t2".parse().unwrap()));
        }
    }
}
