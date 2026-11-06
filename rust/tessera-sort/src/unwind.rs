// SPDX-License-Identifier: BSD-2-Clause

//! Protecting record ownership when a key function panics.
//!
//! With `std`, a panic in the middle of a pass is caught, the pass is completed
//! and every record is moved back into the input slice before the panic is
//! resumed, so the caller's slice again holds each record exactly once. Without
//! `std`, an unwind guard aborts before unwinding can reach storage whose
//! ownership directory is incomplete. `no_std` alone does not disable unwinding.

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

    struct AbortOnUnwind;

    impl Drop for AbortOnUnwind {
        fn drop(&mut self) {
            // This guard is forgotten on success. Panicking during cleanup
            // aborts before the caller can restore an incomplete directory.
            panic!("cannot unwind through record movement without the std feature");
        }
    }

    pub(crate) fn catch<R>(body: impl FnOnce() -> R) -> Result<R, Payload> {
        let guard = AbortOnUnwind;
        let result = body();
        core::mem::forget(guard);
        Ok(result)
    }

    pub(crate) fn resume(payload: Payload) -> ! {
        match payload {}
    }
}

pub(crate) use imp::{Payload, catch, resume};
