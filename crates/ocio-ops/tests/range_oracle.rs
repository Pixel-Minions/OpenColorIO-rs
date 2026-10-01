// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Range renderers against the wheel, bit for bit, through the oracle test battery
//! (`ocio_testkit::battery`): every case in both directions, with fast math on and off, on
//! the tier's probe sets (`OCIO_RS_TIER`).
//!
//! The oracle builds a `RangeTransform` in a raw config and applies its CPU processor to F32
//! RGBA pixels. The port builds what the wheel does from it:
//! - the binding's constructor sets the bounds and the direction on the transform's default
//!   data, then validates the transform (src/bindings/python/transforms/PyRangeTransform.cpp:
//!   19-38 @ v2.5.2), whose messages start with `RangeTransform validation failed: `
//!   (src/OpenColorIO/transforms/RangeTransform.cpp:52-72), and the processor of a transform
//!   validates it again (src/OpenColorIO/Processor.cpp:623-633). A config's YAML sets them on
//!   the transform without validating it (src/OpenColorIO/OCIOYaml.cpp:3091-3150), and the
//!   processor between its color spaces doesn't either;
//! - `BuildRangeOp` validates the data, and builds a Range op with a copy of it for the clamping
//!   style, or a Matrix op with `convertToMatrix()` for the other (src/OpenColorIO/ops/range/
//!   RangeOp.cpp:263-281);
//! - the processor finalizes the op, which makes an inverse range forward (`getAsForward`,
//!   which validates the swapped bounds), and the CPU processor renders that one op: at F32,
//!   the optimizer leaves a single Range op alone (a Range identity is never replaced,
//!   `ReplaceIdentityOps`, and the bit-depth steps only apply to integer bit depths,
//!   src/OpenColorIO/OpOptimizers.cpp:758-778). `GetRangeRenderer` or `GetMatrixRenderer`
//!   picks the renderer.
//!
//! An empty bound is a NaN (`RangeOpData::EmptyValue`): JSON leaves its key out, which keeps
//! the transform's default, NaN. Infinite bounds go through YAML. The Range renderers never
//! write alpha; the matrix that the non-clamping style builds does.

use std::hint::black_box;

use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::ops::matrix::matrix_op::create_matrix_op;
use ocio_ops::ops::range::RangeOpData;
use ocio_ops::ops::range::range_op::create_range_op;
use ocio_testkit::battery::params::{A, Case, Channels, Params, Precision, RGB, Slot};
use ocio_testkit::battery::{self, Combo, Direction, Family, Port, Spec, Validation, yaml_number};
use serde_json::{Map, json};

/// The prefix of `RangeTransformImpl::validate`'s messages (RangeTransform.cpp:67-72 @ v2.5.2).
const PREFIX: &str = "RangeTransform validation failed: ";

/// The port's direction for the battery's.
fn port_direction(direction: Direction) -> TransformDirection {
    match direction {
        Direction::Forward => TransformDirection::Forward,
        Direction::Inverse => TransformDirection::Inverse,
    }
}

/// A `RangeTransform`'s parameters: the bounds (NaN for an empty one) and the style.
#[derive(Debug, Clone, PartialEq)]
struct Range {
    /// `[minIn, maxIn, minOut, maxOut]`.
    bounds: [f64; 4],
    /// `RANGE_CLAMP`, or `RANGE_NO_CLAMP`.
    clamp: bool,
}

/// The bounds' names in the binding and in YAML.
const KEYS: [(&str, &str); 4] = [
    ("minInValue", "min_in_value"),
    ("maxInValue", "max_in_value"),
    ("minOutValue", "min_out_value"),
    ("maxOutValue", "max_out_value"),
];

impl Params for Range {
    fn slots(&self) -> Vec<Slot> {
        KEYS.iter()
            .map(|(name, _)| Slot::new(*name, Precision::F64AsF32, RGB))
            .collect()
    }
    fn get(&self, i: usize) -> f64 {
        self.bounds[i]
    }
    fn set(&mut self, i: usize, value: f64) {
        self.bounds[i] = value;
    }
}

impl Range {
    /// Clamping bounds.
    fn clamp(bounds: [f64; 4]) -> Range {
        Range {
            bounds,
            clamp: true,
        }
    }

    /// Non-clamping bounds.
    fn no_clamp(bounds: [f64; 4]) -> Range {
        Range {
            bounds,
            clamp: false,
        }
    }

    /// Whether the spec is JSON: no infinite bound (NaN ones are left out).
    fn json_route(&self) -> bool {
        self.bounds.iter().all(|b| !b.is_infinite())
    }
}

/// An empty bound.
const E: f64 = f64::NAN;

/// RangeTransform.
struct RangeFamily {
    cases: Vec<Case<Range>>,
    bases: Vec<Case<Range>>,
}

impl Family for RangeFamily {
    type Params = Range;

    fn name(&self) -> String {
        "RangeTransform".to_string()
    }
    fn cases(&self) -> Vec<Case<Range>> {
        self.cases.clone()
    }
    fn mutation_bases(&self) -> Vec<Case<Range>> {
        self.bases.clone()
    }
    fn spec(&self, p: &Range, direction: Direction) -> Spec {
        if p.json_route() {
            let mut args = Map::new();
            for ((key, _), bound) in KEYS.iter().zip(p.bounds) {
                if !bound.is_nan() {
                    args.insert(key.to_string(), json!(bound));
                }
            }
            args.insert("direction".into(), direction.oracle_enum());
            let mut spec = json!({"class": "RangeTransform", "args": args});
            if !p.clamp {
                spec["calls"] = json!([["setStyle", {"enum": "RANGE_NO_CLAMP"}]]);
            }
            Spec::Transform(spec)
        } else {
            let mut fields: Vec<String> = KEYS
                .iter()
                .zip(p.bounds)
                .filter(|(_, b)| !b.is_nan())
                .map(|((_, key), b)| format!("{key}: {}", yaml_number(b)))
                .collect();
            if !p.clamp {
                fields.push("style: noClamp".into());
            }
            fields.push(format!("direction: {}", direction.yaml()));
            Spec::Yaml(format!("!<RangeTransform> {{{}}}", fields.join(", ")))
        }
    }
    fn port(&self, p: &Range, combo: &Combo) -> Result<Port, String> {
        // The transform's data: its default, then the bounds and the direction.
        let mut data = RangeOpData::new();
        let [min_in, max_in, min_out, max_out] = p.bounds;
        data.set_min_in_value(min_in);
        data.set_max_in_value(max_in);
        data.set_min_out_value(min_out);
        data.set_max_out_value(max_out);
        data.set_direction(port_direction(combo.direction));
        if p.json_route() {
            // The binding's constructor validates the transform, as clamping (the default
            // style); `setStyle` comes after.
            data.validate()
                .map_err(|e| format!("{PREFIX}{}", e.message()))?;
            // `Config::getProcessor(transform)` validates it again, with its style
            // (`Processor::Impl::setTransform`, src/OpenColorIO/Processor.cpp:623-633), and a
            // non-clamping range needs both bounds; that message is thrown inside the `try`, so
            // it gets the prefix twice (RangeTransform.cpp:52-72 @ v2.5.2).
            if !p.clamp && (data.min_is_empty() || data.max_is_empty()) {
                return Err(format!(
                    "{PREFIX}{PREFIX}non clamping range must have min and max values defined."
                ));
            }
        }

        let built = (|| {
            // BuildRangeOp.
            data.validate()?;
            let mut ops = OpVec::new();
            if p.clamp {
                create_range_op(&mut ops, data, TransformDirection::Forward)?;
            } else {
                create_matrix_op(
                    &mut ops,
                    data.convert_to_matrix()?,
                    TransformDirection::Forward,
                );
            }
            ops.finalize()?;
            let renderer = black_box(&ops[0])
                .get_cpu_op(combo.fast_math)?
                .expect("a Range or Matrix op renders");
            Ok(renderer)
        })();
        let renderer =
            built.map_err(|e: ocio_ops::exception::Exception| e.message().to_string())?;
        Ok(Port::in_place(move |px| renderer.apply(px)))
    }
    fn pass_through(&self, p: &Range, _: &Combo) -> Channels {
        if p.clamp { A } else { [false; 4] }
    }
    fn breakpoints(&self, p: &Range, direction: Direction) -> Vec<f32> {
        // The bounds the clamps compare with: the input ones forward, the output ones inverse.
        let [min_in, max_in, min_out, max_out] = p.bounds;
        let points = match direction {
            Direction::Forward => [min_in, max_in],
            Direction::Inverse => [min_out, max_out],
        };
        points
            .iter()
            .map(|&b| b as f32)
            .filter(|b| b.is_finite())
            .collect()
    }
    fn validation(&self) -> Validation {
        Validation::Ported
    }
}

/// Wraps the bases' cases as explicit ones, compared exactly: a NaN bound is an empty one,
/// which no renderer computes with.
fn cases(list: Vec<(&str, Range)>) -> Vec<Case<Range>> {
    list.into_iter()
        .map(|(label, p)| Case::new(label, p).w0002_nowhere())
        .collect()
}

#[test]
fn range_transform_matches_the_wheel() {
    let bases = cases(vec![
        ("scale clamp", Range::clamp([0.1, 0.9, 0.2, 0.7])),
        ("min max clamp", Range::clamp([1.0, 2.0, 1.0, 2.0])),
        ("min clamp", Range::clamp([-0.1, E, -0.1, E])),
        ("max clamp", Range::clamp([E, 1.1, E, 1.1])),
        ("no clamp", Range::no_clamp([0.0, 0.5, 0.5, 1.5])),
    ]);
    let mut explicit = cases(vec![
        // tests/cpu/ops/range/RangeOpCPU_tests.cpp, RangeOp_tests.cpp @ v2.5.2.
        ("upstream 0 1 0.5 1.5", Range::clamp([0.0, 1.0, 0.5, 1.5])),
        ("upstream 0 1 0 1.5", Range::clamp([0.0, 1.0, 0.0, 1.5])),
        ("upstream 0 1 1 2", Range::clamp([0.0, 1.0, 1.0, 2.0])),
        ("upstream 0 1.5 0 1", Range::clamp([0.0, 1.5, 0.0, 1.0])),
        (
            "upstream arbitrary",
            Range::clamp([-0.101, 0.95, 0.194, 1.001]),
        ),
        ("identity", Range::clamp([0.0, 1.0, 0.0, 1.0])),
        ("clamp negatives", Range::clamp([0.0, E, 0.0, E])),
        ("constant", Range::clamp([0.0, 1.0, 0.5, 0.5])),
        ("zero bounds", Range::clamp([-0.0, 0.0 + 1e-3, 0.0, -0.0])),
        ("scale is zero", Range::clamp([0.0, 1.0, 0.25, 0.25])),
        // Bounds past the float range: infinite float bounds.
        ("float overflow", Range::clamp([-1e39, 1e39, -1e39, 1e39])),
        // A NaN scale and offset: the differences overflow.
        ("NaN scale", Range::clamp([-1e308, 1e308, -1e308, 1e308])),
        ("NaN offset", Range::clamp([-1e308, 1e308, 0.0, 1.0])),
        ("no clamp identity", Range::no_clamp([0.0, 1.0, 0.0, 1.0])),
        ("no clamp one-sided", Range::no_clamp([0.0, E, 0.0, E])),
        ("refused", Range::clamp([0.5, 0.5 + 1e-7, 0.0, 1.0])),
        ("refused one-sided", Range::clamp([0.25, E, 0.5, E])),
    ]);
    // Infinite bounds take the YAML spec, as the generated ±Inf cases do: an explicit case
    // there makes a bug in that spec fail rather than show as refusals.
    explicit.extend(cases(vec![(
        "infinite input bounds",
        Range::clamp([f64::NEG_INFINITY, f64::INFINITY, 0.0, 1.0]),
    )]));
    explicit.extend(bases.clone());
    battery::run(&RangeFamily {
        cases: explicit,
        bases,
    });
}
