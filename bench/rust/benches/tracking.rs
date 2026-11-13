// SPDX-License-Identifier: BSD-2-Clause

//! Criterion measurements using the comparison registry and smoke inputs.
//! The sort, capability decisions, pool and checker belong to the shared adapters.

use criterion::{BatchSize, Criterion};
use std::{hint::black_box, rc::Rc, time::Duration};
use tessera_bench::{
    BenchmarkRecord, CaseInput, KernelVisitor, PreparedRecords, Variant, check, list_variants,
    plan::{
        InputCase, Plan, criterion_batch_iterations, criterion_group, criterion_id,
        revision_sampling,
    },
};

struct Measurement<'a> {
    criterion: &'a mut Criterion,
    case: &'a InputCase,
    mode: &'a str,
}
impl KernelVisitor for Measurement<'_> {
    type Output = ();
    fn visit<T: BenchmarkRecord, F: FnMut(&mut [T]) + 'static>(
        self,
        input: Rc<PreparedRecords<T>>,
        stable: bool,
        _threads: usize,
        mut sort: F,
    ) {
        let group_id = criterion_group!("{}", self.case.ty; &self.case.distribution, self.case.len);
        let mut values = input.original().clone();
        sort(&mut values);
        check(&values, input.original(), input.reference(), stable)
            .expect("verified algorithm output");
        let settings = revision_sampling(self.case.len);
        let mut group = self.criterion.benchmark_group(group_id);
        group
            .warm_up_time(Duration::from_millis(settings.warm_ms))
            .measurement_time(Duration::from_millis(settings.window_ms))
            .sample_size(settings.samples);
        let batch = criterion_batch_iterations(self.case.len);
        group.bench_function(self.mode, |b| {
            b.iter_batched_ref(
                || input.original().clone(),
                |values| sort(black_box(values.as_mut_slice())),
                BatchSize::NumIterations(batch),
            );
        });
        group.finish();
    }
}

fn main() {
    let path = std::env::var("TESSERA_BENCH_PLAN").expect("tracking requires a frozen plan");
    let plan = Plan::parse(&std::fs::read_to_string(path).unwrap(), "smoke").unwrap();
    let algorithm = std::env::var("TESSERA_BENCH_ALGORITHM").expect("tracking algorithm");
    let variant: Variant = list_variants()
        .into_iter()
        .find(|v| v.name == algorithm && v.adapter != "revision")
        .expect("available production algorithm");
    let mode = variant
        .threads
        .map_or_else(|| "api".into(), |threads| format!("par-{threads}"));
    let cases = if variant.threads.is_some() {
        plan.parallel_inputs()
    } else {
        plan.inputs()
    }
    .unwrap();
    let list = std::env::var_os("TESSERA_BENCH_LIST").is_some();
    let mut supported = Vec::new();
    let mut unsupported = Vec::new();
    for case in &cases {
        let id = criterion_id!(
            criterion_group!("{}", case.ty; &case.distribution, case.len),
            &mode
        );
        let probe = CaseInput::smoke(&InputCase {
            len: 32,
            ..case.clone()
        });
        if variant.supports(&probe) {
            supported.push((case, id));
        } else {
            unsupported.push(id);
        }
    }
    if list {
        println!(
            "{}",
            serde_json::json!({
                "supported": supported.iter().map(|(_, id)| id).collect::<Vec<_>>(), "unsupported": unsupported
            })
        );
        return;
    }
    assert!(!supported.is_empty(), "algorithm has no supported cases");
    let mut criterion = Criterion::default().without_plots().configure_from_args();
    let selected = std::env::var("TESSERA_BENCH_IDS").expect("explicit tracking inventory");
    let scratch: std::collections::BTreeMap<String, usize> = std::env::var("TESSERA_BENCH_SCRATCH")
        .ok()
        .map(|path| serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap())
        .unwrap_or_default();
    let test = std::env::args().any(|arg| arg == "--test");
    if let Some(threads) = variant.threads {
        assert!(
            test || std::thread::available_parallelism().unwrap().get() >= threads,
            "not enough allowed CPUs for the selected parallel algorithm"
        );
    }
    for (case, id) in supported {
        if !selected.lines().any(|entry| entry == id) {
            continue;
        }
        let bytes = if variant.name == "glidesort-sqrt" {
            *scratch
                .get(&id)
                .expect("Tessera allocation measurement required")
        } else {
            0
        };
        variant
            .with_kernel(
                &CaseInput::smoke(case),
                bytes,
                Measurement {
                    criterion: &mut criterion,
                    case,
                    mode: &mode,
                },
            )
            .expect("listed capability changed");
    }
    criterion.final_summary();
}
