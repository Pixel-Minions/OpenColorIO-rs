// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The tests of `tests/cpu/transforms/CDLTransform_tests.cpp` @ v2.5.2 that read CDL files
//! (`CDLTransform::CreateFromFile`, `CreateGroupFromFile`). They are a test target of their
//! own, as `clear_caches` empties the global caches, which other tests of the library rely on.
//! `faulty_file_content` comes with the CTF reader (WP 4.5): its last case reads a CTF file.
//! Upstream mutes the warnings `create_from_cdl_file` expects (`MuteLogging`); here they go to
//! the default logging function.

use ocio::{CdlTransform, clear_all_caches};
use ocio_ops::open_color_types::CdlStyle;
use ocio_ops::platform::create_temp_filename;
use ocio_testkit::upstream::{check_equal, check_throw_what};

/// `OCIO::GetTestFilesDir()`: upstream's test files.
fn get_test_files_dir() -> String {
    let path = ocio_testkit::paths::upstream_dir().join("tests/data/files");
    let path = path.to_str().expect("a UTF-8 path").replace('\\', "/");
    path.trim_start_matches("//?/").to_owned()
}

/// A temporary file (without an extension), removed when the guard goes, which also clears
/// the caches.
///
/// Port of `FileGuard` (CDLTransform_tests.cpp:208-222 @ v2.5.2), with the process id added
/// to the name.
struct FileGuard {
    filename: String,
}

impl FileGuard {
    fn new() -> FileGuard {
        // The process id keeps the names of test processes that run at once apart (each
        // starts the same sequence of random numbers).
        let mut filename = create_temp_filename(b"");
        filename.extend_from_slice(format!("_{}", std::process::id()).as_bytes());
        FileGuard {
            filename: String::from_utf8(filename).expect("a UTF-8 path"),
        }
    }

    /// Writes `text` as a `std::fstream` opened for output (in text mode) writes it: each line
    /// feed as CR LF on Windows.
    fn write(&self, text: &str) {
        let bytes = if cfg!(windows) {
            text.replace('\n', "\r\n")
        } else {
            text.to_owned()
        };
        std::fs::write(&self.filename, bytes).unwrap();
    }
}

impl Drop for FileGuard {
    fn drop(&mut self) {
        // Even if not strictly required on most OSes, perform the cleanup.
        let _ = std::fs::remove_file(&self.filename);
        clear_all_caches();
    }
}

/// Port of `OCIO_ADD_TEST(CDLTransform, create_from_cc_file)` @ v2.5.2.
#[test]
fn create_from_cc_file() {
    let file_path = format!("{}/cdl_test1.cc", get_test_files_dir());

    {
        let transform = CdlTransform::from_file(&file_path, "").expect("OCIO_CHECK_NO_THROW");
        check_equal(transform.id(), b"foo".as_slice());
        check_equal(
            transform.first_sop_description(),
            b"this is a description".as_slice(),
        );
        check_equal(transform.style(), CdlStyle::NoClamp);
        let slope = transform.slope();
        check_equal(1.1, slope[0]);
        check_equal(1.2, slope[1]);
        check_equal(1.3, slope[2]);
        let offset = transform.offset();
        check_equal(2.1, offset[0]);
        check_equal(2.2, offset[1]);
        check_equal(2.3, offset[2]);
        let power = transform.power();
        check_equal(3.1, power[0]);
        check_equal(3.2, power[1]);
        check_equal(3.3, power[2]);
        check_equal(0.7, transform.sat());
    }

    {
        CdlTransform::from_file(&file_path, "foo").expect("OCIO_CHECK_NO_THROW");
    }

    {
        CdlTransform::from_file(&file_path, "0").expect("OCIO_CHECK_NO_THROW");
    }

    {
        // The cccid is case sensitive.
        check_throw_what(
            CdlTransform::from_file(&file_path, "FOO"),
            "The specified CDL Id/Index 'FOO' could not be loaded from the file",
        );
    }

    {
        let group = CdlTransform::group_from_file(&file_path).expect("OCIO_CHECK_NO_THROW");
        assert_eq!(group.num_transforms(), 1);
    }
}

/// Port of `OCIO_ADD_TEST(CDLTransform, create_from_ccc_file)` @ v2.5.2.
#[test]
fn create_from_ccc_file() {
    let file_path = format!("{}/cdl_test1.ccc", get_test_files_dir());
    {
        // Using ID.
        let transform = CdlTransform::from_file(&file_path, "cc0003").expect("OCIO_CHECK_NO_THROW");
        check_equal(transform.id(), b"cc0003".as_slice());
        check_equal(transform.style(), CdlStyle::NoClamp);

        check_equal(transform.first_sop_description(), b"golden".as_slice());

        let slope = transform.slope();
        check_equal(1.2, slope[0]);
        check_equal(1.1, slope[1]);
        check_equal(1.0, slope[2]);
        let offset = transform.offset();
        check_equal(0.0, offset[0]);
        check_equal(0.0, offset[1]);
        check_equal(0.0, offset[2]);
        let power = transform.power();
        check_equal(0.9, power[0]);
        check_equal(1.0, power[1]);
        check_equal(1.2, power[2]);
        check_equal(1.0, transform.sat());
    }
    {
        // Using 0 based index.
        let transform = CdlTransform::from_file(&file_path, "3").unwrap();
        check_equal(transform.id(), b"".as_slice());
        check_equal(transform.style(), CdlStyle::NoClamp);

        let slope = transform.slope();
        check_equal(4.0, slope[0]);
        check_equal(5.0, slope[1]);
        check_equal(6.0, slope[2]);
        let offset = transform.offset();
        check_equal(0.0, offset[0]);
        check_equal(0.0, offset[1]);
        check_equal(0.0, offset[2]);
        let power = transform.power();
        check_equal(0.9, power[0]);
        check_equal(1.0, power[1]);
        check_equal(1.2, power[2]);
        // `OCIO_CHECK_EQUAL(1.0f, transform->getSat())`: the float promotes to double.
        check_equal(f64::from(1.0f32), transform.sat());
    }
    {
        // No ID: return the first one.
        let transform = CdlTransform::from_file(&file_path, "").expect("OCIO_CHECK_NO_THROW");
        check_equal(transform.id(), b"cc0001".as_slice());
    }
    {
        let group = CdlTransform::group_from_file(&file_path).expect("OCIO_CHECK_NO_THROW");
        assert_eq!(group.num_transforms(), 5);
    }
    {
        // Wrong ID.
        check_throw_what(
            CdlTransform::from_file(&file_path, "NotFound"),
            "could not be loaded from the file",
        );
    }
    {
        // Wrong index.
        check_throw_what(
            CdlTransform::from_file(&file_path, "42"),
            "is outside the valid range for this file [0,4]",
        );
    }
}

/// Port of `OCIO_ADD_TEST(CDLTransform, create_from_cdl_file)` @ v2.5.2.
#[test]
fn create_from_cdl_file() {
    // Note: Detailed test is already done, this unit test only validates that
    // this CDL file (i.e. containing a ColorDecisionList) correctly loads
    // using a CDLTransform.

    let file_path = format!("{}/cdl_test1.cdl", get_test_files_dir());
    {
        let transform = CdlTransform::from_file(&file_path, "cc0003").expect("OCIO_CHECK_NO_THROW");
        check_equal(transform.id(), b"cc0003".as_slice());
        check_equal(transform.style(), CdlStyle::NoClamp);
    }
    {
        let group = CdlTransform::group_from_file(&file_path).expect("OCIO_CHECK_NO_THROW");
        assert_eq!(group.num_transforms(), 5);
    }
}

/// Port of `OCIO_ADD_TEST(CDLTransform, escape_xml)` @ v2.5.2.
#[test]
fn escape_xml() {
    let input_xml = "<ColorCorrection id=\"Esc &lt; &amp; &quot; &apos; &gt;\">\n\
                     \x20   <SOPNode>\n\
                     \x20       <Description>These: &lt; &amp; &quot; &apos; &gt; are escape chars</Description>\n\
                     \x20       <Slope>1.1 1.2 1.3</Slope>\n\
                     \x20       <Offset>2.1 2.2 2.3</Offset>\n\
                     \x20       <Power>3.1 3.2 3.3</Power>\n\
                     \x20   </SOPNode>\n\
                     \x20   <SatNode>\n\
                     \x20       <Saturation>0.7</Saturation>\n\
                     \x20   </SatNode>\n\
                     </ColorCorrection>";

    let guard = FileGuard::new();

    guard.write(input_xml);

    let transform = CdlTransform::from_file(&guard.filename, "").expect("OCIO_CHECK_NO_THROW");

    {
        let id_str = transform.id();
        check_equal(b"Esc < & \" ' >".as_slice(), id_str);
        check_equal(transform.style(), CdlStyle::NoClamp);

        let desc_str = transform.first_sop_description();
        check_equal(b"These: < & \" ' > are escape chars".as_slice(), desc_str);
    }
}

/// `kContentsA` (CDLTransform_tests.cpp:266-289 @ v2.5.2).
const CONTENTS_A: &str = "<ColorCorrectionCollection>\n\
    \x20   <ColorCorrection id=\"cc03343\">\n\
    \x20       <SOPNode>\n\
    \x20           <Slope>0.1 0.2 0.3 </Slope>\n\
    \x20           <Offset>0.8 0.1 0.3 </Offset>\n\
    \x20           <Power>0.5 0.5 0.5 </Power>\n\
    \x20       </SOPNode>\n\
    \x20       <SATNode>\n\
    \x20           <Saturation>1</Saturation>\n\
    \x20       </SATNode>\n\
    \x20   </ColorCorrection>\n\
    \x20   <ColorCorrection id=\"cc03344\">\n\
    \x20       <SOPNode>\n\
    \x20           <Slope>1.2 1.3 1.4 </Slope>\n\
    \x20           <Offset>0.3 0 0 </Offset>\n\
    \x20           <Power>0.75 0.75 0.75 </Power>\n\
    \x20       </SOPNode>\n\
    \x20       <SATNode>\n\
    \x20           <Saturation>1</Saturation>\n\
    \x20       </SATNode>\n\
    \x20   </ColorCorrection>\n\
    </ColorCorrectionCollection>\n";

/// `kContentsB` (CDLTransform_tests.cpp:291-314 @ v2.5.2).
const CONTENTS_B: &str = "<ColorCorrectionCollection>\n\
    \x20   <ColorCorrection id=\"cc03343\">\n\
    \x20       <SOPNode>\n\
    \x20           <Slope>1.1 2.2 3.3 </Slope>\n\
    \x20           <Offset>0.8 0.1 0.3 </Offset>\n\
    \x20           <Power>0.5 0.5 0.5 </Power>\n\
    \x20       </SOPNode>\n\
    \x20       <SATNode>\n\
    \x20           <Saturation>1</Saturation>\n\
    \x20       </SATNode>\n\
    \x20   </ColorCorrection>\n\
    \x20   <ColorCorrection id=\"cc03344\">\n\
    \x20       <SOPNode>\n\
    \x20           <Slope>1.2 1.3 1.4 </Slope>\n\
    \x20           <Offset>0.3 0 0 </Offset>\n\
    \x20           <Power>0.75 0.75 0.75 </Power>\n\
    \x20       </SOPNode>\n\
    \x20       <SATNode>\n\
    \x20           <Saturation>1</Saturation>\n\
    \x20       </SATNode>\n\
    \x20   </ColorCorrection>\n\
    </ColorCorrectionCollection>\n";

/// Port of `OCIO_ADD_TEST(CDLTransform, clear_caches)` @ v2.5.2.
#[test]
fn clear_caches() {
    let guard = FileGuard::new();

    guard.write(CONTENTS_A);

    let transform =
        CdlTransform::from_file(&guard.filename, "cc03343").expect("OCIO_CHECK_NO_THROW");
    let slope = transform.slope();
    check_equal(slope[0], 0.1);
    check_equal(slope[1], 0.2);
    check_equal(slope[2], 0.3);

    guard.write(CONTENTS_B);

    clear_all_caches();

    let transform =
        CdlTransform::from_file(&guard.filename, "cc03343").expect("OCIO_CHECK_NO_THROW");
    let slope = transform.slope();

    check_equal(slope[0], 1.1);
    check_equal(slope[1], 2.2);
    check_equal(slope[2], 3.3);
}
