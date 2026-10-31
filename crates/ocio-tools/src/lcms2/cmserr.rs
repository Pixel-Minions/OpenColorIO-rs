// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of the error logging and allocation limits of `src/cmserr.c` @ lcms2.17.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// `MAX_ERROR_MESSAGE_LEN` (src/cmserr.c:448 @ lcms2.17).
const MAX_ERROR_MESSAGE_LEN: usize = 1024;
/// `MAX_MEMORY_FOR_ALLOC` (src/cmserr.c:78 @ lcms2.17).
pub const MAX_MEMORY_FOR_ALLOC: u32 = 1024 * 1024 * 512;

/// `cmsLogErrorHandlerFunction`, without the context, which is always null here.
pub type LogErrorHandler = fn(u32, &[u8]);

static HANDLER: Mutex<Option<LogErrorHandler>> = Mutex::new(None);

/// A read the wheel makes outside its memory has been reached (`U-` entries in
/// `docs/improvements.md`). The evaluators can't stop there, so they return zero and leave this
/// set; the tool checks it where it can stop, and reports a deliberate error.
static OUT_OF_BOUNDS: AtomicBool = AtomicBool::new(false);

/// Port of `cmsSetLogErrorHandler` (src/cmserr.c:503-506 @ lcms2.17).
pub fn cms_set_log_error_handler(handler: LogErrorHandler) {
    *HANDLER.lock().unwrap() = Some(handler);
}

/// Port of `cmsSignalError` (src/cmserr.c:510-526 @ lcms2.17). `text` is what `vsnprintf` formats.
///
/// `vsnprintf(Buffer, MAX_ERROR_MESSAGE_LEN-1, ...)` keeps 1022 characters with the C library's
/// function. `lcms2_internal.h` makes it `_vsnprintf` under MSVC, which keeps 1023 and then no
/// terminator, so the message runs on into whatever follows on the stack (U-650).
pub fn cms_signal_error(code: u32, text: &[u8]) {
    let kept = if cfg!(windows) {
        MAX_ERROR_MESSAGE_LEN - 1
    } else {
        MAX_ERROR_MESSAGE_LEN - 2
    };
    let text = &text[..text.len().min(kept)];
    let handler = *HANDLER.lock().unwrap();
    if let Some(handler) = handler {
        handler(code, text);
    }
}

/// Port of `_cmsTagSignature2String` (src/cmserr.c:529-541 @ lcms2.17). The result is a C string: it
/// stops at the signature's first zero byte.
pub fn cms_tag_signature_2_string(sig: u32) -> Vec<u8> {
    let bytes = sig.to_be_bytes();
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(4);
    bytes[..end].to_vec()
}

/// Whether `_cmsCallocDefaultFn` (src/cmserr.c:143-162 @ lcms2.17) gives the memory.
pub fn cms_calloc_fits(num: u32, size: u32) -> bool {
    let total = num.wrapping_mul(size);
    if total == 0 {
        return false;
    }
    if num >= u32::MAX / size {
        return false;
    }
    if total < num || total < size {
        return false;
    }
    total <= MAX_MEMORY_FOR_ALLOC
}

/// Whether `_cmsMallocDefaultFn` (src/cmserr.c:92-101 @ lcms2.17) gives the memory.
pub fn cms_malloc_fits(size: u32) -> bool {
    size != 0 && size <= MAX_MEMORY_FOR_ALLOC
}

/// Notes that the wheel would be outside its memory here.
#[cold]
pub fn mark_out_of_bounds() {
    OUT_OF_BOUNDS.store(true, Ordering::Relaxed);
}

/// Whether [`mark_out_of_bounds`] has been called.
#[inline]
pub fn out_of_bounds() -> bool {
    OUT_OF_BOUNDS.load(Ordering::Relaxed)
}
