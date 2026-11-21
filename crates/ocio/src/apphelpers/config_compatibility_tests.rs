// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/apphelpers/ConfigCompatibility_tests.cpp` @ v2.6.0.

use ocio_ops::open_color_types::{
    ConfigCompatibility, ReferenceSpaceType, TransformDirection, ViewTransformDirection,
};
use ocio_ops::parse_utils::ROLE_INTERCHANGE_DISPLAY;

use super::*;
use crate::color_space::ColorSpace;
use crate::named_transform::NamedTransform;
use crate::test_env::EnvGuard;
use crate::transform::Transform;
use crate::transforms::cdl_transform::CdlTransform;
use crate::transforms::matrix_transform::MatrixTransform;
use crate::view_transform::ViewTransform;

/// Port of `CreateDisplayColorSpace` (tests/cpu/apphelpers/ConfigCompatibility_tests.cpp:14-19
/// @ v2.6.0).
fn create_display_color_space(name: &str) -> ColorSpace {
    let mut cs = ColorSpace::with_reference_space(ReferenceSpaceType::Display);
    cs.set_name(name);
    cs
}

const HDR_26: ConfigCompatibility = ConfigCompatibility::HdrDisplaySupport26;

/// Port of `OCIO_ADD_TEST(ConfigCompatibility, hdr_display_support_26)` @ v2.6.0.
#[test]
fn hdr_display_support_26() {
    let _env = EnvGuard::new();
    let mut config = Config::new().unwrap();

    assert!(!check_compatibility(&config, HDR_26));

    // Add the role, but the referenced display color space has no interop ID or encoding.

    let mut interchange = create_display_color_space("CIE XYZ-D65");
    config.add_color_space(&interchange).unwrap();
    config
        .set_role(ROLE_INTERCHANGE_DISPLAY, Some("CIE XYZ-D65".as_bytes()))
        .unwrap();

    assert!(!display_color_spaces_have_attributes(&config));
    assert!(!check_compatibility(&config, HDR_26));

    // Give the interchange color space an interop ID, but it still has no encoding.

    interchange
        .set_interop_id("ocio:lin_ciexyzd65_display")
        .unwrap();
    config.add_color_space(&interchange).unwrap();

    assert!(!display_color_spaces_have_attributes(&config));
    assert!(!check_compatibility(&config, HDR_26));

    // Give the interchange color space an encoding as well. It is now the only display color
    // space in the config and it has both an interop ID and an encoding.

    interchange.set_encoding("display-linear");
    config.add_color_space(&interchange).unwrap();

    assert!(display_color_spaces_have_attributes(&config));
    assert!(!check_compatibility(&config, HDR_26));

    // Add another display color space without an interop ID or encoding. Since not all display
    // color spaces satisfy the requirement anymore, the check fails again.

    let mut dcs = create_display_color_space("display_cs");
    config.add_color_space(&dcs).unwrap();

    assert!(!display_color_spaces_have_attributes(&config));
    assert!(!check_compatibility(&config, HDR_26));

    // Making it inactive does not help, since inactive display color spaces are also checked.

    config.set_inactive_color_spaces("display_cs");

    assert!(!display_color_spaces_have_attributes(&config));
    assert!(!check_compatibility(&config, HDR_26));

    // Giving it an interop ID and encoding allows it to pass.

    dcs.set_interop_id("foo:display_cs").unwrap();
    dcs.set_encoding("sdr-video");
    config.add_color_space(&dcs).unwrap();

    assert!(display_color_spaces_have_attributes(&config));
    assert!(!check_compatibility(&config, HDR_26));

    // Add a view that does not use a view transform and whose color space is not a data color
    // space. A view transform is required unless the color space is a data color space, so the
    // check fails.

    let mut scs = ColorSpace::new();
    scs.set_name("scene_cs");
    config.add_color_space(&scs).unwrap();

    config
        .add_display_view("display", "view_no_vt", "scene_cs", "")
        .unwrap();

    assert!(!active_views_have_view_transform(&config));
    assert!(!check_compatibility(&config, HDR_26));

    // Mark scene_cs as a data color space. The view with no view transform is now exempt from
    // the requirement, but the check still fails since no view uses a view transform at all.

    scs.set_is_data(true);
    config.add_color_space(&scs).unwrap();

    assert!(!active_views_have_view_transform(&config));
    assert!(!check_compatibility(&config, HDR_26));

    // Add another view that does use a view transform. All views now satisfy the view
    // transform requirement, but the config is not yet compatible since the active display
    // "display" has no color space matching its name.

    let mut vt = ViewTransform::new(ReferenceSpaceType::Scene);
    vt.set_name("view_transform");
    let mut cdl = CdlTransform::new();
    cdl.set_sat(1.2);
    vt.set_transform(
        Some(&Transform::from(cdl)),
        ViewTransformDirection::FromReference,
    )
    .unwrap();
    config.add_view_transform(&vt).unwrap();

    config
        .add_display_view_with_view_transform(
            "display",
            "view_with_vt",
            "view_transform",
            "display_cs",
            "",
            "",
            "",
        )
        .unwrap();

    assert!(active_views_have_view_transform(&config));
    assert!(!active_displays_have_color_space(&config));
    assert!(!check_compatibility(&config, HDR_26));

    // Add a scene-referred color space matching the display's name. It must be
    // display-referred, so the check still fails.

    let mut scene_display_cs = ColorSpace::with_reference_space(ReferenceSpaceType::Scene);
    scene_display_cs.set_name("display");
    scene_display_cs.set_name("display");
    config.add_color_space(&scene_display_cs).unwrap();

    assert!(!active_displays_have_color_space(&config));
    assert!(!check_compatibility(&config, HDR_26));

    // Replace it with a display-referred color space matching the display's name (with an
    // interop ID and encoding, since all display color spaces must also satisfy
    // DisplayColorSpacesHaveAttributes). The config is now compatible.

    let mut display_cs = create_display_color_space("display");
    display_cs.set_interop_id("foo:pq_rec6000_display").unwrap();
    display_cs.set_encoding("hdr-video");
    config.add_color_space(&display_cs).unwrap();

    assert!(display_color_spaces_have_attributes(&config));
    assert!(active_displays_have_color_space(&config));
    assert!(check_compatibility(&config, HDR_26));

    // Add a second display that has no color space matching its name. Since it is active, the
    // check fails. Deactivating it via setActiveDisplays makes the config compatible again,
    // showing that an inactive display does not need to pass the check.

    config
        .add_display_view_with_view_transform(
            "display2",
            "view",
            "view_transform",
            "display_cs",
            "",
            "",
            "",
        )
        .unwrap();

    assert!(!active_displays_have_color_space(&config));
    assert!(!check_compatibility(&config, HDR_26));

    config.set_active_displays("display").unwrap();

    assert!(active_displays_have_color_space(&config));
    assert!(check_compatibility(&config, HDR_26));

    // Add a view that uses a named transform as its view_transform, the config is still
    // compatible.

    let mut nt = NamedTransform::new();
    nt.set_transform(
        Some(&Transform::from(MatrixTransform::new())),
        TransformDirection::Forward,
    )
    .unwrap();
    nt.set_name("namedtransform");
    config.add_named_transform(&nt).unwrap();
    config
        .add_display_view_with_view_transform(
            "display",
            "nt view",
            "namedtransform",
            "display_cs",
            "",
            "",
            "",
        )
        .unwrap();

    assert!(active_views_have_view_transform(&config));
    assert!(check_compatibility(&config, HDR_26));

    // Add a view with no view transform whose color space name refers to a named transform. It
    // is exempt from the view transform requirement, so the config is still compatible. It is
    // added to the active display since display2 is inactive.

    config
        .add_display_view("display", "nt_cs_view", "namedtransform", "")
        .unwrap();

    assert!(active_views_have_view_transform(&config));
    assert!(check_compatibility(&config, HDR_26));

    // Add a view that does not qualify (no view transform, non-data color space) to the active
    // display. Since it is active, the check fails. Deactivating it via setActiveViews makes
    // the config compatible again, showing that an inactive view does not need to pass the
    // check.

    config
        .add_display_view("display", "bad_view", "display_cs", "")
        .unwrap();

    assert!(!active_views_have_view_transform(&config));
    assert!(!check_compatibility(&config, HDR_26));

    config.set_active_views("view_with_vt").unwrap();

    assert!(active_views_have_view_transform(&config));
    assert!(check_compatibility(&config, HDR_26));
}
