// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CDL op: part of a port of `src/OpenColorIO/ops/cdl/CDLOp.h` and `CDLOp.cpp` @ v2.5.2,
//! what the CPU engine needs: the op's behaviors, and the functions that create ops,
//! [`create_cdl_op`] and [`create_cdl_op_from_values`]. `CreateCDLTransform` and `BuildCDLOp`
//! work on the `CDLTransform`: they are in `ocio` (crates/ocio/src/transforms/cdl_transform.rs); `extractGpuShaderInfo`
//! comes with the GPU writer (1.3c4).
//!
//! As for every family, the op is its data, [`OpData::Cdl`]: [`Op`]'s methods match on it and
//! call the methods here, `CDLOp`'s overrides. `CDLOp` keeps the `Op` defaults for `finalize`
//! (nothing: a reverse style renders as it is) and the queries it doesn't override. It never
//! combines with another op; the optimizer simplifies it instead
//! (`CDLOpData::getSimplerReplacement`).

use std::sync::Arc;

use super::cdl_op_cpu::get_cdl_cpu_renderer;
use super::cdl_op_data::{CdlOpData, CdlOpStyle, ChannelParams};
use crate::exception::{Exception, Result};
use crate::op::{CpuOp, Op, OpVec};
use crate::op_data::OpData;
use crate::open_color_types::TransformDirection;

impl CdlOpData {
    /// A new CDL op with a copy of the data.
    ///
    /// Port of `CDLOp::clone` (src/OpenColorIO/ops/cdl/CDLOp.cpp:67-71 @ v2.5.2).
    pub(crate) fn clone_op(&self) -> Op {
        cdl_op(self.clone())
    }

    /// Port of `CDLOp::getInfo` (CDLOp.cpp:77-80 @ v2.5.2).
    pub(crate) fn get_info(&self) -> &'static str {
        "<CDLOp>"
    }

    /// Whether `op` is a CDL op: what upstream's `DynamicPtrCast` to `CDLOp` finds.
    ///
    /// Port of `CDLOp::isSameType` (CDLOp.cpp:87-91 @ v2.5.2).
    pub(crate) fn is_same_type(&self, op: &Op) -> bool {
        matches!(**op.data(), OpData::Cdl(_))
    }

    /// Whether `op` is a CDL op whose data equals this one's inverse
    /// ([`CdlOpData::is_inverse`]).
    ///
    /// Port of `CDLOp::isInverse` (CDLOp.cpp:93-100 @ v2.5.2).
    pub(crate) fn is_inverse_op(&self, op: &Op) -> bool {
        match &**op.data() {
            OpData::Cdl(cdl_data2) => self.is_inverse(cdl_data2),
            OpData::Gamma(_)
            | OpData::Log(_)
            | OpData::FixedFunction(_)
            | OpData::Lut1D(_)
            | OpData::Matrix(_)
            | OpData::Range(_)
            | OpData::Exponent(_)
            | OpData::GradingRgbCurve(_)
            | OpData::Reference(_)
            | OpData::NoOp(_) => false,
        }
    }

    /// Never: upstream's TODOs are to allow combining with LUTs and matrices.
    ///
    /// Port of `CDLOp::canCombineWith` (CDLOp.cpp:102-107 @ v2.5.2).
    pub(crate) fn can_combine_with(&self, _op: &Op) -> bool {
        // TODO: Allow combining with LUTs.
        // TODO: Allow combining with matrices.
        false
    }

    /// "CDLOp: canCombineWith must be checked before calling combineWith.", since
    /// [`can_combine_with`](Self::can_combine_with) never accepts.
    ///
    /// Port of `CDLOp::combineWith` (CDLOp.cpp:109-117 @ v2.5.2).
    pub(crate) fn combine_with(&self, _ops: &mut OpVec, second_op: &Op) -> Result<()> {
        if !self.can_combine_with(second_op) {
            return Err(Exception::new(
                "CDLOp: canCombineWith must be checked before calling combineWith.",
            ));
        }

        // TODO: Implement CDLOp::combineWith()
        Ok(())
    }

    /// The op's cache ID: `<CDLOp `, the data's cache ID (which ends with a space), `>`.
    ///
    /// Port of `CDLOp::getCacheID` (CDLOp.cpp:119-128 @ v2.5.2).
    pub(crate) fn get_op_cache_id(&self) -> Vec<u8> {
        let mut cache_id = b"<CDLOp ".to_vec();
        cache_id.extend_from_slice(&self.get_cache_id());
        cache_id.push(b'>');
        cache_id
    }

    /// The renderer for the style: the fast-math one when `fast_log_exp_pow`.
    ///
    /// Port of `CDLOp::getCPUOp` (CDLOp.cpp:130-134 @ v2.5.2).
    pub(crate) fn get_cpu_op(&self, fast_log_exp_pow: bool) -> Arc<dyn CpuOp> {
        get_cdl_cpu_renderer(self, fast_log_exp_pow)
    }
}

/// A CDL op holding `cdl`. Like upstream's constructor, it doesn't validate the data.
///
/// Port of `CDLOp::CDLOp` (src/OpenColorIO/ops/cdl/CDLOp.cpp:61-65 @ v2.5.2).
fn cdl_op(cdl: CdlOpData) -> Op {
    Op::new(OpData::Cdl(cdl))
}

/// Appends a CDL op with these parameters, validated, in the direction `direction`.
///
/// Port of `CreateCDLOp(OpRcPtrVec &, CDLOpData::Style, const double *, const double *,
/// const double *, double, TransformDirection)` (src/OpenColorIO/ops/cdl/CDLOp.cpp:151-168 @
/// v2.5.2). (CDLOp.h declares it with a `FormatMetadataImpl` parameter, which no definition
/// has.)
pub fn create_cdl_op_from_values(
    ops: &mut OpVec,
    style: CdlOpStyle,
    slope3: &[f64; 3],
    offset3: &[f64; 3],
    power3: &[f64; 3],
    saturation: f64,
    direction: TransformDirection,
) -> Result<()> {
    let cdl_data = CdlOpData::new(
        style,
        ChannelParams::new(slope3[0], slope3[1], slope3[2]),
        ChannelParams::new(offset3[0], offset3[1], offset3[2]),
        ChannelParams::new(power3[0], power3[1], power3[2]),
        saturation,
    )?;

    create_cdl_op(ops, cdl_data, direction);
    Ok(())
}

/// Appends a CDL op holding `cdl_data`, inverted ([`CdlOpData::inverse`]) when `direction` is
/// inverse. The data isn't validated; the format metadata is kept.
///
/// Upstream's op shares the caller's `CDLOpDataRcPtr` when `direction` is forward; here the op
/// owns the data, which nothing changes afterwards (`Op`'s docs).
///
/// Port of `CreateCDLOp(OpRcPtrVec &, CDLOpDataRcPtr &, TransformDirection)`
/// (src/OpenColorIO/ops/cdl/CDLOp.cpp:170-181 @ v2.5.2).
pub fn create_cdl_op(ops: &mut OpVec, cdl_data: CdlOpData, direction: TransformDirection) {
    let mut cdl = cdl_data;
    if direction == TransformDirection::Inverse {
        cdl = cdl.inverse();
    }

    ops.push_back(cdl_op(cdl));
}

#[cfg(test)]
#[path = "cdl_op_tests.rs"]
mod tests;
