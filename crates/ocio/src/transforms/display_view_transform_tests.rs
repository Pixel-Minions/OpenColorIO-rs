// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the display view transform: `tests/cpu/transforms/DisplayViewTransform_tests.cpp`
//! @ v2.5.2. `build_ops` uses an ExposureContrastTransform (Phase 5). The text, the validation
//! and the processors are compared with the wheel's in `tests/config_transforms_oracle.rs`.

use super::*;
use ocio_ops::op_data::{OpData, OpDataType};
use ocio_ops::open_color_types::ColorSpaceDirection;
use ocio_testkit::upstream::check_throw_what;

use crate::test_env::EnvGuard;
use crate::transform::Transform;
use crate::transforms::group_transform::GroupTransform;

/// Port of `OCIO_ADD_TEST(DisplayViewTransform, basic)` @ v2.5.2.
#[test]
fn basic() {
    let empty: &[u8] = b"";
    let mut dt = DisplayViewTransform::new();

    assert_eq!(dt.direction(), TransformDirection::Forward);
    assert_eq!(empty, dt.src());
    assert_eq!(empty, dt.display());
    assert_eq!(empty, dt.view());
    assert!(!dt.looks_bypass());
    assert!(dt.data_bypass());
    dt.set_data_bypass(false);
    assert!(!dt.data_bypass());
    dt.set_data_bypass(true);
    assert!(dt.data_bypass());

    let input_cs: &[u8] = b"inputCS";
    dt.set_src(input_cs);
    assert_eq!(input_cs, dt.src());

    let display: &[u8] = b"display";
    dt.set_display(display);
    assert_eq!(display, dt.display());

    let view: &[u8] = b"view";
    dt.set_view(view);
    assert_eq!(view, dt.view());

    assert!(dt.validate().is_ok());

    dt.set_direction(TransformDirection::Inverse);
    assert_eq!(dt.direction(), TransformDirection::Inverse);

    dt.set_src("");
    check_throw_what(
        dt.validate(),
        "DisplayViewTransform: empty source color space name",
    );
    dt.set_src(input_cs);

    dt.set_display("");
    check_throw_what(dt.validate(), "DisplayViewTransform: empty display name");
    dt.set_display(display);

    dt.set_view("");
    check_throw_what(dt.validate(), "DisplayViewTransform: empty view name");
    dt.set_view(view);

    assert!(dt.validate().is_ok());

    dt.set_looks_bypass(true);
    assert!(dt.looks_bypass());

    dt.set_data_bypass(false);

    // Verify that copy has same values.
    let t: crate::Transform = dt.clone().into();
    let crate::Transform::DisplayView(dt) = t else {
        panic!("a display view transform");
    };
    assert_eq!(input_cs, dt.src());
    assert_eq!(display, dt.display());
    assert_eq!(view, dt.view());
    assert_eq!(dt.direction(), TransformDirection::Inverse);
    assert!(dt.looks_bypass());
    assert!(!dt.data_bypass());
}

/// The setters keep a name up to its first NUL, as from a C string.
#[test]
fn names_stop_at_nul() {
    let mut dt = DisplayViewTransform::new();
    dt.set_src(b"s\0rc");
    dt.set_display(b"d\0");
    dt.set_view(b"\0v");
    assert_eq!(dt.src(), b"s");
    assert_eq!(dt.display(), b"d");
    assert_eq!(dt.view(), b"");
    check_throw_what(dt.validate(), "DisplayViewTransform: empty view name");
}

// The tests that build ops (WP 3.2c).

/// What an op of a display view transform's ops is expected to be: a no-op, or a CDL op of a
/// name in a direction.
enum Step {
    NoOp,
    Cdl(&'static str, TransformDirection),
}

/// Port of `ValidateTransform` (tests/cpu/transforms/DisplayViewTransform_tests.cpp:393-402 @
/// v2.5.2), and the no-op checks between: `ops` are the `steps`, in order.
#[track_caller]
fn check_steps(ops: &OpVec, steps: &[Step]) {
    assert_eq!(ops.len(), steps.len());
    for (i, step) in steps.iter().enumerate() {
        let data = ops[i].data();
        match step {
            Step::NoOp => assert_eq!(data.get_type(), OpDataType::NoOp, "op {i}"),
            Step::Cdl(name, dir) => {
                let metadata = data.get_format_metadata();
                assert_eq!(metadata.get_num_attributes(), 1, "op {i}");
                assert_eq!(name.as_bytes(), metadata.get_attribute_value(0), "op {i}");
                match &**data {
                    OpData::Cdl(cdl) => assert_eq!(*dir, cdl.get_direction(), "op {i}"),
                    other => panic!("op {i}: not a CDL op: {other:?}"),
                }
            }
        }
    }
}

/// `BuildDisplayOps` with the config's current context.
fn build(config: &Config, dt: &DisplayViewTransform, dir: TransformDirection) -> Result<OpVec> {
    let mut ops = OpVec::new();
    build_display_ops(&mut ops, config, &config.current_context().get(), dt, dir)?;
    Ok(ops)
}

/// Port of `OCIO_ADD_TEST(DisplayViewTransform, build_ops_with_looks)` @ v2.5.2.
#[test]
fn build_ops_with_looks() {
    use Step::{Cdl, NoOp};
    use TransformDirection::{Forward, Inverse};

    let _env = EnvGuard::new();
    //
    // Validate BuildDisplayOps using a display-referred ViewTransform and a look with a
    // display-referred ProcessSpace.
    //

    const CONFIG: &str = r#"
ocio_profile_version: 2

roles:
  default: raw

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
  display:
    - !<View> {name: view, view_transform: display_vt, display_colorspace: displayCSOut, looks: look}
    - !<View> {name: viewNoVT, colorspace: displayCSOut, looks: look}
    - !<View> {name: viewVTNT, view_transform: nt_forward, display_colorspace: displayCSOut}
    - !<View> {name: viewCSNT, colorspace: nt_inverse, looks: look}
    - !<View> {name: viewCSNTNoLook, colorspace: nt_inverse}

looks:
  - !<Look>
    name: look
    process_space: displayCSProcess
    transform: !<CDLTransform> {name: look forward, sat: 1.5}
    inverse_transform: !<CDLTransform> {name: look inverse, sat: 1.5}

view_transforms:
  - !<ViewTransform>
    name: default_vt
    to_scene_reference: !<CDLTransform> {sat: 1.5}

  - !<ViewTransform>
    name: display_vt
    to_display_reference: !<CDLTransform> {name: display vt to ref, sat: 1.5}
    from_display_reference: !<CDLTransform> {name: display vt from ref, sat: 1.5}

display_colorspaces:
  - !<ColorSpace>
    name: displayCSIn
    to_display_reference: !<CDLTransform> {name: in cs to ref, sat: 1.5}
    from_display_reference: !<CDLTransform> {name: in cs from ref, sat: 1.5}

  - !<ColorSpace>
    name: displayCSOut
    to_display_reference: !<CDLTransform> {name: out cs to ref, sat: 1.5}
    from_display_reference: !<CDLTransform> {name: out cs from ref, sat: 1.5}

  - !<ColorSpace>
    name: displayCSProcess
    to_display_reference: !<CDLTransform> {name: process cs to ref, sat: 1.5}
    from_display_reference: !<CDLTransform> {name: process cs from ref, sat: 1.5}

colorspaces:
  - !<ColorSpace>
    name: raw
    family: raw
    description: A raw color space.
    isdata: true

named_transforms:
  - !<NamedTransform>
    name: nt_forward
    transform: !<CDLTransform> {name: forward transform for nt_forward, sat: 1.5}

  - !<NamedTransform>
    name: nt_inverse
    inverse_transform: !<CDLTransform> {name: inverse transform for nt_inverse, sat: 1.5}
"#;

    let config = Config::create_from_stream(CONFIG.as_bytes()).unwrap();
    config.validate().unwrap();

    let mut dt = DisplayViewTransform::new();
    dt.set_src("displayCSIn");
    dt.set_display("display");
    dt.set_view("view");

    //
    // Test in forward direction.
    //

    let mut ops = build(&config, &dt, Forward).unwrap();
    assert_eq!(ops.len(), 11);
    ops.validate().unwrap();

    // DisplayCSIn->displayCSProcess:
    //       0. GPU Allocation No-op.
    //       1. In to reference.
    //       2. Look process space from reference.
    //       3. GPU Allocation No-op.
    //     4-5. Look-noop + look transform.
    // DisplayCSProcess->display reference:
    //       6. GPU Allocation No-op.
    //       7. DisplayCSProcess to display reference.
    //       8. Display-referred VT.
    // Reference->DisplayCSOutput:
    //       9. DisplayCSOutput from display reference.
    //      10. GPU Allocation No-op.
    check_steps(
        &ops,
        &[
            NoOp,
            Cdl("in cs to ref", Forward),
            Cdl("process cs from ref", Forward),
            NoOp,
            NoOp,
            Cdl("look forward", Forward),
            NoOp,
            Cdl("process cs to ref", Forward),
            Cdl("display vt from ref", Forward),
            Cdl("out cs from ref", Forward),
            NoOp,
        ],
    );

    //
    // Test in inverse direction.
    //

    let mut ops = build(&config, &dt, Inverse).unwrap();
    assert_eq!(ops.len(), 11);
    ops.validate().unwrap();
    check_steps(
        &ops,
        &[
            // 0. GPU Allocation No-op.
            NoOp,
            // 1. DisplayCSOutput to display reference.
            Cdl("out cs to ref", Forward),
            // 2. Display-referred VT.
            Cdl("display vt to ref", Forward),
            // 3. DisplayCSProcess from display reference.
            Cdl("process cs from ref", Forward),
            // 4. GPU Allocation No-op.
            NoOp,
            // 5. Look No-op.
            NoOp,
            // 6. Look transform (inverse).
            Cdl("look inverse", Forward),
            // 7. GPU Allocation No-op.
            NoOp,
            // 8. Look process space to reference.
            Cdl("process cs to ref", Forward),
            // 9. In from reference.
            Cdl("in cs from ref", Forward),
            // 10. GPU Allocation No-op.
            NoOp,
        ],
    );

    //
    // Check that looks can be bypassed.
    //

    dt.set_looks_bypass(true);
    let mut ops = build(&config, &dt, Forward).unwrap();
    assert_eq!(ops.len(), 5);
    ops.validate().unwrap();
    check_steps(
        &ops,
        &[
            // 0. GPU Allocation No-op.
            NoOp,
            // 1. In to reference.
            Cdl("in cs to ref", Forward),
            // 2. Display-referred VT.
            Cdl("display vt from ref", Forward),
            // 3. DisplayCSOutput from display reference.
            Cdl("out cs from ref", Forward),
            // 4. GPU Allocation No-op.
            NoOp,
        ],
    );

    //
    // Tests without a view transform.
    //

    dt.set_looks_bypass(false);
    dt.set_view("viewNoVT");

    //
    // Test in forward direction.
    //

    let mut ops = build(&config, &dt, Forward).unwrap();
    assert_eq!(ops.len(), 10);
    ops.validate().unwrap();
    check_steps(
        &ops,
        &[
            // 0. GPU Allocation No-op.
            NoOp,
            // 1. In to reference.
            Cdl("in cs to ref", Forward),
            // 2. Look process space from reference.
            Cdl("process cs from ref", Forward),
            // 3. GPU Allocation No-op.
            NoOp,
            // 4. Look No-op.
            NoOp,
            // 5. Look transform.
            Cdl("look forward", Forward),
            // 6. GPU Allocation No-op.
            NoOp,
            // 7. DisplayCSProcess to display reference.
            Cdl("process cs to ref", Forward),
            // 8. DisplayCSOutput from display reference.
            Cdl("out cs from ref", Forward),
            // 9. GPU Allocation No-op.
            NoOp,
        ],
    );

    //
    // Test in inverse direction.
    //

    let mut ops = build(&config, &dt, Inverse).unwrap();
    assert_eq!(ops.len(), 10);
    ops.validate().unwrap();
    check_steps(
        &ops,
        &[
            // 0. GPU Allocation No-op.
            NoOp,
            // 1. DisplayCSOutput to display reference.
            Cdl("out cs to ref", Forward),
            // 2. DisplayCSProcess from display reference.
            Cdl("process cs from ref", Forward),
            // 3. GPU Allocation No-op.
            NoOp,
            // 4. Look No-op.
            NoOp,
            // 5. Look transform (inverse).
            Cdl("look inverse", Forward),
            // 6. GPU Allocation No-op.
            NoOp,
            // 7. Look process space to reference.
            Cdl("process cs to ref", Forward),
            // 8. In from reference.
            Cdl("in cs from ref", Forward),
            // 9. GPU Allocation No-op.
            NoOp,
        ],
    );

    //
    // Using named transforms.
    //

    //
    // Src can't be a named transform.
    //

    dt.set_src("nt_forward");
    dt.set_view("view");
    check_throw_what(
        build(&config, &dt, Forward),
        "Cannot find source color space named 'nt_forward'",
    );

    //
    // View color space is a named transform: looks are applied on src then the named transform
    // is applied.
    //

    dt.set_src("displayCSIn");
    dt.set_view("viewCSNT");
    let mut ops = build(&config, &dt, Forward).unwrap();
    assert_eq!(ops.len(), 7);
    ops.validate().unwrap();
    check_steps(
        &ops,
        &[
            // 0. GPU Allocation No-op.
            NoOp,
            // 1. In to reference.
            Cdl("in cs to ref", Forward),
            // 2. Look process space from reference.
            Cdl("process cs from ref", Forward),
            // 3. GPU Allocation No-op.
            NoOp,
            // 4. Look No-op.
            NoOp,
            // 5. Look transform.
            Cdl("look forward", Forward),
            // 6. Named transform.
            Cdl("inverse transform for nt_inverse", Inverse),
        ],
    );

    //
    // Same in inverse direction.
    //

    let mut ops = build(&config, &dt, Inverse).unwrap();
    assert_eq!(ops.len(), 7);
    ops.validate().unwrap();
    check_steps(
        &ops,
        &[
            // 0. Named transform.
            Cdl("inverse transform for nt_inverse", Forward),
            // 1. Look No-op.
            NoOp,
            // 2. Look transform (inverse).
            Cdl("look inverse", Forward),
            // 3. GPU Allocation No-op.
            NoOp,
            // 4. Look process space to reference.
            Cdl("process cs to ref", Forward),
            // 5. In from reference.
            Cdl("in cs from ref", Forward),
            // 6. GPU Allocation No-op.
            NoOp,
        ],
    );

    //
    // View color space is a named transform and no look.
    //

    dt.set_view("viewCSNTNoLook");
    let mut ops = build(&config, &dt, Forward).unwrap();
    assert_eq!(ops.len(), 1);
    ops.validate().unwrap();
    check_steps(&ops, &[Cdl("inverse transform for nt_inverse", Inverse)]);

    //
    // Same in inverse direction.
    //

    let mut ops = build(&config, &dt, Inverse).unwrap();
    assert_eq!(ops.len(), 1);
    ops.validate().unwrap();
    check_steps(&ops, &[Cdl("inverse transform for nt_inverse", Forward)]);

    //
    // View transforn is a named transform: named transform and dst conversion are applied.
    //

    dt.set_src("displayCSIn");
    dt.set_view("viewVTNT");
    let mut ops = build(&config, &dt, Forward).unwrap();
    assert_eq!(ops.len(), 3);
    ops.validate().unwrap();
    check_steps(
        &ops,
        &[
            // Named transform.
            Cdl("forward transform for nt_forward", Forward),
            // DisplayCSOutput from display reference.
            Cdl("out cs from ref", Forward),
            // GPU Allocation No-op.
            NoOp,
        ],
    );

    //
    // Same in inverse direction: dst conversion and named transform are applied.
    //

    let mut ops = build(&config, &dt, Inverse).unwrap();
    assert_eq!(ops.len(), 3);
    ops.validate().unwrap();
    check_steps(
        &ops,
        &[
            // GPU Allocation No-op.
            NoOp,
            // DisplayCSOutput to display reference.
            Cdl("out cs to ref", Forward),
            // Named transform.
            Cdl("forward transform for nt_forward", Inverse),
        ],
    );
}

/// Port of `OCIO_ADD_TEST(DisplayViewTransform, config_load)` @ v2.5.2.
#[test]
fn config_load() {
    let _env = EnvGuard::new();
    const SIMPLE_CONFIG: &str = r#"
ocio_profile_version: 2

roles:
  default: raw

displays:
  displayName:
    - !<View> {name: viewName, colorspace: out}

colorspaces:
  - !<ColorSpace>
    name: raw

  - !<ColorSpace>
    name: in
    to_scene_reference: !<MatrixTransform> {offset: [0.11, 0.12, 0.13, 0]}

  - !<ColorSpace>
    name: out
    from_scene_reference: !<MatrixTransform> {offset: [0.11, 0.12, 0.13, 0]}

  - !<ColorSpace>
    name: test
    from_scene_reference: !<DisplayViewTransform> {src: in, display: displayName, view: viewName}
    to_scene_reference: !<DisplayViewTransform> {src: in, display: displayName, view: viewName, looks_bypass: true, data_bypass: false}
"#;

    let config = Config::create_from_stream(SIMPLE_CONFIG.as_bytes()).unwrap();

    let cs = config.color_space("test").unwrap();
    let tr = cs.transform(ColorSpaceDirection::FromReference).unwrap();
    let Transform::DisplayView(display_tr) = tr else {
        panic!("not a DisplayViewTransform: {tr:?}");
    };
    assert_eq!(TransformDirection::Forward, display_tr.direction());
    assert_eq!(b"in", display_tr.src());
    assert_eq!(b"displayName", display_tr.display());
    assert_eq!(b"viewName", display_tr.view());
    assert!(!display_tr.looks_bypass());
    assert!(display_tr.data_bypass());

    let tr = cs.transform(ColorSpaceDirection::ToReference).unwrap();
    let Transform::DisplayView(display_tr) = tr else {
        panic!("not a DisplayViewTransform: {tr:?}");
    };
    assert!(display_tr.looks_bypass());
    assert!(!display_tr.data_bypass());
}

/// Port of `OCIO_ADD_TEST(DisplayViewTransform, apply_fwd_inv)` @ v2.5.2.
#[test]
fn apply_fwd_inv() {
    use ocio_ops::open_color_types::{BitDepth, OptimizationFlags};
    use ocio_testkit::upstream::check_close;

    let _env = EnvGuard::new();
    const CONFIG: &str = r#"
ocio_profile_version: 2

roles:
  default: raw

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
  display:
    - !<View> {name: view, view_transform: display_vt, display_colorspace: displayCSOut, looks: look}
    - !<View> {name: viewNoVT, colorspace: displayCSOut, looks: look}

looks:
  - !<Look>
    name: look
    process_space: displayCSProcess
    transform: !<MatrixTransform> {offset: [0.1, 0.2, 0.3, 0]}

view_transforms:
  - !<ViewTransform>
    name: default_vt
    to_scene_reference: !<MatrixTransform> {offset: [0.2, 0.2, 0.4, 0]}

  - !<ViewTransform>
    name: display_vt
    to_display_reference: !<MatrixTransform> {offset: [0.3, 0.1, 0.1, 0]}

display_colorspaces:
  - !<ColorSpace>
    name: displayCSOut
    to_display_reference: !<MatrixTransform> {offset: [0.25, 0.15, 0.35, 0]}

  - !<ColorSpace>
    name: displayCSProcess
    to_display_reference: !<MatrixTransform> {offset: [0.1, 0.1, 0.1, 0]}

colorspaces:
  - !<ColorSpace>
    name: raw
    family: raw
    description: A raw color space.
    isdata: true

  - !<ColorSpace>
    name: displayCSIn
    to_scene_reference: !<MatrixTransform> {offset: [-0.15, 0.15, 0.15, 0.05]}
"#;

    let config = Config::create_from_stream(CONFIG.as_bytes()).unwrap();
    config.validate().unwrap();

    // Create a display transform using a view that use a view transform and a scene-referred
    // input color space. Create forward and inverse processor and apply them one after the
    // other to a set of pixels. Finally check that the processor created from a group that
    // holds the forward transform and the inverse transform is a no-op.

    let mut dt = DisplayViewTransform::new();
    dt.set_src("displayCSIn");
    dt.set_display("display");
    dt.set_view("view");

    // This results in the following conversion:
    //   displayCSIn to scene_ref                                 offset= -0.15 0.15 0.15 0.05
    //   scene_ref to display_ref                                 offset= -0.2 -0.2 -0.4 -0
    //   display_ref to displayCSProcess (look process_space)     offset= -0.1 -0.1 -0.1 -0
    //   apply look                                               offset=  0.1 0.2 0.3 0
    //   displayCSProcess to display_ref                          offset=  0.1 0.1 0.1 0
    //   apply display_vt                                         offset= -0.3 -0.1 -0.1 -0
    //   display_ref to displayCSOut                              offset= -0.25 -0.15 -0.35 -0
    // Total transformation:                                      offset= -0.8 -0.1 -0.4 0.05

    let ref_: [f32; 16] = [
        0.0, 0.1, 0.2, 0.0, //
        0.3, 0.4, 0.5, 0.5, //
        0.6, 0.7, 0.8, 0.7, //
        0.9, 1.0, 1.1, 1.0,
    ];
    let error = 1e-6f32;

    // The processor of `dt` forward and inverse, without no-ops, of `num` transforms each; the
    // pixels through both come back.
    let check_round_trip = |dt: &DisplayViewTransform, num: i32| {
        let tr = Transform::from(dt.clone());
        let proc = config.processor(&tr).unwrap();
        // Remove the no-ops, since they are useless here.
        let proc = proc.optimized_processor(OptimizationFlags::NONE).unwrap();
        assert_eq!(proc.num_transforms(), num);
        let proc_group = proc.create_group_transform().unwrap();
        assert_eq!(proc_group.num_transforms(), num);
        let cpu_proc = proc.default_cpu_processor().unwrap();
        assert_eq!(proc.num_transforms(), num);

        let proc_inv = config
            .processor_in_direction(&tr, TransformDirection::Inverse)
            .unwrap();
        // Remove the no-ops, since they are useless here.
        let proc_inv = proc_inv
            .optimized_processor(OptimizationFlags::NONE)
            .unwrap();
        assert_eq!(proc_inv.num_transforms(), num);
        let proc_group = proc_inv.create_group_transform().unwrap();
        assert_eq!(proc_group.num_transforms(), num);
        let cpu_proc_inv = proc_inv.default_cpu_processor().unwrap();

        for i in 0..4 {
            let mut rgba = [
                ref_[4 * i],
                ref_[4 * i + 1],
                ref_[4 * i + 2],
                ref_[4 * i + 3],
            ];
            cpu_proc.apply_rgba(&mut rgba).unwrap();
            cpu_proc_inv.apply_rgba(&mut rgba).unwrap();
            for c in 0..4 {
                check_close(rgba[c], ref_[4 * i + c], error);
            }
        }

        let mut group = GroupTransform::new();
        group.append_transform(tr.clone());
        let mut dt_inv = dt.clone();
        dt_inv.set_direction(TransformDirection::Inverse);
        group.append_transform(Transform::from(dt_inv));

        // Note that optimization does only happen once each transform has been converted to ops.
        let group_proc = config.processor(&Transform::from(group)).unwrap();
        let group_proc = group_proc
            .optimized_processor_with_bit_depths(
                BitDepth::F32,
                BitDepth::F32,
                OptimizationFlags::DEFAULT,
            )
            .unwrap();
        assert!(group_proc.is_no_op().unwrap());
    };

    check_round_trip(&dt, 7);

    // Do a similar test using a display transform that does not use a view transform.

    dt.set_display("display");
    dt.set_view("viewNoVT");

    check_round_trip(&dt, 6);

    //
    // Check that the correct error message is thrown in various scenarios.
    //

    dt.set_src("displayCSIn");
    dt.set_display("display");
    dt.set_view("view");

    let processor_of =
        |config: &Config, dt: &DisplayViewTransform| config.processor(&Transform::from(dt.clone()));

    // Empty arguments get handled in DisplayViewTransform::validate.

    // Display name is empty.
    dt.set_display("");
    check_throw_what(
        processor_of(&config, &dt),
        "DisplayViewTransform: empty display name.",
    );
    dt.set_display("display");

    // View name is empty.
    dt.set_view("");
    check_throw_what(
        processor_of(&config, &dt),
        "DisplayViewTransform: empty view name.",
    );
    dt.set_view("view");

    // Source CS is empty.
    dt.set_src("");
    check_throw_what(
        processor_of(&config, &dt),
        "DisplayViewTransform: empty source color space name.",
    );

    // More detailed error handling is in DisplayViewTransform::BuildDisplayOps.

    // Source CS doesn't exist in the config.
    dt.set_src("missing cs");
    check_throw_what(
        processor_of(&config, &dt),
        "DisplayViewTransform error. Cannot find source color space named 'missing cs'.",
    );
    dt.set_src("displayCSIn");

    // Display doesn't exist in the config.
    dt.set_display("missing display");
    check_throw_what(
        processor_of(&config, &dt),
        "DisplayViewTransform error. Display 'missing display' not found.",
    );
    dt.set_display("display");

    let mut e_config = (*config).clone();

    // View references a view transform that doesn't exist in the config.
    e_config
        .add_display_view_with_view_transform(
            "display",
            "bad_view",
            "missing vt",
            "displayCSOut",
            "",
            "",
            "",
        )
        .unwrap();
    dt.set_view("bad_view");
    check_throw_what(
        processor_of(&e_config, &dt),
        "DisplayViewTransform error. The view transform 'missing vt' is neither a view transform \
         nor a named transform.",
    );

    // View doesn't exist in the config.
    dt.set_view("missing view");
    check_throw_what(
        processor_of(&config, &dt),
        "DisplayViewTransform error. The display 'display' does not have view 'missing view'.",
    );

    // View references a "display_colorspace" that doesn't exist in the config.
    e_config
        .add_display_view_with_view_transform(
            "display",
            "bad_view",
            "display_vt",
            "missing cs",
            "",
            "",
            "",
        )
        .unwrap();
    dt.set_view("bad_view");
    check_throw_what(
        processor_of(&e_config, &dt),
        "DisplayViewTransform error. The view 'bad_view' refers to a display color space \
         'missing cs' that can't be found.",
    );
    // As with most of these, validation also fails.
    check_throw_what(
        e_config.validate(),
        "Config failed display view validation. Display 'display' has a view 'bad_view' that \
         refers to a color space or a named transform, 'missing cs', which is not defined.",
    );

    // View references a "colorspace" that doesn't exist in the config.
    e_config
        .add_display_view("display", "bad_view", "missing cs", "")
        .unwrap();
    dt.set_view("bad_view");
    check_throw_what(
        processor_of(&e_config, &dt),
        "DisplayViewTransform error. Cannot find color space or named transform with name \
         'missing cs'.",
    );

    // Check a few more scenarios.

    // Missing look.
    e_config
        .add_display_view_with_view_transform(
            "display",
            "bad_view",
            "display_vt",
            "displayCSOut",
            "missing look",
            "",
            "",
        )
        .unwrap();
    dt.set_view("bad_view");
    check_throw_what(
        processor_of(&e_config, &dt),
        "RunLookTokens error. The specified look, 'missing look', cannot be found.  (looks: \
         look).",
    );
    dt.set_view("view");

    // Missing viewing rule does not currently throw when getting a processor.
    e_config
        .add_display_view_with_view_transform(
            "display",
            "bad_view",
            "display_vt",
            "displayCSOut",
            "",
            "missing rule",
            "desc: foo",
        )
        .unwrap();
    processor_of(&e_config, &dt).unwrap();
    // But validation fails.
    check_throw_what(
        e_config.validate(),
        "Config failed display view validation. Display 'display' has a view 'bad_view' refers \
         to a viewing rule, 'missing rule', which is not defined.",
    );
}

/// Port of `OCIO_ADD_TEST(DisplayViewTransform, context_variables)` @ v2.5.2.
#[test]
fn context_variables() {
    let _env = EnvGuard::new();
    const OCIO_CONFIG: &str = r#"
ocio_profile_version: 2

environment: { FILE: cdl_test1.cc }

roles:
  default: cs1

file_rules:
  - !<Rule> {name: Default, colorspace: default}

displays:
  Disp1:
    - !<View> {name: View1, colorspace: cs1}
    - !<View> {name: View2, colorspace: cs4}
    - !<View> {name: View3, view_transform: vt1, display_colorspace: dcs1}
    - !<View> {name: View4, view_transform: vt1, display_colorspace: dcs2}
    - !<View> {name: View5, view_transform: vt2, display_colorspace: dcs1}
    - !<View> {name: View6, view_transform: vt2, display_colorspace: dcs2}
    - !<View> {name: View10, colorspace: cs1, looks: look1}
    - !<View> {name: View11, colorspace: cs1, looks: look2}
    - !<View> {name: View12, colorspace: cs1, looks: look3}
    - !<View> {name: View13, view_transform: vt1, display_colorspace: dcs2, looks: +look1}
    - !<View> {name: View14, view_transform: vt1, display_colorspace: dcs2, looks: +look2}
    - !<View> {name: View15, view_transform: vt1, display_colorspace: dcs2, looks: +look3}
    - !<View> {name: View16, view_transform: vt2, display_colorspace: dcs2, looks: +look1}
    - !<View> {name: View17, view_transform: vt2, display_colorspace: dcs2, looks: +look2}
    - !<View> {name: View18, view_transform: vt2, display_colorspace: dcs2, looks: +look3}

looks:
  - !<Look>
    name: look1
    process_space: default
    transform: !<FileTransform> {src: $FILE}
  - !<Look>
    name: look2
    process_space: default
    transform: !<LookTransform> {src: default, dst: cs2, looks: +look1}
  - !<Look>
    name: look3
    process_space: default
    transform: !<CDLTransform> {offset: [0.1, 0.1, 0.1]}

view_transforms:
  - !<ViewTransform>
    name: vt1
    to_scene_reference: !<FileTransform> {src: $FILE}
  - !<ViewTransform>
    name: vt2
    to_scene_reference: !<MatrixTransform> {offset: [0.2, 0.2, 0.4, 0]}

display_colorspaces:
  - !<ColorSpace>
    name: dcs1
    to_display_reference: !<FileTransform> {src: $FILE}
  - !<ColorSpace>
    name: dcs2
    to_display_reference: !<MatrixTransform> {offset: [0.25, 0.15, 0.35, 0]}

colorspaces:
  - !<ColorSpace>
    name: cs1
    allocation: uniform
  - !<ColorSpace>
    name: cs2
    allocation: uniform
    from_scene_reference: !<MatrixTransform> {offset: [0.11, 0.12, 0.13, 0]}
  - !<ColorSpace>
    name: cs3
    allocation: uniform
    from_scene_reference: !<MatrixTransform> {offset: [0.1, 0.2, 0.3, 0]}
  - !<ColorSpace>
    name: cs4
    allocation: uniform
    from_scene_reference: !<FileTransform> {src: $FILE}
"#;

    let mut used_context_vars = Context::new();

    let mut cfg = (*Config::create_from_stream(OCIO_CONFIG.as_bytes()).unwrap()).clone();
    cfg.set_search_path(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../upstream/OpenColorIO/tests/data/files"
    ));
    cfg.validate().unwrap();

    let mut dt = DisplayViewTransform::new();
    dt.set_src("cs1");
    dt.set_display("Disp1");

    // As upstream, one context collects every view's variables.
    for (view, found) in [
        ("View1", false),
        ("View2", true),
        ("View3", true),
        ("View4", true),
        ("View5", true),
        ("View6", false),
        // Validations including looks.
        ("View10", true),
        ("View11", true),
        ("View12", false),
        ("View13", true),
        ("View14", true),
        ("View15", true),
        ("View16", true),
        ("View17", true),
        ("View18", false),
    ] {
        dt.set_view(view);
        assert_eq!(
            collect_display_view_transform_context_variables(
                &cfg,
                &cfg.current_context().get(),
                &dt,
                &mut used_context_vars
            )
            .unwrap(),
            found,
            "{view}"
        );
    }
}

/// Inverse, a view transform after looks of empty names only has no source color space, which
/// upstream dereferences (docs/improvements.md, U-57): an error instead.
#[test]
fn an_inverse_view_transform_after_empty_looks_is_refused() {
    let _env = EnvGuard::new();
    const CONFIG: &str = r#"ocio_profile_version: 2
roles: {default: raw}
displays:
  d:
    - !<View> {name: v, view_transform: vt, display_colorspace: dout, looks: ", "}
view_transforms:
  - !<ViewTransform> {name: vt, from_scene_reference: !<MatrixTransform> {}}
display_colorspaces:
  - !<ColorSpace> {name: dout}
colorspaces:
  - !<ColorSpace> {name: raw}
  - !<ColorSpace> {name: in}
"#;
    let config = Config::create_from_stream(CONFIG.as_bytes()).unwrap();
    let mut dt = DisplayViewTransform::new();
    dt.set_src("in");
    dt.set_display("d");
    dt.set_view("v");
    check_throw_what(
        build(&config, &dt, TransformDirection::Inverse),
        "DisplayViewTransform error. The looks leave no color space to apply the view transform \
         from.",
    );
    // Forward, the looks leave the source color space as it is.
    build(&config, &dt, TransformDirection::Forward).unwrap();
}
