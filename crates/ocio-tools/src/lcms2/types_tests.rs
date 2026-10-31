// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! lcms2's own tests of the fast floors: `testbed/testcms2.c` @ lcms2.17.

use super::*;

/// Port of `CheckQuickFloor` (testbed/testcms2.c:792-807 @ lcms2.17).
#[test]
fn check_quick_floor() {
    assert_eq!(cms_quick_floor(1.234), 1);
    assert_eq!(cms_quick_floor(32767.234), 32767);
    assert_eq!(cms_quick_floor(-1.234), -2);
    assert_eq!(cms_quick_floor(-32767.1), -32768);
}

/// Port of `CheckQuickFloorWord` (testbed/testcms2.c:810-824 @ lcms2.17).
#[test]
fn check_quick_floor_word() {
    for i in 0u32..65535 {
        assert_eq!(cms_quick_floor_word(i as f64 + 0.1234) as u32, i);
    }
}
