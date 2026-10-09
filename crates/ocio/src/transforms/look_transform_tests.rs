// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the look transform: `tests/cpu/transforms/LookTransform_tests.cpp` @ v2.5.2.
//! `build_look_options_ops` waits for the file transform's ops (WP 4.1). The text, the
//! validation and the processors are compared with the wheel's in
//! `tests/config_transforms_oracle.rs`.

use super::*;
use ocio_ops::op::Op;
use ocio_ops::op_data::{OpData, OpDataType};
use ocio_testkit::upstream::check_throw_what;

use crate::test_env::EnvGuard;

/// Port of `OCIO_ADD_TEST(LookTransform, basic)` @ v2.5.2.
#[test]
fn basic() {
    let mut look = LookTransform::new();
    assert_eq!(look.direction(), TransformDirection::Forward);

    look.set_direction(TransformDirection::Inverse);
    assert_eq!(look.direction(), TransformDirection::Inverse);

    assert_eq!(look.src(), b"");
    assert_eq!(look.dst(), b"");
    assert_eq!(look.looks(), b"");

    check_throw_what(look.validate(), "empty source");

    let src: &[u8] = b"src";
    let dst: &[u8] = b"dst";
    let looks: &[u8] = b"look1, look2, look3";

    look.set_src(src);
    assert_eq!(look.src(), src);

    check_throw_what(look.validate(), "empty destination");

    look.set_dst(dst);
    assert_eq!(look.dst(), dst);

    assert!(look.validate().is_ok());

    look.set_looks(looks);
    assert_eq!(look.looks(), looks);

    assert!(!look.skip_color_space_conversion());
    look.set_skip_color_space_conversion(true);
    assert!(look.skip_color_space_conversion());

    // Copy and check copy has same values.
    let tr: crate::Transform = look.clone().into();
    let crate::Transform::Look(mut look) = tr else {
        panic!("a look transform");
    };
    assert_eq!(look.src(), src);
    assert_eq!(look.dst(), dst);
    assert_eq!(look.looks(), looks);
    assert!(look.skip_color_space_conversion());

    // Using null is similar as using an empty string. (A null pointer is an empty slice here.)
    look.set_src(b"");
    assert_eq!(look.src(), b"");

    look.set_dst(b"");
    assert_eq!(look.dst(), b"");
}

/// The setters keep a name up to its first NUL, as from a C string.
#[test]
fn names_stop_at_nul() {
    let mut look = LookTransform::new();
    look.set_src(b"a\0b");
    look.set_dst(b"\0");
    look.set_looks(b"+l1,\0-l2");
    assert_eq!(look.src(), b"a");
    assert_eq!(look.dst(), b"");
    assert_eq!(look.looks(), b"+l1,");
}

// The tests that build ops (WP 3.2b).

/// Port of `ValidateTransform` (tests/cpu/transforms/LookTransform_tests.cpp:71-80 @ v2.5.2):
/// a fixed function op of the name `name`, in the direction `dir`.
#[track_caller]
fn validate_transform(op: &Op, name: &str, dir: TransformDirection) {
    let metadata = op.data().get_format_metadata();
    assert_eq!(metadata.get_num_attributes(), 1);
    assert_eq!(metadata.get_attribute_value(0), name.as_bytes());

    match &**op.data() {
        OpData::FixedFunction(ff) => assert_eq!(ff.direction(), dir),
        other => panic!("not a FixedFunction op: {other:?}"),
    }
}

/// `BuildLookOps` with the config's current context.
fn build(config: &Config, lt: &LookTransform, dir: TransformDirection) -> Result<OpVec> {
    let mut ops = OpVec::new();
    build_look_ops(&mut ops, config, &config.current_context().get(), lt, dir)?;
    Ok(ops)
}

/// Port of `OCIO_ADD_TEST(LookTransform, build_look_ops)` @ v2.5.2.
#[test]
fn build_look_ops_test() {
    let _env = EnvGuard::new();
    const OCIO_CONFIG: &str = r#"
ocio_profile_version: 2

roles:
  default: raw

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}

looks:
  - !<Look>
    name: look1
    process_space: look1_cs
    transform: !<FixedFunctionTransform> {name: look1 trans, style: ACES_RedMod03}

  - !<Look>
    name: look2
    process_space: look2_3_cs
    transform: !<FixedFunctionTransform> {name: look2 trans, style: ACES_RedMod03}
    inverse_transform: !<FixedFunctionTransform> {name: look2 inverse trans, style: ACES_RedMod03}

  - !<Look>
    name: look3
    process_space: look2_3_cs
    inverse_transform: !<FixedFunctionTransform> {name: look3 inverse trans, style: ACES_RedMod03}

colorspaces:
  - !<ColorSpace>
    name: raw
    family: raw
    bitdepth: 32f
    description: |
      A raw color space. Conversions to and from this space are no-ops.
    isdata: true

  - !<ColorSpace>
    name: source
    to_scene_reference: !<FixedFunctionTransform> {name: src, style: ACES_RedMod03}

  - !<ColorSpace>
    name: destination
    from_scene_reference: !<FixedFunctionTransform> {name: dst, style: ACES_RedMod03}

  - !<ColorSpace>
    name: look1_cs
    to_scene_reference: !<FixedFunctionTransform> {name: look1_cs trans, style: ACES_RedMod03}

  - !<ColorSpace>
    name: look2_3_cs
    to_scene_reference: !<FixedFunctionTransform> {name: look2_3_cs trans, style: ACES_RedMod03}
"#;

    let config = Config::create_from_stream(OCIO_CONFIG.as_bytes()).unwrap();
    config.validate().unwrap();

    // Create look transform with source and destination color spaces, and 3 looks. Each look
    // has its own process space.
    let mut lt = LookTransform::new();
    lt.set_src("source");
    lt.set_dst("destination");
    lt.set_looks("look1, +look2, -look3");

    use TransformDirection::{Forward, Inverse};

    // Create ops in forward direction.
    let mut ops = build(&config, &lt, Forward).unwrap();
    ops.validate().unwrap();
    assert_eq!(ops.len(), 18); // There are many no-ops.

    // Source color space to look1 process color space.
    // No-ops are created at the beginning and at the end of the color space conversion.
    assert!(ops[0].is_no_op_type());
    validate_transform(&ops[1], "src", Forward); // Source to ref.
    validate_transform(&ops[2], "look1_cs trans", Inverse); // Ref to look1_cs.
    assert!(ops[3].is_no_op_type());

    // Look1 transform.
    assert!(ops[4].is_no_op_type()); // No-op added before each look.
    validate_transform(&ops[5], "look1 trans", Forward);

    // Look1 process color space to look2 process color space.
    assert!(ops[6].is_no_op_type());
    validate_transform(&ops[7], "look1_cs trans", Forward); // Look1 cs to ref.
    validate_transform(&ops[8], "look2_3_cs trans", Inverse); // Ref to look2_3_cs.
    assert!(ops[9].is_no_op_type());

    // Look2 transform.
    assert!(ops[10].is_no_op_type());
    // Look2 has both forward and inverse, using forward.
    validate_transform(&ops[11], "look2 trans", Forward);

    // No color space conversion: look2 & look3 have the same process color space.

    // Look3 transform.
    assert!(ops[12].is_no_op_type());
    // Look3 is applied with a "-" so want to use the inverse_transform direction.
    validate_transform(&ops[13], "look3 inverse trans", Forward);

    // Look3 process color space to destination color space.
    assert!(ops[14].is_no_op_type());
    validate_transform(&ops[15], "look2_3_cs trans", Forward); // Look2_3_cs to ref.
    validate_transform(&ops[16], "dst", Forward); // Ref to detination.
    assert!(ops[17].is_no_op_type());

    // Test in inverse direction.
    let mut ops = build(&config, &lt, Inverse).unwrap();
    ops.validate().unwrap();
    assert_eq!(ops.len(), 18);

    // Destination color space to Look3 process color space.
    assert!(ops[0].is_no_op_type());
    validate_transform(&ops[1], "dst", Inverse);
    validate_transform(&ops[2], "look2_3_cs trans", Inverse);
    assert!(ops[3].is_no_op_type());

    // Look3 transform.
    assert!(ops[4].is_no_op_type());
    // Forward transform is not available, so use the inverse of the inverse_transform.
    validate_transform(&ops[5], "look3 inverse trans", Inverse);

    // No color space conversion: look3 and look2 have the same process color space.

    // Look2 transform.
    assert!(ops[6].is_no_op_type());
    // Look2 has both forward and inverse, using inverse.
    validate_transform(&ops[7], "look2 inverse trans", Forward);

    // Look2 process color space to look1 process color space.
    assert!(ops[8].is_no_op_type());
    validate_transform(&ops[9], "look2_3_cs trans", Forward);
    validate_transform(&ops[10], "look1_cs trans", Inverse);
    assert!(ops[11].is_no_op_type());

    // Look1 transform.
    assert!(ops[12].is_no_op_type());
    // Inverse_transform not available so use the inverse of the forward transform.
    validate_transform(&ops[13], "look1 trans", Inverse);

    // Look1 process color space to source color space.
    assert!(ops[14].is_no_op_type());
    validate_transform(&ops[15], "look1_cs trans", Forward);
    validate_transform(&ops[16], "src", Inverse);
    assert!(ops[17].is_no_op_type());
}

/// Port of `OCIO_ADD_TEST(LookTransform, context_variables)` @ v2.5.2.
#[test]
fn context_variables() {
    let _env = EnvGuard::new();
    const OCIO_CONFIG: &str = r#"
ocio_profile_version: 2

environment: { FILE1: cdl_test1.cc, FILE2: cdl_test1.cc }

roles:
  default: cs1

file_rules:
  - !<Rule> {name: Default, colorspace: default}

displays:
  Disp1:
  - !<View> {name: View1, colorspace: cs1}

looks:
  - !<Look>
    name: look1
    process_space: default
    transform: !<FileTransform> {src: $FILE1}
  - !<Look>
    name: look2
    process_space: default
    inverse_transform: !<LookTransform> {src: default, dst: cs2, looks: +look1}
  - !<Look>
    name: look3
    process_space: default
    transform: !<CDLTransform> {offset: [0.1, 0.1, 0.1]}
  - !<Look>
    name: look4
    process_space: cs4
    transform: !<CDLTransform> {offset: [0.1, 0.1, 0.1]}

colorspaces:
  - !<ColorSpace>
    name: cs1
  - !<ColorSpace>
    name: cs2
    from_reference: !<MatrixTransform> {offset: [0.11, 0.12, 0.13, 0]}
  - !<ColorSpace>
    name: cs3
    from_reference: !<MatrixTransform> {offset: [0.1, 0.2, 0.3, 0]}
  - !<ColorSpace>
    name: cs4
    from_reference: !<FileTransform> {src: $FILE2}
"#;

    let mut cfg = (*Config::create_from_stream(OCIO_CONFIG.as_bytes()).unwrap()).clone();
    cfg.set_search_path(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../upstream/OpenColorIO/tests/data/files"
    ));
    cfg.validate().unwrap();

    // Collects the variables of `look` in the config's current context, in a new context.
    let collect = |look: &LookTransform| {
        let mut used_context_vars = Context::new(); // New & empty instance.
        let found = collect_look_transform_context_variables(
            &cfg,
            &cfg.current_context().get(),
            look,
            &mut used_context_vars,
        )
        .unwrap();
        let vars: Vec<(Vec<u8>, Vec<u8>)> = (0..used_context_vars.num_string_vars())
            .map(|i| {
                (
                    used_context_vars.string_var_name_by_index(i).to_vec(),
                    used_context_vars.string_var_by_index(i).to_vec(),
                )
            })
            .collect();
        (found, vars)
    };
    let var = |name: &str, value: &str| (name.as_bytes().to_vec(), value.as_bytes().to_vec());

    let mut look = LookTransform::new();
    look.set_src("cs1");
    look.set_dst("cs3");
    assert_eq!(collect(&look), (false, vec![]));

    // Step 1 - Test each basic cases.

    look.set_looks("+look1");
    assert_eq!(collect(&look), (true, vec![var("FILE1", "cdl_test1.cc")]));

    look.set_looks("-look2");
    assert_eq!(collect(&look), (true, vec![var("FILE1", "cdl_test1.cc")]));

    look.set_looks("look3");
    assert_eq!(collect(&look), (false, vec![]));

    look.set_looks("+look4");
    assert_eq!(collect(&look), (true, vec![var("FILE2", "cdl_test1.cc")]));

    // Step 2 - Test with several looks.

    look.set_looks("look3, -look1");
    assert_eq!(collect(&look), (true, vec![var("FILE1", "cdl_test1.cc")]));

    look.set_looks("look3, -look2, +look4");
    assert_eq!(
        collect(&look),
        (
            true,
            vec![var("FILE1", "cdl_test1.cc"), var("FILE2", "cdl_test1.cc")]
        )
    );
}

/// Port of `OCIO_ADD_TEST(LookTransform, inverse_look_transform)` @ v2.5.2.
#[test]
fn inverse_look_transform() {
    let _env = EnvGuard::new();
    // Test inversion of the transform containing a look.

    const OCIO_CONFIG: &str = r#"
ocio_profile_version: 2

search_path: luts

roles:
  default: raw

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}

looks:
  - !<Look>
    name: look1
    process_space: log
    transform: !<CDLTransform> {sat: 0.8}

colorspaces:
  - !<ColorSpace>
    name: raw
    family: raw
    bitdepth: 32f
    isdata: false

  - !<ColorSpace>
    name: log
    to_scene_reference: !<LogTransform> {base: 2, direction: inverse}

  - !<ColorSpace>
    name: vd
    from_scene_reference: !<ExponentTransform> {value: [2.4, 2.4, 2.4, 1], direction: inverse}

  - !<ColorSpace>
    name: vd_graded
    from_scene_reference: !<LookTransform> {src: raw, dst: vd, looks: look1}

  - !<ColorSpace>
    name: vd_graded_inverse
    to_scene_reference: !<LookTransform> {src: raw, dst: vd, looks: look1, direction: inverse}

"#;

    use TransformDirection::{Forward, Inverse};

    let config = Config::create_from_stream(OCIO_CONFIG.as_bytes()).unwrap();
    config.validate().unwrap();

    let src_color_space = config.color_space("raw").unwrap();
    let dst_color_space = config.color_space("vd_graded").unwrap();
    let context = config.current_context().get();

    let direction = |op: &Op| match &**op.data() {
        OpData::Log(d) => (OpDataType::Log, d.direction()),
        OpData::Cdl(d) => (OpDataType::Cdl, d.get_direction()),
        OpData::Gamma(d) => (OpDataType::Gamma, d.direction()),
        other => panic!("unexpected op: {other:?}"),
    };

    let mut ops = OpVec::new();
    build_color_space_ops_between(
        &mut ops,
        &config,
        &context,
        Some(src_color_space),
        Some(dst_color_space),
        true,
    )
    .unwrap();
    ops.validate().unwrap();
    assert_eq!(ops.len(), 11);
    assert!(ops[0].is_no_op_type());
    assert!(ops[1].is_no_op_type());
    assert_eq!(direction(&ops[2]), (OpDataType::Log, Forward)); // raw to log
    assert!(ops[3].is_no_op_type());
    assert!(ops[4].is_no_op_type());
    assert_eq!(direction(&ops[5]), (OpDataType::Cdl, Forward)); // look
    assert!(ops[6].is_no_op_type());
    assert_eq!(direction(&ops[7]), (OpDataType::Log, Inverse)); // log to raw
    assert_eq!(direction(&ops[8]), (OpDataType::Gamma, Inverse)); // raw to vd
    assert!(ops[9].is_no_op_type());
    assert!(ops[10].is_no_op_type());

    // Test in inverse direction.
    let mut ops = OpVec::new();
    build_color_space_ops_between(
        &mut ops,
        &config,
        &context,
        Some(dst_color_space),
        Some(src_color_space),
        true,
    )
    .unwrap();
    assert_eq!(ops.len(), 11);
    ops.validate().unwrap();
    assert!(ops[0].is_no_op_type());
    assert!(ops[1].is_no_op_type());
    assert_eq!(direction(&ops[2]), (OpDataType::Gamma, Forward)); // vd to raw
    assert_eq!(direction(&ops[3]), (OpDataType::Log, Forward)); // raw to log
    assert!(ops[4].is_no_op_type());
    assert!(ops[5].is_no_op_type());
    assert_eq!(direction(&ops[6]), (OpDataType::Cdl, Inverse)); // look
    assert!(ops[7].is_no_op_type());
    assert_eq!(direction(&ops[8]), (OpDataType::Log, Inverse)); // log to raw
    assert!(ops[9].is_no_op_type());
    assert!(ops[10].is_no_op_type());

    // Generated ops for vd_graded_inverse should be identical to the above
    // (only difference being that it's defined using to_scene_reference and inverse
    // look transform direction instead of from_scene_reference)
    let dst_color_space_inv = config.color_space("vd_graded_inverse").unwrap();

    let mut ops2 = OpVec::new();
    build_color_space_ops_between(
        &mut ops2,
        &config,
        &context,
        Some(dst_color_space_inv),
        Some(src_color_space),
        true,
    )
    .unwrap();
    assert_eq!(ops2.len(), ops.len());
    ops2.validate().unwrap();
    for i in 0..ops2.len() {
        assert!(**ops[i].data() == **ops2[i].data());
    }
}
