// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The 3D LUT transform: a port of `src/OpenColorIO/transforms/Lut3DTransform.h` and
//! `Lut3DTransform.cpp` @ v2.5.2, with its op glue from `src/OpenColorIO/ops/lut3d/Lut3DOp.cpp`
//! (`CreateLut3DTransform`, `BuildLut3DOp`).

use std::ffi::c_ulong;
use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::math_utils::{std_max, std_min};
use ocio_ops::op::{Op, OpVec};
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::{
    BitDepth, Interpolation, TransformDirection, bit_depth_to_string, interpolation_to_string,
    transform_direction_to_string,
};
use ocio_ops::ops::lut3d::lut3d_op::create_lut3d_op;
use ocio_ops::ops::lut3d::lut3d_op_data::{Lut3DArray, Lut3DOpData};

use crate::transform::validate_direction;
use crate::transforms::group_transform::GroupTransform;

/// A 3D LUT: `grid_size^3` RGB entries over `[0, 1]^3`, the blue index changing fastest, with
/// its interpolation.
///
/// The transform is its op data, as upstream's `Lut3DTransformImpl` holds it. A copy is
/// upstream's `createEditableCopy`.
///
/// Port of `Lut3DTransform` and `Lut3DTransformImpl` (include/OpenColorIO/
/// OpenColorTransforms.h, src/OpenColorIO/transforms/Lut3DTransform.h, Lut3DTransform.cpp @
/// v2.5.2).
#[derive(Debug, Clone)]
pub struct Lut3DTransform {
    /// `m_data`.
    data: Lut3DOpData,
}

impl Default for Lut3DTransform {
    fn default() -> Self {
        Self::new()
    }
}

impl Lut3DTransform {
    /// A forward identity LUT of 2 entries per side, with the default interpolation and an
    /// unknown file bit depth.
    ///
    /// Port of `Lut3DTransform::Create()` (Lut3DTransform.cpp:15-18 @ v2.5.2) and the default
    /// constructor (30-33).
    #[doc(alias = "Create")]
    pub fn new() -> Lut3DTransform {
        Lut3DTransform {
            data: Lut3DOpData::new(2).expect("a 3D LUT of 2 entries per side is valid"),
        }
    }

    /// A forward identity LUT of `grid_size` entries per side: "LUT 3D: Grid size
    /// '<grid_size>' must not be greater than '129'." over that.
    ///
    /// Port of `Lut3DTransform::Create(unsigned long)` (Lut3DTransform.cpp:20-23 @ v2.5.2) and
    /// its constructor (35-38).
    #[doc(alias = "Create")]
    pub fn with_grid_size(grid_size: c_ulong) -> Result<Lut3DTransform> {
        Ok(Lut3DTransform {
            data: Lut3DOpData::new(grid_size)?,
        })
    }

    /// The op data the transform holds.
    ///
    /// Port of `Lut3DTransformImpl::data() const` (Lut3DTransform.h:56 @ v2.5.2).
    pub(crate) fn data(&self) -> &Lut3DOpData {
        &self.data
    }

    /// Port of `Lut3DTransformImpl::getDirection` (Lut3DTransform.cpp:47-50 @ v2.5.2).
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        self.data.get_direction()
    }

    /// Port of `Lut3DTransformImpl::setDirection` (Lut3DTransform.cpp:52-55 @ v2.5.2).
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.data.set_direction(dir);
    }

    /// Checks the direction and the data (the interpolation and the values): "Lut3DTransform
    /// validation failed: " and the first problem.
    ///
    /// Port of `Lut3DTransformImpl::validate` (Lut3DTransform.cpp:57-70 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        let checked = (|| {
            validate_direction(self.direction())?;
            self.data.validate()
        })();
        checked.map_err(|ex| {
            Exception::new([b"Lut3DTransform validation failed: ".as_slice(), ex.what()].concat())
        })
    }

    /// The bit depth of the file the values came from (or go to), at the output.
    ///
    /// Port of `Lut3DTransformImpl::getFileOutputBitDepth` (Lut3DTransform.cpp:72-75 @ v2.5.2).
    #[doc(alias = "getFileOutputBitDepth")]
    pub fn file_output_bit_depth(&self) -> BitDepth {
        self.data.get_file_output_bit_depth()
    }

    /// Port of `Lut3DTransformImpl::setFileOutputBitDepth` (Lut3DTransform.cpp:77-80 @ v2.5.2).
    #[doc(alias = "setFileOutputBitDepth")]
    pub fn set_file_output_bit_depth(&mut self, bit_depth: BitDepth) {
        self.data.set_file_output_bit_depth(bit_depth);
    }

    /// Port of `Lut3DTransformImpl::getFormatMetadata() const` (Lut3DTransform.cpp:87-90 @
    /// v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata(&self) -> &FormatMetadataImpl {
        self.data.get_format_metadata()
    }

    /// Port of `Lut3DTransformImpl::getFormatMetadata()` (Lut3DTransform.cpp:82-85 @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        self.data.get_format_metadata_mut()
    }

    /// Whether `other` has the same data: the direction, the interpolation and the values,
    /// ignoring the metadata and the file bit depth.
    ///
    /// Port of `Lut3DTransformImpl::equals` (Lut3DTransform.cpp:92-96 @ v2.5.2).
    pub fn equals(&self, other: &Lut3DTransform) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        self.data == other.data
    }

    /// The number of entries per side.
    ///
    /// Port of `Lut3DTransformImpl::getGridSize` (Lut3DTransform.cpp:98-101 @ v2.5.2).
    #[doc(alias = "getGridSize")]
    pub fn grid_size(&self) -> c_ulong {
        self.data.get_array().get_length()
    }

    /// Replaces the values with an identity LUT of `grid_size` entries per side: "LUT 3D: Grid
    /// size '<grid_size>' must not be greater than '129'." over that, and the values are
    /// unchanged.
    ///
    /// Port of `Lut3DTransformImpl::setGridSize` (Lut3DTransform.cpp:103-107 @ v2.5.2).
    #[doc(alias = "setGridSize")]
    pub fn set_grid_size(&mut self, grid_size: c_ulong) -> Result<()> {
        let lut_array = Lut3DArray::new(grid_size)?;
        *self.data.get_array_mut() = lut_array;
        Ok(())
    }

    /// The entry of red index `index_r`, green `index_g` and blue `index_b`: "Lut3DTransform
    /// getValue: <Red|Green|Blue> index (<index>) should be less than the grid size (<size>)."
    /// past the end.
    ///
    /// Port of `Lut3DTransformImpl::getValue` (Lut3DTransform.cpp:147-162 @ v2.5.2).
    #[doc(alias = "getValue")]
    pub fn value(&self, index_r: c_ulong, index_g: c_ulong, index_b: c_ulong) -> Result<[f32; 3]> {
        let gs = self.grid_size();
        check_lut3d_index("getValue", COMPONENT_R, index_r, gs)?;
        check_lut3d_index("getValue", COMPONENT_G, index_g, gs)?;
        check_lut3d_index("getValue", COMPONENT_B, index_b, gs)?;

        // Array is stored in blue-fastest order.
        let array_idx = (3 * ((index_r * gs + index_g) * gs + index_b)) as usize;
        let array = self.data.get_array();
        Ok([array[array_idx], array[array_idx + 1], array[array_idx + 2]])
    }

    /// Sets the entry of red index `index_r`, green `index_g` and blue `index_b`:
    /// "Lut3DTransform setValue: <Red|Green|Blue> index (<index>) should be less than the grid
    /// size (<size>)." past the end.
    ///
    /// Port of `Lut3DTransformImpl::setValue` (Lut3DTransform.cpp:129-144 @ v2.5.2).
    #[doc(alias = "setValue")]
    pub fn set_value(
        &mut self,
        index_r: c_ulong,
        index_g: c_ulong,
        index_b: c_ulong,
        r: f32,
        g: f32,
        b: f32,
    ) -> Result<()> {
        let gs = self.grid_size();
        check_lut3d_index("setValue", COMPONENT_R, index_r, gs)?;
        check_lut3d_index("setValue", COMPONENT_G, index_g, gs)?;
        check_lut3d_index("setValue", COMPONENT_B, index_b, gs)?;

        // Array is stored in blue-fastest order.
        let array_idx = (3 * ((index_r * gs + index_g) * gs + index_b)) as usize;
        let array = self.data.get_array_mut();
        array[array_idx] = r;
        array[array_idx + 1] = g;
        array[array_idx + 2] = b;
        Ok(())
    }

    /// Sets the interpolation; one a 3D LUT doesn't support fails `validate`.
    ///
    /// Port of `Lut3DTransformImpl::setInterpolation` (Lut3DTransform.cpp:164-167 @ v2.5.2).
    #[doc(alias = "setInterpolation")]
    pub fn set_interpolation(&mut self, algo: Interpolation) {
        self.data.set_interpolation(algo);
    }

    /// Port of `Lut3DTransformImpl::getInterpolation` (Lut3DTransform.cpp:169-172 @ v2.5.2).
    #[doc(alias = "getInterpolation")]
    pub fn interpolation(&self) -> Interpolation {
        self.data.get_interpolation()
    }

    /// Writes the transform's text to `os`, its numbers with the stream's precision: 6 on a new
    /// stream, 16 after a MatrixTransform in the same group (docs/improvements.md, I-73).
    ///
    /// The minimum and maximum of each channel are C++'s `std::min` and `std::max` from
    /// `FLT_MAX` and `-FLT_MAX`, so a NaN entry is skipped and a LUT of NaNs prints those
    /// starting values.
    ///
    /// Port of `operator<<(std::ostream &, const Lut3DTransform &)` (Lut3DTransform.cpp:
    /// 174-218 @ v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        os.put_str("<Lut3DTransform ");
        os.put_str("direction=");
        os.put_str(transform_direction_to_string(self.direction()));
        os.put_str(", ");
        os.put_str("fileoutdepth=");
        os.put_str(bit_depth_to_string(self.file_output_bit_depth()));
        os.put_str(", ");
        os.put_str("interpolation=");
        os.put_str(interpolation_to_string(self.interpolation()));
        os.put_str(", ");
        let l = self.grid_size();
        os.put_str("gridSize=");
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
            let n = l as usize;
            // `getValue(r, g, b)` for r, g, b in order: the array's order.
            for i in 0..n * n * n {
                let (rv, gv, bv) = (array[3 * i], array[3 * i + 1], array[3 * i + 2]);
                r_min = std_min(r_min, rv);
                g_min = std_min(g_min, gv);
                b_min = std_min(b_min, bv);
                r_max = std_max(r_max, rv);
                g_max = std_max(g_max, gv);
                b_max = std_max(b_max, bv);
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

/// `COMPONENT_R` (Lut3DTransform.cpp:111 @ v2.5.2).
const COMPONENT_R: &str = "Red";
/// `COMPONENT_G` (Lut3DTransform.cpp:112 @ v2.5.2).
const COMPONENT_G: &str = "Green";
/// `COMPONENT_B` (Lut3DTransform.cpp:113 @ v2.5.2).
const COMPONENT_B: &str = "Blue";

/// "Lut3DTransform <function>: <component> index (<index>) should be less than the grid size
/// (<size>)." when `index` is past the end.
///
/// Port of `CheckLUT3DIndex` (src/OpenColorIO/transforms/Lut3DTransform.cpp:114-126 @ v2.5.2).
fn check_lut3d_index(function: &str, component: &str, index: c_ulong, size: c_ulong) -> Result<()> {
    if index >= size {
        return Err(Exception::new(format!(
            "Lut3DTransform {function}: {component} index ({index}) should be less than the grid \
             size ({size})."
        )));
    }
    Ok(())
}

impl PartialEq for Lut3DTransform {
    /// [`Lut3DTransform::equals`].
    fn eq(&self, other: &Lut3DTransform) -> bool {
        self.equals(other)
    }
}

impl fmt::Display for Lut3DTransform {
    /// `<Lut3DTransform direction=<dir>, fileoutdepth=<depth>, interpolation=<interp>,
    /// gridSize=<n>, minrgb=[r, g, b], maxrgb=[r, g, b]>`.
    ///
    /// Port of `operator<<(std::ostream &, const Lut3DTransform &)` (Lut3DTransform.cpp:
    /// 174-218 @ v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(&os.to_string_lossy())
    }
}

/// Appends to `group` the transform of the Lut3D op `op`: a copy of its data, metadata, file
/// bit depth and direction included.
///
/// Port of `CreateLut3DTransform` (src/OpenColorIO/ops/lut3d/Lut3DOp.cpp:235-249 @ v2.5.2).
pub(crate) fn create_lut3d_transform(group: &mut GroupTransform, op: &Op) -> Result<()> {
    let OpData::Lut3D(lut_data) = &**op.data() else {
        return Err(Exception::new(
            "CreateLut3DTransform: op has to be a Lut3DOp",
        ));
    };
    let mut lut_transform = Lut3DTransform::new();
    lut_transform.data = lut_data.clone();

    group.append_transform(lut_transform.into());
    Ok(())
}

/// Validates the transform's data, then appends a Lut3D op of a copy of it, in the direction
/// `dir` combined with the data's.
///
/// Port of `BuildLut3DOp` (src/OpenColorIO/ops/lut3d/Lut3DOp.cpp:251-261 @ v2.5.2).
pub(crate) fn build_lut3d_op(
    ops: &mut OpVec,
    transform: &Lut3DTransform,
    dir: TransformDirection,
) -> Result<()> {
    let data = transform.data();
    data.validate()?;

    let lut = data.clone();
    create_lut3d_op(ops, lut, dir);
    Ok(())
}

#[cfg(test)]
#[path = "lut3d_transform_tests.rs"]
mod tests;
