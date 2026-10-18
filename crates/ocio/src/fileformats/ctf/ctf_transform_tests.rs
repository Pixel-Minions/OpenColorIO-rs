// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/fileformats/ctf/CTFTransform_tests.cpp` @ v2.5.2, its reader's tests
//! (`read_version`, `accessors`); `version_write` comes with the writer (4.6e).

use ocio_ops::op_data::OpData;
use ocio_ops::ops::matrix::matrix_op_data::MatrixOpData;
use ocio_testkit::upstream::{check_equal, check_throw_what};

use super::*;

/// Port of `OCIO_ADD_TEST(CTFVersion, read_version)` @ v2.5.2.
#[test]
fn read_version() {
    {
        let version1 = CtfVersion::new(1, 2, 3);
        let version2 = CtfVersion::new(1, 2, 3);
        check_equal(version1, version2);
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

    check_throw_what(CtfVersion::read_version(b""), "is not a valid version");
    check_throw_what(CtfVersion::read_version(b"1 2"), "is not a valid version");
    check_throw_what(CtfVersion::read_version(b"1-2"), "is not a valid version");
    check_throw_what(CtfVersion::read_version(b"a"), "is not a valid version");
    check_throw_what(CtfVersion::read_version(b"1."), "is not a valid version");
    check_throw_what(CtfVersion::read_version(b".2"), "is not a valid version");
    check_throw_what(CtfVersion::read_version(b"1.0 2"), "is not a valid version");
    check_throw_what(CtfVersion::read_version(b"-1"), "is not a valid version");
}

/// Port of `OCIO_ADD_TEST(CTFVersion, version_write)` @ v2.5.2.
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
}

/// Port of `OCIO_ADD_TEST(CTFReaderTransform, accessors)` @ v2.5.2.
#[test]
fn accessors() {
    let mut t = CtfReaderTransform::new();
    {
        let info = t.get_info_metadata_mut().get_element_name().to_vec();
        let ct = &t;
        let cinfo = ct.get_info_metadata();

        check_equal(info.as_slice(), METADATA_INFO);
        check_equal(cinfo.get_element_name(), METADATA_INFO);

        check_equal(ct.get_id(), b"".as_slice());
        check_equal(ct.get_name(), b"".as_slice());
        check_equal(ct.get_inverse_of_id(), b"".as_slice());
        check_equal(ct.get_input_descriptor(), b"".as_slice());
        check_equal(ct.get_output_descriptor(), b"".as_slice());

        assert!(ct.get_ops().is_empty());

        assert!(ct.get_descriptions().is_empty());
    }
    t.set_name(b"Name");
    t.set_id(b"123");
    t.set_inverse_of_id(b"654");
    t.set_input_descriptor(b"input");
    t.set_output_descriptor(b"output");

    let matrix_op = Arc::new(OpData::Matrix(MatrixOpData::new()));
    t.get_ops_mut().push(matrix_op);

    t.get_descriptions_mut().push(b"One".to_vec());
    t.get_descriptions_mut().push(b"Two".to_vec());

    {
        let ct = &t;

        check_equal(ct.get_id(), b"123".as_slice());
        check_equal(ct.get_name(), b"Name".as_slice());
        check_equal(ct.get_inverse_of_id(), b"654".as_slice());
        check_equal(ct.get_input_descriptor(), b"input".as_slice());
        check_equal(ct.get_output_descriptor(), b"output".as_slice());

        check_equal(ct.get_ops().len(), 1);

        check_equal(ct.get_descriptions().len(), 2);
        check_equal(ct.get_descriptions()[0].as_slice(), b"One".as_slice());
        check_equal(ct.get_descriptions()[1].as_slice(), b"Two".as_slice());
    }
}
