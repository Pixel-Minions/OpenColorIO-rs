// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The analytic transforms' pixels through the port's API against the wheel's, bit for bit,
//! through the oracle test battery (`ocio_testkit::battery`): every case in both directions,
//! with fast math on and off, on the tier's probe sets (`OCIO_RS_TIER`), and the extreme, NaN
//! and infinite parameters the battery generates from each family's bases.
//!
//! Both sides build the transform from one spec (`common::api`): the wheel through the
//! oracle's `cpu_apply`, the port with `port_transform`. Each then takes the processor of the
//! raw config, `Config::CreateRaw()->getProcessor(transform)`, and its CPU processor:
//! `getDefaultCPUProcessor()` with fast math on, `getOptimizedCPUProcessor(F32, F32,
//! OPTIMIZATION_DEFAULT without OPTIMIZATION_FAST_LOG_EXP_POW)` with it off; and applies it
//! from one packed F32 RGBA image to another (`CPUProcessor::apply(src, dst)`). So the port's
//! transform, its validation, `BuildOps`, the optimizer and the CPU engine are all on the
//! path, where the op families' batteries (`crates/ocio-ops/tests`) build the op by hand.
//!
//! Where the wheel refuses a case, the port must refuse it with the same text.

mod common;

use std::sync::Arc;

use common::api::{Calls, port_processor};
use common::api_cases::{self, Cases};
use ocio::{BitDepth, OptimizationFlags};
use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::image_desc::{Bytes, PackedImageDesc};
use ocio_testkit::battery::params::Case;
use ocio_testkit::battery::{self, Combo, Direction, Family, Port, Spec, Validation};

/// `OPTIMIZATION_DEFAULT` without `OPTIMIZATION_FAST_LOG_EXP_POW`: the battery's flags with
/// fast math off.
fn default_without_fast_math() -> OptimizationFlags {
    OptimizationFlags(OptimizationFlags::DEFAULT.0 & !OptimizationFlags::FAST_LOG_EXP_POW.0)
}

/// `cpu.apply(src, dst)` on `pixels`, packed F32 RGBA, as the oracle's `cpu_apply` calls it:
/// one row, explicit strides, a zeroed destination.
fn apply_f32_rgba(cpu: &CpuProcessor, pixels: &mut [f32]) {
    let n = pixels.len() / 4;
    let row = (16 * n) as isize;
    let src: Vec<u8> = pixels.iter().flat_map(|v| v.to_ne_bytes()).collect();
    let mut dst = vec![0u8; src.len()];
    {
        let src =
            PackedImageDesc::with_strides(Bytes(&src[..]), n, 1, 4, BitDepth::F32, 4, 16, row)
                .expect("a source image");
        let mut dst =
            PackedImageDesc::with_strides(Bytes(&mut dst[..]), n, 1, 4, BitDepth::F32, 4, 16, row)
                .expect("a destination image");
        cpu.apply_src_dst(&src, &mut dst).expect("the apply");
    }
    for (value, bytes) in pixels.iter_mut().zip(dst.as_chunks::<4>().0) {
        *value = f32::from_ne_bytes(*bytes);
    }
}

/// The port's CPU processor of `spec` for the battery's fast-math setting, through the API, or
/// the text of the exception it raises.
fn api_port(calls: &Calls, combo: &Combo) -> Result<Port, String> {
    let message = |e: ocio::Exception| e.message().to_string();
    let processor = port_processor(&calls.spec(combo.direction)).map_err(message)?;
    let cpu: Arc<CpuProcessor> = if combo.fast_math {
        processor.default_cpu_processor()
    } else {
        processor.optimized_cpu_processor(default_without_fast_math())
    }
    .map_err(message)?;
    Ok(Port::in_place(move |px| apply_f32_rgba(&cpu, px)))
}

/// A transform class through the API.
struct ApiFamily {
    name: &'static str,
    cases: Vec<Case<Calls>>,
    bases: Vec<Case<Calls>>,
}

impl ApiFamily {
    fn new(name: &'static str, cases: Cases) -> ApiFamily {
        ApiFamily {
            name,
            cases: cases.cases,
            bases: cases.bases,
        }
    }
}

impl Family for ApiFamily {
    type Params = Calls;

    fn name(&self) -> String {
        format!("{} through the API", self.name)
    }
    fn cases(&self) -> Vec<Case<Calls>> {
        self.cases.clone()
    }
    fn mutation_bases(&self) -> Vec<Case<Calls>> {
        self.bases.clone()
    }
    fn spec(&self, p: &Calls, direction: Direction) -> Spec {
        Spec::Transform(p.spec(direction))
    }
    fn port(&self, p: &Calls, combo: &Combo) -> Result<Port, String> {
        api_port(p, combo)
    }
    fn validation(&self) -> Validation {
        Validation::Ported
    }
}

#[test]
fn matrix_transform_through_the_api_matches_the_wheel() {
    battery::run(&ApiFamily::new("MatrixTransform", api_cases::matrix()));
}

#[test]
fn range_transform_through_the_api_matches_the_wheel() {
    battery::run(&ApiFamily::new("RangeTransform", api_cases::range()));
}

#[test]
fn cdl_transform_through_the_api_matches_the_wheel() {
    battery::run(&ApiFamily::new("CDLTransform", api_cases::cdl()));
}

#[test]
fn log_transform_through_the_api_matches_the_wheel() {
    battery::run(&ApiFamily::new("LogTransform", api_cases::log()));
}

#[test]
fn log_affine_transform_through_the_api_matches_the_wheel() {
    battery::run(&ApiFamily::new(
        "LogAffineTransform",
        api_cases::log_affine(),
    ));
}

#[test]
fn log_camera_transform_through_the_api_matches_the_wheel() {
    battery::run(&ApiFamily::new(
        "LogCameraTransform",
        api_cases::log_camera(),
    ));
}

#[test]
fn exponent_transform_through_the_api_matches_the_wheel() {
    battery::run(&ApiFamily::new("ExponentTransform", api_cases::exponent()));
}

#[test]
fn exponent_with_linear_transform_through_the_api_matches_the_wheel() {
    battery::run(&ApiFamily::new(
        "ExponentWithLinearTransform",
        api_cases::exponent_with_linear(),
    ));
}

#[test]
fn allocation_transform_through_the_api_matches_the_wheel() {
    battery::run(&ApiFamily::new(
        "AllocationTransform",
        api_cases::allocation(),
    ));
}

#[test]
fn group_transform_through_the_api_matches_the_wheel() {
    battery::run(&ApiFamily::new("GroupTransform", api_cases::group()));
}

#[test]
fn fixed_function_transform_through_the_api_matches_the_wheel() {
    battery::run(&ApiFamily::new(
        "FixedFunctionTransform",
        api_cases::fixed_function(),
    ));
}

/// The built-in transforms whose ops are ported (WP 3.2e-g): the identity.
const BUILTINS: &[&str] = &["IDENTITY"];

#[test]
fn builtin_transform_through_the_api_matches_the_wheel() {
    battery::run(&ApiFamily::new(
        "BuiltinTransform",
        api_cases::builtin(BUILTINS),
    ));
}
