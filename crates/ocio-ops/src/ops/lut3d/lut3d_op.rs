// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Lut3D op: a port of `src/OpenColorIO/ops/lut3d/Lut3DOp.h` and `Lut3DOp.cpp` @ v2.5.2:
//! the op's behaviors, [`create_lut3d_op`], [`generate_identity_lut3d`] and
//! [`get_3d_lut_edge_len_from_num_pixels`].
//!
//! As for every family, the op is its data, [`OpData::Lut3D`]: [`Op`]'s methods match on it
//! and call the methods here, `Lut3DOp`'s overrides. `getCPUOp` is [`Op::get_cpu_op`]'s arm,
//! `GetLut3DRenderer` (`ops::lut3d::lut3d_op_cpu`).
//!
//! `extractGpuShaderInfo` comes with the GPU writer (WP 2.2f); `CreateLut3DTransform` and
//! `BuildLut3DOp` are the `ocio` crate's, with the `Lut3DTransform` (WP 2.2c).

use super::lut3d_op_data::Lut3DOpData;
use crate::exception::{Exception, Result};
use crate::op::{Op, OpVec};
use crate::op_data::{OpData, OpDataType};
use crate::open_color_types::TransformDirection;

/// The index of the entry `(index_r, index_g, index_b)` of a LUT whose red coordinate changes
/// fastest, then green, then blue.
///
/// Port of `GetLut3DIndex_RedFast` (src/OpenColorIO/ops/lut3d/Lut3DOp.h:18-22 @ v2.5.2).
pub fn get_lut3d_index_red_fast(
    index_r: i32,
    index_g: i32,
    index_b: i32,
    size_r: i32,
    size_g: i32,
    _size_b: i32,
) -> i32 {
    3 * (index_r + size_r * (index_g + size_g * index_b))
}

/// The index of the entry `(index_r, index_g, index_b)` of a LUT whose blue coordinate changes
/// fastest, then green, then red.
///
/// Port of `GetLut3DIndex_BlueFast` (src/OpenColorIO/ops/lut3d/Lut3DOp.h:29-33 @ v2.5.2).
pub fn get_lut3d_index_blue_fast(
    index_r: i32,
    index_g: i32,
    index_b: i32,
    _size_r: i32,
    size_g: i32,
    size_b: i32,
) -> i32 {
    3 * (index_b + size_b * (index_g + size_g * index_r))
}

/// The order of a 3D LUT's entries: whether the red or the blue coordinate changes fastest.
///
/// Port of `enum Lut3DOrder` (src/OpenColorIO/ops/lut3d/Lut3DOp.h:40-44 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lut3DOrder {
    /// `LUT3DORDER_FAST_RED`
    FastRed = 0,
    /// `LUT3DORDER_FAST_BLUE`
    FastBlue,
}

/// Fills `img`, `edge_len^3` pixels of `num_channels` channels, with an identity 3D LUT in
/// `lut3d_order`: "Cannot generate idenitity 3d LUT with less than 3 channels." for fewer
/// channels. (Upstream returns at once for a null `img`; a slice is never null.)
///
/// Port of `GenerateIdentityLut3D` (src/OpenColorIO/ops/lut3d/Lut3DOp.cpp:25-57 @ v2.5.2).
pub fn generate_identity_lut3d(
    img: &mut [f32],
    edge_len: i32,
    num_channels: i32,
    lut3d_order: Lut3DOrder,
) -> Result<()> {
    if num_channels < 3 {
        return Err(Exception::new(
            "Cannot generate idenitity 3d LUT with less than 3 channels.",
        ));
    }

    let c = 1.0f32 / (edge_len as f32 - 1.0f32);

    let channels = num_channels as usize;
    match lut3d_order {
        Lut3DOrder::FastRed => {
            for i in 0..edge_len * edge_len * edge_len {
                let at = channels * i as usize;
                img[at] = (i % edge_len) as f32 * c;
                img[at + 1] = ((i / edge_len) % edge_len) as f32 * c;
                img[at + 2] = ((i / edge_len / edge_len) % edge_len) as f32 * c;
            }
        }
        Lut3DOrder::FastBlue => {
            for i in 0..edge_len * edge_len * edge_len {
                let at = channels * i as usize;
                img[at] = ((i / edge_len / edge_len) % edge_len) as f32 * c;
                img[at + 1] = ((i / edge_len) % edge_len) as f32 * c;
                img[at + 2] = (i % edge_len) as f32 * c;
            }
        }
    }
    Ok(())
}

/// The edge length of a 3D LUT of `num_pixels` entries, `roundf(powf(numPixels, 1/3))`:
/// "Cannot infer 3D LUT size. <n> element(s) does not correspond to a unform cube edge
/// length. (nearest edge length is <dim>)." if its cube isn't `num_pixels`.
///
/// Port of `Get3DLutEdgeLenFromNumPixels` (src/OpenColorIO/ops/lut3d/Lut3DOp.cpp:59-73 @
/// v2.5.2).
pub fn get_3d_lut_edge_len_from_num_pixels(num_pixels: i32) -> Result<i32> {
    let dim = (num_pixels as f32).powf(1.0f32 / 3.0f32).round() as i32;

    // (`int` products: a cube past `INT_MAX` wraps negative in both wheels, and never equals
    // a positive `numPixels`.)
    if dim.wrapping_mul(dim).wrapping_mul(dim) != num_pixels {
        return Err(Exception::new(format!(
            "Cannot infer 3D LUT size. {num_pixels} element(s) does not correspond to a unform \
             cube edge length. (nearest edge length is {dim})."
        )));
    }

    Ok(dim)
}

impl Lut3DOpData {
    /// A new Lut3D op with a copy of the data.
    ///
    /// Port of `Lut3DOp::clone` (src/OpenColorIO/ops/lut3d/Lut3DOp.cpp:127-131 @ v2.5.2).
    pub(crate) fn clone_op(&self) -> Op {
        Op::new(OpData::Lut3D(self.clone()))
    }

    /// `<Lut3DOp>`.
    ///
    /// Port of `Lut3DOp::getInfo` (Lut3DOp.cpp:133-136 @ v2.5.2).
    pub(crate) fn get_info(&self) -> &'static str {
        "<Lut3DOp>"
    }

    /// Whether `op` is a Lut3D op.
    ///
    /// Port of `Lut3DOp::isSameType` (Lut3DOp.cpp:138-141 @ v2.5.2).
    pub(crate) fn is_same_type(&self, op: &Op) -> bool {
        op.data().get_type() == OpDataType::Lut3D
    }

    /// Whether `op` is a Lut3D op whose LUT is this one in the other direction.
    ///
    /// Port of `Lut3DOp::isInverse` (Lut3DOp.cpp:143-153 @ v2.5.2).
    pub(crate) fn is_inverse_op(&self, op: &Op) -> bool {
        match &**op.data() {
            OpData::Lut3D(lut_data) => self.is_inverse(lut_data),
            _ => false,
        }
    }

    /// Whether `op` is a Lut3D op, which any Lut3D op may compose with.
    ///
    /// Port of `Lut3DOp::canCombineWith` (Lut3DOp.cpp:155-166 @ v2.5.2).
    pub(crate) fn can_combine_with(&self, op: &Op) -> bool {
        // TODO: Anything following the LUT 3D could be combined into it. Just need to be
        // careful about causing the interpolation to become worse.
        // TODO: drop a clampIdentity Range after an Inverse LUT.
        self.is_same_type(op)
    }

    /// Appends the composition of this LUT and `second_op`'s ([`Lut3DOpData::compose`]) to
    /// `ops`.
    ///
    /// Port of `Lut3DOp::combineWith` (Lut3DOp.cpp:168-180 @ v2.5.2).
    pub(crate) fn combine_with(&self, ops: &mut OpVec, second_op: &Op) -> Result<()> {
        if !self.can_combine_with(second_op) {
            return Err(Exception::new(
                "Lut3DOp: canCombineWith must be checked before calling combineWith.",
            ));
        }
        let OpData::Lut3D(second_lut) = &**second_op.data() else {
            unreachable!("canCombineWith holds for a Lut3D op only")
        };
        let composed = Lut3DOpData::compose(self, second_lut)?;
        ops.push_back(Op::new(OpData::Lut3D(composed)));
        Ok(())
    }

    /// The op's cache ID: `<Lut3D `, the data's cache ID, `>`.
    ///
    /// Port of `Lut3DOp::getCacheID` (Lut3DOp.cpp:187-195 @ v2.5.2).
    pub(crate) fn get_op_cache_id(&self) -> Vec<u8> {
        let mut cache_id = b"<Lut3D ".to_vec();
        cache_id.extend_from_slice(&self.get_cache_id());
        cache_id.push(b'>');
        cache_id
    }
}

/// Appends a Lut3D op holding `lut`, or its inverse when `direction` is inverse.
///
/// Upstream's op shares the caller's `Lut3DOpDataRcPtr` when `direction` is forward; here the
/// op owns the data (`Op`'s docs).
///
/// Port of `CreateLut3DOp` (src/OpenColorIO/ops/lut3d/Lut3DOp.cpp:223-233 @ v2.5.2).
pub fn create_lut3d_op(ops: &mut OpVec, lut: Lut3DOpData, direction: TransformDirection) {
    let mut lut_data = lut;

    if direction == TransformDirection::Inverse {
        lut_data = lut_data.inverse();
    }

    ops.push_back(Op::new(OpData::Lut3D(lut_data)));
}

#[cfg(test)]
#[path = "lut3d_op_tests.rs"]
mod tests;
