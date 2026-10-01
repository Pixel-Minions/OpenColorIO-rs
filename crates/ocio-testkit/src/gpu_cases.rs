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

#[cfg(test)]
mod tests {
    use super::*;

    /// The table has upstream's ten tests, and their specs build as `AddMatrixTest` does.
    #[test]
    fn matrix_cases_are_upstreams() {
        let cases = matrix_cases();
        assert_eq!(cases.len(), 10);
        assert_eq!(
            cases[8].transform(),
            json!({"class": "MatrixTransform", "calls": [
                ["setDirection", {"enum": "TRANSFORM_DIR_FORWARD"}],
                ["setMatrix", MATRIX.to_vec()],
                ["setOffset", [-0.0, -0.25, 0.25, 0.0]],
            ]})
        );
        assert_eq!(
            cases[6].transform(),
            json!({"class": "MatrixTransform", "calls": [
                ["setDirection", {"enum": "TRANSFORM_DIR_INVERSE"}],
                ["setOffset", [-0.5, 0.25, -0.25, 0.0]],
            ]})
        );
        assert_eq!(cases.iter().filter(|c| !c.legacy_shader).count(), 2);
    }
}
