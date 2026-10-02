// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/ops/log/LogOpData_tests.cpp` @ v2.5.2, and tests of the accessors.

use ocio_testkit::upstream::check_throw_what;

use super::*;
use crate::ops::log::log_utils::{
    CtfChannel, CtfParams, LogStyle, convert_log_parameters, ctf_values, get_log_direction,
};
use TransformDirection::{Forward, Inverse};

/// Sets one channel's legacy CTF parameters: gamma, refWhite, refBlack, highlight, shadow.
fn set(params: &mut CtfParams, c: CtfChannel, values: [f64; 5]) {
    let p = params.get_mut(c);
    p[ctf_values::GAMMA] = values[0];
    p[ctf_values::REF_WHITE] = values[1];
    p[ctf_values::REF_BLACK] = values[2];
    p[ctf_values::HIGHLIGHT] = values[3];
    p[ctf_values::SHADOW] = values[4];
}

/// The log op data of `ctf_params`, as upstream's tests build it: `GetLogDirection`,
/// `ConvertLogParameters`, then `LogOpData(base, r, g, b, dir)`; with the base and the
/// parameters.
fn from_ctf(ctf_params: &CtfParams) -> (LogOpData, f64, [Params; 3]) {
    let mut base = 1.0;
    let (mut r, mut g, mut b) = (Params::new(), Params::new(), Params::new());
    let dir = get_log_direction(ctf_params.style);
    convert_log_parameters(ctf_params, &mut base, &mut r, &mut g, &mut b).unwrap();
    let log = LogOpData::from_channel_params(base, r.clone(), g.clone(), b.clone(), dir).unwrap();
    (log, base, [r, g, b])
}

/// Port of `OCIO_ADD_TEST(LogOpData, accessor_test)` @ v2.5.2.
#[test]
fn accessor_test() {
    let mut ctf_params = CtfParams::default();
    set(
        &mut ctf_params,
        CtfChannel::Red,
        [2.4, 410., 256., 0.2, 0.1],
    );
    set(
        &mut ctf_params,
        CtfChannel::Green,
        [3.5, 620., 485., 0.7, 0.6],
    );
    set(
        &mut ctf_params,
        CtfChannel::Blue,
        [4.6, 730., 558., 0.9, 0.7],
    );

    ctf_params.style = LogStyle::LogToLin;

    let (log_op, base, [params_r, params_g, params_b]) = from_ctf(&ctf_params);

    assert_eq!(log_op.get_type(), OpDataType::Log);

    assert!(!log_op.all_components_equal());
    assert_eq!(log_op.base(), base);
    assert!(*log_op.red_params() == params_r);
    assert!(*log_op.green_params() == params_g);
    assert!(*log_op.blue_params() == params_b);

    // Update all channels with same parameters.
    let red = ctf_params.get(CtfChannel::Red).clone();
    *ctf_params.get_mut(CtfChannel::Green) = red.clone();
    *ctf_params.get_mut(CtfChannel::Blue) = red;

    let (log_op2, _, [params_r, _, _]) = from_ctf(&ctf_params);

    assert!(log_op2.all_components_equal());
    assert!(*log_op2.red_params() == params_r);
    assert!(*log_op2.green_params() == params_r);
    assert!(*log_op2.blue_params() == params_r);

    // Update only red channel with new parameters.
    set(
        &mut ctf_params,
        CtfChannel::Red,
        [0.6, 358., 115., 0.7, 0.3],
    );

    let (log_op3, _, [params_r, params_g, params_b]) = from_ctf(&ctf_params);

    assert!(!log_op3.all_components_equal());
    assert!(*log_op3.red_params() == params_r);
    assert!(*log_op3.green_params() == params_g);
    assert!(*log_op3.blue_params() == params_b);

    // Update only green channel with new parameters.
    let green = ctf_params.get(CtfChannel::Green).clone();
    *ctf_params.get_mut(CtfChannel::Red) = green;
    set(
        &mut ctf_params,
        CtfChannel::Green,
        [0.3, 333., 155., 0.85, 0.111],
    );

    let (log_op4, _, [params_r, params_g, params_b]) = from_ctf(&ctf_params);
    assert!(!log_op4.all_components_equal());
    assert!(*log_op4.red_params() == params_r);
    assert!(*log_op4.green_params() == params_g);
    assert!(*log_op4.blue_params() == params_b);

    // Update only blue channel with new parameters.
    let red = ctf_params.get(CtfChannel::Red).clone();
    *ctf_params.get_mut(CtfChannel::Green) = red;
    set(
        &mut ctf_params,
        CtfChannel::Blue,
        [0.124, 55., 33., 0.27, 0.22],
    );

    let (log_op5, _, [params_r, params_g, params_b]) = from_ctf(&ctf_params);
    assert!(!log_op5.all_components_equal());
    assert!(*log_op5.red_params() == params_r);
    assert!(*log_op5.green_params() == params_g);
    assert!(*log_op5.blue_params() == params_b);

    // Initialize with base.
    let base_val = 2.0;
    let log_op6 = LogOpData::new(base_val, Forward);
    assert!(log_op6.all_components_equal());
    let param = log_op6.red_params();
    assert_eq!(log_op6.base(), base_val);
    assert_eq!(param[LOG_SIDE_SLOPE], f64::from(1.0f32));
    assert_eq!(param[LIN_SIDE_SLOPE], f64::from(1.0f32));
    assert_eq!(param[LIN_SIDE_OFFSET], f64::from(0.0f32));
    assert_eq!(param[LOG_SIDE_OFFSET], f64::from(0.0f32));

    // Initialize with OCIO parameters.
    let log_slope = [1.5, 1.6, 1.7];
    let lin_slope = [1.1, 1.2, 1.3];
    let lin_offset = [1.0, 2.0, 3.0];
    let log_offset = [10.0, 20.0, 30.0];

    let log_op7 = LogOpData::with_parameters(
        base,
        &log_slope,
        &log_offset,
        &lin_slope,
        &lin_offset,
        Forward,
    );
    assert!(!log_op7.all_components_equal());
    assert_eq!(log_op7.base(), base);
    for (c, params) in [
        log_op7.red_params(),
        log_op7.green_params(),
        log_op7.blue_params(),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(params[LOG_SIDE_SLOPE], log_slope[c]);
        assert_eq!(params[LIN_SIDE_SLOPE], lin_slope[c]);
        assert_eq!(params[LIN_SIDE_OFFSET], lin_offset[c]);
        assert_eq!(params[LOG_SIDE_OFFSET], log_offset[c]);
    }
}

/// Port of `OCIO_ADD_TEST(LogOpData, validation_fails_test)` @ v2.5.2.
#[test]
fn validation_fails_test() {
    let mut base = 1.0;
    let mut log_slope = [1.0, 1.0, 1.0];
    let mut lin_slope = [1.0, 1.0, 1.0];
    let lin_offset = [0.0, 0.0, 0.0];
    let log_offset = [0.0, 0.0, 0.0];

    // Fail invalid base.
    for direction in [Forward, Inverse] {
        let log_op1 = LogOpData::with_parameters(
            base,
            &log_slope,
            &log_offset,
            &lin_slope,
            &lin_offset,
            direction,
        );
        check_throw_what(log_op1.validate(), "base cannot be 1");
    }

    base = 10.0;

    // Fail invalid slope.
    lin_slope = [0.0; 3];
    for direction in [Forward, Inverse] {
        let log_op2 = LogOpData::with_parameters(
            base,
            &log_slope,
            &log_offset,
            &lin_slope,
            &lin_offset,
            direction,
        );
        check_throw_what(log_op2.validate(), "linear side slope cannot be 0");
    }

    lin_slope = [1.0; 3];

    // Fail invalid multiplier.
    log_slope = [0.0; 3];
    for direction in [Forward, Inverse] {
        let log_op3 = LogOpData::with_parameters(
            base,
            &log_slope,
            &log_offset,
            &lin_slope,
            &lin_offset,
            direction,
        );
        check_throw_what(log_op3.validate(), "log side slope cannot be 0");
    }
}

/// Port of `OCIO_ADD_TEST(LogOpData, log_inverse)` @ v2.5.2.
#[test]
fn log_inverse() {
    let param_r: Params = vec![1.5, 10.0, 1.1, 1.0];
    let param_g: Params = vec![1.6, 20.0, 1.2, 2.0];
    let param_b: Params = vec![1.7, 30.0, 1.3, 3.0];
    let base = 10.0;

    let log_op0 =
        LogOpData::from_channel_params(base, param_r.clone(), param_g, param_b, Forward).unwrap();
    let inv_log_op0 = log_op0.inverse().unwrap();

    assert!(log_op0.red_params() == inv_log_op0.red_params());
    assert!(log_op0.green_params() == inv_log_op0.green_params());
    assert!(log_op0.blue_params() == inv_log_op0.blue_params());

    // When components are not equals, ops are not considered inverse.
    assert!(!log_op0.is_inverse(&inv_log_op0));

    // Using equal components.
    let log_op1 =
        LogOpData::from_channel_params(base, param_r.clone(), param_r.clone(), param_r, Forward)
            .unwrap();
    let inv_log_op1 = log_op1.inverse().unwrap();

    assert!(log_op1.is_inverse(&inv_log_op1));
}

/// Port of `OCIO_ADD_TEST(LogOpData, identity_replacement)` @ v2.5.2.
#[test]
fn identity_replacement() {
    let params_r: Params = vec![1.5, 10.0, 2.0, 1.0];
    let base = 2.0;
    let with = |dir| {
        LogOpData::from_channel_params(
            base,
            params_r.clone(),
            params_r.clone(),
            params_r.clone(),
            dir,
        )
        .unwrap()
    };
    {
        let log_op = with(Inverse);
        assert_eq!(
            log_op.get_identity_replacement().unwrap().get_type(),
            OpDataType::Matrix
        );
    }
    {
        let log_op = with(Forward);
        let op = log_op.get_identity_replacement().unwrap();
        assert_eq!(op.get_type(), OpDataType::Range);
        let OpData::Range(r) = op else {
            unreachable!("a range");
        };
        // -(1.0/2.0)
        assert_eq!(r.get_min_in_value(), -0.5);
        assert!(r.max_is_empty());
    }

    {
        let log_op = LogOpData::new(f64::from(2.0f32), Forward);
        assert_eq!(
            log_op.get_identity_replacement().unwrap().get_type(),
            OpDataType::Range
        );
    }
    {
        let log_op = LogOpData::new(f64::from(2.0f32), Inverse);
        assert_eq!(
            log_op.get_identity_replacement().unwrap().get_type(),
            OpDataType::Matrix
        );
    }
}

/// The styles follow the parameters: plain logs, affine logs, and the camera style with its
/// fifth and sixth parameters.
#[test]
fn styles_follow_the_parameters() {
    let log2 = LogOpData::new(2.0, Forward);
    assert!(log2.is_simple_log() && log2.is_log2() && !log2.is_log10());
    assert!(!log2.is_camera());
    assert!(LogOpData::new(10.0, Inverse).is_log10());

    // Any non-default parameter makes it an affine log.
    let mut affine = LogOpData::new(2.0, Forward);
    let mut slope = affine.value(LogAffineParameter::LogSideSlope).unwrap();
    slope[1] = 0.5;
    affine
        .set_value(LogAffineParameter::LogSideSlope, &slope)
        .unwrap();
    assert!(!affine.is_simple_log() && !affine.is_log2());
    assert!(!affine.all_components_equal());

    // The break adds a fifth parameter, the linear slope a sixth.
    let mut camera = LogOpData::new(2.0, Forward);
    let brk = camera.value(LogAffineParameter::LinSideSlope).unwrap();
    assert!(
        camera
            .set_value(LogAffineParameter::LinearSlope, &brk)
            .is_err()
    );
    camera
        .set_value(LogAffineParameter::LinSideBreak, &brk)
        .unwrap();
    assert!(camera.is_camera() && !camera.is_log2());
    assert_eq!(camera.red_params().len(), 5);
    camera
        .set_value(LogAffineParameter::LinearSlope, &brk)
        .unwrap();
    assert_eq!(camera.red_params().len(), 6);
    camera.unset_linear_slope();
    assert_eq!(camera.red_params().len(), 5);
    assert_eq!(camera.value(LogAffineParameter::LinearSlope), None);
}

/// Channels with 4 or more parameters can't be mixed with ones with fewer.
#[test]
fn channels_need_the_same_style() {
    let four = LogOpData::new(2.0, Forward).red_params().clone();
    let three = four[..3].to_vec();
    let err = LogOpData::from_channel_params(2.0, four.clone(), three, four.clone(), Forward)
        .unwrap_err();
    assert_eq!(
        err.message(),
        "Cannot create Log op, all channels need to have the same style."
    );
    let a = LogOpData::from_channel_params(2.0, four.clone(), four.clone(), four, Forward);
    let b = LogOpData::new(2.0, Inverse);
    assert!(a.unwrap().is_inverse(&b));
}

/// Channels with fewer than 4 parameters, or different numbers of them (docs/improvements.md,
/// U-20): where upstream reads or writes past a channel's parameters, the port returns an
/// error; validation refuses both.
#[test]
fn short_channels_are_errors() {
    let message =
        "Log: the channels have fewer parameters than this needs: upstream accesses past them.";
    let three: Params = vec![1.0, 0.0, 1.0];
    let mut log =
        LogOpData::from_channel_params(2.0, three.clone(), three.clone(), three, Forward).unwrap();
    assert_eq!(
        log.validate().unwrap_err().message(),
        "Log: expecting at least 4 parameters."
    );
    let values = [0.5, 0.5, 0.5];
    assert_eq!(
        log.set_value(LogAffineParameter::LinSideOffset, &values)
            .unwrap_err()
            .message(),
        message
    );
    assert_eq!(
        log.get_identity_replacement().unwrap_err().message(),
        message
    );
    // An index the channels have can be set, and the arrays of the others stay as they are.
    log.set_value(LogAffineParameter::LinSideSlope, &values)
        .unwrap();
    let mut out = [[7.0; 3]; 4];
    let [a, b, c, d] = &mut out;
    log.get_parameters(a, b, c, d);
    assert_eq!(out, [[1.0; 3], [0.0; 3], values, [7.0; 3]]);

    // A green channel shorter than the red one.
    let mut log = LogOpData::new(2.0, Forward);
    let mut red = log.red_params().clone();
    red[LOG_SIDE_SLOPE] = 2.0;
    log.set_red_params(red);
    log.set_value(LogAffineParameter::LinSideBreak, &values)
        .unwrap();
    log.set_green_params(vec![1.0, 0.0, 1.0, 0.0]);
    assert_eq!(
        log.validate().unwrap_err().message(),
        "Log: Red, green & blue parameters must have the same size."
    );
    assert_eq!(log.get_lin_break_string(7).unwrap_err().message(), message);
    assert_eq!(
        log.set_value(LogAffineParameter::LinSideBreak, &values)
            .unwrap_err()
            .message(),
        message
    );
    // Setting the linear slope grows every channel to 6.
    log.set_value(LogAffineParameter::LinearSlope, &values)
        .unwrap();
    assert_eq!(log.green_params().len(), 6);
    assert_eq!(log.green_params()[LINEAR_SLOPE], values[1]);
    log.unset_linear_slope();
    // The red channel's parameter strings past its own are upstream's error.
    assert_eq!(
        log.get_linear_slope_string(7).unwrap_err().message(),
        "Log: accessing parameter that does not exist."
    );
}
