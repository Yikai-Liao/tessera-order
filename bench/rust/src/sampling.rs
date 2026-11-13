// SPDX-License-Identifier: BSD-2-Clause

//! One sample per process; stdin owns when preparation and timing may run.

use std::{
    io::{self, BufRead, Write},
    time::{Duration, Instant},
};

/// Prepares one case and answers one warm/sample pair over stdin/stdout.
///
/// Input copies and output checks are outside the clock. Prepared batches use
/// the same element cap as Criterion; scratch allocation stays inside each sort.
///
/// # Panics
/// Panics on empty input, invalid requests, failed I/O or incorrect sort output.
pub fn serve(
    id: &str,
    kernel: &mut dyn crate::Kernel,
    records_len: usize,
    settings: &crate::plan::RevisionSampling,
) {
    let batch = crate::plan::criterion_batch_iterations(records_len) as usize;
    let mut measure = || {
        let elapsed = kernel
            .measure_batch(batch)
            .unwrap_or_else(|error| panic!("{id}: {error}"));
        (elapsed.as_nanos().max(1) as u64, batch as u64)
    };
    println!("{{\"id\":{}}}", serde_json::to_string(id).unwrap());
    io::stdout().flush().expect("flush ready response");
    let mut warmed = false;
    let mut completed = 0;
    for line in io::stdin().lock().lines() {
        let request = line.expect("read paired request");
        let response = match request.as_str() {
            "warm" if !warmed => {
                let start = Instant::now();
                while start.elapsed()
                    < Duration::from_micros(settings.warm_ms * 1000 / settings.samples as u64)
                {
                    measure();
                }
                warmed = true;
                serde_json::json!({"id": id, "samples": settings.samples})
            }
            "sample" if warmed && completed == 0 => {
                let (elapsed_ns, sorts) = sample(
                    Duration::from_micros(settings.window_ms * 1000 / settings.samples as u64),
                    &mut measure,
                );
                completed += 1;
                serde_json::json!({"elapsed_ns": elapsed_ns, "sorts": sorts})
            }
            "stop" if completed == 1 => {
                println!("{{}}");
                return;
            }
            _ => panic!("invalid paired request or sequence: {request}"),
        };
        println!("{response}");
        io::stdout().flush().expect("flush paired response");
    }
    panic!("incomplete paired session");
}

/// Always records a batch, even when preemption has already exhausted the budget.
fn sample(budget: Duration, mut measure: impl FnMut() -> (u64, u64)) -> (u64, u64) {
    let start = Instant::now();
    let (mut elapsed_ns, mut sorts) = (0, 0);
    loop {
        let (elapsed, count) = measure();
        elapsed_ns += elapsed;
        sorts += count;
        // Wall budget includes restoration and verification; reported time does not.
        if start.elapsed() >= budget {
            return (elapsed_ns, sorts);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn expired_budget_still_measures_one_batch() {
        assert_eq!(
            super::sample(std::time::Duration::ZERO, || (123, 4)),
            (123, 4)
        );
    }
}
