// SPDX-License-Identifier: BSD-2-Clause

//! Exact comparison cases and resumable groups.

use serde::{Deserialize, Serialize};

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
