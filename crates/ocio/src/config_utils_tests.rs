// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the reference-space converters and the color-space fingerprints:
//! `tests/cpu/ConfigUtils_tests.cpp` @ v2.5.2.

use ocio_testkit::upstream::check_close;

use super::*;
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
