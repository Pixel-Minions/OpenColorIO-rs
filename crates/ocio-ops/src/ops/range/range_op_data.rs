// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Range op's data: an affine map from `[minIn, maxIn]` to `[minOut, maxOut]` that clamps
//! to the output bounds. A port of `src/OpenColorIO/ops/range/RangeOpData.h` and
//! `RangeOpData.cpp` @ v2.5.2.
//!
//! Upstream's comment (RangeOpData.h:24-45): a bound may be missing, which means no clamping at
//! that end, and is then NaN ([`RangeOpData::empty_value`]). If minIn is set then minOut must
//! also be set, and the same for the maximums. With both bounds set, they define the scale and
//! offset; with one bound only, the scale is 1 and the offset 0.
//!
//! Upstream's `validate` is `const` and fills the `mutable` scale and offset. The port keeps
//! them in atomics, so that [`RangeOpData::validate`] takes `&self`, as the ops that share the
//! data call it.
//!
//! Not ported here: the constructor from a CTF `IndexMapping` (RangeOpData.cpp:67-103), which
//! comes with the CTF reader in `ocio-formats`.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::bit_depth_utils::get_bit_depth_max_value;
use crate::cfmt::{Crt, OStringStream};
use crate::exception::{Exception, Result};
use crate::format_metadata::{FormatMetadataImpl, METADATA_ID, METADATA_NAME};
use crate::math_utils::is_nan;
use crate::op_data::OpDataType;
use crate::open_color_types::{BitDepth, TransformDirection, transform_direction_to_string};
use crate::ops::matrix::MatrixOpData;

/// `DefaultValues::FLOAT_DECIMALS` (RangeOpData.cpp:17-20 @ v2.5.2): the cache ID's precision.
const FLOAT_DECIMALS: i64 = 7;

/// A `mutable double`: a value that `const` methods change.
#[derive(Debug, Default)]
struct MutableF64(AtomicU64);

impl MutableF64 {
    fn new(value: f64) -> Self {
        MutableF64(AtomicU64::new(value.to_bits()))
    }

    fn get(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Relaxed))
    }

    fn set(&self, value: f64) {
        self.0.store(value.to_bits(), Ordering::Relaxed);
    }
}

impl Clone for MutableF64 {
    fn clone(&self) -> Self {
        MutableF64::new(self.get())
    }
}

/// `IsNan((float)value)`: whether a bound is missing. A `double` too large for a `float` is
/// an infinity there, so it is set.
fn is_empty(value: f64) -> bool {
    is_nan(value as f32)
}

/// The Range op's data.
///
/// Port of `RangeOpData` (src/OpenColorIO/ops/range/RangeOpData.h:47-180 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct RangeOpData {
    /// The `OpData` base's `m_metadata`.
    metadata: FormatMetadataImpl,
    /// `m_minInValue`: the lower bound of the domain.
    min_in_value: f64,
    /// `m_maxInValue`: the upper bound of the domain.
    max_in_value: f64,
    /// `m_minOutValue`: the lower bound of the range.
    min_out_value: f64,
    /// `m_maxOutValue`: the upper bound of the range.
    max_out_value: f64,
    /// `mutable double m_scale`: computed from the bounds.
    scale: MutableF64,
    /// `mutable double m_offset`: computed from the bounds.
    offset: MutableF64,
    /// `m_fileInBitDepth`: the input bit depth for file I/O.
    file_in_bit_depth: BitDepth,
    /// `m_fileOutBitDepth`: the output bit depth for file I/O.
    file_out_bit_depth: BitDepth,
    /// `m_direction`.
    direction: TransformDirection,
}

impl Default for RangeOpData {
    fn default() -> Self {
        Self::new()
    }
}

impl RangeOpData {
    /// Every bound empty, scale 1 and offset 0, forward. Not valid as it is.
    ///
    /// Port of `RangeOpData::RangeOpData()` (RangeOpData.cpp:23-33 @ v2.5.2).
    pub fn new() -> Self {
        RangeOpData {
            metadata: FormatMetadataImpl::default(),
            min_in_value: Self::empty_value(),
            max_in_value: Self::empty_value(),
            min_out_value: Self::empty_value(),
            max_out_value: Self::empty_value(),
            scale: MutableF64::new(1.),
            offset: MutableF64::new(0.),
            file_in_bit_depth: BitDepth::Unknown,
            file_out_bit_depth: BitDepth::Unknown,
            direction: TransformDirection::Forward,
        }
    }

    /// The range with these bounds, forward, validated.
    ///
    /// Port of `RangeOpData::RangeOpData(double, double, double, double)`
    /// (RangeOpData.cpp:35-48 @ v2.5.2).
    pub fn with_values(
        min_in_value: f64,
        max_in_value: f64,
        min_out_value: f64,
        max_out_value: f64,
    ) -> Result<Self> {
        let range = RangeOpData {
            min_in_value,
            max_in_value,
            min_out_value,
            max_out_value,
            scale: MutableF64::new(0.),
            offset: MutableF64::new(0.),
            ..RangeOpData::new()
        };
        range.validate()?;
        Ok(range)
    }

    /// The range with these bounds, in the direction `dir`, validated.
    ///
    /// Port of `RangeOpData::RangeOpData(double, double, double, double, TransformDirection)`
    /// (RangeOpData.cpp:50-65 @ v2.5.2).
    pub fn with_direction(
        min_in_value: f64,
        max_in_value: f64,
        min_out_value: f64,
        max_out_value: f64,
        dir: TransformDirection,
    ) -> Result<Self> {
        let mut range = RangeOpData {
            min_in_value,
            max_in_value,
            min_out_value,
            max_out_value,
            scale: MutableF64::new(0.),
            offset: MutableF64::new(0.),
            ..RangeOpData::new()
        };
        range.set_direction(dir);
        range.validate()?;
        Ok(range)
    }

    /// The value of an empty bound: a quiet NaN.
    ///
    /// Port of `RangeOpData::EmptyValue` (RangeOpData.cpp:178-185 @ v2.5.2).
    pub fn empty_value() -> f64 {
        f64::NAN
    }

    /// Port of `RangeOpData::getMinInValue` (RangeOpData.h:83 @ v2.5.2).
    pub fn get_min_in_value(&self) -> f64 {
        self.min_in_value
    }

    /// Port of `RangeOpData::hasMinInValue` (RangeOpData.cpp:119-122 @ v2.5.2).
    pub fn has_min_in_value(&self) -> bool {
        !is_empty(self.min_in_value)
    }

    /// Port of `RangeOpData::unsetMinInValue` (RangeOpData.cpp:124-127 @ v2.5.2).
    pub fn unset_min_in_value(&mut self) {
        self.min_in_value = Self::empty_value();
    }

    /// Port of `RangeOpData::setMinInValue` (RangeOpData.cpp:114-117 @ v2.5.2). As upstream's
    /// setters, it doesn't validate.
    pub fn set_min_in_value(&mut self, value: f64) {
        self.min_in_value = value;
    }

    /// Port of `RangeOpData::getMaxInValue` (RangeOpData.h:89 @ v2.5.2).
    pub fn get_max_in_value(&self) -> f64 {
        self.max_in_value
    }

    /// Port of `RangeOpData::hasMaxInValue` (RangeOpData.cpp:135-138 @ v2.5.2).
    pub fn has_max_in_value(&self) -> bool {
        !is_empty(self.max_in_value)
    }

    /// Port of `RangeOpData::unsetMaxInValue` (RangeOpData.cpp:140-143 @ v2.5.2).
    pub fn unset_max_in_value(&mut self) {
        self.max_in_value = Self::empty_value();
    }

    /// Port of `RangeOpData::setMaxInValue` (RangeOpData.cpp:130-133 @ v2.5.2).
    pub fn set_max_in_value(&mut self, value: f64) {
        self.max_in_value = value;
    }

    /// Port of `RangeOpData::getMinOutValue` (RangeOpData.h:95 @ v2.5.2).
    pub fn get_min_out_value(&self) -> f64 {
        self.min_out_value
    }

    /// Port of `RangeOpData::hasMinOutValue` (RangeOpData.cpp:151-154 @ v2.5.2).
    pub fn has_min_out_value(&self) -> bool {
        !is_empty(self.min_out_value)
    }

    /// Port of `RangeOpData::unsetMinOutValue` (RangeOpData.cpp:156-159 @ v2.5.2).
    pub fn unset_min_out_value(&mut self) {
        self.min_out_value = Self::empty_value();
    }

    /// Port of `RangeOpData::setMinOutValue` (RangeOpData.cpp:146-149 @ v2.5.2).
    pub fn set_min_out_value(&mut self, value: f64) {
        self.min_out_value = value;
    }

    /// Port of `RangeOpData::getMaxOutValue` (RangeOpData.h:101 @ v2.5.2).
    pub fn get_max_out_value(&self) -> f64 {
        self.max_out_value
    }

    /// Port of `RangeOpData::hasMaxOutValue` (RangeOpData.cpp:167-170 @ v2.5.2).
    pub fn has_max_out_value(&self) -> bool {
        !is_empty(self.max_out_value)
    }

    /// Port of `RangeOpData::unsetMaxOutValue` (RangeOpData.cpp:172-175 @ v2.5.2).
    pub fn unset_max_out_value(&mut self) {
        self.max_out_value = Self::empty_value();
    }

    /// Port of `RangeOpData::setMaxOutValue` (RangeOpData.cpp:162-165 @ v2.5.2).
    pub fn set_max_out_value(&mut self, value: f64) {
        self.max_out_value = value;
    }

    /// The scale of `out = in * scale + offset`, as the last [`validate`](Self::validate)
    /// computed it.
    ///
    /// Port of `RangeOpData::getScale` (RangeOpData.h:107 @ v2.5.2).
    pub fn get_scale(&self) -> f64 {
        self.scale.get()
    }

    /// The offset of `out = in * scale + offset`, as the last [`validate`](Self::validate)
    /// computed it.
    ///
    /// Port of `RangeOpData::getOffset` (RangeOpData.h:110 @ v2.5.2).
    pub fn get_offset(&self) -> f64 {
        self.offset.get()
    }

    /// Checks the bounds, then computes the scale and the offset.
    ///
    /// Port of `RangeOpData::validate` (RangeOpData.cpp:187-262 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        // NB: Need to allow vals to exceed normal integer range
        // to allow lossless setting of bit-depth from float-->int-->float.

        // If in_min or out_min is not empty, so must the other half be.
        if is_empty(self.min_in_value) {
            if !is_empty(self.min_out_value) {
                return Err(Exception::new(
                    "In and out minimum limits must be both set or both missing in Range.",
                ));
            }
        } else if is_empty(self.min_out_value) {
            return Err(Exception::new(
                "In and out minimum limits must be both set or both missing in Range.",
            ));
        }

        if is_empty(self.max_in_value) {
            if !is_empty(self.max_out_value) {
                return Err(Exception::new(
                    "In and out maximum limits must be both set or both missing in Range.",
                ));
            }
            if is_empty(self.min_in_value) {
                return Err(Exception::new(
                    "At least minimum or maximum limits must be set in Range.",
                ));
            }
        } else if is_empty(self.max_out_value) {
            return Err(Exception::new(
                "In and out maximum limits must be both set or both missing in Range.",
            ));
        }

        // Currently not allowing polarity inversion so enforce max > min.
        if !is_empty(self.min_in_value) && !is_empty(self.max_in_value) {
            if self.min_in_value > self.max_in_value {
                return Err(Exception::new(
                    "Range maximum input value is less than minimum input value",
                ));
            }
            if self.min_out_value > self.max_out_value {
                return Err(Exception::new(
                    "Range maximum output value is less than minimum output value",
                ));
            }
        }

        // A one-sided clamp must have matching in & out values.

        if is_empty(self.max_in_value)
            && !is_empty(self.min_in_value)
            && Self::floats_differ(self.min_out_value, self.min_in_value)
        {
            return Err(Exception::new(
                "In and out minimum limits must be equal if maximum values are missing in Range.",
            ));
        }

        if is_empty(self.min_in_value)
            && !is_empty(self.max_in_value)
            && Self::floats_differ(self.max_out_value, self.max_in_value)
        {
            return Err(Exception::new(
                "In and out maximum limits must be equal if minimum values are missing in Range.",
            ));
        }

        // Complete the initialization of the object.
        self.fill_scale_offset() // This also validates that maxIn - minIn != 0.
    }

    /// Port of `RangeOpData::getCacheID` (RangeOpData.cpp:577-598 @ v2.5.2): the ID, the
    /// direction, and the bounds with 7 significant digits, empty ones as NaN.
    pub fn get_cache_id(&self) -> Vec<u8> {
        let mut cache_id = Vec::new();
        if !self.get_id().is_empty() {
            cache_id.extend_from_slice(self.get_id());
            cache_id.push(b' ');
        }

        let mut os = OStringStream::new(Crt::NATIVE);
        os.put_str(transform_direction_to_string(self.direction));
        os.put_str(" ");

        os.precision = FLOAT_DECIMALS;

        os.put_str("[");
        os.put_f64(self.min_in_value);
        os.put_str(", ");
        os.put_f64(self.max_in_value);
        os.put_str(", ");
        os.put_f64(self.min_out_value);
        os.put_str(", ");
        os.put_f64(self.max_out_value);
        os.put_str("]");

        cache_id.extend_from_slice(os.str());
        cache_id
    }

    /// Port of `RangeOpData::getType` (RangeOpData.h:117 @ v2.5.2).
    pub fn get_type(&self) -> OpDataType {
        OpDataType::Range
    }

    /// A Range op always clamps (the noClamp style is converted to a Matrix).
    ///
    /// Port of `RangeOpData::isNoOp` (RangeOpData.cpp:264-268 @ v2.5.2).
    pub fn is_no_op(&self) -> bool {
        false
    }

    /// An identity Range does not modify pixel values between [0,1] but may clamp values
    /// outside that domain.
    ///
    /// Port of `RangeOpData::isIdentity` (RangeOpData.cpp:270-291 @ v2.5.2).
    pub fn is_identity(&self) -> bool {
        // No scale or offset allowed.
        if self.scales() {
            return false;
        }

        if !self.min_is_empty() && self.min_in_value > 0.0 {
            return false;
        }

        if !self.max_is_empty() && self.max_in_value < 1.0 {
            return false;
        }

        true
    }

    /// Whether the op limits the incoming pixels at least as much as a 1D or 3D LUT would:
    /// the clamps are at least as narrow as [0, 1].
    ///
    /// Port of `RangeOpData::clampsToLutDomain` (RangeOpData.cpp:293-306 @ v2.5.2).
    pub fn clamps_to_lut_domain(&self) -> bool {
        if self.min_is_empty() || self.min_in_value < 0.0 {
            return false;
        }

        if self.max_is_empty() || self.max_in_value > 1.0 {
            return false;
        }

        true
    }

    /// Whether the op only clamps values below 0.
    ///
    /// Port of `RangeOpData::isClampNegs` (RangeOpData.cpp:308-311 @ v2.5.2).
    pub fn is_clamp_negs(&self) -> bool {
        self.max_is_empty() && !self.min_is_empty() && self.min_in_value == 0.0
    }

    /// Port of `RangeOpData::hasChannelCrosstalk` (RangeOpData.h:131 @ v2.5.2).
    pub fn has_channel_crosstalk(&self) -> bool {
        false
    }

    /// Whether the doubles differ: a hybrid absolute/relative comparison, with tolerances
    /// chosen for the Range op's use cases.
    ///
    /// Port of `RangeOpData::FloatsDiffer` (RangeOpData.cpp:313-330 @ v2.5.2).
    fn floats_differ(x1: f64, x2: f64) -> bool {
        if x1.abs() < 1e-3 {
            (x1 - x2).abs() > 1e-6 // absolute error near zero
        } else {
            (1.0 - (x2 / x1)).abs() > 1e-6 // relative error otherwise
        }
    }

    /// Whether the scale and offset are not the identity.
    ///
    /// Port of `RangeOpData::scales` (RangeOpData.cpp:332-350 @ v2.5.2).
    pub fn scales(&self) -> bool {
        // Check if offset is non-zero or scale is not unity.

        // Offset is likely to be zero, so cannot do a relative comparison.
        if self.offset.get().abs() > 1e-6 {
            return true;
        }

        // Scale may vary from very small to vary large, however it's also allowed to be 0, so
        // neither relative or absolute comparison is appropriate for all cases.
        if Self::floats_differ(self.scale.get(), 1.0) {
            return true;
        }

        false
    }

    /// The range that applies this one then `r`: a constant range where this one's output
    /// falls outside `r`'s input, otherwise the bounds mapped through each other.
    ///
    /// Port of `RangeOpData::compose` (RangeOpData.cpp:352-431 @ v2.5.2).
    pub fn compose(&self, r: &RangeOpData) -> Result<RangeOpData> {
        let mut min_in_new = self.min_in_value;
        let mut max_in_new = self.max_in_value;
        let mut min_out_new = r.min_out_value;
        let mut max_out_new = r.max_out_value;
        if !self.min_is_empty() {
            if !r.max_is_empty() && self.min_out_value >= r.max_in_value {
                min_out_new = r.max_out_value;
                max_out_new = r.max_out_value;
                // Range outputting a constant value.
                return RangeOpData::with_values(
                    self.min_in_value,
                    self.max_in_value,
                    min_out_new,
                    max_out_new,
                );
            } else if !r.min_is_empty() {
                if self.min_out_value >= r.min_in_value {
                    // Transform m_minOutValue with r.
                    min_out_new = self.min_out_value * r.scale.get() + r.offset.get();
                } else {
                    // Transform r->m_minInValue with inverse of this.
                    min_in_new = (r.min_in_value - self.offset.get()) / self.scale.get();
                }
            } else {
                min_out_new = self.min_out_value;
            }
        } else if !r.min_is_empty() {
            // minIsEmpty() is true.
            min_in_new = r.min_in_value;
        }

        if !self.max_is_empty() {
            if !r.min_is_empty() && self.max_out_value <= r.min_in_value {
                min_out_new = r.min_out_value;
                max_out_new = r.min_out_value;
                // Range outputting a constant value.
                return RangeOpData::with_values(
                    self.min_in_value,
                    self.max_in_value,
                    min_out_new,
                    max_out_new,
                );
            } else if !r.max_is_empty() {
                if self.max_out_value <= r.max_in_value {
                    // Transform m_maxOutValue with r.
                    max_out_new = self.max_out_value * r.scale.get() + r.offset.get();
                } else {
                    // Transform r->m_maxOutValue with inverse of this.
                    max_in_new = (r.max_in_value - self.offset.get()) / self.scale.get();
                }
            } else {
                max_out_new = self.max_out_value;
            }
        } else if !r.max_is_empty() {
            // maxIsEmpty() is true.
            max_in_new = r.max_in_value;
        }

        RangeOpData::with_values(min_in_new, max_in_new, min_out_new, max_out_new)
    }

    /// Whether minIn (and so minOut) doesn't request clipping.
    ///
    /// Port of `RangeOpData::minIsEmpty` (RangeOpData.cpp:433-437 @ v2.5.2).
    pub fn min_is_empty(&self) -> bool {
        // NB: Validation ensures out is not empty if in is not.
        is_empty(self.min_in_value)
    }

    /// Whether maxIn (and so maxOut) doesn't request clipping.
    ///
    /// Port of `RangeOpData::maxIsEmpty` (RangeOpData.cpp:439-443 @ v2.5.2).
    pub fn max_is_empty(&self) -> bool {
        // NB: Validation ensures out is not empty if in is not.
        is_empty(self.max_in_value)
    }

    /// Converts `out = (in - minIn) * scale + minOut` to `out = in * scale + offset`; "Range
    /// maxInValue is too close to minInValue" when the input bounds are within 1e-6, after
    /// setting the scale to 1.
    ///
    /// Port of `RangeOpData::fillScaleOffset` (RangeOpData.cpp:445-478 @ v2.5.2).
    fn fill_scale_offset(&self) -> Result<()> {
        // The case where one bound clamps and the other is empty was potentially ambiguous in
        // versions 1 and 2 of CLF but v3 requires that the in & out bounds must match in this
        // case. Hence offset must be zero and scale must be 1.
        self.scale.set(1.0);

        if self.min_is_empty() {
            self.offset.set(0.); // Bottom unlimited but top clamps
        } else if self.max_is_empty() {
            // Top unlimited but bottom clamps
            self.offset.set(0.);
        } else {
            // Both ends clamp
            let denom = self.max_in_value - self.min_in_value;
            if denom.abs() < 1e-6 {
                return Err(Exception::new(
                    "Range maxInValue is too close to minInValue",
                ));
            }
            // NB: Allowing out min == max as it could be useful to create a constant.
            let scale = (self.max_out_value - self.min_out_value) / denom;
            self.scale.set(scale);
            self.offset
                .set(self.min_out_value - scale * self.min_in_value);
        }
        Ok(())
    }

    /// A Matrix op's data that does what the range does, without clamping: the scale on the
    /// diagonal (alpha 1) and the offset on RGB. "Non-clamping Range min & max values have to
    /// be set." without both bounds.
    ///
    /// Port of `RangeOpData::convertToMatrix` (RangeOpData.cpp:480-513 @ v2.5.2).
    pub fn convert_to_matrix(&self) -> Result<MatrixOpData> {
        if self.min_is_empty() || self.max_is_empty() {
            return Err(Exception::new(
                "Non-clamping Range min & max values have to be set.",
            ));
        }
        let temp_fwd;
        let mut fwd_this = self;
        if self.get_direction() == TransformDirection::Inverse {
            temp_fwd = self.get_as_forward()?;
            fwd_this = &temp_fwd;
        }
        // Create an identity matrix.
        let mut mtx = MatrixOpData::new();
        *mtx.get_format_metadata_mut() = fwd_this.get_format_metadata().clone();
        mtx.set_file_input_bit_depth(fwd_this.get_file_input_bit_depth());
        mtx.set_file_output_bit_depth(fwd_this.get_file_output_bit_depth());

        let scale = fwd_this.get_scale();
        mtx.set_array_value(0, scale);
        mtx.set_array_value(5, scale);
        mtx.set_array_value(10, scale);

        let offset = fwd_this.get_offset();
        mtx.set_offset_value(0, offset)?;
        mtx.set_offset_value(1, offset)?;
        mtx.set_offset_value(2, offset)?;
        mtx.set_offset_value(3, 0.)?;

        mtx.validate()?;

        Ok(mtx)
    }

    /// Whether `other` has the same direction, the same empty bounds and bounds that
    /// [`floats_differ`](Self::floats_differ) finds equal. The metadata and the file bit
    /// depths are ignored. The `OpData` base's type comparison is
    /// [`crate::op_data::OpData::equals`]'s.
    ///
    /// Port of `RangeOpData::equals` (RangeOpData.cpp:515-548 @ v2.5.2).
    pub fn equals(&self, rop: &RangeOpData) -> bool {
        // NB: FormatMetadata and fileIn/OutDepths are ignored.
        // `OpData::equals`: the same object, or the same type.
        if std::ptr::eq(self, rop) {
            return true;
        }

        if self.direction != rop.direction {
            return false;
        }

        if (self.min_is_empty() != rop.min_is_empty())
            || (self.max_is_empty() != rop.max_is_empty())
        {
            return false;
        }

        if !self.min_is_empty()
            && !rop.min_is_empty()
            && (Self::floats_differ(self.min_in_value, rop.min_in_value)
                || Self::floats_differ(self.min_out_value, rop.min_out_value))
        {
            return false;
        }

        if !self.max_is_empty()
            && !rop.max_is_empty()
            && (Self::floats_differ(self.max_in_value, rop.max_in_value)
                || Self::floats_differ(self.max_out_value, rop.max_out_value))
        {
            return false;
        }

        true
    }

    /// Port of `RangeOpData::getDirection` (RangeOpData.h:140 @ v2.5.2).
    pub fn get_direction(&self) -> TransformDirection {
        self.direction
    }

    /// Port of `RangeOpData::setDirection` (RangeOpData.cpp:550-553 @ v2.5.2).
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.direction = dir;
    }

    /// The forward range that does what this one does: a copy of a forward one; for an
    /// inverse one, the range with the input and output bounds and file bit depths swapped.
    ///
    /// Port of `RangeOpData::getAsForward` (RangeOpData.cpp:555-575 @ v2.5.2).
    pub fn get_as_forward(&self) -> Result<RangeOpData> {
        if self.direction == TransformDirection::Forward {
            return Ok(self.clone());
        }
        let mut inv_op = RangeOpData::with_values(
            self.get_min_out_value(),
            self.get_max_out_value(),
            self.get_min_in_value(),
            self.get_max_in_value(),
        )?;

        // Note any existing metadata may be stale at this point, but trying to update it is
        // challenging.
        *inv_op.get_format_metadata_mut() = self.get_format_metadata().clone();
        inv_op.file_in_bit_depth = self.file_out_bit_depth;
        inv_op.file_out_bit_depth = self.file_in_bit_depth;

        inv_op.validate()?;

        Ok(inv_op)
    }

    /// Port of `RangeOpData::getFileInputBitDepth` (RangeOpData.h:143 @ v2.5.2).
    pub fn get_file_input_bit_depth(&self) -> BitDepth {
        self.file_in_bit_depth
    }

    /// Port of `RangeOpData::setFileInputBitDepth` (RangeOpData.h:144 @ v2.5.2).
    pub fn set_file_input_bit_depth(&mut self, in_bit_depth: BitDepth) {
        self.file_in_bit_depth = in_bit_depth;
    }

    /// Port of `RangeOpData::getFileOutputBitDepth` (RangeOpData.h:146 @ v2.5.2).
    pub fn get_file_output_bit_depth(&self) -> BitDepth {
        self.file_out_bit_depth
    }

    /// Port of `RangeOpData::setFileOutputBitDepth` (RangeOpData.h:147 @ v2.5.2).
    pub fn set_file_output_bit_depth(&mut self, out_bit_depth: BitDepth) {
        self.file_out_bit_depth = out_bit_depth;
    }

    /// Scales the set bounds from the file bit depths to [0, 1]: the input bounds by
    /// `1 / max(fileIn)`, the output bounds by `1 / max(fileOut)`. A file bit depth that isn't
    /// supported raises, before any bound changes.
    ///
    /// Port of `RangeOpData::normalize` (RangeOpData.cpp:600-621 @ v2.5.2).
    pub fn normalize(&mut self) -> Result<()> {
        let in_scale = 1.0 / get_bit_depth_max_value(self.get_file_input_bit_depth())?;
        let out_scale = 1.0 / get_bit_depth_max_value(self.get_file_output_bit_depth())?;
        if !self.min_is_empty() {
            self.min_in_value *= in_scale;
        }
        if !self.max_is_empty() {
            self.max_in_value *= in_scale;
        }

        if !self.min_is_empty() {
            self.min_out_value *= out_scale;
        }
        if !self.max_is_empty() {
            self.max_out_value *= out_scale;
        }
        Ok(())
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
}

/// Port of `operator==(const RangeOpData &, const RangeOpData &)` (RangeOpData.cpp:623-626 @
/// v2.5.2).
impl PartialEq for RangeOpData {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}

#[cfg(test)]
#[path = "range_op_data_tests.rs"]
mod tests;
