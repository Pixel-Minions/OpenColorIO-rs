// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! 1D LUT op data: a port of `src/OpenColorIO/ops/lut1d/Lut1DOpData.h` and `Lut1DOpData.cpp`
//! @ v2.5.2, with the inverse LUT's set-up (`initializeFromForward`, the component properties)
//! and `getPairIdentityReplacement`.
//!
//! Not here yet (WP 2.1g): `Compose`, `MakeFastLut1DFromInverse`. The optimizer's bake makes
//! forward LUTs, with [`Lut1DOpData::compose_vec`].

use core::ffi::c_ulong;

use crate::bit_depth_utils::{get_bit_depth_max_value, is_float_bit_depth};
use crate::exception::{Exception, Result};
use crate::format_metadata::{FormatMetadataImpl, METADATA_ID};
use crate::hash_utils::cache_id_hash;
use crate::imath_half::{float_to_half, half_to_float};
use crate::math_utils::halfs_differ;
use crate::op::OpVec;
use crate::op_data::{OpData, OpDataType};
use crate::open_color_types::{
    BitDepth, Lut1DHueAdjust, TransformDirection, bit_depth_to_string, interpolation_to_string,
    transform_direction_to_string,
};
use crate::ops::lut3d::lut3d_op_data::Interpolation;
use crate::ops::matrix::MatrixOpData;
use crate::ops::op_array::Array;
use crate::ops::op_tools::eval_transform;
use crate::ops::range::RangeOpData;

/// Number of possible values for the Half domain.
///
/// Port of `HALF_DOMAIN_REQUIRED_ENTRIES` (src/OpenColorIO/ops/lut1d/Lut1DOpData.cpp:21 @
/// v2.5.2).
const HALF_DOMAIN_REQUIRED_ENTRIES: c_ulong = 65536;

/// How a 1D LUT's indices and values are encoded: bit flags.
///
/// Port of `Lut1DOpData::HalfFlags` (src/OpenColorIO/ops/lut1d/Lut1DOpData.h:39-49 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HalfFlags(pub u32);

impl HalfFlags {
    /// `LUT_STANDARD`: indices and values use standard encoding.
    pub const STANDARD: HalfFlags = HalfFlags(0x00);
    /// `LUT_INPUT_HALF_CODE`: the LUT indices are half float codes.
    pub const INPUT_HALF_CODE: HalfFlags = HalfFlags(0x01);
    /// `LUT_OUTPUT_HALF_CODE`: the LUT values are half float codes.
    pub const OUTPUT_HALF_CODE: HalfFlags = HalfFlags(0x02);
    /// `LUT_INPUT_OUTPUT_HALF_CODE`: indices and values are half float codes.
    pub const INPUT_OUTPUT_HALF_CODE: HalfFlags = HalfFlags(0x03);
}

/// Whether the flags have half float codes as the LUT indices.
///
/// Port of `Lut1DOpData::IsInputHalfDomain` (Lut1DOpData.h:153-157 @ v2.5.2).
pub fn is_input_half_domain_flags(half_flags: HalfFlags) -> bool {
    (half_flags.0 & HalfFlags::INPUT_HALF_CODE.0) == HalfFlags::INPUT_HALF_CODE.0
}

/// What an inverse LUT's renderer needs to know of one channel of the LUT.
///
/// Port of `Lut1DOpData::ComponentProperties` (src/OpenColorIO/ops/lut1d/Lut1DOpData.h:41-56 @
/// v2.5.2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ComponentProperties {
    /// Represents the overall increasing state.
    pub is_increasing: bool,
    /// Is the lowest index such that LUT[start] != LUT[start+1].
    pub start_domain: c_ulong,
    /// Is the highest index such that LUT[end-1] != LUT[end].
    pub end_domain: c_ulong,
    /// StartDomain for half-domain negative values.
    pub neg_start_domain: c_ulong,
    /// EndDomain for half-domain negative values.
    pub neg_end_domain: c_ulong,
}

/// The values of a 1D LUT: `length` RGB entries, three values each, whatever the number of
/// color components (1 when the three channels are equal).
///
/// Port of `Lut1DOpData::Lut3by1DArray` (src/OpenColorIO/ops/lut1d/Lut1DOpData.h:236-262,
/// Lut1DOpData.cpp:23-180 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct Lut3by1DArray {
    array: Array,
}

impl Lut3by1DArray {
    /// An identity LUT of `length` entries: "LUT 1D length needs to be at least 2." for a
    /// shorter one, "LUT 1D channels needs to be 1 or 3." for other channel counts. With
    /// `filter_nans`, the NaN codes of a half domain map to 0.
    ///
    /// Port of `Lut3by1DArray::Lut3by1DArray(HalfFlags, unsigned long, unsigned long, bool)`
    /// (Lut1DOpData.cpp:23-39 @ v2.5.2).
    pub fn new(
        half_flags: HalfFlags,
        num_channels: c_ulong,
        length: c_ulong,
        filter_nans: bool,
    ) -> Result<Self> {
        if length < 2 {
            return Err(Exception::new("LUT 1D length needs to be at least 2."));
        }
        if num_channels != 1 && num_channels != 3 {
            return Err(Exception::new("LUT 1D channels needs to be 1 or 3."));
        }

        let mut lut = Lut3by1DArray {
            array: Array::new(),
        };
        lut.resize(length, num_channels)?;
        lut.fill(half_flags, filter_nans);
        Ok(lut)
    }

    /// The values of an identity LUT: each half code's value for a half domain (0 for a NaN
    /// code with `filter_nans`), else `idx / (dim - 1)` in `float`. It writes the first
    /// `getNumColorComponents()` values of each row of that many values, as upstream does.
    ///
    /// Port of `Lut3by1DArray::fill` (Lut1DOpData.cpp:45-85 @ v2.5.2).
    fn fill(&mut self, half_flags: HalfFlags, filter_nans: bool) {
        let dim = self.get_length() as usize;
        let max_channels = self.get_num_color_components() as usize;

        let values = self.array.get_values_mut();
        if is_input_half_domain_flags(half_flags) {
            for idx in 0..dim {
                let mut ftemp = half_to_float(idx as u16);
                if ftemp.is_nan() && filter_nans {
                    ftemp = 0.0;
                }

                let row = max_channels * idx;
                for channel in 0..max_channels {
                    values[channel + row] = ftemp;
                }
            }
        } else {
            let step_value = 1.0f32 / (dim as f32 - 1.0f32);

            for idx in 0..dim {
                let ftemp = idx as f32 * step_value;

                let row = max_channels * idx;
                for channel in 0..max_channels {
                    values[channel + row] = ftemp;
                }
            }
        }
    }

    /// Sets the dimensions: "LUT 1D length needs to be at least 2." under 2, "LUT 1D: Length
    /// '<length>' must not be greater than 1024x1024 (1048576)." over it. The values are
    /// `length * 3`, new ones 0.
    ///
    /// Port of `Lut3by1DArray::resize` (Lut1DOpData.cpp:87-101 @ v2.5.2).
    pub fn resize(&mut self, length: c_ulong, num_color_components: c_ulong) -> Result<()> {
        if length < 2 {
            return Err(Exception::new("LUT 1D length needs to be at least 2."));
        } else if length > 1024 * 1024 {
            return Err(Exception::new(format!(
                "LUT 1D: Length '{length}' must not be greater than 1024x1024 (1048576)."
            )));
        }
        let num_values = length * self.get_max_color_components();
        self.array.resize(length, num_color_components, num_values);
        Ok(())
    }

    /// `length * 3`: three values per entry, whatever the number of color components.
    ///
    /// Port of `Lut3by1DArray::getNumValues` (Lut1DOpData.cpp:103-106 @ v2.5.2).
    pub fn get_num_values(&self) -> c_ulong {
        self.get_length() * self.get_max_color_components()
    }

    /// Whether the LUT is an identity: for a half domain, each value as a half within one step
    /// of its code's; else each value within 1e-5 of `idx / (dim - 1)`, so that a NaN value
    /// passes.
    ///
    /// Port of `Lut3by1DArray::isIdentity` (Lut1DOpData.cpp:108-180 @ v2.5.2).
    pub fn is_identity(&self, half_flags: HalfFlags) -> bool {
        // An identity LUT does nothing except possibly bit-depth conversion.
        let dim = self.get_length() as usize;
        let values = self.get_values();
        let max_channels = self.get_max_color_components() as usize;

        if is_input_half_domain_flags(half_flags) {
            for idx in 0..dim {
                let aim_half = half::f16::from_bits(idx as u16);
                let row = max_channels * idx;
                for channel in 0..max_channels {
                    let val_half = half::f16::from_bits(float_to_half(values[channel + row]));
                    // Must be different by at least two ULPs to not be an identity.
                    if halfs_differ(aim_half, val_half, 1) {
                        return false;
                    }
                }
            }
        } else {
            const ABS_TOL: f32 = 1e-5;
            let step_value = 1.0f32 / (dim as f32 - 1.0f32);

            for idx in 0..dim {
                let aim = idx as f32 * step_value;

                let row = max_channels * idx;
                for channel in 0..max_channels {
                    let err = values[channel + row] - aim;

                    if err.abs() > ABS_TOL {
                        return false;
                    }
                }
            }
        }

        true
    }

    /// Port of `ArrayT::getLength` (src/OpenColorIO/ops/OpArray.h:83-86 @ v2.5.2).
    pub fn get_length(&self) -> c_ulong {
        self.array.get_length()
    }

    /// Port of `ArrayT::getNumColorComponents` (OpArray.h:97-100 @ v2.5.2).
    pub fn get_num_color_components(&self) -> c_ulong {
        self.array.get_num_color_components()
    }

    /// Port of `ArrayT::setNumColorComponents` (OpArray.h:102-109 @ v2.5.2).
    pub fn set_num_color_components(&mut self, num_color_components: c_ulong) {
        let num_values = self.get_length() * self.get_max_color_components();
        self.array
            .set_num_color_components(num_color_components, num_values);
    }

    /// Port of `ArrayT::getMaxColorComponents` (OpArray.h:139-142 @ v2.5.2).
    pub fn get_max_color_components(&self) -> c_ulong {
        self.array.get_max_color_components()
    }

    /// Port of `ArrayT::adjustColorComponentNumber` (OpArray.h:111-137 @ v2.5.2).
    pub fn adjust_color_component_number(&mut self) {
        self.array.adjust_color_component_number();
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

    /// Port of `ArrayT::scale` (OpArray.h:190-200 @ v2.5.2).
    pub fn scale(&mut self, scale: f32) {
        self.array.scale(scale);
    }

    /// Port of `ArrayT::operator==` (OpArray.h:182-188 @ v2.5.2).
    pub fn equals(&self, other: &Lut3by1DArray) -> bool {
        std::ptr::eq(self, other) || self.array.equals(&other.array)
    }
}

impl std::ops::Index<usize> for Lut3by1DArray {
    type Output = f32;

    /// Port of `ArrayT::operator[] const` (OpArray.h:154-157 @ v2.5.2).
    fn index(&self, index: usize) -> &f32 {
        &self.array[index]
    }
}

impl std::ops::IndexMut<usize> for Lut3by1DArray {
    /// Port of `ArrayT::operator[]` (OpArray.h:159-162 @ v2.5.2).
    fn index_mut(&mut self, index: usize) -> &mut f32 {
        &mut self.array[index]
    }
}

impl PartialEq for Lut3by1DArray {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}

/// The 1D LUT op's data.
///
/// Port of `Lut1DOpData` (src/OpenColorIO/ops/lut1d/Lut1DOpData.h:30-276 @ v2.5.2). `Clone` is
/// upstream's `clone()`, a copy (Lut1DOpData.cpp:562-568).
#[derive(Debug, Clone)]
pub struct Lut1DOpData {
    /// The `OpData` base's `m_metadata`.
    metadata: FormatMetadataImpl,
    interpolation: Interpolation,
    array: Lut3by1DArray,
    half_flags: HalfFlags,
    hue_adjust: Lut1DHueAdjust,
    direction: TransformDirection,
    /// `m_componentProperties`: for an inverse LUT, set by `finalize`.
    component_properties: [ComponentProperties; 3],
    /// `m_fileOutBitDepth`.
    file_out_bit_depth: BitDepth,
}

impl Lut1DOpData {
    /// An identity LUT of `dimension` entries, standard domain, forward.
    ///
    /// Port of `Lut1DOpData::Lut1DOpData(unsigned long)` (Lut1DOpData.cpp:182-190 @ v2.5.2).
    pub fn new(dimension: c_ulong) -> Result<Self> {
        Self::with_direction(dimension, TransformDirection::Forward)
    }

    /// An identity LUT of `dimension` entries, standard domain, in the direction `dir`.
    ///
    /// Port of `Lut1DOpData::Lut1DOpData(unsigned long, TransformDirection)`
    /// (Lut1DOpData.cpp:192-200 @ v2.5.2).
    pub fn with_direction(dimension: c_ulong, dir: TransformDirection) -> Result<Self> {
        Ok(Lut1DOpData {
            metadata: FormatMetadataImpl::default(),
            interpolation: Interpolation::Default,
            array: Lut3by1DArray::new(HalfFlags::STANDARD, 3, dimension, false)?,
            half_flags: HalfFlags::STANDARD,
            hue_adjust: Lut1DHueAdjust::None,
            direction: dir,
            component_properties: [ComponentProperties::default(); 3],
            file_out_bit_depth: BitDepth::Unknown,
        })
    }

    /// An identity LUT of `dimension` entries with the encoding `half_flags`, forward; with
    /// `filter_nans`, the NaN codes of a half domain map to 0.
    ///
    /// Port of `Lut1DOpData::Lut1DOpData(HalfFlags, unsigned long, bool)`
    /// (Lut1DOpData.cpp:202-210 @ v2.5.2).
    pub fn with_half_flags(
        half_flags: HalfFlags,
        dimension: c_ulong,
        filter_nans: bool,
    ) -> Result<Self> {
        Ok(Lut1DOpData {
            metadata: FormatMetadataImpl::default(),
            interpolation: Interpolation::Default,
            array: Lut3by1DArray::new(half_flags, 3, dimension, filter_nans)?,
            half_flags,
            hue_adjust: Lut1DHueAdjust::None,
            direction: TransformDirection::Forward,
            component_properties: [ComponentProperties::default(); 3],
            file_out_bit_depth: BitDepth::Unknown,
        })
    }

    /// The domain of a LUT that a renderer can look up with `incoming_depth` values: a
    /// standard domain of one entry per code for integer depths, a half domain for 16f (and
    /// 32f, "even though a pure lookup wouldn't be appropriate"). NaN codes map to 0.
    ///
    /// Port of `Lut1DOpData::MakeLookupDomain` (Lut1DOpData.cpp:508-526 @ v2.5.2).
    pub fn make_lookup_domain(incoming_depth: BitDepth) -> Result<Lut1DOpData> {
        // For integer in-depths, we need a standard domain.
        let mut domain_type = HalfFlags::STANDARD;

        // For 16f in-depth, we need a half domain.
        // (Return same for 32f, even though a pure lookup wouldn't be appropriate.)
        if is_float_bit_depth(incoming_depth)? {
            domain_type = HalfFlags::INPUT_HALF_CODE;
        }

        let ideal_size = Self::get_lut_ideal_size_for(incoming_depth, domain_type)?;
        // Note that in this case the domainType is always appropriate for the incomingDepth, so
        // it should be safe to rely on the constructor and fill() to always return the correct
        // length. (E.g., we don't need to worry about 10i with a half domain.)
        Lut1DOpData::with_half_flags(domain_type, ideal_size, true)
    }

    /// Renders the LUT's own entries, its domain, through `ops` at F32 into its values, which
    /// become three per entry: the composition of the LUT and the ops. "There is nothing to
    /// compose the 1D LUT with" without ops. The ops must be separable, and the LUT a suitable
    /// domain: unlike `Compose`, it doesn't resample. The hue adjust and the bypass are the
    /// caller's.
    ///
    /// Port of `Lut1DOpData::ComposeVec` (src/OpenColorIO/ops/lut1d/Lut1DOpData.cpp:683-705
    /// @ v2.5.2).
    pub fn compose_vec(lut: &mut Lut1DOpData, ops: &mut OpVec) -> Result<()> {
        if ops.is_empty() {
            return Err(Exception::new(
                "There is nothing to compose the 1D LUT with",
            ));
        }

        // Set up so that the eval directly fills in the array of the result LUT.

        let num_pixels = lut.get_array().get_length();

        // TODO: Could keep it one channel in some cases.
        lut.get_array_mut().resize(num_pixels, 3)?;
        let in_values = lut.get_array().get_values().clone();

        // Evaluate the transforms at 32f.
        // Note: If any ops are bypassed, that will be respected here.
        eval_transform(
            &in_values,
            lut.get_array_mut().get_values_mut(),
            num_pixels as usize,
            ops,
        )
    }

    /// The number of entries a lookup needs for the bit depth: one per code for integer
    /// depths, 65536 for 16f and 32f; "Bit-depth is not supported: <depth>" for the others.
    ///
    /// Port of `Lut1DOpData::GetLutIdealSize(BitDepth)` (Lut1DOpData.cpp:437-469 @ v2.5.2).
    pub fn get_lut_ideal_size(incoming_bit_depth: BitDepth) -> Result<c_ulong> {
        // Return the number of entries needed in order to do a lookup for the specified
        // bit-depth.

        // For 32f, a look-up is impractical so in that case return 64k.
        match incoming_bit_depth {
            BitDepth::Uint8
            | BitDepth::Uint10
            | BitDepth::Uint12
            | BitDepth::Uint14
            | BitDepth::Uint16 => {
                Ok((get_bit_depth_max_value(incoming_bit_depth)? + 1.0) as c_ulong)
            }
            BitDepth::F16 | BitDepth::F32 => Ok(65536),
            BitDepth::Unknown | BitDepth::Uint32 => Err(Exception::new(format!(
                "Bit-depth is not supported: {}",
                bit_depth_to_string(incoming_bit_depth)
            ))),
        }
    }

    /// The number of entries `fill` expects for an identity LUT: always 65536 for a half
    /// domain, else [`get_lut_ideal_size`](Self::get_lut_ideal_size).
    ///
    /// Port of `Lut1DOpData::GetLutIdealSize(BitDepth, HalfFlags)` (Lut1DOpData.cpp:471-489
    /// @ v2.5.2).
    fn get_lut_ideal_size_for(input_bit_depth: BitDepth, half_flags: HalfFlags) -> Result<c_ulong> {
        // For half domain always return 65536, since that is what fill() expects. However note
        // that if the inputBitDepth is, e.g. 10i, this might not be the number of entries
        // required for a look-up.
        let size = HALF_DOMAIN_REQUIRED_ENTRIES;

        if is_input_half_domain_flags(half_flags) {
            return Ok(size);
        }

        Self::get_lut_ideal_size(input_bit_depth)
    }

    /// Port of `Lut1DOpData::getInterpolation` (Lut1DOpData.h:96 @ v2.5.2).
    pub fn get_interpolation(&self) -> Interpolation {
        self.interpolation
    }

    /// The interpolation the renderers implement: always linear (nearest is rendered linear,
    /// and invalid styles make `validate` fail).
    ///
    /// Port of `Lut1DOpData::getConcreteInterpolation` and `GetConcreteInterpolation`
    /// (Lut1DOpData.cpp:216-231 @ v2.5.2).
    pub fn get_concrete_interpolation(&self) -> Interpolation {
        Interpolation::Linear
    }

    /// Port of `Lut1DOpData::setInterpolation` (Lut1DOpData.cpp:233-236 @ v2.5.2).
    pub fn set_interpolation(&mut self, interpolation: Interpolation) {
        self.interpolation = interpolation;
    }

    /// Best, default, linear and nearest.
    ///
    /// Port of `Lut1DOpData::IsValidInterpolation` (Lut1DOpData.cpp:376-391 @ v2.5.2).
    pub fn is_valid_interpolation(interpolation: Interpolation) -> bool {
        match interpolation {
            Interpolation::Best
            | Interpolation::Default
            | Interpolation::Linear
            | Interpolation::Nearest => true,
            Interpolation::Cubic | Interpolation::Tetrahedral | Interpolation::Unknown => false,
        }
    }

    /// Port of `Lut1DOpData::getDirection` (Lut1DOpData.h:104 @ v2.5.2).
    pub fn get_direction(&self) -> TransformDirection {
        self.direction
    }

    /// Port of `Lut1DOpData::setDirection` (Lut1DOpData.h:105 @ v2.5.2).
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.direction = dir;
    }

    /// Port of `Lut1DOpData::getType` (Lut1DOpData.h:107 @ v2.5.2).
    pub fn get_type(&self) -> OpDataType {
        OpDataType::Lut1D
    }

    /// Whether the LUT leaves every value as it is: a half-domain identity; a standard domain
    /// never is (it clamps).
    ///
    /// Port of `Lut1DOpData::isNoOp` (Lut1DOpData.cpp:256-266 @ v2.5.2).
    pub fn is_no_op(&self) -> bool {
        if self.is_input_half_domain() {
            self.is_identity()
        } else {
            false
        }
    }

    /// Port of `Lut1DOpData::isIdentity` (Lut1DOpData.cpp:238-241 @ v2.5.2).
    pub fn is_identity(&self) -> bool {
        self.array.is_identity(self.half_flags)
    }

    /// Whether the LUT mixes channels: with hue adjust, even for an identity ("time
    /// consuming" to check).
    ///
    /// Port of `Lut1DOpData::hasChannelCrosstalk` (Lut1DOpData.cpp:243-254 @ v2.5.2).
    pub fn has_channel_crosstalk(&self) -> bool {
        // Returning !isIdentity() would be time consuming.
        self.get_hue_adjust() != Lut1DHueAdjust::None
    }

    /// The data's cache ID: its id and a space, if it has one, then the hash of the values'
    /// bytes, the direction, the interpolation, `half domain` or `standard domain`, and the hue
    /// adjust's name; "1D LUT HUE_WYPN hue adjust style is not implemented." for that style.
    ///
    /// Port of `Lut1DOpData::getCacheID` (Lut1DOpData.cpp:628-652 @ v2.5.2) and
    /// `GetHueAdjustName` (Lut1DOpData.cpp:604-626).
    pub fn get_cache_id(&self) -> Result<Vec<u8>> {
        let values = self.get_array().get_values();

        let mut cache_id = Vec::new();
        if !self.get_id().is_empty() {
            cache_id.extend_from_slice(self.get_id());
            cache_id.push(b' ');
        }

        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_ne_bytes()).collect();
        let hue_name = match self.hue_adjust {
            Lut1DHueAdjust::Dw3 => "dw3",
            Lut1DHueAdjust::None => "none",
            Lut1DHueAdjust::Wypn => {
                return Err(Exception::new(
                    "1D LUT HUE_WYPN hue adjust style is not implemented.",
                ));
            }
        };
        let text = format!(
            "{} {} {} {} {}",
            cache_id_hash(&bytes),
            transform_direction_to_string(self.direction),
            interpolation_to_string(self.interpolation),
            if self.is_input_half_domain() {
                "half domain"
            } else {
                "standard domain"
            },
            hue_name
        );
        cache_id.extend_from_slice(text.as_bytes());

        // NB: The m_invQuality is not currently included.

        Ok(cache_id)
    }

    /// Port of `Lut1DOpData::isInputHalfDomain` (Lut1DOpData.h:159-162 @ v2.5.2).
    pub fn is_input_half_domain(&self) -> bool {
        is_input_half_domain_flags(self.half_flags)
    }

    /// Port of `Lut1DOpData::setInputHalfDomain` (Lut1DOpData.cpp:362-367 @ v2.5.2).
    pub fn set_input_half_domain(&mut self, is_half_domain: bool) {
        self.half_flags = if is_half_domain {
            HalfFlags(self.half_flags.0 | HalfFlags::INPUT_HALF_CODE.0)
        } else {
            HalfFlags(self.half_flags.0 & !HalfFlags::INPUT_HALF_CODE.0)
        };
    }

    /// Port of `Lut1DOpData::setOutputRawHalfs` (Lut1DOpData.cpp:369-374 @ v2.5.2).
    pub fn set_output_raw_halfs(&mut self, is_raw_halfs: bool) {
        self.half_flags = if is_raw_halfs {
            HalfFlags(self.half_flags.0 | HalfFlags::OUTPUT_HALF_CODE.0)
        } else {
            HalfFlags(self.half_flags.0 & !HalfFlags::OUTPUT_HALF_CODE.0)
        };
    }

    /// Port of `Lut1DOpData::isOutputRawHalfs` (Lut1DOpData.h:168-171 @ v2.5.2).
    pub fn is_output_raw_halfs(&self) -> bool {
        (self.half_flags.0 & HalfFlags::OUTPUT_HALF_CODE.0) == HalfFlags::OUTPUT_HALF_CODE.0
    }

    /// Port of `Lut1DOpData::getHalfFlags` (Lut1DOpData.h:173 @ v2.5.2).
    pub fn get_half_flags(&self) -> HalfFlags {
        self.half_flags
    }

    /// Port of `Lut1DOpData::getHueAdjust` (Lut1DOpData.h:175 @ v2.5.2).
    pub fn get_hue_adjust(&self) -> Lut1DHueAdjust {
        self.hue_adjust
    }

    /// "1D LUT HUE_WYPN hue adjust style is not implemented." for that style.
    ///
    /// Port of `Lut1DOpData::setHueAdjust` (Lut1DOpData.cpp:552-560 @ v2.5.2).
    pub fn set_hue_adjust(&mut self, algo: Lut1DHueAdjust) -> Result<()> {
        if algo == Lut1DHueAdjust::Wypn {
            return Err(Exception::new(
                "1D LUT HUE_WYPN hue adjust style is not implemented.",
            ));
        }

        self.hue_adjust = algo;
        Ok(())
    }

    /// Port of `Lut1DOpData::getArray() const` (Lut1DOpData.h:179 @ v2.5.2).
    pub fn get_array(&self) -> &Lut3by1DArray {
        &self.array
    }

    /// Port of `Lut1DOpData::getArray()` (Lut1DOpData.h:180 @ v2.5.2).
    pub fn get_array_mut(&mut self) -> &mut Lut3by1DArray {
        &mut self.array
    }

    /// Checks the hue adjust, the interpolation, the array, and the 65536 entries of a half
    /// domain, with upstream's messages.
    ///
    /// Port of `Lut1DOpData::validate` (Lut1DOpData.cpp:393-435 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        if self.hue_adjust == Lut1DHueAdjust::Wypn {
            return Err(Exception::new(
                "1D LUT HUE_WYPN hue adjust style is not implemented.",
            ));
        }

        if !Self::is_valid_interpolation(self.interpolation) {
            return Err(Exception::new(format!(
                "1D LUT does not support interpolation algorithm: {}.",
                interpolation_to_string(self.get_interpolation())
            )));
        }

        if let Err(e) = self.get_array().validate() {
            return Err(Exception::new(
                [b"1D LUT content array issue: ".as_slice(), e.what()].concat(),
            ));
        }

        // If isHalfDomain is set, we need to make sure we have 65536 entries.
        if self.is_input_half_domain()
            && self.get_array().get_length() != HALF_DOMAIN_REQUIRED_ENTRIES
        {
            return Err(Exception::new(format!(
                "1D LUT: {} entries found, {HALF_DOMAIN_REQUIRED_ENTRIES} required for \
                 halfDomain 1D LUT.",
                self.get_array().get_length()
            )));
        }
        Ok(())
    }

    /// The same LUT in the other direction (an inverse LUT is set up by `finalize`).
    ///
    /// Port of `Lut1DOpData::inverse` (Lut1DOpData.cpp:591-602 @ v2.5.2).
    pub fn inverse(&self) -> Lut1DOpData {
        let mut inv_lut = self.clone();

        inv_lut.direction = match self.direction {
            TransformDirection::Forward => TransformDirection::Inverse,
            TransformDirection::Inverse => TransformDirection::Forward,
        };

        // Note that any existing metadata could become stale at this point but trying to update
        // it is also challenging since inverse() is sometimes called even during the creation
        // of new ops.
        inv_lut
    }

    /// Whether `other` undoes this LUT: the other direction, and the same half flags, hue
    /// adjust and values.
    ///
    /// Port of `Lut1DOpData::isInverse` (Lut1DOpData.cpp:570-584 @ v2.5.2).
    pub fn is_inverse(&self, other: &Lut1DOpData) -> bool {
        if (self.direction == TransformDirection::Forward
            && other.direction == TransformDirection::Inverse)
            || (self.direction == TransformDirection::Inverse
                && other.direction == TransformDirection::Forward)
        {
            // Note: The inverse LUT 1D finalize modifies the array to make it monotonic, hence,
            // this could return false in unexpected cases. However, one could argue that those
            // LUTs should not be optimized out as an identity anyway.
            return self.have_equal_basics(other);
        }
        false
    }

    /// Whether both LUTs may compose: neither adjusts hue.
    ///
    /// Port of `Lut1DOpData::mayCompose` (Lut1DOpData.cpp:586-589 @ v2.5.2).
    pub fn may_compose(&self, other: &Lut1DOpData) -> bool {
        self.get_hue_adjust() == Lut1DHueAdjust::None
            && other.get_hue_adjust() == Lut1DHueAdjust::None
    }

    /// Port of `Lut1DOpData::hasSingleLut` (Lut1DOpData.h:198-202 @ v2.5.2).
    pub fn has_single_lut(&self) -> bool {
        self.array.get_num_color_components() == 1
    }

    /// Whether a renderer can look values of `incoming_depth` up in the LUT without
    /// resampling it: 16f values in a half domain, integer values in a standard domain of one
    /// entry per code.
    ///
    /// Port of `Lut1DOpData::mayLookup` (Lut1DOpData.cpp:491-506 @ v2.5.2).
    pub fn may_lookup(&self, incoming_depth: BitDepth) -> Result<bool> {
        if self.is_input_half_domain() {
            return Ok(incoming_depth == BitDepth::F16);
        } else {
            // not a half-domain LUT
            if !is_float_bit_depth(incoming_depth)? {
                return Ok(self.array.get_length() as f64
                    == (get_bit_depth_max_value(incoming_depth)? + 1.0));
            }
        }
        Ok(false)
    }

    /// The half flags, the hue adjust and the array.
    ///
    /// Port of `Lut1DOpData::haveEqualBasics` (Lut1DOpData.cpp:528-534 @ v2.5.2).
    fn have_equal_basics(&self, other: &Lut1DOpData) -> bool {
        // Question: Should interpolation style be considered?
        self.half_flags == other.half_flags
            && self.hue_adjust == other.hue_adjust
            && self.array == other.array
    }

    /// Whether `other` has the direction, the concrete interpolation (always linear), the half
    /// flags, the hue adjust and the values. The metadata is ignored. The `OpData` base's type
    /// comparison is [`OpData::equals`]'s.
    ///
    /// Port of `Lut1DOpData::equals` (Lut1DOpData.cpp:536-550 @ v2.5.2).
    pub fn equals(&self, other: &Lut1DOpData) -> bool {
        // `OpData::equals`: the same object, or the same type.
        if std::ptr::eq(self, other) {
            return true;
        }

        // NB: The m_invQuality is not currently included.
        if self.direction != other.direction
            || self.get_concrete_interpolation() != other.get_concrete_interpolation()
        {
            return false;
        }

        self.have_equal_basics(other)
    }

    /// The data that replaces an identity LUT: an identity matrix for a half domain, a [0, 1]
    /// clamp otherwise.
    ///
    /// Port of `Lut1DOpData::getIdentityReplacement` (Lut1DOpData.cpp:268-280 @ v2.5.2).
    pub fn get_identity_replacement(&self) -> Result<OpData> {
        Ok(if self.is_input_half_domain() {
            OpData::Matrix(MatrixOpData::new())
        } else {
            OpData::Range(RangeOpData::with_values(0., 1., 0., 1.)?)
        })
    }

    /// Whether the LUT has values outside [0, 1] by more than 1e-5, NaNs aside.
    ///
    /// Port of `Lut1DOpData::hasExtendedRange` (Lut1DOpData.cpp:874-910 @ v2.5.2).
    pub fn has_extended_range(&self) -> bool {
        let values = self.get_array().get_values();

        const NORMAL_MIN: f32 = 0.0 - 1e-5;
        const NORMAL_MAX: f32 = 1.0 + 1e-5;

        for &val in values {
            if val.is_nan() {
                continue;
            }
            if val < NORMAL_MIN {
                return true;
            }
            if val > NORMAL_MAX {
                return true;
            }
        }

        false
    }

    /// Port of `Lut1DOpData::getFileOutputBitDepth` (Lut1DOpData.h:224 @ v2.5.2).
    pub fn get_file_output_bit_depth(&self) -> BitDepth {
        self.file_out_bit_depth
    }

    /// Port of `Lut1DOpData::setFileOutputBitDepth` (Lut1DOpData.h:225 @ v2.5.2).
    pub fn set_file_output_bit_depth(&mut self, out: BitDepth) {
        self.file_out_bit_depth = out;
    }

    /// Multiplies every value by `scale`.
    ///
    /// Port of `Lut1DOpData::scale` (Lut1DOpData.cpp:869-872 @ v2.5.2).
    pub fn scale(&mut self, scale: f32) {
        self.get_array_mut().scale(scale);
    }

    /// Prepares the LUT for rendering: an inverse LUT is made monotonic and its component
    /// properties set ([`initialize_from_forward`](Self::initialize_from_forward)); then one
    /// color component when the channels are equal.
    ///
    /// Port of `Lut1DOpData::finalize` (Lut1DOpData.cpp:912-919 @ v2.5.2).
    pub fn finalize(&mut self) -> Result<()> {
        if self.direction == TransformDirection::Inverse {
            self.initialize_from_forward();
        }
        self.array.adjust_color_component_number();
        Ok(())
    }

    /// The red channel's properties for an inverse LUT.
    ///
    /// Port of `Lut1DOpData::getRedProperties` (Lut1DOpData.h:184-187 @ v2.5.2).
    pub fn get_red_properties(&self) -> &ComponentProperties {
        &self.component_properties[0]
    }

    /// Port of `Lut1DOpData::getGreenProperties` (Lut1DOpData.h:189-192 @ v2.5.2).
    pub fn get_green_properties(&self) -> &ComponentProperties {
        &self.component_properties[1]
    }

    /// Port of `Lut1DOpData::getBlueProperties` (Lut1DOpData.h:194-197 @ v2.5.2).
    pub fn get_blue_properties(&self) -> &ComponentProperties {
        &self.component_properties[2]
    }

    /// Sets up an inverse LUT from its forward values: per active channel, whether it is
    /// overall increasing (its first entry below its last; for a half domain, the entries of
    /// 0 and 1), its reversals flattened (an entry that goes the wrong way takes the value
    /// before it) so that the values sort, and its effective domain without the flat spots at
    /// its ends (a half domain's positive and negative halves separately, the NaN codes left
    /// out). With one active channel, all three take its properties.
    ///
    /// The half domain's flattening ends at index `31744 * maxChannels` (and the negative
    /// half's at `64512 * maxChannels`) without the channel's offset, so for green and blue it
    /// stops one entry short of +Inf and -Inf (docs/improvements.md, I-66).
    ///
    /// Port of `Lut1DOpData::initializeFromForward` (Lut1DOpData.cpp:921-1118 @ v2.5.2).
    fn initialize_from_forward(&mut self) {
        // This routine is to be called (e.g. in XML reader) once the base forward
        // Lut1D has been created and then sets up what is needed for the invLut1D.

        // Note that if the original LUT had a half domain, the invLut needs to as
        // well so that the appropriate evaluation algorithm is called.

        // NB: The file reader must call setFileOutputBitDepth since some methods
        // need to know the original scaling of the LUT.

        // NB: The half domain includes pos/neg infinity and NaNs.
        // InitializeFromForward makes the LUT monotonic to ensure a unique inverse and
        // determines an effective domain to handle flat spots at the ends nicely.
        // It's not clear how the NaN part of the domain should be included in the
        // monotonicity constraints, furthermore there are 2048 NaNs that could each
        // potentially have different values.  For now, the inversion algorithm and
        // the pre-processing ignore the NaN part of the domain.

        // Note: Data allocated for the array is length*getMaxColorComponents().
        let half_domain = self.is_input_half_domain();
        let length = self.array.get_length() as usize;
        let max_channels = self.array.get_max_color_components() as usize;
        let active_channels = self.array.get_num_color_components() as usize;
        let values = self.array.get_values_mut();

        for c in 0..active_channels {
            let props = &mut self.component_properties[c];

            // Determine if the LUT is overall increasing or decreasing.
            // The heuristic used is to compare first and last entries.
            // (Note flat LUTs (arbitrarily) have isIncreasing == false.)
            let mut low_ind = c;
            let mut high_ind = (length - 1) * max_channels + c;
            if half_domain {
                // For half-domain LUTs, I am concerned that customer LUTs may not
                // correctly populate the whole domain, so using -HALF_MAX and
                // +HALF_MAX could potentially give unreliable results.
                // Just using 0 and 1 for now.
                low_ind = c; // 0u * maxChannels + c: 0.0
                high_ind = 15360 * max_channels + c; // 15360 == 1.0
            }

            props.is_increasing = values[low_ind] < values[high_ind];

            // Flatten reversals.
            // (If the LUT has a reversal, there is not a unique inverse.
            // Furthermore we require sorted values for the exact eval algorithm.)
            {
                let mut is_increasing = props.is_increasing;

                // One run of the flattening, from `start` to `end` included, by steps of
                // `max_channels`, from the value `prev_value`.
                let flatten = |values: &mut Vec<f32>,
                               start: usize,
                               end: usize,
                               is_increasing: bool,
                               mut prev_value: f32| {
                    let mut idx = start;
                    while idx <= end {
                        if is_increasing != (values[idx] > prev_value) {
                            values[idx] = prev_value;
                        } else {
                            prev_value = values[idx];
                        }
                        idx += max_channels;
                    }
                };

                if !half_domain {
                    let prev_value = values[c];
                    // `idx < length * maxChannels`.
                    let end = length * max_channels - 1;
                    flatten(values, c + max_channels, end, is_increasing, prev_value);
                } else {
                    // Do positive numbers.
                    let start_ind = c; // 0u * maxChannels + c: 0 == +zero
                    let end_ind = 31744 * max_channels; // 31744 == +infinity
                    let prev_value = values[start_ind];
                    flatten(
                        values,
                        start_ind + max_channels,
                        end_ind,
                        is_increasing,
                        prev_value,
                    );

                    // Do negative numbers.
                    is_increasing = !is_increasing;
                    let start_ind = 32768 * max_channels + c; // 32768 == -zero
                    let end_ind = 64512 * max_channels; // 64512 == -infinity
                    let prev_value = values[c]; // prev value for -0 is +0 (disallow overlaps)
                    flatten(values, start_ind, end_ind, is_increasing, prev_value);
                }
            }

            // Determine effective domain from the starting/ending flat spots.
            // (If the LUT begins or ends with a flat spot, the inverse should be
            // the value nearest the center of the LUT.)
            // For constant LUTs, the end domain == start domain == 0.
            {
                if !half_domain {
                    let mut end_domain = length - 1;
                    let end_value = values[end_domain * max_channels + c];
                    while end_domain > 0 && values[(end_domain - 1) * max_channels + c] == end_value
                    {
                        end_domain -= 1;
                    }

                    let mut start_domain = 0;
                    let start_value = values[start_domain * max_channels + c];
                    // Note that this works for both increasing and decreasing LUTs
                    // since there is no reqmt that startValue < endValue.
                    while start_domain < end_domain
                        && values[(start_domain + 1) * max_channels + c] == start_value
                    {
                        start_domain += 1;
                    }

                    props.start_domain = start_domain as c_ulong;
                    props.end_domain = end_domain as c_ulong;
                } else {
                    // Question: Should the value for infinity be considered for
                    // interpolation? The advantage is that in theory, if infinity
                    // is mapped to some value by the forward LUT, you could
                    // restore that value to infinity in the inverse.
                    // This does seem to work in INV_EXACT mode (e.g.
                    // CPURendererInvLUT1DHalf_fclut unit test).
                    // TODO: Test to be ported CPURenderer_cases.cpp_inc
                    // The problem is that in INV_FAST mode, there are Infs in the fast
                    // LUT and these seem to make the value for both inf and 65504
                    // a NaN. Limiting the effective domain allows 65504 to invert
                    // correctly.
                    let mut end_domain = 31743; // +65504 = largest half value < inf
                    let end_value = values[end_domain * max_channels + c];
                    while end_domain > 0 && values[(end_domain - 1) * max_channels + c] == end_value
                    {
                        end_domain -= 1;
                    }

                    let mut start_domain = 0; // positive zero
                    let start_value = values[start_domain * max_channels + c];
                    // Note that this works for both increasing and decreasing LUTs
                    // since there is no reqmt that startValue < endValue.
                    while start_domain < end_domain
                        && values[(start_domain + 1) * max_channels + c] == start_value
                    {
                        start_domain += 1;
                    }

                    props.start_domain = start_domain as c_ulong;
                    props.end_domain = end_domain as c_ulong;

                    // Negative half of domain has its own start/end.
                    let mut neg_end_domain = 64511; // -65504 = last value before neg inf
                    let neg_end_value = values[neg_end_domain * max_channels + c];
                    while neg_end_domain > 32768 // negative zero
                        && values[(neg_end_domain - 1) * max_channels + c] == neg_end_value
                    {
                        neg_end_domain -= 1;
                    }

                    let mut neg_start_domain = 32768; // negative zero
                    let neg_start_value = values[neg_start_domain * max_channels + c];
                    while neg_start_domain < neg_end_domain
                        && values[(neg_start_domain + 1) * max_channels + c] == neg_start_value
                    {
                        neg_start_domain += 1;
                    }

                    props.neg_start_domain = neg_start_domain as c_ulong;
                    props.neg_end_domain = neg_end_domain as c_ulong;
                }
            }
        }

        if active_channels == 1 {
            self.component_properties[2] = self.component_properties[0];
            self.component_properties[1] = self.component_properties[0];
        }
    }

    /// The data that replaces this LUT followed by `lut2`, its inverse (or the reverse): an
    /// identity matrix for a half domain; otherwise a range that clamps as the round trip
    /// does, from the inverse LUT's red channel (which `finalize` set up): its effective
    /// domain over `[0, 1]` for a forward LUT then its inverse, its extreme values for an
    /// inverse LUT then its forward.
    ///
    /// Port of `Lut1DOpData::getPairIdentityReplacement` (Lut1DOpData.cpp:282-360 @ v2.5.2).
    pub fn get_pair_identity_replacement(&self, lut2: &Lut1DOpData) -> Result<OpData> {
        if self.is_input_half_domain() {
            // TODO: If a half-domain LUT has a flat spot, it would be more appropriate
            // to use a Range, since some areas would be clamped in a round-trip.
            // Currently leaving this a Matrix since it is a potential work-around
            // for situations where you want a pair identity of LUTs to be totally
            // removed, even if it omits some clamping at extreme values.
            return Ok(OpData::Matrix(MatrixOpData::new()));
        }

        // Note that the ops have been finalized by the time this is called,
        // Therefore, for an inverse Lut1D, it means initializeFromForward() has
        // been called and so any reversals have been converted to flat regions.
        // Therefore, the first and last LUT entries are the extreme values and
        // the ComponentProperties has been initialized, but only for the op
        // whose direction is INVERSE.
        let inv_lut = if self.direction == TransformDirection::Inverse {
            self
        } else {
            lut2
        };
        let red_properties = inv_lut.get_red_properties();
        let length = inv_lut.get_array().get_length();

        // If the start or end of the LUT contains a flat region, that will cause
        // a round-trip to be limited.

        let (min_value, max_value) = match self.direction {
            // Fwd Lut1D -> Inv Lut1D
            TransformDirection::Forward => {
                // A round-trip in this order will impose at least a clamp to [0,1]
                // based on what happens entering the first Fwd Lut1D.  However, the
                // clamping may be to an even narrower range if there are flat regions.
                //
                // The flat region limitation is imposed based on the where it falls
                // relative to the [0,1] input domain.

                // TODO: A RangeOp has one min & max for all channels, whereas a Lut1D may
                // have three independent channels.  Potentially could look at all chans
                // and take the extrema of each?  For now, just using the first channel.
                let min_index = red_properties.start_domain;
                let max_index = red_properties.end_domain;

                (
                    min_index as f64 / (length - 1) as f64,
                    max_index as f64 / (length - 1) as f64,
                )
            }
            // Inv Lut1D -> Fwd Lut1D
            TransformDirection::Inverse => {
                // A round-trip in this order will impose a clamp, but it may be to
                // bounds outside of [0,1] since the Fwd LUT may contain values outside
                // [0,1] and so the Inv LUT will accept inputs on that extended range.
                //
                // The flat region limitation is imposed based on the output range.

                let is_increasing = red_properties.is_increasing;
                let max_channels = inv_lut.get_array().get_max_color_components();
                let last_val_index = ((length - 1) * max_channels) as usize;
                // Note that the array for the invLut has had initializeFromForward()
                // done and so any reversals have been converted to flat regions and
                // the extrema are at the beginning & end of the LUT.
                let lut_values = inv_lut.get_array().get_values();

                // TODO: Currently only basing this on the red channel.
                (
                    f64::from(if is_increasing {
                        lut_values[0]
                    } else {
                        lut_values[last_val_index]
                    }),
                    f64::from(if is_increasing {
                        lut_values[last_val_index]
                    } else {
                        lut_values[0]
                    }),
                )
            }
        };

        Ok(OpData::Range(RangeOpData::with_values(
            min_value, max_value, min_value, max_value,
        )?))
    }

    /// Port of `OpData::getFormatMetadata() const` (src/OpenColorIO/Op.h:164 @ v2.5.2).
    pub fn get_format_metadata(&self) -> &FormatMetadataImpl {
        &self.metadata
    }

    /// Port of `OpData::getFormatMetadata()` (src/OpenColorIO/Op.h:163 @ v2.5.2).
    pub fn get_format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        &mut self.metadata
    }

    /// Port of `OpData::getID` (src/OpenColorIO/Op.cpp:81-84 @ v2.5.2).
    pub fn get_id(&self) -> &[u8] {
        self.metadata.get_attribute_value_string(Some(METADATA_ID))
    }
}

/// Port of `operator==(const Lut1DOpData &, const Lut1DOpData &)` (Lut1DOpData.cpp:1120-1123
/// @ v2.5.2).
impl PartialEq for Lut1DOpData {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}

#[cfg(test)]
#[path = "lut1d_op_data_tests.rs"]
mod tests;
