// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Gamma op: part of a port of `src/OpenColorIO/ops/gamma/GammaOp.h` and `GammaOp.cpp` @
//! v2.5.2, what the CPU engine needs: the op's behaviors, and [`create_gamma_op`].
//! `CreateGammaTransform`, `BuildExponentWithLinearOp` and `BuildExponentOp` work on the
//! transforms, and come with them (WP 1.8); `extractGpuShaderInfo` comes with the GPU writer
//! (1.3g4).
//!
//! As for every family, the op is its data, [`OpData::Gamma`]: [`Op`]'s methods match on it
//! and call the methods here, `GammaOp`'s overrides. `GammaOp` keeps the `Op` defaults for
//! `finalize` (nothing: a reverse style renders as it is) and the queries it doesn't override.

use std::sync::Arc;

use super::gamma_op_cpu::get_gamma_renderer;
use super::gamma_op_data::GammaOpData;
use crate::exception::{Exception, Result};
use crate::op::{CpuOp, Op, OpVec};
use crate::op_data::OpData;
use crate::open_color_types::TransformDirection;

impl GammaOpData {
    /// A new Gamma op with a copy of the data.
    ///
    /// Port of `GammaOp::clone` (src/OpenColorIO/ops/gamma/GammaOp.cpp:70-74 @ v2.5.2).
    pub(crate) fn clone_op(&self) -> Op {
        gamma_op(self.clone())
    }

    /// Port of `GammaOp::getInfo` (GammaOp.cpp:65-68 @ v2.5.2).
    pub(crate) fn get_info(&self) -> &'static str {
        "<GammaOp>"
    }

    /// Whether `op` is a Gamma op: what upstream's `DynamicPtrCast` to `GammaOp` finds.
    ///
    /// Port of `GammaOp::isSameType` (GammaOp.cpp:76-80 @ v2.5.2).
    pub(crate) fn is_same_type(&self, op: &Op) -> bool {
        matches!(**op.data(), OpData::Gamma(_))
    }

    /// Whether `op` is a Gamma op whose data undoes this one's
    /// ([`GammaOpData::is_inverse`]).
    ///
    /// Port of `GammaOp::isInverse` (GammaOp.cpp:82-88 @ v2.5.2).
    pub(crate) fn is_inverse_op(&self, op: &Op) -> bool {
        match &**op.data() {
            OpData::Gamma(gamma2) => self.is_inverse(gamma2),
            OpData::Matrix(_)
            | OpData::Range(_)
            | OpData::Exponent(_)
            | OpData::Reference(_)
            | OpData::NoOp(_) => false,
        }
    }

    /// Whether this op combines with `op`: a Gamma op whose style
    /// [`may_compose`](GammaOpData::may_compose) accepts.
    ///
    /// Port of `GammaOp::canCombineWith` (GammaOp.cpp:90-94 @ v2.5.2).
    pub(crate) fn can_combine_with(&self, op: &Op) -> bool {
        match &**op.data() {
            OpData::Gamma(gamma2) => self.may_compose(gamma2),
            OpData::Matrix(_)
            | OpData::Range(_)
            | OpData::Exponent(_)
            | OpData::Reference(_)
            | OpData::NoOp(_) => false,
        }
    }

    /// Appends to `ops` the Gamma op that does what this one then `second_op` does: the
    /// composition ([`compose`](GammaOpData::compose)), forward.
    ///
    /// Port of `GammaOp::combineWith` (GammaOp.cpp:96-107 @ v2.5.2).
    pub(crate) fn combine_with(&self, ops: &mut OpVec, second_op: &Op) -> Result<()> {
        if !self.can_combine_with(second_op) {
            return Err(Exception::new(
                "GammaOp: canCombineWith must be checked before calling combineWith.",
            ));
        }

        let OpData::Gamma(gamma2) = &**second_op.data() else {
            unreachable!("can_combine_with accepts Gamma ops only")
        };
        let res = self.compose(gamma2)?;
        create_gamma_op(ops, res, TransformDirection::Forward);
        Ok(())
    }

    /// The op's cache ID: `<GammaOp `, the data's cache ID (which ends with a space), ` >`.
    ///
    /// Port of `GammaOp::getCacheID` (GammaOp.cpp:109-118 @ v2.5.2).
    pub(crate) fn get_op_cache_id(&self) -> Result<Vec<u8>> {
        let mut cache_id = b"<GammaOp ".to_vec();
        cache_id.extend_from_slice(&self.get_cache_id()?);
        cache_id.extend_from_slice(b" >");
        Ok(cache_id)
    }

    /// The renderer for the style: the fast-math one when `fast_log_exp_pow`.
    ///
    /// Port of `GammaOp::getCPUOp` (GammaOp.cpp:120-124 @ v2.5.2).
    pub(crate) fn get_cpu_op(&self, fast_log_exp_pow: bool) -> Result<Arc<dyn CpuOp>> {
        get_gamma_renderer(self, fast_log_exp_pow)
    }
}

/// A Gamma op holding `gamma`. Unlike the Matrix and Range ops, it doesn't validate the data:
/// the builders do (`BuildExponentOp`, `BuildExponentWithLinearOp`, GammaOp.cpp:179-216), and
/// a processor validates its ops when it finalizes them.
///
/// Port of `GammaOp::GammaOp` (src/OpenColorIO/ops/gamma/GammaOp.cpp:55-59 @ v2.5.2).
fn gamma_op(gamma: GammaOpData) -> Op {
    Op::new(OpData::Gamma(gamma))
}

/// Appends a Gamma op holding `gamma_data`, inverted ([`GammaOpData::inverse`]) when
/// `direction` is inverse. The data isn't validated.
///
/// Upstream's op shares the caller's `GammaOpDataRcPtr` when `direction` is forward; here the
/// op owns the data, which nothing changes afterwards (`Op`'s docs).
///
/// Port of `CreateGammaOp` (src/OpenColorIO/ops/gamma/GammaOp.cpp:134-145 @ v2.5.2).
pub fn create_gamma_op(ops: &mut OpVec, gamma_data: GammaOpData, direction: TransformDirection) {
    let mut gamma = gamma_data;
    if direction == TransformDirection::Inverse {
        gamma = gamma.inverse();
    }

    ops.push_back(gamma_op(gamma));
}

#[cfg(test)]
#[path = "gamma_op_tests.rs"]
mod tests;
