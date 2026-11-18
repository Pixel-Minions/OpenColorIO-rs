// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the reference-space converters and the color-space fingerprints:
//! `tests/cpu/ConfigUtils_tests.cpp` @ v2.5.2.

use ocio_testkit::upstream::{check_close, check_throw_what};

use super::*;
use crate::test_env::EnvGuard;
use crate::transform::TransformType;
use crate::transforms::matrix_transform::MatrixTransform;
use crate::view_transform::ViewTransform;

/// `OCIO_CHECK_CLOSE(x, y, 1e-5f)` on doubles: the float tolerance promoted to double.
#[track_caller]
fn close(x: f64, y: f64) {
    check_close(x, y, f64::from(1e-5f32));
}

/// The transform's MatrixTransform (upstream's `DynamicPtrCast<const MatrixTransform>`, which
/// `OCIO_REQUIRE_ASSERT` checks).
#[track_caller]
fn matrix(t: &Transform) -> &MatrixTransform {
    match t {
        Transform::Matrix(m) => m,
        other => panic!("not a MatrixTransform: {:?}", other.transform_type()),
    }
}

/// The transform's GroupTransform.
#[track_caller]
fn group(t: &Transform) -> &GroupTransform {
    match t {
        Transform::Group(g) => g,
        other => panic!("not a GroupTransform: {:?}", other.transform_type()),
    }
}

/// Port of `OCIO_ADD_TEST(MergeConfigs, config_utils_update_reference)` @ v2.5.2.
#[test]
fn config_utils_update_reference() {
    const BASE: &str = r#"ocio_profile_version: 2.1
name: base
environment:
  TEXTURE_SPACE: sRGB - Texture
  SHOT: 001a

search_path:
  - luts
  - .

roles:
  cie_xyz_d65_interchange: CIE-XYZ-D65

file_rules:
  - !<Rule> {name: Default, colorspace: ACEScg}

#inactive_colorspaces: [sRGB - Texture, ACEScg]

display_colorspaces:   # reference space = cie xyz d65
  - !<ColorSpace>
    name: sRGB - Display
    aliases: [srgb_display]
    family: Display-Basic
    description: from base
    from_display_reference: !<BuiltinTransform> {style: DISPLAY - CIE-XYZ-D65_to_sRGB}

  - !<ColorSpace>
    name: CIE-XYZ-D65
    aliases: [cie_xyz_d65]
    description: The \"CIE XYZ (D65)\" display connection colorspace.
    isdata: false

colorspaces:   # reference space = aces2065-1
  - !<ColorSpace>
    name: ACEScg
    aliases: [aces]
    family: ACES-Linear
    description: from base
    to_scene_reference: !<BuiltinTransform> {style: ACEScg_to_ACES2065-1}

  - !<ColorSpace>
    name: ap0
    family: ACES-Linear
    description: from base

  - !<ColorSpace>
    name: sRGB - Texture
    family: Texture
    aliases: [srgb, srgb_tx]
    description: from base
    from_scene_reference: !<GroupTransform>
      children:
        - !<MatrixTransform> {matrix: [2.52168618674388, -1.13413098823972, -0.387555198504164, 0, -0.276479914229922, 1.37271908766826, -0.096239173438334, 0, -0.0153780649660342, -0.152975335867399, 1.16835340083343, 0, 0, 0, 0, 1]}
        - !<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055, direction: inverse}

  - !<ColorSpace>
    name: look
    aliases:
    family:
    description: from base
    from_scene_reference: !<ColorSpaceTransform> {src: ACEScg, dst: sRGB - Texture}
"#;

    const INPUT: &str = r#"ocio_profile_version: 2.1
name: input
search_path: lut_dir
#inactive_colorspaces: [ACES2065-1]

roles:
  cie_xyz_d65_interchange: CIE-XYZ-D65

file_rules:
  - !<Rule> {name: Default, colorspace: sRGB}

displays:
  sRGB - Display:
    - !<View> {name: Raw, colorspace: raw}
    - !<View> {name: ACES 1.0 - SDR Video, view_transform: ACES 1.0 - SDR Video, display_colorspace: sRGB - Display}

view_transforms:
  - !<ViewTransform>
    name: ACES 1.0 - SDR Video
    from_scene_reference:  !<GroupTransform>
      children:
        # Matrix from rec.709 to aces2065-1
        - !<MatrixTransform> {matrix: [ 0.439632981919, 0.382988698152, 0.177378319929, 0, 0.089776442959, 0.813439428749, 0.096784128292, 0, 0.017541170383, 0.111546553302, 0.870912276314, 0, 0, 0, 0, 1 ]}
        # Built-in transform from aces2065-1 to cie-xyz
        - !<BuiltinTransform> {style: ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO_1.0}
        # Matrix from cie-xyz to linear rec.709
        - !<MatrixTransform> {matrix: [ 3.240969941905, -1.537383177570, -0.498610760293, 0, -0.969243636281, 1.875967501508, 0.041555057407, 0, 0.055630079697, -0.203976958889, 1.056971514243, 0, 0, 0, 0, 1 ]}

  - !<ViewTransform>
    name: vt scene ref
    from_scene_reference: !<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055, direction: inverse}
    to_scene_reference: !<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055}

  - !<ViewTransform>
    name: vt display ref
    description: from base
    from_display_reference: !<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055, direction: inverse}
    to_display_reference: !<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055}

display_colorspaces:   # reference space = linear rec 709
  - !<ColorSpace>
    name: sRGB - Display
    aliases: [srgb_display]
    family: Display~Standard
    description: from input
    from_display_reference: !<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055, direction: inverse}

  - !<ColorSpace>
    name: CIE-XYZ-D65
    aliases: [cie_xyz_d65]
    description: The \"CIE XYZ (D65)\" display connection colorspace.
    isdata: false
    from_display_reference: !<MatrixTransform> {matrix: [ 0.412390799266, 0.357584339384, 0.180480788402, 0, 0.212639005872, 0.715168678768, 0.072192315361, 0, 0.019330818716, 0.119194779795, 0.950532152250, 0, 0, 0, 0, 1 ]}

colorspaces:   # reference space = linear rec 709
  - !<ColorSpace>
    name: ACES2065-1
    aliases: [aces]
    family: ACES~Linear
    description: from input
    to_scene_reference: !<MatrixTransform> {matrix: [ 2.521686186744, -1.134130988240, -0.387555198504, 0, -0.276479914230, 1.372719087668, -0.096239173438, 0, -0.015378064966, -0.152975335867, 1.168353400833, 0, 0, 0, 0, 1 ]}

  - !<ColorSpace>
    name: sRGB
    family: Texture~
    description: from input
    to_scene_reference: !<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055}

  - !<ColorSpace>
    name: rec709
    description: from input

  - !<ColorSpace>
    name: raw
    description: from base
    isdata: true
"#;

    let base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
    let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();
    let _cg_config =
        Config::create_from_file("ocio://cg-config-v1.0.0_aces-v1.3_ocio-v2.1").unwrap();

    // Scene-referred reference space check.

    // Get the transform to convert the scene-referred reference space.
    let input_to_base_gt_scene =
        ref_space_converter(&input_config, &base_config, ReferenceSpaceType::Scene).unwrap();
    {
        // Convert each one of the scene-referred color spaces and check the result.
        let mut colorspaces = Vec::new();
        let num_spaces = input_config
            .num_color_spaces_with(SearchReferenceSpaceType::Scene, ColorSpaceVisibility::All);
        for i in 0..num_spaces {
            let name = input_config.color_space_name_by_index_with(
                SearchReferenceSpaceType::Scene,
                ColorSpaceVisibility::All,
                i,
            );
            let cs = input_config.color_space(name).unwrap();
            // scene-referred colorspace
            colorspaces.push(cs);
        }
        assert_eq!(colorspaces.len(), 4);

        // ACES2065-1 no longer needs transforms, it is now the reference space. But transforms
        // are not simplified, for clarity in what was done, so it is a forward and inverse of
        // the same matrix.
        {
            let mut cs = colorspaces[0].clone();
            update_reference_colorspace(&mut cs, &input_to_base_gt_scene).unwrap();
            let t = cs.transform(ColorSpaceDirection::ToReference).unwrap();
            assert_eq!(t.transform_type(), TransformType::Group);

            let gtx = group(t);
            assert_eq!(gtx.num_transforms(), 2);
            {
                let mtx0 = matrix(gtx.transform(0).unwrap());
                assert!(mtx0.direction() == TransformDirection::Forward);
                let m44 = mtx0.matrix();
                close(m44[0], 2.521686186744);
            }
            {
                let mtx1 = matrix(gtx.transform(1).unwrap());
                assert!(mtx1.direction() == TransformDirection::Forward);
                let m44 = mtx1.matrix();
                close(m44[0], 0.4396329819194919);
            }

            assert!(cs.transform(ColorSpaceDirection::FromReference).is_none());
        }

        // sRGB now needs a Rec.709 to ACES2065-1 matrix after the exponent.
        {
            let mut cs = colorspaces[1].clone();
            update_reference_colorspace(&mut cs, &input_to_base_gt_scene).unwrap();
            let t = cs.transform(ColorSpaceDirection::ToReference).unwrap();
            assert_eq!(t.transform_type(), TransformType::Group);

            let gtx = group(t);
            assert_eq!(gtx.num_transforms(), 2);

            let mtx0 = matrix(gtx.transform(1).unwrap());
            assert!(mtx0.direction() == TransformDirection::Forward);
            let m44 = mtx0.matrix();
            close(m44[0], 0.4396329819194919);
            close(m44[1], 0.3829886981515535);

            assert!(cs.transform(ColorSpaceDirection::FromReference).is_none());
        }

        // rec709 had no transforms but now needs the Rec.709 to ACES2065-1 matrix.
        {
            let mut cs = colorspaces[2].clone();
            update_reference_colorspace(&mut cs, &input_to_base_gt_scene).unwrap();
            let t = cs.transform(ColorSpaceDirection::ToReference).unwrap();
            assert_eq!(t.transform_type(), TransformType::Group);
            let gtx = group(t);
            assert_eq!(gtx.num_transforms(), 1);

            let mtx0 = matrix(gtx.transform(0).unwrap());
            assert!(mtx0.direction() == TransformDirection::Forward);
            let m44 = mtx0.matrix();
            close(m44[0], 0.4396329819194919);
            close(m44[1], 0.3829886981515535);

            assert!(cs.transform(ColorSpaceDirection::FromReference).is_none());
        }

        // raw had no transforms and none are added since isdata is true.
        {
            let mut cs = colorspaces[3].clone();
            update_reference_colorspace(&mut cs, &input_to_base_gt_scene).unwrap();
            assert!(cs.is_data());
            assert!(cs.transform(ColorSpaceDirection::ToReference).is_none());
            assert!(cs.transform(ColorSpaceDirection::FromReference).is_none());
        }
    }

    // Display-referred reference space check.

    // Get the transform to convert the display-referred reference space.
    let input_to_base_gt_display =
        ref_space_converter(&input_config, &base_config, ReferenceSpaceType::Display).unwrap();
    {
        // Convert each one of the display-referred color spaces and check the result.
        let mut colorspaces = Vec::new();
        let num_spaces = input_config
            .num_color_spaces_with(SearchReferenceSpaceType::Display, ColorSpaceVisibility::All);
        for i in 0..num_spaces {
            let name = input_config.color_space_name_by_index_with(
                SearchReferenceSpaceType::Display,
                ColorSpaceVisibility::All,
                i,
            );
            let cs = input_config.color_space(name).unwrap();
            colorspaces.push(cs);
        }
        assert_eq!(colorspaces.len(), 2);

        // sRGB - Display needs a CIE-XYZ to Rec.709 matrix before the exponent.
        {
            let mut cs = colorspaces[0].clone();
            update_reference_colorspace(&mut cs, &input_to_base_gt_display).unwrap();
            assert!(cs.transform(ColorSpaceDirection::ToReference).is_none());

            let t = cs.transform(ColorSpaceDirection::FromReference).unwrap();
            assert_eq!(t.transform_type(), TransformType::Group);
            let gtx = group(t);
            assert_eq!(gtx.num_transforms(), 2);

            assert_eq!(
                gtx.transform(0).unwrap().transform_type(),
                TransformType::Matrix
            );
            let mtx0 = matrix(gtx.transform(0).unwrap());
            assert!(mtx0.direction() == TransformDirection::Inverse);
            let m44 = mtx0.matrix();
            // The coefficients are for Rec.709 to CIE-XYZ, but the direction is inverse.
            close(m44[0], 0.412390799266);
            close(m44[1], 0.357584339384);

            assert_eq!(
                gtx.transform(1).unwrap().transform_type(),
                TransformType::ExponentWithLinear
            );
        }

        // CIE-XYZ-D65 had a matrix but no longer needs transforms, it's now the reference
        // space. An additional matrix is added to invert the original matrix.
        {
            let mut cs = colorspaces[1].clone();
            update_reference_colorspace(&mut cs, &input_to_base_gt_display).unwrap();
            assert!(cs.transform(ColorSpaceDirection::ToReference).is_none());

            let t = cs.transform(ColorSpaceDirection::FromReference).unwrap();
            assert_eq!(t.transform_type(), TransformType::Group);
            let gtx = group(t);
            assert_eq!(gtx.num_transforms(), 2);

            {
                assert_eq!(
                    gtx.transform(0).unwrap().transform_type(),
                    TransformType::Matrix
                );
                let mtx0 = matrix(gtx.transform(0).unwrap());
                assert!(mtx0.direction() == TransformDirection::Inverse);
                let m44 = mtx0.matrix();
                // The coefficients are for Rec.709 to CIE-XYZ, but the direction is inverse.
                close(m44[0], 0.412390799266);
            }
            {
                assert_eq!(
                    gtx.transform(1).unwrap().transform_type(),
                    TransformType::Matrix
                );
                let mtx0 = matrix(gtx.transform(1).unwrap());
                assert!(mtx0.direction() == TransformDirection::Forward);
                let m44 = mtx0.matrix();
                close(m44[0], 0.412390799266);
            }
        }
    }

    // View transform reference space check.

    // Convert each one of the view transforms and check the result. The view transform of the
    // input config goes from linear rec.709 to linear rec.709. This needs to be adapted to the
    // base config that goes from cie-xyz to aces2065-1.
    let mut view_transforms: Vec<&ViewTransform> = Vec::new();
    for v in 0..input_config.num_view_transforms() {
        let name = input_config.view_transform_name_by_index(v);
        view_transforms.push(input_config.view_transform(name).unwrap());
    }
    assert_eq!(view_transforms.len(), 3);

    {
        let mut vt = view_transforms[0].clone();
        update_reference_view(&mut vt, &input_to_base_gt_scene, &input_to_base_gt_display).unwrap();
        assert!(vt.transform(ViewTransformDirection::ToReference).is_none());

        let t = vt.transform(ViewTransformDirection::FromReference).unwrap();
        assert_eq!(t.transform_type(), TransformType::Group);
        let gtx = group(t);
        assert_eq!(gtx.num_transforms(), 3);

        // Matrix from ACES2065-1 to Rec.709.
        {
            assert_eq!(
                gtx.transform(0).unwrap().transform_type(),
                TransformType::Matrix
            );
            let mtx0 = matrix(gtx.transform(0).unwrap());
            assert!(mtx0.direction() == TransformDirection::Inverse);
            let m44 = mtx0.matrix();
            // The coefficients are for Rec.709 to ACES2065-1, but the direction is inverse.
            close(m44[0], 0.439632981919);
        }

        // The original group transform from the input config.
        {
            assert_eq!(
                gtx.transform(1).unwrap().transform_type(),
                TransformType::Group
            );
            let gtx_a = group(gtx.transform(1).unwrap());
            assert_eq!(gtx_a.num_transforms(), 3);
            {
                let mtx0 = matrix(gtx_a.transform(0).unwrap());
                assert!(mtx0.direction() == TransformDirection::Forward);
                let m44 = mtx0.matrix();
                close(m44[0], 0.439632981919);
            }
            assert_eq!(
                gtx_a.transform(1).unwrap().transform_type(),
                TransformType::Builtin
            );
            {
                let mtx2 = matrix(gtx_a.transform(2).unwrap());
                assert!(mtx2.direction() == TransformDirection::Forward);
                let m44 = mtx2.matrix();
                close(m44[0], 3.240969941905);
            }
        }

        // Matrix from Rec.709 to CIE-XYZ.
        {
            let mtx2 = matrix(gtx.transform(2).unwrap());
            assert!(mtx2.direction() == TransformDirection::Forward);
            let m44 = mtx2.matrix();
            close(m44[0], 0.412390799266);
        }
    }

    // Verify that both directions are converted correctly.
    {
        let mut vt = view_transforms[1].clone();
        update_reference_view(&mut vt, &input_to_base_gt_scene, &input_to_base_gt_display).unwrap();

        assert_eq!(vt.reference_space_type(), ReferenceSpaceType::Scene);

        {
            let t = vt.transform(ViewTransformDirection::FromReference).unwrap();
            assert_eq!(t.transform_type(), TransformType::Group);
            let gtx = group(t);
            assert_eq!(gtx.num_transforms(), 3);
        }
        let t = vt.transform(ViewTransformDirection::ToReference).unwrap();
        assert_eq!(t.transform_type(), TransformType::Group);
        let gtx = group(t);
        assert_eq!(gtx.num_transforms(), 3);

        // Matrix from CIE-XYZ to Rec.709.
        {
            assert_eq!(
                gtx.transform(0).unwrap().transform_type(),
                TransformType::Matrix
            );
            let mtx2 = matrix(gtx.transform(0).unwrap());
            assert!(mtx2.direction() == TransformDirection::Inverse);
            let m44 = mtx2.matrix();
            // The coefficients are for Rec.709 to CIE-XYZ, but the direction is inverse.
            close(m44[0], 0.412390799266);
        }
        // The original transform from the input config.
        {
            assert_eq!(
                gtx.transform(1).unwrap().transform_type(),
                TransformType::ExponentWithLinear
            );
        }
        // Matrix from Rec.709 to ACES2065-1.
        {
            let mtx0 = matrix(gtx.transform(2).unwrap());
            assert!(mtx0.direction() == TransformDirection::Forward);
            let m44 = mtx0.matrix();
            close(m44[0], 0.439632981919);
        }
    }

    // Verify that display-ref view transforms have the display converter on both sides.
    {
        let mut vt = view_transforms[2].clone();
        update_reference_view(&mut vt, &input_to_base_gt_scene, &input_to_base_gt_display).unwrap();

        assert_eq!(vt.reference_space_type(), ReferenceSpaceType::Display);

        // FROM-REF direction.

        let t = vt.transform(ViewTransformDirection::FromReference).unwrap();
        assert_eq!(t.transform_type(), TransformType::Group);
        let gtx = group(t);
        assert_eq!(gtx.num_transforms(), 3);

        // Because the reference space type is display, use the XYZ matrix on both ends.
        // Matrix from CIE-XYZ to Rec.709.
        {
            assert_eq!(
                gtx.transform(0).unwrap().transform_type(),
                TransformType::Matrix
            );
            let mtx2 = matrix(gtx.transform(0).unwrap());
            assert!(mtx2.direction() == TransformDirection::Inverse);
            let m44 = mtx2.matrix();
            // The coefficients are for Rec.709 to CIE-XYZ, but the direction is inverse.
            close(m44[0], 0.412390799266);
        }
        // The original transform from the input config.
        {
            assert_eq!(
                gtx.transform(1).unwrap().transform_type(),
                TransformType::ExponentWithLinear
            );
        }
        // Matrix from Rec.709 to CIE-XYZ.
        {
            let mtx0 = matrix(gtx.transform(2).unwrap());
            assert!(mtx0.direction() == TransformDirection::Forward);
            let m44 = mtx0.matrix();
            close(m44[0], 0.412390799266);
        }

        // TO-REF direction.

        let t1 = vt.transform(ViewTransformDirection::ToReference).unwrap();
        assert_eq!(t1.transform_type(), TransformType::Group);
        let gtx = group(t1);
        assert_eq!(gtx.num_transforms(), 3);

        // Matrix from CIE-XYZ to Rec.709.
        {
            assert_eq!(
                gtx.transform(0).unwrap().transform_type(),
                TransformType::Matrix
            );
            let mtx2 = matrix(gtx.transform(0).unwrap());
            assert!(mtx2.direction() == TransformDirection::Inverse);
            let m44 = mtx2.matrix();
            // The coefficients are for Rec.709 to CIE-XYZ, but the direction is inverse.
            close(m44[0], 0.412390799266);
        }
        // The original transform from the input config.
        {
            assert_eq!(
                gtx.transform(1).unwrap().transform_type(),
                TransformType::ExponentWithLinear
            );
        }
        // Because the reference space type is display, use the XYZ matrix on both ends.
        // Matrix from Rec.709 to CIE-XYZ.
        {
            let mtx0 = matrix(gtx.transform(2).unwrap());
            assert!(mtx0.direction() == TransformDirection::Forward);
            let m44 = mtx0.matrix();
            close(m44[0], 0.412390799266);
        }
    }

    // Test when the conversion is a no-op (base to base).
    // Get the transform to convert the scene-referred reference space.
    let input_to_base_gt_scene =
        ref_space_converter(&base_config, &base_config, ReferenceSpaceType::Scene).unwrap();

    {
        let mut cs = input_config.color_space("ACES2065-1").unwrap().clone();
        update_reference_colorspace(&mut cs, &input_to_base_gt_scene).unwrap();
        let t = cs.transform(ColorSpaceDirection::ToReference).unwrap();
        assert_eq!(t.transform_type(), TransformType::Matrix);
    }

    {
        let mut vt = view_transforms[1].clone();
        update_reference_view(&mut vt, &input_to_base_gt_scene, &input_to_base_gt_display).unwrap();
        {
            let t = vt.transform(ViewTransformDirection::FromReference).unwrap();
            assert_eq!(t.transform_type(), TransformType::Group);
            let gtx = group(t);
            assert_eq!(gtx.num_transforms(), 2);
            assert_eq!(
                gtx.transform(0).unwrap().transform_type(),
                TransformType::ExponentWithLinear
            );
            assert_eq!(
                gtx.transform(1).unwrap().transform_type(),
                TransformType::Matrix
            );
        }
    }

    // Both sides are now no-ops.
    let input_to_base_gt_display =
        ref_space_converter(&input_config, &input_config, ReferenceSpaceType::Display).unwrap();
    {
        let mut vt = view_transforms[1].clone();
        update_reference_view(&mut vt, &input_to_base_gt_scene, &input_to_base_gt_display).unwrap();
        {
            let t = vt.transform(ViewTransformDirection::FromReference).unwrap();
            assert_eq!(t.transform_type(), TransformType::ExponentWithLinear);
        }
    }
}

/// Port of `OCIO_ADD_TEST(MergeConfigs, config_utils_find_equivalent_colorspace)` @ v2.5.2.
#[test]
fn config_utils_find_equivalent_colorspace() {
    const INPUT: &str = r#"ocio_profile_version: 2.1
environment:
  {}
search_path: ""

file_rules:
  - !<Rule> {name: Default, colorspace: ref_space}

inactive_colorspaces: []

colorspaces:
  - !<ColorSpace>
    name: ref_space
    description: The reference space, but with a different name.
    isdata: false

  - !<ColorSpace>
    name: Unknown
    description: Raw but with a different name.
    isdata: true

  - !<ColorSpace>
    name: requires ACES cct
    isdata: false
    description: A color space that requires a space lower in the config that may not have been added yet.
    to_scene_reference: !<ColorSpaceTransform> {src: ACES cct, dst: ref_space}

  - !<ColorSpace>
    name: standard RGB
    isdata: false
    description: sRGB - Texture
    from_scene_reference: !<GroupTransform>
      children:
        - !<MatrixTransform> {matrix: [2.52168618674388, -1.13413098823972, -0.387555198504164, 0, -0.276479914229922, 1.37271908766826, -0.096239173438334, 0, -0.0153780649660342, -0.152975335867399, 1.16835340083343, 0, 0, 0, 0, 1]}
        - !<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055, direction: inverse}

  - !<ColorSpace>
    name: approx. standard RGB
    isdata: false
    description: sRGB - Texture with truncated matrix values
    from_scene_reference: !<GroupTransform>
      children:
        - !<MatrixTransform> {matrix: [2.522, -1.134, -0.388, 0, -0.276, 1.373, -0.0962, 0, -0.0154, -0.153, 1.168, 0, 0, 0, 0, 1]}
        - !<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055, direction: inverse}

  - !<ColorSpace>
    name: very approx. standard RGB
    isdata: false
    description: sRGB - Texture with truncated matrix values and different gamma
    from_scene_reference: !<GroupTransform>
      children:
        - !<MatrixTransform> {matrix: [2.52, -1.13, -0.39, 0, -0.28, 1.37, -0.096, 0, -0.015, -0.15, 1.17, 0, 0, 0, 0, 1]}
        - !<ExponentWithLinearTransform> {gamma: 2.42, offset: 0.055, direction: inverse}

  - !<ColorSpace>
    name: ACES cct
    isdata: false
    description: ACEScct
    to_scene_reference: !<BuiltinTransform> {style: ACEScct_to_ACES2065-1}

  - !<ColorSpace>
    name: ACES cg
    isdata: false
    description: ACEScg but with a Matrix rather than Builtin Transform.
    to_reference: !<MatrixTransform> {matrix: [ 0.695452241357, 0.140678696470, 0.163869062172, 0, 0.044794563372, 0.859671118456, 0.095534318172, 0, -0.005525882558, 0.004025210306, 1.001500672252, 0, 0, 0, 0, 1 ]}

  - !<ColorSpace>
    name: pq display scene
    isdata: false
    description: Rec.2100-PQ - incorrectly as scene-referred
    from_reference: !<GroupTransform>
      children:
        - !<BuiltinTransform> {style: UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD}
        - !<BuiltinTransform> {style: DISPLAY - CIE-XYZ-D65_to_REC.2100-PQ}

display_colorspaces:
  - !<ColorSpace>
    name: pq display
    isdata: false
    description: Rec.2100-PQ - Display
    from_display_reference: !<BuiltinTransform> {style: DISPLAY - CIE-XYZ-D65_to_REC.2100-PQ}

  - !<ColorSpace>
    name: ACES cg display
    isdata: false
    description: ACEScg - incorrectly as a display
    to_display_reference: !<GroupTransform>
      children:
        - !<MatrixTransform> {matrix: [ 0.695452241357, 0.140678696470, 0.163869062172, 0, 0.044794563372, 0.859671118456, 0.095534318172, 0, -0.005525882558, 0.004025210306, 1.001500672252, 0, 0, 0, 0, 1 ]}

  - !<ColorSpace>
    name: Rec.601 - Display
    isdata: false
    from_display_reference: !<GroupTransform>
      children:
        - !<MatrixTransform> {matrix: [3.50600328272467, -1.73979072630283, -0.544058268362742, 0, -1.06904755985382, 1.97777888272879, 0.0351714193371952, 0, 0.0563065917341277, -0.196975654820772, 1.04995232821873, 0, 0, 0, 0, 1]}
        - !<ExponentTransform> {value: 2.4, style: mirror, direction: inverse}

  - !<ColorSpace>
    name: Rec.601 (PAL) - Display
    isdata: false
    from_display_reference: !<GroupTransform>
      children:
        - !<MatrixTransform> {matrix: [3.06336109008327, -1.39339017490737, -0.47582373799753, 0, -0.96924363628088, 1.87596750150772, 0.0415550574071756, 0, 0.0678610475535669, -0.228799269620496, 1.06908961801603, 0, 0, 0, 0, 1]}
        - !<ExponentTransform> {value: 2.4, style: mirror, direction: inverse}
"#;

    let base_config =
        Config::create_from_file("ocio://cg-config-v1.0.0_aces-v1.3_ocio-v2.1").unwrap();
    let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();

    let mut fingerprints = ColorSpaceFingerprints::default();
    initialize_color_space_fingerprints(&mut fingerprints, &base_config).unwrap();

    let find = |name: &str| -> Vec<u8> {
        let cs = input_config.color_space(name).unwrap();
        find_equivalent_colorspace(&fingerprints, &input_config, cs)
            .unwrap()
            .to_vec()
    };

    assert_eq!(find("ref_space"), b"ACES2065-1");
    assert_eq!(find("standard RGB"), b"sRGB - Texture");
    assert_eq!(find("approx. standard RGB"), b"sRGB - Texture");
    assert_eq!(find("very approx. standard RGB"), b"");
    assert_eq!(find("ACES cct"), b"ACEScct");
    assert_eq!(find("requires ACES cct"), b"ACEScct");
    assert_eq!(find("ACES cg"), b"ACEScg");
    assert_eq!(find("ACES cg display"), b"");
    assert_eq!(find("pq display"), b"Rec.2100-PQ - Display");
    assert_eq!(find("pq display scene"), b"");

    // Validate that the fingerprints for the smallest likely gamuts are positive.
    let mut fingerprint_vals = Vec::new();
    assert!(
        !calc_color_space_fingerprint(
            &mut fingerprint_vals,
            &fingerprints,
            &input_config,
            input_config.color_space("Rec.601 - Display").unwrap(),
        )
        .unwrap()
    );
    for v in &fingerprint_vals {
        assert!(f64::from(*v) >= 0.);
    }

    assert!(
        !calc_color_space_fingerprint(
            &mut fingerprint_vals,
            &fingerprints,
            &input_config,
            input_config.color_space("Rec.601 (PAL) - Display").unwrap(),
        )
        .unwrap()
    );
    for v in &fingerprint_vals {
        assert!(f64::from(*v) >= 0.);
    }
}

/// Port of `OCIO_ADD_TEST(ConfigUtils, sanitize_id_token)` @ v2.6.0.
#[test]
fn sanitize_id_token_test() {
    let check = |token: &[u8], expected: &[u8]| {
        assert_eq!(
            sanitize_id_token(token),
            expected,
            "{}",
            String::from_utf8_lossy(token)
        );
    };

    // Lower-case allowed characters pass through unchanged.
    check(b"abc.-_~/*#%^+()[]|09", b"abc.-_~/*#%^+()[]|09");

    // Upper-case letters are lowered.
    check(b"ABCxyz", b"abcxyz");

    // Explicit character mappings.
    check(b" \t\n\r", b"____");
    check(b"{}<>", b"()()");
    check(b",", b".");
    check(b";:", b"||");
    check(b"'\"", b"##");
    check(b"\\", b"/");

    // Any other disallowed ASCII character becomes '*'.
    check(b"a!b@c?", b"a*b*c*");

    // A multi-byte UTF-8 codepoint becomes a single '^', regardless of how many bytes it takes
    // to encode, so the result does not depend on the input's byte-level UTF-8 encoding.
    check(b"a\xC3\xA9z", b"a^z"); // "a" + e acute (2 bytes) + "z"
    check(b"a\xE4\xB8\xADz", b"a^z"); // "a" + a CJK ideograph (3 bytes) + "z"
    check(b"a\xF0\x9F\x98\x80z", b"a^z"); // "a" + emoji (4 bytes) + "z"

    // Malformed UTF-8 (a stray continuation/lead byte not followed by the expected
    // continuation bytes) is still handled safely, one '^' per byte actually consumed.
    check(b"a\x80z", b"a^z");
    check(b"a\xC3z", b"a^z");

    // Empty string.
    check(b"", b"");
}

/// Port of `OCIO_ADD_TEST(ConfigUtils, generate_local_id_for_color_space)` @ v2.6.0.
#[test]
fn generate_local_id_for_color_space_test() {
    let _env = EnvGuard::new();
    let mut cfg = Config::new().unwrap();
    cfg.set_name("My Studio!");

    let mut cs = ColorSpace::new();
    cs.set_name("sRGB Encoded,Space");
    // This alias also requires sanitization (space, uppercase), so it does not qualify to be
    // preferred over the color space name; see the alias-preference test below for that case.
    cs.add_alias("My Alias");
    // The color space already has an interop ID, but generateLocalIDForColorSpace must ignore
    // it and always generate a local ID.
    cs.set_interop_id("lin_rec709_scene").unwrap();
    cfg.add_color_space(&cs).unwrap();

    // Config name and color space name are each sanitized per Annex C: uppercase is lowered,
    // spaces become '_', ',' becomes '.', and '!' (not allowed, no explicit mapping) becomes
    // '*'.
    let id = cfg
        .generate_local_id_for_color_space("sRGB Encoded,Space")
        .unwrap();
    assert_eq!(id, b"my_studio*:local:srgb_encoded.space");
    assert!(!id.windows(9).any(|w| w == b"populated"));
    // The color space's own interop ID is left untouched.
    assert_eq!(
        cfg.color_space("sRGB Encoded,Space").unwrap().interop_id(),
        b"lin_rec709_scene"
    );

    // An alias resolves to the same ID, since the canonical color space name is used, not the
    // caller-supplied string.
    let id = cfg.generate_local_id_for_color_space("My Alias").unwrap();
    assert_eq!(id, b"my_studio*:local:srgb_encoded.space");

    // A config with no name set cannot generate a local ID.
    let mut cfg_no_name = Config::new().unwrap();
    cfg_no_name.add_color_space(&cs).unwrap();
    check_throw_what(
        cfg_no_name.generate_local_id_for_color_space("sRGB Encoded,Space"),
        "May not generate a local interop ID if the config name is empty.",
    );

    // Null / empty color space name.
    check_throw_what(
        cfg.generate_local_id_for_color_space(""),
        "srcColorSpaceName must not be null or empty",
    );
    check_throw_what(
        cfg.generate_local_id_for_color_space(""),
        "srcColorSpaceName must not be null or empty",
    );

    // Color space that does not exist in the config.
    check_throw_what(
        cfg.generate_local_id_for_color_space("does_not_exist"),
        "does not contain the requested color space",
    );

    // Renaming the config is reflected immediately.
    cfg.set_name("Other Studio");
    let id = cfg
        .generate_local_id_for_color_space("sRGB Encoded,Space")
        .unwrap();
    assert_eq!(id, b"other_studio:local:srgb_encoded.space");

    // Disallow usage of config names for one of the OCIO configs for ACES that already contains
    // interop IDs for all color spaces. Trying to avoid the situation where someone edits one
    // of these configs but forgets to change the config name. This would result in a
    // meaningless interop ID.

    // A config named like one of the built-in configs at version 4.0.0 or higher already has
    // interop IDs populated, so it may not be used as the namespace of a locally-generated one.
    // Don't allow anything that would sanitize to one of the disallowed config names. Then an
    // example of a potential future built-in config name, and one with more digits in the ACES
    // version.
    for name in [
        "cg-config-v4.0.0_aces-v2.0_ocio-v2.5",
        "CG-config-v4.0.0_aces-v2.0_ocio-v2.5",
        "studio-config-v5.1.2_aces-v2.2_ocio-v2.7",
        "studio-config-v4.1.2_aces-v2.2.1_ocio-v2.7",
    ] {
        cfg.set_name(name);
        check_throw_what(
            cfg.generate_local_id_for_color_space("sRGB Encoded,Space"),
            "May not generate a local interop ID if the name matches",
        );
    }

    // A built-in-looking config name with a major version below 4 does not have interop IDs, so
    // it is still legal to use as a local ID namespace.
    cfg.set_name("studio-config-v3.0.0_aces-v2.0_ocio-v2.4");
    let id = cfg
        .generate_local_id_for_color_space("sRGB Encoded,Space")
        .unwrap();
    assert_eq!(
        id,
        b"studio-config-v3.0.0_aces-v2.0_ocio-v2.4:local:srgb_encoded.space"
    );

    // A name that starts like a built-in config and has a version 4.0.0+, but continues with
    // something other than the exact "_aces-<version>_ocio-<version>" suffix, is not actually
    // one of the reserved built-in config names and so remains legal.
    cfg.set_name("cg-config-v4.0.0_aces-v2.0_ocio-v2.5_custom");
    let id = cfg
        .generate_local_id_for_color_space("sRGB Encoded,Space")
        .unwrap();
    assert_eq!(
        id,
        b"cg-config-v4.0.0_aces-v2.0_ocio-v2.5_custom:local:srgb_encoded.space"
    );
}

/// Port of `OCIO_ADD_TEST(ConfigUtils, generate_local_id_for_color_space_alias_preference)` @
/// v2.6.0.
#[test]
fn generate_local_id_for_color_space_alias_preference() {
    let _env = EnvGuard::new();
    let mut cfg = Config::new().unwrap();
    cfg.set_name("studio");

    let mut add = |name: &str, aliases: &[&str]| {
        let mut cs = ColorSpace::new();
        cs.set_name(name);
        for alias in aliases {
            cs.add_alias(alias);
        }
        cfg.add_color_space(&cs).unwrap();
        cfg.generate_local_id_for_color_space(name).unwrap()
    };

    // If the color space name already requires no sanitization, it is used as-is, even though
    // an alias that also requires no sanitization is present.
    assert_eq!(
        add("aces_cg", &["some_other_name"]),
        b"studio:local:aces_cg"
    );

    // If the name requires sanitization but an alias does not, the alias is preferred.
    assert_eq!(
        add("ACES CG", &["aces_cg_alias"]),
        b"studio:local:aces_cg_alias"
    );

    // If several aliases would not require sanitization, the first one (in alias order) wins.
    assert_eq!(
        add(
            "Linear Rec709",
            &[
                "Needs Sanitizing",
                "first_clean_alias",
                "second_clean_alias"
            ]
        ),
        b"studio:local:first_clean_alias"
    );

    // If the name and all aliases require sanitization, fall back to the sanitized name, as
    // before this feature was added.
    assert_eq!(
        add("Camera Log,Space", &["Also Needs Sanitizing"]),
        b"studio:local:camera_log.space"
    );
}

/// Port of `OCIO_ADD_TEST(ConfigUtils, find_color_space_for_id)` @ v2.6.0.
#[test]
fn find_color_space_for_id_test() {
    let _env = EnvGuard::new();
    let mut cfg = Config::new().unwrap();
    cfg.set_name("MyStudio!");

    let mut acescg = ColorSpace::new();
    acescg.set_name("ACEScg");
    acescg.add_alias("lin_ap1");
    cfg.add_color_space(&acescg).unwrap();

    let mut srgb = ColorSpace::new();
    srgb.set_name("srgb");
    cfg.add_color_space(&srgb).unwrap();

    let mut foo = ColorSpace::new();
    foo.set_name("foo");
    cfg.add_color_space(&foo).unwrap();

    let mut fancy = ColorSpace::new();
    fancy.set_name("My Color Space!");
    fancy.add_alias("Fancy Alias");
    cfg.add_color_space(&fancy).unwrap();

    // A different name that sanitizes to the same base as the color space above (both '!' and
    // '?' sanitize to '*'). The color space added first wins.
    let mut fancy_too = ColorSpace::new();
    fancy_too.set_name("My Color Space?");
    cfg.add_color_space(&fancy_too).unwrap();

    let found = |cfg: &Config, id: &[u8]| -> Option<Vec<u8>> {
        cfg.find_color_space_for_id(id).map(|cs| cs.name().to_vec())
    };
    let name = |n: &str| Some(n.as_bytes().to_vec());

    // Mode 1: exact name and exact alias.
    assert_eq!(found(&cfg, b"ACEScg"), name("ACEScg"));
    assert_eq!(found(&cfg, b"lin_ap1"), name("ACEScg"));

    // Mode 2: one namespace, falls back to the base name.
    assert_eq!(found(&cfg, b"somestudio:acescg"), name("ACEScg"));

    // But if the namespaced name is there, it takes precedence.
    let mut nsname = ColorSpace::new();
    nsname.set_name("Namespaced name");
    nsname.add_alias("somestudio:acescg");
    cfg.add_color_space(&nsname).unwrap();
    assert_eq!(found(&cfg, b"somestudio:acescg"), name("Namespaced name"));

    // Two-namespace ID whose one-level-stripped remainder is not itself a valid name/alias: no
    // recursive stripping, so this must not match "ACEScg".
    assert_eq!(found(&cfg, b"outer:inner:acescg"), None);

    // Mode 3 (local): outer namespace matches this config's sanitized name.
    assert_eq!(found(&cfg, b"mystudio*:local:acescg"), name("ACEScg"));

    // Mode 3 negative: outer namespace does not match the config's name.
    assert_eq!(found(&cfg, b"othername:local:acescg"), None);
    assert_eq!(found(&cfg, b":local:acescg"), None);
    assert_eq!(found(&cfg, b"::local:acescg"), None);

    // Mode 3 with a color space whose name must be sanitized in order to match the ID base.
    // (The first of the two color spaces that sanitize to this base is the one returned.)
    assert_eq!(
        found(&cfg, b"mystudio*:local:my_color_space*"),
        name("My Color Space!")
    );

    // Aliases are searched as well, and are also compared in their sanitized form.
    assert_eq!(
        found(&cfg, b"mystudio*:local:fancy_alias"),
        name("My Color Space!")
    );

    // In this case, the outer namespace is illegal since it's not lower-case. However, the base
    // name would match regardless of case, so it's more predictable to allow this to match
    // too.
    assert_eq!(
        found(&cfg, b"myStudio*:local:fancy_Alias"),
        name("My Color Space!")
    );

    // The sanitized comparison is only used for the local mode fall-back, so the sanitized base
    // does not match on its own or with a namespace that is not this config's name.
    assert_eq!(found(&cfg, b"my_color_space*"), None);
    assert_eq!(found(&cfg, b"othername:local:my_color_space*"), None);

    // Empty inner namespace: "my-studio::srgb" strips to ":srgb", which must not match "srgb".
    assert_eq!(found(&cfg, b"my-studio::srgb"), None);

    // Use of "local" as an ordinary namespace is not allowed, it may only be used as a keyword
    // for the local (mode 3) form of an ID.
    assert_eq!(found(&cfg, b"local:foo"), None);

    // Garbage input: extra colons, empty string, and null all resolve to "not found" without
    // throwing.
    assert_eq!(found(&cfg, b"a:b:c:d"), None);
    assert_eq!(found(&cfg, b""), None);
    assert_eq!(found(&cfg, b""), None);

    // Round-trip: an ID generated by this config is resolved back to the same color space.
    let generated = cfg.generate_local_id_for_color_space("ACEScg").unwrap();
    assert_eq!(found(&cfg, &generated), name("ACEScg"));

    // Round-trip for a color space whose name requires more than case sanitizing.
    let generated_fancy = cfg
        .generate_local_id_for_color_space("My Color Space!")
        .unwrap();
    assert_eq!(generated_fancy, b"mystudio*:local:my_color_space*");
    assert_eq!(found(&cfg, &generated_fancy), name("My Color Space!"));

    // A config with no name never matches a local mode ID, not even one with an empty outer
    // namespace (such an ID could not have been generated by an unnamed config).
    let mut cfg_no_name = Config::new().unwrap();
    cfg_no_name.add_color_space(&foo).unwrap();
    assert_eq!(found(&cfg_no_name, b"foo"), name("foo"));
    assert_eq!(found(&cfg_no_name, b":local:foo"), None);
    assert_eq!(found(&cfg_no_name, b"mystudio*:local:foo"), None);
}
