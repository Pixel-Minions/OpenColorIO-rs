// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The FixedFunction op: a port of `src/OpenColorIO/ops/fixedfunction/FixedFunctionOp.h` and
//! `FixedFunctionOp.cpp` @ v2.5.2, what the CPU engine needs: the op's behaviors, and
//! [`create_fixed_function_op`] and [`create_fixed_function_op_from_data`].
//! `BuildFixedFunctionOp` and `CreateFixedFunctionTransform` work on the transform: they are in
//! `ocio` (crates/ocio/src/transforms/fixed_function_transform.rs); `extractGpuShaderInfo` is in
//! `ocio-gpu`, the FixedFunction arm of `gpu_processor::extract_op_gpu_shader_info` (2.3f).
//!
//! As for every family, the op is its data, [`OpData::FixedFunction`]: [`Op`]'s methods match
//! on it and call the methods here, `FixedFunctionOp`'s overrides. `FixedFunctionOp` keeps the
//! `Op` defaults for the rest: its `finalize` does nothing (each style's renderer handles its
//! direction).

use std::sync::Arc;

use super::fixed_function_op_cpu::get_fixed_function_cpu_renderer;
use super::fixed_function_op_data::{FixedFunctionOpData, FixedFunctionOpStyle, Params};
use crate::exception::{Exception, Result};
use crate::op::{CpuOp, Op, OpVec};
use crate::op_data::OpData;
use crate::open_color_types::TransformDirection;

impl FixedFunctionOpData {
    /// A new FixedFunction op with a copy of the data ([`FixedFunctionOpData::try_clone`],
    /// which validates it).
    ///
    /// Port of `FixedFunctionOp::clone` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOp.cpp:
    /// 62-66 @ v2.5.2).
    pub(crate) fn clone_op(&self) -> Result<Op> {
        Ok(Op::new(OpData::FixedFunction(self.try_clone()?)))
    }

    /// Port of `FixedFunctionOp::getInfo` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOp.cpp:72-75 @ v2.5.2).
    pub(crate) fn get_info(&self) -> &'static str {
        "<FixedFunctionOp>"
    }

    /// Whether `op` is a FixedFunction op: what upstream's `DynamicPtrCast` to
    /// `FixedFunctionOp` finds.
    ///
    /// Port of `FixedFunctionOp::isSameType` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOp.cpp:82-86 @ v2.5.2).
    pub(crate) fn is_same_type(&self, op: &Op) -> bool {
        matches!(**op.data(), OpData::FixedFunction(_))
    }

    /// Whether `op` is a FixedFunction op whose data undoes this one's
    /// ([`FixedFunctionOpData::is_inverse`]).
    ///
    /// # Panics
    ///
    /// Panics, with upstream's error text, where [`FixedFunctionOpData::is_inverse`] fails:
    /// where this op's data doesn't validate, or, for a Rec.2100 surround, where either op has
    /// no parameter. Upstream throws in the first case, since its comparison builds this
    /// data's inverse, a copy it validates; in the second it reads a parameter past the end of
    /// its vector, which the port refuses (`docs/improvements.md` U-31). `Op::is_inverse`
    /// answers a `bool`, as upstream's does, so the error can't be returned. The `ocio` crate
    /// never gets there: the optimizer, the only caller, compares ops of an `OpRcPtrVec` that
    /// `finalize` validated first.
    ///
    /// Port of `FixedFunctionOp::isInverse` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOp.cpp:88-95 @ v2.5.2).
    pub(crate) fn is_inverse_op(&self, op: &Op) -> bool {
        match &**op.data() {
            OpData::FixedFunction(fn_op_data) => self
                .is_inverse(fn_op_data)
                .unwrap_or_else(|e| panic!("FixedFunctionOp::isInverse: {}", e.message())),
            _ => false,
        }
    }

    /// Never: the op combines with nothing.
    ///
    /// Port of `FixedFunctionOp::canCombineWith` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOp.cpp:97-100 @ v2.5.2).
    pub(crate) fn can_combine_with(&self, _op: &Op) -> bool {
        false
    }

    /// Refuses: the caller must check [`can_combine_with`](Self::can_combine_with) first.
    ///
    /// Port of `FixedFunctionOp::combineWith` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOp.cpp:102-109 @ v2.5.2).
    pub(crate) fn combine_with(&self, _ops: &mut OpVec, second_op: &Op) -> Result<()> {
        if !self.can_combine_with(second_op) {
            return Err(Exception::new(
                "FixedFunctionOp: canCombineWith must be checked before calling combineWith.",
            ));
        }
        Ok(())
    }

    /// The op's cache ID: `<FixedFunctionOp `, the data's cache ID, `>`.
    ///
    /// Port of `FixedFunctionOp::getCacheID` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOp.cpp:111-120 @ v2.5.2).
    pub(crate) fn get_op_cache_id(&self) -> Vec<u8> {
        let mut cache_id = b"<FixedFunctionOp ".to_vec();
        cache_id.extend_from_slice(&self.get_cache_id());
        cache_id.push(b'>');
        cache_id
    }

    /// The renderer of the style ([`get_fixed_function_cpu_renderer`]).
    ///
    /// Port of `FixedFunctionOp::getCPUOp` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOp.cpp:122-126 @ v2.5.2).
    pub(crate) fn get_cpu_op(&self, fast_log_exp_pow: bool) -> Result<Arc<dyn CpuOp>> {
        get_fixed_function_cpu_renderer(self, fast_log_exp_pow)
    }
}

/// Appends a forward FixedFunction op of `style` and `params`, validated.
///
/// Port of `CreateFixedFunctionOp(OpRcPtrVec &, FixedFunctionOpData::Style, const Params &)`
/// (src/OpenColorIO/ops/fixedfunction/FixedFunctionOp.cpp:143-148 @ v2.5.2).
pub fn create_fixed_function_op(
    ops: &mut OpVec,
    style: FixedFunctionOpStyle,
    params: &Params,
) -> Result<()> {
    let func_data = FixedFunctionOpData::with_params(style, params.clone())?;
    create_fixed_function_op_from_data(ops, func_data, TransformDirection::Forward)
}

/// Appends a FixedFunction op holding `func_data`, or its inverse
/// ([`FixedFunctionOpData::inverse`], which validates it) when `direction` is inverse. A
/// forward op's data isn't validated.
///
/// Upstream's op shares the caller's `FixedFunctionOpDataRcPtr` when `direction` is forward;
/// here the op owns the data, which nothing changes afterwards (`Op`'s docs).
///
/// Port of `CreateFixedFunctionOp(OpRcPtrVec &, FixedFunctionOpDataRcPtr &,
/// TransformDirection)` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOp.cpp:150-161 @
/// v2.5.2).
pub fn create_fixed_function_op_from_data(
    ops: &mut OpVec,
    func_data: FixedFunctionOpData,
    direction: TransformDirection,
) -> Result<()> {
    let mut func = func_data;
    if direction == TransformDirection::Inverse {
        func = func.inverse()?;
    }
    ops.push_back(Op::new(OpData::FixedFunction(func)));
    Ok(())
}

#[cfg(test)]
#[path = "fixed_function_op_tests.rs"]
mod tests;
