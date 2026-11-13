// SPDX-License-Identifier: BSD-2-Clause

//! Test helper for observing panic recovery across record-moving operations.

use alloc::boxed::Box;
use core::any::Any;

/// The payload a caught panic carries.
pub(crate) type Payload = Box<dyn Any + Send>;

/// Catches a test panic without imposing unwind bounds on its fixture.
pub(crate) fn catch<R>(body: impl FnOnce() -> R) -> Result<R, Payload> {
    std::panic::catch_unwind(core::panic::AssertUnwindSafe(body))
}
