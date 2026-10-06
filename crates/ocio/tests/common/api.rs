// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The port's side of the oracle's transform specs, for the tests that check pixels and
//! shaders through the API (`Transform -> Config::CreateRaw() -> Processor -> CPU or GPU
//! processor`) with one spec for both sides.
//!
//! - [`port_transform`] builds the port's [`Transform`] from a transform spec the way the
//!   oracle's `spec.transform` builds the wheel's (`oracle/ocio_oracle/spec.py`): the default
//!   object of the class, then the setters of `"calls"` in order, then the `"children"` of a
//!   group. It panics on anything it doesn't know, so a spec can't mean one thing to the wheel
//!   and another to the port.
//! - [`Calls`] writes such specs from parameters: a class and its setters, every number of
//!   which is a battery slot ([`Params`]), so that the battery generates extreme, NaN and
//!   infinite parameters for the API as it does for the ops. Numbers JSON can't hold go as
//!   `{"f64": bits}`, which the oracle reads back exactly.
//! - [`port_processor`] is `Config::CreateRaw()->getProcessor(transform, TRANSFORM_DIR_FORWARD)`,
//!   as the oracle's `cpu_apply`, `image_apply` and `gpu_shader` build theirs.

use std::sync::Arc;

use ocio::{
    Allocation, AllocationTransform, BitDepth, CdlTransform, Config, Exception, ExponentTransform,
    ExponentWithLinearTransform, FixedFunctionStyle, FixedFunctionTransform, GroupTransform,
    Interpolation, LogAffineTransform, LogCameraTransform, LogTransform, Lut1DHueAdjust,
    Lut1DTransform, MatrixTransform, NegativeStyle, OptimizationFlags, Processor, RangeStyle,
    RangeTransform, Transform, TransformDirection,
};
use ocio_ops::open_color_types::CdlStyle;
use ocio_testkit::battery::Direction;
use ocio_testkit::battery::params::{Channels, Params, Precision, Slot};
use serde_json::{Map, Value, json};

/// A number of a spec: a JSON number when JSON holds it exactly, `{"f64": bits}` otherwise
/// (NaNs, infinities, -0.0).
pub(crate) fn num(v: f64) -> Value {
    if v.is_finite() && !(v == 0.0 && v.is_sign_negative()) {
        json!(v)
    } else {
        json!({"f64": v.to_bits()})
    }
}

/// The number a spec value holds: a JSON number, or `{"f64": bits}` (`spec.value`).
fn number(v: &Value) -> f64 {
    if let Some(bits) = v.get("f64") {
        return f64::from_bits(bits.as_u64().expect("f64 bits"));
    }
    v.as_f64().unwrap_or_else(|| panic!("a number, not {v}"))
}

/// `N` numbers from a spec's list.
fn numbers<const N: usize>(v: &Value) -> [f64; N] {
    let list = v.as_array().unwrap_or_else(|| panic!("a list, not {v}"));
    assert_eq!(list.len(), N, "{N} numbers: {v}");
    std::array::from_fn(|i| number(&list[i]))
}

/// The name of a spec's `{"enum": name}`.
fn enum_name(v: &Value) -> &str {
    v.get("enum")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("an enum, not {v}"))
}

/// A spec's unsigned integer, as the binding converts it to an `unsigned long`.
fn unsigned(v: &Value) -> std::ffi::c_ulong {
    let n = v
        .as_u64()
        .unwrap_or_else(|| panic!("an unsigned integer, not {v}"));
    std::ffi::c_ulong::try_from(n).expect("an unsigned long")
}

/// A spec's bool.
fn boolean(v: &Value) -> bool {
    v.as_bool().unwrap_or_else(|| panic!("a bool, not {v}"))
}

fn direction(v: &Value) -> TransformDirection {
    match enum_name(v) {
        "TRANSFORM_DIR_FORWARD" => TransformDirection::Forward,
        "TRANSFORM_DIR_INVERSE" => TransformDirection::Inverse,
        other => panic!("direction {other}"),
    }
}

fn negative_style(v: &Value) -> NegativeStyle {
    match enum_name(v) {
        "NEGATIVE_CLAMP" => NegativeStyle::Clamp,
        "NEGATIVE_MIRROR" => NegativeStyle::Mirror,
        "NEGATIVE_PASS_THRU" => NegativeStyle::PassThru,
        "NEGATIVE_LINEAR" => NegativeStyle::Linear,
        other => panic!("negative style {other}"),
    }
}

/// A `FixedFunctionStyle` by its PyOpenColorIO name.
fn fixed_function_style(v: &Value) -> FixedFunctionStyle {
    use FixedFunctionStyle::*;
    match enum_name(v) {
        "FIXED_FUNCTION_ACES_RED_MOD_03" => AcesRedMod03,
        "FIXED_FUNCTION_ACES_RED_MOD_10" => AcesRedMod10,
        "FIXED_FUNCTION_ACES_GLOW_03" => AcesGlow03,
        "FIXED_FUNCTION_ACES_GLOW_10" => AcesGlow10,
        "FIXED_FUNCTION_ACES_DARK_TO_DIM_10" => AcesDarkToDim10,
        "FIXED_FUNCTION_REC2100_SURROUND" => Rec2100Surround,
        "FIXED_FUNCTION_RGB_TO_HSV" => RgbToHsv,
        "FIXED_FUNCTION_XYZ_TO_xyY" => XyzToXyy,
        "FIXED_FUNCTION_XYZ_TO_uvY" => XyzToUvy,
        "FIXED_FUNCTION_XYZ_TO_LUV" => XyzToLuv,
        "FIXED_FUNCTION_ACES_GAMUTMAP_02" => AcesGamutMap02,
        "FIXED_FUNCTION_ACES_GAMUTMAP_07" => AcesGamutMap07,
        "FIXED_FUNCTION_ACES_GAMUT_COMP_13" => AcesGamutComp13,
        "FIXED_FUNCTION_LIN_TO_PQ" => LinToPq,
        "FIXED_FUNCTION_LIN_TO_GAMMA_LOG" => LinToGammaLog,
        "FIXED_FUNCTION_LIN_TO_DOUBLE_LOG" => LinToDoubleLog,
        "FIXED_FUNCTION_ACES_OUTPUT_TRANSFORM_20" => AcesOutputTransform20,
        "FIXED_FUNCTION_ACES_RGB_TO_JMH_20" => AcesRgbToJmh20,
        "FIXED_FUNCTION_ACES_TONESCALE_COMPRESS_20" => AcesTonescaleCompress20,
        "FIXED_FUNCTION_ACES_GAMUT_COMPRESS_20" => AcesGamutCompress20,
        "FIXED_FUNCTION_RGB_TO_HSY_LIN" => RgbToHsyLin,
        "FIXED_FUNCTION_RGB_TO_HSY_LOG" => RgbToHsyLog,
        "FIXED_FUNCTION_RGB_TO_HSY_VID" => RgbToHsyVid,
        other => panic!("fixed function style {other}"),
    }
}

fn bit_depth(v: &Value) -> BitDepth {
    match enum_name(v) {
        "BIT_DEPTH_UNKNOWN" => BitDepth::Unknown,
        "BIT_DEPTH_UINT8" => BitDepth::Uint8,
        "BIT_DEPTH_UINT10" => BitDepth::Uint10,
        "BIT_DEPTH_UINT12" => BitDepth::Uint12,
        "BIT_DEPTH_UINT14" => BitDepth::Uint14,
        "BIT_DEPTH_UINT16" => BitDepth::Uint16,
        "BIT_DEPTH_UINT32" => BitDepth::Uint32,
        "BIT_DEPTH_F16" => BitDepth::F16,
        "BIT_DEPTH_F32" => BitDepth::F32,
        other => panic!("bit depth {other}"),
    }
}

/// The arguments of a call, after its name.
fn args_of(call: &Value) -> (&str, &[Value]) {
    let list = call.as_array().expect("a call is a list");
    let name = list[0].as_str().expect("a setter's name");
    (name, &list[1..])
}

/// The one argument of a call.
fn one<'a>(name: &str, args: &'a [Value]) -> &'a Value {
    assert_eq!(args.len(), 1, "{name} takes one argument: {args:?}");
    &args[0]
}

/// The port's transform of `spec`, built as the oracle's `spec.transform` builds the wheel's:
/// the class's default object (a `LogCameraTransform` from its `linSideBreak`, the one
/// constructor argument without a default), then each setter of `"calls"`, then the
/// `"children"` of a group. A setter's error is the wheel's at stage `"transform"`.
pub(crate) fn port_transform(spec: &Value) -> Result<Transform, Exception> {
    let class = spec["class"].as_str().expect("a class");
    assert!(
        spec.get("factory").is_none(),
        "port_transform takes no factory: {spec}"
    );
    let args = spec
        .get("args")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let calls: &[Value] = spec
        .get("calls")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice);
    let check_args = |allowed: &[&str]| {
        for key in args.keys() {
            assert!(
                allowed.contains(&key.as_str()),
                "port_transform: {class} takes no argument {key}; use a setter"
            );
        }
    };
    let unknown = |name: &str| -> ! { panic!("port_transform: no {class}.{name}") };
    let transform: Transform = match class {
        "GroupTransform" => {
            check_args(&[]);
            let mut t = GroupTransform::new();
            for call in calls {
                let (name, a) = args_of(call);
                match name {
                    "setDirection" => t.set_direction(direction(one(name, a))),
                    _ => unknown(name),
                }
            }
            for child in spec
                .get("children")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                t.append_transform(port_transform(child)?);
            }
            t.into()
        }
        "MatrixTransform" => {
            check_args(&[]);
            let mut t = MatrixTransform::new();
            for call in calls {
                let (name, a) = args_of(call);
                match name {
                    "setDirection" => t.set_direction(direction(one(name, a))),
                    "setMatrix" => t.set_matrix(&numbers(one(name, a))),
                    "setOffset" => t.set_offset(&numbers(one(name, a))),
                    "setFileInputBitDepth" => t.set_file_input_bit_depth(bit_depth(one(name, a))),
                    "setFileOutputBitDepth" => {
                        t.set_file_output_bit_depth(bit_depth(one(name, a)));
                    }
                    _ => unknown(name),
                }
            }
            t.into()
        }
        "RangeTransform" => {
            check_args(&[]);
            let mut t = RangeTransform::new();
            for call in calls {
                let (name, a) = args_of(call);
                match name {
                    "setDirection" => t.set_direction(direction(one(name, a))),
                    "setStyle" => t.set_style(match enum_name(one(name, a)) {
                        "RANGE_NO_CLAMP" => RangeStyle::NoClamp,
                        "RANGE_CLAMP" => RangeStyle::Clamp,
                        other => panic!("range style {other}"),
                    }),
                    "setMinInValue" => t.set_min_in_value(number(one(name, a))),
                    "setMaxInValue" => t.set_max_in_value(number(one(name, a))),
                    "setMinOutValue" => t.set_min_out_value(number(one(name, a))),
                    "setMaxOutValue" => t.set_max_out_value(number(one(name, a))),
                    "setFileInputBitDepth" => t.set_file_input_bit_depth(bit_depth(one(name, a))),
                    "setFileOutputBitDepth" => {
                        t.set_file_output_bit_depth(bit_depth(one(name, a)));
                    }
                    _ => unknown(name),
                }
            }
            t.into()
        }
        "ExponentTransform" => {
            check_args(&[]);
            let mut t = ExponentTransform::new();
            for call in calls {
                let (name, a) = args_of(call);
                match name {
                    "setDirection" => t.set_direction(direction(one(name, a))),
                    "setValue" => t.set_value(&numbers(one(name, a))),
                    "setNegativeStyle" => t.set_negative_style(negative_style(one(name, a)))?,
                    _ => unknown(name),
                }
            }
            t.into()
        }
        "ExponentWithLinearTransform" => {
            check_args(&[]);
            let mut t = ExponentWithLinearTransform::new();
            for call in calls {
                let (name, a) = args_of(call);
                match name {
                    "setDirection" => t.set_direction(direction(one(name, a))),
                    "setGamma" => t.set_gamma(&numbers(one(name, a))),
                    "setOffset" => t.set_offset(&numbers(one(name, a))),
                    "setNegativeStyle" => t.set_negative_style(negative_style(one(name, a)))?,
                    _ => unknown(name),
                }
            }
            t.into()
        }
        "LogTransform" => {
            check_args(&[]);
            let mut t = LogTransform::new();
            for call in calls {
                let (name, a) = args_of(call);
                match name {
                    "setDirection" => t.set_direction(direction(one(name, a))),
                    "setBase" => t.set_base(number(one(name, a))),
                    _ => unknown(name),
                }
            }
            t.into()
        }
        "LogAffineTransform" => {
            check_args(&[]);
            let mut t = LogAffineTransform::new();
            for call in calls {
                let (name, a) = args_of(call);
                match name {
                    "setDirection" => t.set_direction(direction(one(name, a))),
                    "setBase" => t.set_base(number(one(name, a))),
                    "setLogSideSlopeValue" => t.set_log_side_slope_value(&numbers(one(name, a))),
                    "setLogSideOffsetValue" => t.set_log_side_offset_value(&numbers(one(name, a))),
                    "setLinSideSlopeValue" => t.set_lin_side_slope_value(&numbers(one(name, a))),
                    "setLinSideOffsetValue" => t.set_lin_side_offset_value(&numbers(one(name, a))),
                    _ => unknown(name),
                }
            }
            t.into()
        }
        "LogCameraTransform" => {
            check_args(&["linSideBreak"]);
            let brk = args
                .get("linSideBreak")
                .unwrap_or_else(|| panic!("LogCameraTransform needs linSideBreak: {spec}"));
            let mut t = LogCameraTransform::new(&numbers(brk));
            for call in calls {
                let (name, a) = args_of(call);
                match name {
                    "setDirection" => t.set_direction(direction(one(name, a))),
                    "setBase" => t.set_base(number(one(name, a))),
                    "setLogSideSlopeValue" => t.set_log_side_slope_value(&numbers(one(name, a))),
                    "setLogSideOffsetValue" => t.set_log_side_offset_value(&numbers(one(name, a))),
                    "setLinSideSlopeValue" => t.set_lin_side_slope_value(&numbers(one(name, a))),
                    "setLinSideOffsetValue" => t.set_lin_side_offset_value(&numbers(one(name, a))),
                    "setLinSideBreakValue" => t.set_lin_side_break_value(&numbers(one(name, a))),
                    "setLinearSlopeValue" => t.set_linear_slope_value(&numbers(one(name, a)))?,
                    "unsetLinearSlopeValue" => {
                        assert!(a.is_empty(), "{name} takes no argument");
                        t.unset_linear_slope_value();
                    }
                    _ => unknown(name),
                }
            }
            t.into()
        }
        "CDLTransform" => {
            check_args(&[]);
            let mut t = CdlTransform::new();
            for call in calls {
                let (name, a) = args_of(call);
                match name {
                    "setDirection" => t.set_direction(direction(one(name, a))),
                    "setStyle" => t.set_style(match enum_name(one(name, a)) {
                        "CDL_ASC" => CdlStyle::Asc,
                        "CDL_NO_CLAMP" => CdlStyle::NoClamp,
                        other => panic!("CDL style {other}"),
                    }),
                    "setSlope" => t.set_slope(&numbers(one(name, a))),
                    "setOffset" => t.set_offset(&numbers(one(name, a))),
                    "setPower" => t.set_power(&numbers(one(name, a))),
                    "setSOP" => t.set_sop(&numbers(one(name, a))),
                    "setSat" => t.set_sat(number(one(name, a))),
                    _ => unknown(name),
                }
            }
            t.into()
        }
        "AllocationTransform" => {
            check_args(&[]);
            let mut t = AllocationTransform::new();
            for call in calls {
                let (name, a) = args_of(call);
                match name {
                    "setDirection" => t.set_direction(direction(one(name, a))),
                    "setAllocation" => t.set_allocation(match enum_name(one(name, a)) {
                        "ALLOCATION_UNKNOWN" => Allocation::Unknown,
                        "ALLOCATION_UNIFORM" => Allocation::Uniform,
                        "ALLOCATION_LG2" => Allocation::Lg2,
                        other => panic!("allocation {other}"),
                    }),
                    // The binding takes a `std::vector<float>`: each Python float narrowed.
                    "setVars" => {
                        let vars: Vec<f32> = one(name, a)
                            .as_array()
                            .expect("a list")
                            .iter()
                            .map(|v| number(v) as f32)
                            .collect();
                        t.set_vars(&vars);
                    }
                    _ => unknown(name),
                }
            }
            t.into()
        }
        "Lut1DTransform" => {
            check_args(&[]);
            let mut t = Lut1DTransform::new();
            for call in calls {
                let (name, a) = args_of(call);
                match name {
                    "setDirection" => t.set_direction(direction(one(name, a))),
                    "setLength" => t.set_length(unsigned(one(name, a)))?,
                    // `setValue(index, r, g, b)`: the binding narrows each float.
                    "setValue" => {
                        assert_eq!(a.len(), 4, "{name} takes 4 arguments");
                        let [r, g, b] = [1, 2, 3].map(|k| number(&a[k]) as f32);
                        t.set_value(unsigned(&a[0]), r, g, b)?;
                    }
                    "setInputHalfDomain" => t.set_input_half_domain(boolean(one(name, a))),
                    "setOutputRawHalfs" => t.set_output_raw_halfs(boolean(one(name, a))),
                    "setHueAdjust" => t.set_hue_adjust(match enum_name(one(name, a)) {
                        "HUE_NONE" => Lut1DHueAdjust::None,
                        "HUE_DW3" => Lut1DHueAdjust::Dw3,
                        "HUE_WYPN" => Lut1DHueAdjust::Wypn,
                        other => panic!("hue adjust {other}"),
                    })?,
                    "setInterpolation" => t.set_interpolation(match enum_name(one(name, a)) {
                        "INTERP_UNKNOWN" => Interpolation::Unknown,
                        "INTERP_NEAREST" => Interpolation::Nearest,
                        "INTERP_LINEAR" => Interpolation::Linear,
                        "INTERP_TETRAHEDRAL" => Interpolation::Tetrahedral,
                        "INTERP_CUBIC" => Interpolation::Cubic,
                        "INTERP_DEFAULT" => Interpolation::Default,
                        "INTERP_BEST" => Interpolation::Best,
                        other => panic!("interpolation {other}"),
                    }),
                    "setFileOutputBitDepth" => {
                        t.set_file_output_bit_depth(bit_depth(one(name, a)));
                    }
                    _ => unknown(name),
                }
            }
            t.into()
        }
        "FixedFunctionTransform" => {
            check_args(&["style"]);
            // The binding's constructor: `Create(style)` (validating), then `setDirection` to
            // the default forward direction and `validate`
            // (src/bindings/python/transforms/PyFixedFunctionTransform.cpp:26-42 @ v2.5.2).
            let style = args
                .get("style")
                .expect("FixedFunctionTransform takes a style");
            let mut t = FixedFunctionTransform::new(fixed_function_style(style), &[])?;
            t.set_direction(TransformDirection::Forward);
            t.validate()?;
            for call in calls {
                let (name, a) = args_of(call);
                match name {
                    "setDirection" => t.set_direction(direction(one(name, a))),
                    "setStyle" => t.set_style(fixed_function_style(one(name, a)))?,
                    "setParams" => {
                        let params: Vec<f64> = one(name, a)
                            .as_array()
                            .expect("a list")
                            .iter()
                            .map(number)
                            .collect();
                        t.set_params(&params);
                    }
                    _ => unknown(name),
                }
            }
            t.into()
        }
        other => panic!("port_transform: no class {other}"),
    };
    if class != "GroupTransform" {
        assert!(
            spec.get("children").is_none(),
            "only a GroupTransform has children: {spec}"
        );
    }
    Ok(transform)
}

/// `Config::CreateRaw()->getProcessor(transform, TRANSFORM_DIR_FORWARD)` of `spec`, as the
/// oracle builds the processor of a request with a `"transform"` and no `"config"` or
/// `"direction"` (`_processor`, oracle/ocio_oracle/commands.py); a fresh raw config, as the
/// oracle's is per request.
pub(crate) fn port_processor(spec: &Value) -> Result<Arc<Processor>, Exception> {
    let transform = port_transform(spec)?;
    Config::create_raw()
        .unwrap()
        .processor_in_direction(&transform, TransformDirection::Forward)
}

/// The optimization levels of `OptimizationFlags`, as the oracle names them, and the port's.
pub(crate) const LEVELS: [(&str, OptimizationFlags); 6] = [
    ("OPTIMIZATION_NONE", OptimizationFlags::NONE),
    ("OPTIMIZATION_LOSSLESS", OptimizationFlags::LOSSLESS),
    ("OPTIMIZATION_VERY_GOOD", OptimizationFlags::VERY_GOOD),
    ("OPTIMIZATION_GOOD", OptimizationFlags::GOOD),
    ("OPTIMIZATION_DRAFT", OptimizationFlags::DRAFT),
    ("OPTIMIZATION_DEFAULT", OptimizationFlags::DEFAULT),
];

/// One argument of a setter in a [`Calls`] spec.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Arg {
    /// A number: a slot.
    Num(f64, Channels),
    /// A list of numbers: a slot each.
    List(Vec<(f64, Channels)>),
    /// Anything else (an enum, an index, a bool): not a slot.
    Fixed(Value),
}

impl Arg {
    fn json(&self) -> Value {
        match self {
            Arg::Num(v, _) => num(*v),
            Arg::List(values) => values.iter().map(|(v, _)| num(*v)).collect(),
            Arg::Fixed(v) => v.clone(),
        }
    }
}

/// A transform as a class, its constructor arguments and its setters, with every number a
/// battery slot; a group's children follow. It writes the spec both sides build
/// ([`Calls::spec`]).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Calls {
    class: &'static str,
    args: Vec<(&'static str, Arg)>,
    calls: Vec<(&'static str, Vec<Arg>)>,
    children: Vec<(Calls, Direction)>,
    precision: Precision,
}

/// The output channels of the matrix's row of element `k`, and of offset `k`.
const ROWS: [Channels; 4] = [
    [true, false, false, false],
    [false, true, false, false],
    [false, false, true, false],
    [false, false, false, true],
];

impl Calls {
    /// A transform of `class` with no setter yet; its numbers are `double`s the ops narrow to
    /// `float` ([`Precision::F64AsF32`]).
    pub(crate) fn new(class: &'static str) -> Calls {
        Calls {
            class,
            args: Vec::new(),
            calls: Vec::new(),
            children: Vec::new(),
            precision: Precision::F64AsF32,
        }
    }

    /// The same, with numbers the transform stores as `float`.
    pub(crate) fn f32(mut self) -> Calls {
        self.precision = Precision::F32;
        self
    }

    /// A constructor argument of numbers, one per channel R, G, B.
    pub(crate) fn arg_rgb(mut self, key: &'static str, values: [f64; 3]) -> Calls {
        self.args.push((key, rgb(values)));
        self
    }

    /// A constructor argument that isn't a number: an enum's name, an index, a bool.
    pub(crate) fn arg_fixed(mut self, key: &'static str, value: Value) -> Calls {
        self.args.push((key, Arg::Fixed(value)));
        self
    }

    /// A setter with these arguments.
    pub(crate) fn call(mut self, name: &'static str, args: Vec<Arg>) -> Calls {
        self.calls.push((name, args));
        self
    }

    /// A setter of one number that applies to `channels`.
    pub(crate) fn scalar(self, name: &'static str, v: f64, channels: Channels) -> Calls {
        self.call(name, vec![Arg::Num(v, channels)])
    }

    /// A setter of a list of numbers, one per channel R, G, B.
    pub(crate) fn rgb(self, name: &'static str, values: [f64; 3]) -> Calls {
        self.call(name, vec![rgb(values)])
    }

    /// A setter of a list of numbers, one per channel R, G, B, A.
    pub(crate) fn rgba(self, name: &'static str, values: [f64; 4]) -> Calls {
        let list = values.iter().zip(ROWS).map(|(&v, c)| (v, c)).collect();
        self.call(name, vec![Arg::List(list)])
    }

    /// A setter of a 4x4 matrix, row by row: a row's numbers apply to its output channel.
    pub(crate) fn matrix(self, name: &'static str, values: [f64; 16]) -> Calls {
        let list = values
            .iter()
            .enumerate()
            .map(|(k, &v)| (v, ROWS[k / 4]))
            .collect();
        self.call(name, vec![Arg::List(list)])
    }

    /// A setter of a list of numbers that all apply to `channels`.
    pub(crate) fn list(self, name: &'static str, values: &[f64], channels: Channels) -> Calls {
        let list = values.iter().map(|&v| (v, channels)).collect();
        self.call(name, vec![Arg::List(list)])
    }

    /// A setter of one value that isn't a number: an enum's name, an index, a bool.
    pub(crate) fn fixed(self, name: &'static str, value: Value) -> Calls {
        self.call(name, vec![Arg::Fixed(value)])
    }

    /// A setter of an enum value.
    pub(crate) fn enumerated(self, name: &'static str, value: &str) -> Calls {
        self.fixed(name, json!({"enum": value}))
    }

    /// A group's child, forward.
    pub(crate) fn child(self, child: Calls) -> Calls {
        self.child_in(child, Direction::Forward)
    }

    /// A group's child, in the direction `dir`.
    pub(crate) fn child_in(mut self, child: Calls, dir: Direction) -> Calls {
        assert_eq!(self.class, "GroupTransform", "only a group has children");
        self.children.push((child, dir));
        self
    }

    /// The class.
    pub(crate) fn class(&self) -> &'static str {
        self.class
    }

    /// The spec, in the direction `dir`: `setDirection` after the other setters.
    pub(crate) fn spec(&self, dir: Direction) -> Value {
        let mut spec = self.spec_without_direction();
        let dir = match dir {
            Direction::Forward => "TRANSFORM_DIR_FORWARD",
            Direction::Inverse => "TRANSFORM_DIR_INVERSE",
        };
        spec["calls"]
            .as_array_mut()
            .expect("calls")
            .push(json!(["setDirection", {"enum": dir}]));
        spec
    }

    /// The spec with the setters given, each child in its direction.
    fn spec_without_direction(&self) -> Value {
        let mut spec = json!({"class": self.class});
        let args: Map<String, Value> = self
            .args
            .iter()
            .map(|(key, arg)| (key.to_string(), arg.json()))
            .collect();
        if !args.is_empty() {
            spec["args"] = Value::Object(args);
        }
        spec["calls"] = self
            .calls
            .iter()
            .map(|(name, args)| {
                let mut call = vec![json!(name)];
                call.extend(args.iter().map(Arg::json));
                Value::Array(call)
            })
            .collect();
        if !self.children.is_empty() {
            spec["children"] = self.children.iter().map(|(c, dir)| c.spec(*dir)).collect();
        }
        spec
    }

    /// Every number, with its slot's name and channels, in order: the constructor's, the
    /// setters', then the children's.
    fn numbers(&self) -> Vec<(String, f64, Channels, Precision)> {
        let mut out = Vec::new();
        let mut push = |name: String, arg: &Arg| match arg {
            Arg::Num(v, c) => out.push((name, *v, *c, self.precision)),
            Arg::List(values) => {
                for (k, (v, c)) in values.iter().enumerate() {
                    out.push((format!("{name}[{k}]"), *v, *c, self.precision));
                }
            }
            Arg::Fixed(_) => {}
        };
        for (key, arg) in &self.args {
            push(key.to_string(), arg);
        }
        for (name, args) in &self.calls {
            for (k, arg) in args.iter().enumerate() {
                let label = if args.len() == 1 {
                    name.to_string()
                } else {
                    format!("{name}#{k}")
                };
                push(label, arg);
            }
        }
        for (k, (child, _)) in self.children.iter().enumerate() {
            for (name, v, c, p) in child.numbers() {
                out.push((format!("{k}.{}.{name}", child.class), v, c, p));
            }
        }
        out
    }

    /// The number of slot `index`, to read or write.
    fn slot_mut(&mut self, mut index: usize) -> &mut f64 {
        let own = self
            .args
            .iter_mut()
            .map(|(_, arg)| arg)
            .chain(self.calls.iter_mut().flat_map(|(_, args)| args.iter_mut()));
        for arg in own {
            match arg {
                Arg::Num(v, _) => {
                    if index == 0 {
                        return v;
                    }
                    index -= 1;
                }
                Arg::List(values) => {
                    if index < values.len() {
                        return &mut values[index].0;
                    }
                    index -= values.len();
                }
                Arg::Fixed(_) => {}
            }
        }
        for (child, _) in &mut self.children {
            let n = child.numbers().len();
            if index < n {
                return child.slot_mut(index);
            }
            index -= n;
        }
        panic!("no slot {index} left in {}", self.class)
    }
}

/// Numbers, one per channel R, G, B.
fn rgb(values: [f64; 3]) -> Arg {
    Arg::List(values.iter().zip(ROWS).map(|(&v, c)| (v, c)).collect())
}

impl Params for Calls {
    fn slots(&self) -> Vec<Slot> {
        self.numbers()
            .into_iter()
            .map(|(name, _, channels, precision)| Slot::new(name, precision, channels))
            .collect()
    }
    fn get(&self, index: usize) -> f64 {
        self.numbers()[index].1
    }
    fn set(&mut self, index: usize, value: f64) {
        *self.slot_mut(index) = value;
    }
}
