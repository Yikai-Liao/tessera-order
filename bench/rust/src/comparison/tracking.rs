// SPDX-License-Identifier: BSD-2-Clause

//! Tessera scratch budgets for Criterion's memory-limited glidesort adapter.

use super::{record::CaseInput, variant::list_variants};
use crate::plan::{Plan, criterion_group, criterion_id};
use std::{collections::BTreeMap, fs, path::Path};

/// Measures Tessera's sort-only allocation peak on the exact smoke representation.
/// Run only in the separate instrumented memory executable, never in the timing process.
///
/// # Errors
/// Returns an error for invalid plans, failed verification or filesystem errors.
pub fn tracking_scratch(plan: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let plan = Plan::parse(&fs::read_to_string(plan)?, "smoke")?;
    let tessera = list_variants()
        .into_iter()
        .find(|v| v.name == "tessera")
        .ok_or("Tessera adapter unavailable")?;
    let selected = std::env::var("TESSERA_BENCH_IDS")?;
    let mut scratch = BTreeMap::new();
    for case in plan.inputs()? {
        let id = criterion_id!(
            criterion_group!("{}", case.ty; &case.distribution, case.len),
            "api"
        );
        if !selected.lines().any(|entry| entry == id) {
            continue;
        }
        let mut kernel = tessera
            .prepare(&CaseInput::smoke(&case), 0)
            .ok_or("Unsupported Tessera input")?;
        kernel.restore(1);
        crate::allocation::begin();
        kernel.run_batch();
        let bytes = crate::allocation::end();
        kernel.check()?;
        scratch.insert(id, bytes);
    }
    fs::write(output, serde_json::to_vec(&scratch)?)?;
    Ok(())
}
