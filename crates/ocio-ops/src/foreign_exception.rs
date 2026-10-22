// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Exceptions of an application's callbacks that travel through the library as C++ exceptions
//! travel through upstream's (owner decision 2026-10-22, Q2 = U).
//!
//! Upstream calls an application's functions (the logging function, the hash function, a
//! `ConfigIOProxy`) from deep inside the library, and whatever they throw unwinds through it:
//! PyOpenColorIO's callbacks throw pybind11's `error_already_set` for a Python exception,
//! which is a `std::exception` but not an `OCIO::Exception`. A site that catches
//! `const Exception &` lets it through; a site that catches `std::exception` reads its `what()`
//! (and usually throws an `OCIO::Exception` of its own); `catch (...)` swallows it; everywhere
//! else it reaches the caller as it was.
//!
//! The port's callbacks return nothing (`LoggingFunction`, `ComputeHashFunction`) or the
//! library's own errors (`ConfigIoProxy`), so such an exception travels as a Rust unwind: a
//! callback calls [`raise`] with a [`ForeignException`] (its `what()` and a payload: the Python
//! exception), which `std::panic::resume_unwind` carries (no panic hook runs, nothing is
//! printed); the sites that port a C++ `catch (std::exception &)` or `catch (...)` catch it
//! ([`try_foreign`], [`catch_std`]); the application's own boundary catches the rest (the
//! Python module's dispatcher, which restores the Python exception). A Rust panic that isn't a
//! `ForeignException` is never caught here.
//!
//! The library's locks recover from an unwind (each lock site takes the guard of a poisoned
//! lock), as C++'s RAII locks release on unwinding; state a site changed before the callback
//! stays changed, as in C++ (`GetFastFileHash` marks the hash ready before computing it).
//!
//! This works only with `panic = "unwind"`: a build with `panic = "abort"` fails here.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use crate::exception::{Exception, Result};

#[cfg(panic = "abort")]
compile_error!(
    "ocio-ops needs panic = \"unwind\": an application's callback exceptions travel through the \
     library as unwinds (ocio_ops::foreign_exception)"
);

/// An exception an application's callback raised, unwinding through the library: its
/// `what()` (the text a C++ `catch (const std::exception & e)` reads), and the application's
/// payload (the Python module's: the Python exception itself).
pub struct ForeignException {
    what: Vec<u8>,
    payload: Box<dyn Any + Send>,
}

impl std::fmt::Debug for ForeignException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ForeignException")
            .field("what", &String::from_utf8_lossy(&self.what))
            .finish_non_exhaustive()
    }
}

impl ForeignException {
    /// An exception of `what()` `what`, carrying `payload`.
    pub fn new(what: impl Into<Vec<u8>>, payload: Box<dyn Any + Send>) -> ForeignException {
        ForeignException {
            what: what.into(),
            payload,
        }
    }

    /// Its `what()`.
    pub fn what(&self) -> &[u8] {
        &self.what
    }

    /// The application's payload.
    pub fn into_payload(self) -> Box<dyn Any + Send> {
        self.payload
    }
}

/// Throws `e` through the library: a Rust unwind (`resume_unwind`, so no panic hook runs).
pub fn raise(e: ForeignException) -> ! {
    resume_unwind(Box::new(e))
}

/// Runs `f`, and catches a [`ForeignException`] that unwinds out of it: what a C++ `try` block
/// with a `catch` of `std::exception` or `...` sees of an application's exception. Any other
/// unwind goes on.
pub fn try_foreign<R>(f: impl FnOnce() -> R) -> std::result::Result<R, ForeignException> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => Ok(r),
        Err(payload) => match payload.downcast::<ForeignException>() {
            Ok(e) => Err(*e),
            Err(other) => resume_unwind(other),
        },
    }
}

/// Runs `f`, a `try` block whose `catch (const std::exception & e)` handles the library's
/// exceptions and an application's alike, by their `what()`: a [`ForeignException`] out of
/// `f` becomes an [`Exception`] of its `what()` (the handler then reads it as it reads the
/// library's).
pub fn catch_std<R>(f: impl FnOnce() -> Result<R>) -> Result<R> {
    try_foreign(f).unwrap_or_else(|e| Err(Exception::new(e.what)))
}

/// Throws again an exception [`try_foreign`] caught (C++'s `throw;`).
pub fn rethrow(e: ForeignException) -> ! {
    raise(e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, PoisonError};

    fn boom() -> ForeignException {
        ForeignException::new("Boom: text", Box::new(42u32))
    }

    #[test]
    fn a_foreign_exception_unwinds_to_the_site_that_catches_it() {
        let caught = try_foreign(|| -> u8 { raise(boom()) }).unwrap_err();
        assert_eq!(caught.what(), b"Boom: text");
        assert_eq!(*caught.into_payload().downcast::<u32>().unwrap(), 42);
        assert_eq!(try_foreign(|| 7).unwrap(), 7);
    }

    #[test]
    fn catch_std_reads_its_what() {
        let e = catch_std(|| -> Result<()> { raise(boom()) }).unwrap_err();
        assert_eq!(e.what(), b"Boom: text");
        assert_eq!(catch_std(|| Ok(3)).unwrap(), 3);
        let own = catch_std(|| -> Result<()> { Err(Exception::new("own")) }).unwrap_err();
        assert_eq!(own.what(), b"own");
    }

    #[test]
    fn another_panic_goes_on() {
        let outer = catch_unwind(|| try_foreign(|| -> () { resume_unwind(Box::new("other")) }));
        let payload = outer.unwrap_err();
        assert_eq!(*payload.downcast::<&str>().unwrap(), "other");
    }

    #[test]
    fn a_lock_held_while_it_unwinds_recovers() {
        let lock = Mutex::new(1);
        let caught = try_foreign(|| {
            let mut guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
            *guard = 2;
            raise(boom())
        });
        assert!(caught.is_err());
        assert!(lock.is_poisoned());
        assert_eq!(*lock.lock().unwrap_or_else(PoisonError::into_inner), 2);
    }

    #[test]
    fn rethrow_carries_the_same_exception() {
        let first = try_foreign(|| -> () { raise(boom()) }).unwrap_err();
        let again = try_foreign(|| -> () { rethrow(first) }).unwrap_err();
        assert_eq!(again.what(), b"Boom: text");
    }
}
