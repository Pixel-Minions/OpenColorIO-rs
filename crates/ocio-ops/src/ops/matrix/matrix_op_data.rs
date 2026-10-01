// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Matrix op's data: a 4x4 matrix and 4 offsets, applied as
//! `out = matrix * in + offsets` to RGBA. A port of `src/OpenColorIO/ops/matrix/MatrixOpData.h`
//! and `MatrixOpData.cpp` @ v2.5.2.
//!
//! The matrix math here, the product (`MatrixArray::inner`) and the inverse
//! (`MatrixArray::inverse`, Imath's Gauss-Jordan elimination), is what the wheel computes with;
//! `MathUtils`' `GetM44*` functions are not. Where two NaNs can meet in a product or a sum, the
//! operands are in the order the wheel's machine code has them, per platform, and where a
//! compiler turned an operation into another (GCC's negation for `getAsForward`'s
//! `scale(-1.)`), the port does what that wheel does (`tools/wheel-inspect`, noted at each
//! place).
//!
//! Upstream's `MatrixArray::validate` is `const`, and turns a 3x3 array (from a CLF or CTF file)
//! into the canonical 4x4 through a `const_cast`. The port's
//! [`MatrixOpData::validate`] and [`MatrixArray::validate`] take `&mut self` for that, and
//! [`MatrixOpData::validate_ref`] runs the same checks on a shared reference, on a converted
//! copy.

use core::ffi::c_ulong;
use std::ops::{Index, IndexMut};

use crate::exception::{Exception, Result};
use crate::format_metadata::{FormatMetadataImpl, METADATA_ID, METADATA_NAME};
use crate::hash_utils::cache_id_hash;
use crate::math_utils::{equal_with_abs_error, sse_add, sse_mul};
use crate::op_data::OpDataType;
use crate::open_color_types::{BitDepth, TransformDirection, transform_direction_to_string};
use crate::ops::op_array::ArrayDouble;
use crate::utils::string_utils::c_str;

/// The offsets of a matrix op: one per channel, RGBA.
///
/// `==` compares the bits, as upstream's `memcmp`: NaNs of the same bits are equal, `0.0` and
/// `-0.0` are not.
///
/// Port of `MatrixOpData::Offsets` (src/OpenColorIO/ops/matrix/MatrixOpData.h:41-85,
/// MatrixOpData.cpp:17-90 @ v2.5.2). Its setters' null-pointer checks ("Matrix: setRGB NULL
/// pointer.") have no Rust counterpart: arrays aren't null.
#[derive(Debug, Clone, Copy, Default)]
pub struct Offsets {
    /// `m_values`.
    values: [f64; 4],
}

impl Offsets {
    /// Port of `Offsets::Offsets(double, double, double, double)` (MatrixOpData.cpp:17-23 @
    /// v2.5.2).
    pub fn new(red: f64, green: f64, blue: f64, white: f64) -> Self {
        Offsets {
            values: [red, green, blue, white],
        }
    }

    /// Sets red, green and blue, and alpha to 0.
    ///
    /// Port of `Offsets::setRGB<T>` (MatrixOpData.cpp:44-56 @ v2.5.2).
    pub fn set_rgb<T: Copy + Into<f64>>(&mut self, v3: &[T; 3]) {
        self.values = [v3[0].into(), v3[1].into(), v3[2].into(), 0.0];
    }

    /// Port of `Offsets::setRGBA<T>` (MatrixOpData.cpp:61-73 @ v2.5.2).
    pub fn set_rgba<T: Copy + Into<f64>>(&mut self, v4: &[T; 4]) {
        self.values = v4.map(Into::into);
    }

    /// Port of `Offsets::getValues() const` (MatrixOpData.h:69-72 @ v2.5.2).
    pub fn get_values(&self) -> &[f64; 4] {
        &self.values
    }

    /// Port of `Offsets::getValues()` (MatrixOpData.h:74-77 @ v2.5.2).
    pub fn get_values_mut(&mut self) -> &mut [f64; 4] {
        &mut self.values
    }

    /// Whether an offset isn't 0.
    ///
    /// Port of `Offsets::isNotNull` (MatrixOpData.cpp:78-82 @ v2.5.2).
    pub fn is_not_null(&self) -> bool {
        self.values[0] != 0. || self.values[1] != 0. || self.values[2] != 0. || self.values[3] != 0.
    }

    /// Multiplies each offset by `s`.
    ///
    /// Port of `Offsets::scale` (MatrixOpData.cpp:84-90 @ v2.5.2).
    pub fn scale(&mut self, s: f64) {
        for value in &mut self.values {
            *value = sse_mul(*value, s);
        }
    }
}

impl PartialEq for Offsets {
    /// Port of `Offsets::operator==` (MatrixOpData.cpp:39-42 @ v2.5.2): `memcmp` of the values.
    fn eq(&self, other: &Offsets) -> bool {
        self.values.map(f64::to_bits) == other.values.map(f64::to_bits)
    }
}

impl Index<usize> for Offsets {
    type Output = f64;

    /// Port of `Offsets::operator[] const` (MatrixOpData.h:59-62 @ v2.5.2).
    fn index(&self, index: usize) -> &f64 {
        &self.values[index]
    }
}

impl IndexMut<usize> for Offsets {
    /// Port of `Offsets::operator[]` (MatrixOpData.h:64-67 @ v2.5.2).
    fn index_mut(&mut self, index: usize) -> &mut f64 {
        &mut self.values[index]
    }
}

/// The matrix of a matrix op: `length` by `length` values, row by row, 4 by 4 once validated.
///
/// Port of `MatrixOpData::MatrixArray` (src/OpenColorIO/ops/matrix/MatrixOpData.h:87-125,
/// MatrixOpData.cpp:92-435 @ v2.5.2), an `ArrayDouble`.
#[derive(Debug, Clone)]
pub struct MatrixArray {
    array: ArrayDouble,
}

impl Default for MatrixArray {
    fn default() -> Self {
        Self::new()
    }
}

impl MatrixArray {
    /// The 4x4 identity.
    ///
    /// Port of `MatrixArray::MatrixArray()` (MatrixOpData.cpp:92-96 @ v2.5.2).
    pub fn new() -> Self {
        let mut matrix = MatrixArray {
            array: ArrayDouble::new(),
        };
        matrix.resize(4, 4);
        matrix.fill();
        matrix
    }

    /// Sets the dimensions, and resizes the values to `length * length`.
    ///
    /// Port of `ArrayT::resize` (src/OpenColorIO/ops/OpArray.h:57-62 @ v2.5.2) with
    /// `MatrixArray::getNumValues`.
    pub fn resize(&mut self, length: c_ulong, num_color_components: c_ulong) {
        let num_values = length.wrapping_mul(length);
        self.array.resize(length, num_color_components, num_values);
    }

    /// Port of `ArrayT::getLength` (OpArray.h:83-86 @ v2.5.2).
    pub fn get_length(&self) -> c_ulong {
        self.array.get_length()
    }

    /// Port of `ArrayT::getNumColorComponents` (OpArray.h:97-100 @ v2.5.2).
    pub fn get_num_color_components(&self) -> c_ulong {
        self.array.get_num_color_components()
    }

    /// The number of values the dimensions call for: `length * length`.
    ///
    /// Port of `MatrixArray::getNumValues` (MatrixOpData.cpp:312-315 @ v2.5.2).
    pub fn get_num_values(&self) -> c_ulong {
        self.get_length().wrapping_mul(self.get_length())
    }

    /// Port of `ArrayT::getValues() const` (OpArray.h:144-147 @ v2.5.2).
    pub fn get_values(&self) -> &Vec<f64> {
        self.array.get_values()
    }

    /// Port of `ArrayT::getValues()` (OpArray.h:149-152 @ v2.5.2).
    pub fn get_values_mut(&mut self) -> &mut Vec<f64> {
        self.array.get_values_mut()
    }

    /// Whether this is the same array, or has the same dimensions and values.
    ///
    /// Port of `ArrayT::operator==` (OpArray.h:182-188 @ v2.5.2).
    pub fn equals(&self, other: &MatrixArray) -> bool {
        self.array.equals(&other.array)
    }

    /// Multiplies every value by `scale`, unless it is 1.
    ///
    /// Port of `ArrayT::scale` (OpArray.h:190-200 @ v2.5.2), inherited: [`ArrayT::scale`].
    ///
    /// [`ArrayT::scale`]: crate::ops::op_array::ArrayT::scale
    pub fn scale(&mut self, scale: f64) {
        self.array.scale(scale);
    }

    /// Whether the matrix is the identity: exactly 1 on the diagonal, 0 elsewhere.
    ///
    /// Port of `MatrixArray::isUnityDiagonal` (MatrixOpData.cpp:317-344 @ v2.5.2).
    pub fn is_unity_diagonal(&self) -> bool {
        let dim = self.get_length() as usize;
        let values = self.get_values();

        for i in 0..dim {
            for j in 0..dim {
                if i == j {
                    if values[i * dim + j] != 1.0 {
                        // Strict comparison intended
                        return false;
                    }
                } else if values[i * dim + j] != 0.0 {
                    // Strict comparison intended
                    return false;
                }
            }
        }

        true
    }

    /// Sets the identity.
    ///
    /// Port of `MatrixArray::fill` (MatrixOpData.cpp:346-363 @ v2.5.2).
    fn fill(&mut self) {
        let dim = self.get_length() as usize;
        let values = self.get_values_mut();
        values.fill(0.0);

        for i in 0..dim {
            for j in 0..dim {
                if i == j {
                    values[i * dim + j] = 1.0;
                }
            }
        }
    }

    /// Makes a 3x3 matrix 4x4: its values become the RGB part, with alpha passed through.
    ///
    /// Port of `MatrixArray::expandFrom3x3To4x4` (MatrixOpData.cpp:365-372 @ v2.5.2).
    fn expand_from_3x3_to_4x4(&mut self) {
        let old_values = self.get_values().clone();

        self.resize(4, 4);

        self.set_rgb(&[
            old_values[0],
            old_values[1],
            old_values[2],
            old_values[3],
            old_values[4],
            old_values[5],
            old_values[6],
            old_values[7],
            old_values[8],
        ]);
    }

    /// Sets the RGB part, with alpha passed through: the last row and column are those of the
    /// identity.
    ///
    /// Port of `MatrixArray::setRGB<T>` (MatrixOpData.cpp:283-307 @ v2.5.2).
    pub fn set_rgb<T: Copy + Into<f64>>(&mut self, values: &[T; 9]) {
        let v = self.get_values_mut();
        let values = values.map(Into::into);

        v[0] = values[0];
        v[1] = values[1];
        v[2] = values[2];
        v[3] = 0.;

        v[4] = values[3];
        v[5] = values[4];
        v[6] = values[5];
        v[7] = 0.;

        v[8] = values[6];
        v[9] = values[7];
        v[10] = values[8];
        v[11] = 0.;

        v[12] = 0.;
        v[13] = 0.;
        v[14] = 0.;
        v[15] = 1.;
    }

    /// Sets the 16 values, row by row. The null-pointer check ("Matrix: setRGBA NULL pointer.")
    /// has no Rust counterpart.
    ///
    /// Port of `MatrixArray::setRGBA(const float *)` and `setRGBA(const double *)`
    /// (MatrixOpData.cpp:374-411 @ v2.5.2).
    pub fn set_rgba<T: Copy + Into<f64>>(&mut self, values: &[T; 16]) {
        self.get_values_mut()[..16].copy_from_slice(&values.map(Into::into));
    }

    /// Checks the values, and makes a 3x3 matrix the canonical 4x4 (upstream does it in its
    /// `const` method, through a `const_cast`).
    ///
    /// Port of `MatrixArray::validate` (MatrixOpData.cpp:413-435 @ v2.5.2).
    pub fn validate(&mut self) -> Result<()> {
        // Note: By design, only 4x4 matrices are instantiated. The CLF 3x3 (and 3x4) matrices
        // are automatically converted to 4x4 matrices, and a Matrix Transform only expects 4x4
        // matrices.

        self.array.validate(self.get_num_values())?;

        // A 4x4 matrix is the canonical form, convert if it is only a 3x3.
        if self.get_length() == 3 {
            self.expand_from_3x3_to_4x4();
        } else if self.get_length() != 4 {
            return Err(Exception::new("Matrix: array content issue."));
        }

        if self.get_num_color_components() != 4 {
            return Err(Exception::new("Matrix: dimensions must be 4x4."));
        }
        Ok(())
    }

    /// [`validate`](Self::validate) on a shared reference: a 3x3 matrix is checked in its 4x4
    /// form, on a copy; `validate` changes no other.
    pub fn validate_ref(&self) -> Result<()> {
        if self.get_length() == 3 {
            return self.clone().validate();
        }

        self.array.validate(self.get_num_values())?;

        if self.get_length() != 4 {
            return Err(Exception::new("Matrix: array content issue."));
        }

        if self.get_num_color_components() != 4 {
            return Err(Exception::new("Matrix: dimensions must be 4x4."));
        }
        Ok(())
    }

    /// The product `self * b`, 4 by 4, computed in `double`.
    ///
    /// Both wheels compute each product with the element of `b` first, `b[i][col] * a[row][i]`
    /// (the Windows wheel at 0x1802b428d, `movsd xmm1, [rbp + rax*8]` then `mulsd xmm1,
    /// [rsi + r9*8]`; the Linux wheel's `mulpd` at 0x4e7b14), and add it to the sum, `accum +
    /// product`.
    ///
    /// Port of `MatrixArray::inner(const MatrixArray &)` (MatrixOpData.cpp:109-140 @ v2.5.2).
    pub fn inner(&self, b: &MatrixArray) -> MatrixArray {
        // Use operator= to make sure we have a 4x4 copy of the original matrices.
        let a_vals = self.get_values();
        let b_vals = b.get_values();

        let mut out = MatrixArray::new();
        let dim = out.get_length() as usize;
        let o_vals = out.get_values_mut();

        // Note: The matrix elements are stored in the vector in row-major order.
        // [ a00, a01, a02, a03, a10, a11, a12, a13, a20, ... a44 ]
        for row in 0..dim {
            for col in 0..dim {
                let mut accum = 0.;
                for i in 0..dim {
                    accum = sse_add(accum, sse_mul(b_vals[i * dim + col], a_vals[row * dim + i]));
                }
                o_vals[row * dim + col] = accum;
            }
        }

        out
    }

    /// The product of the matrix and the offsets `b`, computed in `double`, in the source's
    /// order on both wheels (the Windows wheel at 0x1802b4454; the Linux wheel's `mulpd` at
    /// 0x4e50c8, whose first addition, `product + 0.0`, gives the same bits).
    ///
    /// Port of `MatrixArray::inner(const Offsets &)` (MatrixOpData.cpp:147-165 @ v2.5.2).
    pub fn inner_offsets(&self, b: &Offsets) -> Offsets {
        let mut out = Offsets::default();

        let dim = self.get_length() as usize;
        let a_vals = self.get_values();

        for i in 0..dim {
            let mut accum = 0.;
            for j in 0..dim {
                accum = sse_add(accum, sse_mul(a_vals[i * dim + j], b[j]));
            }
            out[i] = accum;
        }

        out
    }

    /// The inverse, by Gauss-Jordan elimination with partial pivoting, in `double`: "Singular
    /// Matrix can't be inverted." for a pivot of 0. The matrix is validated first, so a 3x3
    /// matrix is inverted as its 4x4 form.
    ///
    /// Port of `MatrixArray::inverse` (MatrixOpData.cpp:167-281 @ v2.5.2), itself a copy of
    /// Imath's `Matrix44<T>::gjInverse`.
    pub fn inverse(&self) -> Result<MatrixArray> {
        // Call validate to ensure that the matrix is 4x4, will be expanded if only 3x3.
        let mut t = self.clone();
        t.validate()?;

        // Create a new matrix array. The new matrix is initialized as identity.
        let mut s = MatrixArray::new();

        let dim = s.get_length() as usize;

        // Inversion starts with identity (without bit-depth scaling).
        s[0] = 1.;
        s[5] = 1.;
        s[10] = 1.;
        s[15] = 1.;

        // From Imath
        // Code copied from Matrix44<T>::gjInverse (bool singExc) const in ImathMatrix.h

        // Forward elimination.

        for i in 0..3 {
            let mut pivot = i;

            let mut pivotsize = t[i * dim + i];

            if pivotsize < 0. {
                pivotsize = -pivotsize;
            }

            for j in i + 1..4 {
                let mut tmp = t[j * dim + i];

                if tmp < 0.0 {
                    tmp = -tmp;
                }

                if tmp > pivotsize {
                    pivot = j;
                    pivotsize = tmp;
                }
            }

            if pivotsize == 0.0 {
                return Err(Exception::new("Singular Matrix can't be inverted."));
            }

            if pivot != i {
                for j in 0..4 {
                    t.get_values_mut().swap(i * dim + j, pivot * dim + j);
                    s.get_values_mut().swap(i * dim + j, pivot * dim + j);
                }
            }

            for j in i + 1..4 {
                let f = t[j * dim + i] / t[i * dim + i];

                for k in 0..4 {
                    t[j * dim + k] -= elimination_product(f, t[i * dim + k], false);
                    s[j * dim + k] -= elimination_product(f, s[i * dim + k], k == 3);
                }
            }
        }

        // Backward substitution.

        for i in (0..4).rev() {
            let mut f = t[i * dim + i];

            // TODO: Perhaps change to throw even if f is near zero (nearly singular).
            if f == 0.0 {
                return Err(Exception::new("Singular Matrix can't be inverted."));
            }

            for j in 0..4 {
                t[i * dim + j] /= f;
                s[i * dim + j] /= f;
            }

            for j in 0..i {
                f = t[j * dim + i];

                for k in 0..4 {
                    t[j * dim + k] -= elimination_product(f, t[i * dim + k], false);
                    s[j * dim + k] -= elimination_product(f, s[i * dim + k], k == 3);
                }
            }
        }

        Ok(s)
    }
}

/// `f * x` in the elimination of [`MatrixArray::inverse`] (`t[..] -= f * t[..]`), with the
/// operands in the order each wheel's machine code has them. The Windows wheel keeps the
/// source's, `f * x` (0x1802b4758, `mulsd xmm1, [r11 + r9*8]` with `xmm1 = f`). The Linux wheel
/// computes `x * f`, except for the last product of each step, `f * s[i*4 + 3]`, whose register
/// it reuses (its 9 unrolled steps, e.g. 0x4e673e `mulsd xmm2, xmm0` and 0x4e67de `mulsd xmm0,
/// [rsi + 0x18]`). When both are NaN, the first one's comes out.
fn elimination_product(f: f64, x: f64, last_of_step: bool) -> f64 {
    if cfg!(target_os = "windows") || last_of_step {
        sse_mul(f, x)
    } else {
        sse_mul(x, f)
    }
}

impl Index<usize> for MatrixArray {
    type Output = f64;

    /// Port of `ArrayT::operator[] const` (OpArray.h:154-157 @ v2.5.2).
    fn index(&self, index: usize) -> &f64 {
        &self.array[index]
    }
}

impl IndexMut<usize> for MatrixArray {
    /// Port of `ArrayT::operator[]` (OpArray.h:159-162 @ v2.5.2).
    fn index_mut(&mut self, index: usize) -> &mut f64 {
        &mut self.array[index]
    }
}

/// The data of a matrix op:
///
/// ```text
/// Rout = a[0][0]*Rin + a[0][1]*Gin + a[0][2]*Bin + a[0][3]*Ain + o[0];
/// Gout = a[1][0]*Rin + a[1][1]*Gin + a[1][2]*Bin + a[1][3]*Ain + o[1];
/// Bout = a[2][0]*Rin + a[2][1]*Gin + a[2][2]*Bin + a[2][3]*Ain + o[2];
/// Aout = a[3][0]*Rin + a[3][1]*Gin + a[3][2]*Bin + a[3][3]*Ain + o[3];
/// ```
///
/// with its direction, and the bit depths of the file it came from. `Clone` is upstream's copy
/// constructor, and `clone()`.
///
/// Port of `MatrixOpData` (src/OpenColorIO/ops/matrix/MatrixOpData.h:35-245,
/// MatrixOpData.cpp:437-882 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct MatrixOpData {
    /// The `OpData` base's `m_metadata`.
    metadata: FormatMetadataImpl,
    /// `m_array`.
    array: MatrixArray,
    /// `m_offsets`.
    offsets: Offsets,
    /// `m_fileInBitDepth`: the input bit depth of the file the matrix comes from.
    file_in_bit_depth: BitDepth,
    /// `m_fileOutBitDepth`.
    file_out_bit_depth: BitDepth,
    /// `m_direction`.
    direction: TransformDirection,
}

impl Default for MatrixOpData {
    fn default() -> Self {
        Self::new()
    }
}

impl MatrixOpData {
    /// The identity, forward.
    ///
    /// Port of `MatrixOpData::MatrixOpData()` (MatrixOpData.cpp:439-443 @ v2.5.2).
    pub fn new() -> Self {
        MatrixOpData::from_array(MatrixArray::new())
    }

    /// The identity, in the direction `direction`.
    ///
    /// Port of `MatrixOpData::MatrixOpData(TransformDirection)` (MatrixOpData.cpp:451-456 @
    /// v2.5.2).
    pub fn with_direction(direction: TransformDirection) -> Self {
        let mut matrix = MatrixOpData::new();
        matrix.set_direction(direction);
        matrix
    }

    /// The matrix `matrix`, without offsets, forward.
    ///
    /// Port of `MatrixOpData::MatrixOpData(const MatrixArray &)` (MatrixOpData.cpp:445-449 @
    /// v2.5.2).
    pub fn from_array(matrix: MatrixArray) -> Self {
        MatrixOpData {
            metadata: FormatMetadataImpl::default(),
            array: matrix,
            offsets: Offsets::default(),
            file_in_bit_depth: BitDepth::Unknown,
            file_out_bit_depth: BitDepth::Unknown,
            direction: TransformDirection::Forward,
        }
    }

    /// A matrix with `diag_value` on the diagonal, alpha included, and no offset.
    ///
    /// Port of `MatrixOpData::CreateDiagonalMatrix` (MatrixOpData.cpp:616-629 @ v2.5.2).
    pub fn create_diagonal_matrix(diag_value: f64) -> MatrixOpData {
        // Create a matrix with no offset.
        let mut matrix = MatrixOpData::new();

        matrix
            .validate()
            .expect("a new matrix is the valid identity");

        matrix.set_array_value(0, diag_value);
        matrix.set_array_value(5, diag_value);
        matrix.set_array_value(10, diag_value);
        matrix.set_array_value(15, diag_value);

        matrix
    }

    /// Port of `MatrixOpData::getType` (MatrixOpData.h:186 @ v2.5.2).
    pub fn get_type(&self) -> OpDataType {
        OpDataType::Matrix
    }

    /// Port of `MatrixOpData::getArray() const` (MatrixOpData.h:137 @ v2.5.2).
    pub fn get_array(&self) -> &MatrixArray {
        &self.array
    }

    /// Port of `MatrixOpData::getArray()` (MatrixOpData.h:139 @ v2.5.2).
    pub fn get_array_mut(&mut self) -> &mut MatrixArray {
        &mut self.array
    }

    /// Port of `MatrixOpData::setArrayValue` (MatrixOpData.cpp:467-470 @ v2.5.2).
    pub fn set_array_value(&mut self, index: usize, value: f64) {
        self.array.get_values_mut()[index] = value;
    }

    /// Port of `MatrixOpData::getArrayValue` (MatrixOpData.cpp:472-475 @ v2.5.2).
    pub fn get_array_value(&self, index: usize) -> f64 {
        self.array.get_values()[index]
    }

    /// Sets the RGB part (alpha passed through).
    ///
    /// Port of `MatrixOpData::setRGB(const float *)` (MatrixOpData.cpp:477-480 @ v2.5.2).
    pub fn set_rgb(&mut self, values: &[f32; 9]) {
        self.array.set_rgb(values);
    }

    /// Sets the 16 values, row by row.
    ///
    /// Port of `MatrixOpData::setRGBA<T>` (MatrixOpData.cpp:482-489 @ v2.5.2).
    pub fn set_rgba<T: Copy + Into<f64>>(&mut self, values: &[T; 16]) {
        self.array.set_rgba(values);
    }

    /// Port of `MatrixOpData::getOffsets() const` (MatrixOpData.h:150-153 @ v2.5.2).
    pub fn get_offsets(&self) -> &Offsets {
        &self.offsets
    }

    /// Port of `MatrixOpData::getOffsets()` (MatrixOpData.h:155-158 @ v2.5.2).
    pub fn get_offsets_mut(&mut self) -> &mut Offsets {
        &mut self.offsets
    }

    /// The error of the offset accessors for an index past the matrix's size.
    fn offset_index_error(&self, index: c_ulong) -> Exception {
        // TODO: should never happen. Consider assert.
        Exception::new(format!(
            "Matrix array content issue: '{}' offset index out of range '{index}'. ",
            String::from_utf8_lossy(c_str(self.get_id()))
        ))
    }

    /// The offset at `index`, below the matrix's size.
    ///
    /// Port of `MatrixOpData::getOffsetValue` (MatrixOpData.cpp:631-648 @ v2.5.2).
    pub fn get_offset_value(&self, index: c_ulong) -> Result<f64> {
        let dim = self.get_array().get_length();
        if index >= dim {
            return Err(self.offset_index_error(index));
        }

        Ok(self.offsets[index as usize])
    }

    /// Sets the offset at `index`, below the matrix's size.
    ///
    /// Port of `MatrixOpData::setOffsetValue` (MatrixOpData.cpp:650-667 @ v2.5.2).
    pub fn set_offset_value(&mut self, index: c_ulong, value: f64) -> Result<()> {
        let dim = self.get_array().get_length();
        if index >= dim {
            return Err(self.offset_index_error(index));
        }

        self.offsets[index as usize] = value;
        Ok(())
    }

    /// Sets red, green and blue's offsets, and alpha's to 0.
    ///
    /// Port of `MatrixOpData::setRGBOffsets` (MatrixOpData.h:162-165 @ v2.5.2).
    pub fn set_rgb_offsets(&mut self, offsets: &[f32; 3]) {
        self.offsets.set_rgb(offsets);
    }

    /// Port of `MatrixOpData::setRGBAOffsets(const float *)` and `setRGBAOffsets(const double
    /// *)` (MatrixOpData.h:167-175 @ v2.5.2).
    pub fn set_rgba_offsets<T: Copy + Into<f64>>(&mut self, offsets: &[T; 4]) {
        self.offsets.set_rgba(offsets);
    }

    /// Port of `MatrixOpData::setOffsets` (MatrixOpData.h:177-180 @ v2.5.2).
    pub fn set_offsets(&mut self, offsets: Offsets) {
        self.offsets = offsets;
    }

    /// Checks the matrix, prefixing its error with "Matrix array content issue: ", and that an
    /// inverse matrix can be inverted. A 3x3 matrix becomes the canonical 4x4.
    ///
    /// Port of `MatrixOpData::validate` (MatrixOpData.cpp:491-510 @ v2.5.2).
    pub fn validate(&mut self) -> Result<()> {
        if let Err(e) = self.array.validate() {
            return Err(Exception::new(format!(
                "Matrix array content issue: {}",
                e.message()
            )));
        }
        if self.direction == TransformDirection::Inverse {
            // Make sure matrix can be inverted.
            self.get_as_forward()?;
        }
        Ok(())
    }

    /// [`validate`](Self::validate) on a shared reference. A 3x3 matrix is checked in its 4x4
    /// form, which isn't kept; any other is left alone by `validate`, which this runs on a copy
    /// only for a 3x3 one. An op keeps the 4x4 form: [`crate::op::Op::validate`] runs
    /// `validate` on the op's own data.
    pub fn validate_ref(&self) -> Result<()> {
        if self.array.get_length() == 3 {
            return self.clone().validate();
        }
        if let Err(e) = self.array.validate_ref() {
            return Err(Exception::new(format!(
                "Matrix array content issue: {}",
                e.message()
            )));
        }
        if self.direction == TransformDirection::Inverse {
            // Make sure matrix can be inverted.
            self.get_as_forward()?;
        }
        Ok(())
    }

    // We do a number of exact floating-point comparisons in the following methods. Note that
    // this op may be used to do very fine adjustments to pixels. Therefore it is problematic to
    // attempt to judge values passed in from a user's transform as to whether they are "close
    // enough" to e.g. 1 or 0. However, we still want to allow a matrix and its inverse to be
    // composed and be able to call the result an identity (recognizing it won't quite be).
    // Therefore, the strategy here is to do exact compares on users files but to "clean up"
    // matrices as part of composition to make this work in practice. The concept is that the
    // tolerances are moved to where errors are introduced rather than indiscriminately applying
    // them to all user ops.

    /// Whether the matrix, ignoring the offsets, is the identity.
    ///
    /// Port of `MatrixOpData::isUnityDiagonal` (MatrixOpData.cpp:524-527 @ v2.5.2).
    pub fn is_unity_diagonal(&self) -> bool {
        self.array.is_unity_diagonal()
    }

    /// Port of `MatrixOpData::isNoOp` (MatrixOpData.cpp:529-532 @ v2.5.2); an error for an
    /// unvalidated 3x3 matrix ([`has_alpha`](Self::has_alpha), docs/improvements.md U-16).
    pub fn is_no_op(&self) -> Result<bool> {
        self.is_identity()
    }

    /// Whether the op is the identity: no offset, alpha passed through, a diagonal matrix, and
    /// each diagonal value within 1e-6 of 1.
    ///
    /// Port of `MatrixOpData::isIdentity` (MatrixOpData.cpp:534-564 @ v2.5.2); an error for an
    /// unvalidated 3x3 matrix ([`has_alpha`](Self::has_alpha), docs/improvements.md U-16).
    pub fn is_identity(&self) -> Result<bool> {
        if self.has_offsets() || self.has_alpha()? || !self.is_diagonal() {
            return Ok(false);
        }

        // Now check the diagonal elements.

        let max_diff = 1e-6;

        let a = self.get_array();
        let m = a.get_values();
        let dim = a.get_length() as usize;

        for i in 0..dim {
            for j in 0..dim {
                if i == j && !equal_with_abs_error(m[i * dim + j], 1.0, max_diff) {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }

    /// Whether the op mixes channels: whether the matrix isn't diagonal.
    ///
    /// Port of `MatrixOpData::hasChannelCrosstalk` (MatrixOpData.h:198 @ v2.5.2).
    pub fn has_channel_crosstalk(&self) -> bool {
        !self.is_diagonal()
    }

    /// Whether the values off the diagonal are exactly 0.
    ///
    /// Port of `MatrixOpData::isDiagonal` (MatrixOpData.cpp:566-585 @ v2.5.2).
    pub fn is_diagonal(&self) -> bool {
        let a = self.get_array();
        let m = a.get_values();
        let max = a.get_num_values();
        let dim = a.get_length();

        for idx in 0..max {
            // Not on the diagonal
            if idx % dim.wrapping_add(1) != 0 && m[idx as usize] != 0.0 {
                // Strict comparison intended
                return false;
            }
        }

        true
    }

    /// Whether an offset isn't 0.
    ///
    /// Port of `MatrixOpData::hasOffsets` (MatrixOpData.h:208 @ v2.5.2).
    pub fn has_offsets(&self) -> bool {
        self.offsets.is_not_null()
    }

    /// The error of a query that reads a 4x4 matrix's positions on a 3x3 one, which `validate`
    /// hasn't made 4x4 yet (docs/improvements.md, U-16).
    pub(crate) fn require_4x4(&self) -> Result<()> {
        if self.array.get_length() != 4 {
            return Err(Exception::new(
                "Matrix: a 3x3 matrix has to be validated before this query: upstream reads \
                 past its 9 values.",
            ));
        }
        Ok(())
    }

    /// Whether alpha isn't passed through: the last row or column isn't the identity's (within
    /// 1e-6 on the diagonal), or alpha has an offset.
    ///
    /// Upstream reads the 4x4 positions of the values, so a 3x3 matrix that `validate` hasn't
    /// made 4x4 yet (from a CLF or CTF file) has it read past its 9 values; the port returns an
    /// error instead (docs/improvements.md, U-16).
    ///
    /// Port of `MatrixOpData::hasAlpha` (MatrixOpData.cpp:587-614 @ v2.5.2).
    pub fn has_alpha(&self) -> Result<bool> {
        self.require_4x4()?;
        let a = self.get_array();
        let m = a.get_values();

        // Now check the diagonal elements.

        let max_diff = 1e-6;

        // Last column.
        Ok((m[3] != 0.0) || // Strict comparison intended
        (m[7] != 0.0) ||
        (m[11] != 0.0) ||

        // Diagonal.
        !equal_with_abs_error(m[15], 1.0, max_diff) ||

        // Bottom row.
        (m[12] != 0.0) || // Strict comparison intended
        (m[13] != 0.0) ||
        (m[14] != 0.0) ||

        // Alpha offset
        (self.offsets[3] != 0.0))
    }

    /// The op doing this one then `b`, both forward: the matrix `b * self` and the offsets
    /// `b * self.offsets + b.offsets`, cleaned up ([`clean_up`](Self::clean_up)), with this one's
    /// metadata combined with `b`'s, this one's file input bit depth and `b`'s output one.
    ///
    /// Port of `MatrixOpData::compose` (MatrixOpData.cpp:669-735 @ v2.5.2).
    pub fn compose(&self, b: &MatrixOpData) -> Result<MatrixOpData> {
        // Ensure that both matrices will have the right dimension (ie. 4x4).
        if self.array.get_length() != 4 || b.array.get_length() != 4 {
            // Note: By design, only 4x4 matrices are instantiated. The CLF 3x3 (and 3x4)
            // matrices are automatically converted to 4x4 matrices, and a Matrix Transform
            // only expects 4x4 matrices.
            return Err(Exception::new("MatrixOpData: array content issue."));
        }
        if self.get_direction() == TransformDirection::Inverse
            || b.get_direction() == TransformDirection::Inverse
        {
            return Err(Exception::new("Op::finalize has to be called."));
        }

        // TODO: May want to revisit how the metadata is set.
        let mut new_desc = self.get_format_metadata().clone();
        new_desc.combine(b.get_format_metadata())?;

        let mut out = MatrixOpData::new();

        out.set_file_input_bit_depth(self.get_file_input_bit_depth());
        out.set_file_output_bit_depth(b.get_file_output_bit_depth());

        *out.get_format_metadata_mut() = new_desc;

        // By definition, A.compose(B) implies that op A precedes op B in the opList. The LUT
        // format coefficients follow matrix math: vec2 = A x vec1 where A is 3x3 and vec is
        // 3x1. So the composite operation in matrix form is vec2 = B x A x vec1. Hence we
        // compute B x A rather than A x B.

        let out_array = b.array.inner(&self.array);

        out.get_array_mut().array = out_array.array;

        // Compute matrix B times offsets from A.

        let mut offs = b.array.inner_offsets(self.get_offsets());

        let dim = b.array.get_length() as usize;

        // Determine overall scaling of the offsets prior to any catastrophic cancellation that
        // may occur during the add.
        let mut max_val = 0.;
        for i in 0..dim {
            let val = offs[i].abs();
            max_val = if max_val > val { max_val } else { val };
            let val = b.get_offsets()[i].abs();
            max_val = if max_val > val { max_val } else { val };
        }

        // Add offsets from B. Both wheels compute `B[i] + offs[i]`, B's offset first: the
        // Windows wheel at 0x1802b3710 (`movsd xmm0, [B's offset]`, then `addsd xmm0, [offs]`),
        // the Linux wheel at 0x4e8f78 (`movupd xmm1, [B + 0xd8]`, then `addpd xmm1, [offs]`).
        for i in 0..dim {
            offs[i] = sse_add(b.get_offsets()[i], offs[i]);
        }

        out.set_offsets(offs);

        // To enable use of strict float comparisons above, we adjust the result so that values
        // very near integers become exactly integers.
        out.clean_up(max_val)?;

        Ok(out)
    }

    /// Makes the matrix values and the offsets that are within `1e-7` times the matrix's (at
    /// least `1e-4`) or `offset_scale` (at least `1e-4`) of an integer that integer.
    ///
    /// Port of `MatrixOpData::cleanUp` (MatrixOpData.cpp:737-792 @ v2.5.2).
    pub fn clean_up(&mut self, offset_scale: f64) -> Result<()> {
        let dim = self.array.get_length() as usize;

        // Estimate the magnitude of the matrix.
        let mut max_val = 0.;
        for i in 0..dim {
            for j in 0..dim {
                let val = self.array[i * dim + j].abs();
                max_val = if max_val > val { max_val } else { val };
            }
        }

        // Determine an absolute tolerance.
        // TODO: For double matrices a smaller tolerance could be used. However we have
        // matrices that may have been quantized to less than double precision either from
        // being written to files or via the factories that take float args. In any case, the
        // tolerance is small enough to pick up anything that would be significant in the
        // context of color management.
        let scale = if max_val > 1e-4 { max_val } else { 1e-4 };
        let abs_tol = scale * 1e-7;

        // Replace values that are close to integers by exact values.
        for i in 0..dim {
            for j in 0..dim {
                let val = self.array[i * dim + j];
                let round_val = val.round();
                let diff = (val - round_val).abs();
                if diff < abs_tol {
                    self.set_array_value(i * dim + j, round_val);
                }
            }
        }

        // Do likewise for the offsets.
        let scale2 = if offset_scale > 1e-4 {
            offset_scale
        } else {
            1e-4
        };
        let abs_tol2 = scale2 * 1e-7;

        for i in 0..dim {
            let val = self.get_offsets()[i];
            let round_val = val.round();
            let diff = (val - round_val).abs();
            if diff < abs_tol2 {
                self.set_offset_value(i as c_ulong, round_val)?;
            }
        }
        Ok(())
    }

    /// Whether `other` has the same direction, offsets (bit for bit) and matrix. The metadata
    /// and the file bit depths are ignored. The `OpData` base's type comparison is
    /// [`crate::op_data::OpData::equals`]'s.
    ///
    /// Port of `MatrixOpData::equals` (MatrixOpData.cpp:794-803 @ v2.5.2).
    pub fn equals(&self, other: &MatrixOpData) -> bool {
        self.direction == other.direction
            && self.offsets == other.offsets
            && self.array.equals(&other.array)
    }

    /// Port of `MatrixOpData::getDirection` (MatrixOpData.h:219 @ v2.5.2).
    pub fn get_direction(&self) -> TransformDirection {
        self.direction
    }

    /// Port of `MatrixOpData::setDirection` (MatrixOpData.cpp:805-808 @ v2.5.2).
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.direction = dir;
    }

    /// The forward op that does what this one does: a copy of a forward one, and the inverse
    /// matrix and offsets of an inverse one ("Singular Matrix can't be inverted." when it has
    /// none), with the file bit depths swapped, and the same metadata.
    ///
    /// Port of `MatrixOpData::getAsForward` (MatrixOpData.cpp:810-844 @ v2.5.2).
    pub fn get_as_forward(&self) -> Result<MatrixOpData> {
        if self.get_direction() == TransformDirection::Forward {
            return Ok(self.clone());
        }
        // Get the inverse matrix.
        let inv_matrix_array = self.array.inverse()?;
        // MatrixArray::inverse() will throw for singular matrices.
        // TODO: Perhaps calculate pseudo-inverse rather than throw.

        // Calculate the inverse offset.
        let offsets = self.get_offsets();
        let mut inv_offsets = Offsets::default();
        if offsets.is_not_null() {
            inv_offsets = inv_matrix_array.inner_offsets(offsets);
            // `invOffsets.scale(-1.)`. The Windows wheel multiplies by -1 (0x1802b3b5f, `mulpd
            // xmm6, {-1.0, -1.0}`), which keeps a NaN's sign. GCC inlines it and folds `x * -1.0`
            // into a negation (the Linux wheel at 0x4e8303, `xorpd` with -0.0), which flips the
            // sign of a NaN too. Both give the same bits for every other value.
            if cfg!(target_os = "windows") {
                inv_offsets.scale(-1.);
            } else {
                for value in inv_offsets.get_values_mut() {
                    *value = -*value;
                }
            }
        }

        let mut inv_op = MatrixOpData::new();
        inv_op.file_in_bit_depth = self.file_out_bit_depth;
        inv_op.file_out_bit_depth = self.file_in_bit_depth;

        let values: &[f64; 16] = inv_matrix_array.get_values()[..16]
            .try_into()
            .expect("an inverse is 4x4");
        inv_op.set_rgba(values);
        inv_op.set_offsets(inv_offsets);
        *inv_op.get_format_metadata_mut() = self.get_format_metadata().clone();

        // No need to call validate(), the invOp will have proper dimension, bit-depths, matrix
        // and offsets values.

        // Note that the metadata may become stale at this point but trying to update it is
        // challenging.
        Ok(inv_op)
    }

    /// Port of `MatrixOpData::getFileInputBitDepth` (MatrixOpData.h:224 @ v2.5.2).
    pub fn get_file_input_bit_depth(&self) -> BitDepth {
        self.file_in_bit_depth
    }

    /// Port of `MatrixOpData::setFileInputBitDepth` (MatrixOpData.h:225 @ v2.5.2).
    pub fn set_file_input_bit_depth(&mut self, in_bit_depth: BitDepth) {
        self.file_in_bit_depth = in_bit_depth;
    }

    /// Port of `MatrixOpData::getFileOutputBitDepth` (MatrixOpData.h:227 @ v2.5.2).
    pub fn get_file_output_bit_depth(&self) -> BitDepth {
        self.file_out_bit_depth
    }

    /// Port of `MatrixOpData::setFileOutputBitDepth` (MatrixOpData.h:228 @ v2.5.2).
    pub fn set_file_output_bit_depth(&mut self, out_bit_depth: BitDepth) {
        self.file_out_bit_depth = out_bit_depth;
    }

    /// Scales the matrix by `in_scale * out_scale` and the offsets by `out_scale`: the matrix
    /// of a file with other bit depths.
    ///
    /// Port of `MatrixOpData::scale` (MatrixOpData.cpp:871-877 @ v2.5.2).
    pub fn scale(&mut self, in_scale: f64, out_scale: f64) {
        let combined_scale = sse_mul(in_scale, out_scale);
        self.get_array_mut().scale(combined_scale);

        self.offsets.scale(out_scale);
    }

    /// The data's cache ID: its id and a space, if it has an id, then its direction, a space,
    /// and the hash of the hashes of the 16 matrix values and the 4 offsets.
    ///
    /// Upstream hashes 16 values, so it reads past the 9 values of a 3x3 matrix that `validate`
    /// hasn't made 4x4 yet; the port returns an error instead (docs/improvements.md, U-16).
    ///
    /// Port of `MatrixOpData::getCacheID` (MatrixOpData.cpp:846-869 @ v2.5.2).
    pub fn get_cache_id(&self) -> Result<Vec<u8>> {
        self.require_4x4()?;
        let mut cache_id_stream = Vec::new();
        if !self.get_id().is_empty() {
            cache_id_stream.extend_from_slice(self.get_id());
            cache_id_stream.push(b' ');
        }

        cache_id_stream.extend_from_slice(transform_direction_to_string(self.direction).as_bytes());
        cache_id_stream.push(b' ');

        let bytes =
            |values: &[f64]| -> Vec<u8> { values.iter().flat_map(|v| v.to_ne_bytes()).collect() };
        let mut hash = String::new();
        hash += &cache_id_hash(&bytes(&self.get_array().get_values()[..16]));
        hash += &cache_id_hash(&bytes(self.get_offsets().get_values()));

        cache_id_stream.extend_from_slice(cache_id_hash(hash.as_bytes()).as_bytes());

        Ok(cache_id_stream)
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

    /// Port of `OpData::setID` (src/OpenColorIO/Op.cpp:86-89 @ v2.5.2).
    pub fn set_id(&mut self, id: &[u8]) {
        self.metadata.set_id(Some(id));
    }

    /// Port of `OpData::getName` (src/OpenColorIO/Op.cpp:91-94 @ v2.5.2).
    pub fn get_name(&self) -> &[u8] {
        self.metadata
            .get_attribute_value_string(Some(METADATA_NAME))
    }

    /// Port of `OpData::setName` (src/OpenColorIO/Op.cpp:96-99 @ v2.5.2).
    pub fn set_name(&mut self, name: &[u8]) {
        self.metadata.set_name(Some(name));
    }
}

impl PartialEq for MatrixOpData {
    /// Port of `operator==(const MatrixOpData &, const MatrixOpData &)` (MatrixOpData.cpp:
    /// 879-882 @ v2.5.2).
    fn eq(&self, other: &MatrixOpData) -> bool {
        self.equals(other)
    }
}

#[cfg(test)]
#[path = "matrix_op_data_tests.rs"]
mod tests;
