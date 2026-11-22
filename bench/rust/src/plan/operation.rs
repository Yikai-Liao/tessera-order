// SPDX-License-Identifier: BSD-2-Clause

//! Operation parameters, concrete identities and borrowed input facts.
//! Public calls own execution and allocation; this module resolves only operation
//! inputs and independent verification expectations, never engine decisions.

use serde::{Deserialize, Serialize};

/// Operation parameters; admitted routes define stable or arbitrary tie membership.
///
/// Variants and their parameter fields follow the catalog `operations` table,
/// which owns identity order; `catalog_names_every_operation` checks the match.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Operation {
    /// Complete sorting.
    #[serde(rename = "sort")]
    Sort {},
    /// Stable sorted indices, including creation of the returned vector.
    #[serde(rename = "argsort")]
    Argsort {},
    /// Stable category grouping and offsets, including empty categories.
    #[serde(rename = "partition-buckets")]
    PartitionBuckets {
        /// Classifications are encoded key bits modulo this count.
        bucket_count: usize,
    },
    /// One ranked key and outer memberships; tie identities are route-specific.
    #[serde(rename = "select_nth")]
    SelectNth {
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
    /// Whether this is complete sorting.
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
            Self::SelectNth { index } => *index < len,
        };
        if valid {
            Ok(())
        } else {
            Err(format!(
                "operation parameters are invalid for {len} records: {self:?}"
            ))
        }
    }

    /// Keeps the operation class for a small capability probe, not a timed case.
    pub(crate) fn probe(&self) -> Self {
        match self {
            Self::SelectNth { .. } => Self::SelectNth { index: 0 },
            other => other.clone(),
        }
    }
}
