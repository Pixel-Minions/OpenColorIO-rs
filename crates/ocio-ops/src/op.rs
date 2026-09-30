// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The op model: a port of `src/OpenColorIO/Op.h` and `Op.cpp` @ v2.5.2.
//!
//! So far only the CPU renderer interface, `OpCPU`, is here: the S2 spike's Log and Gamma
//! renderers implement it. The rest of `Op.h` (`OpData`, `Op`, `OpRcPtrVec`) is WP 1.2c.

use std::fmt::Debug;

/// A CPU renderer: processes RGBA `f32` pixels.
///
/// Upstream's `OpCPU::apply(inImg, outImg, numPixels)` reads `inImg` and writes `outImg`,
/// which may be the same buffer. Every renderer that works on `f32` pixels reads a whole
/// pixel before writing it, so the port applies them in place: `rgba` holds
/// `numPixels * 4` values, and the caller copies the input first when it has separate
/// buffers (`docs/architecture.md`, "CPU renderers and numeric profiles").
///
/// Port of `OpCPU` (src/OpenColorIO/Op.h:26-50 @ v2.5.2). The dynamic-property accessors
/// come with WP 1.2b.
pub trait CpuOp: Send + Sync + Debug {
    /// Port of `OpCPU::apply`, in place. `rgba.len()` must be a multiple of 4.
    fn apply(&self, rgba: &mut [f32]);
}
