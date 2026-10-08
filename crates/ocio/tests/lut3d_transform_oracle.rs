// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The `Lut3DTransform` against the wheel (`common::transforms`):
//! - its text (`repr()`, `str()`): the direction, file bit depth, interpolation and grid size,
//!   then each channel's minimum and maximum, C++'s `std::min` and `std::max` from `FLT_MAX`
//!   and `-FLT_MAX` (so NaN entries are skipped), with 6 significant digits, or 16 after a
//!   MatrixTransform in the same group (I-73); its validation, every message of the data's;
//!   `equals()` between LUTs that differ in each part;
//! - the errors of its setters (`setGridSize`, `setValue`) and of its constructor, by their
//!   messages, and of `getValue`;
//! - the raw config's processor of each LUT: `BuildLut3DOp`, then `CreateLut3DTransform`
//!   through `createGroupTransform()`, the values bit for bit;
//! - the optimized processors of a LUT and its inverse, which the optimizer replaces with
//!   the LUT's identity replacement, a [0, 1] range;
//! - the 3D LUTs the optimizer makes: an inverse LUT's fast forward LUT and the composition
//!   of two LUTs, entry for entry. They render the forward LUTs with the CPU's SIMD kernel,
//!   so this target is in `cpu-tests`.
//!
//! The pixels through the API are `api_battery_oracle.rs`'s.
//!
//! The binding passes a C `float` as a Python float, which quiets a signalling NaN, so the
//! values set and compared are floats a double holds exactly: quiet NaNs, with their
//! payloads, are widened by their bits.

mod common;

use std::ffi::c_ulong;

use common::transforms::{
    BIT_DEPTHS, Case, bit_depth_spec, check_optimized_processors, check_optimized_processors_at,
    check_processors_dirs, check_text, direction_spec, group, interpolation_name, setter_errors,
};
use ocio::{
    BitDepth, Interpolation, Lut3DTransform, MatrixTransform, OptimizationFlags, RangeTransform,
    Transform, TransformDirection,
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

/// A LUT built the same way through the binding and the port: a constructor, then setters.
#[derive(Debug, Clone)]
struct Lut {
    port: Lut3DTransform,
    args: Value,
    calls: Vec<Value>,
}

impl Lut {
    /// `Lut3DTransform()`.
    fn new() -> Lut {
        Lut {
            port: Lut3DTransform::new(),
            args: json!({}),
            calls: Vec::new(),
        }
    }

    /// `Lut3DTransform(gridSize)`.
    fn with_grid_size(grid_size: c_ulong) -> Lut {
        Lut {
            port: Lut3DTransform::with_grid_size(grid_size).unwrap(),
            args: json!({"gridSize": grid_size}),
            calls: Vec::new(),
        }
    }

    fn call(mut self, call: Value, set: impl FnOnce(&mut Lut3DTransform)) -> Lut {
        set(&mut self.port);
        self.calls.push(call);
        self
    }

    fn grid(self, grid_size: c_ulong) -> Lut {
        self.call(json!(["setGridSize", grid_size]), |t| {
            t.set_grid_size(grid_size).unwrap()
        })
    }

    fn value(self, [i, j, k]: [c_ulong; 3], [r, g, b]: [f32; 3]) -> Lut {
        self.call(
            json!(["setValue", i, j, k, f32_spec(r), f32_spec(g), f32_spec(b)]),
            |t| t.set_value(i, j, k, r, g, b).unwrap(),
        )
    }

    fn interpolation(self, interp: Interpolation) -> Lut {
        self.call(
            json!(["setInterpolation", {"enum": interpolation_name(interp)}]),
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
        json!({"class": "Lut3DTransform", "args": self.args, "calls": self.calls})
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

/// A cube of `grid_size` entries per side with three different curves of the identity's
/// values.
fn curves(lut: Lut, grid_size: c_ulong) -> Lut {
    let mut lut = lut.grid(grid_size);
    let last = (grid_size - 1) as f32;
    for i in 0..grid_size {
        for j in 0..grid_size {
            for k in 0..grid_size {
                let (x, y, z) = (i as f32 / last, j as f32 / last, k as f32 / last);
                lut = lut.value([i, j, k], [x * y, 1.0 - 2.0 * z, x + y * 4.0 - 1.5]);
            }
        }
    }
    lut
}

/// The LUT cases: upstream's, the defaults, every setting, curves, and the special floats in
/// each channel and position (a channel of NaNs prints `FLT_MAX` and `-FLT_MAX`).
fn lut_cases() -> Vec<Case> {
    let mut cases = vec![
        Lut::new().case("the default"),
        Lut::with_grid_size(8).case("8 entries per side"),
        Lut::with_grid_size(33).case("33 entries per side"),
        Lut::new().grid(17).case("17 entries per side, set"),
        Lut::new().grid(1).case("1 entry"),
        Lut::new().grid(0).case("no entry"),
        Lut::with_grid_size(129).case("the largest"),
    ];
    // tests/cpu/transforms/Lut3DTransform_tests.cpp:12-104 @ v2.5.2.
    let upstream = Lut::new()
        .dir(Inverse)
        .grid(3)
        .value([0, 1, 2], [0.1, 0.52, 0.93])
        .depth(BitDepth::Uint8)
        .value([0, 0, 0], [-0.2, -0.1, -0.3])
        .value([2, 2, 2], [1.2, 1.3, 1.8]);
    cases.push(upstream.case("upstream's"));
    for interp in INTERPOLATIONS {
        cases.push(Lut::new().interpolation(interp).case(format!("{interp:?}")));
    }
    for depth in BIT_DEPTHS {
        cases.push(Lut::new().depth(depth).case(format!("bit depth {depth:?}")));
    }
    for dir in [Forward, Inverse] {
        for interp in [Interpolation::Linear, Interpolation::Tetrahedral] {
            cases.push(
                curves(Lut::new().dir(dir).interpolation(interp), 5)
                    .case(format!("curves, {dir:?}, {interp:?}")),
            );
        }
    }
    let specials = special_floats();
    let n = specials.len();
    for (k, &v) in specials.iter().enumerate() {
        let w = specials[(k + 7) % n];
        cases.push(
            Lut::new()
                .grid(3)
                .value([0, 0, 0], [v, 0.5, w])
                .case(format!("special {k} ({v:e}) first")),
        );
        cases.push(
            Lut::new()
                .grid(3)
                .value([2, 2, 2], [w, v, -0.5])
                .case(format!("special {k} ({v:e}) last")),
        );
        let mut everywhere = Lut::new();
        for i in 0..2 {
            for j in 0..2 {
                for l in 0..2 {
                    everywhere = everywhere.value([i, j, l], [v, if j == 0 { v } else { w }, v]);
                }
            }
        }
        cases.push(everywhere.case(format!("special {k} ({v:e}) everywhere")));
    }
    let nan = f32::from_bits(0x7fc0_0000);
    let mut nans = Lut::new();
    for i in 0..2 {
        for j in 0..2 {
            for l in 0..2 {
                nans = nans.value([i, j, l], [nan, nan, nan]);
            }
        }
    }
    cases.push(nans.case("NaNs only"));
    // A channel's minimum and maximum keep the first of equal values: `-0` before `0` and `0`
    // before `-0` print as they come.
    let mut zeros = Lut::new();
    for i in 0..2 {
        for j in 0..2 {
            for l in 0..2 {
                let first = (i + j + l) % 2 == 0;
                zeros = zeros.value(
                    [i, j, l],
                    if first {
                        [-0.0, 0.0, -0.0]
                    } else {
                        [0.0, -0.0, -0.0]
                    },
                );
            }
        }
    }
    cases.push(zeros.case("signed zeros in either order"));
    // The direction set through `Transform::setDirection`, the base class's virtual.
    for dir in [Forward, Inverse] {
        let lut = curves(Lut::new(), 3);
        let mut port: Transform = lut.port.clone().into();
        port.set_direction(dir);
        let mut spec = lut.spec();
        spec["calls"]
            .as_array_mut()
            .expect("calls")
            .push(json!(["setDirection", direction_spec(dir)]));
        cases.push(Case::new(
            format!("curves, {dir:?} through the Transform"),
            spec,
            port,
        ));
    }
    cases
}

/// Groups that show a MatrixTransform's precision on the LUTs after it (I-73).
fn group_cases() -> Vec<Case> {
    let lut = Lut::new()
        .value([0, 0, 0], [0.123_456_79, 1.0 / 3.0, -2.0 / 3.0])
        .value([1, 1, 1], [123_456.79, 1.0 / 7.0, 0.1])
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
    let base = Lut::new().grid(3).value([1, 1, 1], [0.25, 0.5, 0.75]);
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
            .interpolation(Interpolation::Linear)
            .case("linear"),
        base.clone()
            .interpolation(Interpolation::Cubic)
            .case("cubic"),
        base.clone()
            .value([1, 1, 1], [0.25, 0.5, 0.75000006])
            .case("another value"),
        base.clone().value([1, 1, 1], [0.25, -0.0, 0.75]).case("-0"),
        base.clone().value([1, 1, 1], [0.25, 0.0, 0.75]).case("+0"),
        base.clone().grid(4).case("another grid size"),
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
            "a grid size over 129",
            json!({"class": "Lut3DTransform", "calls": [["setGridSize", 130]]}),
            l().port.set_grid_size(130),
        ),
        (
            "a constructor's grid size over 129",
            json!({"class": "Lut3DTransform", "args": {"gridSize": 130}}),
            Lut3DTransform::with_grid_size(130).map(|_| ()),
        ),
        (
            "a red index past the end",
            json!({"class": "Lut3DTransform", "calls": [["setValue", 2, 0, 0, 0.0, 0.0, 0.0]]}),
            l().port.set_value(2, 0, 0, 0.0, 0.0, 0.0),
        ),
        (
            "a green index past the end",
            json!({"class": "Lut3DTransform", "calls": [["setValue", 0, 2, 0, 0.0, 0.0, 0.0]]}),
            l().port.set_value(0, 2, 0, 0.0, 0.0, 0.0),
        ),
        (
            "a blue index past the end",
            json!({"class": "Lut3DTransform", "calls": [["setValue", 0, 0, 2, 0.0, 0.0, 0.0]]}),
            l().port.set_value(0, 0, 2, 0.0, 0.0, 0.0),
        ),
        (
            "every index past the end",
            json!({"class": "Lut3DTransform", "calls": [["setValue", 5, 6, 7, 0.0, 0.0, 0.0]]}),
            l().port.set_value(5, 6, 7, 0.0, 0.0, 0.0),
        ),
        (
            "an index far past the end",
            json!({"class": "Lut3DTransform",
                "calls": [["setValue", 0, 0, 4_000_000_000u32, 0.0, 0.0, 0.0]]}),
            l().port.set_value(0, 0, 4_000_000_000, 0.0, 0.0, 0.0),
        ),
    ];
    setter_errors(&checks);
}

/// `getValue` past the end raises the message `setValue` does with its own name: the
/// binding's `getValue(indexR, indexG, indexB)` against the port's.
#[test]
fn get_value_errors_match_the_wheel() {
    let mut checks: Vec<(&str, Value, ocio::Result<()>)> = Vec::new();
    for (label, index) in [
        ("a red index past the end", [3, 0, 0]),
        ("a green index past the end", [0, 3, 0]),
        ("a blue index past the end", [0, 0, 3]),
    ] {
        let spec = json!({"class": "Lut3DTransform",
            "calls": [["setGridSize", 3], ["getValue", index[0], index[1], index[2]]]});
        let mut t = Lut3DTransform::new();
        t.set_grid_size(3).unwrap();
        checks.push((
            label,
            spec,
            t.value(index[0], index[1], index[2]).map(|_| ()),
        ));
    }
    setter_errors(&checks);
}

#[test]
fn processors_match_the_wheel() {
    let mut cases: Vec<Case> = lut_cases();
    cases.push(group(
        "LUTs in a group",
        Forward,
        &[
            curves(Lut::new(), 3).case("curves"),
            Lut::new()
                .interpolation(Interpolation::Tetrahedral)
                .case("tetrahedral"),
        ],
    ));
    check_processors_dirs(&cases, &[Forward, Inverse]);
}

/// A LUT and its inverse, in either order and with either interpolation: the optimizer's
/// pair removal (`OPTIMIZATION_PAIR_IDENTITY_LUT3D`, in the default) replaces them with
/// `Lut3DOpData::getIdentityReplacement`, a [0, 1] range (src/OpenColorIO/ops/lut3d/
/// Lut3DOpData.cpp:411-414 @ v2.5.2), for every bit depth pair the processor can take.
#[test]
fn a_lut_and_its_inverse_optimize_to_a_range() {
    let interps = [Interpolation::Linear, Interpolation::Tetrahedral];
    let mut cases = Vec::new();
    for fwd in interps {
        for inv in interps {
            let lut = |interp, dir| {
                curves(Lut::new().interpolation(interp).dir(dir), 3)
                    .case(format!("{interp:?} {dir:?}"))
            };
            cases.push(group(
                &format!("LUT ({fwd:?}), inverse ({inv:?})"),
                Forward,
                &[lut(fwd, Forward), lut(inv, Inverse)],
            ));
            cases.push(group(
                &format!("inverse ({inv:?}), LUT ({fwd:?})"),
                Forward,
                &[lut(inv, Inverse), lut(fwd, Forward)],
            ));
        }
    }
    check_optimized_processors(
        &cases,
        &[
            (Depth::F32, Depth::F32),
            (Depth::Uint8, Depth::Uint8),
            (Depth::Uint16, Depth::F16),
        ],
    );
}

/// A cube of `grid_size` entries per side, `f` of the identity's values, with `interp`.
fn lut_of(
    grid_size: c_ulong,
    interp: Interpolation,
    dir: TransformDirection,
    f: impl Fn([f32; 3]) -> [f32; 3],
) -> Lut {
    let mut lut = Lut::new().grid(grid_size).interpolation(interp).dir(dir);
    let last = (grid_size - 1) as f32;
    for i in 0..grid_size {
        for j in 0..grid_size {
            for k in 0..grid_size {
                let x = [i, j, k].map(|v| v as f32 / last);
                lut = lut.value([i, j, k], f(x));
            }
        }
    }
    lut
}

/// Smooth and invertible, with some crosstalk.
fn smooth([r, g, b]: [f32; 3]) -> [f32; 3] {
    let curve = |c: f32| 0.06 + 0.88 * (0.6 * c * c + 0.4 * c);
    [
        curve(r) + 0.03 * (g - b),
        curve(g) + 0.03 * (b - r),
        curve(b) + 0.03 * (r - g),
    ]
}

/// Folded: not invertible, and partly outside [0, 1].
fn folded([r, g, b]: [f32; 3]) -> [f32; 3] {
    [1.5 * g - 0.25, (r - b).abs(), 1.0 - r * g]
}

/// Each channel rounded down to a quarter: flat areas.
fn quantized(x: [f32; 3]) -> [f32; 3] {
    x.map(|c| (c * 4.0).floor() / 4.0)
}

/// The optimizer's 3D LUTs, entry for entry (the optimized processors' `getData`, and their
/// cache IDs, which hash the values): an inverse LUT's fast forward LUT
/// (`MakeFastLut3DFromInverse`, src/OpenColorIO/ops/lut3d/Lut3DOpData.cpp:29-58 @ v2.5.2) on
/// the default domain of 48 entries per side and on a larger LUT's own, with its file output
/// bit depth; and compositions of two LUTs (`Lut3DOpData::Compose`, 60-153, through
/// `Lut3DOp::combineWith`): forward, smaller then larger and the reverse, with an inverse first,
/// second or both, at `OPTIMIZATION_GOOD` (the inverse LUTs replaced by their fast forward LUTs
/// first) and at `OPTIMIZATION_LOSSLESS | OPTIMIZATION_COMP_LUT3D` (composed through their exact
/// inverse). The forward LUTs render with the SIMD kernel the CPU dispatches to: this target is
/// in `cpu-tests`.
#[test]
fn optimized_luts_match_the_wheel() {
    use Interpolation::{Default as D, Linear as L, Tetrahedral as T};
    let case = |label: &str, lut: Lut| lut.case(label.to_string());
    let inverses = vec![
        case("smooth 5^3", lut_of(5, T, Inverse, smooth)),
        case("smooth 17^3, linear", lut_of(17, L, Inverse, smooth)),
        case("quantized 9^3", lut_of(9, T, Inverse, quantized)),
        case("folded 4^3", lut_of(4, D, Inverse, folded)),
        case(
            "smooth 5^3, 10-bit file depth",
            lut_of(5, T, Inverse, smooth).depth(BitDepth::Uint10),
        ),
        case("smooth 49^3", lut_of(49, T, Inverse, smooth)),
    ];
    check_optimized_processors(&inverses, &[(Depth::F32, Depth::F32)]);

    let pair =
        |label: &str, a: Lut, b: Lut| group(label, Forward, &[a.case("first"), b.case("second")]);
    let pairs = vec![
        pair(
            "3^3 then 5^3",
            lut_of(3, L, Forward, folded),
            lut_of(5, T, Forward, smooth),
        ),
        pair(
            "5^3 then 3^3",
            lut_of(5, T, Forward, smooth),
            lut_of(3, L, Forward, folded),
        ),
        pair(
            "17^3 twice",
            lut_of(17, T, Forward, smooth),
            lut_of(17, D, Forward, quantized),
        ),
        // Equal sizes keep the first LUT's own values (`lut1->clone()`), NaN and infinities
        // included, which the second LUT's renderer then meets as inputs.
        pair(
            "3^3 with a NaN and an infinity, then 3^3",
            lut_of(3, T, Forward, smooth)
                .value([1, 1, 1], [f32::NAN, 0.5, f32::INFINITY])
                .value([0, 2, 1], [-f32::INFINITY, 0.25, 0.75]),
            lut_of(3, L, Forward, folded),
        ),
        pair(
            "inverse 5^3 then 3^3",
            lut_of(5, T, Inverse, smooth),
            lut_of(3, L, Forward, folded),
        ),
        pair(
            "3^3 then inverse 5^3",
            lut_of(3, L, Forward, folded),
            lut_of(5, T, Inverse, smooth),
        ),
        pair(
            "inverse 4^3 then inverse 6^3",
            lut_of(4, T, Inverse, quantized),
            lut_of(6, L, Inverse, smooth),
        ),
        pair(
            "inverse 6^3 then inverse 4^3",
            lut_of(6, L, Inverse, smooth),
            lut_of(4, T, Inverse, quantized),
        ),
    ];
    let good = OptimizationFlags::GOOD;
    check_optimized_processors_at(
        &pairs,
        &[(Depth::F32, Depth::F32)],
        &json!("OPTIMIZATION_GOOD"),
        good,
    );
    let exact = OptimizationFlags(OptimizationFlags::LOSSLESS.0 | OptimizationFlags::COMP_LUT3D.0);
    check_optimized_processors_at(
        &pairs,
        &[(Depth::F32, Depth::F32)],
        &json!(["OPTIMIZATION_LOSSLESS", "OPTIMIZATION_COMP_LUT3D"]),
        exact,
    );
}
