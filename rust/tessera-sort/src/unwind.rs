// SPDX-License-Identifier: BSD-2-Clause

//! Catch key-function panics while ownership is distributed across record storage.
//! The caller completes recovery before resuming the panic, so the input again
//! holds every record exactly once.

use alloc::boxed::Box;
use core::any::Any;

/// The payload a caught panic carries.
pub(crate) type Payload = Box<dyn Any + Send>;

/// Catches a panic inside a record-moving operation whose caller owns recovery.
pub(crate) fn catch<R>(body: impl FnOnce() -> R) -> Result<R, Payload> {
    std::panic::catch_unwind(core::panic::AssertUnwindSafe(body))
}

/// Resumes a caught panic after record ownership has been restored.
pub(crate) fn resume(payload: Payload) -> ! {
    std::panic::resume_unwind(payload)
}
