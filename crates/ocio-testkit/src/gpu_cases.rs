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

/// One `OCIO_ADD_GPU_TEST(RangeOp, ...)`: a `RangeTransform` in `Config::Create()`, with the
/// style and the bounds the test sets (tests/gpu/RangeOp_test.cpp @ v2.5.2). The tests pass
/// `float` literals to the `double` setters, so each bound is that `float`'s value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RangeCase {
    /// The test's name.
    pub name: &'static str,
    /// `setStyle(RANGE_NO_CLAMP)`, which builds a Matrix op rather than a Range op
    /// (`BuildRangeOp`, src/OpenColorIO/ops/range/RangeOp.cpp:263-281).
    pub no_clamp: bool,
    /// `setMinInValue`, when the test calls it.
    pub min_in: Option<f64>,
    /// `setMaxInValue`, when the test calls it.
    pub max_in: Option<f64>,
    /// `setMinOutValue`, when the test calls it.
    pub min_out: Option<f64>,
    /// `setMaxOutValue`, when the test calls it.
    pub max_out: Option<f64>,
    /// `setErrorThreshold`: the absolute error allowed.
    pub error_threshold: f32,
}

impl RangeCase {
    /// The transform's spec for the oracle, with the setters in the test's order: the style
    /// first, then the bounds the test sets. (`setTestWideRange(true)`, which some tests call,
    /// is the harness's default.)
    pub fn transform(&self) -> Value {
        let mut calls = Vec::new();
        if self.no_clamp {
            calls.push(json!(["setStyle", {"enum": "RANGE_NO_CLAMP"}]));
        }
        for (setter, value) in [
            ("setMinInValue", self.min_in),
            ("setMaxInValue", self.max_in),
            ("setMinOutValue", self.min_out),
            ("setMaxOutValue", self.max_out),
        ] {
            if let Some(v) = value {
                calls.push(json!([setter, v]));
            }
        }
        json!({"class": "RangeTransform", "calls": calls})
    }
}

/// `g_epsilon` (tests/gpu/RangeOp_test.cpp:15 @ v2.5.2).
const RANGE_EPSILON: f32 = 1e-6;

/// The `RangeOp` GPU tests (tests/gpu/RangeOp_test.cpp:17-119 @ v2.5.2), in the file's order.
pub fn range_cases() -> Vec<RangeCase> {
    // A `float` literal, as the `double` setter receives it.
    let f = |v: f32| Some(f64::from(v));
    let case = |name, no_clamp, min_in, max_in, min_out, max_out| RangeCase {
        name,
        no_clamp,
        min_in,
        max_in,
        min_out,
        max_out,
        error_threshold: RANGE_EPSILON,
    };
    vec![
        case(
            "scale_with_low_and_high_clippings",
            false,
            f(0.1),
            f(1.1),
            f(0.5),
            f(1.5),
        ),
        case("scale_with_low_clipping", false, f(0.2), None, f(0.2), None),
        case(
            "scale_with_high_clipping",
            false,
            None,
            f(0.9),
            None,
            f(0.9),
        ),
        case(
            "scale_with_low_and_high_clippings_2",
            false,
            f(0.1),
            f(1.1),
            f(-0.5),
            f(1.5),
        ),
        case(
            "arbitrary_1",
            false,
            f(0.4000202),
            f(0.6000502),
            f(0.4000601),
            f(0.6000801),
        ),
        case(
            "arbitrary_1_no_clamp",
            true,
            f(0.4000202),
            f(0.6000502),
            f(0.4000601),
            f(0.6000801),
        ),
        case(
            "arbitrary_2",
            false,
            f(-0.010201),
            f(0.601102),
            f(0.209803),
            f(1.600208),
        ),
        case(
            "arbitrary_2_no_clamp",
            true,
            f(-0.010201),
            f(0.601102),
            f(0.209803),
            f(1.600208),
        ),
    ]
}

/// The parameters of a `CDLOp` GPU test's CDL (tests/gpu/CDLOp_test.cpp @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CdlParams {
    /// `setSlope`.
    pub slope: [f64; 3],
    /// `setOffset`.
    pub offset: [f64; 3],
    /// `setPower`.
    pub power: [f64; 3],
    /// `setSat`, when the test calls it.
    pub sat: Option<f64>,
}

/// One `OCIO_ADD_GPU_TEST(CDLOp, ...)`: a `CDLTransform` in a config of major version
/// `version`, with the setters the test calls (tests/gpu/CDLOp_test.cpp @ v2.5.2). A version 1
/// config builds the CDL as matrices and an exponent, a version 2 config a CDL op
/// (`BuildCDLOp`, src/OpenColorIO/ops/cdl/CDLOp.cpp:200-265).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CdlCase {
    /// The test's name.
    pub name: &'static str,
    /// `setStyle`, when the test calls it: a `CDL_*` name.
    pub style: Option<&'static str>,
    /// `TRANSFORM_DIR_INVERSE`, rather than forward.
    pub inverse: bool,
    /// The CDL's parameters.
    pub params: CdlParams,
    /// `config->setMajorVersion`: 1 when the test calls it, otherwise `Config::Create()`'s 2.
    pub version: u32,
    /// `setLegacyShader(true)`.
    pub legacy_shader: bool,
    /// `setTestWideRange`: every test calls it.
    pub test_wide_range: bool,
    /// The harness's NaN inputs, unless the test calls `setTestNaN(false)`.
    pub test_nan: bool,
    /// The harness's infinite inputs, unless the test calls `setTestInfinity(false)`.
    pub test_infinity: bool,
    /// `setErrorThreshold`: the absolute error allowed (every test calls
    /// `setRelativeComparison(false)`).
    pub error_threshold: f32,
}

impl CdlCase {
    /// The transform's spec for the oracle, with the setters in the test's order: the style
    /// when it sets one, the direction, the slope, offset and power, then the saturation when
    /// it sets one.
    pub fn transform(&self) -> Value {
        let mut calls = Vec::new();
        if let Some(style) = self.style {
            calls.push(json!(["setStyle", {"enum": style}]));
        }
        let direction = if self.inverse {
            "TRANSFORM_DIR_INVERSE"
        } else {
            "TRANSFORM_DIR_FORWARD"
        };
        calls.push(json!(["setDirection", {"enum": direction}]));
        calls.push(json!(["setSlope", self.params.slope.to_vec()]));
        calls.push(json!(["setOffset", self.params.offset.to_vec()]));
        calls.push(json!(["setPower", self.params.power.to_vec()]));
        if let Some(sat) = self.params.sat {
            calls.push(json!(["setSat", sat]));
        }
        json!({"class": "CDLTransform", "calls": calls})
    }
}

/// `CDL_Data_1` (tests/gpu/CDLOp_test.cpp:20-25 @ v2.5.2).
const CDL_DATA_1: CdlParams = CdlParams {
    slope: [1.35, 1.10, 0.71],
    offset: [0.05, -0.23, 0.11],
    power: [0.93, 0.81, 1.27],
    sat: None,
};

/// `CDL_Data_2` (tests/gpu/CDLOp_test.cpp:149-155 @ v2.5.2).
const CDL_DATA_2: CdlParams = CdlParams {
    slope: [1.15, 1.10, 0.90],
    offset: [0.05, -0.02, 0.07],
    power: [1.20, 0.95, 1.13],
    sat: Some(0.9),
};

/// `CDL_Data_3` (tests/gpu/CDLOp_test.cpp:282-288 @ v2.5.2).
const CDL_DATA_3: CdlParams = CdlParams {
    slope: [3.405, 1.0, 1.0],
    offset: [-0.178, -0.178, -0.178],
    power: [1.095, 1.095, 1.0],
    sat: Some(1.2),
};

/// The `CDLOp` GPU tests (tests/gpu/CDLOp_test.cpp:27-352 @ v2.5.2), in the file's order.
pub fn cdl_cases() -> Vec<CdlCase> {
    // (name, style, inverse, params, version, legacy, wide range, NaN, infinity, threshold)
    type Row = (
        &'static str,
        Option<&'static str>,
        bool,
        CdlParams,
        u32,
        bool,
        bool,
        bool,
        bool,
        f32,
    );
    let (asc, no_clamp) = (Some("CDL_ASC"), Some("CDL_NO_CLAMP"));
    let rows: [Row; 15] = [
        (
            "clamp_fwd_v1_legacy_shader",
            None,
            false,
            CDL_DATA_1,
            1,
            true,
            true,
            false,
            true,
            1e-6,
        ),
        (
            "clamp_fwd_v1",
            None,
            false,
            CDL_DATA_1,
            1,
            false,
            true,
            false,
            true,
            1e-6,
        ),
        (
            "clamp_fwd_v2",
            asc,
            false,
            CDL_DATA_1,
            2,
            false,
            true,
            true,
            true,
            1e-5,
        ),
        (
            "clamp_fwd_no_clamp_v2",
            no_clamp,
            false,
            CDL_DATA_1,
            2,
            false,
            true,
            false,
            false,
            5e-5,
        ),
        (
            "clamp_inv_v2",
            asc,
            true,
            CDL_DATA_1,
            2,
            false,
            true,
            true,
            true,
            1e-4,
        ),
        (
            "clamp_inv_no_clamp_v2",
            no_clamp,
            true,
            CDL_DATA_1,
            2,
            false,
            true,
            false,
            false,
            1e-4,
        ),
        (
            "clamp_fwd_v1_legacy_shader_Data_2",
            None,
            false,
            CDL_DATA_2,
            1,
            true,
            true,
            false,
            false,
            1e-6,
        ),
        (
            "clamp_fwd_v1_Data_2",
            None,
            false,
            CDL_DATA_2,
            1,
            false,
            true,
            false,
            false,
            1e-6,
        ),
        (
            "clamp_fwd_v2_Data_2",
            asc,
            false,
            CDL_DATA_2,
            2,
            false,
            true,
            true,
            true,
            2e-5,
        ),
        (
            "clamp_inv_v2_Data_2",
            asc,
            true,
            CDL_DATA_2,
            2,
            false,
            true,
            true,
            true,
            2e-5,
        ),
        (
            "clamp_fwd_no_clamp_v2_Data_2",
            no_clamp,
            false,
            CDL_DATA_2,
            2,
            false,
            true,
            false,
            false,
            5e-5,
        ),
        (
            "clamp_inv_no_clamp_v2_Data_2",
            no_clamp,
            true,
            CDL_DATA_2,
            2,
            false,
            true,
            false,
            false,
            5e-5,
        ),
        (
            "clamp_fwd_v2_Data_3",
            asc,
            false,
            CDL_DATA_3,
            2,
            false,
            true,
            true,
            true,
            5e-5,
        ),
        (
            "clamp_fwd_no_clamp_v2_Data_3",
            no_clamp,
            false,
            CDL_DATA_3,
            2,
            false,
            false,
            false,
            false,
            5e-5,
        ),
        (
            "clamp_inv_no_clamp_v2_Data_3",
            no_clamp,
            true,
            CDL_DATA_3,
            2,
            false,
            false,
            false,
            false,
            5e-5,
        ),
    ];
    rows.into_iter()
        .map(
            |(name, style, inverse, params, version, legacy, wide, nan, inf, threshold)| CdlCase {
                name,
                style,
                inverse,
                params,
                version,
                legacy_shader: legacy,
                test_wide_range: wide,
                test_nan: nan,
                test_infinity: inf,
                error_threshold: threshold,
            },
        )
        .collect()
}

/// The parameters a `LogAffineTransform` or `LogCameraTransform` GPU test sets, each when it
/// sets it (tests/gpu/LogOp_test.cpp @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LogParams {
    /// `setLogSideSlopeValue`.
    pub log_side_slope: Option<[f64; 3]>,
    /// `setLogSideOffsetValue`.
    pub log_side_offset: Option<[f64; 3]>,
    /// `setLinSideSlopeValue`.
    pub lin_side_slope: Option<[f64; 3]>,
    /// `setLinSideOffsetValue`.
    pub lin_side_offset: Option<[f64; 3]>,
    /// `setLinearSlopeValue` (camera logs).
    pub linear_slope: Option<[f64; 3]>,
}

/// One `OCIO_ADD_GPU_TEST(LogTransform, ...)`, `(LogAffineTransform, ...)` or
/// `(LogCameraTransform, ...)` (tests/gpu/LogOp_test.cpp @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogCase {
    /// The test's group, which is the transform's class.
    pub group: &'static str,
    /// The test's name.
    pub name: &'static str,
    /// `TRANSFORM_DIR_INVERSE`, rather than forward.
    pub inverse: bool,
    /// `setBase`, when the test calls it: the `float` the test passes, as the `double` the
    /// setter takes.
    pub base: Option<f64>,
    /// `LogCameraTransform::Create`'s break on the linear side.
    pub lin_side_break: Option<[f64; 3]>,
    /// The parameters the test sets.
    pub params: LogParams,
    /// `setLegacyShader(true)`.
    pub legacy_shader: bool,
    /// `setRelativeComparison(true)`, rather than the absolute default.
    pub relative_comparison: bool,
    /// The harness's NaN inputs, unless the test calls `setTestNaN(false)` (the camera tests
    /// call it on Apple platforms only).
    pub test_nan: bool,
    /// The harness's infinite inputs, unless the test calls `setTestInfinity(false)`.
    pub test_infinity: bool,
    /// `setErrorThreshold`: the error allowed. The file chooses `g_epsilon` and
    /// `g_epsilon_inverse` with `#if OCIO_USE_SSE2`: these are the SSE2 values, as both wheels
    /// are built with SSE2.
    pub error_threshold: f32,
}

impl LogCase {
    /// The transform's spec for the oracle, with the setters in the test's order: the
    /// direction, the base when it sets one, then the log side slope and offset, the linear
    /// side slope and offset, and the linear slope, each when it sets it (every test sets them
    /// in that order). A camera log's break is the constructor's argument.
    pub fn transform(&self) -> Value {
        let direction = if self.inverse {
            "TRANSFORM_DIR_INVERSE"
        } else {
            "TRANSFORM_DIR_FORWARD"
        };
        let mut calls = vec![json!(["setDirection", {"enum": direction}])];
        if let Some(base) = self.base {
            calls.push(json!(["setBase", base]));
        }
        let p = &self.params;
        for (setter, values) in [
            ("setLogSideSlopeValue", p.log_side_slope),
            ("setLogSideOffsetValue", p.log_side_offset),
            ("setLinSideSlopeValue", p.lin_side_slope),
            ("setLinSideOffsetValue", p.lin_side_offset),
            ("setLinearSlopeValue", p.linear_slope),
        ] {
            if let Some(values) = values {
                calls.push(json!([setter, values.to_vec()]));
            }
        }
        match self.lin_side_break {
            Some(brk) => json!({
                "class": self.group,
                "args": {"linSideBreak": brk.to_vec()},
                "calls": calls,
            }),
            None => json!({"class": self.group, "calls": calls}),
        }
    }
}

/// `g_epsilon` with SSE2 (tests/gpu/LogOp_test.cpp:13-14 @ v2.5.2).
const LOG_EPSILON: f32 = 1e-4;

/// `g_epsilon_inverse` with SSE2 (tests/gpu/LogOp_test.cpp:13, 15 @ v2.5.2).
const LOG_EPSILON_INVERSE: f32 = 1e-3;

/// `base10` (tests/gpu/LogOp_test.cpp:21 @ v2.5.2).
const BASE10: f32 = 10.0;

/// `eulerConstant`, `expf(1.0f)` (tests/gpu/LogOp_test.cpp:22 @ v2.5.2), from the platform's
/// `expf` at run time.
fn euler_constant() -> f32 {
    std::hint::black_box(1.0f32).exp()
}

/// The `LogTransform`, `LogAffineTransform` and `LogCameraTransform` GPU tests
/// (tests/gpu/LogOp_test.cpp:45-353 @ v2.5.2), in the file's order.
pub fn log_cases() -> Vec<LogCase> {
    let euler = euler_constant();
    // `AddLogTest` (tests/gpu/LogOp_test.cpp:26-42): a `LogTransform` in `base`, without
    // infinite inputs; every `LogTransform` test then sets no NaN.
    let log_test = |name, inverse, base: f32, epsilon, legacy_shader| LogCase {
        group: "LogTransform",
        name,
        inverse,
        base: Some(f64::from(base)),
        lin_side_break: None,
        params: LogParams::default(),
        legacy_shader,
        relative_comparison: false,
        test_nan: false,
        test_infinity: false,
        error_threshold: epsilon,
    };
    // A `LogAffineTransform` test: no infinite inputs, and NaN ones forward only.
    let affine = |name, inverse, base: Option<f32>, params, error_threshold| LogCase {
        group: "LogAffineTransform",
        name,
        inverse,
        base: base.map(f64::from),
        lin_side_break: None,
        params,
        legacy_shader: false,
        relative_comparison: false,
        test_nan: !inverse,
        test_infinity: false,
        error_threshold,
    };
    let camera = |name, inverse, lin_side_break, params, error_threshold| LogCase {
        group: "LogCameraTransform",
        name,
        inverse,
        base: None,
        lin_side_break: Some(lin_side_break),
        params,
        legacy_shader: false,
        relative_comparison: false,
        test_nan: true,
        test_infinity: false,
        error_threshold,
    };
    let only = |set: fn(&mut LogParams, [f64; 3]), values| {
        let mut p = LogParams::default();
        set(&mut p, values);
        p
    };
    let log_slope = |p: &mut LogParams, v| p.log_side_slope = Some(v);
    let log_offset = |p: &mut LogParams, v| p.log_side_offset = Some(v);
    let lin_slope = |p: &mut LogParams, v| p.lin_side_slope = Some(v);
    let lin_offset = |p: &mut LogParams, v| p.lin_side_offset = Some(v);
    let (eps, eps_inv) = (LOG_EPSILON, LOG_EPSILON_INVERSE);
    let mut base_inverse = affine(
        "base_inverse",
        true,
        Some(BASE10),
        LogParams::default(),
        eps,
    );
    base_inverse.relative_comparison = true;
    vec![
        log_test("LogBase_10_legacy", false, BASE10, eps, true),
        log_test("LogBase_10_legacy_inverse", true, BASE10, eps_inv, true),
        log_test("LogBase_10_generic_shader", false, BASE10, eps, false),
        log_test(
            "LogBase_10_inverse_generic_shader",
            true,
            BASE10,
            eps_inv,
            false,
        ),
        log_test("LogBase_euler_legacy", false, euler, eps, true),
        log_test("LogBase_euler_legacy_inverse", true, euler, eps_inv, true),
        log_test("LogBase_euler_generic_shader", false, euler, eps, false),
        log_test(
            "LogBase_euler_inverse_generic_shader",
            true,
            euler,
            eps_inv,
            false,
        ),
        affine("base", false, Some(BASE10), LogParams::default(), eps),
        base_inverse,
        affine(
            "linSideSlope",
            false,
            None,
            only(lin_slope, [2.0, 0.5, 3.0]),
            eps,
        ),
        affine(
            "linSideSlope_inverse",
            true,
            None,
            only(lin_slope, [2.0, 0.5, 3.0]),
            eps,
        ),
        affine(
            "linSideOffset",
            false,
            None,
            only(lin_offset, [0.1, 0.2, 0.3]),
            eps,
        ),
        affine(
            "linSideOffset_inverse",
            true,
            None,
            only(lin_offset, [0.1, 0.2, 0.3]),
            eps,
        ),
        affine(
            "logSideSlope",
            false,
            None,
            only(log_slope, [2.0, 0.5, 3.0]),
            eps * 5.0,
        ),
        affine(
            "logSideSlope_inverse",
            true,
            None,
            only(log_slope, [2.0, 0.5, 3.0]),
            eps,
        ),
        affine(
            "logSideOffset",
            false,
            None,
            only(log_offset, [0.1, 0.2, 0.3]),
            eps,
        ),
        affine(
            "logSideOffset_inverse",
            true,
            None,
            only(log_offset, [0.1, 0.2, 0.3]),
            eps,
        ),
        affine(
            "lin2log",
            false,
            None,
            LogParams {
                log_side_slope: Some([0.2, 0.4, 0.25]),
                log_side_offset: Some([0.14, 0.13, 0.12]),
                lin_side_slope: Some([1.5, 1.8, 1.2]),
                lin_side_offset: Some([0.05, 0.1, 0.15]),
                linear_slope: None,
            },
            eps * 5.0,
        ),
        // The only inverse affine test that keeps the NaN inputs.
        LogCase {
            test_nan: true,
            ..affine(
                "log2lin",
                true,
                None,
                LogParams {
                    log_side_slope: Some([0.21, 0.2, 0.19]),
                    log_side_offset: Some([0.61, 0.6, 0.59]),
                    lin_side_slope: Some([1.11, 1.1, 1.12]),
                    lin_side_offset: Some([0.051, 0.05, 0.052]),
                    linear_slope: None,
                },
                eps_inv,
            )
        },
        camera(
            "camera_lin2log",
            false,
            [0.12, 0.13, 0.15],
            LogParams {
                log_side_slope: Some([0.2, 0.3, 0.4]),
                log_side_offset: Some([0.7, 0.6, 0.5]),
                lin_side_slope: Some([1.4, 1.1, 1.2]),
                lin_side_offset: Some([0.15, 0.16, 0.25]),
                linear_slope: Some([1.22, 1.33, 1.44]),
            },
            eps,
        ),
        camera(
            "camera_log2lin",
            true,
            [0.12, 0.13, 0.14],
            LogParams {
                log_side_slope: Some([0.21, 0.22, 0.23]),
                log_side_offset: Some([0.6, 0.7, 0.8]),
                lin_side_slope: Some([1.1, 1.2, 1.3]),
                lin_side_offset: Some([0.051, 0.052, 0.053]),
                linear_slope: Some([1.25, 1.23, 1.22]),
            },
            eps_inv,
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

    /// Each of upstream's eight tests, in the file's order, builds the spec its body builds
    /// (tests/gpu/RangeOp_test.cpp:17-119 @ v2.5.2): the style first when it sets one, then
    /// each bound it sets, as the `float` literal's value, and `g_epsilon`.
    #[test]
    fn range_cases_are_upstreams() {
        let style = json!(["setStyle", {"enum": "RANGE_NO_CLAMP"}]);
        let min_in = |v: f32| json!(["setMinInValue", f64::from(v)]);
        let max_in = |v: f32| json!(["setMaxInValue", f64::from(v)]);
        let min_out = |v: f32| json!(["setMinOutValue", f64::from(v)]);
        let max_out = |v: f32| json!(["setMaxOutValue", f64::from(v)]);
        let range = |calls: Vec<Value>| json!({"class": "RangeTransform", "calls": calls});
        let expected: Vec<(&str, Value)> = vec![
            (
                "scale_with_low_and_high_clippings",
                range(vec![min_in(0.1), max_in(1.1), min_out(0.5), max_out(1.5)]),
            ),
            (
                "scale_with_low_clipping",
                range(vec![min_in(0.2), min_out(0.2)]),
            ),
            (
                "scale_with_high_clipping",
                range(vec![max_in(0.9), max_out(0.9)]),
            ),
            (
                "scale_with_low_and_high_clippings_2",
                range(vec![min_in(0.1), max_in(1.1), min_out(-0.5), max_out(1.5)]),
            ),
            (
                "arbitrary_1",
                range(vec![
                    min_in(0.4000202),
                    max_in(0.6000502),
                    min_out(0.4000601),
                    max_out(0.6000801),
                ]),
            ),
            (
                "arbitrary_1_no_clamp",
                range(vec![
                    style.clone(),
                    min_in(0.4000202),
                    max_in(0.6000502),
                    min_out(0.4000601),
                    max_out(0.6000801),
                ]),
            ),
            (
                "arbitrary_2",
                range(vec![
                    min_in(-0.010201),
                    max_in(0.601102),
                    min_out(0.209803),
                    max_out(1.600208),
                ]),
            ),
            (
                "arbitrary_2_no_clamp",
                range(vec![
                    style,
                    min_in(-0.010201),
                    max_in(0.601102),
                    min_out(0.209803),
                    max_out(1.600208),
                ]),
            ),
        ];
        let cases = range_cases();
        assert_eq!(cases.len(), expected.len());
        for (case, (name, spec)) in cases.iter().zip(expected) {
            assert_eq!(case.name, name);
            assert_eq!(case.transform(), spec, "{name}");
            assert_eq!(case.error_threshold.to_bits(), 1e-6f32.to_bits(), "{name}");
        }
        // The `float` literal reaches the setter, not its decimal text.
        assert_ne!(cases[0].min_in, Some(0.1));
    }

    /// Each of upstream's fifteen tests, in the file's order, builds the spec its body builds
    /// (tests/gpu/CDLOp_test.cpp:20-352 @ v2.5.2): the style when it sets one, the direction,
    /// the data set's slope, offset and power, and its saturation when it sets one; the
    /// config's version, the legacy shader, the wide range, NaN and infinity inputs, and the
    /// threshold.
    #[test]
    fn cdl_cases_are_upstreams() {
        let data_1 = (
            json!([1.35, 1.10, 0.71]),
            json!([0.05, -0.23, 0.11]),
            json!([0.93, 0.81, 1.27]),
            None,
        );
        let data_2 = (
            json!([1.15, 1.10, 0.90]),
            json!([0.05, -0.02, 0.07]),
            json!([1.20, 0.95, 1.13]),
            Some(json!(0.9)),
        );
        let data_3 = (
            json!([3.405, 1.0, 1.0]),
            json!([-0.178, -0.178, -0.178]),
            json!([1.095, 1.095, 1.0]),
            Some(json!(1.2)),
        );
        let spec = |style: Option<&str>, dir: &str, data: &(Value, Value, Value, Option<Value>)| {
            let mut calls = Vec::new();
            if let Some(style) = style {
                calls.push(json!(["setStyle", {"enum": style}]));
            }
            calls.push(json!(["setDirection", {"enum": dir}]));
            calls.push(json!(["setSlope", data.0]));
            calls.push(json!(["setOffset", data.1]));
            calls.push(json!(["setPower", data.2]));
            if let Some(sat) = &data.3 {
                calls.push(json!(["setSat", sat]));
            }
            json!({"class": "CDLTransform", "calls": calls})
        };
        let (fwd, inv) = ("TRANSFORM_DIR_FORWARD", "TRANSFORM_DIR_INVERSE");
        let (asc, nc) = (Some("CDL_ASC"), Some("CDL_NO_CLAMP"));
        // (name, spec, version, legacy, wide range, NaN, infinity, threshold)
        type Expected = (&'static str, Value, u32, bool, bool, bool, bool, f32);
        let expected: Vec<Expected> = vec![
            (
                "clamp_fwd_v1_legacy_shader",
                spec(None, fwd, &data_1),
                1,
                true,
                true,
                false,
                true,
                1e-6,
            ),
            (
                "clamp_fwd_v1",
                spec(None, fwd, &data_1),
                1,
                false,
                true,
                false,
                true,
                1e-6,
            ),
            (
                "clamp_fwd_v2",
                spec(asc, fwd, &data_1),
                2,
                false,
                true,
                true,
                true,
                1e-5,
            ),
            (
                "clamp_fwd_no_clamp_v2",
                spec(nc, fwd, &data_1),
                2,
                false,
                true,
                false,
                false,
                5e-5,
            ),
            (
                "clamp_inv_v2",
                spec(asc, inv, &data_1),
                2,
                false,
                true,
                true,
                true,
                1e-4,
            ),
            (
                "clamp_inv_no_clamp_v2",
                spec(nc, inv, &data_1),
                2,
                false,
                true,
                false,
                false,
                1e-4,
            ),
            (
                "clamp_fwd_v1_legacy_shader_Data_2",
                spec(None, fwd, &data_2),
                1,
                true,
                true,
                false,
                false,
                1e-6,
            ),
            (
                "clamp_fwd_v1_Data_2",
                spec(None, fwd, &data_2),
                1,
                false,
                true,
                false,
                false,
                1e-6,
            ),
            (
                "clamp_fwd_v2_Data_2",
                spec(asc, fwd, &data_2),
                2,
                false,
                true,
                true,
                true,
                2e-5,
            ),
            (
                "clamp_inv_v2_Data_2",
                spec(asc, inv, &data_2),
                2,
                false,
                true,
                true,
                true,
                2e-5,
            ),
            (
                "clamp_fwd_no_clamp_v2_Data_2",
                spec(nc, fwd, &data_2),
                2,
                false,
                true,
                false,
                false,
                5e-5,
            ),
            (
                "clamp_inv_no_clamp_v2_Data_2",
                spec(nc, inv, &data_2),
                2,
                false,
                true,
                false,
                false,
                5e-5,
            ),
            (
                "clamp_fwd_v2_Data_3",
                spec(asc, fwd, &data_3),
                2,
                false,
                true,
                true,
                true,
                5e-5,
            ),
            (
                "clamp_fwd_no_clamp_v2_Data_3",
                spec(nc, fwd, &data_3),
                2,
                false,
                false,
                false,
                false,
                5e-5,
            ),
            (
                "clamp_inv_no_clamp_v2_Data_3",
                spec(nc, inv, &data_3),
                2,
                false,
                false,
                false,
                false,
                5e-5,
            ),
        ];
        let cases = cdl_cases();
        assert_eq!(cases.len(), expected.len());
        for (case, (name, spec, version, legacy, wide, nan, inf, threshold)) in
            cases.iter().zip(expected)
        {
            assert_eq!(case.name, name);
            assert_eq!(case.transform(), spec, "{name}");
            assert_eq!(case.version, version, "{name}");
            assert_eq!(case.legacy_shader, legacy, "{name}");
            assert_eq!(
                (case.test_wide_range, case.test_nan, case.test_infinity),
                (wide, nan, inf),
                "{name}"
            );
            assert_eq!(
                case.error_threshold.to_bits(),
                threshold.to_bits(),
                "{name}"
            );
        }
    }

    /// Each of upstream's 22 tests, in the file's order, builds the spec its body builds
    /// (tests/gpu/LogOp_test.cpp:21-353 @ v2.5.2): the transform's class, the break the
    /// camera constructor takes, the direction, the base when it sets one (`base10`, or
    /// `eulerConstant`: `expf(1.0f)`), and the parameters it sets; the legacy shader, the
    /// relative comparison, NaN and infinity inputs, and the threshold (the SSE2 values of
    /// `g_epsilon`, 1e-4, and `g_epsilon_inverse`, 1e-3).
    #[test]
    fn log_cases_are_upstreams() {
        let euler = f64::from(std::hint::black_box(1.0f32).exp());
        let base10 = f64::from(10.0f32);
        let (fwd, inv) = ("TRANSFORM_DIR_FORWARD", "TRANSFORM_DIR_INVERSE");
        let dir = |d: &str| json!(["setDirection", {"enum": d}]);
        let log = |d: &str, base: f64| json!({"class": "LogTransform", "calls": [dir(d), ["setBase", base]]});
        let affine = |calls: Vec<Value>| json!({"class": "LogAffineTransform", "calls": calls});
        let set = |setter: &str, v: [f64; 3]| json!([setter, v.to_vec()]);
        let (ls, lo, ns, no) = (
            "setLogSideSlopeValue",
            "setLogSideOffsetValue",
            "setLinSideSlopeValue",
            "setLinSideOffsetValue",
        );
        let camera = |brk: [f64; 3], calls: Vec<Value>| {
            json!({"class": "LogCameraTransform", "args": {"linSideBreak": brk.to_vec()},
                "calls": calls})
        };
        // (group, name, spec, legacy, relative, NaN, infinity, threshold)
        type Expected = (
            &'static str,
            &'static str,
            Value,
            bool,
            bool,
            bool,
            bool,
            f32,
        );
        let (lt, at, ct) = ("LogTransform", "LogAffineTransform", "LogCameraTransform");
        let expected: Vec<Expected> = vec![
            (
                lt,
                "LogBase_10_legacy",
                log(fwd, base10),
                true,
                false,
                false,
                false,
                1e-4,
            ),
            (
                lt,
                "LogBase_10_legacy_inverse",
                log(inv, base10),
                true,
                false,
                false,
                false,
                1e-3,
            ),
            (
                lt,
                "LogBase_10_generic_shader",
                log(fwd, base10),
                false,
                false,
                false,
                false,
                1e-4,
            ),
            (
                lt,
                "LogBase_10_inverse_generic_shader",
                log(inv, base10),
                false,
                false,
                false,
                false,
                1e-3,
            ),
            (
                lt,
                "LogBase_euler_legacy",
                log(fwd, euler),
                true,
                false,
                false,
                false,
                1e-4,
            ),
            (
                lt,
                "LogBase_euler_legacy_inverse",
                log(inv, euler),
                true,
                false,
                false,
                false,
                1e-3,
            ),
            (
                lt,
                "LogBase_euler_generic_shader",
                log(fwd, euler),
                false,
                false,
                false,
                false,
                1e-4,
            ),
            (
                lt,
                "LogBase_euler_inverse_generic_shader",
                log(inv, euler),
                false,
                false,
                false,
                false,
                1e-3,
            ),
            (
                at,
                "base",
                affine(vec![dir(fwd), json!(["setBase", base10])]),
                false,
                false,
                true,
                false,
                1e-4,
            ),
            (
                at,
                "base_inverse",
                affine(vec![dir(inv), json!(["setBase", base10])]),
                false,
                true,
                false,
                false,
                1e-4,
            ),
            (
                at,
                "linSideSlope",
                affine(vec![dir(fwd), set(ns, [2.0, 0.5, 3.0])]),
                false,
                false,
                true,
                false,
                1e-4,
            ),
            (
                at,
                "linSideSlope_inverse",
                affine(vec![dir(inv), set(ns, [2.0, 0.5, 3.0])]),
                false,
                false,
                false,
                false,
                1e-4,
            ),
            (
                at,
                "linSideOffset",
                affine(vec![dir(fwd), set(no, [0.1, 0.2, 0.3])]),
                false,
                false,
                true,
                false,
                1e-4,
            ),
            (
                at,
                "linSideOffset_inverse",
                affine(vec![dir(inv), set(no, [0.1, 0.2, 0.3])]),
                false,
                false,
                false,
                false,
                1e-4,
            ),
            (
                at,
                "logSideSlope",
                affine(vec![dir(fwd), set(ls, [2.0, 0.5, 3.0])]),
                false,
                false,
                true,
                false,
                1e-4 * 5.0,
            ),
            (
                at,
                "logSideSlope_inverse",
                affine(vec![dir(inv), set(ls, [2.0, 0.5, 3.0])]),
                false,
                false,
                false,
                false,
                1e-4,
            ),
            (
                at,
                "logSideOffset",
                affine(vec![dir(fwd), set(lo, [0.1, 0.2, 0.3])]),
                false,
                false,
                true,
                false,
                1e-4,
            ),
            (
                at,
                "logSideOffset_inverse",
                affine(vec![dir(inv), set(lo, [0.1, 0.2, 0.3])]),
                false,
                false,
                false,
                false,
                1e-4,
            ),
            (
                at,
                "lin2log",
                affine(vec![
                    dir(fwd),
                    set(ls, [0.2, 0.4, 0.25]),
                    set(lo, [0.14, 0.13, 0.12]),
                    set(ns, [1.5, 1.8, 1.2]),
                    set(no, [0.05, 0.1, 0.15]),
                ]),
                false,
                false,
                true,
                false,
                1e-4 * 5.0,
            ),
            (
                at,
                "log2lin",
                affine(vec![
                    dir(inv),
                    set(ls, [0.21, 0.2, 0.19]),
                    set(lo, [0.61, 0.6, 0.59]),
                    set(ns, [1.11, 1.1, 1.12]),
                    set(no, [0.051, 0.05, 0.052]),
                ]),
                false,
                false,
                true,
                false,
                1e-3,
            ),
            (
                ct,
                "camera_lin2log",
                camera(
                    [0.12, 0.13, 0.15],
                    vec![
                        dir(fwd),
                        set(ls, [0.2, 0.3, 0.4]),
                        set(lo, [0.7, 0.6, 0.5]),
                        set(ns, [1.4, 1.1, 1.2]),
                        set(no, [0.15, 0.16, 0.25]),
                        set("setLinearSlopeValue", [1.22, 1.33, 1.44]),
                    ],
                ),
                false,
                false,
                true,
                false,
                1e-4,
            ),
            (
                ct,
                "camera_log2lin",
                camera(
                    [0.12, 0.13, 0.14],
                    vec![
                        dir(inv),
                        set(ls, [0.21, 0.22, 0.23]),
                        set(lo, [0.6, 0.7, 0.8]),
                        set(ns, [1.1, 1.2, 1.3]),
                        set(no, [0.051, 0.052, 0.053]),
                        set("setLinearSlopeValue", [1.25, 1.23, 1.22]),
                    ],
                ),
                false,
                false,
                true,
                false,
                1e-3,
            ),
        ];
        let cases = log_cases();
        assert_eq!(cases.len(), expected.len());
        for (case, (group, name, spec, legacy, relative, nan, inf, threshold)) in
            cases.iter().zip(expected)
        {
            assert_eq!((case.group, case.name), (group, name));
            assert_eq!(case.transform(), spec, "{name}");
            assert_eq!(
                (case.legacy_shader, case.relative_comparison),
                (legacy, relative),
                "{name}"
            );
            assert_eq!((case.test_nan, case.test_infinity), (nan, inf), "{name}");
            assert_eq!(
                case.error_threshold.to_bits(),
                threshold.to_bits(),
                "{name}"
            );
        }
    }
}
