// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The test of upstream's tests/cpu/DynamicProperty_tests.cpp @ v2.5.2 that reads a file
//! (`LoadTransformFile`, which needs the CTF reader and the file transform). The others are in
//! `crates/ocio-ops/src/dynamic_property_tests.rs`.

use std::sync::Arc;

use ocio_ops::dynamic_property::DynamicPropertyRcPtr;
use ocio_ops::open_color_types::{DynamicPropertyType, OptimizationFlags, TransformDirection};
use ocio_testkit::upstream::{check_close, check_throw_what};

use crate::config::Config;
use crate::fileformats::test_utils::get_test_files_dir;
use crate::processor::Processor;
use crate::transform::Transform;
use crate::transforms::file_transform::FileTransform;

/// The processor of a forward file transform of the test file `file_name`, from an empty
/// config.
///
/// Port of `LoadTransformFile` (tests/cpu/DynamicProperty_tests.cpp:73-88 @ v2.5.2).
fn load_transform_file(file_name: &str) -> crate::Result<Arc<Processor>> {
    let file_path = format!("{}/{file_name}", get_test_files_dir());

    // Create a FileTransform.
    let mut file_transform = FileTransform::new();
    file_transform.set_direction(TransformDirection::Forward);
    file_transform.set_src(file_path);

    // Create empty Config to use.
    let config = Config::new()?;

    // Get the processor corresponding to the transform.
    config.processor(&Transform::from(file_transform))
}

// Test several aspects of dynamic properties, especially the ability to set
// values via the processor.
/// Port of `OCIO_ADD_TEST(DynamicProperty, get_dynamic_via_cpu_processor)` @ v2.5.2.
#[test]
fn get_dynamic_via_cpu_processor() {
    let ctf_file = "exposure_contrast_video_dp.ctf";

    let processor = load_transform_file(ctf_file).unwrap();

    let mut cpu_processor = processor.default_cpu_processor().unwrap();

    let mut pixel: [f32; 3] = [0.5, 0.4, 0.2];
    cpu_processor.apply_rgb(&mut pixel).unwrap();

    let error: f32 = 1e-5;
    check_close(pixel[0], 0.57495f32, error);
    check_close(pixel[1], 0.43988f32, error);
    check_close(pixel[2], 0.19147f32, error);

    let dpt = DynamicPropertyType::Exposure;
    let dp = cpu_processor.get_dynamic_property(dpt).unwrap();
    let DynamicPropertyRcPtr::Double(dp_double) = dp else {
        panic!("a double dynamic property")
    };
    let file_value = dp_double.get_value();
    dp_double.set_value(0.4);

    pixel[0] = 0.5;
    pixel[1] = 0.4;
    pixel[2] = 0.2;
    cpu_processor.apply_rgb(&mut pixel).unwrap();

    // Adjust error for SSE approximation.
    check_close(pixel[0], 0.62966f32, error * 2.0);
    check_close(pixel[1], 0.48175f32, error);
    check_close(pixel[2], 0.20969f32, error);

    // Restore file value.
    dp_double.set_value(file_value);

    pixel[0] = 0.5;
    pixel[1] = 0.4;
    pixel[2] = 0.2;
    cpu_processor.apply_rgb(&mut pixel).unwrap();

    check_close(pixel[0], 0.57495f32, error);
    check_close(pixel[1], 0.43988f32, error);
    check_close(pixel[2], 0.19147f32, error);

    // Note: The CTF does not define gamma as being dynamic.
    check_throw_what(
        cpu_processor.get_dynamic_property(DynamicPropertyType::Gamma),
        "Cannot find dynamic property",
    );

    // Get optimized CPU processor without dynamic properties.
    cpu_processor = processor
        .optimized_cpu_processor(OptimizationFlags::ALL)
        .unwrap();

    // Now the dynamic property can't be found.
    check_throw_what(
        cpu_processor.get_dynamic_property(dpt),
        "Cannot find dynamic property",
    );
}
