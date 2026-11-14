// SPDX-License-Identifier: BSD-2-Clause

//! Batch timing and paired block scheduling.

use super::{InputCase, variant::Kernel};
use serde::Serialize;

/// Fixed block budget and limits for calibrating each verified sample.
pub(super) struct Sampling {
    pub min_sample_ns: u64,
    pub prepared_cap_bytes: usize,
    pub blocks: usize,
}
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
/// Chooses a batch from a warm-up timing and the prepared-memory limit.
pub(super) fn calibrate(
    k: &mut dyn Kernel,
    min_sample_ns: u64,
    prepared_cap_bytes: usize,
) -> Result<usize, String> {
    let elapsed = k.measure_batch(1)?.as_nanos().max(1) as u64;
    Ok(((min_sample_ns.div_ceil(elapsed)) as usize)
        .clamp(1, (prepared_cap_bytes / k.buffer_bytes()).max(1)))
}
/// Shuffles adapter slots reproducibly from the case identity and block.
pub(super) fn shuffle_variants(n: usize, block: usize, c: &InputCase) -> Vec<usize> {
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

/// Rejects unsupported required controls before journals or memory measurement.
/// Capability probes retain the operation but never generate the full input.
pub(super) fn validate_controls(
    cases: &[InputCase],
    variants: &[super::variant::Variant],
) -> super::Result<()> {
    for case in cases {
        let input = super::record::CaseInput::new(&case.probe());
        for name in ["tessera", "std-stable"] {
            let control = variants
                .iter()
                .find(|v| v.name == name)
                .ok_or_else(|| format!("missing required control: {name}"))?;
            if !control.supports(&input) {
                return Err(
                    format!("required control {name} does not support {}", case.id()).into(),
                );
            }
        }
    }
    Ok(())
}

/// Publishes the requested shuffled blocks with a separate Tessera A/A sample.
pub(super) fn measure_case(
    case: &InputCase,
    variants: &[super::variant::Variant],
    scratch_bytes: usize,
    sampling: &Sampling,
    build: &str,
    mut publish: impl FnMut(Sample) -> super::Result<()>,
) -> super::Result<()> {
    let input = super::record::CaseInput::new(case);
    let mut active: Vec<_> = variants
        .iter()
        .filter(|v| v.supports(&input))
        .copied()
        .collect();
    let control = active
        .iter()
        .find(|v| v.name == "tessera")
        .copied()
        .ok_or_else(|| format!("required control tessera does not support {}", case.id()))?;
    active.push(control);
    for block in 0..sampling.blocks {
        for (slot, index) in shuffle_variants(active.len(), block, case)
            .into_iter()
            .enumerate()
        {
            let variant = active[index];
            let mut kernel = variant.prepare(&input, scratch_bytes).unwrap();
            let batch = calibrate(
                &mut *kernel,
                sampling.min_sample_ns,
                sampling.prepared_cap_bytes,
            )?;
            let elapsed_ns = kernel
                .measure_batch(batch)
                .map_err(|e| format!("{} {}: {e}", case.id(), variant.name))?
                .as_nanos()
                .max(1) as u64;
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
