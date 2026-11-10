// SPDX-License-Identifier: BSD-2-Clause

//! One case per process; stdin owns when preparation and timed samples may run.

use std::{
    hint::black_box,
    io::{self, BufRead, Write},
    time::{Duration, Instant},
};

/// Warms one case, then answers individual sample requests over stdin/stdout.
///
/// Input copies and output checks are outside the clock. Prepared batches use
/// the same element cap as Criterion; scratch allocation stays inside each sort.
///
/// # Panics
/// Panics on empty input, invalid requests, failed I/O or incorrect sort output.
pub fn serve<T: Copy>(
    id: &str,
    input: &[T],
    expected: &[T],
    same: impl Fn(&T, &T) -> bool,
    mut sort: impl FnMut(&mut [T]),
    settings: &crate::plan::RevisionSampling,
) {
    let batch = ((1 << 18) / input.len()).max(1);
    let mut buffers = vec![input.to_vec(); batch];
    let mut measure = || {
        for values in &mut buffers {
            values.copy_from_slice(input);
        }
        let start = Instant::now();
        for values in &mut buffers {
            sort(black_box(values.as_mut_slice()));
        }
        let elapsed = start.elapsed();
        assert!(
            buffers
                .iter()
                .all(|values| values.iter().zip(expected).all(|(a, b)| same(a, b))),
            "{id}: output differs from reference"
        );
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
                while start.elapsed() < Duration::from_millis(settings.warm_ms) {
                    measure();
                }
                warmed = true;
                serde_json::json!({"id": id, "samples": settings.samples})
            }
            "sample" if warmed && completed < settings.samples => {
                let (elapsed_ns, sorts) = sample(
                    Duration::from_micros(settings.window_ms * 1000 / settings.samples as u64),
                    &mut measure,
                );
                completed += 1;
                serde_json::json!({"elapsed_ns": elapsed_ns, "sorts": sorts})
            }
            "stop" if completed == settings.samples => {
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
