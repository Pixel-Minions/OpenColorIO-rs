// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The finalization and optimization of op lists: a port of
//! `src/OpenColorIO/OpOptimizers.cpp` @ v2.5.2.
//!
//! So far `OpRcPtrVec::finalize`, which the CPU processor needs. The optimizer
//! (`OpRcPtrVec::optimize` and `optimizeForBitdepth`) is WP 1.6.

use crate::exception::Result;
use crate::op::OpVec;

/// Finalizes each op ([`Op::finalize`](crate::op::Op::finalize)): e.g. Matrix and Range ops
/// become forward, and a Lut1D gets ready for inversion.
///
/// Port of `FinalizeOps` (src/OpenColorIO/OpOptimizers.cpp:132-139 @ v2.5.2).
fn finalize_ops(op_vec: &mut OpVec) -> Result<()> {
    for op in op_vec.iter_mut() {
        // Prepare LUT 1D for inversion and ensure Matrix & Range are forward.
        op.finalize()?;
    }
    Ok(())
}

impl OpVec {
    /// Validates the ops, then finalizes each one: e.g. Matrix and Range ops become forward,
    /// and a Lut1D gets ready for inversion. Nothing happens to an empty list.
    ///
    /// Port of `OpRcPtrVec::finalize` (src/OpenColorIO/OpOptimizers.cpp:598-609 @ v2.5.2).
    pub fn finalize(&mut self) -> Result<()> {
        if self.is_empty() {
            return Ok(());
        }

        self.validate()?;

        // Prepare LUT 1D for inversion and ensure Matrix & Range are forward.
        finalize_ops(self)
    }
}
