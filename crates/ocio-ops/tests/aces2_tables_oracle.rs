// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! ACES 2.0's parameters and tables against the wheel, bit for bit, before the renderers
//! exist: the GPU shader of an `ACES_OUTPUT_TRANSFORM_20` (the oracle's `gpu_shader`) holds
//! them, built by the same `init_*` functions as the CPU renderer
//! (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpGPU.cpp:1287-1366,
//! FixedFunctionOpCPU.cpp:1057-1088 @ v2.5.2). The JMh models' matrices, `cz`, `A_w_J` and
//! `1/cz` are literals in it, which the shader writes with 9 significant digits
//! (`getFloatString`, src/OpenColorIO/GpuShaderUtils.cpp:21-36 @ v2.5.2), enough to give back
//! every `float` exactly.
//!
//! The limiting primaries go through `float`, as the renderers read them.

use ocio_ops::ops::fixedfunction::aces2::common::JMhParams;
use ocio_ops::ops::fixedfunction::aces2::transform::init_jmh_params;
use ocio_ops::transforms::builtins::color_matrix_helpers::{Chromaticities, Primaries, aces_ap0};
use ocio_testkit::gpu::{GpuShaderReply, GpuShaderRequest, ShaderSettings};
use ocio_testkit::oracle::Oracle;
use serde_json::json;

/// The port's model for an output transform of `params` (peak luminance, then the limiting
/// primaries and white).
struct Model {
    p_in: JMhParams,
    p_out: JMhParams,
}

/// The parameters as `Renderer_ACES_OutputTransform20` builds them
/// (FixedFunctionOpCPU.cpp:1057-1088 @ v2.5.2).
fn model(params: &[f64; 9]) -> Model {
    let f = |i: usize| f64::from(params[i] as f32);
    let lim = Primaries::new(
        Chromaticities::new(f(1), f(2)),
        Chromaticities::new(f(3), f(4)),
        Chromaticities::new(f(5), f(6)),
        Chromaticities::new(f(7), f(8)),
    );
    let p_in = init_jmh_params(&aces_ap0::PRIMARIES).unwrap();
    let p_out = init_jmh_params(&lim).unwrap();
    Model { p_in, p_out }
}

/// The text after `prefix`, which must occur once at least.
fn after<'a>(text: &'a str, prefix: &str) -> &'a str {
    let at = text
        .find(prefix)
        .unwrap_or_else(|| panic!("no {prefix:?} in the shader"));
    &text[at + prefix.len()..]
}

/// The comma-separated numbers after `prefix`, up to the closing parenthesis.
fn numbers_after(text: &str, prefix: &str) -> Vec<f32> {
    let rest = after(text, prefix);
    let end = rest.find(')').expect("a closing parenthesis");
    rest[..end]
        .split(',')
        .map(|v| {
            let v = v.trim();
            v.parse::<f32>()
                .unwrap_or_else(|_| panic!("not a number: {v:?} after {prefix:?}"))
        })
        .collect()
}

/// The number after `prefix`, up to the next delimiter.
fn number_after(text: &str, prefix: &str) -> f32 {
    let rest = after(text, prefix).trim_start();
    let end = rest.find([',', ')', ';', ' ']).expect("the number's end");
    rest[..end]
        .parse::<f32>()
        .unwrap_or_else(|_| panic!("not a number: {:?} after {prefix:?}", &rest[..end]))
}

/// A 3x3 matrix as the GLSL `mat3` constructor lists it: transposed
/// (`getMatrixValues<T, 3>(m, lang, true)`, GpuShaderUtils.cpp:221-237 @ v2.5.2).
fn transposed(m: &[f32; 9]) -> Vec<f32> {
    (0..9).map(|i| m[(i % 3) * 3 + i / 3]).collect()
}

fn bits(values: &[f32]) -> Vec<u32> {
    values.iter().map(|v| v.to_bits()).collect()
}

/// Every parameter the shader exposes, for an output transform of each case, against the
/// port's.
#[test]
fn tables_and_parameters_match_the_gpu_shader() {
    // Peak luminances and limiting primaries of the ACES 2.0 outputs (Rec.709, P3-D65,
    // Rec.2020, P3-DCI, P3-D60).
    let cases: [[f64; 9]; 6] = [
        [100.0, 0.64, 0.33, 0.30, 0.60, 0.15, 0.06, 0.3127, 0.3290],
        [
            1000.0, 0.680, 0.320, 0.265, 0.690, 0.150, 0.060, 0.3127, 0.3290,
        ],
        [
            4000.0, 0.708, 0.292, 0.170, 0.797, 0.131, 0.046, 0.3127, 0.3290,
        ],
        [
            108.0, 0.680, 0.320, 0.265, 0.690, 0.150, 0.060, 0.314, 0.351,
        ],
        [
            500.0, 0.708, 0.292, 0.170, 0.797, 0.131, 0.046, 0.3127, 0.3290,
        ],
        [
            48.0, 0.680, 0.320, 0.265, 0.690, 0.150, 0.060, 0.32168, 0.33767,
        ],
    ];
    let requests: Vec<GpuShaderRequest> = cases
        .iter()
        .map(|params| {
            GpuShaderRequest::new(
                json!({"transform": {"class": "FixedFunctionTransform", "args": {
                    "style": {"enum": "FIXED_FUNCTION_ACES_OUTPUT_TRANSFORM_20"},
                    "params": params,
                }}}),
                ShaderSettings::default(),
            )
        })
        .collect();
    let calls: Vec<_> = requests.iter().map(GpuShaderRequest::call).collect();
    let responses = Oracle::get().batch(&calls, true);

    let mut failures = Vec::new();
    for (params, response) in cases.iter().zip(responses) {
        let reply = GpuShaderReply::from_response(response.expect("the oracle answers"));
        let shader = reply.shader();
        let text = shader.text.as_str();
        let m = model(params);

        let mut check = |what: &str, wheel: Vec<f32>, port: Vec<f32>| {
            if bits(&wheel) != bits(&port) {
                let first = wheel
                    .iter()
                    .zip(&port)
                    .position(|(w, p)| w.to_bits() != p.to_bits());
                failures.push(format!(
                    "{params:?} {what}: {} vs {} values, first difference at {first:?}: wheel \
                     {:?}, port {:?}",
                    wheel.len(),
                    port.len(),
                    first.map(|i| wheel[i]),
                    first.map(|i| port[i])
                ));
            }
        };

        check(
            "RGB to CAM16 (input)",
            numbers_after(text, "vec3 lms = mat3("),
            transposed(&m.p_in.matrix_rgb_to_cam16_c),
        );
        check(
            "cone response to Aab (input)",
            numbers_after(text, "Aab = mat3("),
            transposed(&m.p_in.matrix_cone_response_to_aab),
        );
        check(
            "Aab to cone response (limit)",
            numbers_after(text, "vec3 rgb_a = mat3("),
            transposed(&m.p_out.matrix_aab_to_cone_response),
        );
        check(
            "CAM16 to RGB (limit)",
            numbers_after(text, "JMh.rgb = mat3("),
            transposed(&m.p_out.matrix_cam16_c_to_rgb),
        );
        check(
            "cz, A_w_J, inv_cz",
            vec![
                number_after(text, "pow(Aab.r, "),
                number_after(text, "float A = "),
                number_after(text, "Aab.r = pow(JMh.r * 0.00999999978, "),
            ],
            vec![m.p_in.cz, m.p_in.a_w_j, m.p_out.inv_cz],
        );
    }
    assert!(
        failures.is_empty(),
        "{} differences:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
