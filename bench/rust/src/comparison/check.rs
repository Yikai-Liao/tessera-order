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
    permutation(values, input)
}

/// Payload indices identify original records independently of rank or tie order.
fn permutation<T: BenchmarkRecord>(values: &[T], input: &[T]) -> Result<(), String> {
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

/// Checks the selected rank and each side's membership, irrespective of order.
/// Stable selection compares exact identities; weak selection compares keys
/// and independently checks that every payload record survived intact.
pub(super) fn selection<T: BenchmarkRecord>(
    values: &[T],
    input: &[T],
    reference: &[T],
    index: usize,
    stable: bool,
) -> Result<(), String> {
    if values.len() != reference.len() || values.len() != input.len() {
        return Err("output length changed".into());
    }
    if !stable && T::HAS_PAYLOAD {
        permutation(values, input)?;
    }
    let matches = |a: &T, b: &T| {
        if stable {
            a.same(b)
        } else {
            a.order(b).is_eq()
        }
    };
    if !matches(&values[index], &reference[index]) {
        return Err("selected record differs from the requested rank contract".into());
    }
    let same_members = |values: &[T], reference: &[T]| {
        let mut members: Vec<_> = values.iter().collect();
        members.sort_unstable_by(|a, b| a.order(b).then_with(|| a.index().cmp(&b.index())));
        members.iter().zip(reference).all(|(a, b)| matches(a, b))
    };
    if !same_members(&values[..index], &reference[..index])
        || !same_members(&values[index + 1..], &reference[index + 1..])
    {
        return Err("outer region has incorrect ranked members".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::case::Row;
    #[test]
    fn stable_selection_checks_ranked_identity_and_unordered_membership() {
        let reference: Vec<_> = (0..5).map(|row| Row { key: 0u64, row }).collect();
        let mut values = reference.clone();
        values.swap(0, 1);
        values.swap(3, 4);
        assert!(selection(&values, &reference, &reference, 2, true).is_ok());

        let mut wrong = values.clone();
        wrong.swap(0, 3); // The pivot is correct, but records crossed it.
        assert!(selection(&wrong, &reference, &reference, 2, true).is_err());
        values.swap(1, 2); // The pivot has the right key but wrong identity.
        assert!(selection(&values, &reference, &reference, 2, true).is_err());
    }

    #[test]
    fn weak_selection_allows_ties_but_rejects_wrong_keys_and_damaged_payloads() {
        let input: Vec<_> = (0..4).map(|row| Row { key: 0u64, row }).collect();
        let mut values = input.clone();
        values.swap(0, 1);
        assert!(selection(&values, &input, &input, 1, false).is_ok());
        values[0] = values[1];
        assert!(selection(&values, &input, &input, 1, false).is_err());
        values = input.clone();
        values[0].key = 1;
        assert!(selection(&values, &input, &input, 1, false).is_err());
        let input: Vec<_> = (0..4)
            .map(|row| Row {
                key: row as u64,
                row,
            })
            .collect();
        values = input.clone();
        values.swap(0, 3);
        assert!(selection(&values, &input, &input, 1, false).is_err());
        values = input.clone();
        values.swap(1, 3);
        assert!(selection(&values, &input, &input, 1, false).is_err());
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
