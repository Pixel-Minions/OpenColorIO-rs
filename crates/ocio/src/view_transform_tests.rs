// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the view transform: `tests/cpu/ViewTransform_tests.cpp` @ v2.5.2. The text is
//! compared with the wheel's in `tests/model_objects_oracle.rs`.

use super::*;

/// Port of `OCIO_ADD_TEST(ViewTransform, basic)` @ v2.5.2.
#[test]
fn basic() {
    let mut vt = ViewTransform::new(ReferenceSpaceType::Scene);
    assert_eq!(ReferenceSpaceType::Scene, vt.reference_space_type());
    assert_eq!(b"", vt.name());
    assert_eq!(b"", vt.family());
    assert_eq!(b"", vt.description());
    assert_eq!(0, vt.num_categories());
    assert_eq!(b"", vt.interchange_attribute("amf_transform_ids").unwrap());

    vt.set_name("name");
    assert_eq!(b"name", vt.name());
    vt.set_family("family");
    assert_eq!(b"family", vt.family());
    vt.set_description("description");
    assert_eq!(b"description", vt.description());
    vt.set_interchange_attribute("amf_transform_ids", "amf_text")
        .unwrap();
    assert_eq!(
        b"amf_text",
        vt.interchange_attribute("amf_transform_ids").unwrap()
    );
    assert_eq!(1, vt.interchange_attributes().len());

    assert_eq!(vt.num_categories(), 0);

    assert!(!vt.has_category("linear"));
    assert!(!vt.has_category("rendering"));
    assert!(!vt.has_category("log"));

    vt.add_category("linear");
    vt.add_category("rendering");
    assert_eq!(vt.num_categories(), 2);

    assert!(vt.has_category("linear"));
    assert!(vt.has_category("rendering"));
    assert!(!vt.has_category("log"));

    assert_eq!(vt.category(0), Some(&b"linear"[..]));
    assert_eq!(vt.category(1), Some(&b"rendering"[..]));
    // Check with an invalid index.
    assert!(vt.category(2).is_none());

    vt.remove_category("linear");
    assert_eq!(vt.num_categories(), 1);
    assert!(!vt.has_category("linear"));
    assert!(vt.has_category("rendering"));
    assert!(!vt.has_category("log"));

    // Remove a category not in the view transform.
    vt.remove_category("log");
    assert_eq!(vt.num_categories(), 1);
    assert!(vt.has_category("rendering"));

    vt.clear_categories();
    assert_eq!(vt.num_categories(), 0);

    let vtd = ViewTransform::new(ReferenceSpaceType::Display);
    assert_eq!(ReferenceSpaceType::Display, vtd.reference_space_type());
}
