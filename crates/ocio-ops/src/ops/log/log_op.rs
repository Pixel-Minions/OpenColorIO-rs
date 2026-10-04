// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Log op: a port of `src/OpenColorIO/ops/log/LogOp.h` and `LogOp.cpp` @ v2.5.2, what the
//! CPU engine needs: the op's behaviors, and the functions that create ops, [`create_log_op`],
//! [`create_log_op_from_parameters`] and [`create_log_op_from_base`]. `BuildLogOp` and
//! `CreateLogTransform` work on the Log transforms: they are in `ocio`
//! (crates/ocio/src/transforms/log_transform.rs);
//! `extractGpuShaderInfo` comes with the GPU writer (1.3l4).
//!
//! As for every family, the op is its data, [`OpData::Log`]: [`Op`]'s methods match on it and
//! call the methods here, `LogOp`'s overrides. `LogOp` keeps the `Op` defaults for the rest:
//! it never combines with another op, and its `finalize` does nothing (the renderers handle
//! both directions).

use std::sync::Arc;

use super::log_op_cpu::get_log_renderer;
use super::log_op_data::LogOpData;
use crate::exception::Result;
use crate::op::{CpuOp, Op, OpVec};
use crate::op_data::OpData;
use crate::open_color_types::TransformDirection;

impl LogOpData {
    /// A new Log op with a copy of the data ([`LogOpData::try_clone`]): channels of mixed
    /// styles are its error, "Cannot create Log op, all channels need to have the same
    /// style.". `create_log_op` doesn't validate a forward op's data, and the channel setters
    /// are public, so an op can hold such channels.
    ///
    /// Port of `LogOp::clone` (src/OpenColorIO/ops/log/LogOp.cpp:64-68 @ v2.5.2).
    pub(crate) fn clone_op(&self) -> Result<Op> {
        Ok(Op::new(OpData::Log(self.try_clone()?)))
    }

    /// Port of `LogOp::getInfo` (src/OpenColorIO/ops/log/LogOp.cpp:73-76 @ v2.5.2).
    pub(crate) fn get_info(&self) -> &'static str {
        "<LogOp>"
    }

    /// Whether `op` is a Log op: what upstream's `DynamicPtrCast` to `LogOp` finds.
    ///
    /// Port of `LogOp::isSameType` (src/OpenColorIO/ops/log/LogOp.cpp:78-83 @ v2.5.2).
    pub(crate) fn is_same_type(&self, op: &Op) -> bool {
        matches!(**op.data(), OpData::Log(_))
    }

    /// Whether `op` is a Log op whose data is this one's inverse
    /// ([`LogOpData::is_inverse`]: equal channels, the same parameters and base, the other
    /// direction).
    ///
    /// Port of `LogOp::isInverse` (src/OpenColorIO/ops/log/LogOp.cpp:85-92 @ v2.5.2).
    pub(crate) fn is_inverse_op(&self, op: &Op) -> bool {
        match &**op.data() {
            OpData::Log(log_op_data) => self.is_inverse(log_op_data),
            _ => false,
        }
    }

    /// The op's cache ID: `<LogOp `, the data's cache ID, `>`.
    ///
    /// Port of `LogOp::getCacheID` (src/OpenColorIO/ops/log/LogOp.cpp:94-103 @ v2.5.2).
    pub(crate) fn get_op_cache_id(&self) -> Result<Vec<u8>> {
        let mut cache_id = b"<LogOp ".to_vec();
        cache_id.extend_from_slice(&self.get_cache_id()?);
        cache_id.push(b'>');
        Ok(cache_id)
    }

    /// The renderer for the log, with the fast approximations of `log2` and `exp2` when
    /// `fast_log_exp_pow`. Channels with fewer parameters than the renderer reads are an error
    /// ([`get_log_renderer`], U-20).
    ///
    /// Port of `LogOp::getCPUOp` (src/OpenColorIO/ops/log/LogOp.cpp:105-109 @ v2.5.2).
    pub(crate) fn get_cpu_op(&self, fast_log_exp_pow: bool) -> Result<Arc<dyn CpuOp>> {
        get_log_renderer(self, fast_log_exp_pow)
    }
}

/// Appends an affine Log op: `logSlope * log(linSlope * x + linOffset, base) + logOffset`
/// per channel, in the direction `direction` (forward is lin to log). Nothing is validated.
///
/// Port of `CreateLogOp(OpRcPtrVec &, double, const double(&)[3] x4, TransformDirection)`
/// (src/OpenColorIO/ops/log/LogOp.cpp:119-131 @ v2.5.2).
pub fn create_log_op_from_parameters(
    ops: &mut OpVec,
    base: f64,
    log_slope: &[f64; 3],
    log_offset: &[f64; 3],
    lin_slope: &[f64; 3],
    lin_offset: &[f64; 3],
    direction: TransformDirection,
) {
    let op_data = LogOpData::with_parameters(
        base, log_slope, log_offset, lin_slope, lin_offset, direction,
    );
    ops.push_back(Op::new(OpData::Log(op_data)));
}

/// Appends a plain Log op in `base`, in the direction `direction`. Nothing is validated.
///
/// Port of `CreateLogOp(OpRcPtrVec &, double, TransformDirection)`
/// (src/OpenColorIO/ops/log/LogOp.cpp:133-137 @ v2.5.2).
pub fn create_log_op_from_base(ops: &mut OpVec, base: f64, direction: TransformDirection) {
    let op_data = LogOpData::new(base, direction);
    ops.push_back(Op::new(OpData::Log(op_data)));
}

/// Appends a Log op holding `log_data`, or its inverse ([`LogOpData::inverse`], which
/// validates it) when `direction` is inverse. A forward op's data isn't validated.
///
/// Upstream's op shares the caller's `LogOpDataRcPtr` when `direction` is forward; here the op
/// owns the data, which nothing changes afterwards (`Op`'s docs).
///
/// Port of `CreateLogOp(OpRcPtrVec &, LogOpDataRcPtr &, TransformDirection)`
/// (src/OpenColorIO/ops/log/LogOp.cpp:139-150 @ v2.5.2).
pub fn create_log_op(
    ops: &mut OpVec,
    log_data: LogOpData,
    direction: TransformDirection,
) -> Result<()> {
    let mut log = log_data;
    if direction == TransformDirection::Inverse {
        log = log.inverse()?;
    }

    ops.push_back(Op::new(OpData::Log(log)));
    Ok(())
}

#[cfg(test)]
#[path = "log_op_tests.rs"]
mod tests;
