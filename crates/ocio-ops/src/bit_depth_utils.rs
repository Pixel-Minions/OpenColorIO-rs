// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! What OCIO knows about each bit depth, and the scalar casts from float to a bit depth's
//! channel type: a port of `src/OpenColorIO/BitDepthUtils.h` and `BitDepthUtils.cpp`
//! @ v2.5.2.
//!
//! - The run-time queries [`get_bit_depth_max_value`], [`is_float_bit_depth`] and
//!   [`get_channel_size_in_bytes`] answer for the six bit depths the CPU processor supports,
//!   and fail with upstream's "Bit depth is not supported" message for the other three.
//! - Upstream's compile-time templates `BitDepthInfo<BD>` and `Converter<BD>` become the
//!   [`BitDepthInfo`] and [`Converter`] traits. Each supported bit depth has a marker type that
//!   implements both: [`Uint8`], [`Uint10`], [`Uint12`], [`Uint16`], [`F16`] and [`F32`].
//!   Generic code takes these types as parameters, where upstream's templates take the
//!   enumerators; a bit depth without a specialization upstream has no marker type here.
//! - [`get_bitdepth_from_max_value`] guesses a LUT file's bit depth from its largest value.
//! - The `CLAMP` macro is [`clamp_macro`].
//!
//! The channel type of `F16` is `half::f16`, as storage only: OCIO's `half` is Imath's, whose
//! conversions differ from the `half` crate's for signaling NaNs, so every conversion goes
//! through [`crate::imath_half`].

use std::fmt::Debug;

use crate::exception::{Exception, Result};
use crate::imath_half;
use crate::math_utils::sse_cvttps_epi32;
use crate::op::{Pixels, PixelsMut};
use crate::open_color_types::{BitDepth, bit_depth_to_string};

/// Port of `errBDNotSupported` (src/OpenColorIO/BitDepthUtils.cpp:12 @ v2.5.2).
const ERR_BD_NOT_SUPPORTED: &str = "Bit depth is not supported: ";

/// The exception the run-time queries throw for a bit depth without a `BitDepthInfo`:
/// `errBDNotSupported + BitDepthToString(in) + "."`.
///
/// Port of the `BIT_DEPTH_UNKNOWN`, `BIT_DEPTH_UINT14`, `BIT_DEPTH_UINT32` and `default` cases
/// of `GetBitDepthMaxValue`, `IsFloatBitDepth` and `GetChannelSizeInBytes`, which build the
/// same message (src/OpenColorIO/BitDepthUtils.cpp:35-44, 107-116 and 137-146 @ v2.5.2).
fn bit_depth_not_supported(bit_depth: BitDepth) -> Exception {
    let mut err = String::from(ERR_BD_NOT_SUPPORTED);
    err += bit_depth_to_string(bit_depth);
    err += ".";
    Exception::new(err)
}

/// The largest value of a bit depth's channel, as a double: `BitDepthInfo<BD>::maxValue`.
/// Callers often divide two of these to get a scale factor, and upstream wants that ratio in
/// double precision.
///
/// Port of `GetBitDepthMaxValue` (src/OpenColorIO/BitDepthUtils.cpp:18-46 @ v2.5.2).
pub fn get_bit_depth_max_value(bit_depth: BitDepth) -> Result<f64> {
    match bit_depth {
        BitDepth::Uint8 => Ok(f64::from(Uint8::MAX_VALUE)),
        BitDepth::Uint10 => Ok(f64::from(Uint10::MAX_VALUE)),
        BitDepth::Uint12 => Ok(f64::from(Uint12::MAX_VALUE)),
        BitDepth::Uint16 => Ok(f64::from(Uint16::MAX_VALUE)),
        BitDepth::F16 => Ok(f64::from(F16::MAX_VALUE)),
        BitDepth::F32 => Ok(f64::from(F32::MAX_VALUE)),

        BitDepth::Unknown | BitDepth::Uint14 | BitDepth::Uint32 => {
            Err(bit_depth_not_supported(bit_depth))
        }
    }
}

/// Whether the bit depth is a float type: `BitDepthInfo<BD>::isFloat`.
///
/// Port of `IsFloatBitDepth` (src/OpenColorIO/BitDepthUtils.cpp:90-118 @ v2.5.2).
pub fn is_float_bit_depth(bit_depth: BitDepth) -> Result<bool> {
    match bit_depth {
        BitDepth::Uint8 => Ok(Uint8::IS_FLOAT),
        BitDepth::Uint10 => Ok(Uint10::IS_FLOAT),
        BitDepth::Uint12 => Ok(Uint12::IS_FLOAT),
        BitDepth::Uint16 => Ok(Uint16::IS_FLOAT),
        BitDepth::F16 => Ok(F16::IS_FLOAT),
        BitDepth::F32 => Ok(F32::IS_FLOAT),

        BitDepth::Unknown | BitDepth::Uint14 | BitDepth::Uint32 => {
            Err(bit_depth_not_supported(bit_depth))
        }
    }
}

/// `sizeof(BitDepthInfo<BD>::Type)`, as the `unsigned` that `GetChannelSizeInBytes` returns.
fn channel_size<BD: BitDepthInfo>() -> u32 {
    size_of::<BD::Type>() as u32
}

/// The size in bytes of one channel: `sizeof(BitDepthInfo<BD>::Type)`.
///
/// Port of `GetChannelSizeInBytes` (src/OpenColorIO/BitDepthUtils.cpp:121-148 @ v2.5.2).
pub fn get_channel_size_in_bytes(bit_depth: BitDepth) -> Result<u32> {
    match bit_depth {
        BitDepth::Uint8 => Ok(channel_size::<Uint8>()),
        BitDepth::Uint10 => Ok(channel_size::<Uint10>()),
        BitDepth::Uint12 => Ok(channel_size::<Uint12>()),
        BitDepth::Uint16 => Ok(channel_size::<Uint16>()),
        BitDepth::F16 => Ok(channel_size::<F16>()),
        BitDepth::F32 => Ok(channel_size::<F32>()),

        BitDepth::Uint14 | BitDepth::Uint32 | BitDepth::Unknown => {
            Err(bit_depth_not_supported(bit_depth))
        }
    }
}

/// A channel type of the CPU processor, `BitDepthInfo<BD>::Type`: `u8`, `u16`, `half::f16`
/// (Imath's `half`) or `f32`. What C++ does with such a value implicitly, the port does through
/// these methods.
pub trait ChannelType: Copy + Default + Debug + Send + Sync + 'static {
    /// The value as `float`, as C++ converts it (`in[0] * m_scale` in `BitDepthCast`): integers
    /// exactly, a half through Imath's `half::operator float`.
    fn to_float(self) -> f32;

    /// The value in the first `size_of::<Self>()` bytes, in the machine's byte order: C++
    /// dereferences a `Type *` into an image buffer, at any alignment.
    fn read_ne(bytes: &[u8]) -> Self;

    /// Writes the value into the first `size_of::<Self>()` bytes, in the machine's byte order.
    fn write_ne(self, bytes: &mut [u8]);

    /// The values as the input of a bit-depth conversion.
    fn pixels(values: &[Self]) -> Pixels<'_>;

    /// The values as the output of a bit-depth conversion.
    fn pixels_mut(values: &mut [Self]) -> PixelsMut<'_>;

    /// The values of `pixels`, if they have this type.
    fn from_pixels(pixels: Pixels<'_>) -> Option<&[Self]>;

    /// The values of `pixels`, if they have this type.
    fn from_pixels_mut(pixels: PixelsMut<'_>) -> Option<&mut [Self]>;
}

macro_rules! channel_type {
    ($t:ty, $variant:ident, |$v:ident| $to_float:expr) => {
        impl ChannelType for $t {
            #[inline]
            fn to_float(self) -> f32 {
                let $v = self;
                $to_float
            }

            #[inline]
            fn read_ne(bytes: &[u8]) -> Self {
                let mut raw = [0u8; size_of::<$t>()];
                raw.copy_from_slice(&bytes[..size_of::<$t>()]);
                <$t>::from_ne_bytes(raw)
            }

            #[inline]
            fn write_ne(self, bytes: &mut [u8]) {
                bytes[..size_of::<$t>()].copy_from_slice(&self.to_ne_bytes());
            }

            fn pixels(values: &[Self]) -> Pixels<'_> {
                Pixels::$variant(values)
            }

            fn pixels_mut(values: &mut [Self]) -> PixelsMut<'_> {
                PixelsMut::$variant(values)
            }

            fn from_pixels(pixels: Pixels<'_>) -> Option<&[Self]> {
                match pixels {
                    Pixels::$variant(values) => Some(values),
                    _ => None,
                }
            }

            fn from_pixels_mut(pixels: PixelsMut<'_>) -> Option<&mut [Self]> {
                match pixels {
                    PixelsMut::$variant(values) => Some(values),
                    _ => None,
                }
            }
        }
    };
}

channel_type!(u8, U8, |v| f32::from(v));
channel_type!(u16, U16, |v| f32::from(v));
channel_type!(half::f16, F16, |v| imath_half::half_to_float(v.to_bits()));
channel_type!(f32, F32, |v| v);

/// What OCIO knows about a supported bit depth at compile time. Implemented by one marker type
/// per bit depth ([`Uint8`], [`Uint10`], [`Uint12`], [`Uint16`], [`F16`], [`F32`]); upstream
/// leaves `BitDepthInfo` incomplete for the others, so that using one fails to compile.
///
/// Port of `BitDepthInfo<BD>` (src/OpenColorIO/BitDepthUtils.h:27-74 @ v2.5.2).
pub trait BitDepthInfo {
    /// `BitDepthInfo<BD>::Type`: how one channel is stored.
    type Type: ChannelType;
    /// `BitDepthInfo<BD>::isFloat`.
    const IS_FLOAT: bool;
    /// `BitDepthInfo<BD>::maxValue`.
    const MAX_VALUE: u32;
    /// The template argument `BD`.
    const BIT_DEPTH: BitDepth;
}

/// `BIT_DEPTH_UINT8` as a type parameter: a `uint8_t` channel with values up to 255.
#[derive(Debug, Clone, Copy)]
pub struct Uint8;

/// `BIT_DEPTH_UINT10` as a type parameter: values up to 1023, stored in a `uint16_t`.
#[derive(Debug, Clone, Copy)]
pub struct Uint10;

/// `BIT_DEPTH_UINT12` as a type parameter: values up to 4095, stored in a `uint16_t`.
#[derive(Debug, Clone, Copy)]
pub struct Uint12;

/// `BIT_DEPTH_UINT16` as a type parameter: a `uint16_t` channel with values up to 65535.
#[derive(Debug, Clone, Copy)]
pub struct Uint16;

/// `BIT_DEPTH_F16` as a type parameter: a half-float channel, with 1 as its maximum value.
#[derive(Debug, Clone, Copy)]
pub struct F16;

/// `BIT_DEPTH_F32` as a type parameter: a float channel, with 1 as its maximum value.
#[derive(Debug, Clone, Copy)]
pub struct F32;

/// Port of `BitDepthInfo<BIT_DEPTH_UINT8>` (src/OpenColorIO/BitDepthUtils.h:34-39 @ v2.5.2).
impl BitDepthInfo for Uint8 {
    type Type = u8;
    const IS_FLOAT: bool = false;
    const MAX_VALUE: u32 = 255;
    const BIT_DEPTH: BitDepth = BitDepth::Uint8;
}

/// Port of `BitDepthInfo<BIT_DEPTH_UINT10>` (src/OpenColorIO/BitDepthUtils.h:41-46 @ v2.5.2).
impl BitDepthInfo for Uint10 {
    type Type = u16;
    const IS_FLOAT: bool = false;
    const MAX_VALUE: u32 = 1023;
    const BIT_DEPTH: BitDepth = BitDepth::Uint10;
}

/// Port of `BitDepthInfo<BIT_DEPTH_UINT12>` (src/OpenColorIO/BitDepthUtils.h:48-53 @ v2.5.2).
impl BitDepthInfo for Uint12 {
    type Type = u16;
    const IS_FLOAT: bool = false;
    const MAX_VALUE: u32 = 4095;
    const BIT_DEPTH: BitDepth = BitDepth::Uint12;
}

/// Port of `BitDepthInfo<BIT_DEPTH_UINT16>` (src/OpenColorIO/BitDepthUtils.h:55-60 @ v2.5.2).
impl BitDepthInfo for Uint16 {
    type Type = u16;
    const IS_FLOAT: bool = false;
    const MAX_VALUE: u32 = 65535;
    const BIT_DEPTH: BitDepth = BitDepth::Uint16;
}

/// Port of `BitDepthInfo<BIT_DEPTH_F16>` (src/OpenColorIO/BitDepthUtils.h:62-67 @ v2.5.2). The
/// channel type is Imath's `half` upstream; here `half::f16` stores its bits.
impl BitDepthInfo for F16 {
    type Type = half::f16;
    const IS_FLOAT: bool = true;
    const MAX_VALUE: u32 = 1;
    const BIT_DEPTH: BitDepth = BitDepth::F16;
}

/// Port of `BitDepthInfo<BIT_DEPTH_F32>` (src/OpenColorIO/BitDepthUtils.h:69-74 @ v2.5.2).
impl BitDepthInfo for F32 {
    type Type = f32;
    const IS_FLOAT: bool = true;
    const MAX_VALUE: u32 = 1;
    const BIT_DEPTH: BitDepth = BitDepth::F32;
}

/// `MiddleMaxValue<A, B>()`: the midpoint of two bit depths' maximum values, rounded down.
///
/// Port of `MiddleMaxValue` (src/OpenColorIO/BitDepthUtils.cpp:51-55 @ v2.5.2).
const fn middle_max_value<A: BitDepthInfo, B: BitDepthInfo>() -> u32 {
    (A::MAX_VALUE + B::MAX_VALUE) / 2
}

/// The bit depth a LUT file's values are scaled for, inferred from the largest value: for
/// formats that don't say. The breakpoints lie midway between the bit depths' maximum values,
/// so that a LUT with a few values beyond its nominal range still gets its intended bit depth.
///
/// Port of `GetBitdepthFromMaxValue` (src/OpenColorIO/BitDepthUtils.cpp:59-87 @ v2.5.2).
pub fn get_bitdepth_from_max_value(max_value: u32) -> BitDepth {
    if max_value < middle_max_value::<F32, Uint8>() {
        // 128
        BitDepth::F32
    } else if max_value < middle_max_value::<Uint8, Uint10>() {
        // 639
        BitDepth::Uint8
    } else if max_value < middle_max_value::<Uint10, Uint12>() {
        // 2559
        BitDepth::Uint10
    } else if max_value < middle_max_value::<Uint12, Uint16>() {
        // 34815
        BitDepth::Uint12
    } else {
        BitDepth::Uint16
    }
}

/// The `CLAMP(a, min, max)` macro: `(a > max) ? max : ((min > a) ? min : a)`. Both comparisons
/// are false for a NaN `a`, so it comes out unchanged, unlike with OCIO's `Clamp`
/// ([`crate::math_utils::clamp`]), which returns `min`.
///
/// The macro works on mixed types, with C++'s usual arithmetic conversions; callers convert the
/// arguments to the type the macro's comparisons use.
///
/// Port of `CLAMP` (src/OpenColorIO/BitDepthUtils.h:79-81 @ v2.5.2).
#[inline]
pub fn clamp_macro<T: PartialOrd>(a: T, min: T, max: T) -> T {
    if a > max {
        max
    } else if min > a {
        min
    } else {
        a
    }
}

/// The cast from float to a supported bit depth's channel type, as the CPU engine's bit-depth
/// conversions do it. Upstream leaves `Converter` incomplete for the other bit depths.
///
/// Port of `Converter<BD>` (src/OpenColorIO/BitDepthUtils.h:84-162 @ v2.5.2).
pub trait Converter: BitDepthInfo {
    /// `Converter<BD>::CastValue`.
    fn cast_value(value: f32) -> Self::Type;
}

/// The shared body of the integer `CastValue`s: `v = value + 0.5f`, then
/// `(Type)CLAMP(v, 0.0f, maxValue)`. The comparisons convert the unsigned `maxValue` to float,
/// and so does the conditional expression, so the clamped value is a float, which the cast
/// truncates toward zero.
///
/// A NaN reaches the cast unclamped (both comparisons are false). Converting it is undefined
/// behaviour in C++; x86-64 compilers emit a 32-bit `cvttss2si`, which gives
/// [`INTEGER_INDEFINITE`](crate::math_utils::INTEGER_INDEFINITE), and the callers keep its low
/// 8 or 16 bits, which are 0.
///
/// Port of the integer `Converter<BD>::CastValue` bodies (src/OpenColorIO/BitDepthUtils.h:95-100,
/// 108-113, 121-126 and 134-139 @ v2.5.2).
#[inline]
fn cast_value_uint(value: f32, max_value: u32) -> i32 {
    // Compute once here instead of several times in the macro.
    let v = value + 0.5f32;
    sse_cvttps_epi32(clamp_macro(v, 0.0f32, max_value as f32))
}

/// Add 0.5, clamp to [0, 255], truncate.
///
/// Port of `Converter<BIT_DEPTH_UINT8>` (src/OpenColorIO/BitDepthUtils.h:90-101 @ v2.5.2).
impl Converter for Uint8 {
    #[inline]
    fn cast_value(value: f32) -> u8 {
        cast_value_uint(value, Self::MAX_VALUE) as u8
    }
}

/// Add 0.5, clamp to [0, 1023], truncate.
///
/// Port of `Converter<BIT_DEPTH_UINT10>` (src/OpenColorIO/BitDepthUtils.h:103-114 @ v2.5.2).
impl Converter for Uint10 {
    #[inline]
    fn cast_value(value: f32) -> u16 {
        cast_value_uint(value, Self::MAX_VALUE) as u16
    }
}

/// Add 0.5, clamp to [0, 4095], truncate.
///
/// Port of `Converter<BIT_DEPTH_UINT12>` (src/OpenColorIO/BitDepthUtils.h:116-127 @ v2.5.2).
impl Converter for Uint12 {
    #[inline]
    fn cast_value(value: f32) -> u16 {
        cast_value_uint(value, Self::MAX_VALUE) as u16
    }
}

/// Add 0.5, clamp to [0, 65535], truncate.
///
/// Port of `Converter<BIT_DEPTH_UINT16>` (src/OpenColorIO/BitDepthUtils.h:129-140 @ v2.5.2).
impl Converter for Uint16 {
    #[inline]
    fn cast_value(value: f32) -> u16 {
        cast_value_uint(value, Self::MAX_VALUE) as u16
    }
}

/// `half(value)`: Imath's constructor, which rounds to nearest even
/// ([`imath_half::float_to_half`]).
///
/// Port of `Converter<BIT_DEPTH_F16>` (src/OpenColorIO/BitDepthUtils.h:142-151 @ v2.5.2).
impl Converter for F16 {
    #[inline]
    fn cast_value(value: f32) -> half::f16 {
        half::f16::from_bits(imath_half::float_to_half(value))
    }
}

/// `float(value)`: the value unchanged.
///
/// Port of `Converter<BIT_DEPTH_F32>` (src/OpenColorIO/BitDepthUtils.h:153-162 @ v2.5.2).
impl Converter for F32 {
    #[inline]
    fn cast_value(value: f32) -> f32 {
        value
    }
}

#[cfg(test)]
#[path = "bit_depth_utils_tests.rs"]
mod tests;
