// SPDX-License-Identifier: BSD-2-Clause

//! Versioned operation parameters, concrete identities and borrowed input facts.
//! Public calls own execution and allocation; this module resolves only workload
//! inputs and independent verification expectations, never engine decisions.

use serde::{Deserialize, Serialize};

/// Workload parameters; admitted routes define stable or arbitrary tie membership.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Operation {
    /// Historical complete sorting.
    Sort {},
    /// Stable sorted indices, including creation of the returned vector.
    Argsort {},
    /// Stable category grouping and offsets, including empty categories.
    PartitionBuckets {
        /// Classifications are encoded key bits modulo this count.
        bucket_count: usize,
    },
    /// One ranked key and outer memberships; tie identities are route-specific.
    Select {
        /// Zero-based target rank.
        index: usize,
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
            // The result is a Vec<usize> with one more offset than categories.
            Self::PartitionBuckets { bucket_count } => {
                *bucket_count > 0 && *bucket_count < isize::MAX as usize / size_of::<usize>()
            }
            Self::Select { index } => *index < len,
        };
        if valid {
            Ok(())
        } else {
            Err(format!(
                "operation parameters are invalid for {len} records: {self:?}"
            ))
        }
    }

    /// Extra identity components; full sorting retains historical IDs.
    pub(crate) fn suffix(&self) -> String {
        match self {
            Self::Sort {} => String::new(),
            Self::Argsort {} => "/argsort".into(),
            Self::PartitionBuckets { bucket_count } => format!("/partition-buckets/{bucket_count}"),
            Self::Select { index } => format!("/select/{index}"),
        }
    }

    /// Keeps the operation class for a small capability probe, not a timed case.
    pub(crate) fn probe(&self) -> Self {
        match self {
            Self::Select { .. } => Self::Select { index: 0 },
            other => other.clone(),
        }
    }
}
