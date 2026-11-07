// SPDX-License-Identifier: BSD-2-Clause

use serde::{Deserialize, Serialize};

pub(super) const SHAPES: [&str; 10] = [
    "random", "d256", "d16", "zipf1.1", "zipf2", "skew99", "sorted", "reverse", "sorted99", "runs",
];
pub(super) const TYPES: [&str; 7] = [
    "u32",
    "u64",
    "f64",
    "row_u64",
    "record128",
    "owned",
    "row_u32",
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct Case {
    pub ty: String,
    pub distribution: String,
    pub len: usize,
}
impl Case {
    pub(super) fn id(&self) -> String {
        format!("{}/{}/{}/s1", self.ty, self.distribution, self.len)
    }
    pub(super) fn group(&self) -> String {
        format!("{}/{}", self.ty, self.distribution)
    }
}
pub(super) fn cases(suite: &str, group: Option<&str>) -> Result<Vec<Case>, String> {
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
    fn release_has_unique_cases_and_groups_partition_it() {
        let cases = cases("release", None).unwrap();
        assert_eq!(cases.len(), 260);
        let ids: std::collections::BTreeSet<_> = cases.iter().map(Case::id).collect();
        assert_eq!(ids.len(), cases.len());
        let groups: std::collections::BTreeSet<_> = cases.iter().map(Case::group).collect();
        assert_eq!(groups.len(), 70);
        assert_eq!(
            groups
                .iter()
                .map(|g| super::cases("release", Some(g)).unwrap().len())
                .sum::<usize>(),
            cases.len()
        );
    }
}
