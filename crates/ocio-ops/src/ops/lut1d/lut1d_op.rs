// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Lut1D op: a port of `src/OpenColorIO/ops/lut1d/Lut1DOp.h` and `Lut1DOp.cpp` @ v2.5.2,
//! what the optimizer's separable-prefix bake needs: the op's behaviors and
//! [`create_lut1d_op`].
//!
//! As for every family, the op is its data, [`OpData::Lut1D`]: [`Op`]'s methods match on it
//! and call the methods here, `Lut1DOp`'s overrides. Its `finalize` changes the op's data, so
//! it is [`Op::finalize`]'s own arm.
//!
//! `getCPUOp` is [`Op::get_cpu_op`]'s arm: `GetLut1DRenderer` for F32 to F32
//! (`ops::lut1d::lut1d_op_cpu`), whose inverse renderers are still to come
//! ([`NOT_PORTED_INVERSE_RENDERER`]).
//! Not here yet (Phase 2, WP 2.1g): `combineWith`, which composes two LUTs (`Compose`); until
//! then it returns [`NOT_PORTED_COMPOSE`]. `extractGpuShaderInfo` comes with the GPU writer
//! (WP 2.1h); `CreateLut1DTransform` and `BuildLut1DOp` are the `ocio` crate's, with the
//! `Lut1DTransform`. `GenerateLinearScaleLut1D` comes with the file readers that use it.

use super::lut1d_op_data::Lut1DOpData;
use crate::exception::{Exception, Result};
use crate::op::{Op, OpVec};
use crate::op_data::OpData;
use crate::open_color_types::TransformDirection;

/// The error of the inverse renderers until WP 2.1f.
pub const NOT_PORTED_INVERSE_RENDERER: &str =
    "Lut1D: the inverse 1D LUT renderers are not ported yet (Phase 2, WP 2.1f).";

/// The error of composing two 1D LUTs until Phase 2 (WP 2.5).
pub const NOT_PORTED_COMPOSE: &str =
    "Lut1D: composing 1D LUTs is not ported yet (Phase 2, WP 2.5).";

impl Lut1DOpData {
    /// A new Lut1D op with a copy of the data.
    ///
    /// Port of `Lut1DOp::clone` (src/OpenColorIO/ops/lut1d/Lut1DOp.cpp:70-74 @ v2.5.2).
    pub(crate) fn clone_op(&self) -> Op {
        Op::new(OpData::Lut1D(self.clone()))
    }

    /// Port of `Lut1DOp::getInfo` (Lut1DOp.cpp:76-79 @ v2.5.2).
    pub(crate) fn get_info(&self) -> &'static str {
        "<Lut1DOp>"
    }

    /// Whether `op` is a Lut1D op: what upstream's `DynamicPtrCast` to `Lut1DOp` finds.
    ///
    /// Port of `Lut1DOp::isSameType` (Lut1DOp.cpp:81-85 @ v2.5.2).
    pub(crate) fn is_same_type(&self, op: &Op) -> bool {
        matches!(**op.data(), OpData::Lut1D(_))
    }

    /// Whether `op` is a Lut1D op whose data is this one's inverse
    /// ([`Lut1DOpData::is_inverse`]).
    ///
    /// Port of `Lut1DOp::isInverse` (Lut1DOp.cpp:87-97 @ v2.5.2).
    pub(crate) fn is_inverse_op(&self, op: &Op) -> bool {
        match &**op.data() {
            OpData::Lut1D(lut_data) => self.is_inverse(lut_data),
            _ => false,
        }
    }

    /// Whether `op` is a Lut1D op this one may compose with (neither adjusts hue).
    ///
    /// Port of `Lut1DOp::canCombineWith` (Lut1DOp.cpp:99-110 @ v2.5.2).
    pub(crate) fn can_combine_with(&self, op: &Op) -> bool {
        match &**op.data() {
            OpData::Lut1D(lut_data) => self.may_compose(lut_data),
            // TODO: drop a clampIdentity Range after an Inverse LUT.
            _ => false,
        }
    }

    /// The composition of this LUT and `second_op`'s, which waits for `Compose` (Phase 2):
    /// [`NOT_PORTED_COMPOSE`]. Only two 1D LUTs combine, and Phase 1 makes one at most.
    ///
    /// Port of `Lut1DOp::combineWith` (Lut1DOp.cpp:112-128 @ v2.5.2), up to the composition.
    pub(crate) fn combine_with(&self, _ops: &mut OpVec, second_op: &Op) -> Result<()> {
        if !self.can_combine_with(second_op) {
            return Err(Exception::new(
                "Lut1DOp: canCombineWith must be checked before calling combineWith.",
            ));
        }
        Err(Exception::new(NOT_PORTED_COMPOSE))
    }

    /// The op's cache ID: `<Lut1D `, the data's cache ID, `>`.
    ///
    /// Port of `Lut1DOp::getCacheID` (Lut1DOp.cpp:140-149 @ v2.5.2).
    pub(crate) fn get_op_cache_id(&self) -> Result<Vec<u8>> {
        let mut cache_id = b"<Lut1D ".to_vec();
        cache_id.extend_from_slice(&self.get_cache_id()?);
        cache_id.push(b'>');
        Ok(cache_id)
    }
}

/// Appends a Lut1D op holding `lut`, or its inverse when `direction` is inverse.
///
/// Upstream's op shares the caller's `Lut1DOpDataRcPtr` when `direction` is forward; here the
/// op owns the data (`Op`'s docs).
///
/// Port of `CreateLut1DOp` (src/OpenColorIO/ops/lut1d/Lut1DOp.cpp:178-192 @ v2.5.2).
pub fn create_lut1d_op(ops: &mut OpVec, lut: Lut1DOpData, direction: TransformDirection) {
    // TODO: Detect if 1D LUT can be exactly approximated as y = mx + b If so, return a mtx
    // instead.

    let mut lut_data = lut;
    if direction == TransformDirection::Inverse {
        lut_data = lut_data.inverse();
    }

    ops.push_back(Op::new(OpData::Lut1D(lut_data)));
}

/// Fills `img`, `num_elements` pixels of `num_channels` channels, with an identity ramp from 0
/// to 1 on the first 3 channels (`scale * i`, in `float`).
///
/// Port of `GenerateIdentityLut1D` (src/OpenColorIO/ops/lut1d/Lut1DOp.cpp:194-207 @ v2.5.2).
pub fn generate_identity_lut1d(img: &mut [f32], num_elements: i32, num_channels: i32) {
    let num_channels_to_fill = std::cmp::min(3, num_channels);

    let scale = 1.0f32 / (num_elements as f32 - 1.0f32);
    for i in 0..num_elements {
        for c in 0..num_channels_to_fill {
            img[(num_channels * i + c) as usize] = scale * i as f32;
        }
    }
}

#[cfg(test)]
#[path = "lut1d_op_tests.rs"]
mod tests;
