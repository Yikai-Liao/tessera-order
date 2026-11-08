// SPDX-License-Identifier: BSD-2-Clause

//! Batch timing and paired block scheduling.

use super::{suite::Case, variant::Kernel};
use serde::Serialize;
use std::time::Instant;

/// Number of paired blocks in one case measurement.
pub(super) const BLOCKS: usize = 9;
/// One verified batch timing and its position within a case.
#[derive(Serialize)]
pub(super) struct Sample {
    pub kind: &'static str,
    pub case_id: String,
    pub group: String,
    pub variant: String,
    pub build: String,
    pub block: usize,
    pub slot: usize,
    pub batch: usize,
    pub elapsed_ns: u64,
    pub verified: bool,
}
/// Restores a batch, times only the sort calls, and verifies every output.
pub(super) fn measure_batch(k: &mut dyn Kernel, batch: usize) -> Result<u64, String> {
    k.restore(batch);
    let start = Instant::now();
    k.run_batch();
    let elapsed = start.elapsed().as_nanos() as u64;
    k.check()?;
    super::host::check_threads()?;
    Ok(elapsed.max(1))
}
/// Chooses a batch from a warm-up timing and the prepared-memory limit.
pub(super) fn calibrate(
    k: &mut dyn Kernel,
    min_sample_ns: u64,
    prepared_cap_bytes: usize,
) -> Result<usize, String> {
    let elapsed = measure_batch(k, 1)?;
    Ok(((min_sample_ns.div_ceil(elapsed)) as usize)
        .clamp(1, (prepared_cap_bytes / k.buffer_bytes()).max(1)))
}
/// Shuffles adapter slots reproducibly from the case identity and block.
pub(super) fn shuffle_variants(n: usize, block: usize, c: &Case) -> Vec<usize> {
    let mut seed = c.id().bytes().fold(block as u64 + 1, |a, b| {
        a.wrapping_mul(31).wrapping_add(u64::from(b))
    });
    let mut order: Vec<_> = (0..n).collect();
    for i in (1..n).rev() {
        seed = seed.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = seed;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^= z >> 31;
        order.swap(i, z as usize % (i + 1));
    }
    order
}

/// Publishes nine shuffled blocks with a separate Tessera A/A sample.
pub(super) fn measure_case(
    case: &Case,
    variants: &[super::variant::Variant],
    scratch_bytes: usize,
    min_sample_ns: u64,
    prepared_cap_bytes: usize,
    build: &str,
    mut publish: impl FnMut(Sample) -> super::Result<()>,
) -> super::Result<()> {
    let input = super::variant::CaseInput::new(case);
    let active: Vec<_> = variants
        .iter()
        .filter(|v| v.prepare(&input, scratch_bytes).is_some())
        .copied()
        .chain(std::iter::once(variants[0]))
        .collect();
    for block in 0..BLOCKS {
        for (slot, index) in shuffle_variants(active.len(), block, case)
            .into_iter()
            .enumerate()
        {
            let variant = active[index];
            let mut kernel = variant.prepare(&input, scratch_bytes).unwrap();
            let batch = calibrate(&mut *kernel, min_sample_ns, prepared_cap_bytes)?;
            let elapsed_ns = measure_batch(&mut *kernel, batch)
                .map_err(|e| format!("{} {}: {e}", case.id(), variant.name))?;
            publish(Sample {
                kind: "sample",
                case_id: case.id(),
                group: case.group(),
                variant: if index == active.len() - 1 {
                    "tessera-aa".into()
                } else {
                    variant.name.into()
                },
                build: build.into(),
                block,
                slot,
                batch,
                elapsed_ns,
                verified: true,
            })?;
        }
    }
    Ok(())
}
