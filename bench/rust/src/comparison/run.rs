// SPDX-License-Identifier: BSD-2-Clause

//! Batch timing and paired block scheduling.

use super::{InputCase, variant::Kernel, variant::Variant};
use crate::plan::{Column, Execution};
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
    /// Column ID, or `tessera-aa` for the repeated serial Tessera control.
    pub column: String,
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

/// Formats the column an adapter measures at one execution.
pub(super) fn column_id(variant: Variant, execution: Execution) -> String {
    Column {
        implementation: variant.name.into(),
        execution,
    }
    .to_string()
}

/// Rejects unsupported required controls before journals or memory measurement.
/// Every case pairs its columns with serial Tessera and its operation's reference controls.
/// Capability probes retain the operation but never generate the full input.
pub(super) fn validate_controls(
    cases: &[(InputCase, Vec<Execution>)],
    variants: &[Variant],
) -> super::Result<()> {
    for (case, executions) in cases {
        if !executions.contains(&Execution::Serial) {
            return Err(format!(
                "{} lacks the serial execution that its Tessera and reference controls need",
                case.id()
            )
            .into());
        }
        let input = super::record::CaseInput::new(&case.probe());
        for name in [
            crate::plan::comparison_control(&case.operation),
            crate::plan::operation_reference(&case.operation),
        ] {
            let control = variants
                .iter()
                .find(|v| v.name == name)
                .ok_or_else(|| format!("missing required control: {name}"))?;
            if !control.supports(&input, Execution::Serial) {
                return Err(format!(
                    "required control {name}/serial does not support {}",
                    case.id()
                )
                .into());
            }
        }
    }
    Ok(())
}

/// Publishes the requested shuffled blocks with a separate serial Tessera A/A sample.
pub(super) fn measure_case(
    case: &InputCase,
    columns: &[(Variant, Execution)],
    scratch_bytes: usize,
    sampling: &Sampling,
    build: &str,
    mut publish: impl FnMut(Sample) -> super::Result<()>,
) -> super::Result<()> {
    let input = super::record::CaseInput::new(case);
    let mut active = columns.to_vec();
    let control_name = crate::plan::comparison_control(&case.operation);
    let control = active
        .iter()
        .find(|(v, e)| v.name == control_name && *e == Execution::Serial)
        .copied()
        .ok_or_else(|| {
            format!(
                "required control {control_name}/serial does not support {}",
                case.id()
            )
        })?;
    active.push(control);
    for block in 0..sampling.blocks {
        for (slot, index) in shuffle_variants(active.len(), block, case)
            .into_iter()
            .enumerate()
        {
            let (variant, execution) = active[index];
            let mut kernel = variant.prepare(&input, execution, scratch_bytes).unwrap();
            let batch = calibrate(
                &mut *kernel,
                sampling.min_sample_ns,
                sampling.prepared_cap_bytes,
            )?;
            let elapsed_ns = kernel
                .measure_batch(batch)
                .map_err(|e| format!("{} {}: {e}", case.id(), column_id(variant, execution)))?
                .as_nanos()
                .max(1) as u64;
            publish(Sample {
                kind: "sample",
                case_id: case.id(),
                group: case.group(),
                column: if index == active.len() - 1 {
                    if case.operation.is_collection() {
                        "reference-aa"
                    } else {
                        "tessera-aa"
                    }
                    .into()
                } else {
                    column_id(variant, execution)
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
