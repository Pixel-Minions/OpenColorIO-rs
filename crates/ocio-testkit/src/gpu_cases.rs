// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Upstream's GPU test cases (`tests/gpu/*_test.cpp` @ v2.5.2), as data, one table per op
//! family, each case citing its `OCIO_ADD_GPU_TEST`.
//!
//! Upstream runs these on a GPU (`tests/gpu/GPUUnitTest.cpp`): it renders an image through
//! the case's shader and compares it with the CPU's output, within the case's error
//! threshold. Until the GPU execution harness (Phase 7) can do that, the op families' oracle
//! tests use the same cases for text parity: the port's shader for each case, in every
//! language, must be the wheel's byte for byte. Phase 7 then replays the tables with
//! upstream's thresholds and the harness's other settings, which the tables record.
//!
//! The harness's defaults (GPUUnitTest.h:157-173 @ v2.5.2): an absolute comparison; input
//! values from -1 to 2 (`m_testWideRange`, `m_rangeMin`, `m_rangeMax`), with NaN and
//! infinities (`m_testNaN`, `m_testInfinity`); expected values below 1e-6 compared as 1e-6
//! (`m_expectedMinimalValue`); a legacy shader's LUT edge of 32; GLSL 1.2.

use serde_json::{Value, json};

/// One `OCIO_ADD_GPU_TEST(MatrixOps, ...)`: a `MatrixTransform` with a direction, and a
/// matrix and offsets when the test sets them (`AddMatrixTest`,
/// tests/gpu/MatrixOp_test.cpp:16-35 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MatrixCase {
    /// The test's name.
    pub name: &'static str,
    /// `TRANSFORM_DIR_INVERSE`, rather than forward.
    pub inverse: bool,
    /// `setMatrix`, when the test calls it.
    pub matrix: Option<[f64; 16]>,
    /// `setOffset`, when the test calls it.
    pub offset: Option<[f64; 4]>,
    /// `setLegacyShader(!genericShaderDesc)`: the legacy GPU processor
    /// (`getOptimizedLegacyGPUProcessor`) renders the case, rather than
    /// `getDefaultGPUProcessor`.
    pub legacy_shader: bool,
    /// `setErrorThreshold`: the absolute error allowed.
    pub error_threshold: f32,
}

impl MatrixCase {
    /// The transform's spec for the oracle, as `AddMatrixTest` builds it: `setDirection`, then
    /// `setMatrix` and `setOffset` when the test gives them.
    pub fn transform(&self) -> Value {
        let direction = if self.inverse {
            "TRANSFORM_DIR_INVERSE"
        } else {
            "TRANSFORM_DIR_FORWARD"
        };
        let mut calls = vec![json!(["setDirection", {"enum": direction}])];
        if let Some(m) = self.matrix {
            calls.push(json!(["setMatrix", m.to_vec()]));
        }
        if let Some(o) = self.offset {
            calls.push(json!(["setOffset", o.to_vec()]));
        }
        json!({"class": "MatrixTransform", "calls": calls})
    }
}

/// `g_epsilon` (tests/gpu/MatrixOp_test.cpp:12 @ v2.5.2).
const MATRIX_EPSILON: f32 = 5e-7;

/// The matrix the tests share (tests/gpu/MatrixOp_test.cpp:40-43 @ v2.5.2).
const MATRIX: [f64; 16] = [
    1.1, 0.2, 0.3, 0.4, 0.5, 1.6, 0.7, 0.8, 0.2, 0.1, 1.1, 0.2, 0.3, 0.4, 0.5, 1.6,
];

/// The scale the tests share (tests/gpu/MatrixOp_test.cpp:51-54 @ v2.5.2).
const SCALE: [f64; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, -0.3, 0.0, 0.0, 0.0, 0.0, 0.6, 0.0, 0.0, 0.0, 0.0, 1.0,
];

/// The `MatrixOps` GPU tests (tests/gpu/MatrixOp_test.cpp:38-145 @ v2.5.2), in the file's
/// order.
pub fn matrix_cases() -> Vec<MatrixCase> {
    let case = |name, inverse, matrix, offset, generic: bool| MatrixCase {
        name,
        inverse,
        matrix,
        offset,
        legacy_shader: !generic,
        error_threshold: MATRIX_EPSILON,
    };
    vec![
        case("matrix", false, Some(MATRIX), None, false),
        case("scale", false, Some(SCALE), None, false),
        case("offset", false, None, Some([-0.5, 0.25, -0.25, 0.0]), false),
        case(
            "matrix_offset",
            false,
            Some(MATRIX),
            Some([-0.5, -0.25, 0.25, 0.0]),
            false,
        ),
        case("matrix_inverse", true, Some(MATRIX), None, false),
        case("scale_inverse", true, Some(SCALE), None, false),
        case(
            "offset_inverse",
            true,
            None,
            Some([-0.5, 0.25, -0.25, 0.0]),
            false,
        ),
        case(
            "matrix_offset_inverse",
            true,
            Some(MATRIX),
            Some([-0.5, -0.25, 0.25, 0.0]),
            false,
        ),
        case(
            "matrix_offset_generic_shader",
            false,
            Some(MATRIX),
            Some([-0.0, -0.25, 0.25, 0.0]),
            true,
        ),
        case(
            "matrix_offset_inverse_generic_shader",
            true,
            Some(MATRIX),
            Some([-0.5, -0.25, 0.25, 0.0]),
            true,
        ),
    ]
}

/// The transform of an `OCIO_ADD_GPU_TEST(ExponentOp, ...)` or
/// `OCIO_ADD_GPU_TEST(ExponentWithLinearOp, ...)` (tests/gpu/GammaOp_test.cpp @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GammaTransform {
    /// `AddExponent` (GammaOp_test.cpp:20-38): an `ExponentTransform` in a config of major
    /// version `version`. A version 1 config builds an Exponent op from it, a version 2
    /// config a Gamma op (`BuildExponentOp`, src/OpenColorIO/ops/gamma/GammaOp.cpp:190-216).
    Exponent {
        /// `setValue`.
        value: [f64; 4],
        /// `config->setMajorVersion`.
        version: u32,
    },
    /// `AddExponentWithLinear` (GammaOp_test.cpp:40-58): an `ExponentWithLinearTransform` in
    /// `Config::Create()`, a version 2 config.
    ExponentWithLinear {
        /// `setGamma`.
        gamma: [f64; 4],
        /// `setOffset`.
        offset: [f64; 4],
    },
}

/// One `OCIO_ADD_GPU_TEST(ExponentOp, ...)` or `OCIO_ADD_GPU_TEST(ExponentWithLinearOp, ...)`
/// (tests/gpu/GammaOp_test.cpp @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GammaCase {
    /// The test's group: `ExponentOp` or `ExponentWithLinearOp`.
    pub group: &'static str,
    /// The test's name.
    pub name: &'static str,
    /// The transform and its parameters.
    pub transform: GammaTransform,
    /// `setNegativeStyle`: a `NEGATIVE_*` name.
    pub negative_style: &'static str,
    /// `TRANSFORM_DIR_INVERSE`, rather than forward.
    pub inverse: bool,
    /// `setLegacyShader(true)`: the legacy GPU processor (`getOptimizedLegacyGPUProcessor`)
    /// renders the case, rather than `getDefaultGPUProcessor`.
    pub legacy_shader: bool,
    /// The harness's NaN inputs, unless the test calls `setTestNaN(false)`.
    pub test_nan: bool,
    /// The harness's infinite inputs, unless the test calls `setTestInfinity(false)`.
    pub test_infinity: bool,
    /// `setErrorThreshold`: the absolute error allowed. Where the file chooses it with
    /// `#if OCIO_USE_SSE2`, the SSE2 value: both wheels are built with SSE2.
    pub error_threshold: f32,
}

impl GammaCase {
    /// The major version of the case's config.
    pub fn config_major_version(&self) -> u32 {
        match self.transform {
            GammaTransform::Exponent { version, .. } => version,
            GammaTransform::ExponentWithLinear { .. } => 2,
        }
    }

    /// The transform's spec for the oracle, with the setters in the order the helper calls
    /// them: `AddExponent` calls `setNegativeStyle`, `setDirection`, `setValue`;
    /// `AddExponentWithLinear` calls `setDirection`, `setGamma`, `setOffset`,
    /// `setNegativeStyle`.
    pub fn transform(&self) -> Value {
        let direction = if self.inverse {
            "TRANSFORM_DIR_INVERSE"
        } else {
            "TRANSFORM_DIR_FORWARD"
        };
        let style = json!(["setNegativeStyle", {"enum": self.negative_style}]);
        let dir = json!(["setDirection", {"enum": direction}]);
        match self.transform {
            GammaTransform::Exponent { value, .. } => json!({
                "class": "ExponentTransform",
                "calls": [style, dir, ["setValue", value.to_vec()]],
            }),
            GammaTransform::ExponentWithLinear { gamma, offset } => json!({
                "class": "ExponentWithLinearTransform",
                "calls": [dir, ["setGamma", gamma.to_vec()], ["setOffset", offset.to_vec()], style],
            }),
        }
    }
}

/// `g_epsilon` (tests/gpu/GammaOp_test.cpp:11 @ v2.5.2).
const GAMMA_EPSILON: f32 = 1e-6;

/// The exponents of the `ExponentOp` tests (tests/gpu/GammaOp_test.cpp:63-175 @ v2.5.2).
const EXPONENTS: [f64; 4] = [2.6, 1.0, 1.8, 1.1];

/// `gammaVals` (tests/gpu/GammaOp_test.cpp:187 @ v2.5.2).
const GAMMA_VALS: [f64; 4] = [2.1, 1.0, 2.3, 1.5];

/// `offsetVals` (tests/gpu/GammaOp_test.cpp:188 @ v2.5.2).
const OFFSET_VALS: [f64; 4] = [0.01, 0.0, 0.03, 0.05];

/// The `ExponentOp` and `ExponentWithLinearOp` GPU tests (tests/gpu/GammaOp_test.cpp:61-240
/// @ v2.5.2), in the file's order.
pub fn gamma_cases() -> Vec<GammaCase> {
    #[allow(clippy::too_many_arguments)]
    fn case(
        group: &'static str,
        name: &'static str,
        transform: GammaTransform,
        negative_style: &'static str,
        inverse: bool,
        legacy_shader: bool,
        (test_nan, test_infinity): (bool, bool),
        error_threshold: f32,
    ) -> GammaCase {
        GammaCase {
            group,
            name,
            transform,
            negative_style,
            inverse,
            legacy_shader,
            test_nan,
            test_infinity,
            error_threshold,
        }
    }
    let v1 = GammaTransform::Exponent {
        value: EXPONENTS,
        version: 1,
    };
    let v2 = GammaTransform::Exponent {
        value: EXPONENTS,
        version: 2,
    };
    let lin = GammaTransform::ExponentWithLinear {
        gamma: GAMMA_VALS,
        offset: OFFSET_VALS,
    };
    let (exp, exp_lin) = ("ExponentOp", "ExponentWithLinearOp");
    // (setTestNaN, setTestInfinity)
    let no_nan = (false, true);
    let no_inf = (true, false);
    let both = (true, true);
    let neither = (false, false);
    vec![
        case(
            exp,
            "legacy_shader_v1",
            v1,
            "NEGATIVE_CLAMP",
            false,
            true,
            no_nan,
            1e-5,
        ),
        case(
            exp,
            "forward_v1",
            v1,
            "NEGATIVE_CLAMP",
            false,
            false,
            no_nan,
            1e-5,
        ),
        case(
            exp,
            "forward",
            v2,
            "NEGATIVE_CLAMP",
            false,
            false,
            both,
            5e-4,
        ),
        case(
            exp,
            "forward_mirror",
            v2,
            "NEGATIVE_MIRROR",
            false,
            false,
            no_nan,
            5e-4,
        ),
        case(
            exp,
            "forward_pass_thru",
            v2,
            "NEGATIVE_PASS_THRU",
            false,
            false,
            no_inf,
            5e-4,
        ),
        case(
            exp,
            "inverse_legacy_shader_v1",
            v1,
            "NEGATIVE_CLAMP",
            true,
            true,
            no_nan,
            GAMMA_EPSILON,
        ),
        case(
            exp,
            "inverse_v1",
            v1,
            "NEGATIVE_CLAMP",
            true,
            false,
            no_nan,
            GAMMA_EPSILON,
        ),
        case(
            exp,
            "inverse",
            v2,
            "NEGATIVE_CLAMP",
            true,
            false,
            no_inf,
            5e-4,
        ),
        case(
            exp,
            "inverse_mirror",
            v2,
            "NEGATIVE_MIRROR",
            true,
            false,
            neither,
            5e-4,
        ),
        case(
            exp,
            "inverse_pass_thru",
            v2,
            "NEGATIVE_PASS_THRU",
            true,
            false,
            no_inf,
            5e-4,
        ),
        case(
            exp_lin,
            "forward",
            lin,
            "NEGATIVE_LINEAR",
            false,
            false,
            no_inf,
            1e-4,
        ),
        case(
            exp_lin,
            "mirror_forward",
            lin,
            "NEGATIVE_MIRROR",
            false,
            false,
            no_inf,
            1e-4,
        ),
        case(
            exp_lin,
            "inverse",
            lin,
            "NEGATIVE_LINEAR",
            true,
            false,
            no_inf,
            5e-5,
        ),
        case(
            exp_lin,
            "mirror_inverse",
            lin,
            "NEGATIVE_MIRROR",
            true,
            false,
            no_inf,
            5e-5,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each of upstream's ten tests, in the file's order, builds the spec `AddMatrixTest`
    /// builds from its arguments (tests/gpu/MatrixOp_test.cpp:38-145 @ v2.5.2): its name, its
    /// direction, its matrix and offsets when it passes them, and the generic shader (not the
    /// legacy one) for the last two only.
    #[test]
    fn matrix_cases_are_upstreams() {
        let m = MATRIX.to_vec();
        let s = SCALE.to_vec();
        // (name, direction, matrix, offsets, legacy)
        type Expected = (
            &'static str,
            &'static str,
            Option<Vec<f64>>,
            Option<Vec<f64>>,
            bool,
        );
        let expected: Vec<Expected> = vec![
            (
                "matrix",
                "TRANSFORM_DIR_FORWARD",
                Some(m.clone()),
                None,
                true,
            ),
            (
                "scale",
                "TRANSFORM_DIR_FORWARD",
                Some(s.clone()),
                None,
                true,
            ),
            (
                "offset",
                "TRANSFORM_DIR_FORWARD",
                None,
                Some(vec![-0.5, 0.25, -0.25, 0.0]),
                true,
            ),
            (
                "matrix_offset",
                "TRANSFORM_DIR_FORWARD",
                Some(m.clone()),
                Some(vec![-0.5, -0.25, 0.25, 0.0]),
                true,
            ),
            (
                "matrix_inverse",
                "TRANSFORM_DIR_INVERSE",
                Some(m.clone()),
                None,
                true,
            ),
            (
                "scale_inverse",
                "TRANSFORM_DIR_INVERSE",
                Some(s),
                None,
                true,
            ),
            (
                "offset_inverse",
                "TRANSFORM_DIR_INVERSE",
                None,
                Some(vec![-0.5, 0.25, -0.25, 0.0]),
                true,
            ),
            (
                "matrix_offset_inverse",
                "TRANSFORM_DIR_INVERSE",
                Some(m.clone()),
                Some(vec![-0.5, -0.25, 0.25, 0.0]),
                true,
            ),
            (
                "matrix_offset_generic_shader",
                "TRANSFORM_DIR_FORWARD",
                Some(m.clone()),
                Some(vec![-0.0, -0.25, 0.25, 0.0]),
                false,
            ),
            (
                "matrix_offset_inverse_generic_shader",
                "TRANSFORM_DIR_INVERSE",
                Some(m),
                Some(vec![-0.5, -0.25, 0.25, 0.0]),
                false,
            ),
        ];
        let cases = matrix_cases();
        assert_eq!(cases.len(), expected.len());
        for (case, (name, direction, matrix, offset, legacy)) in cases.iter().zip(expected) {
            let mut calls = vec![json!(["setDirection", {"enum": direction}])];
            if let Some(matrix) = matrix {
                calls.push(json!(["setMatrix", matrix]));
            }
            if let Some(offset) = offset {
                calls.push(json!(["setOffset", offset]));
            }
            assert_eq!(case.name, name);
            assert_eq!(
                case.transform(),
                json!({"class": "MatrixTransform", "calls": calls}),
                "{name}"
            );
            assert_eq!(case.legacy_shader, legacy, "{name}");
            assert_eq!(
                case.error_threshold.to_bits(),
                MATRIX_EPSILON.to_bits(),
                "{name}"
            );
        }
        // The generic shader's -0 offset keeps its sign through the spec.
        assert!(
            cases[8].transform()["calls"][2][1][0]
                .as_f64()
                .unwrap()
                .is_sign_negative()
        );
    }

    /// Each of upstream's fourteen tests, in the file's order, builds the spec its helper
    /// builds from its arguments (tests/gpu/GammaOp_test.cpp:20-240 @ v2.5.2): the transform's
    /// class and setters in the helper's order, the config's version, the legacy shader, the
    /// NaN and infinity inputs, and the threshold (the `OCIO_USE_SSE2` one where the file
    /// chooses).
    #[test]
    fn gamma_cases_are_upstreams() {
        let exponents = json!([2.6, 1.0, 1.8, 1.1]);
        let gammas = json!([2.1, 1.0, 2.3, 1.5]);
        let offsets = json!([0.01, 0.0, 0.03, 0.05]);
        let fwd = json!({"enum": "TRANSFORM_DIR_FORWARD"});
        let inv = json!({"enum": "TRANSFORM_DIR_INVERSE"});
        let style = |s: &str| json!(["setNegativeStyle", {"enum": s}]);
        let exponent = |s: &str, dir: &Value| {
            json!({"class": "ExponentTransform", "calls": [
                style(s), ["setDirection", dir], ["setValue", exponents]]})
        };
        let with_linear = |s: &str, dir: &Value| {
            json!({"class": "ExponentWithLinearTransform", "calls": [
                ["setDirection", dir], ["setGamma", gammas], ["setOffset", offsets], style(s)]})
        };
        // (group, name, spec, version, legacy, NaN, infinity, threshold)
        type Expected = (
            &'static str,
            &'static str,
            Value,
            u32,
            bool,
            bool,
            bool,
            f32,
        );
        let expected: Vec<Expected> = vec![
            (
                "ExponentOp",
                "legacy_shader_v1",
                exponent("NEGATIVE_CLAMP", &fwd),
                1,
                true,
                false,
                true,
                1e-5,
            ),
            (
                "ExponentOp",
                "forward_v1",
                exponent("NEGATIVE_CLAMP", &fwd),
                1,
                false,
                false,
                true,
                1e-5,
            ),
            (
                "ExponentOp",
                "forward",
                exponent("NEGATIVE_CLAMP", &fwd),
                2,
                false,
                true,
                true,
                5e-4,
            ),
            (
                "ExponentOp",
                "forward_mirror",
                exponent("NEGATIVE_MIRROR", &fwd),
                2,
                false,
                false,
                true,
                5e-4,
            ),
            (
                "ExponentOp",
                "forward_pass_thru",
                exponent("NEGATIVE_PASS_THRU", &fwd),
                2,
                false,
                true,
                false,
                5e-4,
            ),
            (
                "ExponentOp",
                "inverse_legacy_shader_v1",
                exponent("NEGATIVE_CLAMP", &inv),
                1,
                true,
                false,
                true,
                1e-6,
            ),
            (
                "ExponentOp",
                "inverse_v1",
                exponent("NEGATIVE_CLAMP", &inv),
                1,
                false,
                false,
                true,
                1e-6,
            ),
            (
                "ExponentOp",
                "inverse",
                exponent("NEGATIVE_CLAMP", &inv),
                2,
                false,
                true,
                false,
                5e-4,
            ),
            (
                "ExponentOp",
                "inverse_mirror",
                exponent("NEGATIVE_MIRROR", &inv),
                2,
                false,
                false,
                false,
                5e-4,
            ),
            (
                "ExponentOp",
                "inverse_pass_thru",
                exponent("NEGATIVE_PASS_THRU", &inv),
                2,
                false,
                true,
                false,
                5e-4,
            ),
            (
                "ExponentWithLinearOp",
                "forward",
                with_linear("NEGATIVE_LINEAR", &fwd),
                2,
                false,
                true,
                false,
                1e-4,
            ),
            (
                "ExponentWithLinearOp",
                "mirror_forward",
                with_linear("NEGATIVE_MIRROR", &fwd),
                2,
                false,
                true,
                false,
                1e-4,
            ),
            (
                "ExponentWithLinearOp",
                "inverse",
                with_linear("NEGATIVE_LINEAR", &inv),
                2,
                false,
                true,
                false,
                5e-5,
            ),
            (
                "ExponentWithLinearOp",
                "mirror_inverse",
                with_linear("NEGATIVE_MIRROR", &inv),
                2,
                false,
                true,
                false,
                5e-5,
            ),
        ];
        let cases = gamma_cases();
        assert_eq!(cases.len(), expected.len());
        for (case, (group, name, spec, version, legacy, nan, inf, threshold)) in
            cases.iter().zip(expected)
        {
            assert_eq!((case.group, case.name), (group, name));
            assert_eq!(case.transform(), spec, "{group} {name}");
            assert_eq!(case.config_major_version(), version, "{group} {name}");
            assert_eq!(case.legacy_shader, legacy, "{group} {name}");
            assert_eq!(
                (case.test_nan, case.test_infinity),
                (nan, inf),
                "{group} {name}"
            );
            assert_eq!(
                case.error_threshold.to_bits(),
                threshold.to_bits(),
                "{group} {name}"
            );
        }
    }
}
