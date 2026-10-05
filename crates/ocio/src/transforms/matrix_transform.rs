// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The matrix transform: a port of `src/OpenColorIO/transforms/MatrixTransform.h` and
//! `MatrixTransform.cpp` @ v2.5.2, with its op glue from `src/OpenColorIO/ops/matrix/
//! MatrixOp.cpp` (`CreateMatrixTransform`, `BuildMatrixOp`).
//!
//! `MatrixTransform::Fit` and `MatrixTransform::Sat` are computed by the ops crate, where
//! `CreateFitOp` and `CreateSaturationOp` call them ([`matrix_transform_fit`],
//! [`matrix_transform_sat`]); [`MatrixTransform::fit`] and [`MatrixTransform::sat`] return them.

use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::math_utils::{is_scalar_equal_to_zero, sse_add};
use ocio_ops::op::{Op, OpVec};
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::{
    BitDepth, TransformDirection, bit_depth_to_string, transform_direction_to_string,
};
use ocio_ops::ops::matrix::MatrixOpData;
use ocio_ops::ops::matrix::matrix_op::{
    create_matrix_op, matrix_transform_fit, matrix_transform_sat,
};

use crate::transform::validate_direction;
use crate::transforms::group_transform::GroupTransform;

/// A 4x4 matrix and its offsets, as the static functions return them: the 16 values row by row,
/// then the R, G, B, A offsets.
pub type MatrixAndOffset = ([f64; 16], [f64; 4]);

/// A matrix with offsets: `out = matrix * in + offset`, on RGBA.
///
/// The transform is its op data, as upstream's `MatrixTransformImpl` holds a `MatrixOpData`.
/// A copy is upstream's `createEditableCopy`.
///
/// Port of `MatrixTransform` and `MatrixTransformImpl` (include/OpenColorIO/
/// OpenColorTransforms.h:1985-2077, src/OpenColorIO/transforms/MatrixTransform.h,
/// MatrixTransform.cpp @ v2.5.2).
#[derive(Debug, Clone)]
pub struct MatrixTransform {
    /// `m_data`.
    data: MatrixOpData,
}

impl Default for MatrixTransform {
    fn default() -> Self {
        Self::new()
    }
}

impl MatrixTransform {
    /// The identity, forward, without offsets, with unknown file bit depths.
    ///
    /// Port of `MatrixTransform::Create` (MatrixTransform.cpp:13-16 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> MatrixTransform {
        MatrixTransform {
            data: MatrixOpData::new(),
        }
    }

    /// The transform of `data`, a copy: what `CreateMatrixTransform` builds from an op.
    pub(crate) fn from_data(data: MatrixOpData) -> MatrixTransform {
        MatrixTransform { data }
    }

    /// The op data the transform holds.
    ///
    /// Port of `MatrixTransformImpl::data() const` (MatrixTransform.h:48 @ v2.5.2).
    pub(crate) fn data(&self) -> &MatrixOpData {
        &self.data
    }

    /// Port of `MatrixTransformImpl::getDirection` (MatrixTransform.cpp:30-33 @ v2.5.2).
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        self.data.get_direction()
    }

    /// Port of `MatrixTransformImpl::setDirection` (MatrixTransform.cpp:35-38 @ v2.5.2).
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.data.set_direction(dir);
    }

    /// Checks the direction and the data, an inverse matrix's inverse included: "MatrixTransform
    /// validation failed: " and the first problem.
    ///
    /// Port of `MatrixTransformImpl::validate` (MatrixTransform.cpp:40-53 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        let checked = validate_direction(self.direction()).and_then(|()| self.data.validate_ref());
        checked.map_err(|ex| {
            Exception::new([b"MatrixTransform validation failed: ".as_slice(), ex.what()].concat())
        })
    }

    /// The bit depth of the file the values came from, at the input.
    ///
    /// Port of `MatrixTransformImpl::getFileInputBitDepth` (MatrixTransform.cpp:55-58 @ v2.5.2).
    #[doc(alias = "getFileInputBitDepth")]
    pub fn file_input_bit_depth(&self) -> BitDepth {
        self.data.get_file_input_bit_depth()
    }

    /// Port of `MatrixTransformImpl::getFileOutputBitDepth` (MatrixTransform.cpp:59-62 @
    /// v2.5.2).
    #[doc(alias = "getFileOutputBitDepth")]
    pub fn file_output_bit_depth(&self) -> BitDepth {
        self.data.get_file_output_bit_depth()
    }

    /// Port of `MatrixTransformImpl::setFileInputBitDepth` (MatrixTransform.cpp:63-66 @
    /// v2.5.2).
    #[doc(alias = "setFileInputBitDepth")]
    pub fn set_file_input_bit_depth(&mut self, bit_depth: BitDepth) {
        self.data.set_file_input_bit_depth(bit_depth);
    }

    /// Port of `MatrixTransformImpl::setFileOutputBitDepth` (MatrixTransform.cpp:67-70 @
    /// v2.5.2).
    #[doc(alias = "setFileOutputBitDepth")]
    pub fn set_file_output_bit_depth(&mut self, bit_depth: BitDepth) {
        self.data.set_file_output_bit_depth(bit_depth);
    }

    /// Port of `MatrixTransformImpl::getFormatMetadata() const` (MatrixTransform.cpp:77-80 @
    /// v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata(&self) -> &FormatMetadataImpl {
        self.data.get_format_metadata()
    }

    /// Port of `MatrixTransformImpl::getFormatMetadata()` (MatrixTransform.cpp:72-75 @
    /// v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        self.data.get_format_metadata_mut()
    }

    /// Whether `other` has the same direction, offsets and matrix: the data's equality, which
    /// ignores the metadata and the file bit depths. A transform equals itself, NaNs included.
    ///
    /// Port of `MatrixTransformImpl::equals` (MatrixTransform.cpp:82-86 @ v2.5.2).
    pub fn equals(&self, other: &MatrixTransform) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        self.data == other.data
    }

    /// Sets the 16 values, row by row: the first four make the R output from R, G, B, A.
    ///
    /// Port of `MatrixTransformImpl::setMatrix` (MatrixTransform.cpp:88-91 @ v2.5.2).
    #[doc(alias = "setMatrix")]
    pub fn set_matrix(&mut self, m44: &[f64; 16]) {
        self.data.set_rgba(m44);
    }

    /// The 16 values, row by row.
    ///
    /// Port of `MatrixTransformImpl::getMatrix` (MatrixTransform.cpp:93-124 @ v2.5.2).
    #[doc(alias = "getMatrix")]
    pub fn matrix(&self) -> [f64; 16] {
        let vals = self.data.get_array().get_values();
        let mut m44 = [0.0; 16];
        m44.copy_from_slice(&vals[..16]);
        m44
    }

    /// Sets the R, G, B, A offsets, added after the matrix.
    ///
    /// Port of `MatrixTransformImpl::setOffset` (MatrixTransform.cpp:126-129 @ v2.5.2).
    #[doc(alias = "setOffset")]
    pub fn set_offset(&mut self, offset4: &[f64; 4]) {
        self.data.set_rgba_offsets(offset4);
    }

    /// The R, G, B, A offsets.
    ///
    /// Port of `MatrixTransformImpl::getOffset` (MatrixTransform.cpp:131-150 @ v2.5.2).
    #[doc(alias = "getOffset")]
    pub fn offset(&self) -> [f64; 4] {
        *self.data.get_offsets().get_values()
    }

    /// The matrix and offsets that map `[old_min4, old_max4]` to `[new_min4, new_max4]`,
    /// channel by channel: "Cannot create Fit operator. Max value equals min value '<max>' in
    /// channel index <i>." where a channel's old range is 0.
    ///
    /// Port of `MatrixTransform::Fit` (MatrixTransform.cpp:152-188 @ v2.5.2), with both outputs
    /// asked for ([`matrix_transform_fit`]).
    #[doc(alias = "Fit")]
    pub fn fit(
        old_min4: &[f64; 4],
        old_max4: &[f64; 4],
        new_min4: &[f64; 4],
        new_max4: &[f64; 4],
    ) -> Result<MatrixAndOffset> {
        matrix_transform_fit(old_min4, old_max4, new_min4, new_max4)
    }

    /// The identity matrix, without offsets.
    ///
    /// Port of `MatrixTransform::Identity` (MatrixTransform.cpp:190-208 @ v2.5.2).
    #[doc(alias = "Identity")]
    pub fn identity() -> MatrixAndOffset {
        let mut m44 = [0.0; 16];
        m44[0] = 1.0;
        m44[5] = 1.0;
        m44[10] = 1.0;
        m44[15] = 1.0;

        (m44, [0.0; 4])
    }

    /// The matrix that sets the saturation to `sat` around the luma of `luma_coef3`, without
    /// offsets.
    ///
    /// Port of `MatrixTransform::Sat` (MatrixTransform.cpp:210-245 @ v2.5.2), with both outputs
    /// asked for ([`matrix_transform_sat`]).
    #[doc(alias = "Sat")]
    pub fn sat(sat: f64, luma_coef3: &[f64; 3]) -> MatrixAndOffset {
        matrix_transform_sat(sat, luma_coef3)
    }

    /// The diagonal matrix of `scale4`, without offsets.
    ///
    /// Port of `MatrixTransform::Scale` (MatrixTransform.cpp:247-268 @ v2.5.2).
    #[doc(alias = "Scale")]
    pub fn scale(scale4: &[f64; 4]) -> MatrixAndOffset {
        let mut m44 = [0.0; 16];
        m44[0] = scale4[0];
        m44[5] = scale4[1];
        m44[10] = scale4[2];
        m44[15] = scale4[3];

        (m44, [0.0; 4])
    }

    /// The matrix that shows the channels `channel_hot4` marks (non-zero): the identity when
    /// all four are; else alpha in every channel when alpha is; else the hot channels' luma
    /// (`luma_coef3`), normalized unless their sum is about 0, in R, G and B, alpha kept.
    /// Without offsets.
    ///
    /// Port of `MatrixTransform::View` (MatrixTransform.cpp:269-334 @ v2.5.2).
    #[doc(alias = "View")]
    pub fn view(channel_hot4: &[i32; 4], luma_coef3: &[f64; 3]) -> MatrixAndOffset {
        let offset4 = [0.0; 4];
        let mut m44 = [0.0; 16];

        if channel_hot4[0] != 0
            && channel_hot4[1] != 0
            && channel_hot4[2] != 0
            && channel_hot4[3] != 0
        {
            // All channels are hot, return identity.
            m44 = Self::identity().0;
        } else if channel_hot4[3] != 0 {
            // If not all the channels are hot, but alpha is, just show it.
            for i in 0..4 {
                m44[4 * i + 3] = 1.0;
            }
        } else {
            // Blend rgb as specified, place it in all 3 output channels (to make a grayscale
            // final image).
            let mut values = [0.0f64; 3];

            for i in 0..3 {
                values[i] += luma_coef3[i] * if channel_hot4[i] != 0 { 1.0 } else { 0.0 };
            }

            // `values[0] + values[1] + values[2]`. Where two NaNs meet, the sum keeps one of
            // them: the Windows wheel adds the first two in the other order, the Linux wheel in
            // the source's, so a NaN luma's sign can come out differently on each
            // (docs/improvements.md, I-74; seen through each wheel's `MatrixTransform.View`).
            let first_two = if cfg!(target_os = "windows") {
                sse_add(values[1], values[0])
            } else {
                sse_add(values[0], values[1])
            };
            let sum = sse_add(first_two, values[2]);
            if !is_scalar_equal_to_zero(sum) {
                values[0] /= sum;
                values[1] /= sum;
                values[2] /= sum;
            }

            // Copy rgb into rgb rows.
            for row in 0..3 {
                for i in 0..3 {
                    m44[4 * row + i] = values[i];
                }
            }

            // Preserve alpha.
            m44[15] = 1.0;
        }

        (m44, offset4)
    }

    /// Writes the transform's text to `os`. It sets the stream's precision to 16 and leaves it
    /// there, so what a group prints after it prints its numbers with 16 digits too
    /// (docs/improvements.md, I-73).
    ///
    /// Port of `operator<<(std::ostream &, const MatrixTransform &)` (MatrixTransform.cpp:
    /// 336-366 @ v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        /// `DOUBLE_DECIMALS` (MatrixTransform.cpp:338 @ v2.5.2).
        const DOUBLE_DECIMALS: i64 = 16;

        let matrix = self.matrix();
        let offset = self.offset();

        os.precision = DOUBLE_DECIMALS;

        os.put_str("<MatrixTransform ");
        os.put_str("direction=");
        os.put_str(transform_direction_to_string(self.direction()));
        os.put_str(", fileindepth=");
        os.put_str(bit_depth_to_string(self.file_input_bit_depth()));
        os.put_str(", fileoutdepth=");
        os.put_str(bit_depth_to_string(self.file_output_bit_depth()));
        os.put_str(", matrix=[");
        os.put_f64(matrix[0]);
        for value in &matrix[1..] {
            os.put_str(", ");
            os.put_f64(*value);
        }
        os.put_str("], offset=[");
        os.put_f64(offset[0]);
        for value in &offset[1..] {
            os.put_str(", ");
            os.put_f64(*value);
        }
        os.put_str("]>");
    }
}

impl PartialEq for MatrixTransform {
    /// [`MatrixTransform::equals`].
    fn eq(&self, other: &MatrixTransform) -> bool {
        self.equals(other)
    }
}

impl fmt::Display for MatrixTransform {
    /// `<MatrixTransform direction=<dir>, fileindepth=<depth>, fileoutdepth=<depth>,
    /// matrix=[<16 values>], offset=[<4 values>]>`, the values with 16 significant digits.
    ///
    /// Port of `operator<<(std::ostream &, const MatrixTransform &)` (MatrixTransform.cpp:
    /// 336-366 @ v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(&os.to_string_lossy())
    }
}

/// Appends to `group` the transform of the Matrix op `op`: a copy of its data, metadata and
/// file bit depths included.
///
/// Port of `CreateMatrixTransform` (src/OpenColorIO/ops/matrix/MatrixOp.cpp:363-377 @ v2.5.2).
pub(crate) fn create_matrix_transform(group: &mut GroupTransform, op: &Op) -> Result<()> {
    let OpData::Matrix(mat_data_src) = &**op.data() else {
        return Err(Exception::new(
            "CreateMatrixTransform: op has to be a MatrixOffsetOp",
        ));
    };
    let mat_transform = MatrixTransform::from_data(mat_data_src.clone());

    group.append_transform(mat_transform.into());
    Ok(())
}

/// Validates the transform's data, then appends the op of a copy of it in the direction `dir`
/// combined with the data's.
///
/// Port of `BuildMatrixOp` (src/OpenColorIO/ops/matrix/MatrixOp.cpp:379-388 @ v2.5.2).
pub(crate) fn build_matrix_op(
    ops: &mut OpVec,
    transform: &MatrixTransform,
    dir: TransformDirection,
) -> Result<()> {
    let data = transform.data();
    data.validate_ref()?;

    let mat = data.clone();
    create_matrix_op(ops, mat, dir);
    Ok(())
}

#[cfg(test)]
#[path = "matrix_transform_tests.rs"]
mod tests;
