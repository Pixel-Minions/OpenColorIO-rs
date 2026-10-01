// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Ported `tests/cpu/ops/cdl/CDLOpData_tests.cpp` @ v2.5.2.

use super::*;
use crate::format_metadata::METADATA_DESCRIPTION;

/// `OCIO_CHECK_THROW_WHAT`: the call fails, and its message contains `what`.
#[track_caller]
fn check_throw_what<T: std::fmt::Debug>(result: Result<T>, what: &str) {
    let err = result.expect_err("an exception");
    assert!(
        err.message().contains(what),
        "{:?} doesn't contain {what:?}",
        err.message()
    );
}

/// `RangeOpDataRcPtr rg = DynamicPtrCast<RangeOpData>(op); OCIO_REQUIRE_ASSERT(rg)`.
fn as_range(op: OpData) -> RangeOpData {
    match op {
        OpData::Range(rg) => rg,
        other => panic!("not a range: {other:?}"),
    }
}

/// Port of `OCIO_ADD_TEST(CDLOpData, accessors)` @ v2.5.2.
#[test]
fn accessors() {
    let slope_params = ChannelParams::new(1.35, 1.1, 0.71);
    let offset_params = ChannelParams::new(0.05, -0.23, 0.11);
    let power_params = ChannelParams::new(0.93, 0.81, 1.27);

    let mut cdl_op = CdlOpData::new(
        CdlOpStyle::V1_2Fwd,
        slope_params,
        offset_params,
        power_params,
        1.23,
    )
    .unwrap();

    // Update slope parameters with the same value.
    let new_slope_params = ChannelParams::splat(0.66);
    cdl_op.set_slope_params(new_slope_params);

    assert!(*cdl_op.get_slope_params() == new_slope_params);
    assert!(*cdl_op.get_offset_params() == offset_params);
    assert!(*cdl_op.get_power_params() == power_params);
    assert_eq!(cdl_op.get_saturation(), 1.23);

    // Update offset parameters with the same value.
    let new_offset_params = ChannelParams::splat(0.09);
    cdl_op.set_offset_params(new_offset_params);

    assert!(*cdl_op.get_slope_params() == new_slope_params);
    assert!(*cdl_op.get_offset_params() == new_offset_params);
    assert!(*cdl_op.get_power_params() == power_params);
    assert_eq!(cdl_op.get_saturation(), 1.23);

    // Update power parameters with the same value.
    let new_power_params = ChannelParams::splat(1.1);
    cdl_op.set_power_params(new_power_params);

    assert!(*cdl_op.get_slope_params() == new_slope_params);
    assert!(*cdl_op.get_offset_params() == new_offset_params);
    assert!(*cdl_op.get_power_params() == new_power_params);
    assert_eq!(cdl_op.get_saturation(), 1.23);

    // Update the saturation parameter.
    cdl_op.set_saturation(0.99);

    assert!(*cdl_op.get_slope_params() == new_slope_params);
    assert!(*cdl_op.get_offset_params() == new_offset_params);
    assert!(*cdl_op.get_power_params() == new_power_params);
    assert_eq!(cdl_op.get_saturation(), 0.99);
}

/// Port of `OCIO_ADD_TEST(CDLOpData, constructors)` @ v2.5.2.
#[test]
fn constructors() {
    // Check default constructor.
    let cdl_op_default = CdlOpData::default();

    assert_eq!(cdl_op_default.get_type(), OpDataType::Cdl);

    assert_eq!(cdl_op_default.get_id(), b"");
    assert!(
        cdl_op_default
            .get_format_metadata()
            .get_children_elements()
            .is_empty()
    );

    assert_eq!(cdl_op_default.get_style(), CdlOpStyle::NoClampFwd);

    assert!(!cdl_op_default.is_reverse());

    assert!(*cdl_op_default.get_slope_params() == ChannelParams::splat(1.0));
    assert!(*cdl_op_default.get_offset_params() == ChannelParams::splat(0.0));
    assert!(*cdl_op_default.get_power_params() == ChannelParams::splat(1.0));
    assert_eq!(cdl_op_default.get_saturation(), 1.0);

    // Check complete constructor.
    let mut cdl_op_complete = CdlOpData::new(
        CdlOpStyle::NoClampRev,
        ChannelParams::new(1.35, 1.1, 0.71),
        ChannelParams::new(0.05, -0.23, 0.11),
        ChannelParams::new(0.93, 0.81, 1.27),
        1.23,
    )
    .unwrap();

    let metadata = cdl_op_complete.get_format_metadata_mut();
    metadata
        .add_attribute(Some(METADATA_NAME), Some(b"cdl-name"))
        .unwrap();
    metadata
        .add_attribute(Some(METADATA_ID), Some(b"cdl-id"))
        .unwrap();

    assert_eq!(cdl_op_complete.get_name(), b"cdl-name");
    assert_eq!(cdl_op_complete.get_id(), b"cdl-id");

    assert_eq!(cdl_op_complete.get_type(), OpDataType::Cdl);

    assert_eq!(cdl_op_complete.get_style(), CdlOpStyle::NoClampRev);

    assert!(cdl_op_complete.is_reverse());

    assert!(*cdl_op_complete.get_slope_params() == ChannelParams::new(1.35, 1.1, 0.71));
    assert!(*cdl_op_complete.get_offset_params() == ChannelParams::new(0.05, -0.23, 0.11));
    assert!(*cdl_op_complete.get_power_params() == ChannelParams::new(0.93, 0.81, 1.27));
    assert_eq!(cdl_op_complete.get_saturation(), 1.23);
}

/// Port of `OCIO_ADD_TEST(CDLOpData, inverse)` @ v2.5.2.
#[test]
fn inverse() {
    let mut cdl_op = CdlOpData::new(
        CdlOpStyle::V1_2Fwd,
        ChannelParams::new(1.35, 1.1, 0.71),
        ChannelParams::new(0.05, -0.23, 0.11),
        ChannelParams::new(0.93, 0.81, 1.27),
        1.23,
    )
    .unwrap();
    cdl_op
        .get_format_metadata_mut()
        .add_attribute(Some(METADATA_ID), Some(b"test_id"))
        .unwrap();
    cdl_op
        .get_format_metadata_mut()
        .add_child_element(
            Some(METADATA_DESCRIPTION),
            Some(b"Inverse op test description"),
        )
        .unwrap();

    let check_params = |inv_op: &CdlOpData| {
        // Ensure CDL parameters are unchanged
        assert!(*inv_op.get_slope_params() == ChannelParams::new(1.35, 1.1, 0.71));
        assert!(*inv_op.get_offset_params() == ChannelParams::new(0.05, -0.23, 0.11));
        assert!(*inv_op.get_power_params() == ChannelParams::new(0.93, 0.81, 1.27));
        assert_eq!(inv_op.get_saturation(), 1.23);
    };

    // Test CDL_V1_2_FWD inverse
    {
        cdl_op.set_style(CdlOpStyle::V1_2Fwd);
        let inv_op = cdl_op.inverse();

        // Ensure metadata is copied
        assert_eq!(inv_op.get_id(), b"test_id");
        let children = inv_op.get_format_metadata().get_children_elements();
        assert_eq!(children.len(), 1);
        assert_eq!(METADATA_DESCRIPTION, children[0].get_element_name());
        assert_eq!(
            b"Inverse op test description".as_slice(),
            children[0].get_element_value()
        );

        // Ensure style is inverted
        assert_eq!(inv_op.get_style(), CdlOpStyle::V1_2Rev);

        assert!(inv_op.is_reverse());

        check_params(&inv_op);
    }

    // Test CDL_V1_2_REV inverse
    {
        cdl_op.set_style(CdlOpStyle::V1_2Rev);
        let inv_op = cdl_op.inverse();

        // Ensure metadata is copied
        assert_eq!(inv_op.get_id(), b"test_id");
        assert_eq!(
            inv_op.get_format_metadata().get_children_elements().len(),
            1
        );

        // Ensure style is inverted
        assert_eq!(inv_op.get_style(), CdlOpStyle::V1_2Fwd);

        assert!(!inv_op.is_reverse());

        check_params(&inv_op);
    }

    // Test CDL_NO_CLAMP_FWD inverse
    {
        cdl_op.set_style(CdlOpStyle::NoClampFwd);
        let inv_op = cdl_op.inverse();

        // Ensure metadata is copied
        assert_eq!(inv_op.get_id(), b"test_id");
        assert_eq!(
            inv_op.get_format_metadata().get_children_elements().len(),
            1
        );

        // Ensure style is inverted
        assert_eq!(inv_op.get_style(), CdlOpStyle::NoClampRev);
        assert!(inv_op.is_reverse());

        check_params(&inv_op);
    }

    // Test CDL_NO_CLAMP_REV inverse
    {
        cdl_op.set_style(CdlOpStyle::NoClampRev);
        let inv_op = cdl_op.inverse();

        // Ensure metadata is copied
        assert_eq!(inv_op.get_id(), b"test_id");
        assert_eq!(
            inv_op.get_format_metadata().get_children_elements().len(),
            1
        );

        // Ensure style is inverted
        assert_eq!(inv_op.get_style(), CdlOpStyle::NoClampFwd);
        assert!(!inv_op.is_reverse());

        check_params(&inv_op);
    }
}

/// Port of `OCIO_ADD_TEST(CDLOpData, style)` @ v2.5.2.
#[test]
fn style() {
    // Check default constructor
    let mut cdl_op = CdlOpData::default();

    // Check CDL_V1_2_FWD

    cdl_op.set_style(CdlOpStyle::V1_2Fwd);
    assert_eq!(cdl_op.get_style(), CdlOpStyle::V1_2Fwd);
    assert!(!cdl_op.is_reverse());

    // Check the identity replacement.
    let rg = as_range(cdl_op.get_identity_replacement());
    assert!(rg.has_min_in_value() && (rg.get_min_in_value() == 0.));
    assert!(rg.has_max_in_value() && (rg.get_max_in_value() == 1.));
    assert!(rg.has_min_out_value() && (rg.get_min_out_value() == 0.));
    assert!(rg.has_max_out_value() && (rg.get_max_out_value() == 1.));
    assert!(!rg.scales());

    // Check CDL_V1_2_REV

    cdl_op.set_style(CdlOpStyle::V1_2Rev);
    assert_eq!(cdl_op.get_style(), CdlOpStyle::V1_2Rev);
    assert!(cdl_op.is_reverse());

    // Check the identity replacement.
    let rg = as_range(cdl_op.get_identity_replacement());
    assert!(rg.has_min_in_value() && (rg.get_min_in_value() == 0.));
    assert!(rg.has_max_in_value() && (rg.get_max_in_value() == 1.));
    assert!(rg.has_min_out_value() && (rg.get_min_out_value() == 0.));
    assert!(rg.has_max_out_value() && (rg.get_max_out_value() == 1.));
    assert!(!rg.scales());

    // Check CDL_NO_CLAMP_FWD

    cdl_op.set_style(CdlOpStyle::NoClampFwd);
    assert_eq!(cdl_op.get_style(), CdlOpStyle::NoClampFwd);
    assert!(!cdl_op.is_reverse());

    // Check the identity replacement.
    let OpData::Matrix(mtx) = cdl_op.get_identity_replacement() else {
        panic!("a matrix")
    };
    assert!(mtx.is_identity().unwrap());

    // Check CDL_NO_CLAMP_REV

    cdl_op.set_style(CdlOpStyle::NoClampRev);
    assert_eq!(cdl_op.get_style(), CdlOpStyle::NoClampRev);
    assert!(cdl_op.is_reverse());

    // Check the identity replacement.
    let OpData::Matrix(mtx) = cdl_op.get_identity_replacement() else {
        panic!("a matrix")
    };
    assert!(mtx.is_identity().unwrap());

    // Check unknown style

    check_throw_what(
        CdlOpData::get_style_from_name(Some("unknown_style")),
        "Unknown style for CDL",
    );
}

/// Port of `OCIO_ADD_TEST(CDLOpData, validation_success)` @ v2.5.2.
#[test]
fn validation_success() {
    let mut cdl_op = CdlOpData::default();

    // Set valid parameters
    let slope_params = ChannelParams::splat(1.15);
    let offset_params = ChannelParams::splat(-0.02);
    let power_params = ChannelParams::splat(0.97);

    cdl_op.set_style(CdlOpStyle::V1_2Fwd);

    cdl_op.set_slope_params(slope_params);
    cdl_op.set_offset_params(offset_params);
    cdl_op.set_power_params(power_params);
    cdl_op.set_saturation(1.22);

    assert!(!cdl_op.is_identity());
    assert!(!cdl_op.is_no_op());

    cdl_op.validate().unwrap();

    // Set an identity operation
    cdl_op.set_slope_params(K_ONE_PARAMS);
    cdl_op.set_offset_params(K_ZERO_PARAMS);
    cdl_op.set_power_params(K_ONE_PARAMS);
    cdl_op.set_saturation(1.0);

    assert!(cdl_op.is_identity());
    assert!(!cdl_op.is_no_op());
    // Set to non clamping
    cdl_op.set_style(CdlOpStyle::NoClampFwd);
    assert!(cdl_op.is_identity());
    assert!(cdl_op.is_no_op());

    cdl_op.validate().unwrap();

    // Check for slope = 0
    cdl_op.set_slope_params(ChannelParams::splat(0.0));
    cdl_op.set_offset_params(offset_params);
    cdl_op.set_power_params(power_params);
    cdl_op.set_saturation(1.0);

    cdl_op.set_style(CdlOpStyle::V1_2Fwd);

    assert!(!cdl_op.is_identity());
    assert!(!cdl_op.is_no_op());

    cdl_op.validate().unwrap();

    // Check for saturation = 0
    cdl_op.set_slope_params(slope_params);
    cdl_op.set_offset_params(offset_params);
    cdl_op.set_power_params(power_params);
    cdl_op.set_saturation(0.0);

    assert!(!cdl_op.is_identity());
    assert!(!cdl_op.is_no_op());

    cdl_op.validate().unwrap();
}

/// Port of `OCIO_ADD_TEST(CDLOpData, validation_failure)` @ v2.5.2.
#[test]
fn validation_failure() {
    let mut cdl_op = CdlOpData::default();

    // Fail: invalid scale
    cdl_op.set_slope_params(ChannelParams::splat(-0.9));
    cdl_op.set_offset_params(ChannelParams::splat(0.01));
    cdl_op.set_power_params(ChannelParams::splat(1.2));
    cdl_op.set_saturation(1.17);

    check_throw_what(cdl_op.validate(), "should be greater than 0");

    // Fail: invalid power
    cdl_op.set_slope_params(ChannelParams::splat(0.9));
    cdl_op.set_offset_params(ChannelParams::splat(0.01));
    cdl_op.set_power_params(ChannelParams::splat(-1.2));
    cdl_op.set_saturation(1.17);

    check_throw_what(cdl_op.validate(), "should be greater than 0");

    // Fail: invalid saturation
    cdl_op.set_slope_params(ChannelParams::splat(0.9));
    cdl_op.set_offset_params(ChannelParams::splat(0.01));
    cdl_op.set_power_params(ChannelParams::splat(1.2));
    cdl_op.set_saturation(-1.17);

    check_throw_what(cdl_op.validate(), "should be greater than 0");

    // Check for power = 0
    cdl_op.set_slope_params(ChannelParams::splat(0.7));
    cdl_op.set_offset_params(ChannelParams::splat(0.2));
    cdl_op.set_power_params(ChannelParams::splat(0.0));
    cdl_op.set_saturation(1.4);

    check_throw_what(cdl_op.validate(), "should be greater than 0");
}

/// Port of `OCIO_ADD_TEST(CDLOpData, channel)` @ v2.5.2.
#[test]
fn channel() {
    {
        let cdl_op = CdlOpData::default();

        // False: identity
        assert!(!cdl_op.has_channel_crosstalk());
    }

    {
        let mut cdl_op = CdlOpData::default();
        cdl_op.set_slope_params(ChannelParams::splat(-0.9));
        cdl_op.set_offset_params(ChannelParams::splat(0.01));
        cdl_op.set_power_params(ChannelParams::splat(1.2));

        // False: slope, offset, and power
        assert!(!cdl_op.has_channel_crosstalk());
    }

    {
        let mut cdl_op = CdlOpData::default();
        cdl_op.set_saturation(1.17);

        // True: saturation
        assert!(cdl_op.has_channel_crosstalk());
    }
}

// ---------------------------------------------------------------------------------------------
// The port's own checks.

/// Each style's CTF and CLF names read back as that style, in any ASCII case and up to a NUL,
/// and the CLF name is the one written (CDLOpData.cpp:27-80 @ v2.5.2); an empty or missing
/// name is unknown.
#[test]
fn style_names() {
    for style in [
        CdlOpStyle::V1_2Fwd,
        CdlOpStyle::V1_2Rev,
        CdlOpStyle::NoClampFwd,
        CdlOpStyle::NoClampRev,
    ] {
        let name = CdlOpData::get_style_name(style);
        for text in [
            name.to_string(),
            name.to_ascii_uppercase(),
            format!("{name}\0junk"),
        ] {
            assert_eq!(CdlOpData::get_style_from_name(Some(&text)).unwrap(), style);
        }
    }
    for (ctf, style) in [
        (V1_2_FWD_NAME, CdlOpStyle::V1_2Fwd),
        (V1_2_REV_NAME, CdlOpStyle::V1_2Rev),
        (NO_CLAMP_FWD_NAME, CdlOpStyle::NoClampFwd),
        (NO_CLAMP_REV_NAME, CdlOpStyle::NoClampRev),
    ] {
        assert_eq!(CdlOpData::get_style_from_name(Some(ctf)).unwrap(), style);
    }
    for missing in [None, Some(""), Some("\0Fwd")] {
        check_throw_what(
            CdlOpData::get_style_from_name(missing),
            "Unknown style for CDL.",
        );
    }
}

/// The transform's styles and directions combine into the op's styles, and back
/// (CDLOpData.cpp:83-125 @ v2.5.2); `setDirection` inverts the style only when it differs.
#[test]
fn styles_and_directions() {
    for style in [CdlStyle::Asc, CdlStyle::NoClamp] {
        let fwd = CdlOpData::convert_style(style, TransformDirection::Forward);
        let rev = CdlOpData::convert_style(style, TransformDirection::Inverse);
        assert_eq!(CdlOpData::convert_style_to_transform(fwd), style);
        assert_eq!(CdlOpData::convert_style_to_transform(rev), style);
        let mut cdl = CdlOpData::default();
        cdl.set_style(fwd);
        assert_eq!(cdl.get_direction(), TransformDirection::Forward);
        cdl.set_direction(TransformDirection::Forward);
        assert_eq!(cdl.get_style(), fwd);
        cdl.set_direction(TransformDirection::Inverse);
        assert_eq!(cdl.get_style(), rev);
        assert!(cdl.is_inverse(&cdl.inverse()) && !cdl.is_inverse(&cdl));
    }
    assert_eq!(CdlOpData::get_default_style(), CdlOpStyle::NoClampFwd);
    for i in 0..3 {
        assert!(ChannelParams::new(0.5, 1.5, 2.5).get(i).is_ok());
    }
    check_throw_what(ChannelParams::splat(1.0).get(3), "Index is out of range");
}
