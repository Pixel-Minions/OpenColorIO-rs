// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/fileformats/ctf/CTFTransform_tests.cpp` @ v2.6.0, its reader's tests
//! (`read_version`, `version_write`, `accessors`).

use ocio_ops::op_data::OpData;
use ocio_ops::ops::matrix::matrix_op_data::MatrixOpData;
use ocio_testkit::upstream::{check_equal, check_throw_what};

use super::*;

/// Port of `OCIO_ADD_TEST(CTFVersion, read_version)` @ v2.6.0.
#[test]
fn read_version() {
    {
        let version1 = CtfVersion::new(1, 2, 3);
        let version2 = CtfVersion::new(1, 2, 3);
        check_equal(&version1, &version2);
        {
            let version3 = CtfVersion::new(0, 0, 1);
            assert!(!(version1 == version3));
            assert!(version3 < version1);
        }
        {
            let version3 = CtfVersion::new(0, 1, 0);
            assert!(!(version1 == version3));
            assert!(version3 < version1);
        }
        {
            let version3 = CtfVersion::new(1, 0, 0);
            assert!(!(version1 == version3));
            assert!(version3 < version1);
        }
        {
            let version3 = CtfVersion::new(1, 2, 0);
            assert!(!(version1 == version3));
            assert!(version3 < version1);
        }
        {
            let version3 = CtfVersion::new(1, 2, 2);
            assert!(!(version1 == version3));
            assert!(version3 < version1);
        }
    }

    {
        let version_read = CtfVersion::read_version(b"1.2.3").expect("OCIO_CHECK_NO_THROW");
        let version = CtfVersion::new(1, 2, 3);
        check_equal(version, version_read);
    }
    {
        let version_read = CtfVersion::read_version(b"1.2").expect("OCIO_CHECK_NO_THROW");
        let version = CtfVersion::new(1, 2, 0);
        check_equal(version, version_read);
    }
    {
        let version_read = CtfVersion::read_version(b"1").expect("OCIO_CHECK_NO_THROW");
        let version = CtfVersion::new(1, 0, 0);
        check_equal(version, version_read);
    }
    {
        let version_read = CtfVersion::read_version(b"1.10").expect("OCIO_CHECK_NO_THROW");
        let version = CtfVersion::new(1, 10, 0);
        check_equal(version, version_read);
    }
    {
        let version_read = CtfVersion::read_version(b"1.1.0").expect("OCIO_CHECK_NO_THROW");
        let version = CtfVersion::new(1, 1, 0);
        check_equal(version, version_read);
    }
    {
        let version_read = CtfVersion::read_version(b"1.01").expect("OCIO_CHECK_NO_THROW");
        let version = CtfVersion::new(1, 1, 0);
        check_equal(version, version_read);
    }

    {
        // Numeric format is always accepted.
        let version_read =
            CtfVersion::read(b"2.0.0", StringFormat::VersionSmpteClf).expect("OCIO_CHECK_NO_THROW");
        let version = CtfVersion::new(2, 0, 0);
        check_equal(version, version_read);
    }

    {
        // Short SMPTE should be accepted only when the format is allowed.
        check_throw_what(
            CtfVersion::read_version(b"ST2136-1:2024"),
            "is not a valid version. Expecting MAJOR[.MINOR[.REVISION]]",
        );
    }

    {
        // Long SMPTE should be accepted only when the format is allowed.
        check_throw_what(
            CtfVersion::read_version(b"http://www.smpte-ra.org/ns/2136-1/2024"),
            "is not a valid version. Expecting MAJOR[.MINOR[.REVISION]]",
        );
    }

    {
        // SMPTE version is regarded as v3.0.0.
        let version_read = CtfVersion::read(b"ST2136-1:2024", StringFormat::VersionSmpteClf)
            .expect("OCIO_CHECK_NO_THROW");
        let version = CtfVersion::new(3, 0, 0);
        check_equal(version, version_read);
    }

    {
        // Short SMPTE string is not allowed when only the long format is accepted.
        check_throw_what(
            CtfVersion::read(b"ST2136-1:2024", StringFormat::VersionSmpteXmlns),
            "is not a valid version. Expecting 'http://www.smpte-ra.org/ns/2136-1/2024' or \
             MAJOR[.MINOR[.REVISION]]",
        );
    }

    {
        // Long SMPTE should be accepted only when the format is allowed and should be
        // regarded as v3.0.0.
        let version_read = CtfVersion::read(
            b"http://www.smpte-ra.org/ns/2136-1/2024",
            StringFormat::VersionSmpteXmlns,
        )
        .expect("OCIO_CHECK_NO_THROW");
        let version = CtfVersion::new(3, 0, 0);
        check_equal(version, version_read);
    }

    {
        // Long SMPTE string is not allowed when only the short format is accepted.
        check_throw_what(
            CtfVersion::read(
                b"http://www.smpte-ra.org/ns/2136-1/2024",
                StringFormat::VersionSmpteClf,
            ),
            "is not a valid version. Expecting 'ST2136-1:2024' or MAJOR[.MINOR[.REVISION]]",
        );
    }

    check_throw_what(CtfVersion::read_version(b""), "is not a valid version");
    check_throw_what(CtfVersion::read_version(b"1 2"), "is not a valid version");
    check_throw_what(CtfVersion::read_version(b"1-2"), "is not a valid version");
    check_throw_what(CtfVersion::read_version(b"a"), "is not a valid version");
    check_throw_what(CtfVersion::read_version(b"1."), "is not a valid version");
    check_throw_what(CtfVersion::read_version(b".2"), "is not a valid version");
    check_throw_what(CtfVersion::read_version(b"1.0 2"), "is not a valid version");
    check_throw_what(CtfVersion::read_version(b"-1"), "is not a valid version");
}

/// Port of `OCIO_ADD_TEST(CTFVersion, version_write)` @ v2.6.0.
#[test]
fn version_write() {
    for (major, minor, revision, expected) in [
        (1, 2, 3, "1.2.3"),
        (1, 0, 3, "1.0.3"),
        (1, 2, 0, "1.2"),
        (1, 20, 0, "1.20"),
        (1, 0, 0, "1"),
        (0, 0, 0, "0"),
    ] {
        let version = CtfVersion::new(major, minor, revision);
        let ostream = version.to_string();
        assert_eq!(ostream, expected);
    }
    {
        let version = CtfVersion::read(b"ST2136-1:2024", StringFormat::VersionSmpteClf)
            .expect("the constructor doesn't throw");
        let ostream = version.to_string();
        check_equal(ostream.as_str(), "ST2136-1:2024");
    }
    {
        let version = CtfVersion::read(
            b"http://www.smpte-ra.org/ns/2136-1/2024",
            StringFormat::VersionSmpteXmlns,
        )
        .expect("the constructor doesn't throw");
        let ostream = version.to_string();
        check_equal(ostream.as_str(), "http://www.smpte-ra.org/ns/2136-1/2024");
    }
}

/// Port of `OCIO_ADD_TEST(CTFReaderTransform, accessors)` @ v2.6.0. Upstream's `meta` and
/// `cmeta` are references to the info metadata, borrowed here at each use.
#[test]
fn accessors() {
    let mut t = CtfReaderTransform::new();
    {
        let info = t.get_info_metadata_mut().get_element_name().to_vec();
        let ct = &t;
        let cmeta = ct.get_info_metadata();
        let cinfo = ct.get_info_metadata();

        check_equal(info.as_slice(), METADATA_INFO);
        check_equal(cinfo.get_element_name(), METADATA_INFO);

        check_equal(ct.get_id(), b"".as_slice());
        check_equal(ct.get_name(), b"".as_slice());
        check_equal(
            cmeta.get_attribute_value_by_name(Some(b"inverseOf")),
            b"".as_slice(),
        );

        assert!(ct.get_op_data_vec().is_empty());

        check_equal(cmeta.get_num_children_elements(), 0);
    }
    t.set_name(b"Name").expect("a name");
    t.set_id(b"123").expect("a name");
    {
        let meta = t.get_info_metadata_mut();
        meta.add_attribute(Some(ATTR_INVERSE_OF), Some(b"654"))
            .expect("a name");
        meta.add_child_element(
            Some(METADATA_ID_ELEMENT),
            Some(b"urn:uuid:123e4567-e89b-12d3-a456-426655440000"),
        )
        .expect("a valid name");
    }

    let matrix_op = Arc::new(OpData::Matrix(MatrixOpData::new()));
    t.get_op_data_vec_mut().push(matrix_op);

    {
        let meta = t.get_info_metadata_mut();
        for (name, value) in [
            (METADATA_DESCRIPTION, b"One".as_slice()),
            (METADATA_DESCRIPTION, b"Two"),
            (METADATA_INPUT_DESCRIPTOR, b"input 1"),
            (METADATA_INPUT_DESCRIPTOR, b"input 2"),
            (METADATA_OUTPUT_DESCRIPTOR, b"output 1"),
            (METADATA_OUTPUT_DESCRIPTOR, b"output 2"),
        ] {
            meta.add_child_element(Some(name), Some(value))
                .expect("a valid name");
        }
        let last = meta.get_num_children_elements() - 1;
        let out_desc2 = meta.get_child_element_mut(last).expect("an index");
        out_desc2
            .add_attribute(Some(ATTR_LANGUAGE), Some(b"tr"))
            .expect("a name");
    }

    {
        let ct = &t;
        let cmeta = ct.get_info_metadata();

        check_equal(ct.get_id(), b"123".as_slice());
        check_equal(ct.get_name(), b"Name".as_slice());
        check_equal(
            cmeta.get_attribute_value_by_name(Some(b"inverseOf")),
            b"654".as_slice(),
        );

        check_equal(ct.get_op_data_vec().len(), 1);

        let meta = cmeta;
        assert_eq!(meta.get_num_children_elements(), 7, "OCIO_REQUIRE_EQUAL");
        let child = |i: i32| meta.get_child_element(i).expect("an index");
        for (i, name, value) in [
            (
                0,
                b"Id".as_slice(),
                b"urn:uuid:123e4567-e89b-12d3-a456-426655440000".as_slice(),
            ),
            (1, b"Description", b"One"),
            (2, b"Description", b"Two"),
            (3, b"InputDescriptor", b"input 1"),
            (4, b"InputDescriptor", b"input 2"),
            (5, b"OutputDescriptor", b"output 1"),
            (6, b"OutputDescriptor", b"output 2"),
        ] {
            check_equal(child(i).get_element_name(), name);
            check_equal(child(i).get_element_value(), value);
        }
        check_equal(
            child(6).get_attribute_value_by_name(Some(b"language")),
            b"tr".as_slice(),
        );
    }
}
