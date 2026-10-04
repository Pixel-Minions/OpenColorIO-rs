// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Lists of Log transforms with Range and Matrix transforms, as the oracle builds them and as
//! the port builds their ops: for the Log op's tests (`log_op_oracle.rs`) and the optimizer's
//! bake (`lut1d_bake_oracle.rs`).

use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::exception::{Exception, Result};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{OptimizationFlags, TransformDirection};
use ocio_ops::ops::log::log_op::create_log_op;
use ocio_ops::ops::log::log_op_data::{LogAffineParameter, LogOpData};
use ocio_ops::ops::lut1d::Lut1DOpData;
use ocio_ops::ops::lut1d::lut1d_op::create_lut1d_op;
use ocio_ops::ops::lut1d::lut1d_op_data::Lut3by1DArray;
use ocio_ops::ops::matrix::MatrixOpData;
use ocio_ops::ops::matrix::matrix_op::create_matrix_op;
use ocio_ops::ops::range::RangeOpData;
use ocio_ops::ops::range::range_op::create_range_op;
use ocio_testkit::battery::BitDepth as Depth;
use serde_json::{Value, json};

use super::image::{depth_name, port_depth};

use TransformDirection::{Forward as F, Inverse as I};

/// The four affine parameters: `[logSideSlope, logSideOffset, linSideSlope, linSideOffset]`.
pub(crate) type Affine = [[f64; 3]; 4];

/// A transform of the lists.
#[derive(Debug, Clone)]
pub(crate) enum T {
    /// A `LogTransform` of this base.
    Log(f64, TransformDirection),
    /// A `LogAffineTransform`: the base and the four parameters.
    Affine(f64, Affine, TransformDirection),
    /// A `LogCameraTransform`: the base, the four parameters, the break and maybe the linear
    /// slope.
    Camera(f64, Affine, [f64; 3], Option<[f64; 3]>, TransformDirection),
    /// A clamping `RangeTransform` from 0.25 to 2 on both sides.
    Range,
    /// A `MatrixTransform` scaling RGB by 2 with an offset of 0.1.
    Matrix,
    /// A `MatrixTransform` that mixes channels (red takes a tenth of green).
    CrossMatrix,
    /// A clamping `RangeTransform` from 0 to 1 on both sides: an identity clamp at integer bit
    /// depths, which `optimizeForBitdepth` removes at either end.
    Range01,
    /// A forward `Lut1DTransform` of 256 entries, which an 8-bit input looks up.
    Lut8,
}

const SETTERS: [(&str, LogAffineParameter); 4] = [
    ("setLogSideSlopeValue", LogAffineParameter::LogSideSlope),
    ("setLogSideOffsetValue", LogAffineParameter::LogSideOffset),
    ("setLinSideSlopeValue", LogAffineParameter::LinSideSlope),
    ("setLinSideOffsetValue", LogAffineParameter::LinSideOffset),
];

/// The matrix of [`T::Matrix`].
const SCALE: [f64; 16] = [
    2., 0., 0., 0., 0., 2., 0., 0., 0., 0., 2., 0., 0., 0., 0., 1.,
];
/// The offsets of [`T::Matrix`].
const OFFSET: [f64; 4] = [0.1, 0.1, 0.1, 0.];
/// The matrix of [`T::CrossMatrix`].
const CROSS: [f64; 16] = [
    1., 0.1, 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];
/// The bounds of [`T::Range`].
const RANGE: [f64; 4] = [0.25, 2.0, 0.25, 2.0];
/// The bounds of [`T::Range01`].
const RANGE01: [f64; 4] = [0.0, 1.0, 0.0, 1.0];

/// The entries of [`T::Lut8`] for the identity entry `x`.
fn lut8_entry(x: f32) -> [f32; 3] {
    [x * x, 1.0 - x, x * 0.5 + 0.25]
}

/// The entries of [`T::Lut8`]: `setLength(256)` makes the identity, then `setValue` sets each
/// entry (src/OpenColorIO/transforms/Lut1DTransform.cpp @ v2.5.2).
fn lut8_data() -> Lut1DOpData {
    let mut data = Lut1DOpData::new(2).unwrap();
    *data.get_array_mut() = Lut3by1DArray::new(data.get_half_flags(), 3, 256, false).unwrap();
    for i in 0..256 {
        let rgb = lut8_entry(data.get_array()[3 * i]);
        for (c, v) in rgb.into_iter().enumerate() {
            data.get_array_mut()[3 * i + c] = v;
        }
    }
    data
}

fn dir_enum(dir: TransformDirection) -> Value {
    match dir {
        F => json!({"enum": "TRANSFORM_DIR_FORWARD"}),
        I => json!({"enum": "TRANSFORM_DIR_INVERSE"}),
    }
}

/// The spec of a transform: built empty, then set, so that the binding's constructors don't
/// validate.
pub(crate) fn transform(t: &T) -> Value {
    let log = |class: &str, args: Value, base: f64, params: Option<&Affine>, dir| {
        let mut calls = vec![json!(["setBase", base])];
        if let Some(params) = params {
            for ((setter, _), values) in SETTERS.iter().zip(params) {
                calls.push(json!([setter, values]));
            }
        }
        (class.to_string(), args, calls, dir)
    };
    let (class, args, mut calls, dir) = match t {
        T::Log(base, dir) => log("LogTransform", json!({}), *base, None, *dir),
        T::Affine(base, p, dir) => log("LogAffineTransform", json!({}), *base, Some(p), *dir),
        T::Camera(base, p, brk, _, dir) => log(
            "LogCameraTransform",
            json!({"linSideBreak": brk}),
            *base,
            Some(p),
            *dir,
        ),
        T::Range => {
            return json!({"class": "RangeTransform", "args": {
                "minInValue": RANGE[0], "maxInValue": RANGE[1],
                "minOutValue": RANGE[2], "maxOutValue": RANGE[3],
            }});
        }
        T::Matrix => {
            return json!({"class": "MatrixTransform",
                "args": {"matrix": SCALE.to_vec(), "offset": OFFSET.to_vec()}});
        }
        T::CrossMatrix => {
            return json!({"class": "MatrixTransform", "args": {"matrix": CROSS.to_vec()}});
        }
        T::Range01 => {
            return json!({"class": "RangeTransform", "args": {
                "minInValue": RANGE01[0], "maxInValue": RANGE01[1],
                "minOutValue": RANGE01[2], "maxOutValue": RANGE01[3],
            }});
        }
        T::Lut8 => {
            let lut = lut8_data();
            let mut calls = vec![json!(["setLength", 256])];
            for i in 0..256 {
                let a = lut.get_array();
                calls.push(json!(["setValue", i, a[3 * i], a[3 * i + 1], a[3 * i + 2]]));
            }
            return json!({"class": "Lut1DTransform", "args": {}, "calls": calls});
        }
    };
    if let T::Camera(_, _, _, Some(slope), _) = t {
        calls.push(json!(["setLinearSlopeValue", slope]));
    }
    calls.push(json!(["setDirection", dir_enum(dir)]));
    json!({"class": class, "args": args, "calls": calls})
}

/// The port's data of a Log transform, as the transform builds it: `m_data(2.0f,
/// TRANSFORM_DIR_FORWARD)`, the break for a camera log, then the setters.
fn port_log_data(t: &T) -> LogOpData {
    let mut data = LogOpData::new(f64::from(2.0f32), F);
    let (base, params, dir) = match t {
        T::Log(base, dir) => (*base, None, *dir),
        T::Affine(base, p, dir) => (*base, Some(p), *dir),
        T::Camera(base, p, brk, _, dir) => {
            data.set_value(LogAffineParameter::LinSideBreak, brk)
                .unwrap();
            (*base, Some(p), *dir)
        }
        T::Range | T::Range01 | T::Matrix | T::CrossMatrix | T::Lut8 => unreachable!("a log"),
    };
    data.set_base(base);
    if let Some(params) = params {
        for ((_, param), values) in SETTERS.iter().zip(params) {
            data.set_value(*param, values).unwrap();
        }
    }
    if let T::Camera(_, _, _, Some(slope), _) = t {
        data.set_value(LogAffineParameter::LinearSlope, slope)
            .unwrap();
    }
    data.set_direction(dir);
    data
}

/// The processor of `chain`, as `cpu_apply` and `image_apply` take it.
pub(crate) fn processor(chain: &[T], flags: &str, input: Depth, output: Depth) -> Value {
    let children: Vec<Value> = chain.iter().map(transform).collect();
    json!({
        "transform": {"class": "GroupTransform", "children": children},
        "optimization": flags,
        "in_bitdepth": depth_name(port_depth(input)),
        "out_bitdepth": depth_name(port_depth(output)),
    })
}

/// The port's CPU processor of `chain`.
pub(crate) fn port_processor(
    chain: &[T],
    flags: OptimizationFlags,
    input: Depth,
    output: Depth,
) -> Result<CpuProcessor> {
    let raw = port_raw_ops(chain)?;
    CpuProcessor::new(&raw, port_depth(input), port_depth(output), flags)
}

/// The port's ops of `chain`, as the processor builds and finalizes them.
pub(crate) fn port_raw_ops(chain: &[T]) -> Result<OpVec> {
    let mut raw = OpVec::new();
    for t in chain {
        match t {
            T::Range => {
                let data = RangeOpData::with_values(RANGE[0], RANGE[1], RANGE[2], RANGE[3])?;
                create_range_op(&mut raw, data, F)?;
            }
            T::Matrix => {
                let mut data = MatrixOpData::new();
                data.set_rgba(&SCALE);
                data.set_rgba_offsets(&OFFSET);
                data.validate()?;
                create_matrix_op(&mut raw, data, F);
            }
            T::CrossMatrix => {
                let mut data = MatrixOpData::new();
                data.set_rgba(&CROSS);
                data.validate()?;
                create_matrix_op(&mut raw, data, F);
            }
            T::Range01 => {
                let data =
                    RangeOpData::with_values(RANGE01[0], RANGE01[1], RANGE01[2], RANGE01[3])?;
                create_range_op(&mut raw, data, F)?;
            }
            T::Lut8 => {
                // `BuildLut1DOp`: validated, then copied (src/OpenColorIO/ops/lut1d/Lut1DOp.cpp:
                // 244-253 @ v2.5.2).
                let data = lut8_data();
                data.validate()?;
                create_lut1d_op(&mut raw, data, F);
            }
            log => {
                // `BuildLogOp`: the transform's data, validated, then copied
                // (LogOp.cpp:185-216).
                let data = port_log_data(log);
                data.validate()?;
                create_log_op(&mut raw, data.try_clone()?, F)?;
            }
        }
    }
    raw.finalize()?;
    Ok(raw)
}

/// The port's CPU processor of `chain`, or the wheel's error as `<stage>: <message>`, with
/// the stages of `cpu_apply`.
///
/// `Config::getProcessor(group)` validates the group, which validates each child with its
/// prefix (`Processor::Impl::setTransform`, src/OpenColorIO/Processor.cpp:623-633 @ v2.5.2;
/// `GroupTransformImpl::validate`, src/OpenColorIO/transforms/GroupTransform.cpp:56-73), before
/// it builds any op: "LogTransform validation failed: ", "LogAffineTransform validation
/// failed: " or "LogCameraTransform validation failed: ", whose `validate` also needs the break
/// (src/OpenColorIO/transforms/LogCameraTransform.cpp:50-67). The ranges and the matrices of
/// the lists are valid.
pub(crate) fn staged_port_processor(
    chain: &[T],
    flags: OptimizationFlags,
    input: Depth,
    output: Depth,
) -> std::result::Result<CpuProcessor, String> {
    for t in chain {
        let prefix = match t {
            T::Log(..) => "LogTransform validation failed: ",
            T::Affine(..) => "LogAffineTransform validation failed: ",
            T::Camera(..) => "LogCameraTransform validation failed: ",
            T::Range | T::Range01 | T::Matrix | T::CrossMatrix | T::Lut8 => continue,
        };
        let data = port_log_data(t);
        let mut valid = data.validate();
        if valid.is_ok() && matches!(t, T::Camera(..)) && data.red_params().len() < 5 {
            valid = Err(Exception::new("LinSideBreak has to be defined."));
        }
        valid.map_err(|e| format!("processor: {prefix}{}", e.message()))?;
    }
    let raw = port_raw_ops(chain).map_err(|e| format!("processor: {}", e.message()))?;
    CpuProcessor::new(&raw, port_depth(input), port_depth(output), flags)
        .map_err(|e| format!("cpu_processor: {}", e.message()))
}

/// The wheel's error, as [`staged_port_processor`] gives the port's.
pub(crate) fn wheel_error(stage: &str, message: &str) -> String {
    format!("{stage}: {message}")
}
