// SPDX-License-Identifier: BSD-2-Clause

//! Catching a key-function panic, in the one place a build can lose it.
//!
//! With `std`, a panic in the middle of a pass is caught, the pass is completed
//! and every record is moved back into the input slice before the panic is
//! resumed, so the caller's slice again holds each record exactly once. A build
//! without `std` has no unwinder to catch with, so panics must abort: the
//! process ends before any of those invariants can be observed.

#[cfg(feature = "std")]
mod imp {
    use alloc::boxed::Box;
    use core::any::Any;

    /// The payload a caught panic carries.
    pub(crate) type Payload = Box<dyn Any + Send>;

    pub(crate) fn catch<R>(body: impl FnOnce() -> R) -> Result<R, Payload> {
        std::panic::catch_unwind(core::panic::AssertUnwindSafe(body))
    }

    pub(crate) fn resume(payload: Payload) -> ! {
        std::panic::resume_unwind(payload)
    }
}

#[cfg(not(feature = "std"))]
mod imp {
    /// A build without `std` cannot catch a panic, so this type stays empty.
    pub(crate) type Payload = core::convert::Infallible;

    /// Runs `body`; a panic in an aborting build never returns here.
    pub(crate) fn catch<R>(body: impl FnOnce() -> R) -> Result<R, Payload> {
        Ok(body())
    }

    pub(crate) fn resume(payload: Payload) -> ! {
        match payload {}
    }
}

pub(crate) use imp::{Payload, catch, resume};
