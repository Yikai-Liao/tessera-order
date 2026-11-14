// SPDX-License-Identifier: BSD-2-Clause

//! Versioned operation parameters, concrete identities and borrowed input facts.
//! Public calls own execution and allocation; this module resolves only workload
//! inputs and independent verification expectations, never engine decisions.

use serde::{Deserialize, Serialize};
use std::ops::Range;

/// Workload parameters; admitted routes define stable or arbitrary tie membership.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Operation {
    /// Historical complete sorting.
    Sort {},
    /// Stable sorted indices, including creation of the returned vector.
    Argsort {},
    /// Stable independent segments, with a final shorter segment when needed.
    Segments {
        /// Requested records per segment.
        segment_len: usize,
    },
    /// Smallest k keys, without internal ordering; tie membership is route-specific.
    TopK {
        /// Number of retained records.
        k: usize,
    },
    /// Sorted prefix; equal-key record identities are route-specific.
    Prefix {
        /// Number of required records.
        prefix_len: usize,
    },
    /// One ranked key and outer memberships; tie identities are route-specific.
    Select {
        /// Zero-based target rank.
        index: usize,
    },
    /// Sorted rank interval and outer memberships; tie identities are route-specific.
    RankRange {
        /// Inclusive first rank.
        start: usize,
        /// Exclusive last rank.
        end: usize,
    },
    /// Ranked keys; raw request order and duplicates are retained.
    SelectMany {
        /// Exact requested indices, normalized inside the public call.
        indices: Vec<usize>,
    },
}

impl Default for Operation {
    fn default() -> Self {
        Self::Sort {}
    }
}

impl Operation {
    /// Whether this is the historical complete-sorting workload.
    pub fn is_sort(&self) -> bool {
        matches!(self, Self::Sort {})
    }

    /// Validates all parameters against the concrete input before generation.
    pub fn validate(&self, len: usize) -> Result<(), String> {
        let valid = match self {
            Self::Sort {} | Self::Argsort {} => true,
            Self::Segments { segment_len } => *segment_len > 0,
            Self::TopK { k } => *k <= len,
            Self::Prefix { prefix_len } => *prefix_len <= len,
            Self::Select { index } => *index < len,
            Self::RankRange { start, end } => start <= end && *end <= len,
            Self::SelectMany { indices } => indices.iter().all(|&i| i < len),
        };
        if valid {
            Ok(())
        } else {
            Err(format!(
                "operation parameters are invalid for {len} records: {self:?}"
            ))
        }
    }

    /// Intervals for independent output checking, not engine requests.
    /// Top-k uses an empty interval to verify only the cut’s two memberships;
    /// unlike a selected rank it must not require a particular pivot record.
    pub(crate) fn required_ranges(&self) -> Option<Vec<Range<usize>>> {
        let ranges = match self {
            Self::TopK { k } => return Some(std::iter::once(*k..*k).collect()),
            Self::Prefix { prefix_len } => 0..*prefix_len,
            Self::Select { index } => *index..index + 1,
            Self::RankRange { start, end } => *start..*end,
            Self::SelectMany { indices } => {
                let mut indices = indices.clone();
                indices.sort_unstable();
                indices.dedup();
                let mut ranges: Vec<Range<usize>> = Vec::new();
                for index in indices {
                    if let Some(previous) = ranges.last_mut()
                        && previous.end == index
                    {
                        previous.end += 1;
                    } else {
                        ranges.push(index..index + 1);
                    }
                }
                return Some(ranges);
            }
            _ => return None,
        };
        Some(if ranges.is_empty() {
            Vec::new()
        } else {
            std::iter::once(ranges).collect()
        })
    }

    /// Resolves the reproducible segment specification once for a prepared kernel.
    pub(crate) fn offsets(&self, len: usize) -> Option<Vec<usize>> {
        if let Self::Segments { segment_len } = self {
            Some(
                (0..len)
                    .step_by(*segment_len)
                    .chain(std::iter::once(len))
                    .collect(),
            )
        } else {
            None
        }
    }

    /// Whether a partial call needs no key analysis or movement.
    pub(crate) fn is_noop(&self, len: usize) -> bool {
        matches!(self, Self::TopK { k } if *k == 0 || *k == len)
            || matches!(self, Self::Prefix { prefix_len: 0 })
            || matches!(self, Self::RankRange { start, end } if start == end)
            || matches!(self, Self::SelectMany { indices } if indices.is_empty())
    }

    /// Extra identity components; full sorting retains historical IDs.
    pub(crate) fn suffix(&self) -> String {
        match self {
            Self::Sort {} => String::new(),
            Self::Argsort {} => "/argsort".into(),
            Self::Segments { segment_len } => format!("/segments/{segment_len}"),
            Self::TopK { k } => format!("/top-k/{k}"),
            Self::Prefix { prefix_len } => format!("/prefix/{prefix_len}"),
            Self::Select { index } => format!("/select/{index}"),
            Self::RankRange { start, end } => format!("/rank-range/{start}-{end}"),
            Self::SelectMany { indices } => format!(
                "/select-many/{}",
                if indices.is_empty() {
                    "none".into()
                } else {
                    indices
                        .iter()
                        .map(usize::to_string)
                        .collect::<Vec<_>>()
                        .join(",")
                }
            ),
        }
    }

    /// Keeps the operation class for a small capability probe, not a timed case.
    pub(crate) fn probe(&self) -> Self {
        match self {
            Self::TopK { k } => Self::TopK { k: (*k).min(31) },
            Self::Prefix { prefix_len } => Self::Prefix {
                prefix_len: (*prefix_len).min(32),
            },
            Self::Select { .. } => Self::Select { index: 0 },
            Self::RankRange { .. } => Self::RankRange { start: 0, end: 1 },
            Self::SelectMany { .. } => Self::SelectMany { indices: vec![0] },
            other => other.clone(),
        }
    }
}
