// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/gpu/Config_test.cpp` @ v2.5.2: all5 cases.
//! Preserve CreateFromStream/validate/getProcessor(raw,dst), default cache ON.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use std::sync::OnceLock;

use ocio::Config;
use ocio_gpu_exec::gpu_unit_test::{GpuUnitTest, ReferenceRun};
use ocio_testkit::upstream::gpu::input_digest;
use serde_json::{Value, json};

#[path = "support/suite2_reference.rs"]
mod reference;

/// Port of createConfig (`Config_test.cpp:30-76` @ v2.5.2).
fn create_config() -> String {
    let search_path = reference::files_directory();
    ["ocio_profile_version: 1\n\nsearch_path: ", search_path.as_str(), "/", "\nstrictparsing: true\nluma: [0.2126, 0.7152, 0.0722]\n\nroles:\n  default: raw\n  reference: raw\n  scene_linear: raw\n\ndisplays:\n  sRGB:\n    - !<View> {name: Lgh, colorspace: lgh}\n\ncolorspaces:\n  - !<ColorSpace>\n    name: raw\n    family: \"\"\n    equalitygroup: \"\"\n    bitdepth: unknown\n    isdata: false\n    allocation: uniform\n\n  - !<ColorSpace>\n    name: lgh\n    family: \"\"\n    equalitygroup: \"\"\n    bitdepth: unknown\n    isdata: false\n    allocation: uniform\n    allocationvars: [0, 1]\n    from_reference: !<GroupTransform>\n      children:\n"].concat()
}

struct Case {
    name: &'static str,
    yaml_tail: &'static str,
    dst: &'static str,
    threshold: f32,
    legacy: bool,
    nan: bool,
    infinity: bool,
    input_digest: &'static str,
    rename: bool,
    underscores: bool,
}

const CASES: &[Case] = &[
    // Config_test.cpp:78-95 @ v2.5.2.
    Case {
        name: "several_1D_luts_legacy_shader",
        yaml_tail: "        - !<FileTransform> {src: lut1d_1.spi1d, interpolation: linear}\n        - !<FileTransform> {src: lut1d_2.spi1d, interpolation: linear}\n",
        dst: "lgh",
        threshold: 1.0 / 256.0,
        legacy: true,
        nan: true,
        infinity: true,
        input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
        rename: false,
        underscores: false,
    },
    // Config_test.cpp:97-117 @ v2.5.2.
    Case {
        name: "several_1D_luts_generic_shader",
        yaml_tail: "        - !<FileTransform> {src: lut1d_1.spi1d, interpolation: linear}\n        - !<FileTransform> {src: lut1d_2.spi1d, interpolation: linear}\n",
        dst: "lgh",
        threshold: 1.0 / 256.0,
        legacy: false,
        nan: false,
        infinity: true,
        input_digest: "d42def2ca316f1777ee68a7b78242178bd7a67b6d7cd1188f9ac79ac7ea3d489",
        rename: false,
        underscores: false,
    },
    // Config_test.cpp:119-150 @ v2.5.2.
    Case {
        name: "arbitrary_generic_shader",
        yaml_tail: "        - !<FileTransform> {src: lut1d_1.spi1d, interpolation: linear}\n        - !<FileTransform> {src: lut1d_2.spi1d, interpolation: linear}\n        - !<LogTransform> {base: 10}\n        - !<MatrixTransform> {matrix: [0.075573, 0.022197,  0.00223,  0, 0.005901, 0.096928, -0.002829, 0, 0.016134, 0.007406,  0.07646,  0, 0,        0,         0,        1]}\n",
        dst: "lgh",
        threshold: 5e-3,
        legacy: false,
        nan: false,
        infinity: false,
        input_digest: "ea88cf946d9444bfa680a9b6ca485dba98a6362fe9f286d503d218dd6b75be94",
        rename: true,
        underscores: false,
    },
    // Config_test.cpp:152-176 @ v2.5.2.
    Case {
        name: "several_luts_generic_shader",
        yaml_tail: "        - !<FileTransform> {src: lut1d_1.spi1d, interpolation: linear}\n        - !<FileTransform> {src: lut1d_2.spi1d, interpolation: linear}\n        - !<FileTransform> {src: lut1d_1.spi1d, interpolation: linear}\n        - !<FileTransform> {src: lut1d_2.spi1d, interpolation: linear}\n        - !<FileTransform> {src: lut1d_1.spi1d, interpolation: linear}\n        - !<FileTransform> {src: lut1d_2.spi1d, interpolation: linear}\n        - !<FileTransform> {src: lut1d_3.spi1d, interpolation: linear}\n",
        dst: "lgh",
        threshold: 1.0 / 256.0,
        legacy: false,
        nan: false,
        infinity: true,
        input_digest: "d42def2ca316f1777ee68a7b78242178bd7a67b6d7cd1188f9ac79ac7ea3d489",
        rename: false,
        underscores: false,
    },
    // Config_test.cpp:178-221 @ v2.5.2.
    Case {
        name: "with_underscores",
        yaml_tail: "        - !<LogTransform> {base: 10}\n\n  - !<ColorSpace>\n    name: __lgh__\n    family: \"\"\n    equalitygroup: \"\"\n    bitdepth: unknown\n    isdata: false\n    allocation: uniform\n    allocationvars: [0, 1]\n    from_reference: !<GroupTransform>\n      children:\n        - !<MatrixTransform> {matrix: [0.075573, 0.022197,  0.00223,  0, 0.005901, 0.096928, -0.002829, 0, 0.016134, 0.007406,  0.07646,  0, 0,        0,         0,        1]}\n        - !<FileTransform> {src: lut1d_3.spi1d, interpolation: linear}\n",
        dst: "__lgh__",
        threshold: 1.0 / 256.0,
        legacy: false,
        nan: false,
        infinity: false,
        input_digest: "ea88cf946d9444bfa680a9b6ca485dba98a6362fe9f286d503d218dd6b75be94",
        rename: true,
        underscores: true,
    },
];

fn config_text(case: &Case) -> String {
    create_config() + case.yaml_tail
}

fn input_test(case: &Case) -> GpuUnitTest {
    let mut test = GpuUnitTest::new("Config", case.name);
    test.comparison.error_threshold = case.threshold;
    test.legacy_shader = case.legacy;
    test.test_nan = case.nan;
    test.test_infinity = case.infinity;
    if case.underscores {
        // Original raw setter arguments, normalized exactly once by C.
        test.shader_settings.resource_prefix = Some("ocio___".into());
        test.shader_settings.pixel_name = Some("another_pixel_name__".into());
        test.shader_settings.function_name = Some("__another_func_name____".into());
    } else if case.rename {
        test.shader_settings.pixel_name = Some("another_pixel_name".into());
        test.shader_settings.function_name = Some("another_func_name".into());
    }
    test.prepare_values().unwrap();
    assert_eq!(
        input_digest(&test.values.input_values),
        case.input_digest,
        "{}",
        case.name
    );
    assert_eq!(test.num_retests(), 0);
    test
}

fn processor_spec(case: &Case) -> Value {
    json!({"config":{"yaml":config_text(case)}, "src":"raw", "dst":case.dst})
}

fn references() -> &'static [ReferenceRun] {
    static REFERENCES: OnceLock<Vec<ReferenceRun>> = OnceLock::new();
    REFERENCES.get_or_init(|| {
        assert_eq!(CASES.len(), 5);
        assert_eq!(CASES.iter().filter(|case| case.legacy).count(), 1);
        let tests: Vec<_> = CASES.iter().map(input_test).collect();
        let processors: Vec<_> = CASES.iter().map(processor_spec).collect();
        reference::batch(&tests, &processors)
    })
}

/// Each case preserves validate then the named-colorspace processor overload.
fn run_case(index: usize, name: &str) {
    let case = &CASES[index];
    assert_eq!(case.name, name);
    let config = Config::create_from_stream(config_text(case).as_bytes()).unwrap();
    config.validate().unwrap();
    let processor = config.processor_between("raw", case.dst).unwrap();
    let mut test = input_test(case);
    test.set_processor(processor).unwrap();
    assert!(
        test.run(std::slice::from_ref(&references()[index]))
            .unwrap()
    );
}

/// Port of `OCIO_ADD_GPU_TEST(Config, several_1D_luts_legacy_shader)` @ v2.5.2.
#[test]
fn several_1d_luts_legacy_shader() {
    run_case(0, "several_1D_luts_legacy_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Config, several_1D_luts_generic_shader)` @ v2.5.2.
#[test]
fn several_1d_luts_generic_shader() {
    run_case(1, "several_1D_luts_generic_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Config, arbitrary_generic_shader)` @ v2.5.2.
#[test]
fn arbitrary_generic_shader() {
    run_case(2, "arbitrary_generic_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Config, several_luts_generic_shader)` @ v2.5.2.
#[test]
fn several_luts_generic_shader() {
    run_case(3, "several_luts_generic_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Config, with_underscores)` @ v2.5.2.
#[test]
fn with_underscores() {
    run_case(4, "with_underscores");
}
