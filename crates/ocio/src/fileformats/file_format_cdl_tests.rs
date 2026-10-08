// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/fileformats/FileFormatCDL_tests.cpp` (@ v2.5.2). Upstream mutes the
//! warnings its tests expect (`MuteLogging`); here they go to the default logging function.

use ocio_testkit::assert_text_eq;
use ocio_testkit::upstream::{check_equal, check_throw_what};

use super::*;
use crate::fileformats::file_format_ccc::tests::{check_child, check_sop_sat};
use crate::fileformats::input_stream::OpenMode;
use crate::fileformats::test_utils::load_test_file;
use crate::transforms::cdl_transform::CdlTransform;
use crate::transforms::range_transform::RangeTransform;

/// Port of `LoadCDLFile` (FileFormatCDL_tests.cpp:41-45 @ v2.5.2).
fn load_cdl_file(file_name: &str) -> Result<LocalCachedFile> {
    load_test_file(file_name, OpenMode::Text, |istream, path| {
        LocalFileFormat.read_file(istream, path)
    })
}

/// Port of `OCIO_ADD_TEST(FileFormatCDL, test_cdl)` @ v2.5.2.
#[test]
fn test_cdl() {
    // CDL file
    let file_name = "cdl_test1.cdl";

    let cdl_file = load_cdl_file(file_name).expect("OCIO_CHECK_NO_THROW");

    // Check that Descriptive element children of <ColorDecisionList> are preserved.
    assert_eq!(cdl_file.metadata.get_num_children_elements(), 4);
    check_child(
        &cdl_file.metadata,
        0,
        "Description",
        "This is a color decision list example.",
    );
    check_child(
        &cdl_file.metadata,
        1,
        "InputDescription",
        "These should be applied in ACESproxy color space.",
    );
    check_child(
        &cdl_file.metadata,
        2,
        "ViewingDescription",
        "View using the ACES RRT+ODT transforms.",
    );
    check_child(
        &cdl_file.metadata,
        3,
        "Description",
        "It includes all possible description uses.",
    );

    assert_eq!(5, cdl_file.transform_vec.len());
    // Two of the five CDLs in the file don't have an id attribute and are not
    // included in the m_transformMap since it used the id as the key.
    assert_eq!(3, cdl_file.transform_map.len());
    {
        // Note: Descriptive elements that are children of <ColorDecision> are not preserved.

        let id_str = cdl_file.transform_vec[0].id();
        check_equal(b"cc0001".as_slice(), id_str);

        // Check that Descriptive element children of <ColorCorrection> are preserved.
        let format_metadata = cdl_file.transform_vec[0].format_metadata();
        assert_eq!(format_metadata.get_num_children_elements(), 6);
        check_child(format_metadata, 0, "Description", "CC-level description 1");
        check_child(
            format_metadata,
            1,
            "InputDescription",
            "CC-level input description 1",
        );
        check_child(
            format_metadata,
            2,
            "ViewingDescription",
            "CC-level viewing description 1",
        );
        // Check that Descriptive element children of SOPNode and SatNode are preserved.
        check_child(format_metadata, 3, "SOPDescription", "Example look");
        check_child(format_metadata, 4, "SOPDescription", "For scenes 1 and 2");
        check_child(format_metadata, 5, "SATDescription", "boosting sat");

        check_sop_sat(
            &cdl_file.transform_vec[0],
            [1.0, 1.0, 0.9],
            [-0.03, -0.02, 0.0],
            [1.25, 1.0, 1.0],
            1.7,
        );
    }
    {
        let id_str = cdl_file.transform_vec[1].id();
        check_equal(b"cc0002".as_slice(), id_str);

        let format_metadata = cdl_file.transform_vec[1].format_metadata();
        assert_eq!(format_metadata.get_num_children_elements(), 6);
        check_child(format_metadata, 0, "Description", "CC-level description 2");
        check_child(
            format_metadata,
            1,
            "InputDescription",
            "CC-level input description 2",
        );
        check_child(
            format_metadata,
            2,
            "ViewingDescription",
            "CC-level viewing description 2",
        );
        check_child(format_metadata, 3, "SOPDescription", "pastel");
        check_child(format_metadata, 4, "SOPDescription", "another example");
        check_child(format_metadata, 5, "SATDescription", "dropping sat");

        check_sop_sat(
            &cdl_file.transform_vec[1],
            [0.9, 0.7, 0.6],
            [0.1, 0.1, 0.1],
            [0.9, 0.9, 0.9],
            0.7,
        );
    }
    {
        let id_str = cdl_file.transform_vec[2].id();
        check_equal(b"cc0003".as_slice(), id_str);

        let format_metadata = cdl_file.transform_vec[2].format_metadata();
        assert_eq!(format_metadata.get_num_children_elements(), 6);
        check_child(format_metadata, 0, "Description", "CC-level description 3");
        check_child(
            format_metadata,
            1,
            "InputDescription",
            "CC-level input description 3",
        );
        check_child(
            format_metadata,
            2,
            "ViewingDescription",
            "CC-level viewing description 3",
        );
        check_child(format_metadata, 3, "SOPDescription", "golden");
        check_child(format_metadata, 4, "SATDescription", "no sat change");
        check_child(format_metadata, 5, "SATDescription", "sat==1");

        check_sop_sat(
            &cdl_file.transform_vec[2],
            [1.2, 1.1, 1.0],
            [0.0, 0.0, 0.0],
            [0.9, 1.0, 1.2],
            1.0,
        );
    }
    {
        let id_str = cdl_file.transform_vec[3].id();
        check_equal(b"".as_slice(), id_str);

        let format_metadata = cdl_file.transform_vec[3].format_metadata();
        assert_eq!(format_metadata.get_num_children_elements(), 0);

        // SatNode missing from XML, uses a default of 1.0.
        check_sop_sat(
            &cdl_file.transform_vec[3],
            [1.2, 1.1, 1.0],
            [0.0, 0.0, 0.0],
            [0.9, 1.0, 1.2],
            1.0,
        );
    }
    {
        let id_str = cdl_file.transform_vec[4].id();
        check_equal(b"".as_slice(), id_str);

        let format_metadata = cdl_file.transform_vec[4].format_metadata();
        assert_eq!(format_metadata.get_num_children_elements(), 0);

        // SOPNode missing from XML, uses default values.
        check_sop_sat(
            &cdl_file.transform_vec[4],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
            0.0,
        );
    }
}

// See also test: (CDLTransform, create_from_cdl_file).

/// Port of `OCIO_ADD_TEST(FileFormatCDL, write)` @ v2.5.2.
#[test]
fn write() {
    // Note that metadata in ColorDecisionList and in ColorCorrection are preserved, but not inside
    // ColorDecision.
    let file_path = format!(
        "{}/cdl_test1.cdl",
        crate::fileformats::test_utils::get_test_files_dir()
    );
    let group = CdlTransform::group_from_file(&file_path).expect("OCIO_CHECK_NO_THROW");

    let cfg = Config::create_raw().expect("a config");
    let mut oss = Vec::new();
    group
        .write(&cfg, FILEFORMAT_COLOR_DECISION_LIST, &mut oss)
        .expect("OCIO_CHECK_NO_THROW");
    const RESULT: &str = r#"<ColorDecisionList xmlns="urn:ASC:CDL:v1.01">
    <Description>This is a color decision list example.</Description>
    <Description>It includes all possible description uses.</Description>
    <InputDescription>These should be applied in ACESproxy color space.</InputDescription>
    <ViewingDescription>View using the ACES RRT+ODT transforms.</ViewingDescription>
    <ColorDecision>
        <ColorCorrection id="cc0001">
            <Description>CC-level description 1</Description>
            <InputDescription>CC-level input description 1</InputDescription>
            <ViewingDescription>CC-level viewing description 1</ViewingDescription>
            <SOPNode>
                <Description>Example look</Description>
                <Description>For scenes 1 and 2</Description>
                <Slope>1 1 0.9</Slope>
                <Offset>-0.03 -0.02 0</Offset>
                <Power>1.25 1 1</Power>
            </SOPNode>
            <SatNode>
                <Description>boosting sat</Description>
                <Saturation>1.7</Saturation>
            </SatNode>
        </ColorCorrection>
    </ColorDecision>
    <ColorDecision>
        <ColorCorrection id="cc0002">
            <Description>CC-level description 2</Description>
            <InputDescription>CC-level input description 2</InputDescription>
            <ViewingDescription>CC-level viewing description 2</ViewingDescription>
            <SOPNode>
                <Description>pastel</Description>
                <Description>another example</Description>
                <Slope>0.9 0.7 0.6</Slope>
                <Offset>0.1 0.1 0.1</Offset>
                <Power>0.9 0.9 0.9</Power>
            </SOPNode>
            <SatNode>
                <Description>dropping sat</Description>
                <Saturation>0.7</Saturation>
            </SatNode>
        </ColorCorrection>
    </ColorDecision>
    <ColorDecision>
        <ColorCorrection id="cc0003">
            <Description>CC-level description 3</Description>
            <InputDescription>CC-level input description 3</InputDescription>
            <ViewingDescription>CC-level viewing description 3</ViewingDescription>
            <SOPNode>
                <Description>golden</Description>
                <Slope>1.2 1.1 1</Slope>
                <Offset>0 0 0</Offset>
                <Power>0.9 1 1.2</Power>
            </SOPNode>
            <SatNode>
                <Description>no sat change</Description>
                <Description>sat==1</Description>
                <Saturation>1</Saturation>
            </SatNode>
        </ColorCorrection>
    </ColorDecision>
    <ColorDecision>
        <ColorCorrection>
            <SOPNode>
                <Slope>1.2 1.1 1</Slope>
                <Offset>0 0 0</Offset>
                <Power>0.9 1 1.2</Power>
            </SOPNode>
            <SatNode>
                <Saturation>1</Saturation>
            </SatNode>
        </ColorCorrection>
    </ColorDecision>
    <ColorDecision>
        <ColorCorrection>
            <SOPNode>
                <Slope>1 1 1</Slope>
                <Offset>0 0 0</Offset>
                <Power>1 1 1</Power>
            </SOPNode>
            <SatNode>
                <Saturation>0</Saturation>
            </SatNode>
        </ColorCorrection>
    </ColorDecision>
</ColorDecisionList>
"#;
    assert_text_eq("write", RESULT, std::str::from_utf8(&oss).expect("UTF-8"));

    // Write failures.

    oss.clear();

    // Empty group.
    let mut group = GroupTransform::new();
    check_throw_what(
        group.write(&cfg, FILEFORMAT_COLOR_DECISION_LIST, &mut oss),
        "there should be at least one CDL",
    );

    // Only CDL.
    group.append_transform(RangeTransform::new().into());
    check_throw_what(
        group.write(&cfg, FILEFORMAT_COLOR_DECISION_LIST, &mut oss),
        "only CDL can be written",
    );
}
