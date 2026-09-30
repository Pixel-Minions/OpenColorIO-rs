// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The public enums the ops need, from `include/OpenColorIO/OpenColorTypes.h` @ v2.5.2, and
//! their helpers from `src/OpenColorIO/ParseUtils.cpp`.
//!
//! They live in `ocio-ops` because op data uses them; the public `ocio` crate re-exports them.
//! So far: `TransformDirection`, `NegativeStyle` and `BitDepth`.

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

/// The bit depth of a color space, or of the images a CPU processor reads and writes. The
/// processor supports only `Uint8`, `Uint10`, `Uint12`, `Uint16`, `F16` and `F32`; the other
/// enumerators exist for upstream's API and are rejected where a supported one is needed.
///
/// The discriminants are upstream's enumerator values.
///
/// Port of `BitDepth` (include/OpenColorIO/OpenColorTypes.h:422-439 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BitDepth {
    /// `BIT_DEPTH_UNKNOWN`.
    Unknown = 0,
    /// `BIT_DEPTH_UINT8`.
    Uint8,
    /// `BIT_DEPTH_UINT10`.
    Uint10,
    /// `BIT_DEPTH_UINT12`.
    Uint12,
    /// `BIT_DEPTH_UINT14`.
    Uint14,
    /// `BIT_DEPTH_UINT16`.
    Uint16,
    /// `BIT_DEPTH_UINT32`: here for historical reasons, but not supported.
    Uint32,
    /// `BIT_DEPTH_F16`.
    F16,
    /// `BIT_DEPTH_F32`.
    F32,
}

/// The bit depth's name in configs and error messages: `8ui`, `10ui`, `12ui`, `14ui`, `16ui`,
/// `32ui`, `16f`, `32f`, or `unknown`.
///
/// Port of `BitDepthToString` (src/OpenColorIO/ParseUtils.cpp:171-182 @ v2.5.2).
pub fn bit_depth_to_string(bit_depth: BitDepth) -> &'static str {
    match bit_depth {
        BitDepth::Uint8 => "8ui",
        BitDepth::Uint10 => "10ui",
        BitDepth::Uint12 => "12ui",
        BitDepth::Uint14 => "14ui",
        BitDepth::Uint16 => "16ui",
        BitDepth::Uint32 => "32ui",
        BitDepth::F16 => "16f",
        BitDepth::F32 => "32f",
        BitDepth::Unknown => "unknown",
    }
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
