// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `GradingPrimaryOpData` against the wheel, through processors of a `GradingPrimaryTransform`
//! (`oracle processor_ops`):
//! - the cache ID: the processor holds one GradingPrimary op, so its cache ID is the hash of
//!   its ops' cache ID, `" <GradingPrimaryOp " + data cache ID + ">"` (Processor.cpp:331-348,
//!   Op.cpp:448-465, GradingPrimaryOp.cpp:125-134 @ v2.5.2); the data's text (the style, the
//!   direction, the values unless dynamic, 7 significant digits) is compared through its hash;
//! - `isNoOp` (`isIdentity`) and `hasChannelCrosstalk` of the processor, which are its op's;
//! - the op data the processor built, in either direction: `createGroupTransform()` copies it
//!   into a transform (`CreateGradingPrimaryTransform`, GradingPrimaryOp.cpp:223-236), whose
//!   getters give the style, the direction, the values and the dynamic flag;
//! - the identity replacement (`GradingPrimaryOpData::getIdentityReplacement`,
//!   GradingPrimaryOpData.cpp:128-154): the optimized processor (`OPTIMIZATION_DEFAULT`)
//!   removes an op that is an identity (its clamps are the defaults, so the replacement is the
//!   identity matrix), and replaces a pair of an op and its inverse with the Range of their
//!   clamps, or nothing (`RemoveInverseOps`, OpOptimizers.cpp:207-300);
//! - the values the transform refuses: the binding's constructor sets them, and `setValue`
//!   validates them for the style.
//!
//! The binding's constructor creates the transform for the style, sets the values, makes it
//! dynamic, then sets the direction (src/bindings/python/transforms/
//! PyGradingPrimaryTransform.cpp:17-35), and the processor builds its op from a copy of the
//! transform's data, inverted for an inverse processor (GradingPrimaryOp.cpp:208-219, 238-249).

use ocio_ops::hash_utils::cache_id_hash;
use ocio_ops::op::OpVec;
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::{GradingStyle, TransformDirection};
use ocio_ops::ops::gradingprimary::grading_primary::{GradingPrimary, GradingRgbm};
use ocio_ops::ops::gradingprimary::grading_primary_op::create_grading_primary_op;
use ocio_ops::ops::gradingprimary::grading_primary_op_data::GradingPrimaryOpData;
use ocio_testkit::probe::Rng;
use ocio_testkit::processor_ops::{Dump, Dumped, ProcessorOpsReply, ProcessorOpsRequest};
use ocio_testkit::transform_text::f64_spec;
use serde_json::{Value, json};

/// Doubles the cache ID prints: zeros, values that need all 7 significant digits or round in
/// the 7th, the tiniest and largest doubles, and the non-finite ones.
const VALUES: [f64; 24] = [
    0.0,
    -0.0,
    1.0,
    0.1,
    -1.4,
    0.18,
    0.088,
    0.435,
    1234567.8,
    9999999.5,
    0.000123456789,
    123.456789,
    1.23456789e20,
    -2.5e-300,
    1e-7,
    0.99999995,
    5e-324,
    f64::MAX,
    f64::MIN_POSITIVE,
    f64::NAN,
    -f64::NAN,
    f64::INFINITY,
    f64::NEG_INFINITY,
    2.0,
];

const STYLES: [GradingStyle; 3] = [GradingStyle::Log, GradingStyle::Lin, GradingStyle::Video];
const DIRS: [TransformDirection; 2] = [TransformDirection::Forward, TransformDirection::Inverse];

fn style_name(style: GradingStyle) -> &'static str {
    match style {
        GradingStyle::Log => "GRADING_LOG",
        GradingStyle::Lin => "GRADING_LIN",
        GradingStyle::Video => "GRADING_VIDEO",
    }
}

fn dir_name(dir: TransformDirection) -> &'static str {
    match dir {
        TransformDirection::Forward => "TRANSFORM_DIR_FORWARD",
        TransformDirection::Inverse => "TRANSFORM_DIR_INVERSE",
    }
}

fn rgbm_spec(v: &GradingRgbm) -> Value {
    json!({"object": {"class": "GradingRGBM",
                      "args": [f64_spec(v.red), f64_spec(v.green), f64_spec(v.blue),
                               f64_spec(v.master)]}})
}

fn primary_spec(p: &GradingPrimary) -> Value {
    json!({"object": {"class": "GradingPrimary", "args": [{"enum": "GRADING_LOG"}], "attrs": [
        ["brightness", rgbm_spec(&p.brightness)],
        ["contrast", rgbm_spec(&p.contrast)],
        ["gamma", rgbm_spec(&p.gamma)],
        ["offset", rgbm_spec(&p.offset)],
        ["exposure", rgbm_spec(&p.exposure)],
        ["lift", rgbm_spec(&p.lift)],
        ["gain", rgbm_spec(&p.gain)],
        ["saturation", f64_spec(p.saturation)],
        ["pivot", f64_spec(p.pivot)],
        ["pivotBlack", f64_spec(p.pivot_black)],
        ["pivotWhite", f64_spec(p.pivot_white)],
        ["clampBlack", f64_spec(p.clamp_black)],
        ["clampWhite", f64_spec(p.clamp_white)],
    ]}})
}

/// One processor: the transform's parameters and the processor's direction.
#[derive(Debug, Clone)]
struct GpCase {
    style: GradingStyle,
    values: GradingPrimary,
    dynamic: bool,
    transform_dir: TransformDirection,
    processor_dir: TransformDirection,
}

impl GpCase {
    fn processor(&self) -> Value {
        json!({
            "transform": {"class": "GradingPrimaryTransform", "args": {
                "values": primary_spec(&self.values),
                "style": {"enum": style_name(self.style)},
                "dynamic": self.dynamic,
                "dir": {"enum": dir_name(self.transform_dir)},
            }},
            "direction": dir_name(self.processor_dir),
        })
    }

    /// The op data the processor holds, built as the wheel builds it, or the exception of
    /// the values the transform refuses.
    fn port(&self) -> Result<GradingPrimaryOpData, String> {
        // GradingPrimaryTransform::Create(style), setValue, makeDynamic, setDirection.
        let mut data = GradingPrimaryOpData::new(self.style);
        data.set_value(&self.values).map_err(|e| e.to_string())?;
        if self.dynamic {
            data.get_dynamic_property_internal().make_dynamic();
        }
        data.set_direction(self.transform_dir);
        data.validate().map_err(|e| e.to_string())?;
        // BuildGradingPrimaryOp: a clone, inverted for an inverse processor.
        let built = data.clone();
        Ok(match self.processor_dir {
            TransformDirection::Forward => built,
            TransformDirection::Inverse => built.inverse(),
        })
    }
}

/// The fields of a GradingPrimary, by index: the 28 RGBM components, then the 6 doubles.
fn field(p: &mut GradingPrimary, index: usize) -> &mut f64 {
    fn rgbm(v: &mut GradingRgbm, c: usize) -> &mut f64 {
        match c {
            0 => &mut v.red,
            1 => &mut v.green,
            2 => &mut v.blue,
            _ => &mut v.master,
        }
    }
    match index {
        0..=3 => rgbm(&mut p.brightness, index),
        4..=7 => rgbm(&mut p.contrast, index - 4),
        8..=11 => rgbm(&mut p.gamma, index - 8),
        12..=15 => rgbm(&mut p.offset, index - 12),
        16..=19 => rgbm(&mut p.exposure, index - 16),
        20..=23 => rgbm(&mut p.lift, index - 20),
        24..=27 => rgbm(&mut p.gain, index - 24),
        28 => &mut p.saturation,
        29 => &mut p.pivot,
        30 => &mut p.pivot_black,
        31 => &mut p.pivot_white,
        32 => &mut p.clamp_black,
        _ => &mut p.clamp_white,
    }
}

const NUM_FIELDS: usize = 34;

fn cases() -> Vec<GpCase> {
    let mut cases = Vec::new();
    let mut rng = Rng::new(0x9a1d_5e7a);
    let mut k = 0usize;
    for style in STYLES {
        for dynamic in [false, true] {
            for transform_dir in DIRS {
                for processor_dir in DIRS {
                    // The defaults, then values with a few fields from VALUES.
                    for n in 0..8 {
                        let mut values = GradingPrimary::new(style);
                        for _ in 0..n {
                            let f = (rng.next_u64() % NUM_FIELDS as u64) as usize;
                            *field(&mut values, f) = VALUES[k % VALUES.len()];
                            k += 1;
                        }
                        cases.push(GpCase {
                            style,
                            values,
                            dynamic,
                            transform_dir,
                            processor_dir,
                        });
                    }
                }
            }
        }
    }
    // Every value in every field, not dynamic, forward: the defaults elsewhere.
    for f in 0..NUM_FIELDS {
        for (i, &value) in VALUES.iter().enumerate() {
            let style = STYLES[(f + i) % 3];
            let mut values = GradingPrimary::new(style);
            *field(&mut values, f) = value;
            cases.push(GpCase {
                style,
                values,
                dynamic: false,
                transform_dir: TransformDirection::Forward,
                processor_dir: TransformDirection::Forward,
            });
        }
    }
    cases
}

fn getter_f64(dumped: &Dumped) -> f64 {
    match dumped {
        Dumped::F64(v) => *v,
        other => panic!("not a float: {other:?}"),
    }
}

/// The wheel's GradingRGBM, written out, as the port's.
fn rgbm_of(dump: &Dump) -> GradingRgbm {
    GradingRgbm::new(
        getter_f64(dump.property("red")),
        getter_f64(dump.property("green")),
        getter_f64(dump.property("blue")),
        getter_f64(dump.property("master")),
    )
}

/// The bits of every value, so that NaNs compare.
fn bits(p: &GradingPrimary) -> Vec<u64> {
    let mut p = *p;
    (0..NUM_FIELDS)
        .map(|f| field(&mut p, f).to_bits())
        .collect()
}

/// The wheel's GradingPrimary, written out, as the port's.
fn primary_of(dump: &Dump) -> GradingPrimary {
    let rgbm = |name: &str| rgbm_of(dump.property(name).object());
    let f = |name: &str| getter_f64(dump.property(name));
    GradingPrimary {
        brightness: rgbm("brightness"),
        contrast: rgbm("contrast"),
        gamma: rgbm("gamma"),
        offset: rgbm("offset"),
        exposure: rgbm("exposure"),
        lift: rgbm("lift"),
        gain: rgbm("gain"),
        saturation: f("saturation"),
        pivot: f("pivot"),
        pivot_black: f("pivotBlack"),
        pivot_white: f("pivotWhite"),
        clamp_black: f("clampBlack"),
        clamp_white: f("clampWhite"),
    }
}

/// The processor's cache ID, flags and the op data it built are the wheel's; so are the values
/// the transform refuses, and whether the optimized processor removes the op.
#[test]
fn cache_ids_and_built_data_are_the_wheels() {
    let cases = cases();
    let requests: Vec<ProcessorOpsRequest> = cases
        .iter()
        .map(|case| ProcessorOpsRequest::new(case.processor()))
        .collect();
    let calls: Vec<_> = requests.iter().map(|r| r.call()).collect();
    let responses = ocio_testkit::Oracle::get().batch(&calls, true);
    assert_eq!(responses.len(), cases.len());
    let (mut refused, mut removed, mut kept) = (0, 0, 0);
    for (case, response) in cases.iter().zip(responses) {
        let reply = ProcessorOpsReply::from_response(response.expect("the oracle"));
        let data = match case.port() {
            Ok(data) => data,
            Err(text) => {
                let raised = reply
                    .raised()
                    .unwrap_or_else(|| panic!("{case:?}: the wheel accepts it, the port: {text}"));
                assert_eq!(raised.message, text, "{case:?}");
                refused += 1;
                continue;
            }
        };
        assert!(reply.raised().is_none(), "{case:?}: {:?}", reply.result);
        let wheel = reply.processor();

        // The cache ID.
        let mut ops = OpVec::new();
        create_grading_primary_op(&mut ops, data.clone(), TransformDirection::Forward);
        let full = ops.get_cache_id().unwrap();
        assert_eq!(
            cache_id_hash(&full),
            wheel.cache_id,
            "{case:?}: {}",
            String::from_utf8_lossy(&full)
        );
        assert_eq!(wheel.is_no_op, data.is_no_op(), "{case:?}");
        assert_eq!(
            wheel.has_channel_crosstalk,
            data.has_channel_crosstalk(),
            "{case:?}"
        );
        assert_eq!(wheel.is_dynamic, data.is_dynamic(), "{case:?}");

        // The data, through the group transform's getters.
        assert_eq!(wheel.group.children.len(), 1, "{case:?}");
        let t = &wheel.group.children[0];
        assert_eq!(t.class, "GradingPrimaryTransform", "{case:?}");
        assert_eq!(
            t.getter("getStyle"),
            &Dumped::Enum(style_name(data.get_style()).to_string()),
            "{case:?}"
        );
        assert_eq!(
            t.getter("getDirection"),
            &Dumped::Enum(dir_name(data.get_direction()).to_string()),
            "{case:?}"
        );
        assert_eq!(
            t.getter("isDynamic"),
            &Dumped::Bool(data.is_dynamic()),
            "{case:?}"
        );
        let wheel_values = primary_of(t.getter("getValue").object());
        assert_eq!(bits(&wheel_values), bits(&data.get_value()), "{case:?}");

        // The optimized processor: an identity is removed (its replacement, without clamps, is
        // the identity matrix), any other op kept.
        let optimized = reply.optimized();
        if data.is_identity() {
            assert!(
                matches!(data.get_identity_replacement().unwrap(), OpData::Matrix(_)),
                "{case:?}"
            );
            assert!(optimized.group.children.is_empty(), "{case:?}");
            removed += 1;
        } else {
            assert_eq!(optimized.group.children.len(), 1, "{case:?}");
            assert_eq!(
                optimized.group.children[0].class, "GradingPrimaryTransform",
                "{case:?}"
            );
            kept += 1;
        }
    }
    // Each kind of case is there.
    assert!(refused > 20, "{refused} refused");
    assert!(removed > 20, "{removed} removed");
    assert!(kept > 100, "{kept} kept");
}

/// A non-dynamic op and its inverse, in a group: the optimizer replaces the pair with the
/// first op's identity replacement, the Range of its clamps or nothing, or raises the Range's
/// refusal (equal clamps); values that don't equal themselves (a NaN) make no pair, and both
/// ops stay.
#[test]
fn inverse_pairs_become_their_clamps() {
    let clamps = [
        (
            GradingPrimary::NO_CLAMP_BLACK,
            GradingPrimary::NO_CLAMP_WHITE,
        ),
        (0.1, GradingPrimary::NO_CLAMP_WHITE),
        (GradingPrimary::NO_CLAMP_BLACK, 0.9),
        (-0.25, 1.5),
        (0.5, 0.5),
        (f64::NAN, 1.0),
        (0.0, f64::NAN),
        (f64::NEG_INFINITY, f64::INFINITY),
        (-f64::MAX, 2.0),
        (1e-300, f64::MAX),
        (-1e-300, 0.0),
    ];
    let mut cases = Vec::new();
    for style in STYLES {
        for dir in DIRS {
            for (black, white) in clamps {
                let mut values = GradingPrimary::new(style);
                values.clamp_black = black;
                values.clamp_white = white;
                values.saturation = 0.8;
                values.contrast.green = 1.3;
                values.gamma.master = 1.1;
                values.pivot_black = 0.05;
                cases.push((style, dir, values));
            }
        }
    }
    let requests: Vec<ProcessorOpsRequest> = cases
        .iter()
        .map(|(style, dir, values)| {
            let transform = |d: TransformDirection| {
                json!({"class": "GradingPrimaryTransform", "args": {
                    "values": primary_spec(values),
                    "style": {"enum": style_name(*style)},
                    "dir": {"enum": dir_name(d)},
                }})
            };
            let inverse = match dir {
                TransformDirection::Forward => TransformDirection::Inverse,
                TransformDirection::Inverse => TransformDirection::Forward,
            };
            ProcessorOpsRequest::new(json!({"transform": {"class": "GroupTransform",
                "children": [transform(*dir), transform(inverse)]}}))
        })
        .collect();
    let calls: Vec<_> = requests.iter().map(|r| r.call()).collect();
    let responses = ocio_testkit::Oracle::get().batch(&calls, true);
    assert_eq!(responses.len(), cases.len());
    let (mut replaced, mut removed, mut kept, mut refused) = (0, 0, 0, 0);
    for (case, response) in cases.iter().zip(responses) {
        let reply = ProcessorOpsReply::from_response(response.expect("the oracle"));
        let (style, dir, values) = case;
        let make = |d: TransformDirection| {
            let mut data = GradingPrimaryOpData::new(*style);
            data.set_value(values).unwrap();
            data.set_direction(d);
            data
        };
        let first = make(*dir);
        let second = first.inverse();
        if first.is_inverse(&second) {
            let replacement = match first.get_identity_replacement() {
                Ok(replacement) => replacement,
                Err(e) => {
                    // The Range refuses the clamps: the optimizer raises.
                    let raised = reply.raised().unwrap_or_else(|| panic!("{case:?}: {e}"));
                    assert_eq!(raised.message, e.to_string(), "{case:?}");
                    assert_eq!(reply.result["stage"], "optimize", "{case:?}");
                    refused += 1;
                    continue;
                }
            };
            assert!(reply.raised().is_none(), "{case:?}: {:?}", reply.result);
            let optimized = reply.optimized();
            match replacement {
                OpData::Matrix(_) => {
                    assert!(optimized.group.children.is_empty(), "{case:?}");
                    removed += 1;
                }
                OpData::Range(range) => {
                    assert_eq!(optimized.group.children.len(), 1, "{case:?}");
                    let r = &optimized.group.children[0];
                    assert_eq!(r.class, "RangeTransform", "{case:?}");
                    let bounds = [
                        ("getMinInValue", range.get_min_in_value()),
                        ("getMaxInValue", range.get_max_in_value()),
                        ("getMinOutValue", range.get_min_out_value()),
                        ("getMaxOutValue", range.get_max_out_value()),
                    ];
                    for (getter, port) in bounds {
                        assert_eq!(
                            getter_f64(r.getter(getter)).to_bits(),
                            port.to_bits(),
                            "{case:?}: {getter}"
                        );
                    }
                    replaced += 1;
                }
                other => panic!("{case:?}: {other:?}"),
            }
        } else {
            assert!(reply.raised().is_none(), "{case:?}: {:?}", reply.result);
            assert_eq!(
                reply.optimized().classes(),
                ["GradingPrimaryTransform"; 2],
                "{case:?}"
            );
            kept += 1;
        }
    }
    assert!(replaced >= 30, "{replaced} replaced by a Range");
    assert!(removed >= 6, "{removed} removed");
    assert!(kept >= 12, "{kept} kept");
    assert!(refused >= 6, "{refused} refused");
}
