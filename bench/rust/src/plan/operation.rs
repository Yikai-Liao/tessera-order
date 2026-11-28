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
    /// Build from unsorted duplicate-bearing input.
    #[serde(rename = "map-build")]
    MapBuild {},
    /// Build with randomly ordered single inserts.
    #[serde(rename = "map-build-insert-random")]
    MapBuildInsertRandom {},
    /// Build with ascending single inserts.
    #[serde(rename = "map-build-insert-ascending")]
    MapBuildInsertAscending {},
    /// Lookup present entries.
    #[serde(rename = "map-get-hit")]
    MapGetHit {
        /// Hot pct.
        hot_pct: usize,
    },
    /// Lookup absent entries.
    #[serde(rename = "map-get-miss")]
    MapGetMiss {
        /// Hot pct.
        hot_pct: usize,
    },
    /// Insert new entries.
    #[serde(rename = "map-insert-new")]
    MapInsertNew {},
    /// Insert existing entries.
    #[serde(rename = "map-insert-overwrite")]
    MapInsertOverwrite {},
    /// Remove present entries.
    #[serde(rename = "map-remove")]
    MapRemove {},
    /// Visit a bounded range.
    #[serde(rename = "map-range")]
    MapRange {
        /// Visit len.
        visit_len: usize,
        /// Hot pct.
        hot_pct: usize,
    },
    /// Visit all entries in order.
    #[serde(rename = "map-iterate")]
    MapIterate {},
    /// Read the first entry.
    #[serde(rename = "map-first")]
    MapFirst {},
    /// Read the last entry.
    #[serde(rename = "map-last")]
    MapLast {},
    /// Read and update a collection.
    #[serde(rename = "map-mixed")]
    MapMixed {
        /// Write pct.
        write_pct: usize,
        /// Hot pct.
        hot_pct: usize,
    },
    /// Find the insertion rank.
    #[serde(rename = "map-rank")]
    MapRank {
        /// Hot pct.
        hot_pct: usize,
    },
    /// Read by zero-based rank.
    #[serde(rename = "map-select")]
    MapSelect {
        /// Hot pct.
        hot_pct: usize,
    },
    /// Build and run the first lookups.
    #[serde(rename = "map-build-get")]
    MapBuildGet {
        /// Query pct.
        query_pct: usize,
        /// Hot pct.
        hot_pct: usize,
    },
}

impl Default for Operation {
    fn default() -> Self {
        Self::Sort {}
    }
}

impl Operation {
    /// The catalog key shared by all parameterizations of this operation.
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::Sort {} => "sort",
            Self::Argsort {} => "argsort",
            Self::PartitionBuckets { .. } => "partition-buckets",
            Self::SelectNth { .. } => "select_nth",
            Self::MapBuild { .. } => "map-build",
            Self::MapBuildInsertRandom { .. } => "map-build-insert-random",
            Self::MapBuildInsertAscending { .. } => "map-build-insert-ascending",
            Self::MapGetHit { .. } => "map-get-hit",
            Self::MapGetMiss { .. } => "map-get-miss",
            Self::MapInsertNew { .. } => "map-insert-new",
            Self::MapInsertOverwrite { .. } => "map-insert-overwrite",
            Self::MapRemove { .. } => "map-remove",
            Self::MapRange { .. } => "map-range",
            Self::MapIterate { .. } => "map-iterate",
            Self::MapFirst { .. } => "map-first",
            Self::MapLast { .. } => "map-last",
            Self::MapMixed { .. } => "map-mixed",
            Self::MapRank { .. } => "map-rank",
            Self::MapSelect { .. } => "map-select",
            Self::MapBuildGet { .. } => "map-build-get",
        }
    }

    /// Whether this is complete sorting.
    pub fn is_sort(&self) -> bool {
        matches!(self, Self::Sort {})
    }

    /// Whether the case measures an ordered collection.
    pub fn is_collection(&self) -> bool {
        super::operation_entry(self.kind()).is_some_and(|entry| entry.collection)
    }

    pub(crate) fn retained_heap(&self) -> bool {
        matches!(self, Self::MapBuild {} | Self::MapMixed { .. })
    }

    pub(crate) fn work_items(&self, len: usize) -> usize {
        let count = len.min(4096);
        match self {
            Self::MapBuild {}
            | Self::MapBuildInsertRandom {}
            | Self::MapBuildInsertAscending {}
            | Self::MapBuildGet { .. }
            | Self::MapIterate {} => 1,
            Self::MapRange { visit_len, .. } => (count / visit_len).max(1),
            Self::MapMixed { .. } => count.div_ceil(20) * 20,
            _ => count,
        }
    }

    pub(crate) fn collection_input_entries(&self, len: usize) -> usize {
        if self.retained_heap() || matches!(self, Self::MapBuildGet { .. }) {
            len + len / 10
        } else {
            len
        }
    }

    pub(crate) fn cold_queries(&self, len: usize) -> usize {
        match self {
            Self::MapBuildGet { query_pct, .. } => (len * query_pct).div_ceil(100),
            _ => 0,
        }
    }

    pub(crate) fn churn_pairs(&self, len: usize) -> usize {
        if matches!(self, Self::MapMixed { .. }) {
            len
        } else {
            0
        }
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
            Self::MapGetHit { hot_pct } => matches!(hot_pct, 0 | 90),
            Self::MapGetMiss { hot_pct } => matches!(hot_pct, 0 | 90),
            Self::MapRange { visit_len, hot_pct } => {
                matches!(visit_len, 16 | 256) && *visit_len <= len && matches!(hot_pct, 0 | 90)
            }
            Self::MapMixed { write_pct, hot_pct } => {
                matches!(write_pct, 10 | 50) && matches!(hot_pct, 0 | 90)
            }
            Self::MapRank { hot_pct } => matches!(hot_pct, 0 | 90),
            Self::MapSelect { hot_pct } => matches!(hot_pct, 0 | 90),
            Self::MapBuildGet { query_pct, hot_pct } => {
                matches!(query_pct, 1 | 10 | 100) && matches!(hot_pct, 0 | 90)
            }
            _ => true,
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
            Self::MapRange { hot_pct, .. } => Self::MapRange {
                visit_len: 16,
                hot_pct: *hot_pct,
            },
            other => other.clone(),
        }
    }
}
