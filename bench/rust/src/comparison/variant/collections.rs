// SPDX-License-Identifier: BSD-2-Clause

mod adapters;
mod input;

use super::Kernel;
use crate::plan::{InputCase, Operation};
use adapters::*;
use input::{Action, Expected, Fixture, IntKey, Value};
use std::{hint::black_box, mem, rc::Rc};

/// Collection inputs retain their concrete key and value types.
pub struct CollectionInput {
    pub(in crate::comparison) case: InputCase,
    data: Data,
}

enum Data {
    Map32(Rc<Fixture<u32, u32>>),
    Map64(Rc<Fixture<u64, u64>>),
    Wide(Rc<Fixture<u64, [u8; 32]>>),
    Set(Rc<Fixture<u64, ()>>),
}

impl CollectionInput {
    pub(in crate::comparison) fn new(case: &InputCase) -> Self {
        let data = match case.ty.as_str() {
            "map_u32_u32" => Data::Map32(Rc::new(Fixture::new(case))),
            "map_u64_u64" => Data::Map64(Rc::new(Fixture::new(case))),
            "map_u64_payload32" => Data::Wide(Rc::new(Fixture::new(case))),
            "set_u64" => Data::Set(Rc::new(Fixture::new(case))),
            _ => unreachable!(),
        };
        Self {
            case: case.clone(),
            data,
        }
    }
}

pub(super) fn prepare(name: &str, input: &CollectionInput) -> Option<Box<dyn Kernel>> {
    let operation = &input.case.operation;
    match &input.data {
        Data::Map32(data) => map(name, operation, data),
        Data::Map64(data) => map(name, operation, data),
        Data::Wide(data) => map(name, operation, data),
        Data::Set(data) => match name {
            "std-btree" => kernel::<_, _, StdSet>(data, operation),
            "indexset" => kernel::<_, _, IndexedSet>(data, operation),
            "blart" => kernel::<_, _, ArtSet>(data, operation),
            _ => map(name, operation, data),
        },
    }
}

fn map<K: IntKey, V: Value>(
    name: &str,
    operation: &Operation,
    data: &Rc<Fixture<K, V>>,
) -> Option<Box<dyn Kernel>> {
    match name {
        "std-btree" => kernel::<_, _, StdMap<K, V>>(data, operation),
        "sorted-vec" => kernel::<_, _, SortedVec<K, V>>(data, operation),
        "blart" => kernel::<_, _, ArtMap<K, V>>(data, operation),
        "indexset" => kernel::<_, _, IndexedMap<K, V>>(data, operation),
        "index-db" => kernel::<_, _, DbMap<K, V>>(data, operation),
        _ => None,
    }
}

fn kernel<K: IntKey, V: Value, C: Collection<K, V>>(
    data: &Rc<Fixture<K, V>>,
    operation: &Operation,
) -> Option<Box<dyn Kernel>> {
    let route = C::route(operation)?;
    Some(Box::new(CollectionKernel::<K, V, C> {
        data: Rc::clone(data),
        route,
        calls: Vec::new(),
    }))
}

struct Call<K, V, C> {
    collection: Option<C>,
    input: Vec<(K, V)>,
    results: Vec<Expected<K, V>>,
    visited: Vec<(K, V)>,
}

struct CollectionKernel<K, V, C> {
    data: Rc<Fixture<K, V>>,
    route: &'static str,
    calls: Vec<Call<K, V, C>>,
}

impl<K: IntKey, V: Value, C: Collection<K, V>> Kernel for CollectionKernel<K, V, C> {
    fn restore(&mut self, batch: usize) {
        let data = &self.data;
        self.calls.clear();
        self.calls.extend((0..batch).map(|_| {
            let (input, collection) = if data.builds() {
                (data.original.clone(), None)
            } else {
                let collection = C::build(data.original.clone());
                collection.visit(None, |record| {
                    black_box(record);
                });
                (Vec::new(), Some(collection))
            };
            Call {
                input,
                collection,
                results: Vec::with_capacity(data.result_count()),
                visited: Vec::with_capacity(data.visit_count()),
            }
        }));
    }

    fn run_batch(&mut self) {
        let data = &self.data;
        for call in &mut self.calls {
            if data.builds() {
                let input = mem::take(&mut call.input);
                call.collection = Some(if data.single_inserts() {
                    let mut collection = C::empty();
                    for (key, value) in input {
                        call.results
                            .push(Expected::Value(collection.insert(key, value)));
                    }
                    collection
                } else {
                    C::build(input)
                });
            }
            let collection = call.collection.as_mut().unwrap();
            for action in &data.actions {
                let result = match *black_box(action) {
                    Action::Get(key) => Expected::Value(collection.get(key)),
                    Action::Insert(key, value) => Expected::Value(collection.insert(key, value)),
                    Action::Remove(key) => Expected::Value(collection.remove(key)),
                    Action::Range(lo, hi) => {
                        collection.visit(Some((lo, hi)), |entry| call.visited.push(entry));
                        continue;
                    }
                    Action::Iterate => {
                        collection.visit(None, |entry| call.visited.push(entry));
                        continue;
                    }
                    Action::First => Expected::Entry(collection.endpoint(false)),
                    Action::Last => Expected::Entry(collection.endpoint(true)),
                    Action::Rank(key) => Expected::Rank(collection.rank(key)),
                    Action::Select(index) => Expected::Entry(collection.select(index)),
                };
                call.results.push(black_box(result));
            }
        }
    }

    fn check(&self) -> Result<(), String> {
        for call in &self.calls {
            self.data.check_outputs(&call.results, &call.visited)?;
            if let Some(expected) = self.data.expected_final() {
                super::super::check::collection_contents(expected, |visit| {
                    call.collection.as_ref().unwrap().visit(None, visit);
                })?;
            }
        }
        Ok(())
    }

    fn retained_heap(&mut self) -> Option<(usize, usize)> {
        let data = &self.data;
        if !data.operation.retained_heap() {
            return None;
        }
        let churn = data.churn();
        let mut results = vec![(None, None); churn.len()];
        crate::allocation::begin();
        let mut collection = C::build(data.original.clone());
        if !churn.is_empty() {
            collection.churn(&churn, &mut results);
        }
        let retained = crate::allocation::live_bytes();
        let peak = crate::allocation::end();
        data.check_retained(&collection, &churn, &results)
            .unwrap_or_else(|error| panic!("{}: {error}", self.route));
        Some((retained, peak))
    }

    fn buffer_bytes(&self) -> usize {
        let data = &self.data;
        let entries = data.original.len() * (mem::size_of::<(K, V)>() + 128);
        entries
            + data.result_count() * mem::size_of::<Expected<K, V>>()
            + data.visit_count() * mem::size_of::<(K, V)>()
    }

    fn worker_threads(&self) -> usize {
        0
    }
    fn churn_route(&self) -> Option<&'static str> {
        matches!(self.data.operation, Operation::MapMixed { .. }).then_some(C::CHURN)
    }

    fn description(&self) -> &'static str {
        self.route
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        comparison::CaseInput,
        plan::{Execution, Plan},
    };

    #[test]
    fn smoke_adapters_verify_and_restore() {
        let plan = Plan::parse(include_str!("../../../../plans/collections-smoke.toml")).unwrap();
        for (case, _) in plan.cases().unwrap() {
            let input = CaseInput::new(&case);
            for variant in crate::comparison::list_variants()
                .into_iter()
                .filter(|variant| variant.adapter == "collections")
            {
                let Some(mut kernel) = variant.prepare(&input, Execution::Serial, 0) else {
                    assert!(matches!(
                        case.operation,
                        Operation::MapRank { .. } | Operation::MapSelect { .. }
                    ));
                    continue;
                };
                for batch in [1, 2] {
                    kernel.restore(batch);
                    kernel.run_batch();
                    kernel
                        .check()
                        .unwrap_or_else(|error| panic!("{} {}: {error}", case.id(), variant.name));
                }
            }
        }
    }

    #[test]
    fn rejects_wrong_adapter_results() {
        let plan = Plan::parse(include_str!("../../../../plans/collections-smoke.toml")).unwrap();
        for (case, _) in plan
            .cases()
            .unwrap()
            .into_iter()
            .filter(|(case, _)| case.ty == "map_u64_u64")
        {
            let data = Rc::new(Fixture::<u64, u64>::new(&case));
            let mut kernel = CollectionKernel::<_, _, SortedVec<u64, u64>> {
                data,
                route: "mutation",
                calls: Vec::new(),
            };
            kernel.restore(1);
            kernel.run_batch();
            kernel.check().unwrap();
            let call = &mut kernel.calls[0];
            let expected = match case.operation {
                Operation::MapBuild {} => {
                    let (key, value) = call.collection.as_ref().unwrap().endpoint(false).unwrap();
                    call.collection.as_mut().unwrap().insert(key, value ^ 1);
                    "incorrect collection entry"
                }
                Operation::MapRange { .. } | Operation::MapIterate {} => {
                    call.visited[0].1 ^= 1;
                    "ordered visits at 0"
                }
                _ => {
                    call.results[0] = match call.results[0] {
                        Expected::Value(None) => Expected::Value(Some(0)),
                        Expected::Value(Some(_)) => Expected::Value(None),
                        Expected::Entry(_) => Expected::Entry(None),
                        Expected::Rank(rank) => Expected::Rank(rank + 1),
                    };
                    "returned results at 0"
                }
            };
            assert!(
                kernel.check().unwrap_err().contains(expected),
                "{}",
                case.id()
            );
        }
    }

    #[test]
    fn sorted_vec_full_churn_matches_the_checker() {
        let case = InputCase {
            ty: "map_u64_u64".into(),
            distribution: "dense".into(),
            len: 4097,
            operation: Operation::MapMixed {
                write_pct: 10,
                hot_pct: 0,
            },
        };
        let data = Fixture::<u64, u64>::new(&case);
        let trace = data.churn();
        assert_eq!(trace.len(), case.len);
        assert!(trace.iter().any(|&(old, _, _)| old >= case.len as u64));

        let mut collection = SortedVec::build(data.original.clone());
        let mut results = vec![(None, None); trace.len()];
        collection.churn(&trace, &mut results);
        data.check_retained(&collection, &trace, &results).unwrap();

        let mut broken = SortedVec::build(data.original.clone());
        let (last_pair, prefix) = trace.split_last().unwrap();
        let (last_result, prefix_results) = results.split_last_mut().unwrap();
        broken.churn(prefix, prefix_results);
        *last_result = (broken.remove(last_pair.0), None);
        let error = data.check_retained(&broken, &trace, &results).unwrap_err();
        assert!(
            error.starts_with("incorrect collection entry at index")
                || error.ends_with(", got 4096"),
            "{error}"
        );
    }

    #[test]
    fn collection_controls_cannot_use_tessera() {
        let plan = Plan::parse(include_str!("../../../../plans/collections-smoke.toml")).unwrap();
        let build: Vec<_> = plan
            .cases()
            .unwrap()
            .into_iter()
            .filter(|(case, _)| matches!(case.operation, Operation::MapBuild {}))
            .collect();
        let variants = crate::comparison::list_variants();
        let wrong: Vec<_> = variants
            .iter()
            .copied()
            .filter(|v| v.name == "tessera" || v.name == "sorted-vec")
            .collect();
        let error = crate::comparison::run::validate_controls(&build, &wrong).unwrap_err();
        assert_eq!(error.to_string(), "missing required control: std-btree");
        crate::comparison::run::validate_controls(&build, &variants).unwrap();
    }
}
