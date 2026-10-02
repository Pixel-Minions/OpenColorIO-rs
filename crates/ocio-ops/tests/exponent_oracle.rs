// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Exponent op's CPU processor against the wheel, bit for bit, through the oracle test
//! battery (`ocio_testkit::battery`): every case in both directions, with fast math on and
//! off, on the tier's probe sets (`OCIO_RS_TIER`).
//!
//! Only a version 1 config builds an Exponent op: its `ExponentTransform` becomes
//! `CreateExponentOp(ops, data, CombineTransformDirections(dir, transform's direction))`
//! (`BuildExponentOp`, src/OpenColorIO/ops/gamma/GammaOp.cpp:190-215 @ v2.5.2), where a
//! version 2 config's becomes a Gamma op. So every case reaches the wheel as the `from_reference`
//! of a colour space in a version 1 config ([`Spec::YamlV1`]), and the processor from the raw
//! colour space to it runs the op forward, or inverse for an inverse transform.
//!
//! The port builds the same ops and the CPU processor of the default flags (fast math on), or
//! of the default flags without `OPTIMIZATION_FAST_LOG_EXP_POW` (off), F32 to F32
//! (`CpuProcessor`), which finalizes and optimizes them: exponents all 1 leave no op, and the
//! processor then renders an identity matrix. The renderer is the same with fast math on or
//! off (`ExponentOp::getCPUOp` ignores the flag).
//!
//! Every channel is computed, alpha included, so none passes through; there is one scalar
//! profile.

use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::image_desc::PackedImageDesc;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{BitDepth, OptimizationFlags, TransformDirection};
use ocio_ops::ops::exponent::ExponentOpData;
use ocio_ops::ops::exponent::exponent_op::create_exponent_op;
use ocio_testkit::battery::params::{Case, Params, Precision, Slot};
use ocio_testkit::battery::{self, Combo, Direction, Family, Port, Spec, Validation, yaml_list};

/// An ExponentTransform's values.
#[derive(Debug, Clone, PartialEq)]
struct Exponents {
    value: [f64; 4],
}

impl Params for Exponents {
    fn slots(&self) -> Vec<Slot> {
        Slot::rgba("value", Precision::F64AsF32).to_vec()
    }
    fn get(&self, i: usize) -> f64 {
        self.value[i]
    }
    fn set(&mut self, i: usize, value: f64) {
        self.value[i] = value;
    }
}

/// The processor's ops: `BuildExponentOp` (GammaOp.cpp:196-206 @ v2.5.2) makes the op data
/// of the transform's values and calls `CreateExponentOp` in the combined direction, which
/// for the colour space's forward transform is the transform's own; then the processor
/// finalizes the ops (src/OpenColorIO/Processor.cpp:618-641).
fn raw_ops(p: &Exponents, direction: Direction) -> Result<OpVec, String> {
    let dir = match direction {
        Direction::Forward => TransformDirection::Forward,
        Direction::Inverse => TransformDirection::Inverse,
    };
    let mut ops = OpVec::new();
    create_exponent_op(
        &mut ops,
        ExponentOpData::from_values(std::hint::black_box(&p.value)),
        dir,
    )
    .map_err(|e| e.message().to_string())?;
    ops.finalize().map_err(|e| e.message().to_string())?;
    Ok(ops)
}

/// The battery's flags: `OPTIMIZATION_DEFAULT`, without `OPTIMIZATION_FAST_LOG_EXP_POW` when
/// fast math is off.
fn flags(combo: &Combo) -> OptimizationFlags {
    if combo.fast_math {
        OptimizationFlags::DEFAULT
    } else {
        OptimizationFlags(OptimizationFlags::DEFAULT.0 & !OptimizationFlags::FAST_LOG_EXP_POW.0)
    }
}

/// ExponentTransform in a version 1 config.
struct ExponentFamily {
    cases: Vec<Case<Exponents>>,
    bases: Vec<Case<Exponents>>,
}

impl Family for ExponentFamily {
    type Params = Exponents;

    fn name(&self) -> String {
        "ExponentTransform (version 1 config)".to_string()
    }
    fn cases(&self) -> Vec<Case<Exponents>> {
        self.cases.clone()
    }
    fn mutation_bases(&self) -> Vec<Case<Exponents>> {
        self.bases.clone()
    }
    fn spec(&self, p: &Exponents, direction: Direction) -> Spec {
        Spec::YamlV1(format!(
            "!<ExponentTransform> {{value: {}, direction: {}}}",
            yaml_list(&p.value),
            direction.yaml()
        ))
    }
    fn port(&self, p: &Exponents, combo: &Combo) -> Result<Port, String> {
        let raw = raw_ops(p, combo.direction)?;
        let cpu = CpuProcessor::new(&raw, BitDepth::F32, BitDepth::F32, flags(combo))
            .map_err(|e| e.message().to_string())?;
        Ok(Port::in_place(move |px| {
            let width = px.len() / 4;
            let mut img = PackedImageDesc::new(px, width, 1, 4).expect("an RGBA row");
            cpu.apply(&mut img).expect("the CPU processor applies");
        }))
    }
    fn validation(&self) -> Validation {
        // ExponentOpData::validate checks nothing (ExponentOp.cpp:92-94 @ v2.5.2); the only
        // refusal is CreateExponentOp's of a 0 exponent in the inverse, which the port makes.
        Validation::Ported
    }
}

/// Upstream's values (tests/cpu/ops/exponent/ExponentOp_tests.cpp @ v2.5.2: `value`,
/// `value_limits`, `combining`, `throw_create`, `cache_id`), a 0 exponent in each channel
/// (refused in the inverse), an exponent just past 1 (the identity test, in float), tiny
/// exponents around the float zero test, and NaN and infinite exponents.
#[test]
fn exponent_transform_matches_the_wheel() {
    let case = |label: &str, value: [f64; 4]| Case::new(label, Exponents { value });
    let mut cases = vec![
        case("value", [1.2, 1.3, 1.4, 1.5]),
        case("value_limits", [0.0, 2.0, -2.0, 1.5]),
        case("combining 1", [2.0, 2.0, 2.0, 1.0]),
        case("combining 2", [1.2, 1.2, 1.2, 1.0]),
        case("combining 3", [1.037289, 1.019015, 0.966082, 1.0]),
        case("throw_create", [0.0, 1.3, 1.4, 1.5]),
        case("cache_id 1", [2.0, 2.1, 3.0, 3.1]),
        case("cache_id 2", [4.0, 4.1, 5.0, 5.1]),
        case("identity", [1.0, 1.0, 1.0, 1.0]),
        case("near identity", [1.0000002, 1.0, 1.0, 1.0]),
        case("zero alpha", [2.2, 2.2, 2.2, 0.0]),
        case("float zero", [1e-46, 2.0, 2.0, 1.0]),
        case("tiny", [5e-45, 1e-300, 2.0, 1.0]),
        case("infinite", [f64::INFINITY, f64::NEG_INFINITY, 2.0, 1.0]),
    ];
    let bases = vec![cases[0].clone(), cases[1].clone()];
    // The one route (YAML in a version 1 config) needs an explicit case the wheel accepts with
    // a NaN parameter, as the generated cases have.
    cases.push(case("NaN", [f64::NAN, 2.0, 2.0, 1.0]));
    battery::run(&ExponentFamily { cases, bases });
}

/// The CPU processor's cache ID for lists of ExponentTransforms (and a MatrixTransform
/// between them) in a version 1 config, at every optimization level: the optimizer combines
/// neighbours into the product of their exponents (`ExponentOp::combineWith`, in `double`),
/// drops a product of ones, and the cache ID names the ops it keeps
/// (`CPUProcessor::Impl::finalize`, src/OpenColorIO/CPUProcessor.cpp:341-377 @ v2.5.2). The
/// last two lists differ past the cache ID's 7 digits (docs/improvements.md, I-55).
#[test]
fn processors_of_exponent_lists_match_the_wheel() {
    use ocio_ops::ops::matrix::MatrixOpData;
    use ocio_ops::ops::matrix::matrix_op::create_matrix_op;
    use ocio_testkit::Oracle;
    use ocio_testkit::oracle::{BatchCall, f32_to_bytes};
    use serde_json::json;

    #[derive(Debug, Clone, Copy)]
    enum Item {
        Exponent([f64; 4], Direction),
        Scale(f64),
    }
    use Direction::{Forward, Inverse};
    use Item::{Exponent, Scale};
    let a = [1.037289, 1.019015, 0.966082, 1.0];
    let b = [2.0, 2.1, 3.0, 3.1];
    let lists: Vec<Vec<Item>> = vec![
        vec![Exponent(a, Forward)],
        vec![Exponent(a, Forward), Exponent(a, Inverse)],
        vec![Exponent(a, Forward), Exponent(b, Forward)],
        vec![
            Exponent(a, Forward),
            Exponent(a, Forward),
            Exponent(a, Forward),
        ],
        vec![
            Exponent(b, Forward),
            Exponent(b, Inverse),
            Exponent(a, Inverse),
        ],
        vec![Exponent([1.0; 4], Forward)],
        vec![Exponent(a, Forward), Scale(2.0), Exponent(a, Inverse)],
        vec![Exponent([2.0000001, 2.0, 2.0, 1.0], Forward)],
        vec![Exponent([2.0000003, 2.0, 2.0, 1.0], Forward)],
    ];
    let levels = [
        ("OPTIMIZATION_NONE", OptimizationFlags::NONE),
        ("OPTIMIZATION_LOSSLESS", OptimizationFlags::LOSSLESS),
        ("OPTIMIZATION_DEFAULT", OptimizationFlags::DEFAULT),
        ("OPTIMIZATION_ALL", OptimizationFlags::ALL),
    ];
    let yaml = |items: &[Item]| -> String {
        let children: Vec<String> = items
            .iter()
            .map(|item| match item {
                Exponent(v, dir) => format!(
                    "!<ExponentTransform> {{value: {}, direction: {}}}",
                    yaml_list(v),
                    dir.yaml()
                ),
                Scale(s) => format!(
                    "!<MatrixTransform> {{matrix: {}}}",
                    yaml_list(&[
                        *s, 0., 0., 0., 0., *s, 0., 0., 0., 0., *s, 0., 0., 0., 0., 1.
                    ])
                ),
            })
            .collect();
        format!("!<GroupTransform> {{children: [{}]}}", children.join(", "))
    };
    let pixel = f32_to_bytes(&[0.5, 0.25, 0.75, 1.0]);
    let mut cases = Vec::new();
    for items in &lists {
        for level in &levels {
            cases.push((items, level));
        }
    }
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(items, (name, _))| {
            let mut args = Spec::YamlV1(yaml(items)).cpu_apply_args(&Combo {
                direction: Forward,
                fast_math: true,
                format: battery::Format::F32_RGBA,
            });
            args["optimization"] = json!(name);
            BatchCall {
                cmd: "cpu_apply",
                args,
                blobs: vec![pixel.as_slice()],
            }
        })
        .collect();
    let mut failures = Vec::new();
    for ((items, (name, flags)), response) in cases.iter().zip(Oracle::get().batch(&calls, true)) {
        let result = response.expect("the oracle").result;
        let wheel = result["cpu_cache_id"]
            .as_str()
            .unwrap_or_else(|| panic!("{result}"));
        let mut raw = OpVec::new();
        for item in items.iter() {
            match item {
                Exponent(v, dir) => {
                    let dir = match dir {
                        Forward => TransformDirection::Forward,
                        Inverse => TransformDirection::Inverse,
                    };
                    create_exponent_op(&mut raw, ExponentOpData::from_values(v), dir).unwrap();
                }
                Scale(s) => {
                    // BuildMatrixOp: the transform's data, validated (MatrixOp.cpp:395-404).
                    let mut data = MatrixOpData::create_diagonal_matrix(*s);
                    data.set_array_value(15, 1.0);
                    data.validate().unwrap();
                    create_matrix_op(&mut raw, data, TransformDirection::Forward);
                }
            }
        }
        raw.finalize().unwrap();
        let cpu = CpuProcessor::new(&raw, BitDepth::F32, BitDepth::F32, *flags).unwrap();
        let port = String::from_utf8(cpu.get_cache_id().to_vec()).unwrap();
        if port != wheel {
            failures.push(format!("{items:?} {name}\n  wheel {wheel}\n  port  {port}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
