// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/gpu/Lut1DOp_test.cpp` @ v2.5.2.
//! Actual uncached wheel CPU and ordinary/native-legacy shader references are
//! rendered by the reviewed GpuUnitTest runner on the same renderer.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use std::sync::OnceLock;

use ocio::{FileTransform, Interpolation, Lut1DTransform, Transform, TransformDirection};
use ocio_gpu_exec::gpu_unit_test::{CustomValues, GpuUnitTest, ReferenceRun};
use ocio_testkit::upstream::gpu::{comparison_dimensions, input_digest};
use serde_json::{Value, json};

#[path = "support/suite2_reference.rs"]
mod reference;

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
    // Lut1DOp_test.cpp:49-57 @ v2.5.2.
    Case {
        name: "lut1d_1_small_legacy_shader",
        file: Some("lut1d_1.spi1d"),
        inverse: false,
        explicit_forward: false,
        interpolation: None,
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
    // Lut1DOp_test.cpp:59-68 @ v2.5.2.
    Case {
        name: "lut1d_1_small_inverse_legacy_shader",
        file: Some("lut1d_1.spi1d"),
        inverse: true,
        explicit_forward: false,
        interpolation: None,
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
    // Lut1DOp_test.cpp:70-77 @ v2.5.2.
    Case {
        name: "lut1d_1_small_generic_shader",
        file: Some("lut1d_1.spi1d"),
        inverse: false,
        explicit_forward: false,
        interpolation: None,
        legacy: false,
        edge: 32,
        threshold: 5e-6,
        wide: true,
        nan: true,
        infinity: true,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
    },
    // Lut1DOp_test.cpp:79-88 @ v2.5.2.
    Case {
        name: "lut1d_1_small_inverse_generic_shader",
        file: Some("lut1d_1.spi1d"),
        inverse: true,
        explicit_forward: false,
        interpolation: None,
        legacy: false,
        edge: 32,
        threshold: 1e-6,
        wide: true,
        nan: false,
        infinity: true,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "d42def2ca316f1777ee68a7b78242178bd7a67b6d7cd1188f9ac79ac7ea3d489",
    },
    // Lut1DOp_test.cpp:90-99 @ v2.5.2.
    Case {
        name: "lut1d_2_legacy_shader",
        file: Some("lut1d_2.spi1d"),
        inverse: false,
        explicit_forward: false,
        interpolation: None,
        legacy: true,
        edge: 64,
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
    // Lut1DOp_test.cpp:101-111 @ v2.5.2.
    Case {
        name: "lut1d_2_inverse_legacy_shader",
        file: Some("lut1d_2.spi1d"),
        inverse: true,
        explicit_forward: false,
        interpolation: None,
        legacy: true,
        edge: 64,
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
    // Lut1DOp_test.cpp:113-124 @ v2.5.2.
    Case {
        name: "lut1d_2_generic_shader",
        file: Some("lut1d_2.spi1d"),
        inverse: false,
        explicit_forward: false,
        interpolation: None,
        legacy: false,
        edge: 32,
        threshold: 1e-6,
        wide: true,
        nan: false,
        infinity: true,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "d42def2ca316f1777ee68a7b78242178bd7a67b6d7cd1188f9ac79ac7ea3d489",
    },
    // Lut1DOp_test.cpp:126-135 @ v2.5.2.
    Case {
        name: "lut1d_2_inverse_generic_shader",
        file: Some("lut1d_2.spi1d"),
        inverse: true,
        explicit_forward: false,
        interpolation: None,
        legacy: false,
        edge: 32,
        threshold: 1e-4,
        wide: true,
        nan: false,
        infinity: true,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "d42def2ca316f1777ee68a7b78242178bd7a67b6d7cd1188f9ac79ac7ea3d489",
    },
    // Lut1DOp_test.cpp:137-145 @ v2.5.2.
    Case {
        name: "lut1d_3_big_legacy_shader",
        file: Some("lut1d_3.spi1d"),
        inverse: false,
        explicit_forward: false,
        interpolation: None,
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
    // Lut1DOp_test.cpp:147-156 @ v2.5.2.
    Case {
        name: "lut1d_3_big_inverse_legacy_shader",
        file: Some("lut1d_3.spi1d"),
        inverse: true,
        explicit_forward: false,
        interpolation: None,
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
    // Lut1DOp_test.cpp:158-166 @ v2.5.2.
    Case {
        name: "lut1d_3_big_generic_shader",
        file: Some("lut1d_3.spi1d"),
        inverse: false,
        explicit_forward: false,
        interpolation: None,
        legacy: false,
        edge: 32,
        threshold: 1e-6,
        wide: true,
        nan: false,
        infinity: true,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "d42def2ca316f1777ee68a7b78242178bd7a67b6d7cd1188f9ac79ac7ea3d489",
    },
    // Lut1DOp_test.cpp:168-177 @ v2.5.2.
    Case {
        name: "lut1d_3_big_inverse_generic_shader",
        file: Some("lut1d_3.spi1d"),
        inverse: true,
        explicit_forward: false,
        interpolation: None,
        legacy: false,
        edge: 32,
        threshold: 1e-6,
        wide: true,
        nan: false,
        infinity: true,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "d42def2ca316f1777ee68a7b78242178bd7a67b6d7cd1188f9ac79ac7ea3d489",
    },
    // Lut1DOp_test.cpp:179-188 @ v2.5.2.
    Case {
        name: "lut1d_3_big_nearest_generic_shader",
        file: Some("lut1d_3.spi1d"),
        inverse: false,
        explicit_forward: false,
        interpolation: Some(Interpolation::Nearest),
        legacy: false,
        edge: 32,
        threshold: 1e-6,
        wide: true,
        nan: false,
        infinity: true,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "d42def2ca316f1777ee68a7b78242178bd7a67b6d7cd1188f9ac79ac7ea3d489",
    },
    // Lut1DOp_test.cpp:190-206 @ v2.5.2.
    Case {
        name: "scale_lut1d_4_legacy_shader",
        file: Some("lut1d_4.spi1d"),
        inverse: false,
        explicit_forward: false,
        interpolation: None,
        legacy: true,
        edge: 64,
        threshold: 2e-4,
        wide: false,
        nan: false,
        infinity: false,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "394608a63bca0842914575c2b977347e75db40ea3f5f085a2b788687effaf4d2",
    },
    // Lut1DOp_test.cpp:208-223 @ v2.5.2.
    Case {
        name: "scale_lut1d_4_inverse_legacy_shader",
        file: Some("lut1d_4.spi1d"),
        inverse: true,
        explicit_forward: false,
        interpolation: None,
        legacy: true,
        edge: 64,
        threshold: 5e-5,
        wide: false,
        nan: false,
        infinity: false,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "394608a63bca0842914575c2b977347e75db40ea3f5f085a2b788687effaf4d2",
    },
    // Lut1DOp_test.cpp:225-235 @ v2.5.2.
    Case {
        name: "scale_lut1d_4_generic_shader",
        file: Some("lut1d_4.spi1d"),
        inverse: false,
        explicit_forward: false,
        interpolation: None,
        legacy: false,
        edge: 32,
        threshold: 1e-4,
        wide: true,
        nan: false,
        infinity: true,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "d42def2ca316f1777ee68a7b78242178bd7a67b6d7cd1188f9ac79ac7ea3d489",
    },
    // Lut1DOp_test.cpp:237-248 @ v2.5.2.
    Case {
        name: "scale_lut1d_4_inverse_generic_shader",
        file: Some("lut1d_4.spi1d"),
        inverse: true,
        explicit_forward: false,
        interpolation: None,
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
    // Lut1DOp_test.cpp:250-260 @ v2.5.2.
    Case {
        name: "not_linear_lut1d_5_generic_shader",
        file: Some("lut1d_5.spi1d"),
        inverse: false,
        explicit_forward: false,
        interpolation: None,
        legacy: false,
        edge: 32,
        threshold: 5e-4,
        wide: true,
        nan: true,
        infinity: true,
        relative: true,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
    },
    // Lut1DOp_test.cpp:262-270 @ v2.5.2.
    Case {
        name: "not_linear_lut1d_5_inverse_generic_shader",
        file: Some("lut1d_5.spi1d"),
        inverse: true,
        explicit_forward: false,
        interpolation: None,
        legacy: false,
        edge: 32,
        threshold: 1e-6,
        wide: true,
        nan: true,
        infinity: true,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
    },
    // Lut1DOp_test.cpp:272-283 @ v2.5.2.
    Case {
        name: "lut1d_half_domain_unequal_channels",
        file: Some("lut1d_halfdom.ctf"),
        inverse: false,
        explicit_forward: true,
        interpolation: None,
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
    // Lut1DOp_test.cpp:285-330 @ v2.5.2.
    Case {
        name: "lut1d_half_domain_negative_zero",
        file: None,
        inverse: false,
        explicit_forward: false,
        interpolation: None,
        legacy: false,
        edge: 32,
        threshold: 2e-3,
        wide: true,
        nan: false,
        infinity: false,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: true,
        input_digest: "eb480af430e43e79a172719569b29db494cce303b2ecb76a158c90af170b8ea3",
    },
    // Lut1DOp_test.cpp:332-342 @ v2.5.2.
    Case {
        name: "lut1d_file2_test",
        file: Some("lut1d_green.ctf"),
        inverse: false,
        explicit_forward: true,
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
    // Lut1DOp_test.cpp:344-357 @ v2.5.2.
    Case {
        name: "lut1d_file2_disallow_tex1D_test",
        file: Some("lut1d_green.ctf"),
        inverse: false,
        explicit_forward: true,
        interpolation: None,
        legacy: false,
        edge: 32,
        threshold: 1e-4,
        wide: true,
        nan: true,
        infinity: true,
        relative: false,
        minimum: 1e-6,
        allow_1d: false,
        custom: false,
        input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
    },
    // Lut1DOp_test.cpp:359-372 @ v2.5.2.
    Case {
        name: "lut1d_hue_adjust_test",
        file: Some("lut1d_1024_hue_adjust_test.ctf"),
        inverse: false,
        explicit_forward: true,
        interpolation: None,
        legacy: false,
        edge: 32,
        threshold: 1e-5,
        wide: true,
        nan: false,
        infinity: false,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "ea88cf946d9444bfa680a9b6ca485dba98a6362fe9f286d503d218dd6b75be94",
    },
    // Lut1DOp_test.cpp:374-390 @ v2.5.2.
    Case {
        name: "lut1d_half_domain_hue_adjust_test",
        file: Some("lut1d_hd_hue_adjust.ctf"),
        inverse: false,
        explicit_forward: true,
        interpolation: None,
        legacy: false,
        edge: 32,
        threshold: 1e-6,
        wide: true,
        nan: false,
        infinity: false,
        relative: true,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "ea88cf946d9444bfa680a9b6ca485dba98a6362fe9f286d503d218dd6b75be94",
    },
    // Lut1DOp_test.cpp:392-404 @ v2.5.2.
    Case {
        name: "lut1d_inverse_file1_test",
        file: Some("lut1d_inv.ctf"),
        inverse: false,
        explicit_forward: true,
        interpolation: None,
        legacy: false,
        edge: 32,
        threshold: 1e-4,
        wide: true,
        nan: false,
        infinity: false,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "ea88cf946d9444bfa680a9b6ca485dba98a6362fe9f286d503d218dd6b75be94",
    },
    // Lut1DOp_test.cpp:406-418 @ v2.5.2.
    Case {
        name: "lut1d_inverse_file2_test",
        file: Some("lut1d_inverse_gpu.ctf"),
        inverse: false,
        explicit_forward: true,
        interpolation: None,
        legacy: false,
        edge: 32,
        threshold: 1e-6,
        wide: true,
        nan: false,
        infinity: true,
        relative: false,
        minimum: 1e-6,
        allow_1d: true,
        custom: false,
        input_digest: "d42def2ca316f1777ee68a7b78242178bd7a67b6d7cd1188f9ac79ac7ea3d489",
    },
    // Lut1DOp_test.cpp:420-433 @ v2.5.2.
    Case {
        name: "lut1d_inverse_half_file1_test",
        file: Some("lut1d_inverse_halfdom_slog_fclut.ctf"),
        inverse: false,
        explicit_forward: true,
        interpolation: None,
        legacy: false,
        edge: 32,
        threshold: 1e-4,
        wide: true,
        nan: false,
        infinity: true,
        relative: true,
        minimum: 1e-3,
        allow_1d: true,
        custom: false,
        input_digest: "d42def2ca316f1777ee68a7b78242178bd7a67b6d7cd1188f9ac79ac7ea3d489",
    },
    // Lut1DOp_test.cpp:435-446 @ v2.5.2.
    Case {
        name: "lut1d_inverse_half_hue_adjust_file1_test",
        file: Some("lut1d_inverse_hd_hueAdjust.ctf"),
        inverse: false,
        explicit_forward: true,
        interpolation: None,
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
];

// Literal inputs, not expected pixels: Lut1DOp_test.cpp:313-326 @ v2.5.2.
const CUSTOM: [f32; 16] = [
    0.00, 0.00, 0.000, 1.0, 3e-8, 9e-8, 15e-8, 0.0, -3e-8, -9e-8, -15e-8, 0.0, 0.50, 0.05, 0.005,
    0.5,
];
const CUSTOM_RAW_DIGEST: &str = "a1d6f6be15d659221a684056244539d21a6e3da384877503a974c0a5e2ca4c33";

/// Port of the half-domain constructor and denormal edits
/// (`Lut1DOp_test.cpp:295-307` @ v2.5.2); the wheel independently builds its LUT.
fn custom_transform() -> Transform {
    let mut lut = Lut1DTransform::with_length(65536, true).unwrap();
    for i in 0..1024 {
        let x = i as f32;
        lut.set_value(i, x, x, x).unwrap();
        lut.set_value(32768 + i, x + 10.0, x + 10.0, x + 10.0)
            .unwrap();
    }
    lut.into()
}

fn custom_transform_spec() -> Value {
    let mut calls = Vec::new();
    for i in 0..1024 {
        let x = i as f32;
        calls.push(json!(["setValue", i, x, x, x]));
        calls.push(json!(["setValue", 32768 + i, x + 10.0, x + 10.0, x + 10.0]));
    }
    json!({"class":"Lut1DTransform", "args":[65536,true], "calls":calls})
}

fn input_test(case: &Case) -> GpuUnitTest {
    let mut test = GpuUnitTest::new("Lut1DOp", case.name);
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
    if case.custom {
        assert_eq!(input_digest(&CUSTOM), CUSTOM_RAW_DIGEST);
        test.set_custom_values(CustomValues {
            input_values: CUSTOM.to_vec(),
            ..CustomValues::default()
        });
    }
    test.prepare_values().unwrap();
    assert_eq!(
        input_digest(&test.values.input_values),
        case.input_digest,
        "{}",
        case.name
    );
    assert_eq!(test.num_retests(), 0);
    if case.custom {
        // Four RGBA pixels; GPU input remains the fixed256x256 image. Only the
        // source4x1 rectangle reaches CPU/comparison, GPUUnitTest.cpp:396-420.
        assert_eq!(test.values.original_input_value_size, 16);
        assert_eq!(test.values.input_values.len(), 256 * 256 * 4);
        assert_eq!(
            comparison_dimensions(test.values.original_input_value_size),
            Ok((4, 1))
        );
        assert_eq!(test.comparison_entries().unwrap(), 16);
        for (actual, expected) in test.values.input_values[..16].iter().zip(CUSTOM) {
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
        assert!(
            test.values.input_values[16..]
                .iter()
                .all(|value| value.to_bits() == 0)
        );
    }
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
        assert!(case.custom);
        custom_transform_spec()
    };
    json!({"config":{"create":{"processor_cache":"off"}}, "transform":transform})
}

fn references() -> &'static [ReferenceRun] {
    static REFERENCES: OnceLock<Vec<ReferenceRun>> = OnceLock::new();
    REFERENCES.get_or_init(|| {
        assert_eq!(CASES.len(), 29);
        assert_eq!(CASES.iter().filter(|case| case.legacy).count(), 8);
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
        assert!(case.custom);
        custom_transform()
    };
    let mut test = input_test(case);
    test.set_processor_transform(&transform).unwrap();
    assert!(
        test.run(std::slice::from_ref(&references()[index]))
            .unwrap()
    );
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, lut1d_1_small_legacy_shader)` @ v2.5.2.
#[test]
fn lut1d_1_small_legacy_shader() {
    run_case(0, "lut1d_1_small_legacy_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, lut1d_1_small_inverse_legacy_shader)` @ v2.5.2.
#[test]
fn lut1d_1_small_inverse_legacy_shader() {
    run_case(1, "lut1d_1_small_inverse_legacy_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, lut1d_1_small_generic_shader)` @ v2.5.2.
#[test]
fn lut1d_1_small_generic_shader() {
    run_case(2, "lut1d_1_small_generic_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, lut1d_1_small_inverse_generic_shader)` @ v2.5.2.
#[test]
fn lut1d_1_small_inverse_generic_shader() {
    run_case(3, "lut1d_1_small_inverse_generic_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, lut1d_2_legacy_shader)` @ v2.5.2.
#[test]
fn lut1d_2_legacy_shader() {
    run_case(4, "lut1d_2_legacy_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, lut1d_2_inverse_legacy_shader)` @ v2.5.2.
#[test]
fn lut1d_2_inverse_legacy_shader() {
    run_case(5, "lut1d_2_inverse_legacy_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, lut1d_2_generic_shader)` @ v2.5.2.
#[test]
fn lut1d_2_generic_shader() {
    run_case(6, "lut1d_2_generic_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, lut1d_2_inverse_generic_shader)` @ v2.5.2.
#[test]
fn lut1d_2_inverse_generic_shader() {
    run_case(7, "lut1d_2_inverse_generic_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, lut1d_3_big_legacy_shader)` @ v2.5.2.
#[test]
fn lut1d_3_big_legacy_shader() {
    run_case(8, "lut1d_3_big_legacy_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, lut1d_3_big_inverse_legacy_shader)` @ v2.5.2.
#[test]
fn lut1d_3_big_inverse_legacy_shader() {
    run_case(9, "lut1d_3_big_inverse_legacy_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, lut1d_3_big_generic_shader)` @ v2.5.2.
#[test]
fn lut1d_3_big_generic_shader() {
    run_case(10, "lut1d_3_big_generic_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, lut1d_3_big_inverse_generic_shader)` @ v2.5.2.
#[test]
fn lut1d_3_big_inverse_generic_shader() {
    run_case(11, "lut1d_3_big_inverse_generic_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, lut1d_3_big_nearest_generic_shader)` @ v2.5.2.
#[test]
fn lut1d_3_big_nearest_generic_shader() {
    run_case(12, "lut1d_3_big_nearest_generic_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, scale_lut1d_4_legacy_shader)` @ v2.5.2.
#[test]
fn scale_lut1d_4_legacy_shader() {
    run_case(13, "scale_lut1d_4_legacy_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, scale_lut1d_4_inverse_legacy_shader)` @ v2.5.2.
#[test]
fn scale_lut1d_4_inverse_legacy_shader() {
    run_case(14, "scale_lut1d_4_inverse_legacy_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, scale_lut1d_4_generic_shader)` @ v2.5.2.
#[test]
fn scale_lut1d_4_generic_shader() {
    run_case(15, "scale_lut1d_4_generic_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, scale_lut1d_4_inverse_generic_shader)` @ v2.5.2.
#[test]
fn scale_lut1d_4_inverse_generic_shader() {
    run_case(16, "scale_lut1d_4_inverse_generic_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, not_linear_lut1d_5_generic_shader)` @ v2.5.2.
#[test]
fn not_linear_lut1d_5_generic_shader() {
    run_case(17, "not_linear_lut1d_5_generic_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, not_linear_lut1d_5_inverse_generic_shader)` @ v2.5.2.
#[test]
fn not_linear_lut1d_5_inverse_generic_shader() {
    run_case(18, "not_linear_lut1d_5_inverse_generic_shader");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, lut1d_half_domain_unequal_channels)` @ v2.5.2.
#[test]
fn lut1d_half_domain_unequal_channels() {
    run_case(19, "lut1d_half_domain_unequal_channels");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, lut1d_half_domain_negative_zero)` @ v2.5.2.
#[test]
fn lut1d_half_domain_negative_zero() {
    run_case(20, "lut1d_half_domain_negative_zero");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, lut1d_file2_test)` @ v2.5.2.
#[test]
fn lut1d_file2_test() {
    run_case(21, "lut1d_file2_test");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, lut1d_file2_disallow_tex1D_test)` @ v2.5.2.
#[test]
fn lut1d_file2_disallow_tex1d_test() {
    run_case(22, "lut1d_file2_disallow_tex1D_test");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, lut1d_hue_adjust_test)` @ v2.5.2.
#[test]
fn lut1d_hue_adjust_test() {
    run_case(23, "lut1d_hue_adjust_test");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, lut1d_half_domain_hue_adjust_test)` @ v2.5.2.
#[test]
fn lut1d_half_domain_hue_adjust_test() {
    run_case(24, "lut1d_half_domain_hue_adjust_test");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, lut1d_inverse_file1_test)` @ v2.5.2.
#[test]
fn lut1d_inverse_file1_test() {
    run_case(25, "lut1d_inverse_file1_test");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, lut1d_inverse_file2_test)` @ v2.5.2.
#[test]
fn lut1d_inverse_file2_test() {
    run_case(26, "lut1d_inverse_file2_test");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, lut1d_inverse_half_file1_test)` @ v2.5.2.
#[test]
fn lut1d_inverse_half_file1_test() {
    run_case(27, "lut1d_inverse_half_file1_test");
}

/// Port of `OCIO_ADD_GPU_TEST(Lut1DOp, lut1d_inverse_half_hue_adjust_file1_test)` @ v2.5.2.
#[test]
fn lut1d_inverse_half_hue_adjust_file1_test() {
    run_case(28, "lut1d_inverse_half_hue_adjust_file1_test");
}
