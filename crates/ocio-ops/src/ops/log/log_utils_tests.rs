// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/ops/log/LogUtils_tests.cpp` @ v2.5.2, and a test of the style names.

use ocio_testkit::upstream::{check_close, check_throw_what};

use super::*;
use crate::ops::log::log_op_data::LogOpData;

/// Sets one channel's legacy CTF parameters: gamma, refWhite, refBlack, highlight, shadow.
fn set(params: &mut CtfParams, c: CtfChannel, values: [f64; 5]) {
    let p = params.get_mut(c);
    p[ctf_values::GAMMA] = values[0];
    p[ctf_values::REF_WHITE] = values[1];
    p[ctf_values::REF_BLACK] = values[2];
    p[ctf_values::HIGHLIGHT] = values[3];
    p[ctf_values::SHADOW] = values[4];
}

/// Port of `OCIO_ADD_TEST(LogUtil, ctf_to_ocio_fail)` @ v2.5.2.
#[test]
fn ctf_to_ocio_fail() {
    let mut ctf_params = CtfParams {
        style: LogStyle::LogToLin,
        ..CtfParams::default()
    };

    let (mut params_r, mut params_g, mut params_b) = (Params::new(), Params::new(), Params::new());
    let mut base = 1.0;

    // invalid gamma
    set(
        &mut ctf_params,
        CtfChannel::Red,
        [0.005, 375., 140., 0.8, 0.5],
    );
    let red = ctf_params.get(CtfChannel::Red).clone();
    *ctf_params.get_mut(CtfChannel::Green) = red.clone();
    *ctf_params.get_mut(CtfChannel::Blue) = red;

    check_throw_what(
        convert_log_parameters(
            &ctf_params,
            &mut base,
            &mut params_r,
            &mut params_g,
            &mut params_b,
        ),
        "gamma should be greater than 0.01",
    );

    // invalid refWhite and refBlack
    set(
        &mut ctf_params,
        CtfChannel::Red,
        [0.9, 375., 375., 0.8, 0.5],
    );

    check_throw_what(
        convert_log_parameters(
            &ctf_params,
            &mut base,
            &mut params_r,
            &mut params_g,
            &mut params_b,
        ),
        "refWhite should be greater than refBlack",
    );

    // invalid highlight and shadow
    set(
        &mut ctf_params,
        CtfChannel::Red,
        [0.9, 375., 140., 0.5, 0.5],
    );

    check_throw_what(
        convert_log_parameters(
            &ctf_params,
            &mut base,
            &mut params_r,
            &mut params_g,
            &mut params_b,
        ),
        "highlight should be greater than shadow",
    );
}

/// Port of `OCIO_ADD_TEST(LogUtil, ctf_to_ocio_ok)` @ v2.5.2.
#[test]
fn ctf_to_ocio_ok() {
    let mut ctf_params = CtfParams {
        style: LogStyle::Log10,
        ..CtfParams::default()
    };

    let (mut params_r, mut params_g, mut params_b) = (Params::new(), Params::new(), Params::new());
    let mut base = 1.0;

    for (style, expected_base, expected_dir) in [
        (LogStyle::Log10, 10., TransformDirection::Forward),
        (LogStyle::Log2, 2., TransformDirection::Forward),
        (LogStyle::AntiLog10, 10., TransformDirection::Inverse),
        (LogStyle::AntiLog2, 2., TransformDirection::Inverse),
    ] {
        ctf_params.style = style;
        let dir = get_log_direction(ctf_params.style);
        convert_log_parameters(
            &ctf_params,
            &mut base,
            &mut params_r,
            &mut params_g,
            &mut params_b,
        )
        .unwrap();

        assert_eq!(base, expected_base);
        assert_eq!(params_r[LOG_SIDE_SLOPE], 1.);
        assert_eq!(params_r[LIN_SIDE_SLOPE], 1.);
        assert_eq!(params_r[LIN_SIDE_OFFSET], 0.);
        assert_eq!(params_r[LOG_SIDE_OFFSET], 0.);
        assert_eq!(dir, expected_dir);

        let log_op = LogOpData::from_channel_params(
            base,
            params_r.clone(),
            params_g.clone(),
            params_b.clone(),
            dir,
        )
        .unwrap();
        if style == LogStyle::Log10 {
            assert!(!log_op.is_identity());
            assert!(!log_op.has_channel_crosstalk());
        }
        log_op.validate().unwrap();
    }

    set(&mut ctf_params, CtfChannel::Red, [4.6, 758., 30., 0.7, 0.4]);
    set(
        &mut ctf_params,
        CtfChannel::Green,
        [2.6, 300., 42., 0.8, 0.1],
    );
    let red = ctf_params.get(CtfChannel::Red).clone();
    *ctf_params.get_mut(CtfChannel::Blue) = red;

    ctf_params.style = LogStyle::LinToLog;
    let dir = get_log_direction(ctf_params.style);
    convert_log_parameters(
        &ctf_params,
        &mut base,
        &mut params_r,
        &mut params_g,
        &mut params_b,
    )
    .unwrap();

    let tol = 1e-6;
    assert_eq!(base, 10.);
    check_close(params_r[LOG_SIDE_SLOPE], 2.2482893, tol);
    check_close(params_r[LIN_SIDE_SLOPE], 1.7250706, tol);
    check_close(params_r[LIN_SIDE_OFFSET], -0.2075494, tol);
    check_close(params_r[LOG_SIDE_OFFSET], 0.7409580, tol);

    check_close(params_g[LOG_SIDE_SLOPE], 1.2707722, tol);
    check_close(params_g[LIN_SIDE_SLOPE], 0.5240051, tol);
    check_close(params_g[LIN_SIDE_OFFSET], 0.5807959, tol);
    check_close(params_g[LOG_SIDE_OFFSET], 0.2932551, tol);

    assert_eq!(dir, TransformDirection::Forward);

    let log_op5 = LogOpData::from_channel_params(
        base,
        params_r.clone(),
        params_g.clone(),
        params_b.clone(),
        dir,
    )
    .unwrap();
    log_op5.validate().unwrap();

    ctf_params.style = LogStyle::LogToLin;
    let dir = get_log_direction(ctf_params.style);
    convert_log_parameters(
        &ctf_params,
        &mut base,
        &mut params_r,
        &mut params_g,
        &mut params_b,
    )
    .unwrap();
    assert_eq!(dir, TransformDirection::Inverse);

    let log_op6 = LogOpData::from_channel_params(base, params_r, params_g, params_b, dir).unwrap();
    log_op6.validate().unwrap();
}

/// Each style's name gives it back, ignoring ASCII case; none or an empty one is missing.
#[test]
fn style_names_round_trip() {
    for style in [
        LogStyle::Log10,
        LogStyle::Log2,
        LogStyle::AntiLog10,
        LogStyle::AntiLog2,
        LogStyle::LogToLin,
        LogStyle::LinToLog,
        LogStyle::CameraLogToLin,
        LogStyle::CameraLinToLog,
    ] {
        let name = convert_style_to_string(style);
        assert_eq!(convert_string_to_style(Some(name)).unwrap(), style);
        assert_eq!(
            convert_string_to_style(Some(&name.to_ascii_uppercase())).unwrap(),
            style
        );
    }
    assert_eq!(
        convert_string_to_style(None).unwrap_err().message(),
        "Missing Log style."
    );
    assert_eq!(
        convert_string_to_style(Some("")).unwrap_err().message(),
        "Missing Log style."
    );
}

/// An unknown style's message, as upstream's stream writes it over its own start
/// (docs/improvements.md, I-52). Hand-derived: no output of the wheel shows this message (its
/// only caller, the CTF reader, replaces it), so the expected texts follow the C++ standard: a
/// `std::stringstream` constructed with text and opened for output starts writing at the
/// beginning of that text ([stringbuf.cons]).
#[test]
fn unknown_style_message_overwrites_its_start() {
    let message = |name: &str| {
        convert_string_to_style(Some(name))
            .unwrap_err()
            .message()
            .to_string()
    };
    assert_eq!(message("foo"), "foo'.wn Log style: '");
    // 17 characters and the quote and period: 19 of the 20.
    assert_eq!(message("abcdefghijklmnopq"), "abcdefghijklmnopq'.'");
    // 18 and more replace it all.
    assert_eq!(message("abcdefghijklmnopqr"), "abcdefghijklmnopqr'.");
    assert_eq!(
        message("abcdefghijklmnopqrstuvwxyz"),
        "abcdefghijklmnopqrstuvwxyz'."
    );
}
