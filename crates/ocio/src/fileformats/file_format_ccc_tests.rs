// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/fileformats/FileFormatCCC_tests.cpp` (@ v2.5.2).

use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::op_data::{OpData, OpDataType};
use ocio_ops::ops::cdl::cdl_op_data::CdlOpStyle;
use ocio_testkit::assert_text_eq;
use ocio_testkit::upstream::check_equal;

use super::*;
use crate::fileformats::input_stream::OpenMode;
use crate::fileformats::test_utils::load_test_file;

/// Port of `LoadCCCFile` (FileFormatCCC_tests.cpp:15-19 @ v2.5.2).
fn load_ccc_file(file_name: &str) -> Result<LocalCachedFile> {
    load_test_file(file_name, OpenMode::Text, |istream, path| {
        LocalFileFormat.read_file(istream, path)
    })
}

/// The checks of the name and value of the metadata's child `index`
/// (`OCIO_CHECK_EQUAL(std::string(m.getChildElement(i).getElementName()), name)` and its
/// value's).
pub(crate) fn check_child(metadata: &FormatMetadataImpl, index: i32, name: &str, value: &str) {
    let child = metadata.get_child_element(index).expect("a child");
    check_equal(child.get_element_name(), name.as_bytes());
    check_equal(child.get_element_value(), value.as_bytes());
}

/// `getSlope`, `getOffset`, `getPower` and `getSat` of a transform, against the expected
/// values.
pub(crate) fn check_sop_sat(
    cdl: &CdlTransform,
    slope: [f64; 3],
    offset: [f64; 3],
    power: [f64; 3],
    sat: f64,
) {
    let s = cdl.slope();
    check_equal(slope[0], s[0]);
    check_equal(slope[1], s[1]);
    check_equal(slope[2], s[2]);
    let o = cdl.offset();
    check_equal(offset[0], o[0]);
    check_equal(offset[1], o[1]);
    check_equal(offset[2], o[2]);
    let p = cdl.power();
    check_equal(power[0], p[0]);
    check_equal(power[1], p[1]);
    check_equal(power[2], p[2]);
    check_equal(sat, cdl.sat());
}

/// Port of `OCIO_ADD_TEST(FileFormatCCC, read)` @ v2.5.2.
#[test]
fn read() {
    // CCC file
    let file_name = "cdl_test1.ccc";

    let ccc_file = load_ccc_file(file_name).expect("OCIO_CHECK_NO_THROW");

    // Check that Descriptive element children of <ColorCorrectionCollection> are preserved.
    assert_eq!(ccc_file.metadata.get_num_children_elements(), 4);
    check_child(
        &ccc_file.metadata,
        0,
        "Description",
        "This is a color correction collection example.",
    );
    check_child(
        &ccc_file.metadata,
        1,
        "Description",
        "It includes all possible description uses.",
    );
    check_child(
        &ccc_file.metadata,
        2,
        "InputDescription",
        "These should be applied in ACESproxy color space.",
    );
    check_child(
        &ccc_file.metadata,
        3,
        "ViewingDescription",
        "View using the ACES RRT+ODT transforms.",
    );

    assert_eq!(5, ccc_file.transform_vec.len());
    // Two of the five CDLs in the file don't have an id attribute and are not
    // included in the transformMap since it used the id as the key.
    assert_eq!(3, ccc_file.transform_map.len());
    {
        let id_str = ccc_file.transform_vec[0].id();
        check_equal(b"cc0001".as_slice(), id_str);

        // Check that Descriptive element children of <ColorCorrection> are preserved.
        let format_metadata = ccc_file.transform_vec[0].format_metadata();
        assert_eq!(format_metadata.get_num_children_elements(), 7);
        check_child(format_metadata, 0, "Description", "CC-level description 1a");
        check_child(format_metadata, 1, "Description", "CC-level description 1b");
        check_child(
            format_metadata,
            2,
            "InputDescription",
            "CC-level input description 1",
        );
        check_child(
            format_metadata,
            3,
            "ViewingDescription",
            "CC-level viewing description 1",
        );
        // Check that Descriptive element children of SOPNode and SatNode are preserved.
        check_child(format_metadata, 4, "SOPDescription", "Example look");
        check_child(format_metadata, 5, "SOPDescription", "For scenes 1 and 2");
        check_child(format_metadata, 6, "SATDescription", "boosting sat");

        check_sop_sat(
            &ccc_file.transform_vec[0],
            [1.0, 1.0, 0.9],
            [-0.03, -0.02, 0.0],
            [1.25, 1.0, 1.0],
            1.7,
        );
    }
    {
        let id_str = ccc_file.transform_vec[1].id();
        check_equal(b"cc0002".as_slice(), id_str);

        let format_metadata = ccc_file.transform_vec[1].format_metadata();
        assert_eq!(format_metadata.get_num_children_elements(), 7);
        check_child(format_metadata, 0, "Description", "CC-level description 2a");
        check_child(format_metadata, 1, "Description", "CC-level description 2b");
        check_child(
            format_metadata,
            2,
            "InputDescription",
            "CC-level input description 2",
        );
        check_child(
            format_metadata,
            3,
            "ViewingDescription",
            "CC-level viewing description 2",
        );
        check_child(format_metadata, 4, "SOPDescription", "pastel");
        check_child(format_metadata, 5, "SOPDescription", "another example");
        check_child(format_metadata, 6, "SATDescription", "dropping sat");

        check_sop_sat(
            &ccc_file.transform_vec[1],
            [0.9, 0.7, 0.6],
            [0.1, 0.1, 0.1],
            [0.9, 0.9, 0.9],
            0.7,
        );
    }
    {
        let id_str = ccc_file.transform_vec[2].id();
        check_equal(b"cc0003".as_slice(), id_str);

        let format_metadata = ccc_file.transform_vec[2].format_metadata();
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
            &ccc_file.transform_vec[2],
            [1.2, 1.1, 1.0],
            [0.0, 0.0, 0.0],
            [0.9, 1.0, 1.2],
            1.0,
        );
    }
    {
        let id_str = ccc_file.transform_vec[3].id();
        check_equal(b"".as_slice(), id_str);

        let format_metadata = ccc_file.transform_vec[3].format_metadata();
        assert_eq!(format_metadata.get_num_children_elements(), 0);

        // SatNode missing from XML, uses a default of 1.0.
        check_sop_sat(
            &ccc_file.transform_vec[3],
            [4.0, 5.0, 6.0],
            [0.0, 0.0, 0.0],
            [0.9, 1.0, 1.2],
            1.0,
        );
    }
    {
        let id_str = ccc_file.transform_vec[4].id();
        check_equal(b"".as_slice(), id_str);

        let format_metadata = ccc_file.transform_vec[4].format_metadata();
        assert_eq!(format_metadata.get_num_children_elements(), 0);

        // SOPNode missing from XML, uses default values.
        check_sop_sat(
            &ccc_file.transform_vec[4],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
            0.0,
        );
    }

    let file_path = format!(
        "{}/cdl_test1.ccc",
        crate::fileformats::test_utils::get_test_files_dir()
    );

    // Create a FileTransform
    let mut file_transform = FileTransform::new();
    file_transform.set_direction(TransformDirection::Forward);
    file_transform.set_src(&file_path);
    file_transform.set_ccc_id("cc0002");

    // Create empty Config to use.
    let config = Config::new().expect("a config");

    let context = config.current_context().get();
    let tester = LocalFileFormat;
    let mut ops = OpVec::new();

    let ccc_file: CachedFileRcPtr = Arc::new(ccc_file);
    tester
        .build_file_ops(
            &mut ops,
            &config,
            &context,
            &ccc_file,
            &file_transform,
            TransformDirection::Forward,
        )
        .expect("buildFileOps");
    assert_eq!(ops.len(), 1);
    let op = &ops[0];
    // Check that the Descriptive element children of <ColorCorrection> are preserved.
    // Note that Descriptive element children of <ColorCorrectionCollection> are only
    // available in the CachedFile, not in the OpData.
    let data = op.data();
    let metadata = data.get_format_metadata();
    assert_eq!(metadata.get_num_children_elements(), 7);
    check_child(metadata, 0, "Description", "CC-level description 2a");
    check_child(metadata, 1, "Description", "CC-level description 2b");
    check_child(
        metadata,
        2,
        "InputDescription",
        "CC-level input description 2",
    );
    check_child(
        metadata,
        3,
        "ViewingDescription",
        "CC-level viewing description 2",
    );
    // Check that the Descriptive element children of SOPNode and SatNode are preserved.
    check_child(metadata, 4, "SOPDescription", "pastel");
    check_child(metadata, 5, "SOPDescription", "another example");
    check_child(metadata, 6, "SATDescription", "dropping sat");

    assert_eq!(data.get_type(), OpDataType::Cdl);
    let OpData::Cdl(cdl_data) = data.as_ref() else {
        panic!("a CDL");
    };
    assert_eq!(cdl_data.get_style(), CdlOpStyle::NoClampFwd);

    // Test with ASC style.
    file_transform.set_cdl_style(CdlStyle::Asc);

    ops.clear();
    tester
        .build_file_ops(
            &mut ops,
            &config,
            &context,
            &ccc_file,
            &file_transform,
            TransformDirection::Forward,
        )
        .expect("buildFileOps");
    assert_eq!(ops.len(), 1);
    let op = &ops[0];
    let data = op.data();
    assert_eq!(data.get_type(), OpDataType::Cdl);
    let OpData::Cdl(cdl_data) = data.as_ref() else {
        panic!("a CDL");
    };
    assert_eq!(cdl_data.get_style(), CdlOpStyle::V1_2Fwd);
}

// See also test: (CDLTransform, create_from_ccc_file).

/// Port of `OCIO_ADD_TEST(FileFormatCCC, write)` @ v2.5.2.
#[test]
fn write() {
    let file_path = format!(
        "{}/cdl_test1.ccc",
        crate::fileformats::test_utils::get_test_files_dir()
    );
    let group = CdlTransform::group_from_file(&file_path).expect("OCIO_CHECK_NO_THROW");

    let cfg = Config::create_raw().expect("a config");
    let mut oss = Vec::new();
    group
        .write(&cfg, FILEFORMAT_COLOR_CORRECTION_COLLECTION, &mut oss)
        .expect("OCIO_CHECK_NO_THROW");
    const RESULT: &str = r#"<ColorCorrectionCollection xmlns="urn:ASC:CDL:v1.01">
    <Description>This is a color correction collection example.</Description>
    <Description>It includes all possible description uses.</Description>
    <InputDescription>These should be applied in ACESproxy color space.</InputDescription>
    <ViewingDescription>View using the ACES RRT+ODT transforms.</ViewingDescription>
    <ColorCorrection id="cc0001">
        <Description>CC-level description 1a</Description>
        <Description>CC-level description 1b</Description>
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
    <ColorCorrection id="cc0002">
        <Description>CC-level description 2a</Description>
        <Description>CC-level description 2b</Description>
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
    <ColorCorrection>
        <SOPNode>
            <Slope>4 5 6</Slope>
            <Offset>0 0 0</Offset>
            <Power>0.9 1 1.2</Power>
        </SOPNode>
        <SatNode>
            <Saturation>1</Saturation>
        </SatNode>
    </ColorCorrection>
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
</ColorCorrectionCollection>
"#;
    assert_text_eq("write", RESULT, std::str::from_utf8(&oss).expect("UTF-8"));
}
