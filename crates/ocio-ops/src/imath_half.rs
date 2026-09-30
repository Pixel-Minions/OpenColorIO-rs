// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from Imath 3.2.1 (src/Imath/half.h), BSD-3-Clause,
// Copyright Contributors to the OpenEXR Project.

//! The half-float conversions of Imath 3.2.1, as OpenColorIO 2.5.2's wheels build them.
//!
//! OCIO's `half` type is Imath's (`BitDepthUtils.h:9 @ v2.5.2`). Two build choices select
//! which of Imath's conversion paths OCIO runs:
//! - Imath is built with `IMATH_HALF_USE_LOOKUP_TABLE=OFF`
//!   (share/cmake/modules/install/InstallImath.cmake:85 @ v2.5.2), so half to float uses
//!   Imath's bit-shifting code, not its table.
//! - OCIO's library sources are compiled without `-mf16c` (only the AVX and AVX2 kernel files
//!   get it, and MSVC never defines `__F16C__`), so `half.h` does not use F16C
//!   (half.h:287 and :376 @ Imath v3.2.1).
//!
//! These differ from F16C, SSE2's software conversion and the `half` crate only for NaNs:
//! Imath keeps a signaling NaN signaling in both directions (spike S4, `docs/spikes/s4.md`).

/// Converts half bits to float. Port of `imath_half_to_float` without F16C or the lookup
/// table (src/Imath/half.h:284-364 @ Imath v3.2.1).
pub fn half_to_float(h: u16) -> f32 {
    let h = u32::from(h);
    let hexpmant = (h << 17) >> 4;
    let mut v = (h >> 15) << 31;

    if hexpmant >= 0x0080_0000 {
        v |= hexpmant;
        // Either a normal number, in which case add the bias difference, or make sure all the
        // exponent bits are set (infinity or NaN).
        if hexpmant < 0x0f80_0000 {
            v += 0x3800_0000;
        } else {
            v |= 0x7f80_0000;
        }
    } else if hexpmant != 0 {
        // A denormal: the exponent is 0, so the mantissa can be used as is. Both the GCC path
        // (`__builtin_clz`) and the MSVC path (`31 - _BitScanReverse`) count leading zeros.
        let lc = hexpmant.leading_zeros() - 8;
        v |= 0x3880_0000;
        v |= hexpmant << lc;
        v -= lc << 23;
    }
    f32::from_bits(v)
}

/// Converts a float to half bits, rounding to nearest even. Port of `imath_float_to_half`
/// without F16C and without `IMATH_HALF_ENABLE_FP_EXCEPTIONS`
/// (src/Imath/half.h:373-443 @ Imath v3.2.1).
pub fn float_to_half(f: f32) -> u16 {
    let bits = f.to_bits();
    let mut ui = bits & !0x8000_0000;
    let mut ret = ((bits >> 16) & 0x8000) as u16;

    // Exponent large enough to result in a normal number: round and return.
    if ui >= 0x3880_0000 {
        // Infinity or NaN.
        if ui >= 0x7f80_0000 {
            ret |= 0x7c00;
            if ui == 0x7f80_0000 {
                return ret;
            }
            let m = (ui & 0x007f_ffff) >> 13;
            // Keep at least one mantissa bit after the shift, to preserve NaN-ness.
            return ret | m as u16 | u16::from(m == 0);
        }

        // Too large: round to infinity.
        if ui > 0x477f_efff {
            return ret | 0x7c00;
        }

        ui -= 0x3800_0000;
        ui = (ui + 0x0000_0fff + ((ui >> 13) & 1)) >> 13;
        return ret | ui as u16;
    }

    // Zero, or flushed to zero.
    if ui < 0x3300_0001 {
        return ret;
    }

    // Produce a denormalized half.
    let e = ui >> 23;
    let shift = 0x7e - e;
    let m = 0x0080_0000 | (ui & 0x007f_ffff);
    let r = m << (32 - shift);
    ret |= (m >> shift) as u16;
    if r > 0x8000_0000 || (r == 0x8000_0000 && (ret & 0x1) != 0) {
        ret += 1;
    }
    ret
}
