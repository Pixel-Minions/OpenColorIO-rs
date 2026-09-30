// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

use super::*;
use ocio_testkit::crt;
use ocio_testkit::probe::{self, Rng};

/// Port of `OCIO_ADD_TEST(GpuShaderUtils, float_to_string)` @ v2.5.2.
#[test]
fn float_to_string() {
    let (minus_one, one): (i32, i32) = (-1, 1);
    assert_eq!(get_float_string(1.0f32, GpuLanguage::Glsl1_3), "1.");
    assert_eq!(get_float_string(-11.0f32, GpuLanguage::Glsl1_3), "-11.");
    assert_eq!(get_float_string(-1.0f32, GpuLanguage::Glsl1_3), "-1.");
    assert_eq!(
        get_float_string(minus_one as f32, GpuLanguage::Glsl1_3),
        "-1."
    );
    assert_eq!(get_float_string(one as f32, GpuLanguage::Glsl1_3), "1.");
}

/// A literal's digits are what the platform's C runtime writes for `%.9g` (a `float`, which
/// `std::ostream` promotes to `double`) or `%.17g` (a `double`): upstream's
/// `oss.precision(std::numeric_limits<T>::max_digits10); oss << value`. The C runtime spells
/// infinities and NaNs too. Finite whole numbers get a `.`. Every language but Cg writes the
/// value as it is (Cg clamps it to the half range first; the oracle checks that).
#[test]
fn literals_are_the_c_runtime_digits() {
    let mut floats = probe::specials();
    let mut rng = Rng::new(0x9f10a7);
    floats.extend((0..20_000).map(|_| rng.any_bits()));
    floats.extend((-300i32..=300).map(|i| i as f32));

    let mut doubles: Vec<f64> = floats.iter().map(|&f| f64::from(f)).collect();
    doubles.extend((0..20_000).map(|_| f64::from_bits(rng.next_u64())));
    doubles.extend([f64::MAX, f64::MIN, f64::MIN_POSITIVE, f64::from_bits(1)]);

    for lang in GpuLanguage::ALL {
        if lang == GpuLanguage::Cg {
            continue;
        }
        for &v in &floats {
            let dot = if v.is_finite() && v.fract() == 0.0 {
                "."
            } else {
                ""
            };
            let expected = crt::format_f64("%.9g", f64::from(v)) + dot;
            assert_eq!(
                get_float_string(v, lang),
                expected,
                "{:#010x} {lang:?}",
                v.to_bits()
            );
        }
        for &v in &doubles {
            let dot = if v.is_finite() && v.fract() == 0.0 {
                "."
            } else {
                ""
            };
            let expected = crt::format_f64("%.17g", v) + dot;
            assert_eq!(
                get_float_string(v, lang),
                expected,
                "{:#018x} {lang:?}",
                v.to_bits()
            );
        }
    }
}
