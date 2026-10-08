// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The port's refusal of a CDL or CCC whose root element never starts (U-75), where the
//! wheel dereferences a null pointer and crashes (probed 2026-10-08).

use super::*;

#[test]
fn a_cdl_or_ccc_without_its_root_element_is_refused() {
    for (header, root) in [
        ("ColorDecisionList", "ColorDecisionList"),
        ("ColorCorrectionCollection", "ColorCorrectionCollection"),
    ] {
        let mut parser = CdlParser::new(b"x.cdl");
        let mut istream = InputStream::from_bytes(format!("<!-- <{header} --><Foo/>\n"));
        let e = parser.parse(&mut istream).expect_err("refused");
        assert_eq!(
            e.message(),
            format!(
                "Error parsing {root} (x.cdl). Error is: CDL parsing error: the root element \
                 '{root}' is missing. At line (2)"
            )
        );
    }
}
