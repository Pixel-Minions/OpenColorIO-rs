// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Ported `tests/cpu/ops/gamma/GammaOpData_tests.cpp` @ v2.5.2, and the port's own checks of
//! the style conversions and of the errors that replace upstream's reads past short parameters
//! (U-24).

use super::*;
use GammaStyle::*;
use TransformDirection::{Forward, Inverse};

/// `OCIO_CHECK_THROW_WHAT`: the call fails, and its message contains `what`.
#[track_caller]
fn check_throw_what<T: std::fmt::Debug>(result: Result<T>, what: &str) {
    let err = result.expect_err("an exception");
    assert!(
        err.message().contains(what),
        "{:?} doesn't contain {what:?}",
        err.message()
    );
}

/// Port of `OCIO_ADD_TEST(GammaOpData, accessors)` @ v2.5.2.
#[test]
fn accessors() {
    let params_r: Params = vec![2.4, 0.1];
    let params_g: Params = vec![2.2, 0.2];
    let params_b: Params = vec![2.0, 0.4];
    let params_a: Params = vec![1.8, 0.6];

    let mut g1 = GammaOpData::new(
        MoncurveFwd,
        params_r.clone(),
        params_g.clone(),
        params_b.clone(),
        params_a.clone(),
    );

    assert_eq!(g1.get_type(), OpDataType::Gamma);

    assert!(*g1.red_params() == params_r);
    assert!(*g1.green_params() == params_g);
    assert!(*g1.blue_params() == params_b);
    assert!(*g1.alpha_params() == params_a);

    assert_eq!(g1.style(), MoncurveFwd);

    assert!(!g1.are_all_components_equal());
    assert!(!g1.is_non_channel_dependent());
    assert!(!g1.is_alpha_component_identity());

    // Set R, G and B params to paramsR, A set to identity.
    g1.set_params(&params_r);

    assert!(!g1.are_all_components_equal());
    assert!(g1.is_non_channel_dependent());
    assert!(g1.is_alpha_component_identity());

    assert!(*g1.green_params() == params_r);
    assert!(GammaOpData::is_identity_parameters(
        g1.alpha_params(),
        g1.style()
    ));

    g1.set_alpha_params(params_r.clone());
    assert!(g1.are_all_components_equal());

    g1.set_blue_params(params_b.clone());
    assert!(*g1.blue_params() == params_b);

    assert!(!g1.are_all_components_equal());

    g1.set_red_params(params_b.clone());
    assert!(*g1.red_params() == params_b);

    g1.set_green_params(params_b.clone());
    assert!(*g1.green_params() == params_b);

    g1.set_alpha_params(params_a.clone());
    assert!(*g1.alpha_params() == params_a);

    g1.set_style(MoncurveRev);
    assert_eq!(g1.style(), MoncurveRev);
}

/// Port of `OCIO_ADD_TEST(GammaOpData, identity_style_basic)` @ v2.5.2.
#[test]
fn identity_style_basic() {
    let identity_params = GammaOpData::identity_parameters(BasicFwd);

    {
        //
        // Basic identity gamma.
        //
        let g = GammaOpData::new(
            BasicFwd,
            identity_params.clone(),
            identity_params.clone(),
            identity_params.clone(),
            identity_params.clone(),
        );
        assert!(g.is_identity().unwrap());
        assert!(!g.is_no_op().unwrap()); // inBitDepth != outBitDepth
        assert!(g.is_channel_independent());
    }

    {
        //
        // Default constructor test:
        // gamma op is BASIC_FWD, in/out bit depth 32f.
        //
        let mut g = GammaOpData::default();
        g.set_params(&identity_params);
        g.validate().unwrap();
        assert_eq!(g.style(), BasicFwd);
        assert!(g.is_identity().unwrap());
        assert!(!g.is_no_op().unwrap()); // inBitDepth != outBitDepth
        assert!(g.is_channel_independent());
    }

    let params_r: Params = vec![1.2];
    let params_g: Params = vec![1.6];
    let params_b: Params = vec![2.0];
    let params_a: Params = vec![3.1];

    {
        //
        // Non-identity check for basic style.
        //
        let g = GammaOpData::new(
            BasicFwd,
            params_r.clone(),
            params_g.clone(),
            params_b.clone(),
            params_a.clone(),
        );
        assert!(!g.is_identity().unwrap());
        assert!(!g.is_no_op().unwrap());
        assert!(g.is_channel_independent());
    }

    {
        //
        // Non-identity check for default constructor.
        // Default gamma op is BASIC_FWD, in/out bitDepth 32f.
        //
        let mut g = GammaOpData::default();
        assert!(g.is_identity().unwrap());
        assert!(!g.is_no_op().unwrap()); // basic style clamps, so it isn't a no-op
        assert!(g.is_channel_independent());

        g.set_params(&params_r);
        g.validate().unwrap();

        assert_eq!(g.style(), BasicFwd);
        assert!(!g.is_identity().unwrap());
        assert!(!g.is_no_op().unwrap());
        assert!(g.is_channel_independent());
    }
}

/// Port of `OCIO_ADD_TEST(GammaOpData, identity_style_moncurve)` @ v2.5.2.
#[test]
fn identity_style_moncurve() {
    let identity_params = GammaOpData::identity_parameters(MoncurveFwd);

    {
        //
        // Identity test for moncurve.
        //
        let g = GammaOpData::new(
            MoncurveFwd,
            identity_params.clone(),
            identity_params.clone(),
            identity_params.clone(),
            identity_params.clone(),
        );
        assert!(g.is_identity().unwrap());
        assert!(g.is_no_op().unwrap());
        assert!(g.is_channel_independent());
    }

    {
        //
        // Identity test for forward moncurve with default constructor.
        // Default gamma op is BASIC_FWD, in/out bitDepth 32f.
        //
        let mut g = GammaOpData::default();
        g.set_style(MoncurveFwd);
        g.set_params(&identity_params);
        g.validate().unwrap();
        assert!(g.is_identity().unwrap());
        assert!(g.is_no_op().unwrap());
        assert!(g.is_channel_independent());
    }

    let params_r: Params = vec![1.2, 0.2];
    let params_g: Params = vec![1.6, 0.7];
    let params_b: Params = vec![2.0, 0.5];
    let params_a: Params = vec![3.1, 0.1];

    {
        //
        // Non-identity test for moncurve.
        //
        let g = GammaOpData::new(
            MoncurveFwd,
            params_r.clone(),
            params_g.clone(),
            params_b.clone(),
            params_a.clone(),
        );
        assert!(!g.is_identity().unwrap());
        assert!(!g.is_no_op().unwrap());
        assert!(g.is_channel_independent());
    }

    {
        //
        // Non-identity test for moncurve with default constructor.
        // Default gamma op is BASIC_FWD, in/out bitDepth 32f.
        //
        let mut g = GammaOpData::default();
        g.set_style(MoncurveFwd);
        g.set_params(&params_r);
        g.validate().unwrap();

        assert!(!g.is_identity().unwrap());
        assert!(!g.is_no_op().unwrap());
        assert!(g.is_channel_independent());
    }
}

/// Port of `OCIO_ADD_TEST(GammaOpData, noop_style_basic)` @ v2.5.2.
#[test]
fn noop_style_basic() {
    // Test basic gamma
    let identity_params = GammaOpData::identity_parameters(BasicFwd);

    {
        //
        // NoOp test, basic style.
        //
        let g = GammaOpData::new(
            BasicFwd,
            identity_params.clone(),
            identity_params.clone(),
            identity_params.clone(),
            identity_params.clone(),
        );
        assert!(g.is_identity().unwrap());
        assert!(!g.is_no_op().unwrap()); // basic style clamps, so it isn't a no-op
        assert!(g.is_channel_independent());
    }

    let params_r: Params = vec![1.2];
    let params_g: Params = vec![1.6];
    let params_b: Params = vec![2.0];
    let params_a: Params = vec![3.1];

    {
        //
        // Non-NoOp test, basic style.
        //
        let g = GammaOpData::new(BasicFwd, params_r, params_g, params_b, params_a);
        assert!(!g.is_identity().unwrap());
        assert!(!g.is_no_op().unwrap());
        assert!(g.is_channel_independent());
    }
}

/// Port of `OCIO_ADD_TEST(GammaOpData, noop_style_moncurve)` @ v2.5.2.
#[test]
fn noop_style_moncurve() {
    // Test monCurve gamma
    let identity_params = GammaOpData::identity_parameters(MoncurveFwd);

    {
        //
        // NoOp test, moncurve style.
        //
        let g = GammaOpData::new(
            MoncurveFwd,
            identity_params.clone(),
            identity_params.clone(),
            identity_params.clone(),
            identity_params.clone(),
        );
        assert!(g.is_identity().unwrap());
        assert!(g.is_no_op().unwrap());
        assert!(g.is_channel_independent());
    }

    let params_r: Params = vec![1.2, 0.2];
    let params_g: Params = vec![1.6, 0.7];
    let params_b: Params = vec![2.0, 0.5];
    let params_a: Params = vec![3.1, 0.1];

    {
        //
        // Non-NoOp test, moncurve style.
        //
        let g = GammaOpData::new(MoncurveFwd, params_r, params_g, params_b, params_a);
        assert!(!g.is_identity().unwrap());
        assert!(!g.is_no_op().unwrap());
        assert!(g.is_channel_independent());
    }
}

/// Port of `OCIO_ADD_TEST(GammaOpData, validate)` @ v2.5.2.
#[test]
fn validate() {
    let params: Params = vec![2.6];

    let params_r: Params = vec![2.4, 0.1];
    let params_g: Params = vec![2.2, 0.2];
    let params_b: Params = vec![2.0, 0.4];
    let params_a: Params = vec![1.8, 0.6];

    {
        let g1 = GammaOpData::new(
            MoncurveFwd,
            params_r.clone(),
            params_g.clone(),
            params.clone(),
            params_a.clone(),
        );
        check_throw_what(g1.validate(), "GammaOp: Wrong number of parameters");
    }

    {
        let g1 = GammaOpData::new(
            BasicFwd,
            params_b.clone(),
            params_b.clone(),
            params_b.clone(),
            params_b.clone(),
        );
        check_throw_what(g1.validate(), "GammaOp: Wrong number of parameters");
    }

    {
        let params1: Params = vec![0.006]; // valid range is [0.01, 100]

        let g1 = GammaOpData::new(
            BasicFwd,
            params1.clone(),
            params1.clone(),
            params1.clone(),
            params1.clone(),
        );
        check_throw_what(
            g1.validate(),
            "Parameter 0.006 is less than lower bound 0.01",
        );
    }

    {
        let params1: Params = vec![110.]; // valid range is [0.01, 100]

        let g1 = GammaOpData::new(
            BasicFwd,
            params1.clone(),
            params1.clone(),
            params1.clone(),
            params1.clone(),
        );
        check_throw_what(
            g1.validate(),
            "Parameter 110 is greater than upper bound 100",
        );
    }

    {
        let params1: Params = vec![
            1.,  // valid range is [1, 10]
            11., // valid range is [0, 0.9]
        ];

        let g1 = GammaOpData::new(
            MoncurveFwd,
            params1.clone(),
            params1.clone(),
            params1.clone(),
            params1.clone(),
        );
        check_throw_what(
            g1.validate(),
            "Parameter 11 is greater than upper bound 0.9",
        );
    }

    {
        let params1: Params = vec![
            1., // valid range is [1, 10]
            0., // valid range is [0, 0.9]
        ];

        let g1 = GammaOpData::new(
            MoncurveFwd,
            params1.clone(),
            params1.clone(),
            params1.clone(),
            params1.clone(),
        );

        g1.validate().unwrap();
    }

    {
        let params1: Params = vec![
            1.,    // valid range is [1, 10]
            -1e-6, // valid range is [0, 0.9]
        ];

        let g1 = GammaOpData::new(
            MoncurveFwd,
            params1.clone(),
            params1.clone(),
            params1.clone(),
            params1.clone(),
        );
        check_throw_what(g1.validate(), "Parameter -1e-06 is less than lower bound 0");
    }
}

/// Port of `OCIO_ADD_TEST(GammaOpData, equality)` @ v2.5.2.
#[test]
fn equality() {
    let params_r1: Params = vec![2.4, 0.1];
    let params_g1: Params = vec![2.2, 0.2];
    let params_b1: Params = vec![2.0, 0.4];
    let params_a1: Params = vec![1.8, 0.6];

    let g1 = GammaOpData::new(
        MoncurveFwd,
        params_r1.clone(),
        params_g1.clone(),
        params_b1.clone(),
        params_a1.clone(),
    );

    let params_r2: Params = vec![2.6, 0.1]; // 2.6 != 2.4
    let params_g2 = params_g1.clone();
    let params_b2 = params_b1.clone();
    let params_a2 = params_a1.clone();

    let g2 = GammaOpData::new(MoncurveFwd, params_r2, params_g2, params_b2, params_a2);

    assert!(!(g1 == g2));

    let mut g3 = GammaOpData::new(
        MoncurveRev,
        params_r1.clone(),
        params_g1.clone(),
        params_b1.clone(),
        params_a1.clone(),
    );

    assert!(!(g3 == g1));

    g3.set_style(g1.style());
    g3.validate().unwrap();

    assert!(g3 == g1);

    let g4 = GammaOpData::new(MoncurveFwd, params_r1, params_g1, params_b1, params_a1);

    assert!(g4 == g1);
}

/// `CheckGammaInverse` (tests/cpu/ops/gamma/GammaOpData_tests.cpp:389-419 @ v2.5.2).
#[allow(clippy::too_many_arguments)]
fn check_gamma_inverse(
    ref_style: GammaStyle,
    ref_params_r: &Params,
    ref_params_g: &Params,
    ref_params_b: &Params,
    ref_params_a: &Params,
    inv_style: GammaStyle,
    inv_params_r: &Params,
    inv_params_g: &Params,
    inv_params_b: &Params,
    inv_params_a: &Params,
) {
    let ref_gamma_op = GammaOpData::new(
        ref_style,
        ref_params_r.clone(),
        ref_params_g.clone(),
        ref_params_b.clone(),
        ref_params_a.clone(),
    );

    let inv_op = ref_gamma_op.inverse();

    assert_eq!(inv_op.style(), inv_style);

    assert!(inv_op.red_params() == inv_params_r);
    assert!(inv_op.green_params() == inv_params_g);
    assert!(inv_op.blue_params() == inv_params_b);
    assert!(inv_op.alpha_params() == inv_params_a);

    assert!(ref_gamma_op.is_inverse(&inv_op));
    assert!(inv_op.is_inverse(&ref_gamma_op));
    assert!(!ref_gamma_op.is_inverse(&ref_gamma_op));
    assert!(!inv_op.is_inverse(&inv_op));
}

/// Port of `OCIO_ADD_TEST(GammaOpData, basic_inverse)` @ v2.5.2.
#[test]
fn basic_inverse() {
    let params_r: Params = vec![2.2];
    let params_g: Params = vec![2.4];
    let params_b: Params = vec![2.6];
    let params_a: Params = vec![2.8];

    check_gamma_inverse(
        BasicFwd, &params_r, &params_g, &params_b, &params_a, BasicRev, &params_r, &params_g,
        &params_b, &params_a,
    );

    check_gamma_inverse(
        BasicRev, &params_r, &params_g, &params_b, &params_a, BasicFwd, &params_r, &params_g,
        &params_b, &params_a,
    );
}

/// Port of `OCIO_ADD_TEST(GammaOpData, moncurve_inverse)` @ v2.5.2.
#[test]
fn moncurve_inverse() {
    let params_r: Params = vec![2.4, 0.1];
    let params_g: Params = vec![2.2, 0.2];
    let params_b: Params = vec![2.0, 0.4];
    let params_a: Params = vec![1.8, 0.6];

    check_gamma_inverse(
        MoncurveFwd,
        &params_r,
        &params_g,
        &params_b,
        &params_a,
        MoncurveRev,
        &params_r,
        &params_g,
        &params_b,
        &params_a,
    );

    check_gamma_inverse(
        MoncurveRev,
        &params_r,
        &params_g,
        &params_b,
        &params_a,
        MoncurveFwd,
        &params_r,
        &params_g,
        &params_b,
        &params_a,
    );
}

/// Port of `OCIO_ADD_TEST(GammaOpData, is_inverse)` @ v2.5.2.
#[test]
fn is_inverse() {
    // NB: isInverse ignores bit-depth.

    // See also addtl tests in CheckGammaInverse() above.
    // Just need to test that if params are unequal it is not an inverse.
    let mut params_r: Params = vec![2.4]; // gamma
    let mut params_g: Params = vec![2.41]; // gamma

    let gamma_op1 = GammaOpData::new(
        BasicFwd,
        params_r.clone(),
        params_g.clone(),
        params_r.clone(),
        params_r.clone(),
    );

    let gamma_op2 = GammaOpData::new(
        BasicRev,
        params_r.clone(),
        params_g.clone(),
        params_r.clone(),
        params_r.clone(),
    );

    // Set B param differently.
    let gamma_op3 = GammaOpData::new(
        BasicRev,
        params_r.clone(),
        params_g.clone(),
        params_g.clone(),
        params_r.clone(),
    );

    assert!(gamma_op1.is_inverse(&gamma_op2));
    assert!(!gamma_op1.is_inverse(&gamma_op3));

    params_r.push(0.1); // offset
    params_g.push(0.1); // offset

    let gamma_op1m = GammaOpData::new(
        MoncurveFwd,
        params_r.clone(),
        params_g.clone(),
        params_r.clone(),
        params_r.clone(),
    );

    let gamma_op2m = GammaOpData::new(
        MoncurveRev,
        params_r.clone(),
        params_g.clone(),
        params_r.clone(),
        params_r.clone(),
    );

    // Set blue param differently.
    let gamma_op3m = GammaOpData::new(
        MoncurveRev,
        params_r.clone(),
        params_g.clone(),
        params_g.clone(),
        params_r.clone(),
    );

    assert!(gamma_op1m.is_inverse(&gamma_op2m));
    assert!(!gamma_op1m.is_inverse(&gamma_op3m));
}

/// `TestMayComposeStyle` (tests/cpu/ops/gamma/GammaOpData_tests.cpp:493-501 @ v2.5.2).
#[track_caller]
fn test_may_compose_style(s1: GammaStyle, s2: GammaStyle, expected: bool) {
    let params: Params = vec![2.];
    let g1 = GammaOpData::new(
        s1,
        params.clone(),
        params.clone(),
        params.clone(),
        params.clone(),
    );
    let g2 = GammaOpData::new(
        s2,
        params.clone(),
        params.clone(),
        params.clone(),
        params.clone(),
    );
    assert_eq!(g1.may_compose(&g2), expected);
    assert_eq!(g2.may_compose(&g1), expected);
}

/// Port of `OCIO_ADD_TEST(GammaOpData, mayCompose)` @ v2.5.2.
#[test]
fn may_compose() {
    test_may_compose_style(BasicFwd, BasicFwd, true);
    test_may_compose_style(BasicFwd, BasicRev, true);
    test_may_compose_style(BasicRev, BasicRev, true);
    test_may_compose_style(BasicFwd, BasicMirrorFwd, true);
    test_may_compose_style(BasicFwd, BasicMirrorRev, true);
    test_may_compose_style(BasicRev, BasicMirrorFwd, true);
    test_may_compose_style(BasicRev, BasicMirrorRev, true);
    test_may_compose_style(BasicFwd, BasicPassThruFwd, true);
    test_may_compose_style(BasicFwd, BasicPassThruRev, true);
    test_may_compose_style(BasicRev, BasicPassThruFwd, true);
    test_may_compose_style(BasicRev, BasicPassThruRev, true);
    test_may_compose_style(BasicMirrorFwd, BasicMirrorFwd, true);
    test_may_compose_style(BasicMirrorRev, BasicMirrorRev, true);
    test_may_compose_style(BasicMirrorRev, BasicMirrorFwd, true);
    test_may_compose_style(BasicPassThruFwd, BasicPassThruFwd, true);
    test_may_compose_style(BasicPassThruRev, BasicPassThruRev, true);
    test_may_compose_style(BasicPassThruFwd, BasicPassThruRev, true);
    test_may_compose_style(BasicMirrorFwd, BasicPassThruFwd, false);
    test_may_compose_style(BasicMirrorFwd, BasicPassThruRev, false);
    test_may_compose_style(BasicMirrorRev, BasicPassThruFwd, false);
    test_may_compose_style(BasicMirrorRev, BasicPassThruRev, false);

    let mut params1: Params = vec![1.];
    let params2: Params = vec![2.2];
    let mut params3: Params = vec![2.6];

    {
        // R == G != B params.
        let g1 = GammaOpData::new(
            BasicFwd,
            params2.clone(),
            params2.clone(),
            params1.clone(),
            params1.clone(),
        );
        let g2 = GammaOpData::new(
            BasicFwd,
            params2.clone(),
            params2.clone(),
            params2.clone(),
            params1.clone(),
        );
        assert!(g1.may_compose(&g2));
    }

    {
        let g1 = GammaOpData::new(
            BasicFwd,
            params2.clone(),
            params2.clone(),
            params2.clone(),
            params1.clone(),
        );
        params1.push(0.0);
        params3.push(0.1);
        let g2 = GammaOpData::new(
            MoncurveFwd,
            params3.clone(),
            params3.clone(),
            params3.clone(),
            params1.clone(),
        );
        // Moncurve not allowed.
        assert!(!g1.may_compose(&g2));
    }
}

/// `CheckGammaCompose` (tests/cpu/ops/gamma/GammaOpData_tests.cpp:577-601 @ v2.5.2).
#[track_caller]
fn check_gamma_compose(
    style1: GammaStyle,
    params1: &Params,
    style2: GammaStyle,
    params2: &Params,
    ref_style: GammaStyle,
    ref_params: &Params,
) {
    let params_a: Params = vec![1.];

    let g1 = GammaOpData::new(
        style1,
        params1.clone(),
        params1.clone(),
        params1.clone(),
        params_a.clone(),
    );

    let g2 = GammaOpData::new(
        style2,
        params2.clone(),
        params2.clone(),
        params2.clone(),
        params_a.clone(),
    );

    let g3 = g1.compose(&g2).unwrap();

    assert_eq!(g3.style(), ref_style);

    assert!(g3.red_params() == ref_params);
    assert!(g3.green_params() == ref_params);
    assert!(g3.blue_params() == ref_params);
    assert!(*g3.alpha_params() == params_a);
}

/// Port of `OCIO_ADD_TEST(GammaOpData, compose)` @ v2.5.2.
#[test]
fn compose() {
    check_gamma_compose(
        BasicFwd,
        &vec![2.],
        BasicFwd,
        &vec![3.],
        BasicFwd,
        &vec![6.],
    );

    check_gamma_compose(
        BasicRev,
        &vec![2.],
        BasicRev,
        &vec![4.],
        BasicRev,
        &vec![8.],
    );

    check_gamma_compose(
        BasicRev,
        &vec![4.],
        BasicFwd,
        &vec![2.],
        BasicRev,
        &vec![2.],
    );

    check_gamma_compose(
        BasicRev,
        &vec![2.],
        BasicFwd,
        &vec![4.],
        BasicFwd,
        &vec![2.],
    );

    check_gamma_compose(
        BasicFwd,
        &vec![2.],
        BasicRev,
        &vec![4.],
        BasicRev,
        &vec![2.],
    );

    check_gamma_compose(
        BasicPassThruFwd,
        &vec![2.],
        BasicPassThruRev,
        &vec![4.],
        BasicPassThruRev,
        &vec![2.],
    );

    check_gamma_compose(
        BasicMirrorFwd,
        &vec![2.],
        BasicMirrorRev,
        &vec![4.],
        BasicMirrorRev,
        &vec![2.],
    );

    check_gamma_compose(
        BasicMirrorFwd,
        &vec![2.],
        BasicRev,
        &vec![4.],
        BasicRev,
        &vec![2.],
    );

    check_gamma_compose(
        BasicPassThruFwd,
        &vec![2.],
        BasicRev,
        &vec![4.],
        BasicRev,
        &vec![2.],
    );

    {
        let params1: Params = vec![4.];
        let params_a: Params = vec![1.];
        let g1 = GammaOpData::new(
            BasicMirrorFwd,
            params1.clone(),
            params1.clone(),
            params1.clone(),
            params_a.clone(),
        );

        let params2: Params = vec![2.];
        let g2 = GammaOpData::new(
            BasicPassThruFwd,
            params2.clone(),
            params2.clone(),
            params2.clone(),
            params_a.clone(),
        );

        check_throw_what(
            g1.compose(&g2),
            "GammaOp can only be combined with some GammaOps",
        );
    }

    {
        let params1: Params = vec![4.];
        let mut params_a: Params = vec![1.];
        let g1 = GammaOpData::new(
            BasicRev,
            params1.clone(),
            params1.clone(),
            params1.clone(),
            params_a.clone(),
        );

        let params2: Params = vec![2., 0.1];
        params_a.push(0.0);

        let g2 = GammaOpData::new(
            MoncurveRev,
            params2.clone(),
            params2.clone(),
            params2.clone(),
            params_a.clone(),
        );

        check_throw_what(
            g1.compose(&g2),
            "GammaOp can only be combined with some GammaOps",
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The port's own checks.

/// The style conversions of the transforms, and their errors (GammaOpData.cpp:152-242 @
/// v2.5.2).
#[test]
fn styles_and_directions() {
    for neg in [
        NegativeStyle::Clamp,
        NegativeStyle::Mirror,
        NegativeStyle::PassThru,
    ] {
        let fwd = GammaOpData::convert_style_basic(neg, Forward).unwrap();
        let rev = GammaOpData::convert_style_basic(neg, Inverse).unwrap();
        assert_eq!(GammaOpData::convert_style(fwd), neg);
        let mut g = GammaOpData::new(fwd, vec![2.2], vec![2.2], vec![2.2], vec![2.2]);
        assert_eq!(g.direction(), Forward);
        g.set_direction(Inverse);
        assert_eq!(g.style(), rev);
        assert!(g.is_inverse(&g.inverse()));
    }
    let err = GammaOpData::convert_style_basic(NegativeStyle::Linear, Forward).unwrap_err();
    assert_eq!(
        err.message(),
        "Linear negative extrapolation is not valid for basic exponent style."
    );
    let err = GammaOpData::convert_style_mon_curve(NegativeStyle::Clamp, Inverse).unwrap_err();
    assert_eq!(
        err.message(),
        "Clamp negative extrapolation is not valid for MonCurve exponent style."
    );
    let err = GammaOpData::convert_style_mon_curve(NegativeStyle::PassThru, Forward).unwrap_err();
    assert_eq!(
        err.message(),
        "Pass thru negative extrapolation is not valid for MonCurve exponent style."
    );
}

/// Each style's name reads back as that style, in any ASCII case and up to a NUL; the errors
/// for an unknown or missing name (GammaOpData.cpp:66-118 @ v2.5.2).
#[test]
fn style_names() {
    for style in [
        BasicFwd,
        BasicRev,
        BasicMirrorFwd,
        BasicMirrorRev,
        BasicPassThruFwd,
        BasicPassThruRev,
        MoncurveFwd,
        MoncurveRev,
        MoncurveMirrorFwd,
        MoncurveMirrorRev,
    ] {
        let name = GammaOpData::convert_style_to_string(style);
        for text in [
            name.to_string(),
            name.to_ascii_uppercase(),
            name.to_ascii_lowercase(),
            format!("{name}\0junk"),
        ] {
            assert_eq!(
                GammaOpData::convert_string_to_style(Some(&text)).unwrap(),
                style
            );
        }
    }
    assert_eq!(
        GammaOpData::convert_string_to_style(Some("basic"))
            .unwrap_err()
            .message(),
        "Unknown gamma style: 'basic'."
    );
    for missing in [None, Some(""), Some("\0basicFwd")] {
        assert_eq!(
            GammaOpData::convert_string_to_style(missing)
                .unwrap_err()
                .message(),
            "Missing gamma style."
        );
    }
}

/// Where upstream reads past a channel's parameters, the port returns an error instead
/// (`docs/improvements.md` U-24), and only there.
#[test]
fn short_parameters_are_errors() {
    let short = |style, p: Params| GammaOpData::new(style, p.clone(), p.clone(), p.clone(), p);

    // Equal channels: `isIdentity` reads red's p[0], and a moncurve style's p[1] after a 1.
    for g in [
        short(BasicFwd, vec![]),
        short(MoncurveFwd, vec![]),
        short(MoncurveRev, vec![1.]),
    ] {
        assert_eq!(g.is_identity().unwrap_err().message(), SHORT_PARAMS);
        assert_eq!(g.is_no_op().unwrap_err().message(), SHORT_PARAMS);
    }
    for g in [short(BasicFwd, vec![]), short(MoncurveFwd, vec![])] {
        assert_eq!(g.get_cache_id().unwrap_err().message(), SHORT_PARAMS);
    }
    // A moncurve gamma other than 1 (NaN included) is decided on p[0]: no read past it.
    for g in [
        short(MoncurveFwd, vec![2.]),
        short(MoncurveFwd, vec![f64::NAN]),
    ] {
        assert!(g.is_identity().is_ok());
        assert!(g.is_no_op().is_ok());
        assert!(g.get_cache_id().is_ok());
    }
    // Channels that differ are decided before red is read; the cache ID prints each.
    let g = GammaOpData::new(BasicFwd, vec![], vec![2.], vec![], vec![]);
    assert!(g.is_identity().is_ok());
    assert_eq!(g.get_cache_id().unwrap_err().message(), SHORT_PARAMS);

    // `compose` reads [0] of every channel of both, after `mayCompose`.
    let full = short(BasicFwd, vec![2.]);
    let mut empty_alpha = full.clone();
    empty_alpha.set_alpha_params(vec![]);
    assert_eq!(
        full.compose(&empty_alpha).unwrap_err().message(),
        SHORT_PARAMS
    );
    assert_eq!(
        empty_alpha.compose(&full).unwrap_err().message(),
        SHORT_PARAMS
    );
    let moncurve = short(MoncurveFwd, vec![]);
    assert_eq!(
        empty_alpha.compose(&moncurve).unwrap_err().message(),
        "GammaOp can only be combined with some GammaOps"
    );
    // Longer vectors are read as upstream reads them: the first value only.
    assert!(full.compose(&short(BasicFwd, vec![2., 5.])).is_ok());
}
