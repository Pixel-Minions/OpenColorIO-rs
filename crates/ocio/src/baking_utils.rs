// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The processors and ranges a baker samples. A port of `src/OpenColorIO/BakingUtils.cpp` and
//! `BakingUtils.h` @ v2.5.2.

use std::sync::Arc;

use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::exception::{Exception, Result};
use ocio_ops::math_utils::{std_max, std_min};
use ocio_ops::open_color_types::{OptimizationFlags, TransformDirection};

use crate::baker::Baker;
use crate::transform::Transform;
use crate::transforms::color_space_transform::ColorSpaceTransform;
use crate::transforms::display_view_transform::DisplayViewTransform;
use crate::transforms::group_transform::GroupTransform;
use crate::transforms::look_transform::LookTransform;

/// The baker's config: [`Baker::bake`] checks there is one before a format bakes.
fn config(baker: &Baker) -> Result<&crate::config::Config> {
    baker
        .config()
        .ok_or_else(|| Exception::new("No OCIO config has been set."))
}

/// The transform from the input space to the target space, or to the display and view: the
/// looks, then the display/view transform bypassing them; or a look transform to the target
/// space.
///
/// Port of `GetInputToTargetTransform` (BakingUtils.cpp:11-51 @ v2.5.2).
fn get_input_to_target_transform(baker: &Baker) -> GroupTransform {
    let input = baker.input_space();
    let looks = baker.looks();
    let display = baker.display();
    let view = baker.view();

    let mut group = GroupTransform::new();

    if !display.is_empty() && !view.is_empty() {
        if !looks.is_empty() {
            let mut look = LookTransform::new();
            look.set_looks(looks);
            look.set_src(input);
            look.set_dst(input);

            group.append_transform(Transform::from(look));
        }

        let mut disp = DisplayViewTransform::new();
        disp.set_src(input);
        disp.set_display(display);
        disp.set_view(view);
        disp.set_looks_bypass(!looks.is_empty());

        group.append_transform(Transform::from(disp));
    } else {
        let mut look = LookTransform::new();
        look.set_looks(if !looks.is_empty() { looks } else { b"" });
        look.set_src(input);
        look.set_dst(baker.target_space());

        group.append_transform(Transform::from(look));
    }

    group
}

/// The smallest and the largest of the input-space values of `src`'s 0 and 1: the range a
/// shaper or a target space covers.
///
/// Port of `GetSrcRange` (BakingUtils.cpp:53-72 @ v2.5.2).
fn get_src_range(baker: &Baker, src: &[u8]) -> Result<(f32, f32)> {
    // Calculate min/max value
    let proc = config(baker)?.processor_between(src, baker.input_space())?;
    let cpu = proc.optimized_cpu_processor(OptimizationFlags::LOSSLESS)?;

    let mut minval = [0.0f32, 0.0, 0.0];
    let mut maxval = [1.0f32, 1.0, 1.0];

    cpu.apply_rgb(&mut minval)?;
    cpu.apply_rgb(&mut maxval)?;

    let start = std_min(std_min(minval[0], minval[1]), minval[2]);
    let end = std_max(std_max(maxval[0], maxval[1]), maxval[2]);
    Ok((start, end))
}

/// The CPU processor from the input space to the shaper space, lossless.
///
/// Port of `GetInputToShaperProcessor` (BakingUtils.cpp:76-81 @ v2.5.2).
pub fn get_input_to_shaper_processor(baker: &Baker) -> Result<Arc<CpuProcessor>> {
    let processor = config(baker)?.processor_between(baker.input_space(), baker.shaper_space())?;
    processor.optimized_cpu_processor(OptimizationFlags::LOSSLESS)
}

/// The CPU processor from the shaper space to the input space, lossless.
///
/// Port of `GetShaperToInputProcessor` (BakingUtils.cpp:83-88 @ v2.5.2).
pub fn get_shaper_to_input_processor(baker: &Baker) -> Result<Arc<CpuProcessor>> {
    let processor = config(baker)?.processor_between(baker.shaper_space(), baker.input_space())?;
    processor.optimized_cpu_processor(OptimizationFlags::LOSSLESS)
}

/// The CPU processor from the input space to the target (or the display and view), lossless.
///
/// Port of `GetInputToTargetProcessor` (BakingUtils.cpp:90-101 @ v2.5.2).
pub fn get_input_to_target_processor(baker: &Baker) -> Result<Arc<CpuProcessor>> {
    if !baker.input_space().is_empty() {
        let processor = config(baker)?.processor_in_direction(
            &Transform::from(get_input_to_target_transform(baker)),
            TransformDirection::Forward,
        )?;
        return processor.optimized_cpu_processor(OptimizationFlags::LOSSLESS);
    }

    Err(Exception::new("Input space is empty."))
}

/// The CPU processor from the shaper space to the target (through the input space),
/// lossless.
///
/// Port of `GetShaperToTargetProcessor` (BakingUtils.cpp:103-121 @ v2.5.2).
pub fn get_shaper_to_target_processor(baker: &Baker) -> Result<Arc<CpuProcessor>> {
    if !baker.shaper_space().is_empty() {
        let mut csc = ColorSpaceTransform::new();
        csc.set_src(baker.shaper_space());
        csc.set_dst(baker.input_space());

        let mut group = get_input_to_target_transform(baker);
        group.prepend_transform(Transform::from(csc));

        let processor = config(baker)?
            .processor_in_direction(&Transform::from(group), TransformDirection::Forward)?;
        return processor.optimized_cpu_processor(OptimizationFlags::LOSSLESS);
    }

    Err(Exception::new("Shaper space is empty."))
}

/// The input-space range of the shaper space's 0 to 1: `(start, end)`.
///
/// Port of `GetShaperRange` (BakingUtils.cpp:123-126 @ v2.5.2).
pub fn get_shaper_range(baker: &Baker) -> Result<(f32, f32)> {
    get_src_range(baker, baker.shaper_space())
}

/// The input-space range of the target space's 0 to 1: `(start, end)`.
///
/// Port of `GetTargetRange` (BakingUtils.cpp:128-131 @ v2.5.2).
pub fn get_target_range(baker: &Baker) -> Result<(f32, f32)> {
    get_src_range(baker, baker.target_space())
}
