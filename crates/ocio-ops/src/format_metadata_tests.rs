// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/fileformats/FormatMetadata_tests.cpp` @ v2.5.2.
//!
//! Upstream passes `const char *` arguments as string literals or `nullptr`; here they are
//! `c("...")` and `None`.

use super::*;
use ocio_testkit::upstream::check_throw_what;

/// A string literal passed as a `const char *`.
fn c(s: &str) -> Option<&[u8]> {
    Some(s.as_bytes())
}

/// Bytes as text, for readable comparisons (every string in these tests is ASCII).
fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).expect("ASCII")
}

/// `FormatMetadataImpl(name, value)`, which upstream's `emplace_back` calls.
fn element(name: &str, value: &str) -> FormatMetadataImpl {
    FormatMetadataImpl::new(name.as_bytes(), value.as_bytes()).unwrap()
}

/// Port of `OCIO_ADD_TEST(FormatMetadataImpl, test_accessors)` @ v2.5.2.
#[test]
fn test_accessors() {
    let mut info = FormatMetadataImpl::new(METADATA_INFO, b"").unwrap();
    assert_eq!(text(info.get_element_name()), text(METADATA_INFO));

    // Make sure that we can add attributes and that existing attributes will get
    // overwritten.
    info.add_attribute(c("version"), c("1.0")).unwrap();

    let atts1 = info.get_attributes();
    assert_eq!(atts1.len(), 1);
    assert_eq!(text(&atts1[0].0), "version");
    assert_eq!(text(&atts1[0].1), "1.0");

    info.add_attribute(c("version"), c("2.0")).unwrap();

    let atts2 = info.get_attributes();
    assert_eq!(atts2.len(), 1);
    assert_eq!(text(&atts2[0].0), "version");
    assert_eq!(text(&atts2[0].1), "2.0");

    info.get_children_elements_mut()
        .push(element("Copyright", "Copyright 2013 Autodesk"));
    info.get_children_elements_mut()
        .push(element("Release", "2015"));
    assert_eq!(info.get_children_elements().len(), 2);
    assert_eq!(
        text(info.get_children_elements()[0].get_element_name()),
        "Copyright"
    );
    assert_eq!(
        text(info.get_children_elements()[0].get_element_value()),
        "Copyright 2013 Autodesk"
    );
    assert_eq!(
        text(info.get_children_elements()[1].get_element_name()),
        "Release"
    );
    assert_eq!(
        text(info.get_children_elements()[1].get_element_value()),
        "2015"
    );

    // Add input color space metadata.
    info.get_children_elements_mut()
        .push(element("InputColorSpace", ""));
    let in_cs = info.get_children_elements_mut().last_mut().unwrap();
    // 2 elements can have the same name.
    in_cs.get_children_elements_mut().push(
        FormatMetadataImpl::new(METADATA_DESCRIPTION, b"Input color space description").unwrap(),
    );
    in_cs
        .get_children_elements_mut()
        .push(FormatMetadataImpl::new(METADATA_DESCRIPTION, b"Other description").unwrap());
    in_cs
        .get_children_elements_mut()
        .push(element("Profile", "Input color space profile"));
    assert_eq!(info.get_children_elements().len(), 3);
    assert_eq!(
        text(info.get_children_elements()[2].get_element_name()),
        "InputColorSpace"
    );
    assert_eq!(
        text(info.get_children_elements()[2].get_element_value()),
        ""
    );
    assert_eq!(
        info.get_children_elements()[2]
            .get_children_elements()
            .len(),
        3
    );
    assert_eq!(
        text(info.get_children_elements()[2].get_children_elements()[0].get_element_name()),
        text(METADATA_DESCRIPTION)
    );
    assert_eq!(
        text(info.get_children_elements()[2].get_children_elements()[0].get_element_value()),
        "Input color space description"
    );
    assert_eq!(
        text(info.get_children_elements()[2].get_children_elements()[1].get_element_name()),
        text(METADATA_DESCRIPTION)
    );
    assert_eq!(
        text(info.get_children_elements()[2].get_children_elements()[1].get_element_value()),
        "Other description"
    );
    assert_eq!(
        text(info.get_children_elements()[2].get_children_elements()[2].get_element_name()),
        "Profile"
    );
    assert_eq!(
        text(info.get_children_elements()[2].get_children_elements()[2].get_element_value()),
        "Input color space profile"
    );

    // Add output color space metadata.
    info.get_children_elements_mut()
        .push(element("OutputColorSpace", "Output Colors Space"));
    let out_cs = info.get_children_elements_mut().last_mut().unwrap();
    out_cs.get_children_elements_mut().push(
        FormatMetadataImpl::new(METADATA_DESCRIPTION, b"Output color space description").unwrap(),
    );
    out_cs
        .get_children_elements_mut()
        .push(element("Profile", "Output color space profile"));
    assert_eq!(info.get_children_elements().len(), 4);
    assert_eq!(
        text(info.get_children_elements()[3].get_element_name()),
        "OutputColorSpace"
    );
    assert_eq!(
        text(info.get_children_elements()[3].get_element_value()),
        "Output Colors Space"
    );
    assert_eq!(
        info.get_children_elements()[3]
            .get_children_elements()
            .len(),
        2
    );
    assert_eq!(
        text(info.get_children_elements()[3].get_children_elements()[0].get_element_name()),
        text(METADATA_DESCRIPTION)
    );
    assert_eq!(
        text(info.get_children_elements()[3].get_children_elements()[0].get_element_value()),
        "Output color space description"
    );
    assert_eq!(
        text(info.get_children_elements()[3].get_children_elements()[1].get_element_name()),
        "Profile"
    );
    assert_eq!(
        text(info.get_children_elements()[3].get_children_elements()[1].get_element_value()),
        "Output color space profile"
    );

    // Add category.
    // Assign value directly to the metadata element.
    info.get_children_elements_mut()
        .push(element("Category", ""));
    let cat = info.get_children_elements_mut().last_mut().unwrap();
    cat.get_children_elements_mut()
        .push(element("Name", "Color space category name"));
    cat.get_children_elements_mut()
        .push(element("Importance", "High"));

    // Note: This is a hypothetical example to test the class, it doesn't correspond to any
    // actual file format.
    //
    // <Info version="2.0">
    //     <Copyright>Copyright 2013 Autodesk</Copyright>
    //     <Release>2015</Release>
    //     <InputColorSpace>
    //         <Description>Input color space description</Description>
    //         <Description>Other description</Description>
    //         <Profile>Input color space profile</Profile>
    //     </InputColorSpace>
    //     <OutputColorSpace>
    //         Output Colors Space
    //         <Description>Output color space description</Description>
    //         <Profile>Output color space profile</Profile>
    //     </OutputColorSpace>
    //     <Category>
    //         <Name>Color space category name</Name>
    //         <Importance>High</Importance>
    //     </Category>
    // </Info>

    assert_eq!(info.get_children_elements().len(), 5);
    assert_eq!(
        text(info.get_children_elements()[4].get_element_name()),
        "Category"
    );
    assert_eq!(
        text(info.get_children_elements()[4].get_element_value()),
        ""
    );
    assert_eq!(
        info.get_children_elements()[4]
            .get_children_elements()
            .len(),
        2
    );
    assert_eq!(
        text(info.get_children_elements()[4].get_children_elements()[0].get_element_name()),
        "Name"
    );
    assert_eq!(
        text(info.get_children_elements()[4].get_children_elements()[0].get_element_value()),
        "Color space category name"
    );
    assert_eq!(
        text(info.get_children_elements()[4].get_children_elements()[1].get_element_name()),
        "Importance"
    );
    assert_eq!(
        text(info.get_children_elements()[4].get_children_elements()[1].get_element_value()),
        "High"
    );

    //
    // Do similar tests using only FormatMetadata public API interface.
    //
    info.clear();
    assert_eq!(text(info.get_element_name()), "Info");
    assert_eq!(text(info.get_element_value()), "");
    assert_eq!(info.get_num_attributes(), 0);
    assert_eq!(info.get_num_children_elements(), 0);

    info.add_attribute(c("version"), c("1.0")).unwrap();
    assert_eq!(info.get_num_attributes(), 1);
    assert_eq!(text(info.get_attribute_name(0)), "version");
    assert_eq!(text(info.get_attribute_value(0)), "1.0");

    info.add_attribute(c("version"), c("2.0")).unwrap();
    assert_eq!(info.get_num_attributes(), 1);
    assert_eq!(text(info.get_attribute_name(0)), "version");
    assert_eq!(text(info.get_attribute_value(0)), "2.0");

    info.add_child_element(c("Copyright"), c("Copyright 2013 Autodesk"))
        .unwrap();
    info.add_child_element(c("Release"), c("2015")).unwrap();

    assert_eq!(info.get_num_children_elements(), 2);
    let info0 = info.get_child_element(0).unwrap();
    assert_eq!(text(info0.get_element_name()), "Copyright");
    assert_eq!(text(info0.get_element_value()), "Copyright 2013 Autodesk");
    let info1 = info.get_child_element(1).unwrap();
    assert_eq!(text(info1.get_element_name()), "Release");
    assert_eq!(text(info1.get_element_value()), "2015");

    info.add_child_element(c("InputColorSpace"), c("")).unwrap();
    assert_eq!(info.get_num_children_elements(), 3);
    // 2 elements can have the same name.
    let ic_info = info.get_child_element_mut(2).unwrap();
    ic_info
        .add_child_element(
            Some(METADATA_DESCRIPTION),
            c("Input color space description"),
        )
        .unwrap();
    ic_info
        .add_child_element(Some(METADATA_DESCRIPTION), c("Other description"))
        .unwrap();
    ic_info
        .add_child_element(c("Profile"), c("Input color space profile"))
        .unwrap();

    assert_eq!(text(ic_info.get_element_name()), "InputColorSpace");
    assert_eq!(text(ic_info.get_element_value()), "");

    assert_eq!(ic_info.get_num_children_elements(), 3);
    assert_eq!(
        text(ic_info.get_child_element(0).unwrap().get_element_name()),
        text(METADATA_DESCRIPTION)
    );
    assert_eq!(
        text(ic_info.get_child_element(0).unwrap().get_element_value()),
        "Input color space description"
    );
    assert_eq!(
        text(ic_info.get_child_element(1).unwrap().get_element_name()),
        text(METADATA_DESCRIPTION)
    );
    assert_eq!(
        text(ic_info.get_child_element(1).unwrap().get_element_value()),
        "Other description"
    );
    assert_eq!(
        text(ic_info.get_child_element(2).unwrap().get_element_name()),
        "Profile"
    );
    assert_eq!(
        text(ic_info.get_child_element(2).unwrap().get_element_value()),
        "Input color space profile"
    );

    let mut oss = Vec::new();
    info.write_to(&mut oss);
    let expected_res = concat!(
        "<Info version=\"2.0\">",
        "<Copyright>Copyright 2013 Autodesk</Copyright>",
        "<Release>2015</Release>",
        "<InputColorSpace>",
        "<Description>Input color space description</Description>",
        "<Description>Other description</Description>",
        "<Profile>Input color space profile</Profile>",
        "</InputColorSpace></Info>"
    );
    assert_eq!(expected_res, text(&oss));

    // Rename tests.

    // Valid new name.
    info.set_element_name(c("TEST")).unwrap();
    assert_eq!(text(info.get_element_name()), "TEST");

    // Name can't be empty.
    check_throw_what(info.set_element_name(c("")), "has to have a non-empty name");
    check_throw_what(
        info.set_element_name(None),
        "FormatMetadata has to have a non-empty name",
    );

    // ROOT is reserved.
    check_throw_what(
        info.set_element_name(Some(METADATA_ROOT)),
        "'ROOT' is reversed for root FormatMetadata elements",
    );

    // Similar exceptions when adding a child element.

    check_throw_what(
        info.add_child_element(c(""), c("")),
        "has to have a non-empty name",
    );
    check_throw_what(
        info.add_child_element(None, c("")),
        "FormatMetadata has to have a non-empty name",
    );
    check_throw_what(
        info.add_child_element(Some(METADATA_ROOT), c("")),
        "'ROOT' is reversed for root FormatMetadata elements",
    );

    // Root element can't be renamed.
    let mut root = FormatMetadataImpl::root();
    assert_eq!(text(root.get_element_name()), text(METADATA_ROOT));
    check_throw_what(
        root.set_element_name(c("test")),
        "FormatMetadata 'ROOT' element can't be renamed",
    );

    // Attribute name must be non-empty.

    check_throw_what(
        root.add_attribute(c(""), c("test")),
        "Attribute must have a non-empty name",
    );
    check_throw_what(
        root.add_attribute(None, c("test")),
        "Attribute must have a non-empty name",
    );
}

/// Port of `OCIO_ADD_TEST(FormatMetadataImpl, helpers)` @ v2.5.2.
#[test]
fn helpers() {
    let mut root0 = FormatMetadataImpl::root();
    assert_eq!(text(root0.get_name()), "");
    assert_eq!(text(root0.get_id()), "");

    root0
        .add_attribute(Some(METADATA_NAME), c("root0"))
        .unwrap();
    root0.add_attribute(Some(METADATA_ID), c("ID0")).unwrap();

    assert_eq!(text(root0.get_name()), "root0");
    assert_eq!(text(root0.get_id()), "ID0");

    root0.set_name(c("root1"));
    root0.set_id(c("ID1"));

    assert_eq!(text(root0.get_name()), "root1");
    assert_eq!(text(root0.get_id()), "ID1");

    root0.set_name(c(""));
    root0.set_id(c(""));

    assert_eq!(text(root0.get_name()), "");
    assert_eq!(text(root0.get_id()), "");
}

/// Port of `OCIO_ADD_TEST(FormatMetadataImpl, combine)` @ v2.5.2.
#[test]
fn combine() {
    let mut root0 = FormatMetadataImpl::root();
    root0
        .add_attribute(Some(METADATA_NAME), c("root0"))
        .unwrap();
    root0.add_attribute(Some(METADATA_ID), c("ID0")).unwrap();
    root0.add_child_element(c("test0"), c("val0")).unwrap();
    let mut root1 = FormatMetadataImpl::root();
    root1
        .add_attribute(Some(METADATA_NAME), c("root1"))
        .unwrap();
    root1.add_attribute(Some(METADATA_ID), c("ID1")).unwrap();
    root1.add_child_element(c("test1"), c("val1")).unwrap();
    let sub1 = root1.get_child_element_mut(0).unwrap();
    sub1.add_child_element(c("sub1-test"), c("subval")).unwrap();

    root0.add_attribute(c("att0"), c("attval0")).unwrap();
    root0.add_attribute(c("att1"), c("attval1")).unwrap();
    root1.add_attribute(c("att1"), c("otherval")).unwrap();
    root1.add_attribute(c("att2"), c("attval2")).unwrap();
    // root0 is:
    // <ROOT name="root0" id="ID0" att0="attval0" att1="attval1">
    //     <test0>val0</test0>
    // </ROOT>
    //
    // root1 is:
    // <ROOT name="root1" id="ID1" att1="otherval" att2="attval2">
    //     <test1>val1
    //         <sub1-test>subval
    //         </sub1-test>
    //     </test1>
    // </ROOT>

    root0.combine(&root1).unwrap();

    // Now root0 is:
    // <ROOT name="root0 + root1" id="ID0 + ID1" att0="attval0" att1="attval1 + otherval" att2="attval2">
    //     <test0>val0</test0>
    //     <test1>val1
    //         <sub1-test>subval
    //         </sub1-test>
    //     </test1>
    // </ROOT>

    assert_eq!(root0.get_num_attributes(), 5);
    assert_eq!(root0.get_num_children_elements(), 2);

    assert_eq!(
        "test0",
        text(root0.get_children_elements()[0].get_element_name())
    );
    assert_eq!(
        "val0",
        text(root0.get_children_elements()[0].get_element_value())
    );

    assert_eq!(
        "test1",
        text(root0.get_children_elements()[1].get_element_name())
    );
    assert_eq!(
        "val1",
        text(root0.get_children_elements()[1].get_element_value())
    );
    // Sub elements are copied.
    assert_eq!(
        root0.get_children_elements()[1].get_num_children_elements(),
        1
    );

    assert_eq!(text(METADATA_NAME), text(root0.get_attribute_name(0)));
    // Name attributes are is combined.
    assert_eq!("root0 + root1", text(root0.get_attribute_value(0)));

    assert_eq!(text(METADATA_ID), text(root0.get_attribute_name(1)));
    // Id attributes are is combined.
    assert_eq!("ID0 + ID1", text(root0.get_attribute_value(1)));

    // Other attributes are added.
    assert_eq!("att0", text(root0.get_attribute_name(2)));
    assert_eq!("attval0", text(root0.get_attribute_value(2)));
    assert_eq!("att1", text(root0.get_attribute_name(3)));
    // Existing attribute values are combined.
    assert_eq!("attval1 + otherval", text(root0.get_attribute_value(3)));
    assert_eq!("att2", text(root0.get_attribute_name(4)));
    assert_eq!("attval2", text(root0.get_attribute_value(4)));

    let mut root2 = FormatMetadataImpl::root();
    root2
        .add_attribute(Some(METADATA_NAME), c("root2"))
        .unwrap();
    root2.add_child_element(c("test"), c("val2")).unwrap();
    let mut root3 = FormatMetadataImpl::root();
    root3.add_attribute(Some(METADATA_ID), c("ID3")).unwrap();
    root3.add_child_element(c("test"), c("val3")).unwrap();

    // root2 is:
    // <ROOT name="root2">
    // <test>val2</test>
    // </ROOT>
    //
    // root3 is:
    // <ROOT id="ID3">
    // <test>val3</test>
    // </ROOT>

    root2.combine(&root3).unwrap();

    // Now root2 is:
    // <ROOT name="root2" id="ID3">
    // <test>val2</test>
    // <test>val3</test>
    // </ROOT>

    assert_eq!(root2.get_num_attributes(), 2);
    assert_eq!(root2.get_num_children_elements(), 2);
    assert_eq!(text(METADATA_NAME), text(root2.get_attribute_name(0)));
    assert_eq!("root2", text(root2.get_attribute_value(0)));

    assert_eq!(text(METADATA_ID), text(root2.get_attribute_name(1)));
    assert_eq!("ID3", text(root2.get_attribute_value(1)));

    assert_eq!(
        "test",
        text(root2.get_children_elements()[0].get_element_name())
    );
    assert_eq!(
        "val2",
        text(root2.get_children_elements()[0].get_element_value())
    );

    assert_eq!(
        "test",
        text(root2.get_children_elements()[1].get_element_name())
    );
    assert_eq!(
        "val3",
        text(root2.get_children_elements()[1].get_element_value())
    );

    let mut metainfo = FormatMetadataImpl::new(METADATA_INFO, b"").unwrap();
    check_throw_what(
        metainfo.combine(&root3),
        "Only FormatMetadata with the same name",
    );
}
