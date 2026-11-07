// SPDX-License-Identifier: BSD-2-Clause

//! Output ordering, permutation and stability verification.

use super::variant::BenchmarkRecord;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::case::Row;
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
