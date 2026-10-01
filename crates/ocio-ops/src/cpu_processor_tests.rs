// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/CPUProcessor_tests.cpp` @ v2.5.2: so far `flag_composition`. The other
//! tests build processors from transforms through a config (WP 1.8, Phase 3).

use crate::open_color_types::OptimizationFlags;

/// Port of `OCIO_ADD_TEST(CPUProcessor, flag_composition)` @ v2.5.2.
#[test]
fn flag_composition() {
    // The test validates the build of a custom optimization flag.

    let mut custom_flags = OptimizationFlags::LOSSLESS;

    assert_eq!(
        custom_flags & OptimizationFlags::COMP_LUT1D,
        OptimizationFlags::NONE
    );

    custom_flags = custom_flags | OptimizationFlags::COMP_LUT1D;

    assert_eq!(
        custom_flags & OptimizationFlags::COMP_LUT1D,
        OptimizationFlags::COMP_LUT1D
    );
}
