// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Inherit SIGPIPE as the C++ tools do (Linux only; owner, 2026-10-31).
//!
//! A C++ tool keeps the disposition of SIGPIPE its parent gave it: by default a write to a pipe
//! nobody reads ends the process, and under a parent that ignores SIGPIPE the write only fails.
//! Rust's runtime sets SIGPIPE to ignored before `main`, whatever it was. So the disposition is
//! read before the runtime runs, by a constructor in `.init_array`, and [`restore`], the first
//! thing a tool does, puts the default back if that is what the process started with. Nothing
//! else of the runtime changes: its arguments, its panic and stack-overflow handling and its
//! exit stay as they are, and if the constructor hasn't run the disposition stays ignored.

#![allow(unsafe_code)]

use std::ffi::c_int;
use std::sync::atomic::{AtomicUsize, Ordering};

/// SIGPIPE's number on Linux (asm-generic/signal.h).
const SIGPIPE: c_int = 13;
/// `SIG_DFL` on Linux (asm-generic/signal-defs.h).
const SIG_DFL: usize = 0;
/// `SIG_IGN` on Linux (asm-generic/signal-defs.h).
const SIG_IGN: usize = 1;

/// Neither disposition: [`capture`] hasn't run.
const NOT_READ: usize = usize::MAX;

unsafe extern "C" {
    /// The C runtime's `signal`, with the handlers `SIG_DFL` and `SIG_IGN` as numbers. It
    /// returns the disposition it replaces.
    fn signal(signum: c_int, handler: usize) -> usize;
}

/// The disposition the process started with.
static INHERITED: AtomicUsize = AtomicUsize::new(NOT_READ);

/// Reads SIGPIPE's disposition, which only setting it can do through `signal`, and sets it back.
extern "C" fn capture() {
    // SAFETY: `signal` with SIG_IGN and with the handler it returned, which exec leaves as
    // SIG_DFL or SIG_IGN, on a valid signal number, while the process has one thread.
    let inherited = unsafe {
        let inherited = signal(SIGPIPE, SIG_IGN);
        signal(SIGPIPE, inherited);
        inherited
    };
    INHERITED.store(inherited, Ordering::Relaxed);
}

/// Runs [`capture`] before `main`, and so before Rust's runtime sets SIGPIPE to ignored.
#[used]
#[unsafe(link_section = ".init_array")]
static CAPTURE: extern "C" fn() = capture;

/// Gives SIGPIPE back the default disposition, if the process started with it.
pub(crate) fn restore() {
    if INHERITED.load(Ordering::Relaxed) == SIG_DFL {
        // SAFETY: `signal` with SIG_DFL on a valid signal number.
        unsafe {
            signal(SIGPIPE, SIG_DFL);
        }
    }
}
