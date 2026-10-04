// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The `Lut1DTransform` against the wheel (`common::transforms`):
//! - its text (`repr()`, `str()`): the direction, file bit depth, interpolation, half flags,
//!   hue adjustment and length, then each channel's minimum and maximum, C++'s `std::min` and
//!   `std::max` from `FLT_MAX` and `-FLT_MAX` (so NaN entries are skipped), with 6 significant
//!   digits, or 16 after a MatrixTransform in the same group (I-73); its validation, every
//!   message of the data's; `equals()` between LUTs that differ in each part;
//! - the errors of its setters (`setLength`, `setValue`, `setHueAdjust`) and of its
//!   constructor, by their messages;
//! - the raw config's processor of each forward LUT: `BuildLut1DOp`, then
//!   `CreateLut1DTransform` through `createGroupTransform()`, the values bit for bit (an
//!   inverse LUT's processor needs the inverse 1D LUT, Phase 2, WP 2.1);
//! - the optimized processors at integer and half input (`getOptimizedProcessor`), which keep
//!   a lone forward LUT, or replace a range before it, or bake a separable prefix into one:
//!   `CreateLut1DTransform` of the baked LUT, entry for entry. The bakes follow each
//!   platform's math library (I-24), so the checks compare each platform's wheel with the port
//!   on that platform.
//!
//! The binding passes a C `float` as a Python float, which quiets a signalling NaN, so the
//! values set and compared are floats a double holds exactly: quiet NaNs, with their
//! payloads, are widened by their bits.

mod common;

use std::ffi::c_ulong;

use common::transforms::{
    BIT_DEPTHS, Case, bit_depth_spec, check_optimized_processors, check_processors_dirs,
    check_text, direction_spec, group, hue_adjust_name, interpolation_name, setter_errors,
};
use ocio::{
    BitDepth, Interpolation, Lut1DHueAdjust, Lut1DTransform, MatrixTransform, RangeTransform,
    TransformDirection,
};
use ocio_testkit::battery::BitDepth as Depth;
use ocio_testkit::transform_text::f64_spec;
use serde_json::{Value, json};

use TransformDirection::{Forward, Inverse};

/// A float as a spec value: its exact double, a NaN widened by its bits (the sign, the quiet
/// bit and the payload), as the C++ conversion and the port's `f64::from` give it.
fn f32_spec(v: f32) -> Value {
    let bits = v.to_bits();
    let wide = if v.is_nan() {
        (u64::from(bits >> 31) << 63) | (0x7ffu64 << 52) | (u64::from(bits & 0x007f_ffff) << 29)
    } else {
        f64::from(v).to_bits()
    };
    f64_spec(f64::from_bits(wide))
}

/// The interpolation, as a spec value.
fn interpolation_spec(interp: Interpolation) -> Value {
    json!({"enum": interpolation_name(interp)})
}

/// Every interpolation.
const INTERPOLATIONS: [Interpolation; 7] = [
    Interpolation::Unknown,
    Interpolation::Nearest,
    Interpolation::Linear,
    Interpolation::Tetrahedral,
    Interpolation::Cubic,
    Interpolation::Default,
    Interpolation::Best,
];

/// The hue adjustment, as a spec value.
fn hue_spec(hue: Lut1DHueAdjust) -> Value {
    json!({"enum": hue_adjust_name(hue)})
}

/// A LUT built the same way through the binding and the port: a constructor, then setters.
#[derive(Debug, Clone)]
struct Lut {
    port: Lut1DTransform,
    args: Value,
    calls: Vec<Value>,
}

impl Lut {
    /// `Lut1DTransform()`.
    fn new() -> Lut {
        Lut {
            port: Lut1DTransform::new(),
            args: json!({}),
            calls: Vec::new(),
        }
    }

    /// `Lut1DTransform(length, inputHalfDomain)`.
    fn with_length(length: c_ulong, half: bool) -> Lut {
        Lut {
            port: Lut1DTransform::with_length(length, half).unwrap(),
            args: json!({"length": length, "inputHalfDomain": half}),
            calls: Vec::new(),
        }
    }

    fn call(mut self, call: Value, set: impl FnOnce(&mut Lut1DTransform)) -> Lut {
        set(&mut self.port);
        self.calls.push(call);
        self
    }

    fn length(self, length: c_ulong) -> Lut {
        self.call(json!(["setLength", length]), |t| {
            t.set_length(length).unwrap()
        })
    }

    fn value(self, index: c_ulong, [r, g, b]: [f32; 3]) -> Lut {
        self.call(
            json!(["setValue", index, f32_spec(r), f32_spec(g), f32_spec(b)]),
            |t| t.set_value(index, r, g, b).unwrap(),
        )
    }

    fn half(self, half: bool) -> Lut {
        self.call(json!(["setInputHalfDomain", half]), |t| {
            t.set_input_half_domain(half)
        })
    }

    fn raw_halfs(self, raw: bool) -> Lut {
        self.call(json!(["setOutputRawHalfs", raw]), |t| {
            t.set_output_raw_halfs(raw)
        })
    }

    fn hue(self, hue: Lut1DHueAdjust) -> Lut {
        self.call(json!(["setHueAdjust", hue_spec(hue)]), |t| {
            t.set_hue_adjust(hue).unwrap()
        })
    }

    fn interpolation(self, interp: Interpolation) -> Lut {
        self.call(
            json!(["setInterpolation", interpolation_spec(interp)]),
            |t| t.set_interpolation(interp),
        )
    }

    fn depth(self, depth: BitDepth) -> Lut {
        self.call(
            json!(["setFileOutputBitDepth", bit_depth_spec(depth)]),
            |t| t.set_file_output_bit_depth(depth),
        )
    }

    fn dir(self, dir: TransformDirection) -> Lut {
        self.call(json!(["setDirection", direction_spec(dir)]), |t| {
            t.set_direction(dir)
        })
    }

    fn spec(&self) -> Value {
        json!({"class": "Lut1DTransform", "args": self.args, "calls": self.calls})
    }

    fn case(&self, label: impl Into<String>) -> Case {
        Case::new(label, self.spec(), self.port.clone())
    }
}

/// Floats that print or compare in their own way: quiet NaNs of both signs (one with a
/// payload), the infinities, both zeros, subnormals, the extremes, values at the edge of 6
/// significant digits, and ordinary ones.
fn special_floats() -> Vec<f32> {
    let mut values: Vec<f32> = [0x7fc0_0000u32, 0xffc0_0000, 0x7fc0_beef]
        .map(f32::from_bits)
        .to_vec();
    values.extend([
        f32::INFINITY,
        f32::NEG_INFINITY,
        0.0,
        -0.0,
        f32::from_bits(1),
        -f32::from_bits(1),
        f32::MIN_POSITIVE,
        f32::MAX,
        f32::MIN,
        0.1,
        1.0 / 3.0,
        -2.0 / 3.0,
        1e-7,
        123_456.5,
        999_999.5,
        0.5,
        -1.0,
        2.0,
    ]);
    values
}

/// A ramp of `length` entries with three different curves, from the identity's values.
fn ramp(lut: Lut, length: c_ulong) -> Lut {
    let mut lut = lut.length(length);
    for i in 0..length {
        let x = i as f32 / (length - 1) as f32;
        lut = lut.value(i, [x * x, 1.0 - 2.0 * x, x * 4.0 - 1.5]);
    }
    lut
}

/// The LUT cases: upstream's, the defaults, every setting, ramps, and the special floats in
/// each channel and position (a channel of NaNs prints `FLT_MAX` and `-FLT_MAX`).
fn lut_cases() -> Vec<Case> {
    let mut cases = vec![
        Lut::new().case("the default"),
        Lut::with_length(65536, true).case("a half domain"),
        Lut::with_length(10, true).case("a half domain of 10 entries"),
        Lut::with_length(8, false).case("8 entries"),
        Lut::new()
            .length(65536)
            .half(true)
            .case("a half domain set after the length"),
        Lut::new()
            .half(true)
            .length(65536)
            .case("a half domain set before the length"),
        Lut::with_length(65536, true)
            .half(false)
            .case("a half domain's values on a standard domain"),
        Lut::new().raw_halfs(true).case("raw halfs"),
        Lut::with_length(65536, true)
            .raw_halfs(true)
            .case("a half domain of raw halfs"),
        Lut::new().hue(Lut1DHueAdjust::Dw3).case("DW3"),
        Lut::new()
            .hue(Lut1DHueAdjust::Dw3)
            .hue(Lut1DHueAdjust::None)
            .case("DW3, then none"),
        Lut::new().length(1024 * 1024).case("the longest"),
    ];
    // tests/cpu/transforms/Lut1DTransform_tests.cpp:31-105 @ v2.5.2.
    let upstream = Lut::new()
        .dir(Inverse)
        .length(3)
        .value(1, [0.51, 0.52, 0.53])
        .depth(BitDepth::Uint8)
        .value(0, [-0.2, 0.1, -0.3])
        .value(2, [1.2, 1.3, 0.8]);
    cases.push(upstream.case("upstream's"));
    for interp in INTERPOLATIONS {
        cases.push(Lut::new().interpolation(interp).case(format!("{interp:?}")));
    }
    for depth in BIT_DEPTHS {
        cases.push(Lut::new().depth(depth).case(format!("bit depth {depth:?}")));
    }
    for dir in [Forward, Inverse] {
        cases.push(ramp(Lut::new().dir(dir), 17).case(format!("a ramp, {dir:?}")));
    }
    let specials = special_floats();
    let n = specials.len();
    for (k, &v) in specials.iter().enumerate() {
        let w = specials[(k + 7) % n];
        cases.push(
            Lut::new()
                .length(4)
                .value(0, [v, 0.5, w])
                .case(format!("special {k} ({v:e}) first")),
        );
        cases.push(
            Lut::new()
                .length(4)
                .value(3, [w, v, -0.5])
                .case(format!("special {k} ({v:e}) last")),
        );
        cases.push(
            Lut::new()
                .value(0, [v, v, v])
                .value(1, [v, w, v])
                .case(format!("special {k} ({v:e}) everywhere")),
        );
    }
    let nan = f32::from_bits(0x7fc0_0000);
    cases.push(
        Lut::new()
            .value(0, [nan, nan, nan])
            .value(1, [nan, nan, nan])
            .case("NaNs only"),
    );
    cases
}

/// Groups that show a MatrixTransform's precision on the LUTs after it (I-73).
fn group_cases() -> Vec<Case> {
    let lut = Lut::new()
        .length(3)
        .value(0, [0.123_456_79, 1.0 / 3.0, -2.0 / 3.0])
        .value(2, [123_456.79, 1.0 / 7.0, 0.1])
        .case("a LUT of many digits");
    let matrix = Case::new(
        "a matrix",
        json!({"class": "MatrixTransform"}),
        MatrixTransform::new(),
    );
    vec![
        group(
            "a LUT, a matrix, the LUT",
            Forward,
            &[lut.clone(), matrix.clone(), lut.clone()],
        ),
        group(
            "LUTs in a group after a matrix",
            Inverse,
            &[matrix, group("LUTs", Forward, &[lut.clone(), lut])],
        ),
    ]
}

#[test]
fn text_validation_and_equality_match_the_wheel() {
    let mut cases = lut_cases();
    let n = cases.len();
    // Each LUT case built again: NaN values make a LUT unequal to its copy.
    let copies: Vec<Case> = cases.clone();
    cases.extend(copies);
    let mut pairs = Vec::new();
    for i in 0..n {
        pairs.extend([(i, i), (i, (i + 1) % n), (i, i + n)]);
    }
    // LUTs that differ from one in one part each.
    let base = Lut::new().length(3).value(1, [0.25, 0.5, 0.75]);
    let variants = [
        base.clone().case("the base"),
        base.clone().dir(Inverse).case("inverse"),
        base.clone()
            .depth(BitDepth::Uint10)
            .case("another file bit depth"),
        base.clone()
            .interpolation(Interpolation::Nearest)
            .case("nearest"),
        base.clone()
            .interpolation(Interpolation::Cubic)
            .case("cubic"),
        base.clone().hue(Lut1DHueAdjust::Dw3).case("DW3"),
        base.clone().half(true).case("a half domain"),
        base.clone().raw_halfs(true).case("raw halfs"),
        base.clone()
            .value(1, [0.25, 0.5, 0.75000006])
            .case("another value"),
        base.clone().value(1, [0.25, -0.0, 0.75]).case("-0"),
        base.clone().value(1, [0.25, 0.0, 0.75]).case("+0"),
        base.clone().length(4).case("another length"),
    ];
    let first = cases.len();
    cases.extend(variants);
    for i in first..cases.len() {
        for j in first..cases.len() {
            if i != j {
                pairs.push((i, j));
            }
        }
    }
    // Groups have no equals() in the binding, and a LUT compares only with a LUT.
    let groups = group_cases();
    let g = cases.len();
    cases.extend(groups);
    let range = cases.len();
    cases.push(Case::new(
        "a range",
        json!({"class": "RangeTransform"}),
        RangeTransform::new(),
    ));
    pairs.extend([(g, g), (g, 1), (1, range), (range, 1)]);
    check_text(&cases, &pairs);
}

#[test]
fn setter_errors_match_the_wheel() {
    let l = || Lut::new();
    let checks: Vec<(&str, Value, ocio::Result<()>)> = vec![
        (
            "a length of 1",
            json!({"class": "Lut1DTransform", "calls": [["setLength", 1]]}),
            l().port.set_length(1),
        ),
        (
            "a length of 0",
            json!({"class": "Lut1DTransform", "calls": [["setLength", 0]]}),
            l().port.set_length(0),
        ),
        (
            "a length over 1024x1024",
            json!({"class": "Lut1DTransform", "calls": [["setLength", 1024 * 1024 + 1]]}),
            l().port.set_length(1024 * 1024 + 1),
        ),
        (
            "a constructor's length of 1",
            json!({"class": "Lut1DTransform", "args": {"length": 1, "inputHalfDomain": false}}),
            Lut1DTransform::with_length(1, false).map(|_| ()),
        ),
        (
            "a constructor's half domain of 1",
            json!({"class": "Lut1DTransform", "args": {"length": 1, "inputHalfDomain": true}}),
            Lut1DTransform::with_length(1, true).map(|_| ()),
        ),
        (
            "a constructor's length over 1024x1024",
            json!({"class": "Lut1DTransform",
                "args": {"length": 1024 * 1024 + 1, "inputHalfDomain": false}}),
            Lut1DTransform::with_length(1024 * 1024 + 1, false).map(|_| ()),
        ),
        (
            "a value past the end",
            json!({"class": "Lut1DTransform", "calls": [["setValue", 2, 0.0, 0.0, 0.0]]}),
            l().port.set_value(2, 0.0, 0.0, 0.0),
        ),
        (
            "a value far past the end",
            json!({"class": "Lut1DTransform",
                "calls": [["setLength", 5], ["setValue", 4_000_000_000u32, 0.0, 0.0, 0.0]]}),
            {
                let mut t = Lut1DTransform::new();
                t.set_length(5).unwrap();
                t.set_value(4_000_000_000, 0.0, 0.0, 0.0)
            },
        ),
        (
            "WYPN",
            json!({"class": "Lut1DTransform",
                "calls": [["setHueAdjust", hue_spec(Lut1DHueAdjust::Wypn)]]}),
            l().port.set_hue_adjust(Lut1DHueAdjust::Wypn),
        ),
    ];
    setter_errors(&checks);
}

/// `getValue` past the end raises the message `setValue` does with its own name: the
/// binding's `getValue(index)` against the port's.
#[test]
fn get_value_errors_match_the_wheel() {
    let spec = json!({"class": "Lut1DTransform",
        "calls": [["setLength", 3], ["getValue", 3]]});
    let mut t = Lut1DTransform::new();
    t.set_length(3).unwrap();
    setter_errors(&[("a value past the end", spec, t.value(3).map(|_| ()))]);
}

#[test]
fn processors_match_the_wheel() {
    let mut cases: Vec<Case> = lut_cases()
        .into_iter()
        .filter(|c| c.port.direction() == Forward)
        .collect();
    cases.push(group(
        "LUTs in a group",
        Forward,
        &[
            ramp(Lut::new(), 5).case("a ramp"),
            Lut::new().hue(Lut1DHueAdjust::Dw3).case("DW3"),
        ],
    ));
    check_processors_dirs(&cases, &[Forward]);
}

/// The optimized processors at integer and half input: a lone forward LUT of each input's
/// ideal size or another, kept; a range before a LUT, folded into it; and separable prefixes
/// (a log, an exponent) baked into a LUT, written out by `CreateLut1DTransform`.
#[test]
fn optimized_processors_match_the_wheel() {
    let inputs = [
        Depth::Uint8,
        Depth::Uint10,
        Depth::Uint12,
        Depth::Uint16,
        Depth::F16,
    ];
    let mut cases = vec![
        ramp(Lut::new(), 256).case("a ramp of 256"),
        ramp(Lut::new(), 7).case("a ramp of 7"),
        Lut::with_length(65536, true)
            .value(0x3c00, [2.0, 0.5, -1.0])
            .value(0x3800, [0.25, 0.75, 1.5])
            .case("a half domain, 1 and 0.5 changed"),
        ramp(Lut::new().hue(Lut1DHueAdjust::Dw3), 5).case("a ramp, DW3"),
    ];
    let mut range = RangeTransform::new();
    range.set_min_in_value(0.0);
    range.set_max_in_value(1.0);
    range.set_min_out_value(0.0);
    range.set_max_out_value(1.0);
    let range_case = Case::new(
        "an identity range",
        json!({"class": "RangeTransform", "calls": [
            ["setMinInValue", 0.0], ["setMaxInValue", 1.0],
            ["setMinOutValue", 0.0], ["setMaxOutValue", 1.0]]}),
        range,
    );
    cases.push(group(
        "a range, then a LUT",
        Forward,
        &[range_case, ramp(Lut::new(), 9).case("a ramp")],
    ));
    let mut log = ocio::LogTransform::new();
    log.set_base(10.0);
    cases.push(Case::new(
        "a log",
        json!({"class": "LogTransform", "calls": [["setBase", 10.0]]}),
        log,
    ));
    let mut exponent = ocio::ExponentTransform::new();
    exponent.set_value(&[2.2, 2.0, 1.8, 1.0]);
    cases.push(Case::new(
        "an exponent",
        json!({"class": "ExponentTransform", "calls": [["setValue", [2.2, 2.0, 1.8, 1.0]]]}),
        exponent,
    ));
    let mut requests = Vec::new();
    for input in inputs {
        for output in [Depth::F32, Depth::Uint16] {
            requests.push((input, output));
        }
    }
    let luts = check_optimized_processors(&cases, &requests);
    // Every processor holds a LUT: the LUTs kept or folded, and the bakes.
    assert!(luts.iter().all(|&n| n == requests.len()), "{luts:?}");
}
