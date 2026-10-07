// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the `Transform` dispatch: every arm's class, direction, validation and text through
//! the enum, and the processors' refusal to build the ops of the classes and built-in transforms
//! whose builders are not ported yet.

use super::*;
use crate::transforms::builtins::builtin_transform_registry::BuiltinTransformRegistry;
use crate::{Allocation, FixedFunctionStyle, RangeStyle, TransformType};

/// One transform of each class: a valid one, an invalid one where the class can be made
/// invalid (its `validate` then fails), and its class as upstream's `getTransformType`
/// overrides give it (include/OpenColorIO/OpenColorTransforms.h @ v2.5.2).
struct Arm {
    class: TransformType,
    valid: Transform,
    invalid: Option<Transform>,
}

fn arms() -> Vec<Arm> {
    let mut allocation_invalid = AllocationTransform::new();
    allocation_invalid.set_allocation(Allocation::Uniform);
    allocation_invalid.set_vars(&[0.0]);

    let mut cdl_invalid = CdlTransform::new();
    cdl_invalid.set_slope(&[-1.0, 1.0, 1.0]);
    let mut exponent_invalid = ExponentTransform::new();
    exponent_invalid.set_value(&[0.0, 1.0, 1.0, 1.0]);
    let mut ewl_invalid = ExponentWithLinearTransform::new();
    ewl_invalid.set_gamma(&[0.5, 1.0, 1.0, 1.0]);

    let mut cst = ColorSpaceTransform::new();
    cst.set_src("src");
    cst.set_dst("dst");

    let mut dvt = DisplayViewTransform::new();
    dvt.set_src("src");
    dvt.set_display("display");
    dvt.set_view("view");

    let mut ff_invalid =
        FixedFunctionTransform::new(FixedFunctionStyle::AcesRedMod03, &[]).unwrap();
    ff_invalid.set_params(&[1.0]);

    let mut ft = FileTransform::new();
    ft.set_src("lut.cube");

    let mut group = GroupTransform::new();
    group.append_transform(MatrixTransform::new().into());
    let mut group_invalid = GroupTransform::new();
    group_invalid.append_transform(FileTransform::new().into());

    let mut log_invalid = LogTransform::new();
    log_invalid.set_base(-1.0);
    let mut log_affine_invalid = LogAffineTransform::new();
    log_affine_invalid.set_base(-1.0);
    let mut log_camera_invalid = LogCameraTransform::new(&[0.1, 0.1, 0.1]);
    log_camera_invalid.set_base(-1.0);

    let mut lt = LookTransform::new();
    lt.set_src("src");
    lt.set_dst("dst");
    lt.set_looks("look");

    let mut range = RangeTransform::new();
    range.set_min_in_value(0.0);
    range.set_max_in_value(1.0);
    range.set_min_out_value(0.0);
    range.set_max_out_value(1.0);
    let mut range_invalid = RangeTransform::new();
    range_invalid.set_style(RangeStyle::NoClamp);

    vec![
        Arm {
            class: TransformType::Allocation,
            valid: AllocationTransform::new().into(),
            invalid: Some(allocation_invalid.into()),
        },
        Arm {
            class: TransformType::Builtin,
            valid: BuiltinTransform::new().into(),
            invalid: None,
        },
        Arm {
            class: TransformType::Cdl,
            valid: CdlTransform::new().into(),
            invalid: Some(cdl_invalid.into()),
        },
        Arm {
            class: TransformType::ColorSpace,
            valid: cst.into(),
            invalid: Some(ColorSpaceTransform::new().into()),
        },
        Arm {
            class: TransformType::DisplayView,
            valid: dvt.into(),
            invalid: Some(DisplayViewTransform::new().into()),
        },
        Arm {
            class: TransformType::Exponent,
            valid: ExponentTransform::new().into(),
            invalid: Some(exponent_invalid.into()),
        },
        Arm {
            class: TransformType::ExponentWithLinear,
            valid: ExponentWithLinearTransform::new().into(),
            invalid: Some(ewl_invalid.into()),
        },
        Arm {
            class: TransformType::File,
            valid: ft.into(),
            invalid: Some(FileTransform::new().into()),
        },
        Arm {
            class: TransformType::FixedFunction,
            valid: FixedFunctionTransform::new(FixedFunctionStyle::AcesRedMod03, &[])
                .unwrap()
                .into(),
            invalid: Some(ff_invalid.into()),
        },
        Arm {
            class: TransformType::Group,
            valid: group.into(),
            invalid: Some(group_invalid.into()),
        },
        Arm {
            class: TransformType::LogAffine,
            valid: LogAffineTransform::new().into(),
            invalid: Some(log_affine_invalid.into()),
        },
        Arm {
            class: TransformType::LogCamera,
            valid: LogCameraTransform::new(&[0.1, 0.1, 0.1]).into(),
            invalid: Some(log_camera_invalid.into()),
        },
        Arm {
            class: TransformType::Log,
            valid: LogTransform::new().into(),
            invalid: Some(log_invalid.into()),
        },
        Arm {
            class: TransformType::Look,
            valid: lt.into(),
            invalid: Some(LookTransform::new().into()),
        },
        Arm {
            class: TransformType::Lut1D,
            valid: Lut1DTransform::new().into(),
            invalid: None,
        },
        Arm {
            class: TransformType::Matrix,
            valid: MatrixTransform::new().into(),
            invalid: None,
        },
        Arm {
            class: TransformType::Range,
            valid: range.into(),
            invalid: Some(range_invalid.into()),
        },
    ]
}

/// The direction, the validation and the text of the class a `Transform` holds, called on the
/// class itself rather than through the enum's dispatch.
fn class_calls(transform: &Transform) -> (TransformDirection, Option<String>, String) {
    macro_rules! calls {
        ($t:expr) => {
            (
                $t.direction(),
                $t.validate().err().map(|e| e.message().to_string()),
                $t.to_string(),
            )
        };
    }
    match transform {
        Transform::Allocation(t) => calls!(t),
        Transform::Builtin(t) => calls!(t),
        Transform::Cdl(t) => calls!(t),
        Transform::ColorSpace(t) => calls!(t),
        Transform::DisplayView(t) => calls!(t),
        Transform::Exponent(t) => calls!(t),
        Transform::ExponentWithLinear(t) => calls!(t),
        Transform::File(t) => calls!(t),
        Transform::FixedFunction(t) => calls!(t),
        Transform::Group(t) => calls!(t),
        Transform::LogAffine(t) => calls!(t),
        Transform::LogCamera(t) => calls!(t),
        Transform::Log(t) => calls!(t),
        Transform::Look(t) => calls!(t),
        Transform::Lut1D(t) => calls!(t),
        Transform::Matrix(t) => calls!(t),
        Transform::Range(t) => calls!(t),
    }
}

/// Every arm: the class; the direction, set and read through the enum and seen by the class;
/// the validation, which passes for the valid transform and fails with the class's own message
/// for the invalid one; and the text, the class's own in both directions.
#[test]
fn every_arm_dispatches_to_its_class() {
    let arms = arms();
    assert_eq!(arms.len(), 17, "one arm per variant");
    for arm in arms {
        let class = arm.class;
        let mut transform = arm.valid;
        assert_eq!(transform.transform_type(), class);
        assert_eq!(
            transform.direction(),
            TransformDirection::Forward,
            "{class:?}"
        );

        for dir in [
            TransformDirection::Inverse,
            TransformDirection::Forward,
            TransformDirection::Inverse,
        ] {
            transform.set_direction(dir);
            assert_eq!(transform.direction(), dir, "{class:?}");
            let (class_dir, class_error, class_text) = class_calls(&transform);
            assert_eq!(class_dir, dir, "{class:?}");
            assert_eq!(class_error, None, "{class:?} {dir:?}");
            assert!(transform.validate().is_ok(), "{class:?} {dir:?}");
            assert_eq!(transform.to_string(), class_text, "{class:?} {dir:?}");
            assert!(!class_text.is_empty(), "{class:?} {dir:?}");
        }

        if let Some(invalid) = arm.invalid {
            assert_eq!(invalid.transform_type(), class);
            let (_, class_error, class_text) = class_calls(&invalid);
            let class_error =
                class_error.unwrap_or_else(|| panic!("{class:?}: the invalid one validates"));
            let error = invalid
                .validate()
                .expect_err("the class fails, so the enum must");
            assert_eq!(error.message(), class_error, "{class:?}");
            assert_eq!(invalid.to_string(), class_text, "{class:?}");
        }
    }
}

/// The error a processor of `transform` returns, in the direction `dir`.
fn processor_error(config: &Config, transform: &Transform, dir: TransformDirection) -> String {
    match config.processor_in_direction(transform, dir) {
        Ok(_) => panic!("{transform} {dir:?}: a processor was built"),
        Err(error) => error.message().to_string(),
    }
}

/// A processor of each class whose op builder is not ported yet is refused in both directions,
/// with the error `build_ops` documents: the class and the work package that ports it.
#[test]
fn processors_of_the_classes_without_builders_are_refused() {
    let config = Config::create_raw().unwrap();
    let cases = [
        (TransformType::ColorSpace, "ColorSpaceTransform", "WP 3.2a"),
        (
            TransformType::DisplayView,
            "DisplayViewTransform",
            "WP 3.2c",
        ),
        (TransformType::File, "FileTransform", "WP 4.1"),
        (TransformType::Look, "LookTransform", "WP 3.2b"),
    ];
    let arms = arms();
    for (class, name, work_package) in cases {
        let arm = arms.iter().find(|arm| arm.class == class).unwrap();
        for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
            assert_eq!(
                processor_error(&config, &arm.valid, dir),
                format!("{name}: building its ops is not ported yet ({work_package})."),
                "{dir:?}"
            );
        }
    }
}

/// The built-in transforms whose ops are ported (WP 3.2e-g); their pixels are compared with the
/// wheel's in `tests/api_battery_oracle.rs`.
const BUILTINS_WITH_OPS: &[&[u8]] = &[
    b"IDENTITY",
    b"ARRI_ALEXA-LOGC-EI800-AWG_to_ACES2065-1",
    b"ARRI_LOGC4_to_ACES2065-1",
    b"PANASONIC_VLOG-VGAMUT_to_ACES2065-1",
    b"RED_REDLOGFILM-RWG_to_ACES2065-1",
    b"RED_LOG3G10-RWG_to_ACES2065-1",
    b"SONY_SLOG3-SGAMUT3_to_ACES2065-1",
    b"SONY_SLOG3-SGAMUT3.CINE_to_ACES2065-1",
    b"SONY_SLOG3-SGAMUT3-VENICE_to_ACES2065-1",
    b"SONY_SLOG3-SGAMUT3.CINE-VENICE_to_ACES2065-1",
    b"UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD",
    b"UTILITY - ACES-AP1_to_CIE-XYZ-D65_BFD",
    b"UTILITY - ACES-AP1_to_LINEAR-REC709_BFD",
    b"CURVE - ACEScct-LOG_to_LINEAR",
    b"ACEScct_to_ACES2065-1",
    b"ACEScg_to_ACES2065-1",
    b"ACESproxy10i_to_ACES2065-1",
    b"ACES-LMT - BLUE_LIGHT_ARTIFACT_FIX",
    b"DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.709",
    b"DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.709 - MIRROR NEGS",
    b"DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.2020",
    b"DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.2020 - MIRROR NEGS",
    b"DISPLAY - CIE-XYZ-D65_to_G2.2-REC.709",
    b"DISPLAY - CIE-XYZ-D65_to_G2.2-REC.709 - MIRROR NEGS",
    b"DISPLAY - CIE-XYZ-D65_to_sRGB",
    b"DISPLAY - CIE-XYZ-D65_to_sRGB - MIRROR NEGS",
    b"DISPLAY - CIE-XYZ-D65_to_G2.6-P3-DCI-BFD",
    b"DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D65",
    b"DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D65 - MIRROR NEGS",
    b"DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D60-BFD",
    b"DISPLAY - CIE-XYZ-D65_to_DCDM-D65",
    b"DISPLAY - CIE-XYZ-D65_to_DisplayP3",
    b"DISPLAY - CIE-XYZ-D65_to_DisplayP3-HDR",
];

/// A processor of every other built-in transform of the registry is refused in both
/// directions, and with either direction of the transform itself, with the error the
/// registry's creators document: the style, as the registry spells it.
#[test]
fn processors_of_the_builtin_transforms_without_ops_are_refused() {
    let config = Config::create_raw().unwrap();
    let registry = BuiltinTransformRegistry::get();
    assert!(registry.num_builtins() > 0);
    for index in 0..registry.num_builtins() {
        let style = registry.builtin_style(index).unwrap();
        if BUILTINS_WITH_OPS.contains(&style) {
            continue;
        }
        let expected = format!(
            "BuiltinTransform: the ops of '{}' are not ported yet.",
            String::from_utf8_lossy(style)
        );
        for own_dir in [TransformDirection::Forward, TransformDirection::Inverse] {
            let mut builtin = BuiltinTransform::new();
            builtin.set_style(style).unwrap();
            builtin.set_direction(own_dir);
            let transform = Transform::from(builtin);
            for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
                assert_eq!(
                    processor_error(&config, &transform, dir),
                    expected,
                    "{own_dir:?} {dir:?}"
                );
            }
        }
    }
}
