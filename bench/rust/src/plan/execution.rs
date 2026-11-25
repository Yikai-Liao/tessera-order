// SPDX-License-Identifier: BSD-2-Clause

//! How a column calls its implementation: the serial entry point, or the
//! parallel entry point inside a caller-owned pool of a fixed worker count.

use serde::{Deserialize, Serialize};
use std::{fmt, num::NonZeroUsize, str::FromStr};

/// `serial`, or `tN`: the parallel entry point in an N-worker pool.
///
/// `t1` is not `serial`: it measures the parallel entry's one-worker path.
/// Serial orders before every worker count, and worker counts ascend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub enum Execution {
    /// The serial entry point on the calling thread.
    Serial,
    /// The parallel entry point in a pool of exactly this many workers.
    Threads(NonZeroUsize),
}

impl Execution {
    /// Persistent pool workers, absent for the serial entry point.
    pub fn threads(self) -> Option<usize> {
        match self {
            Self::Serial => None,
            Self::Threads(threads) => Some(threads.get()),
        }
    }
}

impl fmt::Display for Execution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Serial => f.write_str("serial"),
            Self::Threads(threads) => write!(f, "t{threads}"),
        }
    }
}

impl FromStr for Execution {
    type Err = String;

    /// Decodes only the canonical spelling, so every execution has one ID.
    fn from_str(text: &str) -> Result<Self, String> {
        if text == "serial" {
            return Ok(Self::Serial);
        }
        text.strip_prefix('t')
            .and_then(super::parse_count)
            .and_then(NonZeroUsize::new)
            .map(Self::Threads)
            .ok_or_else(|| format!("invalid execution {text:?}; expected serial or tN with N >= 1"))
    }
}

impl TryFrom<String> for Execution {
    type Error = String;
    fn try_from(text: String) -> Result<Self, String> {
        text.parse()
    }
}

impl From<Execution> for String {
    fn from(execution: Execution) -> Self {
        execution.to_string()
    }
}
