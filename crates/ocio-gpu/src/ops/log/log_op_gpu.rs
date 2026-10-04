// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/OpenColorIO/ops/log/LogOpGPU.h` and `LogOpGPU.cpp` @ v2.5.2: the Log op's
//! GPU writer.

use ocio_ops::Result;
use ocio_ops::ops::log::log_op_data::{
    LIN_SIDE_BREAK, LIN_SIDE_OFFSET, LIN_SIDE_SLOPE, LOG_SIDE_OFFSET, LOG_SIDE_SLOPE, LogOpData,
    Params,
};
use ocio_ops::ops::log::log_utils::{get_linear_offset, get_linear_slope, get_log_side_break};

use crate::gpu_shader_desc::GpuShaderDesc;
use crate::gpu_shader_utils::GpuShaderText;

/// `std::numeric_limits<float>::min()`: the smallest normal `float`.
const MIN_VALUE: f32 = f32::MIN_POSITIVE;

/// The pixel's RGB: `<pixel name>.rgb`.
fn pixel_rgb(shader_creator: &GpuShaderDesc) -> Vec<u8> {
    [shader_creator.pixel_name(), b".rgb"].concat()
}

/// `1.0f / (float)param` for each channel's parameter `index`, in `float`.
fn float_inverses(params: [&Params; 3], index: usize) -> [f32; 3] {
    params.map(|p| 1.0f32 / p[index] as f32)
}

/// Each channel's parameter `index`, as the `double` it is.
fn doubles(params: [&Params; 3], index: usize) -> [f64; 3] {
    params.map(|p| p[index])
}

/// `(float)(param[LOG_SIDE_SLOPE] / log(base))` for each channel: the log slope with the
/// change of base rolled in, divided in `double`.
fn log_slopes_new(params: [&Params; 3], base: f64) -> [f32; 3] {
    params.map(|p| (p[LOG_SIDE_SLOPE] / base.ln()) as f32)
}

/// A plain logarithm in base 2 or 10: `max(FLT_MIN, rgb)`, then `log2` or `log` times
/// `1.0f / logf(base)`, in `float`.
///
/// Port of `AddLogShader` (LogOpGPU.cpp:18-50 @ v2.5.2).
fn add_log_shader(shader_creator: &mut GpuShaderDesc, base: f32) -> Result<()> {
    let min_value = MIN_VALUE;

    let st = GpuShaderText::new(shader_creator.language());

    st.indent();
    st.new_line().put("");
    st.new_line().put("// Add Log processing");
    st.new_line().put("");
    st.new_line().put("{");
    st.indent();

    let pixrgb = pixel_rgb(shader_creator);

    st.new_line()
        .put(&pixrgb)
        .put(" = max( ")
        .put(st.float3_splat_f32(min_value))
        .put(", ")
        .put(&pixrgb)
        .put(");");

    if base == 2.0 {
        st.new_line()
            .put(&pixrgb)
            .put(" = log2(")
            .put(&pixrgb)
            .put(");");
    } else {
        // base 10
        let one_over_log10 = 1.0f32 / base.ln();
        st.new_line()
            .put(&pixrgb)
            .put(" = log(")
            .put(&pixrgb)
            .put(") * ")
            .put(st.float3_splat_f32(one_over_log10))
            .put(";");
    }

    st.dedent();
    st.new_line().put("}");

    shader_creator.add_to_function_shader_code(st.string());
    Ok(())
}

/// The inverse of a plain logarithm in base 2 or 10: `pow(base, rgb)`, the base a `float`.
///
/// Port of `AddAntiLogShader` (LogOpGPU.cpp:52-72 @ v2.5.2).
fn add_anti_log_shader(shader_creator: &mut GpuShaderDesc, base: f32) -> Result<()> {
    let st = GpuShaderText::new(shader_creator.language());

    st.indent();
    st.new_line().put("");
    st.new_line().put("// Add Log 'Anti-Log' processing");
    st.new_line().put("");
    st.new_line().put("{");
    st.indent();

    let pixrgb = pixel_rgb(shader_creator);

    st.new_line()
        .put(&pixrgb)
        .put(" = pow( ")
        .put(st.float3_splat_f32(base))
        .put(", ")
        .put(&pixrgb)
        .put(");");

    st.dedent();
    st.new_line().put("}");

    shader_creator.add_to_function_shader_code(st.string());
    Ok(())
}

/// The inverse affine log: `(rgb - logOffset) * logSlopeInv`, `pow(base, rgb)`, then
/// `linSlopeInv * (rgb - linOffset)`. The inverse slopes are `float` quotients of the `float`
/// parameters; the offsets and the base are written as `double`s.
///
/// Port of `AddLogToLinShader` (LogOpGPU.cpp:74-119 @ v2.5.2).
fn add_log_to_lin_shader(shader_creator: &mut GpuShaderDesc, log_data: &LogOpData) -> Result<()> {
    let params = [
        log_data.red_params(),
        log_data.green_params(),
        log_data.blue_params(),
    ];
    let base = log_data.base();

    let log_slope_inv = float_inverses(params, LOG_SIDE_SLOPE);
    let lin_slope_inv = float_inverses(params, LIN_SIDE_SLOPE);

    let st = GpuShaderText::new(shader_creator.language());

    st.indent();
    st.new_line().put("");
    st.new_line().put("// Add Log 'Log to Lin' processing");
    st.new_line().put("");
    st.new_line().put("{");
    st.indent();

    let pixrgb = pixel_rgb(shader_creator);

    let lin_offset = doubles(params, LIN_SIDE_OFFSET);
    let log_offset = doubles(params, LOG_SIDE_OFFSET);
    st.declare_float3_f32(
        "log_slopeinv",
        log_slope_inv[0],
        log_slope_inv[1],
        log_slope_inv[2],
    )?;
    st.declare_float3_f32(
        "lin_slopeinv",
        lin_slope_inv[0],
        lin_slope_inv[1],
        lin_slope_inv[2],
    )?;
    st.declare_float3_f64("lin_offset", lin_offset[0], lin_offset[1], lin_offset[2])?;
    st.declare_float3_f64("log_base", base, base, base)?;
    st.declare_float3_f64("log_offset", log_offset[0], log_offset[1], log_offset[2])?;
    // Decompose into 3 steps:
    // 1) (x - logOffset) * logSlopeInv
    // 2) pow(base, x)
    // 3) linSlopeInv * (x - linOffset)
    st.new_line()
        .put(&pixrgb)
        .put(" = (")
        .put(&pixrgb)
        .put(" - log_offset) * log_slopeinv;");
    st.new_line()
        .put(&pixrgb)
        .put(" = pow(log_base, ")
        .put(&pixrgb)
        .put(");");
    st.new_line()
        .put(&pixrgb)
        .put(" = lin_slopeinv * (")
        .put(&pixrgb)
        .put(" - lin_offset);");

    st.dedent();
    st.new_line().put("}");

    shader_creator.add_to_function_shader_code(st.string());
    Ok(())
}

/// The affine log: `max(FLT_MIN, rgb * linSlope + linOffset)`, then
/// `logSlopeNew * log(rgb) + logOffset`, where `logSlopeNew` is the log slope divided by
/// `log(base)` in `double`, then rounded to `float`. The other parameters are written as
/// `double`s.
///
/// Port of `AddLinToLogShader` (LogOpGPU.cpp:121-163 @ v2.5.2).
fn add_lin_to_log_shader(shader_creator: &mut GpuShaderDesc, log_data: &LogOpData) -> Result<()> {
    // logSlope * log(linSlope * x + linOffset, base) + logOffset

    let params = [
        log_data.red_params(),
        log_data.green_params(),
        log_data.blue_params(),
    ];
    let base = log_data.base();

    let min_value = MIN_VALUE;

    let st = GpuShaderText::new(shader_creator.language());

    st.indent();
    st.new_line().put("");
    st.new_line().put("// Add Log 'Lin to Log' processing");
    st.new_line().put("");
    st.new_line().put("{");
    st.indent();

    let pixrgb = pixel_rgb(shader_creator);

    let lin_slope = doubles(params, LIN_SIDE_SLOPE);
    let lin_offset = doubles(params, LIN_SIDE_OFFSET);
    let log_offset = doubles(params, LOG_SIDE_OFFSET);
    st.declare_float3_f32("minValue", min_value, min_value, min_value)?;
    st.declare_float3_f64("lin_slope", lin_slope[0], lin_slope[1], lin_slope[2])?;
    st.declare_float3_f64("lin_offset", lin_offset[0], lin_offset[1], lin_offset[2])?;
    // We account for the change of base by rolling the multiplier in with log slope.
    let log_slope_new = log_slopes_new(params, base);
    st.declare_float3_f32(
        "log_slope",
        log_slope_new[0],
        log_slope_new[1],
        log_slope_new[2],
    )?;
    st.declare_float3_f64("log_offset", log_offset[0], log_offset[1], log_offset[2])?;
    // Decompose into 2 steps:
    // 1) clamp(fltmin, linSlope * x + linOffset)
    // 2) logSlopeNew * log(x) + logOffset
    st.new_line()
        .put(&pixrgb)
        .put(" = max( minValue, (")
        .put(&pixrgb)
        .put(" * lin_slope + lin_offset) );");
    st.new_line()
        .put(&pixrgb)
        .put(" = log_slope * log(")
        .put(&pixrgb)
        .put(" ) + log_offset;");

    st.dedent();
    st.new_line().put("}");

    shader_creator.add_to_function_shader_code(st.string());
    Ok(())
}

/// The camera style's `float` constants, per channel: the linear segment's slope, the break on
/// the log side (each platform's, I-70) and the linear segment's offset.
///
/// Port of the `LogUtil` calls of `AddCameraLogToLinShader` and `AddCameraLinToLogShader`
/// (LogOpGPU.cpp:179-187, 257-265 @ v2.5.2).
fn camera_constants(params: [&Params; 3], base: f64) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let linear_slope = params.map(|p| get_linear_slope(p, base));
    let log_side_break = params.map(|p| get_log_side_break(p, base));
    let linear_offset =
        [0, 1, 2].map(|c| get_linear_offset(params[c], linear_slope[c], log_side_break[c]));
    (linear_slope, log_side_break, linear_offset)
}

/// The inverse camera log: the linear segment `(rgb - linearOffset) * (1.0f / linearSlope)`
/// at and below the log side's break, the inverse affine log above it, combined with
/// `float3GreaterThan`.
///
/// Port of `AddCameraLogToLinShader` (LogOpGPU.cpp:165-242 @ v2.5.2).
fn add_camera_log_to_lin_shader(
    shader_creator: &mut GpuShaderDesc,
    log_data: &LogOpData,
) -> Result<()> {
    // if in <= logBreak
    //  out = ( in - linearOffset ) / linearSlope
    // else
    //  out = ( pow( base, (in - logOffset) / logSlope ) - linOffset ) / linSlope;
    //

    let params = [
        log_data.red_params(),
        log_data.green_params(),
        log_data.blue_params(),
    ];
    let base = log_data.base();

    let (linear_slope, log_side_break, linear_offset) = camera_constants(params, base);

    let log_slope_inv = float_inverses(params, LOG_SIDE_SLOPE);
    let lin_slope_inv = float_inverses(params, LIN_SIDE_SLOPE);

    let st = GpuShaderText::new(shader_creator.language());

    st.indent();
    st.new_line().put("");
    st.new_line()
        .put("// Add Log 'Camera Log to Lin' processing");
    st.new_line().put("");
    st.new_line().put("{");
    st.indent();

    let pixrgb = pixel_rgb(shader_creator);

    let lin_offset = doubles(params, LIN_SIDE_OFFSET);
    let log_offset = doubles(params, LOG_SIDE_OFFSET);
    st.declare_float3_f32(
        "log_break",
        log_side_break[0],
        log_side_break[1],
        log_side_break[2],
    )?;
    st.declare_float3_f32(
        "linear_segment_offset",
        linear_offset[0],
        linear_offset[1],
        linear_offset[2],
    )?;
    st.declare_float3_f32(
        "linear_segment_slopeinv",
        1.0f32 / linear_slope[0],
        1.0f32 / linear_slope[1],
        1.0f32 / linear_slope[2],
    )?;
    st.declare_float3_f32(
        "lin_slopeinv",
        lin_slope_inv[0],
        lin_slope_inv[1],
        lin_slope_inv[2],
    )?;
    st.declare_float3_f64("lin_offset", lin_offset[0], lin_offset[1], lin_offset[2])?;
    st.declare_float3_f32(
        "log_slopeinv",
        log_slope_inv[0],
        log_slope_inv[1],
        log_slope_inv[2],
    )?;
    st.declare_float3_f64("log_base", base, base, base)?;
    st.declare_float3_f64("log_offset", log_offset[0], log_offset[1], log_offset[2])?;

    st.new_line()
        .put(st.float3_decl("isAboveBreak")?)
        .put(" = ")
        .put(st.float3_greater_than(&pixrgb, "log_break"))
        .put(";");

    // Compute linear segment.
    st.new_line()
        .put(st.float3_decl("linSeg")?)
        .put(" = ( ")
        .put(&pixrgb)
        .put(" - linear_segment_offset ) * linear_segment_slopeinv;");

    // Decompose log segment into 3 steps:
    // 1) (x - logOffset) * logSlopeInv
    // 2) pow(base, x)
    // 3) linSlopeInv * (x - linOffset)
    st.new_line()
        .put(st.float3_decl("logSeg")?)
        .put(" = (")
        .put(&pixrgb)
        .put(" - log_offset) * log_slopeinv;");
    st.new_line().put("logSeg = pow(log_base, logSeg);");
    st.new_line()
        .put("logSeg = lin_slopeinv * (logSeg - lin_offset);");

    // Combine linear and log segments.
    write_combine(&st, &pixrgb);

    st.dedent();
    st.new_line().put("}");

    shader_creator.add_to_function_shader_code(st.string());
    Ok(())
}

/// The camera log: the linear segment `rgb * linearSlope + linearOffset` at and below the
/// linear side's break, the affine log above it, combined with `float3GreaterThan`.
///
/// Port of `AddCameraLinToLogShader` (LogOpGPU.cpp:244-313 @ v2.5.2).
fn add_camera_lin_to_log_shader(
    shader_creator: &mut GpuShaderDesc,
    log_data: &LogOpData,
) -> Result<()> {
    // if in <= linBreak
    //  out = linearSlope * in + linearOffset
    // else
    //  out = ( logSlope * log( base, max( minValue, (in*linSlope + linOffset) ) ) + logOffset )

    let params = [
        log_data.red_params(),
        log_data.green_params(),
        log_data.blue_params(),
    ];
    let base = log_data.base();

    let (linear_slope, _, linear_offset) = camera_constants(params, base);

    // We account for the change of base by rolling the multiplier in with log slope.
    let log_slope_new = log_slopes_new(params, base);

    let min_value = MIN_VALUE;

    let st = GpuShaderText::new(shader_creator.language());

    st.indent();
    st.new_line().put("");
    st.new_line()
        .put("// Add Log 'Camera Lin to Log' processing");
    st.new_line().put("");
    st.new_line().put("{");
    st.indent();

    let pixrgb = pixel_rgb(shader_creator);

    let linear_break = doubles(params, LIN_SIDE_BREAK);
    let lin_slope = doubles(params, LIN_SIDE_SLOPE);
    let lin_offset = doubles(params, LIN_SIDE_OFFSET);
    let log_offset = doubles(params, LOG_SIDE_OFFSET);
    st.declare_float3_f32("minValue", min_value, min_value, min_value)?;
    st.declare_float3_f64(
        "linear_break",
        linear_break[0],
        linear_break[1],
        linear_break[2],
    )?;
    st.declare_float3_f32(
        "linear_segment_slope",
        linear_slope[0],
        linear_slope[1],
        linear_slope[2],
    )?;
    st.declare_float3_f32(
        "linear_segment_offset",
        linear_offset[0],
        linear_offset[1],
        linear_offset[2],
    )?;
    st.declare_float3_f64("lin_slope", lin_slope[0], lin_slope[1], lin_slope[2])?;
    st.declare_float3_f64("lin_offset", lin_offset[0], lin_offset[1], lin_offset[2])?;
    st.declare_float3_f32(
        "log_slope",
        log_slope_new[0],
        log_slope_new[1],
        log_slope_new[2],
    )?;
    st.declare_float3_f64("log_offset", log_offset[0], log_offset[1], log_offset[2])?;

    st.new_line()
        .put(st.float3_decl("isAboveBreak")?)
        .put(" = ")
        .put(st.float3_greater_than(&pixrgb, "linear_break"))
        .put(";");

    // Compute linear segment.
    st.new_line()
        .put(st.float3_decl("linSeg")?)
        .put(" = ")
        .put(&pixrgb)
        .put(" * linear_segment_slope + linear_segment_offset;");

    // Decompose log into 2 steps:
    // 1) clamp(fltmin, linSlope * x + linOffset)
    // 2) logSlopeNew * log(x) + logOffset
    st.new_line()
        .put(st.float3_decl("logSeg")?)
        .put(" = max( minValue, (")
        .put(&pixrgb)
        .put(" * lin_slope + lin_offset) );");
    st.new_line()
        .put("logSeg = log_slope * log( logSeg ) + log_offset;");

    // Combine linear and log segments.
    write_combine(&st, &pixrgb);

    st.dedent();
    st.new_line().put("}");

    shader_creator.add_to_function_shader_code(st.string());
    Ok(())
}

/// `rgb = isAboveBreak * logSeg + (1 - isAboveBreak) * linSeg`, the 1 a `float`.
///
/// Port of the last step of `AddCameraLogToLinShader` and `AddCameraLinToLogShader`
/// (LogOpGPU.cpp:236, 307 @ v2.5.2).
fn write_combine(st: &GpuShaderText, pixrgb: &[u8]) {
    st.new_line()
        .put(pixrgb)
        .put(" = isAboveBreak * logSeg + ( ")
        .put(st.float3_splat_f32(1.0))
        .put(" - isAboveBreak ) * linSeg;");
}

/// Adds the code of a Log op to `shader_creator`'s function body: a plain logarithm in base 2
/// or 10, or its inverse, as `log2`/`log` and `pow`; otherwise the affine log or the camera log
/// (a log with a linear side break), in its direction.
///
/// Port of `GetLogGPUShaderProgram` (LogOpGPU.cpp:317-372 @ v2.5.2).
pub fn get_log_gpu_shader_program(
    shader_creator: &mut GpuShaderDesc,
    log_data: &LogOpData,
) -> Result<()> {
    use ocio_ops::open_color_types::TransformDirection::{Forward, Inverse};

    let dir = log_data.direction();
    if log_data.is_log2() {
        match dir {
            Forward => add_log_shader(shader_creator, 2.0),
            Inverse => add_anti_log_shader(shader_creator, 2.0),
        }
    } else if log_data.is_log10() {
        match dir {
            Forward => add_log_shader(shader_creator, 10.0),
            Inverse => add_anti_log_shader(shader_creator, 10.0),
        }
    } else if log_data.is_camera() {
        match dir {
            Forward => add_camera_lin_to_log_shader(shader_creator, log_data),
            Inverse => add_camera_log_to_lin_shader(shader_creator, log_data),
        }
    } else {
        match dir {
            Forward => add_lin_to_log_shader(shader_creator, log_data),
            Inverse => add_log_to_lin_shader(shader_creator, log_data),
        }
    }
}
