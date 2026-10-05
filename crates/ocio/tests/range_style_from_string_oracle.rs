// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `range_style_from_string` against the wheel's `RangeStyleFromString` (ParseUtils.cpp, reached
//! through the oracle's `config_calls` on the module), with its exception, for spellings in
//! every case, with white space, empty and not UTF-8.

use ocio::transforms::range_transform::{RangeStyle, range_style_from_string};
use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::{exception, hex};
use serde_json::json;

#[test]
fn range_style_from_string_matches_the_wheel() {
    let words: Vec<&[u8]> = vec![
        b"clamp",
        b"Clamp",
        b"CLAMP",
        b"noclamp",
        b"noClamp",
        b"NOCLAMP",
        b" clamp",
        b"clamp ",
        b"",
        b"no_clamp",
        b"cl\xffamp",
    ];
    let calls: Vec<_> = words
        .iter()
        .map(|w| json!({"call": "RangeStyleFromString", "on": "OCIO", "args": [{"bytes": hex(w)}]}))
        .collect();
    let response = Oracle::get().call(
        "config_calls",
        json!({"config": "raw", "calls": calls}),
        &[],
    );
    let results = response.result["calls"].as_array().unwrap();
    for (w, wheel) in words.iter().zip(results) {
        match range_style_from_string(Some(w)) {
            Ok(style) => {
                let name = match style {
                    RangeStyle::NoClamp => "RANGE_NO_CLAMP",
                    RangeStyle::Clamp => "RANGE_CLAMP",
                };
                assert_eq!(wheel["result"]["enum"], name, "{w:?}: {wheel}");
            }
            Err(e) => match wheel["undecodable"].as_str() {
                Some(bytes) => assert_eq!(hex(e.what()), bytes, "{w:?}"),
                None => assert_eq!(e.what(), exception(wheel).1, "{w:?}: {wheel}"),
            },
        }
    }
}
