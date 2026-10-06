// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! 3D LUT op data: a port of `src/OpenColorIO/ops/lut3d/Lut3DOpData.h` and `Lut3DOpData.cpp`
//! @ v2.5.2: the LUT's array ([`Lut3DArray`], on [`Array`]), its interpolation, direction
//! and file output bit depth, and the queries the optimizer and the processor ask of it.
//!
//! Not here yet (WP 2.2e): `Compose` and `MakeFastLut3DFromInverse`.

use core::ffi::c_ulong;
use std::ops::{Index, IndexMut};

use crate::exception::{Exception, Result};
use crate::format_metadata::{FormatMetadataImpl, METADATA_ID};
use crate::hash_utils::cache_id_hash;
use crate::math_utils::sse_mul;
use crate::op_data::{OpData, OpDataType};
use crate::open_color_types::{
    BitDepth, TransformDirection, interpolation_to_string, transform_direction_to_string,
};
use crate::ops::op_array::Array;
use crate::ops::range::RangeOpData;

/// The interpolations, now in [`crate::open_color_types`]; re-exported where they were.
pub use crate::open_color_types::Interpolation;

/// The largest 3D LUT grid size. Port of `Max3DLUTLength` (src/OpenColorIO/LutLimits.h:15 @ v2.5.2).
pub const MAX_3D_LUT_LENGTH: u32 = 129;

/// The interpolation a renderer implements: tetrahedral for `INTERP_BEST` and
/// `INTERP_TETRAHEDRAL`, trilinear for the others. In OCIO v2, `INTERP_NEAREST` is trilinear;
/// `INTERP_UNKNOWN` is invalid and makes validation fail.
///
/// Port of `Lut3DOpData::GetConcreteInterpolation`
/// (src/OpenColorIO/ops/lut3d/Lut3DOpData.cpp:289-308 @ v2.5.2).
pub fn get_concrete_interpolation(interp: Interpolation) -> Interpolation {
    match interp {
        Interpolation::Best | Interpolation::Tetrahedral => Interpolation::Tetrahedral,

        Interpolation::Default
        | Interpolation::Linear
        | Interpolation::Cubic
        // NB: In OCIO v2, INTERP_NEAREST is implemented as trilinear,
        // this is a change from OCIO v1.
        | Interpolation::Nearest
        // NB: INTERP_UNKNOWN is not valid and will make validate() throw.
        | Interpolation::Unknown => Interpolation::Linear,
    }
}

/// The values of a 3D LUT: `length^3` RGB entries, the blue index changing fastest, then
/// green, then red ("Array order matches ctf order").
///
/// Port of `Lut3DOpData::Lut3DArray` (src/OpenColorIO/ops/lut3d/Lut3DOpData.h:95-123,
/// Lut3DOpData.cpp:155-249 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct Lut3DArray {
    array: Array,
}

impl Lut3DArray {
    /// An identity LUT of `length` entries per side: "LUT 3D: Grid size '<length>' must not be
    /// greater than '129'." over the limit.
    ///
    /// Port of `Lut3DArray::Lut3DArray(unsigned long)` (Lut3DOpData.cpp:155-159 @ v2.5.2).
    pub fn new(length: c_ulong) -> Result<Self> {
        let mut lut = Lut3DArray {
            array: Array::new(),
        };
        let max_color_components = lut.get_max_color_components();
        lut.resize(length, max_color_components)?;
        lut.fill();
        Ok(lut)
    }

    /// Makes the LUT an identity.
    ///
    /// Port of `Lut3DArray::fill` (Lut3DOpData.cpp:174-192 @ v2.5.2).
    fn fill(&mut self) {
        // Make an identity LUT.
        let length = self.get_length() as i64;
        let max_channels = self.get_max_color_components() as i64;

        let values = self.array.get_values_mut();

        let step_value = 1.0f32 / (length as f32 - 1.0f32);

        let max_entries = length * length * length;

        for idx in 0..max_entries {
            let at = (max_channels * idx) as usize;
            values[at] = ((idx / length / length) % length) as f32 * step_value;
            values[at + 1] = ((idx / length) % length) as f32 * step_value;
            values[at + 2] = (idx % length) as f32 * step_value;
        }
    }

    /// Sets the dimensions: "LUT 3D: Grid size '<length>' must not be greater than '129'."
    /// over the limit. The values are `length^3 * 3`, new ones 0.
    ///
    /// Port of `Lut3DArray::resize` (Lut3DOpData.cpp:194-204 @ v2.5.2).
    pub fn resize(&mut self, length: c_ulong, num_color_components: c_ulong) -> Result<()> {
        if length > c_ulong::from(MAX_3D_LUT_LENGTH) {
            return Err(Exception::new(format!(
                "LUT 3D: Grid size '{length}' must not be greater than '{MAX_3D_LUT_LENGTH}'."
            )));
        }
        let num_values = length * length * length * self.get_max_color_components();
        self.array.resize(length, num_color_components, num_values);
        Ok(())
    }

    /// `length^3 * 3`.
    ///
    /// Port of `Lut3DArray::getNumValues` (Lut3DOpData.cpp:206-210 @ v2.5.2).
    pub fn get_num_values(&self) -> c_ulong {
        let num_entries = self.get_length() * self.get_length() * self.get_length();
        num_entries * self.get_max_color_components()
    }

    /// The offset of entry `(i, j, k)`: red `i`, green `j`, blue `k`.
    fn offset(&self, i: usize, j: usize, k: usize) -> usize {
        let length = self.get_length() as usize;
        let max_channels = self.get_max_color_components() as usize;
        // Array order matches ctf order: channels vary most rapidly, then B, G, R.
        (i * length * length + j * length + k) * max_channels
    }

    /// The entry of red index `i`, green `j` and blue `k`.
    ///
    /// Port of `Lut3DArray::getRGB` (Lut3DOpData.cpp:212-222 @ v2.5.2).
    pub fn get_rgb(&self, i: usize, j: usize, k: usize) -> [f32; 3] {
        let offset = self.offset(i, j, k);
        let values = self.get_values();
        [values[offset], values[offset + 1], values[offset + 2]]
    }

    /// Sets the entry of red index `i`, green `j` and blue `k`.
    ///
    /// Port of `Lut3DArray::setRGB` (Lut3DOpData.cpp:224-234 @ v2.5.2).
    pub fn set_rgb(&mut self, i: usize, j: usize, k: usize, rgb: [f32; 3]) {
        let offset = self.offset(i, j, k);
        let values = self.get_values_mut();
        values[offset] = rgb[0];
        values[offset + 1] = rgb[1];
        values[offset + 2] = rgb[2];
    }

    /// Multiplies every value by `scale_factor`, unless it is 1.
    ///
    /// Port of `Lut3DArray::scale` (Lut3DOpData.cpp:236-249 @ v2.5.2).
    pub fn scale(&mut self, scale_factor: f32) {
        // Don't scale if scaleFactor = 1.0f.
        if scale_factor != 1.0f32 {
            for value in self.array.get_values_mut() {
                *value = sse_mul(*value, scale_factor);
            }
        }
    }

    /// Port of `ArrayT::getLength` (src/OpenColorIO/ops/OpArray.h:83-86 @ v2.5.2).
    pub fn get_length(&self) -> c_ulong {
        self.array.get_length()
    }

    /// Port of `ArrayT::getNumColorComponents` (OpArray.h:97-100 @ v2.5.2).
    pub fn get_num_color_components(&self) -> c_ulong {
        self.array.get_num_color_components()
    }

    /// Port of `ArrayT::getMaxColorComponents` (OpArray.h:139-142 @ v2.5.2).
    pub fn get_max_color_components(&self) -> c_ulong {
        self.array.get_max_color_components()
    }

    /// Port of `ArrayT::getValues() const` (OpArray.h:144-147 @ v2.5.2).
    pub fn get_values(&self) -> &Vec<f32> {
        self.array.get_values()
    }

    /// Port of `ArrayT::getValues()` (OpArray.h:149-152 @ v2.5.2).
    pub fn get_values_mut(&mut self) -> &mut Vec<f32> {
        self.array.get_values_mut()
    }

    /// Port of `ArrayT::validate` (OpArray.h:164-180 @ v2.5.2), with this array's number of
    /// values.
    pub fn validate(&self) -> Result<()> {
        self.array.validate(self.get_num_values())
    }

    /// Port of `ArrayT::operator==` (OpArray.h:182-188 @ v2.5.2).
    pub fn equals(&self, other: &Lut3DArray) -> bool {
        std::ptr::eq(self, other) || self.array.equals(&other.array)
    }
}

impl Index<usize> for Lut3DArray {
    type Output = f32;

    /// Port of `ArrayT::operator[] const` (OpArray.h:154-157 @ v2.5.2).
    fn index(&self, index: usize) -> &f32 {
        &self.array[index]
    }
}

impl IndexMut<usize> for Lut3DArray {
    /// Port of `ArrayT::operator[]` (OpArray.h:159-162 @ v2.5.2).
    fn index_mut(&mut self, index: usize) -> &mut f32 {
        &mut self.array[index]
    }
}

impl PartialEq for Lut3DArray {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}

/// A 3D LUT: its values, interpolation, direction and file output bit depth, with the format
/// metadata of `OpData`.
///
/// Port of `Lut3DOpData` (src/OpenColorIO/ops/lut3d/Lut3DOpData.h:24-136,
/// Lut3DOpData.cpp:251-497 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct Lut3DOpData {
    /// `OpData::m_metadata`.
    metadata: FormatMetadataImpl,
    /// `m_interpolation`.
    interpolation: Interpolation,
    /// `m_array`.
    array: Lut3DArray,
    /// `m_direction`.
    direction: TransformDirection,
    /// `m_fileOutBitDepth`: the out bit-depth to be used for file I/O.
    file_out_bit_depth: BitDepth,
}

impl Lut3DOpData {
    /// An identity LUT of `grid_size` entries per side, `INTERP_DEFAULT`, forward.
    ///
    /// Port of `Lut3DOpData::Lut3DOpData(unsigned long gridSize)` (Lut3DOpData.cpp:251-257 @
    /// v2.5.2).
    pub fn new(grid_size: c_ulong) -> Result<Self> {
        Self::with_all(
            Interpolation::Default,
            grid_size,
            TransformDirection::Forward,
        )
    }

    /// An identity LUT in `dir`.
    ///
    /// Port of `Lut3DOpData::Lut3DOpData(long gridSize, TransformDirection dir)`
    /// (Lut3DOpData.cpp:259-265 @ v2.5.2).
    pub fn with_direction(grid_size: c_ulong, dir: TransformDirection) -> Result<Self> {
        Self::with_all(Interpolation::Default, grid_size, dir)
    }

    /// An identity LUT with `interpolation`, forward.
    ///
    /// Port of `Lut3DOpData::Lut3DOpData(Interpolation, unsigned long gridSize)`
    /// (Lut3DOpData.cpp:267-273 @ v2.5.2).
    pub fn with_interpolation(interpolation: Interpolation, grid_size: c_ulong) -> Result<Self> {
        Self::with_all(interpolation, grid_size, TransformDirection::Forward)
    }

    fn with_all(
        interpolation: Interpolation,
        grid_size: c_ulong,
        direction: TransformDirection,
    ) -> Result<Self> {
        Ok(Lut3DOpData {
            metadata: FormatMetadataImpl::default(),
            interpolation,
            array: Lut3DArray::new(grid_size)?,
            direction,
            file_out_bit_depth: BitDepth::Unknown,
        })
    }

    /// Port of `Lut3DOpData::getInterpolation` (Lut3DOpData.h:41 @ v2.5.2).
    pub fn get_interpolation(&self) -> Interpolation {
        self.interpolation
    }

    /// Port of `Lut3DOpData::setInterpolation` (Lut3DOpData.cpp:279-282 @ v2.5.2).
    pub fn set_interpolation(&mut self, interpolation: Interpolation) {
        self.interpolation = interpolation;
    }

    /// The interpolation that has to be used ([`get_concrete_interpolation`]).
    ///
    /// Port of `Lut3DOpData::getConcreteInterpolation` (Lut3DOpData.cpp:284-287 @ v2.5.2).
    pub fn get_concrete_interpolation(&self) -> Interpolation {
        get_concrete_interpolation(self.interpolation)
    }

    /// Whether a 3D LUT supports the interpolation: all but `INTERP_CUBIC` and
    /// `INTERP_UNKNOWN`.
    ///
    /// Port of `Lut3DOpData::IsValidInterpolation` (Lut3DOpData.cpp:343-358 @ v2.5.2).
    pub fn is_valid_interpolation(interpolation: Interpolation) -> bool {
        match interpolation {
            Interpolation::Best
            | Interpolation::Tetrahedral
            | Interpolation::Default
            | Interpolation::Linear
            | Interpolation::Nearest => true,
            Interpolation::Cubic | Interpolation::Unknown => false,
        }
    }

    /// Port of `Lut3DOpData::getDirection` (Lut3DOpData.h:51 @ v2.5.2).
    pub fn get_direction(&self) -> TransformDirection {
        self.direction
    }

    /// Port of `Lut3DOpData::setDirection` (Lut3DOpData.h:52 @ v2.5.2).
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.direction = dir;
    }

    /// The values, blue fastest.
    ///
    /// Port of `Lut3DOpData::getArray() const` (Lut3DOpData.h:55 @ v2.5.2).
    pub fn get_array(&self) -> &Lut3DArray {
        &self.array
    }

    /// Port of `Lut3DOpData::getArray()` (Lut3DOpData.h:56 @ v2.5.2).
    pub fn get_array_mut(&mut self) -> &mut Lut3DArray {
        &mut self.array
    }

    /// Sets the values from `lut`, red fastest: "Lut3D length 'N * N * N * 3' does not match
    /// the vector size '<size>'." if it doesn't have `N^3 * 3` values.
    ///
    /// Port of `Lut3DOpData::setArrayFromRedFastestOrder` (Lut3DOpData.cpp:310-337 @ v2.5.2).
    pub fn set_array_from_red_fastest_order(&mut self, lut: &[f32]) -> Result<()> {
        let lut_array = &mut self.array;
        let lut_size = lut_array.get_length() as usize;

        if lut_size * lut_size * lut_size * 3 != lut.len() {
            return Err(Exception::new(format!(
                "Lut3D length '{lut_size} * {lut_size} * {lut_size} * 3' does not match the \
                 vector size '{}'.",
                lut.len()
            )));
        }

        for b in 0..lut_size {
            for g in 0..lut_size {
                for r in 0..lut_size {
                    // Lut3DOpData Array index. Blue changes fastest.
                    let blue_fast_idx = 3 * ((r * lut_size + g) * lut_size + b);

                    // Float array index. Red changes fastest.
                    let red_fast_idx = 3 * ((b * lut_size + g) * lut_size + r);

                    lut_array[blue_fast_idx] = lut[red_fast_idx];
                    lut_array[blue_fast_idx + 1] = lut[red_fast_idx + 1];
                    lut_array[blue_fast_idx + 2] = lut[red_fast_idx + 2];
                }
            }
        }
        Ok(())
    }

    /// The grid dimension `N` of the `N x N x N x 3` array.
    ///
    /// Port of `Lut3DOpData::getGridSize` (Lut3DOpData.h:62 @ v2.5.2).
    pub fn get_grid_size(&self) -> c_ulong {
        self.array.get_length()
    }

    /// Checks the interpolation and the array.
    ///
    /// Port of `Lut3DOpData::validate` (Lut3DOpData.cpp:360-398 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        if !Self::is_valid_interpolation(self.interpolation) {
            return Err(Exception::new(format!(
                "Lut3D does not support interpolation algorithm: {}.",
                interpolation_to_string(self.get_interpolation())
            )));
        }

        if let Err(e) = self.get_array().validate() {
            return Err(Exception::new(format!(
                "Lut3D content array issue: {}",
                e.message()
            )));
        }

        if self.get_array().get_num_color_components() != 3 {
            return Err(Exception::new(
                "Lut3D has an incorrect number of color components. ",
            ));
        }

        if self.get_array().get_length() > c_ulong::from(MAX_3D_LUT_LENGTH) {
            // This should never happen. Enforced by resize.
            return Err(Exception::new(format!(
                "Lut3D length: {} is not supported. ",
                self.get_array().get_length()
            )));
        }
        Ok(())
    }

    /// Port of `Lut3DOpData::getType` (Lut3DOpData.h:66 @ v2.5.2).
    pub fn get_type(&self) -> OpDataType {
        OpDataType::Lut3D
    }

    /// Never: a 3D LUT clamps to its domain.
    ///
    /// Port of `Lut3DOpData::isNoOp` (Lut3DOpData.cpp:400-404 @ v2.5.2).
    pub fn is_no_op(&self) -> bool {
        // 3D LUT is clamping to its domain
        false
    }

    /// Never.
    ///
    /// Port of `Lut3DOpData::isIdentity` (Lut3DOpData.cpp:406-409 @ v2.5.2).
    pub fn is_identity(&self) -> bool {
        false
    }

    /// Always.
    ///
    /// Port of `Lut3DOpData::hasChannelCrosstalk` (Lut3DOpData.h:72 @ v2.5.2).
    pub fn has_channel_crosstalk(&self) -> bool {
        true
    }

    /// A [0, 1] clamp.
    ///
    /// Port of `Lut3DOpData::getIdentityReplacement` (Lut3DOpData.cpp:411-414 @ v2.5.2).
    pub fn get_identity_replacement(&self) -> Result<OpData> {
        Ok(OpData::Range(RangeOpData::with_values(0., 1., 0., 1.)?))
    }

    /// The arrays only; the interpolation is not compared.
    ///
    /// Port of `Lut3DOpData::haveEqualBasics` (Lut3DOpData.cpp:416-420 @ v2.5.2).
    fn have_equal_basics(&self, other: &Lut3DOpData) -> bool {
        // TODO: Should interpolation style be considered?
        self.array == other.array
    }

    /// Whether `other` has the direction, the interpolation and the values. The metadata is
    /// ignored. The `OpData` base's type comparison is [`OpData::equals`]'s.
    ///
    /// Port of `Lut3DOpData::equals` (Lut3DOpData.cpp:422-435 @ v2.5.2).
    pub fn equals(&self, other: &Lut3DOpData) -> bool {
        // `OpData::equals`: the same object, or the same type.
        if std::ptr::eq(self, other) {
            return true;
        }

        if self.direction != other.direction || self.interpolation != other.interpolation {
            return false;
        }

        self.have_equal_basics(other)
    }

    /// Whether `other` is this LUT in the other direction (its values, whatever the
    /// interpolation).
    ///
    /// Port of `Lut3DOpData::isInverse` (Lut3DOpData.cpp:442-452 @ v2.5.2).
    pub fn is_inverse(&self, other: &Lut3DOpData) -> bool {
        if (self.direction == TransformDirection::Forward
            && other.direction == TransformDirection::Inverse)
            || (self.direction == TransformDirection::Inverse
                && other.direction == TransformDirection::Forward)
        {
            return self.have_equal_basics(other);
        }
        false
    }

    /// A copy in the other direction.
    ///
    /// Port of `Lut3DOpData::inverse` (Lut3DOpData.cpp:454-465 @ v2.5.2).
    pub fn inverse(&self) -> Lut3DOpData {
        let mut inv_lut = self.clone();

        inv_lut.direction = if self.direction == TransformDirection::Forward {
            TransformDirection::Inverse
        } else {
            TransformDirection::Forward
        };

        // Note that any existing metadata could become stale at this point but
        // trying to update it is also challenging since inverse() is sometimes
        // called even during the creation of new ops.
        inv_lut
    }

    /// The cache ID: the id, if any, and a space; the values' hash, the interpolation and the
    /// direction, each followed by a space.
    ///
    /// Port of `Lut3DOpData::getCacheID` (Lut3DOpData.cpp:467-487 @ v2.5.2). Upstream hashes
    /// `values.size() * sizeof(float)` bytes from `&values[0]`, none for an empty array.
    pub fn get_cache_id(&self) -> Vec<u8> {
        let values = self.get_array().get_values();

        let mut cache_id = Vec::new();
        if !self.get_id().is_empty() {
            cache_id.extend_from_slice(self.get_id());
            cache_id.push(b' ');
        }

        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_ne_bytes()).collect();
        let text = format!(
            "{} {} {} ",
            cache_id_hash(&bytes),
            interpolation_to_string(self.interpolation),
            transform_direction_to_string(self.direction)
        );
        cache_id.extend_from_slice(text.as_bytes());
        cache_id
    }

    /// Port of `Lut3DOpData::getFileOutputBitDepth` (Lut3DOpData.h:86 @ v2.5.2).
    pub fn get_file_output_bit_depth(&self) -> BitDepth {
        self.file_out_bit_depth
    }

    /// Port of `Lut3DOpData::setFileOutputBitDepth` (Lut3DOpData.h:87 @ v2.5.2).
    pub fn set_file_output_bit_depth(&mut self, out: BitDepth) {
        self.file_out_bit_depth = out;
    }

    /// Multiplies the values by `scale` ([`Lut3DArray::scale`]).
    ///
    /// Port of `Lut3DOpData::scale` (Lut3DOpData.cpp:489-492 @ v2.5.2).
    pub fn scale(&mut self, scale: f32) {
        self.array.scale(scale);
    }

    /// Port of `OpData::getFormatMetadata() const` (src/OpenColorIO/Op.h @ v2.5.2).
    pub fn get_format_metadata(&self) -> &FormatMetadataImpl {
        &self.metadata
    }

    /// Port of `OpData::getFormatMetadata()` (src/OpenColorIO/Op.h @ v2.5.2).
    pub fn get_format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        &mut self.metadata
    }

    /// The `id` attribute of the metadata.
    ///
    /// Port of `OpData::getID` (src/OpenColorIO/Op.cpp @ v2.5.2).
    pub fn get_id(&self) -> &[u8] {
        self.metadata.get_attribute_value_string(Some(METADATA_ID))
    }
}

/// Port of `operator==(const Lut3DOpData &, const Lut3DOpData &)` (Lut3DOpData.cpp:494-497
/// @ v2.5.2).
impl PartialEq for Lut3DOpData {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}

#[cfg(test)]
#[path = "lut3d_op_data_tests.rs"]
mod tests;
