// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The color matrix helpers against the wheel, bit for bit, through the built-in transforms
//! whose ops are the helpers' matrices (src/OpenColorIO/transforms/builtins/ACES.cpp,
//! Displays.cpp, ArriCameras.cpp @ v2.5.2): the wheel's processor of each, from the oracle's
//! `processor_ops`, holds a MatrixTransform whose `getMatrix` is the helper's `MatrixArray`
//! (`CreateMatrixOp`), every double as its bits.

use ocio_ops::transforms::builtins::color_matrix_helpers::{
    AdaptationMethod, Chromaticities, Primaries, aces_ap0, aces_ap1, build_conversion_matrix,
    build_conversion_matrix_from_xyz_d65, build_conversion_matrix_to_xyz_d65, rec709, rec2020,
};
use ocio_testkit::oracle::{BatchCall, Oracle};
use serde_json::{Value, json};

/// The `getMatrix` of the MatrixTransforms of the wheel's processor of the built-in `style`,
/// as `f64` bits.
fn wheel_matrices(responses: &Value) -> Vec<Vec<u64>> {
    let children = responses["processor"]["group"]["children"]
        .as_array()
        .unwrap_or_else(|| panic!("no children in {responses}"));
    children
        .iter()
        .filter(|c| c["class"] == "MatrixTransform")
        .map(|c| {
            let m = &c["getters"]["getMatrix"];
            m.as_array()
                .unwrap_or_else(|| panic!("no getMatrix in {c}"))
                .iter()
                .map(|v| {
                    v.get("f64")
                        .and_then(Value::as_u64)
                        .unwrap_or_else(|| panic!("not an f64's bits: {v}"))
                })
                .collect()
        })
        .collect()
}

/// Each built-in's matrix, with the helper call it makes, against the wheel's. The four
/// helpers, both adaptations and none, and the four sets of primaries they use.
#[test]
fn helper_matrices_match_the_builtins() {
    let cases = [
        // ACES.cpp:578-587 @ v2.5.2.
        (
            "UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD",
            build_conversion_matrix_to_xyz_d65(&aces_ap0::PRIMARIES, AdaptationMethod::Bradford),
        ),
        // ACES.cpp:30-35, 589-597 @ v2.5.2.
        (
            "UTILITY - ACES-AP1_to_CIE-XYZ-D65_BFD",
            build_conversion_matrix_to_xyz_d65(&aces_ap1::PRIMARIES, AdaptationMethod::Bradford),
        ),
        // ACES.cpp:598-609 @ v2.5.2.
        (
            "UTILITY - ACES-AP1_to_LINEAR-REC709_BFD",
            build_conversion_matrix(
                &aces_ap1::PRIMARIES,
                &rec709::PRIMARIES,
                AdaptationMethod::Bradford,
            ),
        ),
        // ACES.cpp:620-635 @ v2.5.2.
        (
            "ACEScct_to_ACES2065-1",
            build_conversion_matrix(
                &aces_ap1::PRIMARIES,
                &aces_ap0::PRIMARIES,
                AdaptationMethod::None,
            ),
        ),
        // Displays.cpp:182-185 @ v2.5.2.
        (
            "DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.709",
            build_conversion_matrix_from_xyz_d65(&rec709::PRIMARIES, AdaptationMethod::None),
        ),
        // Displays.cpp:216-219 @ v2.5.2.
        (
            "DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.2020",
            build_conversion_matrix_from_xyz_d65(&rec2020::PRIMARIES, AdaptationMethod::None),
        ),
        // ArriCameras.cpp:28-36, 94-101 @ v2.5.2: CAT02, after a Log op.
        (
            "ARRI_LOGC4_to_ACES2065-1",
            build_conversion_matrix(
                &ARRI_WIDE_GAMUT_4,
                &aces_ap0::PRIMARIES,
                AdaptationMethod::Cat02,
            ),
        ),
    ];
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(style, _)| BatchCall {
            cmd: "processor_ops",
            args: json!({
                "transform": {"class": "BuiltinTransform", "args": {"style": style}},
                "optimization": "OPTIMIZATION_NONE",
            }),
            blobs: vec![],
        })
        .collect();
    let responses = Oracle::get().batch(&calls, true);
    let mut failures = Vec::new();
    for ((style, port), response) in cases.iter().zip(responses) {
        let response = response.unwrap_or_else(|e| panic!("{style}: the oracle failed: {e}"));
        let wheel = wheel_matrices(&response.result);
        let port: Vec<u64> = port
            .as_ref()
            .unwrap_or_else(|e| panic!("{style}: {}", e.message()))
            .get_values()
            .iter()
            .map(|v| v.to_bits())
            .collect();
        if wheel.len() != 1 || wheel[0] != port {
            failures.push(format!(
                "{style}:\n  wheel {:?}\n  port  {:?}",
                wheel
                    .iter()
                    .map(|m| m.iter().map(|&b| f64::from_bits(b)).collect::<Vec<_>>())
                    .collect::<Vec<_>>(),
                port.iter().map(|&b| f64::from_bits(b)).collect::<Vec<_>>()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} matrices differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

/// `ARRI_WIDE_GAMUT_4::primaries` (src/OpenColorIO/transforms/builtins/ArriCameras.cpp:28-36
/// @ v2.5.2).
const ARRI_WIDE_GAMUT_4: Primaries = Primaries::new(
    Chromaticities::new(0.73470, 0.26530),
    Chromaticities::new(0.14240, 0.85760),
    Chromaticities::new(0.09910, -0.03080),
    Chromaticities::new(0.31270, 0.32900),
);
