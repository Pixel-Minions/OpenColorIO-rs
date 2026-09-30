// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The public enums the ops need, from `include/OpenColorIO/OpenColorTypes.h` @ v2.5.2, and
//! their helpers from `src/OpenColorIO/ParseUtils.cpp`.
//!
//! They live in `ocio-ops` because op data uses them; the public `ocio` crate re-exports them.
//! So far: `TransformDirection`, `NegativeStyle` and `DynamicPropertyType`.

/// Port of `TransformDirection` (include/OpenColorIO/OpenColorTypes.h:355-359 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TransformDirection {
    /// `TRANSFORM_DIR_FORWARD`.
    #[default]
    Forward = 0,
    /// `TRANSFORM_DIR_INVERSE`.
    Inverse,
}

/// The other direction.
///
/// Port of `GetInverseTransformDirection` (src/OpenColorIO/ParseUtils.cpp:164-169 @ v2.5.2).
pub fn get_inverse_transform_direction(dir: TransformDirection) -> TransformDirection {
    if dir == TransformDirection::Forward {
        return TransformDirection::Inverse;
    }
    // TRANSFORM_DIR_INVERSE
    TransformDirection::Forward
}

/// Forward when both directions agree, inverse otherwise.
///
/// Port of `CombineTransformDirections` (src/OpenColorIO/ParseUtils.cpp:153-162 @ v2.5.2).
pub fn combine_transform_directions(
    d1: TransformDirection,
    d2: TransformDirection,
) -> TransformDirection {
    if d1 == TransformDirection::Forward && d2 == TransformDirection::Forward {
        return TransformDirection::Forward;
    }

    if d1 == TransformDirection::Inverse && d2 == TransformDirection::Inverse {
        return TransformDirection::Forward;
    }

    TransformDirection::Inverse
}

/// How an exponent or curve handles negative values.
///
/// Port of `NegativeStyle` (include/OpenColorIO/OpenColorTypes.h:552-558 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NegativeStyle {
    /// `NEGATIVE_CLAMP`: clamp negative values.
    Clamp = 0,
    /// `NEGATIVE_MIRROR`: the positive curve is rotated 180 degrees around the origin.
    Mirror,
    /// `NEGATIVE_PASS_THRU`: negative values are passed through unchanged.
    PassThru,
    /// `NEGATIVE_LINEAR`: linearly extrapolate the curve for negative values.
    Linear,
}

/// What a dynamic property holds: a double for the first three, a grading value for the
/// others.
///
/// Port of `DynamicPropertyType` (include/OpenColorIO/OpenColorTypes.h:568-578 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DynamicPropertyType {
    /// `DYNAMIC_PROPERTY_EXPOSURE`: image exposure (a double).
    Exposure = 0,
    /// `DYNAMIC_PROPERTY_CONTRAST`: image contrast (a double).
    Contrast,
    /// `DYNAMIC_PROPERTY_GAMMA`: image gamma (a double).
    Gamma,
    /// `DYNAMIC_PROPERTY_GRADING_PRIMARY`: used by `GradingPrimaryTransform`.
    GradingPrimary,
    /// `DYNAMIC_PROPERTY_GRADING_RGBCURVE`: used by `GradingRGBCurveTransform`.
    GradingRgbCurve,
    /// `DYNAMIC_PROPERTY_GRADING_TONE`: used by `GradingToneTransform`.
    GradingTone,
    /// `DYNAMIC_PROPERTY_GRADING_HUECURVE`: used by `GradingHueCurveTransform`.
    GradingHueCurve,
}

#[cfg(test)]
mod tests {
    use super::*;
    use TransformDirection::{Forward, Inverse};

    #[test]
    fn directions() {
        assert_eq!(get_inverse_transform_direction(Forward), Inverse);
        assert_eq!(get_inverse_transform_direction(Inverse), Forward);
        assert_eq!(combine_transform_directions(Forward, Forward), Forward);
        assert_eq!(combine_transform_directions(Inverse, Inverse), Forward);
        assert_eq!(combine_transform_directions(Forward, Inverse), Inverse);
        assert_eq!(combine_transform_directions(Inverse, Forward), Inverse);
    }
}
