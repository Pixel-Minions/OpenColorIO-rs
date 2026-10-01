// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

use super::*;
use ocio_testkit::probe::{self, Rng};
use ocio_testkit::{assert_text_eq, crt};

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
/// infinities and NaNs too, with every sign and payload. In every language but Cg, which
/// clamps the value first, the literal is those digits, with or without a `.` after them.
/// Whether the `.` is there, and Cg's literals, are checked against the wheel's shaders in
/// `tests/gpu_shader_utils_oracle.rs` (`literals_are_the_wheels`).
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
            let literal = get_float_string(v, lang);
            assert_text_eq(
                &format!("{:#010x} {lang:?}", v.to_bits()),
                &crt::format_f64("%.9g", f64::from(v)),
                literal.strip_suffix('.').unwrap_or(&literal),
            );
        }
        for &v in &doubles {
            let literal = get_float_string(v, lang);
            assert_text_eq(
                &format!("{:#018x} {lang:?}", v.to_bits()),
                &crt::format_f64("%.17g", v),
                literal.strip_suffix('.').unwrap_or(&literal),
            );
        }
    }
}
