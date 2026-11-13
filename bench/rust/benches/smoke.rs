// SPDX-License-Identifier: BSD-2-Clause

//! Criterion and paired scheduling over the shared plan, inputs and batch kernels.

use criterion::Criterion;
#[cfg(feature = "layout-study")]
use std::hint::black_box;
use std::{collections::BTreeMap, time::Duration};
use tessera_bench::{
    CaseInput, Kernel, list_variants,
    plan::{
        InputCase, Plan, criterion_batch_iterations, criterion_group, criterion_id,
        revision_sampling,
    },
};

/// All protocols consume the same restorable kernel; only their scheduling differs.
fn measure(criterion: &mut Criterion, case: &InputCase, mode: &str, kernel: &mut dyn Kernel) {
    let args = std::env::args().collect::<Vec<_>>();
    assert!(
        !args.iter().any(|arg| arg == "--bench")
            || args.iter().any(|arg| arg == "--test")
            || std::thread::available_parallelism().unwrap().get() >= kernel.worker_threads(),
        "insufficient allowed CPUs"
    );
    kernel.restore(1);
    kernel.run_batch();
    kernel.check().expect("verified output");
    let group_id = criterion_group!("{}", case.ty; &case.distribution, case.len);
    let id = criterion_id!(&group_id, mode);
    #[cfg(feature = "layout-study")]
    if let Ok(count) = std::env::var("TESSERA_BENCH_FIXED_SORTS") {
        let count: usize = count.parse().expect("integer fixed sort count");
        assert!(
            (1..=1024).contains(&count),
            "fixed sort count must be 1..1024"
        );
        kernel.restore(count);
        tessera_counted_sorts(&mut || kernel.run_batch());
        kernel.check().expect("verified output");
        println!("{}", serde_json::json!({ "id": id, "sorts": count }));
        return;
    }
    let settings = revision_sampling(case.len);
    if std::env::var_os("TESSERA_BENCH_PAIRED").is_some() {
        tessera_bench::sampling::serve(&id, kernel, case.len, settings);
        return;
    }
    let mut group = criterion.benchmark_group(group_id);
    group
        .warm_up_time(Duration::from_millis(settings.warm_ms))
        .measurement_time(Duration::from_millis(settings.window_ms))
        .sample_size(settings.samples);
    group.bench_function(mode, |b| {
        b.iter_custom(|iterations| {
            let mut remaining = iterations;
            let mut elapsed = Duration::ZERO;
            while remaining > 0 {
                let batch = remaining.min(criterion_batch_iterations(case.len));
                elapsed += kernel
                    .measure_batch(batch as usize)
                    .expect("verified batch");
                remaining -= batch;
            }
            elapsed
        })
    });
    group.finish();
    kernel.check().expect("verified output");
}

#[cfg(feature = "layout-study")]
#[inline(never)]
#[unsafe(no_mangle)]
fn tessera_counted_sorts(run: &mut dyn FnMut()) {
    run();
    black_box(());
}

fn main() {
    if std::env::var_os("TESSERA_BENCH_PAIRED").is_some() {
        let mut request = String::new();
        std::io::stdin()
            .read_line(&mut request)
            .expect("read start request");
        assert_eq!(request.trim(), "start");
    }
    let text = std::env::var("TESSERA_BENCH_PLAN").map_or_else(
        |_| include_str!("../../plans/continuous.toml").to_owned(),
        |path| std::fs::read_to_string(path).expect("read benchmark plan"),
    );
    let plan = Plan::parse(&text).expect("valid benchmark plan");
    assert!(
        plan.operation().is_sort(),
        "Criterion/tracking currently supports full sorting only; use tools compare algorithms for internal refinement"
    );
    let available = list_variants();
    assert!(
        plan.algorithms().iter().all(|id| available
            .iter()
            .any(|v| v.name == id && v.adapter != "revision")),
        "unavailable algorithm"
    );
    let variants: Vec<_> = available
        .into_iter()
        .filter(|v| plan.algorithms().iter().any(|id| id == v.name))
        .collect();
    assert_eq!(
        variants
            .iter()
            .map(|v| v.threads)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        variants.len(),
        "Criterion requires one algorithm per API mode"
    );
    // An adapter's capability decides coverage; unsupported cases are simply not listed.
    let mut supported = Vec::new();
    for variant in variants {
        let mode = variant
            .threads
            .map_or_else(|| "api".into(), |n| format!("par-{n}"));
        let mut cases = if variant.threads.is_some() {
            plan.parallel_inputs().unwrap()
        } else {
            Vec::new()
        };
        if cases.is_empty() {
            cases = plan.inputs().unwrap();
        }
        for case in cases {
            let id = criterion_id!(
                criterion_group!("{}", case.ty; &case.distribution, case.len),
                &mode
            );
            if variant.supports(&CaseInput::new(&InputCase {
                len: 32,
                ..case.clone()
            })) {
                supported.push((variant, case, mode.clone(), id));
            }
        }
    }
    if std::env::var_os("TESSERA_BENCH_LIST").is_some() {
        for (_, _, _, id) in &supported {
            println!("{id}");
        }
        return;
    }
    let selected = std::env::var("TESSERA_BENCH_IDS").ok();
    if let Some(ids) = &selected {
        assert!(
            !ids.is_empty()
                && ids
                    .lines()
                    .all(|id| supported.iter().any(|(_, _, _, expected)| expected == id)),
            "unknown or unsupported case selection"
        );
    }
    let scratch: BTreeMap<String, usize> =
        if plan.algorithms().iter().any(|id| id == "glidesort-sqrt") {
            tessera_bench::read_scratch(std::path::Path::new(
                &std::env::var("TESSERA_BENCH_SCRATCH").expect("scratch measurement required"),
            ))
            .expect("compatible memory evidence")
        } else {
            BTreeMap::new()
        };
    let mut criterion = Criterion::default().without_plots().configure_from_args();
    for (variant, case, mode, id) in supported {
        if selected
            .as_ref()
            .is_some_and(|ids| !ids.lines().any(|selected| selected == id))
        {
            continue;
        }
        let bytes = if variant.name == "glidesort-sqrt" {
            *scratch.get(&case.id()).expect("missing exact-case scratch")
        } else {
            0
        };
        let mut kernel = variant
            .prepare(&CaseInput::new(&case), bytes)
            .expect("listed capability changed");
        measure(&mut criterion, &case, &mode, kernel.as_mut());
    }
    criterion.final_summary();
}
