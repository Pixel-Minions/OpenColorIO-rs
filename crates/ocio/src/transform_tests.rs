// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the `Transform` dispatch: every arm's class, direction, validation and text through
//! the enum.

use super::*;
use crate::GradingStyle;
use crate::{Allocation, FixedFunctionStyle, Interpolation, RangeStyle, TransformType};

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

    let mut lut3d_invalid = Lut3DTransform::new();
    lut3d_invalid.set_interpolation(Interpolation::Cubic);
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
            class: TransformType::ExposureContrast,
            valid: ExposureContrastTransform::new().into(),
            // Nothing in an ExposureContrastTransform can be invalid.
            invalid: None,
        },
        Arm {
            class: TransformType::GradingPrimary,
            valid: GradingPrimaryTransform::new(GradingStyle::Log).into(),
            // The values are validated as they are set.
            invalid: None,
        },
        Arm {
            class: TransformType::GradingRgbCurve,
            valid: GradingRgbCurveTransform::new(GradingStyle::Video).into(),
            // The curves are validated as they are set.
            invalid: None,
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
            class: TransformType::Lut3D,
            valid: Lut3DTransform::new().into(),
            invalid: Some(lut3d_invalid.into()),
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
        Transform::ExposureContrast(t) => calls!(t),
        Transform::File(t) => calls!(t),
        Transform::FixedFunction(t) => calls!(t),
        Transform::GradingPrimary(t) => calls!(t),
        Transform::GradingRgbCurve(t) => calls!(t),
        Transform::Group(t) => calls!(t),
        Transform::LogAffine(t) => calls!(t),
        Transform::LogCamera(t) => calls!(t),
        Transform::Log(t) => calls!(t),
        Transform::Look(t) => calls!(t),
        Transform::Lut1D(t) => calls!(t),
        Transform::Lut3D(t) => calls!(t),
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
    assert_eq!(arms.len(), 21, "one arm per variant");
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

/// `depth` groups, each holding the next, around `inner`.
fn in_groups(inner: Transform, depth: usize) -> Transform {
    let mut transform = inner;
    for _ in 0..depth {
        let mut group = crate::GroupTransform::new();
        group.append_transform(transform);
        transform = Transform::from(group);
    }
    transform
}

/// A chain of `n` CTF files in `dir`, `c0.ctf` referencing `c1.ctf` and so on, the last a
/// matrix; returns the path of `c0.ctf`.
fn reference_chain(dir: &std::path::Path, n: usize) -> String {
    std::fs::create_dir_all(dir).unwrap();
    for i in 0..n {
        let body = if i == n - 1 {
            "    <Matrix inBitDepth=\"32f\" outBitDepth=\"32f\">\n\
             \x20       <Array dim=\"3 3 3\">2 0 0 0 3 0 0 0 4</Array>\n    </Matrix>\n"
                .to_owned()
        } else {
            format!(
                "    <Reference inBitDepth=\"32f\" outBitDepth=\"32f\" path=\"c{}.ctf\"/>\n",
                i + 1
            )
        };
        let text = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <ProcessList id=\"a\" version=\"1.7\">\n{body}</ProcessList>\n"
        );
        let path = dir.join(format!("c{i}.ctf"));
        if std::fs::read(&path).ok().as_deref() != Some(text.as_bytes()) {
            std::fs::write(&path, text).unwrap();
        }
    }
    let path = dir.join("c0.ctf");
    let path = path.to_str().unwrap().replace('\\', "/");
    path.trim_start_matches("//?/").to_owned()
}

/// The limit of the nesting's weight is exact, and so is each weight in it: `groups` groups
/// around a file transform of a chain of `files` CTF files weigh `72 groups + 67 + 67 files`
/// (each group 72, the file transform 67 and each of its files 67; docs/improvements.md,
/// U-58). 766 groups around 193 files weigh 68,150, the limit, and build; 726 groups around
/// 236 files weigh 68,151, one more, and are refused. Through `BuildOps` itself (which the
/// processors reach only after `CollectContextVariables`, which counts the groups too), and
/// through the processor with its cache on and off; on a 1 MiB thread, the smallest the port
/// supports, at every optimization level.
#[test]
fn the_nesting_weight_limit_is_exact() {
    const TOO_DEEP: &[u8] = b"The transforms nest too deep, counting groups and CTF references: \
                              upstream's stack overflows.";
    let root = ocio_testkit::paths::target_dir().join("nesting-weight");
    let run = move || {
        for (groups, files, accepted) in [(766, 193, true), (726, 236, false)] {
            let src = reference_chain(&root.join(format!("n{files}")), files);
            let mut config = (*crate::Config::create_raw().unwrap()).clone();
            config.add_search_path(
                std::path::Path::new(&src)
                    .parent()
                    .unwrap()
                    .to_str()
                    .unwrap(),
            );
            let mut file = FileTransform::new();
            file.set_src(&src);
            let transform = in_groups(Transform::File(file), groups);
            let what = format!("{groups} groups, {files} files");

            // BuildOps alone.
            let context = config.current_context().get();
            let mut ops = OpVec::new();
            let built = build_ops(
                &mut ops,
                &config,
                &context,
                &transform,
                TransformDirection::Forward,
            );
            match &built {
                Ok(()) => assert!(accepted, "{what}: built"),
                Err(e) => {
                    assert!(!accepted && e.what().ends_with(TOO_DEEP), "{what}: {e}");
                }
            }
            drop(ops);

            // The processors.
            for cache in [
                crate::ProcessorCacheFlags::DEFAULT,
                crate::ProcessorCacheFlags::OFF,
            ] {
                config.set_processor_cache_flags(cache);
                match config.processor(&transform) {
                    Ok(processor) => {
                        assert!(accepted, "{what} {cache:?}: built");
                        assert_eq!(
                            processor.processor_metadata().num_files(),
                            files as i32,
                            "{what} {cache:?}"
                        );
                    }
                    Err(e) => assert!(
                        !accepted && e.what().ends_with(TOO_DEEP),
                        "{what} {cache:?}: {e}"
                    ),
                }
            }
        }
    };
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(run)
        .unwrap()
        .join()
        .unwrap();
}
