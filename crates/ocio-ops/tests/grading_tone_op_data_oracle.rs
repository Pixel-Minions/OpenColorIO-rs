// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `GradingToneOpData` against the wheel, through processors of a `GradingToneTransform`
//! (`oracle processor_ops`):
//! - the cache ID: the processor holds one GradingTone op, so its cache ID is the hash of its
//!   ops' cache ID, `" <GradingToneOp " + data cache ID + ">"` (Processor.cpp:331-348,
//!   Op.cpp:448-465, GradingToneOp.cpp:124-133 @ v2.5.2); the data's text (the style, the
//!   direction, the values unless dynamic, 7 significant digits) is compared through its hash;
//! - `isNoOp` (`isIdentity`) and `hasChannelCrosstalk` of the processor, which are its op's;
//! - the op data the processor built, in either direction: `createGroupTransform()` copies it
//!   into a transform (`CreateGradingToneTransform`, GradingToneOp.cpp:217-230), whose getters
//!   give the style, the direction, the values and the dynamic flag;
//! - the identity replacement: GradingToneOpData doesn't override `getIdentityReplacement`, so
//!   the optimized processor (`OPTIMIZATION_DEFAULT`) removes an op that is an identity, and a
//!   pair of an op and its inverse (`RemoveInverseOps`, OpOptimizers.cpp:207-300);
//! - the values the transform refuses: the binding's constructor sets them, and `setValue`
//!   validates them.
//!
//! The binding's constructor creates the transform for the style, sets the values, makes it
//! dynamic, then sets the direction (src/bindings/python/transforms/
//! PyGradingToneTransform.cpp:17-35), and the processor builds its op from a copy of the
//! transform's data, inverted for an inverse processor (GradingToneOp.cpp:202-213, 232-244).

use ocio_ops::hash_utils::cache_id_hash;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{GradingStyle, TransformDirection};
use ocio_ops::ops::gradingtone::grading_tone::{GradingRgbmsw, GradingTone};
use ocio_ops::ops::gradingtone::grading_tone_op::create_grading_tone_op;
use ocio_ops::ops::gradingtone::grading_tone_op_data::GradingToneOpData;
use ocio_testkit::probe::Rng;
use ocio_testkit::processor_ops::{Dump, Dumped, ProcessorOpsReply, ProcessorOpsRequest};
use ocio_testkit::transform_text::f64_spec;
use serde_json::{Value, json};

/// Doubles the cache ID prints: zeros, values that need all 7 significant digits or round in
/// the 7th, the tiniest and largest doubles, the non-finite ones, and values inside the ranges
/// `validate` accepts.
const VALUES: [f64; 28] = [
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
    1.23456789,
    0.7654321,
    1.5,
    0.25,
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

fn rgbmsw_spec(v: &GradingRgbmsw) -> Value {
    json!({"object": {"class": "GradingRGBMSW",
                      "args": [f64_spec(v.red), f64_spec(v.green), f64_spec(v.blue),
                               f64_spec(v.master), f64_spec(v.start), f64_spec(v.width)]}})
}

fn tone_spec(t: &GradingTone) -> Value {
    json!({"object": {"class": "GradingTone", "args": [{"enum": "GRADING_LOG"}], "attrs": [
        ["blacks", rgbmsw_spec(&t.blacks)],
        ["shadows", rgbmsw_spec(&t.shadows)],
        ["midtones", rgbmsw_spec(&t.midtones)],
        ["highlights", rgbmsw_spec(&t.highlights)],
        ["whites", rgbmsw_spec(&t.whites)],
        ["scontrast", f64_spec(t.scontrast)],
    ]}})
}

/// One processor: the transform's parameters and the processor's direction.
#[derive(Debug, Clone)]
struct GtCase {
    style: GradingStyle,
    values: GradingTone,
    dynamic: bool,
    transform_dir: TransformDirection,
    processor_dir: TransformDirection,
}

impl GtCase {
    fn transform(&self, dir: TransformDirection) -> Value {
        json!({"class": "GradingToneTransform", "args": {
            "values": tone_spec(&self.values),
            "style": {"enum": style_name(self.style)},
            "dynamic": self.dynamic,
            "dir": {"enum": dir_name(dir)},
        }})
    }

    fn processor(&self) -> Value {
        json!({
            "transform": self.transform(self.transform_dir),
            "direction": dir_name(self.processor_dir),
        })
    }

    /// The op data the processor holds, built as the wheel builds it, or the exception of
    /// the values the transform refuses.
    fn port(&self) -> Result<GradingToneOpData, String> {
        // GradingToneTransform::Create(style), setValue, makeDynamic, setDirection.
        let mut data = GradingToneOpData::new(self.style);
        data.set_value(&self.values).map_err(|e| e.to_string())?;
        if self.dynamic {
            data.get_dynamic_property_internal().make_dynamic();
        }
        data.set_direction(self.transform_dir);
        data.validate().map_err(|e| e.to_string())?;
        // BuildGradingToneOp: a clone, inverted for an inverse processor.
        let built = data.clone();
        Ok(match self.processor_dir {
            TransformDirection::Forward => built,
            TransformDirection::Inverse => built.inverse(),
        })
    }
}

/// The fields of a GradingTone, by index: the 5 x 6 RGBMSW components, then the s-contrast.
fn field(t: &mut GradingTone, index: usize) -> &mut f64 {
    fn rgbmsw(v: &mut GradingRgbmsw, c: usize) -> &mut f64 {
        match c {
            0 => &mut v.red,
            1 => &mut v.green,
            2 => &mut v.blue,
            3 => &mut v.master,
            4 => &mut v.start,
            _ => &mut v.width,
        }
    }
    match index {
        0..=5 => rgbmsw(&mut t.blacks, index),
        6..=11 => rgbmsw(&mut t.shadows, index - 6),
        12..=17 => rgbmsw(&mut t.midtones, index - 12),
        18..=23 => rgbmsw(&mut t.highlights, index - 18),
        24..=29 => rgbmsw(&mut t.whites, index - 24),
        _ => &mut t.scontrast,
    }
}

const NUM_FIELDS: usize = 31;

fn cases() -> Vec<GtCase> {
    let mut cases = Vec::new();
    let mut rng = Rng::new(0x7013_da7a);
    let mut k = 0usize;
    for style in STYLES {
        for dynamic in [false, true] {
            for transform_dir in DIRS {
                for processor_dir in DIRS {
                    // The defaults, then values with a few fields from VALUES.
                    for n in 0..8 {
                        let mut values = GradingTone::new(style);
                        for _ in 0..n {
                            let f = (rng.next_u64() % NUM_FIELDS as u64) as usize;
                            *field(&mut values, f) = VALUES[k % VALUES.len()];
                            k += 1;
                        }
                        cases.push(GtCase {
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
            let mut values = GradingTone::new(style);
            *field(&mut values, f) = value;
            cases.push(GtCase {
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

/// The wheel's GradingRGBMSW, written out, as the port's.
fn rgbmsw_of(dump: &Dump) -> GradingRgbmsw {
    GradingRgbmsw::new(
        getter_f64(dump.property("red")),
        getter_f64(dump.property("green")),
        getter_f64(dump.property("blue")),
        getter_f64(dump.property("master")),
        getter_f64(dump.property("start")),
        getter_f64(dump.property("width")),
    )
}

/// The bits of every value, so that NaNs compare.
fn bits(t: &GradingTone) -> Vec<u64> {
    let mut t = *t;
    (0..NUM_FIELDS)
        .map(|f| field(&mut t, f).to_bits())
        .collect()
}

/// The wheel's GradingTone, written out, as the port's.
fn tone_of(dump: &Dump) -> GradingTone {
    let rgbmsw = |name: &str| rgbmsw_of(dump.property(name).object());
    GradingTone {
        blacks: rgbmsw("blacks"),
        shadows: rgbmsw("shadows"),
        midtones: rgbmsw("midtones"),
        highlights: rgbmsw("highlights"),
        whites: rgbmsw("whites"),
        scontrast: getter_f64(dump.property("scontrast")),
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
        create_grading_tone_op(&mut ops, data.clone(), TransformDirection::Forward);
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
        assert_eq!(t.class, "GradingToneTransform", "{case:?}");
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
        let wheel_values = tone_of(t.getter("getValue").object());
        assert_eq!(bits(&wheel_values), bits(&data.get_value()), "{case:?}");

        // The optimized processor: an identity is removed (its replacement is the identity
        // matrix), any other op kept.
        let optimized = reply.optimized();
        if data.is_identity() {
            assert!(optimized.group.children.is_empty(), "{case:?}");
            removed += 1;
        } else {
            assert_eq!(optimized.group.children.len(), 1, "{case:?}");
            assert_eq!(
                optimized.group.children[0].class, "GradingToneTransform",
                "{case:?}"
            );
            kept += 1;
        }
    }
    // Each kind of case is there.
    assert!(refused > 100, "{refused} refused");
    assert!(removed > 20, "{removed} removed");
    assert!(kept > 100, "{kept} kept");
}

/// A non-dynamic op and its inverse, in a group: the optimizer removes the pair (the identity
/// matrix replaces it); values that don't equal themselves (a NaN) make no pair, and both ops
/// stay, as do two dynamic ops.
#[test]
fn inverse_pairs_are_removed() {
    let mut cases = Vec::new();
    for style in STYLES {
        for dir in DIRS {
            for (k, nan_field) in [None, Some(4), Some(14), Some(30), Some(7)]
                .into_iter()
                .enumerate()
            {
                for dynamic in [false, true] {
                    let mut values = GradingTone::new(style);
                    values.midtones.green = 1.3;
                    values.highlights.master = 0.7 + 0.05 * k as f64;
                    values.scontrast = 1.2;
                    if let Some(f) = nan_field {
                        *field(&mut values, f) = f64::NAN;
                    }
                    cases.push(GtCase {
                        style,
                        values,
                        dynamic,
                        transform_dir: dir,
                        processor_dir: TransformDirection::Forward,
                    });
                }
            }
        }
    }
    let requests: Vec<ProcessorOpsRequest> = cases
        .iter()
        .map(|case| {
            let inverse = match case.transform_dir {
                TransformDirection::Forward => TransformDirection::Inverse,
                TransformDirection::Inverse => TransformDirection::Forward,
            };
            ProcessorOpsRequest::new(json!({"transform": {"class": "GroupTransform",
                "children": [case.transform(case.transform_dir), case.transform(inverse)]}}))
        })
        .collect();
    let calls: Vec<_> = requests.iter().map(|r| r.call()).collect();
    let responses = ocio_testkit::Oracle::get().batch(&calls, true);
    assert_eq!(responses.len(), cases.len());
    let (mut removed, mut kept) = (0, 0);
    for (case, response) in cases.iter().zip(responses) {
        let reply = ProcessorOpsReply::from_response(response.expect("the oracle"));
        assert!(reply.raised().is_none(), "{case:?}: {:?}", reply.result);
        let first = case.port().unwrap();
        let second = first.inverse();
        if first.is_inverse(&second) {
            assert!(reply.optimized().group.children.is_empty(), "{case:?}");
            removed += 1;
        } else {
            assert_eq!(
                reply.optimized().classes(),
                ["GradingToneTransform"; 2],
                "{case:?}"
            );
            kept += 1;
        }
    }
    assert_eq!(removed, 6, "{removed} removed");
    assert_eq!(kept, cases.len() - 6, "{kept} kept");
}
