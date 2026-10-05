// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Ported `tests/cpu/ops/fixedfunction/FixedFunctionOpData_tests.cpp` @ v2.5.2, and the port's
//! own check of the error that replaces upstream's read of a missing parameter (U-31).

use super::*;
use FixedFunctionOpStyle::*;
use ocio_testkit::upstream::check_throw_what;

/// Port of `OCIO_ADD_TEST(FixedFunctionOpData, aces_red_mod_style)` @ v2.5.2.
#[test]
fn aces_red_mod_style() {
    let mut func = FixedFunctionOpData::new(AcesRedMod03Fwd).unwrap();
    assert_eq!(func.style(), AcesRedMod03Fwd);
    assert_eq!(func.params().len(), 0);
    func.validate().unwrap();
    let cache_id = func.get_cache_id();

    func.set_style(AcesRedMod10Fwd);
    assert_eq!(func.style(), AcesRedMod10Fwd);
    func.validate().unwrap();

    let mut cache_id_updated = func.get_cache_id();
    assert!(cache_id != cache_id_updated);

    let inv = func.inverse().unwrap();
    assert_eq!(inv.style(), AcesRedMod10Inv);
    assert_eq!(inv.params().len(), 0);
    cache_id_updated = inv.get_cache_id();
    assert!(cache_id != cache_id_updated);

    let mut p = func.params().clone();
    p.push(1.);
    func.set_params(p);
    check_throw_what(
        func.validate(),
        "The style 'ACES_RedMod10 (Forward)' must have zero parameters but 1 found.",
    );
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOpData, aces_dark_to_dim10_style)` @ v2.5.2.
#[test]
fn aces_dark_to_dim10_style() {
    let mut func = FixedFunctionOpData::with_params(AcesDarkToDim10Fwd, Params::new()).unwrap();

    assert_eq!(func.style(), AcesDarkToDim10Fwd);
    assert_eq!(func.params().len(), 0);
    func.validate().unwrap();
    let cache_id = func.get_cache_id();

    let inv = func.inverse().unwrap();
    assert_eq!(inv.style(), AcesDarkToDim10Inv);
    assert_eq!(inv.params().len(), 0);
    let cache_id_updated = inv.get_cache_id();
    assert!(cache_id != cache_id_updated);

    let mut p = func.params().clone();
    p.push(1.);
    func.set_params(p);
    check_throw_what(
        func.validate(),
        "The style 'ACES_DarkToDim10 (Forward)' must have zero parameters but 1 found.",
    );
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOpData, aces_gamut_comp_13_style)` @ v2.5.2.
#[test]
fn aces_gamut_comp_13_style() {
    let params: Params = vec![1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2];
    let mut func = FixedFunctionOpData::with_params(AcesGamutComp13Fwd, params.clone()).unwrap();
    func.validate().unwrap();
    let cache_id = func.get_cache_id();
    assert!(*func.params() == params);

    let inv = func.inverse().unwrap();
    assert_eq!(inv.params()[0], func.params()[0]);
    assert_eq!(inv.style(), AcesGamutComp13Inv);
    let cache_id_updated = inv.get_cache_id();
    assert!(cache_id != cache_id_updated);

    assert!(func == func);
    assert!(!(func == inv));

    let mut test_params = params.clone();
    test_params.push(12.);
    func.set_params(test_params);
    check_throw_what(
        func.validate(),
        "The style 'ACES_GamutComp13 (Forward)' must have seven parameters but 8 found.",
    );

    let mut test_params = params.clone();
    test_params.pop();
    func.set_params(test_params);
    check_throw_what(
        func.validate(),
        "The style 'ACES_GamutComp13 (Forward)' must have seven parameters but 6 found.",
    );

    let mut test_params = params.clone();
    test_params.clear();
    func.set_params(test_params);
    check_throw_what(
        func.validate(),
        "The style 'ACES_GamutComp13 (Forward)' must have seven parameters but 0 found.",
    );

    // Each parameter just outside its range, at both ends.
    let cases: [(usize, f64, &str); 14] = [
        (
            0,
            1.0,
            "Parameter 1 (lim_cyan) is outside valid range [1.001,65504]",
        ),
        (
            0,
            65535.0,
            "Parameter 65535 (lim_cyan) is outside valid range [1.001,65504]",
        ),
        (
            1,
            1.0,
            "Parameter 1 (lim_magenta) is outside valid range [1.001,65504]",
        ),
        (
            1,
            65535.0,
            "Parameter 65535 (lim_magenta) is outside valid range [1.001,65504]",
        ),
        (
            2,
            1.0,
            "Parameter 1 (lim_yellow) is outside valid range [1.001,65504]",
        ),
        (
            2,
            65535.0,
            "Parameter 65535 (lim_yellow) is outside valid range [1.001,65504]",
        ),
        (
            3,
            -0.1,
            "Parameter -0.1 (thr_cyan) is outside valid range [0,0.9995]",
        ),
        (
            3,
            1.0,
            "Parameter 1 (thr_cyan) is outside valid range [0,0.9995]",
        ),
        (
            4,
            -0.1,
            "Parameter -0.1 (thr_magenta) is outside valid range [0,0.9995]",
        ),
        (
            4,
            1.0,
            "Parameter 1 (thr_magenta) is outside valid range [0,0.9995]",
        ),
        (
            5,
            -0.1,
            "Parameter -0.1 (thr_yellow) is outside valid range [0,0.9995]",
        ),
        (
            5,
            1.0,
            "Parameter 1 (thr_yellow) is outside valid range [0,0.9995]",
        ),
        (
            6,
            0.0,
            "Parameter 0 (power) is outside valid range [1,65504]",
        ),
        (
            6,
            65535.0,
            "Parameter 65535 (power) is outside valid range [1,65504]",
        ),
    ];
    for (i, value, what) in cases {
        let mut test_params = params.clone();
        test_params[i] = value;
        func.set_params(test_params);
        check_throw_what(func.validate(), what);
    }
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOpData, rec2100_surround_style)` @ v2.5.2.
#[test]
fn rec2100_surround_style() {
    let params: Params = vec![2.0];
    let mut func = FixedFunctionOpData::with_params(Rec2100SurroundFwd, params.clone()).unwrap();
    func.validate().unwrap();
    let cache_id = func.get_cache_id();
    assert!(*func.params() == params);

    let inv = func.inverse().unwrap();
    assert_eq!(inv.params()[0], func.params()[0]);
    assert_eq!(inv.style(), Rec2100SurroundInv);
    let cache_id_updated = inv.get_cache_id();
    assert!(cache_id != cache_id_updated);

    assert!(func == func);
    assert!(!(func == inv));

    let mut params = func.params().clone();
    params[0] = 120.;
    func.set_params(params);
    check_throw_what(
        func.validate(),
        "Parameter 120 is greater than upper bound 100",
    );

    let mut params = func.params().clone();
    params[0] = 0.00001;
    func.set_params(params);
    check_throw_what(
        func.validate(),
        "Parameter 1e-05 is less than lower bound 0.01",
    );

    let mut params = func.params().clone();
    params.push(12.);
    func.set_params(params);
    check_throw_what(
        func.validate(),
        "The style 'REC2100_Surround (Forward)' must have one parameter but 2 found.",
    );

    let mut params = func.params().clone();
    params.clear();
    func.set_params(params);
    check_throw_what(
        func.validate(),
        "The style 'REC2100_Surround (Forward)' must have one parameter but 0 found.",
    );
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOpData, aces_lin_to_doublelog_style)` @ v2.5.2.
#[test]
fn aces_lin_to_doublelog_style() {
    let params: Params = vec![
        10.0, 0.25, 0.5, -1.0, 0.0, -1.0, 1.25, 1.0, 1.0, 1.0, 0.5, 1.0, 0.0,
    ];

    let mut func = FixedFunctionOpData::with_params(LinToDoubleLog, params.clone()).unwrap();
    func.validate().unwrap();
    let cache_id = func.get_cache_id();
    assert!(*func.params() == params);

    let inv = func.inverse().unwrap();
    assert_eq!(inv.params()[0], func.params()[0]);
    assert_eq!(inv.style(), DoubleLogToLin);
    let cache_id_updated = inv.get_cache_id();
    assert!(cache_id != cache_id_updated);

    assert!(func == func);
    assert!(!(func == inv));

    let mut test_params = params.clone();
    test_params.push(12.);
    func.set_params(test_params);
    check_throw_what(
        func.validate(),
        "The style 'Lin_TO_DoubleLog' must have 13 parameters but 14 found.",
    );

    let mut test_params = params.clone();
    test_params.pop();
    func.set_params(test_params);
    check_throw_what(
        func.validate(),
        "The style 'Lin_TO_DoubleLog' must have 13 parameters but 12 found.",
    );

    let mut test_params = params.clone();
    test_params.clear();
    func.set_params(test_params);
    check_throw_what(
        func.validate(),
        "The style 'Lin_TO_DoubleLog' must have 13 parameters but 0 found.",
    );

    let mut test_params = params.clone();
    test_params[1] = 1.0;
    func.set_params(test_params);
    check_throw_what(
        func.validate(),
        "First break point 1 is larger than the second break point 0.5.",
    );
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOpData, aces_lin_to_gammalog_style)` @ v2.5.2.
#[test]
fn aces_lin_to_gammalog_style() {
    let params: Params = vec![
        0.0,
        0.25,
        0.5,
        1.0,
        0.0,
        2.718,
        0.17883277,
        0.807825590164,
        1.0,
        -0.07116723,
    ];

    let mut func = FixedFunctionOpData::with_params(LinToGammaLog, params.clone()).unwrap();
    func.validate().unwrap();
    let cache_id = func.get_cache_id();
    assert!(*func.params() == params);

    let inv = func.inverse().unwrap();
    assert_eq!(inv.params()[0], func.params()[0]);
    assert_eq!(inv.style(), GammaLogToLin);
    let cache_id_updated = inv.get_cache_id();
    assert!(cache_id != cache_id_updated);

    assert!(func == func);
    assert!(!(func == inv));

    let mut test_params = params.clone();
    test_params.push(12.);
    func.set_params(test_params);
    check_throw_what(
        func.validate(),
        "The style 'Lin_TO_GammaLog' must have 10 parameters but 11 found.",
    );

    let mut test_params = params.clone();
    test_params.pop();
    func.set_params(test_params);
    check_throw_what(
        func.validate(),
        "The style 'Lin_TO_GammaLog' must have 10 parameters but 9 found.",
    );

    let mut test_params = params.clone();
    test_params.clear();
    func.set_params(test_params);
    check_throw_what(
        func.validate(),
        "The style 'Lin_TO_GammaLog' must have 10 parameters but 0 found.",
    );

    let mut test_params = params.clone();
    test_params[0] = 1.0;
    func.set_params(test_params);
    check_throw_what(
        func.validate(),
        "Mirror point 1 is not smaller than the break point 0.25.",
    );
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOpData, is_inverse)` @ v2.5.2.
#[test]
fn is_inverse() {
    let params: Params = vec![2.0];
    let f_s = FixedFunctionOpData::with_params(Rec2100SurroundFwd, params.clone()).unwrap();
    let params_inv: Params = vec![0.5];
    let f_s_inv1 = FixedFunctionOpData::with_params(Rec2100SurroundFwd, params_inv).unwrap();
    let f_s_inv2 = FixedFunctionOpData::with_params(Rec2100SurroundInv, params).unwrap();

    assert!(f_s.is_inverse(&f_s_inv1).unwrap());
    assert!(f_s.is_inverse(&f_s_inv2).unwrap());

    assert!(!f_s.is_inverse(&f_s).unwrap());
    assert!(!f_s_inv1.is_inverse(&f_s_inv1).unwrap());
    assert!(!f_s_inv2.is_inverse(&f_s_inv2).unwrap());
    assert!(!f_s_inv1.is_inverse(&f_s_inv2).unwrap());

    let p0 = Params::new();
    let f_g = FixedFunctionOpData::with_params(AcesGlow03Fwd, p0.clone()).unwrap();
    let f_g_inv = FixedFunctionOpData::with_params(AcesGlow03Inv, p0.clone()).unwrap();
    assert!(f_g.is_inverse(&f_g_inv).unwrap());
    assert!(f_g_inv.is_inverse(&f_g).unwrap());
    assert!(!f_g.is_inverse(&f_g).unwrap());
    assert!(!f_g_inv.is_inverse(&f_g_inv).unwrap());
    assert!(!f_g.is_inverse(&f_s).unwrap());

    let f_r = FixedFunctionOpData::with_params(AcesRedMod03Fwd, p0.clone()).unwrap();
    let f_r_inv = FixedFunctionOpData::with_params(AcesRedMod03Inv, p0).unwrap();
    assert!(f_r.is_inverse(&f_r_inv).unwrap());
    assert!(f_r_inv.is_inverse(&f_r).unwrap());
    assert!(!f_r.is_inverse(&f_r).unwrap());
    assert!(!f_r_inv.is_inverse(&f_r_inv).unwrap());
    assert!(!f_r.is_inverse(&f_g).unwrap());

    let mut p7: Params = vec![1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2];
    let f_gm = FixedFunctionOpData::with_params(AcesGamutComp13Fwd, p7.clone()).unwrap();
    let f_gm_inv = FixedFunctionOpData::with_params(AcesGamutComp13Inv, p7.clone()).unwrap();
    assert!(f_gm.is_inverse(&f_gm_inv).unwrap());
    assert!(f_gm_inv.is_inverse(&f_gm).unwrap());
    assert!(!f_gm.is_inverse(&f_gm).unwrap());
    assert!(!f_gm_inv.is_inverse(&f_gm_inv).unwrap());
    assert!(!f_gm.is_inverse(&f_r).unwrap());

    p7[6] += 0.01;
    let f_gm_inv = FixedFunctionOpData::with_params(AcesGamutComp13Inv, p7).unwrap();
    assert!(!f_gm_inv.is_inverse(&f_gm).unwrap());
    assert!(!f_gm.is_inverse(&f_gm_inv).unwrap());
}

/// U-31: a Rec.2100 surround compared with another of its style reads both first parameters
/// upstream; the port refuses a missing one. Data of other styles, or an inverse that
/// validation refuses, give upstream's answers.
#[test]
fn is_inverse_of_short_params_is_refused() {
    let f_s = FixedFunctionOpData::with_params(Rec2100SurroundFwd, vec![2.0]).unwrap();
    let mut empty = f_s.clone();
    empty.set_params(Params::new());

    check_throw_what(f_s.is_inverse(&empty), SHORT_PARAMS);
    check_throw_what(empty.is_inverse(&f_s), SHORT_PARAMS);

    // A different style: upstream compares with the inverse, which validates first.
    let mut inv = empty.clone();
    inv.set_style(Rec2100SurroundInv);
    check_throw_what(
        empty.is_inverse(&FixedFunctionOpData::new(AcesGlow03Fwd).unwrap()),
        "The style 'REC2100_Surround (Forward)' must have one parameter but 0 found.",
    );
    assert!(!f_s.is_inverse(&inv).unwrap());
}
