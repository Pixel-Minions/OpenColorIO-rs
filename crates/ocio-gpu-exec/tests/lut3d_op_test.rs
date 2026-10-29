// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/gpu/Lut3DOp_test.cpp` @ v2.5.2.
//! Actual uncached wheel CPU and ordinary/native-legacy shader references are
//! rendered by the reviewed GpuUnitTest runner on the same renderer.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use std::sync::OnceLock;

use ocio::{FileTransform, Interpolation, Lut3DTransform, TransformDirection};
use ocio_gpu_exec::gpu_unit_test::{GpuUnitTest, ReferenceRun};
use ocio_testkit::upstream::gpu::input_digest;
use serde_json::{Value, json};

#[path = "support/suite2_reference.rs"]
mod reference;

// Corrected owner P7-7 approval2026-10-26: all15 Linux cases run, including
// the four inverse3D functions. Upstream disables those only under
// !defined(NDEBUG) && defined(WIN32); no n/a, waiver or ignored test is used.

// Rows are direct transcriptions of each cited upstream setup. The common
// field defaults are GPUUnitTest.h:157-173, not inferred from port output.
#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    file: Option<&'static str>,
    inverse: bool,
    explicit_forward: bool,
    interpolation: Option<Interpolation>,
    legacy: bool,
    edge: u32,
    threshold: f32,
    wide: bool,
    nan: bool,
    infinity: bool,
    relative: bool,
    minimum: f32,
    allow_1d: bool,
    custom: bool,
    input_digest: &'static str,
}

const CASES: &[Case] = &[
    // Lut3DOp_test.cpp:41-54 @ v2.5.2.
    Case {
        name: "red_only_using_CSP_file_legacy_shader",
        file: Some("lut3d_red-only.csp"),
        inverse: false,
        explicit_forward: false,
        interpolation: Some(Interpolation::Linear),
        legacy: true,
        edge: 32,
        threshold: 2e-4,
        wide: true,
        nan: true,
        infinity: true,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
    },
    // Lut3DOp_test.cpp:56-69 @ v2.5.2.
    Case {
        name: "green_only_using_CSP_file_legacy_shader",
        file: Some("lut3d_grn-only.csp"),
        inverse: false,
        explicit_forward: false,
        interpolation: Some(Interpolation::Linear),
        legacy: true,
        edge: 32,
        threshold: 2e-4,
        wide: true,
        nan: true,
        infinity: true,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
    },
    // Lut3DOp_test.cpp:71-85 @ v2.5.2.
    Case {
        name: "blue_only_using_CSP_file_legacy_shader",
        file: Some("lut3d_blu-only.csp"),
        inverse: false,
        explicit_forward: false,
        interpolation: Some(Interpolation::Linear),
        legacy: true,
        edge: 32,
        threshold: 2e-4,
        wide: true,
        nan: true,
        infinity: true,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
    },
    // Lut3DOp_test.cpp:88-101 @ v2.5.2.
    Case {
        name: "arbitrary_using_CSP_file_legacy_shader",
        file: Some("lut3d_arbitrary.csp"),
        inverse: false,
        explicit_forward: false,
        interpolation: Some(Interpolation::Linear),
        legacy: true,
        edge: 32,
        threshold: 2e-4,
        wide: true,
        nan: true,
        infinity: true,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
    },
    // Lut3DOp_test.cpp:104-118 @ v2.5.2.
    Case {
        name: "arbitrary_using_CSP_file",
        file: Some("lut3d_arbitrary.csp"),
        inverse: false,
        explicit_forward: false,
        interpolation: Some(Interpolation::Linear),
        legacy: false,
        edge: 32,
        threshold: 1e-2,
        wide: true,
        nan: true,
        infinity: true,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
    },
    // Lut3DOp_test.cpp:121-129 @ v2.5.2.
    Case {
        name: "3dlut_file_legacy_shader",
        file: Some("lut3d_1.spi3d"),
        inverse: false,
        explicit_forward: false,
        interpolation: None,
        legacy: true,
        edge: 32,
        threshold: 5e-4,
        wide: true,
        nan: true,
        infinity: true,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
    },
    // Lut3DOp_test.cpp:131-139 @ v2.5.2.
    Case {
        name: "3dlut_file_spi3d_linear",
        file: Some("lut3d_1.spi3d"),
        inverse: false,
        explicit_forward: false,
        interpolation: None,
        legacy: false,
        edge: 32,
        threshold: 5e-4,
        wide: true,
        nan: true,
        infinity: true,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
    },
    // Lut3DOp_test.cpp:141-154 @ v2.5.2.
    Case {
        name: "3dlut_file_spi3d_tetra",
        file: Some("lut3d_1.spi3d"),
        inverse: false,
        explicit_forward: false,
        interpolation: Some(Interpolation::Tetrahedral),
        legacy: false,
        edge: 32,
        threshold: 1e-6,
        wide: true,
        nan: false,
        infinity: false,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "ea88cf946d9444bfa680a9b6ca485dba98a6362fe9f286d503d218dd6b75be94",
    },
    // Lut3DOp_test.cpp:156-171 @ v2.5.2.
    Case {
        name: "inv3dlut_file_spi3d_linear",
        file: Some("lut3d_1.spi3d"),
        inverse: true,
        explicit_forward: false,
        interpolation: None,
        legacy: false,
        edge: 32,
        threshold: 1.2e-3,
        wide: true,
        nan: true,
        infinity: true,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
    },
    // Lut3DOp_test.cpp:173-187 @ v2.5.2.
    Case {
        name: "inv3dlut_file_spi3d_tetra",
        file: Some("lut3d_1.spi3d"),
        inverse: true,
        explicit_forward: false,
        interpolation: Some(Interpolation::Tetrahedral),
        legacy: false,
        edge: 32,
        threshold: 1.2e-3,
        wide: true,
        nan: true,
        infinity: true,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
    },
    // Lut3DOp_test.cpp:189-199 @ v2.5.2.
    Case {
        name: "3dlut_file_spi3d_bizarre_linear",
        file: Some("lut3d_bizarre.spi3d"),
        inverse: false,
        explicit_forward: false,
        interpolation: None,
        legacy: false,
        edge: 32,
        threshold: 1e-2,
        wide: true,
        nan: true,
        infinity: true,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
    },
    // Lut3DOp_test.cpp:201-212 @ v2.5.2.
    Case {
        name: "3dlut_file_spi3d_bizarre_tetra",
        file: Some("lut3d_bizarre.spi3d"),
        inverse: false,
        explicit_forward: false,
        interpolation: Some(Interpolation::Tetrahedral),
        legacy: false,
        edge: 32,
        threshold: 1e-6,
        wide: true,
        nan: false,
        infinity: false,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "ea88cf946d9444bfa680a9b6ca485dba98a6362fe9f286d503d218dd6b75be94",
    },
    // Lut3DOp_test.cpp:214-225 @ v2.5.2.
    Case {
        name: "inv3dlut_file_spi3d_bizarre_linear",
        file: Some("lut3d_bizarre.spi3d"),
        inverse: true,
        explicit_forward: false,
        interpolation: None,
        legacy: false,
        edge: 32,
        threshold: 3e-4,
        wide: true,
        nan: true,
        infinity: true,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
    },
    // Lut3DOp_test.cpp:227-239 @ v2.5.2.
    Case {
        name: "inv3dlut_file_spi3d_bizarre_tetra",
        file: Some("lut3d_bizarre.spi3d"),
        inverse: true,
        explicit_forward: false,
        interpolation: Some(Interpolation::Tetrahedral),
        legacy: false,
        edge: 32,
        threshold: 3e-4,
        wide: true,
        nan: true,
        infinity: true,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
    },
    // Lut3DOp_test.cpp:241-250 @ v2.5.2.
    Case {
        name: "3dlut_biggest_supported",
        file: None,
        inverse: false,
        explicit_forward: false,
        interpolation: None,
        legacy: false,
        edge: 32,
        threshold: 1e-4,
        wide: true,
        nan: true,
        infinity: true,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
    },
];

fn input_test(case: &Case) -> GpuUnitTest {
    let mut test = GpuUnitTest::new("Lut3DOp", case.name);
    test.comparison.error_threshold = case.threshold;
    test.comparison.relative = case.relative;
    test.comparison.expected_minimal_value = case.minimum;
    test.test_wide_range = case.wide;
    test.test_nan = case.nan;
    test.test_infinity = case.infinity;
    test.legacy_shader = case.legacy;
    test.legacy_shader_lut_edge = case.edge;
    if !case.allow_1d {
        // Original setter arguments; C applies this once when creating the desc.
        test.shader_settings.allow_texture_1d = Some(false);
    }
    assert!(!case.custom);
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

/// Port of GetFileTransform's absolute path construction (Lut1D/Lut3D:35-42).
fn file_path(name: &str) -> String {
    let path = format!("{}/{name}", reference::files_directory());
    assert!(std::path::Path::new(&path).is_file(), "{path}");
    path
}

/// Port of GetFileTransform and each case's direction/interpolation setters.
fn file_transform(case: &Case) -> FileTransform {
    let mut file = FileTransform::new();
    file.set_src(file_path(case.file.unwrap()));
    if case.inverse {
        file.set_direction(TransformDirection::Inverse);
    } else if case.explicit_forward {
        file.set_direction(TransformDirection::Forward);
    }
    if let Some(interpolation) = case.interpolation {
        file.set_interpolation(interpolation);
    }
    file
}

fn processor_spec(case: &Case) -> Value {
    let transform = if let Some(filename) = case.file {
        let mut calls = vec![json!(["setSrc", file_path(filename)])];
        if case.inverse {
            calls.push(json!(["setDirection", {"enum":"TRANSFORM_DIR_INVERSE"}]));
        } else if case.explicit_forward {
            calls.push(json!(["setDirection", {"enum":"TRANSFORM_DIR_FORWARD"}]));
        }
        if let Some(interpolation) = case.interpolation {
            let name = match interpolation {
                Interpolation::Nearest => "INTERP_NEAREST",
                Interpolation::Linear => "INTERP_LINEAR",
                Interpolation::Tetrahedral => "INTERP_TETRAHEDRAL",
                other => panic!("unexpected upstream interpolation {other:?}"),
            };
            calls.push(json!(["setInterpolation", {"enum":name}]));
        }
        json!({"class":"FileTransform", "calls":calls})
    } else {
        assert_eq!(case.name, "3dlut_biggest_supported");
        // LutLimits.h:15 @ v2.5.2. Wheel builds its own identity cube.
        json!({"class":"Lut3DTransform", "calls":[["setGridSize",129]]})
    };
    json!({"config":{"create":{"processor_cache":"off"}}, "transform":transform})
}

fn references() -> &'static [ReferenceRun] {
    static REFERENCES: OnceLock<Vec<ReferenceRun>> = OnceLock::new();
    REFERENCES.get_or_init(|| {
        assert_eq!(CASES.len(), 15);
        assert_eq!(CASES.iter().filter(|case| case.legacy).count(), 5);
        let tests: Vec<_> = CASES.iter().map(input_test).collect();
        let processors: Vec<_> = CASES.iter().map(processor_spec).collect();
        reference::batch(&tests, &processors)
    })
}

fn run_case(index: usize, name: &str) {
    let case = &CASES[index];
    assert_eq!(case.name, name);
    let transform = if case.file.is_some() {
        file_transform(case).into()
    } else {
        assert_eq!(case.name, "3dlut_biggest_supported");
        let mut lut = Lut3DTransform::new();
        lut.set_grid_size(129).unwrap(); // OCIO::Max3DLUTLength, LutLimits.h:15.
        lut.into()
    };
    let mut test = input_test(case);
    test.set_processor_transform(&transform).unwrap();
    assert!(
        test.run(std::slice::from_ref(&references()[index]))
            .unwrap()
    );
}

/// Port of `OCIO_ADD_GPU_TEST(Lut3DOp, red_only_using_CSP_file_legacy_shader)` @ v2.5.2.
#[test]
fn red_only_using_csp_file_legacy_shader() {
    run_case(0, "red_only_using_CSP_file_legacy_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut3DOp, green_only_using_CSP_file_legacy_shader)` @ v2.5.2.
#[test]
fn green_only_using_csp_file_legacy_shader() {
    run_case(1, "green_only_using_CSP_file_legacy_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut3DOp, blue_only_using_CSP_file_legacy_shader)` @ v2.5.2.
#[test]
fn blue_only_using_csp_file_legacy_shader() {
    run_case(2, "blue_only_using_CSP_file_legacy_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut3DOp, arbitrary_using_CSP_file_legacy_shader)` @ v2.5.2.
#[test]
fn arbitrary_using_csp_file_legacy_shader() {
    run_case(3, "arbitrary_using_CSP_file_legacy_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut3DOp, arbitrary_using_CSP_file)` @ v2.5.2.
#[test]
fn arbitrary_using_csp_file() {
    run_case(4, "arbitrary_using_CSP_file");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut3DOp, 3dlut_file_legacy_shader)` @ v2.5.2.
#[test]
fn lut3d_3dlut_file_legacy_shader() {
    run_case(5, "3dlut_file_legacy_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut3DOp, 3dlut_file_spi3d_linear)` @ v2.5.2.
#[test]
fn lut3d_3dlut_file_spi3d_linear() {
    run_case(6, "3dlut_file_spi3d_linear");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut3DOp, 3dlut_file_spi3d_tetra)` @ v2.5.2.
#[test]
fn lut3d_3dlut_file_spi3d_tetra() {
    run_case(7, "3dlut_file_spi3d_tetra");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut3DOp, inv3dlut_file_spi3d_linear)` @ v2.5.2.
#[test]
fn inv3dlut_file_spi3d_linear() {
    run_case(8, "inv3dlut_file_spi3d_linear");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut3DOp, inv3dlut_file_spi3d_tetra)` @ v2.5.2.
#[test]
fn inv3dlut_file_spi3d_tetra() {
    run_case(9, "inv3dlut_file_spi3d_tetra");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut3DOp, 3dlut_file_spi3d_bizarre_linear)` @ v2.5.2.
#[test]
fn lut3d_3dlut_file_spi3d_bizarre_linear() {
    run_case(10, "3dlut_file_spi3d_bizarre_linear");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut3DOp, 3dlut_file_spi3d_bizarre_tetra)` @ v2.5.2.
#[test]
fn lut3d_3dlut_file_spi3d_bizarre_tetra() {
    run_case(11, "3dlut_file_spi3d_bizarre_tetra");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut3DOp, inv3dlut_file_spi3d_bizarre_linear)` @ v2.5.2.
#[test]
fn inv3dlut_file_spi3d_bizarre_linear() {
    run_case(12, "inv3dlut_file_spi3d_bizarre_linear");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut3DOp, inv3dlut_file_spi3d_bizarre_tetra)` @ v2.5.2.
#[test]
fn inv3dlut_file_spi3d_bizarre_tetra() {
    run_case(13, "inv3dlut_file_spi3d_bizarre_tetra");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut3DOp, 3dlut_biggest_supported)` @ v2.5.2.
#[test]
fn lut3d_3dlut_biggest_supported() {
    run_case(14, "3dlut_biggest_supported");
}
