// SPDX-License-Identifier: BSD-2-Clause

//! Output ordering, permutation and stability verification.

use super::record::BenchmarkRecord;

/// Checks ordering, record preservation and stable tie order against the reference.
pub(super) fn check<T: BenchmarkRecord>(
    values: &[T],
    input: &[T],
    reference: &[T],
    stable: bool,
) -> Result<(), String> {
    if values.len() != input.len() {
        return Err("output length changed".into());
    }
    if stable || !T::HAS_PAYLOAD {
        if values.iter().zip(reference).all(|(a, b)| a.same(b)) {
            return Ok(());
        }
        return Err(
            "output differs from stable reference (order, permutation or stability)".into(),
        );
    }
    if values.windows(2).any(|v| v[0].order(&v[1]).is_gt()) {
        return Err("keys out of order".into());
    }
    let mut seen = vec![false; input.len()];
    for value in values {
        let index = value.index();
        if index >= input.len() || seen[index] || !value.same(&input[index]) {
            return Err("record lost, duplicated or modified".into());
        }
        seen[index] = true;
    }
    Ok(())
}

/// Checks required positions and the exact stable membership of every gap.
/// Canonicalizing only gap references keeps verification outside the measured
/// call without requiring the engine to attach identities to its records.
pub(super) fn refinement<T: BenchmarkRecord>(
    values: &[T],
    reference: &[T],
    ranges: &[std::ops::Range<usize>],
) -> Result<(), String> {
    if values.len() != reference.len() {
        return Err("output length changed".into());
    }
    let gap = |start: usize, end: usize| {
        let mut members: Vec<_> = values[start..end].iter().collect();
        members.sort_unstable_by(|a, b| a.order(b).then_with(|| a.index().cmp(&b.index())));
        members
            .iter()
            .zip(&reference[start..end])
            .all(|(a, b)| a.same(b))
    };
    let mut start = 0;
    for range in ranges {
        if !gap(start, range.start) {
            return Err("outer region has incorrect stable ranked members".into());
        }
        if !values[range.start..range.end]
            .iter()
            .zip(&reference[range.start..range.end])
            .all(|(a, b)| a.same(b))
        {
            return Err("required positions differ from stable ranked identities".into());
        }
        start = range.end;
    }
    if !gap(start, values.len()) {
        return Err("outer region has incorrect stable ranked members".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::case::Row;
    #[test]
    fn refinement_checks_stable_rank_identity_and_each_gap() {
        let reference: Vec<_> = (0..4).map(|row| Row { key: 0u64, row }).collect();
        // Unordered Top-k accepts any order inside either exact member set;
        // the checker must not accidentally require a selected pivot identity.
        let permuted = vec![reference[1], reference[0], reference[3], reference[2]];
        let cut = 2..2;
        assert!(refinement(&permuted, &reference, std::slice::from_ref(&cut)).is_ok());
        let ranges = [1..2, 3..4];
        assert!(refinement(&reference, &reference, &ranges).is_ok());
        let mut wrong = reference.clone();
        wrong.swap(0, 2); // Required record is right; gap membership is wrong.
        assert!(refinement(&wrong, &reference, &ranges).is_err());
        wrong = reference.clone();
        wrong.swap(1, 2); // Right key but wrong stable ranked identity.
        assert!(refinement(&wrong, &reference, &ranges).is_err());
    }

    #[test]
    fn records_preserve_payloads() {
        let input = vec![
            Row { key: 1u64, row: 0 },
            Row { key: 1, row: 1 },
            Row { key: 0, row: 2 },
        ];
        let reference = vec![input[2], input[0], input[1]];
        let permuted = vec![input[2], input[1], input[0]];
        assert!(check(&permuted, &input, &reference, false).is_ok());
        assert!(check(&permuted, &input, &reference, true).is_err());
        let duplicated = vec![input[2], input[1], input[1]];
        assert!(check(&duplicated, &input, &reference, false).is_err());
        let mut damaged = reference.clone();
        damaged[1].key = 2;
        assert!(check(&damaged, &input, &reference, false).is_err());
    }
}
