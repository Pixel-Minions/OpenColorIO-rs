// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The port's own checks of ACES 2.0's model. Its values are checked against the wheel in
//! `tests/aces2_tables_oracle.rs`, and upstream's tests come with the renderers (2.4e).

use super::*;
use crate::transforms::builtins::color_matrix_helpers::{Chromaticities, aces_ap0, aces_ap1};
use ocio_testkit::upstream::check_throw_what;

/// U-32: limiting primaries with a NaN coordinate, which validation accepts, make upstream's
/// hue table code read and write past its arrays (the wheel's process stops); the port
/// refuses them.
#[test]
fn nan_primaries_are_refused_where_upstream_overruns_the_hue_table() {
    let peak = 100.0f32;
    let p_in = init_jmh_params(&aces_ap0::PRIMARIES).unwrap();
    let reach = init_jmh_params(&aces_ap1::PRIMARIES).unwrap();
    let t = init_tone_scale_params(peak);
    let s = init_shared_compression_params(peak, &p_in, &reach);
    for (red_x, white_x) in [(f64::NAN, 0.3127), (0.64, f64::NAN), (f64::NAN, f64::NAN)] {
        let lim = Primaries::new(
            Chromaticities::new(red_x, 0.33),
            Chromaticities::new(0.30, 0.60),
            Chromaticities::new(0.15, 0.06),
            Chromaticities::new(white_x, 0.3290),
        );
        let p_out = init_jmh_params(&lim).unwrap();
        let mut hue_table: Table1D = [0.0; table_base::TOTAL_SIZE];
        check_throw_what(
            make_uniform_hue_gamut_table(&reach, &p_out, peak, t.forward_limit, &s, &mut hue_table)
                .map(|_| ()),
            CORNERS_OVERRUN,
        );
    }
}
