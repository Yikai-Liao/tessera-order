// SPDX-License-Identifier: BSD-2-Clause

//! Exact comparison cases and resumable groups.

use serde::{Deserialize, Serialize};

/// Input distributions in the fixed release presentation order.
pub(super) const SHAPES: [&str; 10] = [
    "random", "d256", "d16", "zipf1.1", "zipf2", "skew99", "sorted", "reverse", "sorted99", "runs",
];
/// Record representations covered by the release comparison.
pub(super) const TYPES: [&str; 7] = [
    "u32",
    "u64",
    "f64",
    "row_u64",
    "record128",
    "owned",
    "row_u32",
];

/// A seed-one input's type, distribution and number of records.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct Case {
    pub ty: String,
    pub distribution: String,
    pub len: usize,
}
impl Case {
    /// Formats the stable identifier used in raw samples and reports.
    pub(super) fn id(&self) -> String {
        format!("{}/{}/{}/s1", self.ty, self.distribution, self.len)
    }
    /// Names the resumable group containing this case's lengths.
    pub(super) fn group(&self) -> String {
        format!("{}/{}", self.ty, self.distribution)
    }
}
/// Expands a suite into exact cases, optionally selecting one whole group.
pub(super) fn plan_cases(suite: &str, group: Option<&str>) -> Result<Vec<Case>, String> {
    let lengths: &[usize] = match suite {
        "smoke" => &[1000],
        "release" => &[1000, 65_536, 1_000_000, 10_000_000],
        _ => return Err(format!("unknown suite {suite}; expected smoke or release")),
    };
    let mut cases = Vec::new();
    for ty in TYPES {
        for distribution in SHAPES {
            for &len in lengths {
                let c = Case {
                    ty: ty.into(),
                    distribution: distribution.into(),
                    len,
                };
                if len > 1_000_000 && matches!(ty, "record128" | "owned") {
                    continue;
                }
                if group.is_none_or(|g| g == c.group()) {
                    cases.push(c);
                }
            }
        }
    }
    if cases.is_empty() {
        return Err(format!("no cases in group {}", group.unwrap_or_default()));
    }
    Ok(cases)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn suite_groups_partition_cases() {
        let cases = plan_cases("release", None).unwrap();
        assert_eq!(cases.len(), 260);
        let ids: std::collections::BTreeSet<_> = cases.iter().map(Case::id).collect();
        assert_eq!(ids.len(), cases.len());
        let groups: std::collections::BTreeSet<_> = cases.iter().map(Case::group).collect();
        assert_eq!(groups.len(), 70);
        assert_eq!(
            groups
                .iter()
                .map(|g| super::plan_cases("release", Some(g)).unwrap().len())
                .sum::<usize>(),
            cases.len()
        );
    }
}
