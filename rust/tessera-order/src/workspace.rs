// SPDX-License-Identifier: BSD-2-Clause

//! Capacity owned by one invocation and lent to complete movement operations.
//! Tile and block storage retain their separate restoration protocols. A call
//! can reuse capacity only after those operations return or finish unwinding;
//! no live scratch-backed records or cross-call progress live in this owner.

use crate::{block, tile};

/// Lazy storage shared across serial subproblems of the owning invocation.
/// Parallel finishers borrow separate owners from the invocation-local pool.
pub(crate) struct Workspace<T> {
    pub(crate) tile: tile::Scratch<T>,
    pub(crate) block: block::Buffers<T>,
}

impl<T> Workspace<T> {
    /// Creates an empty owner; the complete operations allocate as needed.
    pub(crate) fn new() -> Self {
        Self {
            tile: tile::Scratch::new(),
            block: block::Buffers::new(),
        }
    }
}
