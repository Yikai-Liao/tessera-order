// SPDX-License-Identifier: BSD-2-Clause

//! Protecting record ownership when a key function panics.
//!
//! A panic in the middle of a pass is caught, the pass is completed and every
//! record is moved back into the input slice before the panic is resumed, so
//! the caller slice again holds each record exactly once.

use std::any::Any;
use std::boxed::Box;

/// The payload a caught panic carries.
pub(crate) type Payload = Box<dyn Any + Send>;

/// Runs a body, returning the payload instead of unwinding past the caller.
pub(crate) fn catch<R>(body: impl FnOnce() -> R) -> Result<R, Payload> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(body))
}

/// Resumes a panic caught by [`catch`].
pub(crate) fn resume(payload: Payload) -> ! {
    std::panic::resume_unwind(payload)
}
