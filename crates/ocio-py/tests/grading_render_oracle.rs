// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The curve transforms of chunk 6.7b, rendering: a transform whose curves Python changed
//! through `getValue` (which the binding holds as they are, `set_value_as_is`, and the library
//! fits when it builds the processor's op, as upstream's copy of the op data does) gives the
//! wheel's processor: its CPU processor's cache ID and pixels. The processors come from
//! `common::processors`.
#![cfg(feature = "python")]

mod common;

use common::processors::{cpu, start};
use common::script::{Script, compare, f, fl, i, n, r, ra};

/// A pixel the processor `c` renders now.
fn render(sc: &mut Script, c: usize) {
    sc.call(ra(c, "applyRGB"), vec![fl(&[0.25, 0.5, 0.75])]);
}

/// A curve transform whose curves were changed through `getValue` (held as they are, then
/// fitted when the processor is built, as upstream's copy of the op data fits them) renders
/// and identifies its processor as the wheel's: the CPU processor's cache ID and pixels, for
/// each curve transform, with control points and slopes changed, in each direction.
#[test]
fn curves_changed_through_get_value_render_as_the_wheels() {
    start();
    let mut sc = Script::new();
    for (class, curve, points) in [
        (
            "GradingRGBCurveTransform",
            "master",
            [0.0, 0.1, 0.4, 0.7, 1.0, 0.9],
        ),
        (
            "GradingHueCurveTransform",
            "sat_sat",
            [0.0, 0.0, 0.5, 0.7, 1.0, 1.0],
        ),
    ] {
        for dir in ["OCIO.TRANSFORM_DIR_FORWARD", "OCIO.TRANSFORM_DIR_INVERSE"] {
            let t = sc.call(n(&format!("OCIO.{class}")), vec![]);
            sc.call(ra(t, "setDirection"), vec![n(dir)]);
            let v = sc.call(ra(t, "getValue"), vec![]);
            let it = sc.call(ra(v, &format!("{curve}.getControlPoints")), vec![]);
            for k in 0..3 {
                let p = sc.call(
                    n("OCIO.GradingControlPoint"),
                    vec![f(points[2 * k]), f(points[2 * k + 1])],
                );
                sc.call(ra(it, "__setitem__"), vec![i(k as i128), r(p)]);
            }
            let c = cpu(&mut sc, t);
            sc.call(ra(c, "getCacheID"), vec![]);
            render(&mut sc, c);
            sc.call(ra(c, "applyRGB"), vec![fl(&[-0.5, 0.18, 2.0])]);
            // Slopes too.
            sc.call(
                ra(v, &format!("{curve}.setSlopes")),
                vec![fl(&[0.5, 1.5, 0.25])],
            );
            let c = cpu(&mut sc, t);
            sc.call(ra(c, "getCacheID"), vec![]);
            render(&mut sc, c);
        }
    }
    compare(&[sc]);
}
