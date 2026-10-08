// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! What the readers' ported tests share: a port of the file helpers of
//! `tests/cpu/UnitTestUtils.h` and `UnitTestUtils.cpp` @ v2.5.2.

use crate::fileformats::input_stream::{InputStream, OpenMode};
use ocio_ops::exception::{Exception, Result};

/// The directory of upstream's test files, with `/` separators (not a Windows verbatim path).
///
/// Port of `GetTestFilesDir` (tests/cpu/UnitTestUtils.h @ v2.5.2, `OCIO_UNIT_TEST_FILES_DIR`).
pub(crate) fn get_test_files_dir() -> String {
    let dir = ocio_testkit::paths::upstream_dir().join("tests/data/files");
    let dir = dir.to_str().expect("a UTF-8 path").replace('\\', "/");
    dir.trim_start_matches("//?/").to_owned()
}

/// Opens the test file `file_name` in `mode` and reads it with `read`, which takes the stream
/// and the file's path.
///
/// Port of `LoadTestFile` (tests/cpu/UnitTestUtils.h:47-72 @ v2.5.2), the format's reader
/// passed as `read`.
pub(crate) fn load_test_file<T>(
    file_name: &str,
    mode: OpenMode,
    read: impl FnOnce(&mut InputStream, &[u8]) -> Result<T>,
) -> Result<T> {
    let file_path = format!("{}/{file_name}", get_test_files_dir());

    // Open the filePath
    let mut filestream = InputStream::open_file(file_path.as_bytes(), mode);

    if filestream.fail() {
        return Err(Exception::new("Error opening test file."));
    }

    // Read file
    read(&mut filestream, file_path.as_bytes())
}
