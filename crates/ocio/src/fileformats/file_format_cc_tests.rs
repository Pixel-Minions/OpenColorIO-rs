// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/fileformats/FileFormatCC_tests.cpp` (@ v2.5.2). Upstream mutes the
//! warnings two tests expect (`MuteLogging`); here they go to the default logging function.

use ocio_testkit::assert_text_eq;
use ocio_testkit::upstream::check_equal;

use super::*;
use crate::fileformats::input_stream::OpenMode;
use crate::fileformats::test_utils::load_test_file;
use crate::transforms::file_format::FILEFORMAT_COLOR_CORRECTION;

/// Port of `LoadCCFile` (FileFormatCC_tests.cpp:16-20 @ v2.5.2).
fn load_cc_file(file_name: &str) -> Result<LocalCachedFile> {
    load_test_file(file_name, OpenMode::Text, |istream, path| {
        LocalFileFormat.read_file(istream, path)
    })
}

/// Port of `OCIO_ADD_TEST(FileFormatCC, test_cc1)` @ v2.5.2.
#[test]
fn test_cc1() {
    // CC file
    let file_name = "cdl_test1.cc";

    let cc_file = load_cc_file(file_name).expect("OCIO_CHECK_NO_THROW");

    check_equal(cc_file.transform.id(), b"foo".as_slice());
    check_equal(
        cc_file.transform.first_sop_description(),
        b"this is a description".as_slice(),
    );
    let slope = cc_file.transform.slope();
    check_equal(1.1, slope[0]);
    check_equal(1.2, slope[1]);
    check_equal(1.3, slope[2]);
    let offset = cc_file.transform.offset();
    check_equal(2.1, offset[0]);
    check_equal(2.2, offset[1]);
    check_equal(2.3, offset[2]);
    let power = cc_file.transform.power();
    check_equal(3.1, power[0]);
    check_equal(3.2, power[1]);
    check_equal(3.3, power[2]);
    check_equal(0.7, cc_file.transform.sat());
}

/// Port of `OCIO_ADD_TEST(FileFormatCC, test_cc2)` @ v2.5.2.
#[test]
fn test_cc2() {
    // CC file using windows eol.
    let file_name = "cdl_test2.cc";

    let cc_file = load_cc_file(file_name).expect("OCIO_CHECK_NO_THROW");

    // Access all using metadata.
    let format_metadata = cc_file.transform.format_metadata();
    check_equal(format_metadata.get_id(), b"cc0001".as_slice());
    assert_eq!(format_metadata.get_num_children_elements(), 2);
    check_equal(
        format_metadata
            .get_child_element(0)
            .expect("a child")
            .get_element_name(),
        b"SOPDescription".as_slice(),
    );
    check_equal(
        format_metadata
            .get_child_element(0)
            .expect("a child")
            .get_element_value(),
        b"Example look".as_slice(),
    );
    check_equal(
        format_metadata
            .get_child_element(1)
            .expect("a child")
            .get_element_name(),
        b"SATDescription".as_slice(),
    );
    check_equal(
        format_metadata
            .get_child_element(1)
            .expect("a child")
            .get_element_value(),
        b"boosting sat".as_slice(),
    );
    // Access using CDL transform helper functions (note that only the first SOP description is
    // available that way).
    check_equal(cc_file.transform.id(), b"cc0001".as_slice());
    check_equal(
        cc_file.transform.first_sop_description(),
        b"Example look".as_slice(),
    );

    let slope = cc_file.transform.slope();
    check_equal(1.0, slope[0]);
    check_equal(1.0, slope[1]);
    check_equal(0.9, slope[2]);
    let offset = cc_file.transform.offset();
    check_equal(-0.03, offset[0]);
    check_equal(-0.02, offset[1]);
    check_equal(0.0, offset[2]);
    let power = cc_file.transform.power();
    check_equal(1.25, power[0]);
    check_equal(1.0, power[1]);
    check_equal(1.0, power[2]);
    check_equal(1.7, cc_file.transform.sat());
}

/// Port of `OCIO_ADD_TEST(FileFormatCC, test_cc_sat_node)` @ v2.5.2.
#[test]
fn test_cc_sat_node() {
    // CC file
    let file_name = "cdl_test_SATNode.cc";

    let cc_file = load_cc_file(file_name).expect("OCIO_CHECK_NO_THROW");

    // "SATNode" is recognized.
    check_equal(0.42, cc_file.transform.sat());
}

/// Port of `OCIO_ADD_TEST(FileFormatCC, test_cc_asc_sat)` @ v2.5.2.
#[test]
fn test_cc_asc_sat() {
    // CC file
    let file_name = "cdl_test_ASC_SAT.cc";

    let cc_file = load_cc_file(file_name).expect("OCIO_CHECK_NO_THROW");

    // "ASC_SAT" is not recognized. Default value is returned.
    check_equal(1.0, cc_file.transform.sat());
}

/// Port of `OCIO_ADD_TEST(FileFormatCC, test_cc_asc_sop)` @ v2.5.2.
#[test]
fn test_cc_asc_sop() {
    // CC file
    let file_name = "cdl_test_ASC_SOP.cc";

    let cc_file = load_cc_file(file_name).expect("OCIO_CHECK_NO_THROW");

    // "ASC_SOP" is not recognized. Default values are used.
    let format_metadata = cc_file.transform.format_metadata();
    assert_eq!(format_metadata.get_num_children_elements(), 0);

    check_equal(cc_file.transform.id(), b"foo".as_slice());
    check_equal(cc_file.transform.first_sop_description(), b"".as_slice());

    let slope = cc_file.transform.slope();
    check_equal(1.0, slope[0]);
    let offset = cc_file.transform.offset();
    check_equal(0.0, offset[0]);
    let power = cc_file.transform.power();
    check_equal(1.0, power[0]);
}

/// Port of `OCIO_ADD_TEST(FileFormatCC, test_cc2_load_save)` @ v2.5.2.
#[test]
fn test_cc2_load_save() {
    let file_path = format!(
        "{}/cdl_test2.cc",
        crate::fileformats::test_utils::get_test_files_dir()
    );

    let group = CdlTransform::group_from_file(&file_path).expect("OCIO_CHECK_NO_THROW");

    let mut output_transform = Vec::new();
    let cfg = Config::create_raw().expect("a config");
    group
        .write(&cfg, FILEFORMAT_COLOR_CORRECTION, &mut output_transform)
        .expect("OCIO_CHECK_NO_THROW");
    let expected = r#"<ColorCorrection id="cc0001">
    <SOPNode>
        <Description>Example look</Description>
        <Slope>1 1 0.9</Slope>
        <Offset>-0.03 -0.02 0</Offset>
        <Power>1.25 1 1</Power>
    </SOPNode>
    <SatNode>
        <Description>boosting sat</Description>
        <Saturation>1.7</Saturation>
    </SatNode>
</ColorCorrection>
"#;
    assert_text_eq(
        "write",
        expected,
        std::str::from_utf8(&output_transform).expect("UTF-8"),
    );
}
