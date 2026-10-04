// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The 1D LUT transform: a port of `src/OpenColorIO/transforms/Lut1DTransform.h` and
//! `Lut1DTransform.cpp` @ v2.5.2, with its op glue from `src/OpenColorIO/ops/lut1d/Lut1DOp.cpp`
//! (`CreateLut1DTransform`, `BuildLut1DOp`).

use std::ffi::c_ulong;
use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::math_utils::{std_max, std_min};
use ocio_ops::op::{Op, OpVec};
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::{
    BitDepth, Lut1DHueAdjust, TransformDirection, bit_depth_to_string, interpolation_to_string,
    transform_direction_to_string,
};
use ocio_ops::ops::lut1d::Lut1DOpData;
use ocio_ops::ops::lut1d::lut1d_op::create_lut1d_op;
use ocio_ops::ops::lut1d::lut1d_op_data::{HalfFlags, Lut3by1DArray};
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;

use crate::transform::validate_direction;
use crate::transforms::group_transform::GroupTransform;

/// A 1D LUT: `length` RGB entries over a standard domain (`[0, 1]`, evenly spaced) or a half
/// domain (one entry per half code, 65536 of them), with its interpolation, hue adjustment and
/// the encoding of its values.
///
/// The transform is its op data, as upstream's `Lut1DTransformImpl` holds it. A copy is
/// upstream's `createEditableCopy`.
///
/// Port of `Lut1DTransform` and `Lut1DTransformImpl` (include/OpenColorIO/
/// OpenColorTransforms.h, src/OpenColorIO/transforms/Lut1DTransform.h, Lut1DTransform.cpp @
/// v2.5.2).
#[derive(Debug, Clone)]
pub struct Lut1DTransform {
    /// `m_data`.
    data: Lut1DOpData,
}

impl Default for Lut1DTransform {
    fn default() -> Self {
        Self::new()
    }
}

impl Lut1DTransform {
    /// A forward identity LUT of 2 entries over the standard domain, with the default
    /// interpolation, no hue adjustment and an unknown file bit depth.
    ///
    /// Port of `Lut1DTransform::Create()` (Lut1DTransform.cpp:15-18 @ v2.5.2), the default
    /// constructor (33-35) and the member's initializer `m_data{ 2 }` (Lut1DTransform.h:64).
    #[doc(alias = "Create")]
    pub fn new() -> Lut1DTransform {
        Lut1DTransform {
            data: Lut1DOpData::new(2).expect("a 1D LUT of 2 entries is valid"),
        }
    }

    /// A forward identity LUT of `length` entries, over the half domain when `is_half_domain`:
    /// "LUT 1D length needs to be at least 2." under 2 entries, "LUT 1D: Length '<length>' must
    /// not be greater than 1024x1024 (1048576)." over that.
    ///
    /// Port of `Lut1DTransform::Create(unsigned long, bool)` (Lut1DTransform.cpp:20-26 @
    /// v2.5.2) and its constructor (37-40).
    #[doc(alias = "Create")]
    pub fn with_length(length: c_ulong, is_half_domain: bool) -> Result<Lut1DTransform> {
        let half_flag = if is_half_domain {
            HalfFlags::INPUT_HALF_CODE
        } else {
            HalfFlags::STANDARD
        };
        Ok(Lut1DTransform {
            data: Lut1DOpData::with_half_flags(half_flag, length, false)?,
        })
    }

    /// The op data the transform holds.
    ///
    /// Port of `Lut1DTransformImpl::data() const` (Lut1DTransform.h:59 @ v2.5.2).
    pub(crate) fn data(&self) -> &Lut1DOpData {
        &self.data
    }

    /// Port of `Lut1DTransformImpl::getDirection` (Lut1DTransform.cpp:49-52 @ v2.5.2).
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        self.data.get_direction()
    }

    /// Port of `Lut1DTransformImpl::setDirection` (Lut1DTransform.cpp:54-57 @ v2.5.2).
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.data.set_direction(dir);
    }

    /// Checks the direction and the data (the hue adjustment, the interpolation, the values,
    /// and the 65536 entries of a half domain): "Lut1DTransform validation failed: " and the
    /// first problem.
    ///
    /// Port of `Lut1DTransformImpl::validate` (Lut1DTransform.cpp:59-73 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        let checked = (|| {
            validate_direction(self.direction())?;
            self.data.validate()
        })();
        checked.map_err(|ex| {
            Exception::new(format!(
                "Lut1DTransform validation failed: {}",
                ex.message()
            ))
        })
    }

    /// The bit depth of the file the values came from (or go to), at the output.
    ///
    /// Port of `Lut1DTransformImpl::getFileOutputBitDepth` (Lut1DTransform.cpp:75-78 @ v2.5.2).
    #[doc(alias = "getFileOutputBitDepth")]
    pub fn file_output_bit_depth(&self) -> BitDepth {
        self.data.get_file_output_bit_depth()
    }

    /// Port of `Lut1DTransformImpl::setFileOutputBitDepth` (Lut1DTransform.cpp:80-83 @ v2.5.2).
    #[doc(alias = "setFileOutputBitDepth")]
    pub fn set_file_output_bit_depth(&mut self, bit_depth: BitDepth) {
        self.data.set_file_output_bit_depth(bit_depth);
    }

    /// Port of `Lut1DTransformImpl::getFormatMetadata() const` (Lut1DTransform.cpp:90-93 @
    /// v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata(&self) -> &FormatMetadataImpl {
        self.data.get_format_metadata()
    }

    /// Port of `Lut1DTransformImpl::getFormatMetadata()` (Lut1DTransform.cpp:85-88 @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        self.data.get_format_metadata_mut()
    }

    /// Whether `other` has the same data: the direction, the half flags, the hue adjustment
    /// and the values, ignoring the interpolation (the data compares the one the renderers
    /// implement, always linear), the metadata and the file bit depth.
    ///
    /// Port of `Lut1DTransformImpl::equals` (Lut1DTransform.cpp:95-99 @ v2.5.2).
    pub fn equals(&self, other: &Lut1DTransform) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        self.data == other.data
    }

    /// The number of entries.
    ///
    /// Port of `Lut1DTransformImpl::getLength` (Lut1DTransform.cpp:151-154 @ v2.5.2).
    #[doc(alias = "getLength")]
    pub fn length(&self) -> c_ulong {
        self.data.get_array().get_length()
    }

    /// Replaces the values with an identity LUT of `length` entries over the transform's
    /// domain (the half domain's NaN codes keep their NaNs): "LUT 1D length needs to be at
    /// least 2." under 2 entries, "LUT 1D: Length '<length>' must not be greater than 1024x1024
    /// (1048576)." over that, and the values are unchanged.
    ///
    /// Port of `Lut1DTransformImpl::setLength` (Lut1DTransform.cpp:101-106 @ v2.5.2).
    #[doc(alias = "setLength")]
    pub fn set_length(&mut self, length: c_ulong) -> Result<()> {
        // Use NaNs for the 2048 NaN values in the domain.
        let lut_array = Lut3by1DArray::new(self.data.get_half_flags(), 3, length, false)?;
        *self.data.get_array_mut() = lut_array;
        Ok(())
    }

    /// The entry `index`: "Lut1DTransform getValue: index (<index>) should be less than the
    /// length (<length>)." past the end.
    ///
    /// Port of `Lut1DTransformImpl::getValue` (Lut1DTransform.cpp:156-162 @ v2.5.2).
    #[doc(alias = "getValue")]
    pub fn value(&self, index: c_ulong) -> Result<[f32; 3]> {
        check_lut1d_index("getValue", index, self.length())?;
        let array = self.data.get_array();
        let i = 3 * index as usize;
        Ok([array[i], array[i + 1], array[i + 2]])
    }

    /// Sets the entry `index`: "Lut1DTransform setValue: index (<index>) should be less than
    /// the length (<length>)." past the end.
    ///
    /// Port of `Lut1DTransformImpl::setValue` (Lut1DTransform.cpp:123-129 @ v2.5.2).
    #[doc(alias = "setValue")]
    pub fn set_value(&mut self, index: c_ulong, r: f32, g: f32, b: f32) -> Result<()> {
        check_lut1d_index("setValue", index, self.length())?;
        let array = self.data.get_array_mut();
        let i = 3 * index as usize;
        array[i] = r;
        array[i + 1] = g;
        array[i + 2] = b;
        Ok(())
    }

    /// Whether the entries are indexed by the input's half code.
    ///
    /// Port of `Lut1DTransformImpl::getInputHalfDomain` (Lut1DTransform.cpp:164-167 @ v2.5.2).
    #[doc(alias = "getInputHalfDomain")]
    pub fn input_half_domain(&self) -> bool {
        self.data.is_input_half_domain()
    }

    /// Sets whether the entries are indexed by the input's half code; the values stay as they
    /// are.
    ///
    /// Port of `Lut1DTransformImpl::setInputHalfDomain` (Lut1DTransform.cpp:131-134 @ v2.5.2).
    #[doc(alias = "setInputHalfDomain")]
    pub fn set_input_half_domain(&mut self, is_half_domain: bool) {
        self.data.set_input_half_domain(is_half_domain);
    }

    /// Whether the values are written to a file as raw half codes.
    ///
    /// Port of `Lut1DTransformImpl::getOutputRawHalfs` (Lut1DTransform.cpp:169-172 @ v2.5.2).
    #[doc(alias = "getOutputRawHalfs")]
    pub fn output_raw_halfs(&self) -> bool {
        self.data.is_output_raw_halfs()
    }

    /// Port of `Lut1DTransformImpl::setOutputRawHalfs` (Lut1DTransform.cpp:136-139 @ v2.5.2).
    #[doc(alias = "setOutputRawHalfs")]
    pub fn set_output_raw_halfs(&mut self, is_raw_halfs: bool) {
        self.data.set_output_raw_halfs(is_raw_halfs);
    }

    /// Port of `Lut1DTransformImpl::getHueAdjust` (Lut1DTransform.cpp:174-177 @ v2.5.2).
    #[doc(alias = "getHueAdjust")]
    pub fn hue_adjust(&self) -> Lut1DHueAdjust {
        self.data.get_hue_adjust()
    }

    /// "1D LUT HUE_WYPN hue adjust style is not implemented." for [`Lut1DHueAdjust::Wypn`],
    /// which leaves the hue adjustment as it was.
    ///
    /// Port of `Lut1DTransformImpl::setHueAdjust` (Lut1DTransform.cpp:141-144 @ v2.5.2).
    #[doc(alias = "setHueAdjust")]
    pub fn set_hue_adjust(&mut self, algo: Lut1DHueAdjust) -> Result<()> {
        self.data.set_hue_adjust(algo)
    }

    /// Port of `Lut1DTransformImpl::getInterpolation` (Lut1DTransform.cpp:179-182 @ v2.5.2).
    #[doc(alias = "getInterpolation")]
    pub fn interpolation(&self) -> Interpolation {
        self.data.get_interpolation()
    }

    /// Sets the interpolation; one a 1D LUT doesn't support fails `validate`.
    ///
    /// Port of `Lut1DTransformImpl::setInterpolation` (Lut1DTransform.cpp:146-149 @ v2.5.2).
    #[doc(alias = "setInterpolation")]
    pub fn set_interpolation(&mut self, algo: Interpolation) {
        self.data.set_interpolation(algo);
    }

    /// Writes the transform's text to `os`, its numbers with the stream's precision: 6 on a new
    /// stream, 16 after a MatrixTransform in the same group (docs/improvements.md, I-73).
    ///
    /// The minimum and maximum of each channel are C++'s `std::min` and `std::max` from
    /// `FLT_MAX` and `-FLT_MAX`, so a NaN entry is skipped and a LUT of NaNs prints those
    /// starting values.
    ///
    /// Port of `operator<<(std::ostream &, const Lut1DTransform &)` (Lut1DTransform.cpp:
    /// 184-224 @ v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        os.put_str("<Lut1DTransform ");
        os.put_str("direction=");
        os.put_str(transform_direction_to_string(self.direction()));
        os.put_str(", ");
        os.put_str("fileoutdepth=");
        os.put_str(bit_depth_to_string(self.file_output_bit_depth()));
        os.put_str(", ");
        os.put_str("interpolation=");
        os.put_str(interpolation_to_string(self.interpolation()));
        os.put_str(", ");
        // A bool and an unscoped enum print as integers.
        os.put_str("inputhalf=");
        os.put_i32(i32::from(self.input_half_domain()));
        os.put_str(", ");
        os.put_str("outputrawhalf=");
        os.put_i32(i32::from(self.output_raw_halfs()));
        os.put_str(", ");
        os.put_str("hueadjust=");
        os.put_i32(self.hue_adjust() as i32);
        os.put_str(", ");
        let l = self.length();
        os.put_str("length=");
        // `unsigned long`: 32 bits on Windows, 64 on Linux.
        #[allow(clippy::useless_conversion)]
        os.put_u64(u64::from(l));
        os.put_str(", ");
        if l > 0 {
            let mut r_min = f32::MAX;
            let mut g_min = f32::MAX;
            let mut b_min = f32::MAX;
            let mut r_max = -r_min;
            let mut g_max = -g_min;
            let mut b_max = -b_min;
            let array = self.data.get_array();
            for i in 0..l as usize {
                let (r, g, b) = (array[3 * i], array[3 * i + 1], array[3 * i + 2]);
                r_min = std_min(r_min, r);
                g_min = std_min(g_min, g);
                b_min = std_min(b_min, b);
                r_max = std_max(r_max, r);
                g_max = std_max(g_max, g);
                b_max = std_max(b_max, b);
            }
            os.put_str("minrgb=[");
            os.put_f32(r_min);
            os.put_str(", ");
            os.put_f32(g_min);
            os.put_str(", ");
            os.put_f32(b_min);
            os.put_str("], ");
            os.put_str("maxrgb=[");
            os.put_f32(r_max);
            os.put_str(", ");
            os.put_f32(g_max);
            os.put_str(", ");
            os.put_f32(b_max);
            os.put_str("]");
        }
        os.put_str(">");
    }
}

/// "Lut1DTransform <function>: index (<index>) should be less than the length (<size>)." when
/// `index` is past the end.
///
/// Port of `CheckLUT1DIndex` (src/OpenColorIO/transforms/Lut1DTransform.cpp:110-120 @ v2.5.2).
fn check_lut1d_index(function: &str, index: c_ulong, size: c_ulong) -> Result<()> {
    if index >= size {
        return Err(Exception::new(format!(
            "Lut1DTransform {function}: index ({index}) should be less than the length ({size})."
        )));
    }
    Ok(())
}

impl PartialEq for Lut1DTransform {
    /// [`Lut1DTransform::equals`].
    fn eq(&self, other: &Lut1DTransform) -> bool {
        self.equals(other)
    }
}

impl fmt::Display for Lut1DTransform {
    /// `<Lut1DTransform direction=<dir>, fileoutdepth=<depth>, interpolation=<interp>,
    /// inputhalf=<0|1>, outputrawhalf=<0|1>, hueadjust=<n>, length=<n>, minrgb=[r, g, b],
    /// maxrgb=[r, g, b]>`.
    ///
    /// Port of `operator<<(std::ostream &, const Lut1DTransform &)` (Lut1DTransform.cpp:
    /// 184-224 @ v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(os.str())
    }
}

/// Appends to `group` the transform of the Lut1D op `op`: a copy of its data, metadata, file
/// bit depth and direction included.
///
/// Port of `CreateLut1DTransform` (src/OpenColorIO/ops/lut1d/Lut1DOp.cpp:229-242 @ v2.5.2).
pub(crate) fn create_lut1d_transform(group: &mut GroupTransform, op: &Op) -> Result<()> {
    let OpData::Lut1D(lut_data) = &**op.data() else {
        return Err(Exception::new(
            "CreateLut1DTransform: op has to be a Lut1DOp",
        ));
    };
    let mut lut_transform = Lut1DTransform::new();
    lut_transform.data = lut_data.clone();

    group.append_transform(lut_transform.into());
    Ok(())
}

/// Validates the transform's data, then appends a Lut1D op of a copy of it, in the direction
/// `dir` combined with the data's (an inverse LUT's set-up comes with Phase 2's inverse LUTs).
///
/// Port of `BuildLut1DOp` (src/OpenColorIO/ops/lut1d/Lut1DOp.cpp:244-253 @ v2.5.2).
pub(crate) fn build_lut1d_op(
    ops: &mut OpVec,
    transform: &Lut1DTransform,
    dir: TransformDirection,
) -> Result<()> {
    let data = transform.data();
    data.validate()?;

    let lut = data.clone();
    create_lut1d_op(ops, lut, dir);
    Ok(())
}

#[cfg(test)]
#[path = "lut1d_transform_tests.rs"]
mod tests;
