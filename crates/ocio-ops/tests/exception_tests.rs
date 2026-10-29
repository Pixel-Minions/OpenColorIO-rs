// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Exception_tests.cpp @ v2.5.2. Typed errors, Error trait and Clone represent
//! the established Rust API for upstream catch/base-class/copy observations.

use ocio_ops::{Exception, ExceptionKind};
use std::error::Error;

/// Port of `OCIO_ADD_TEST(Exception, basic)` @ v2.5.2.
#[test]
fn basic() {
    const MESSAGE: &str = "Dummy error";
    // Upstream Test0: receive the concrete OCIO error and inspect what().
    let error = Exception::new(MESSAGE);
    assert_eq!(error.what(), MESSAGE.as_bytes());
    assert_eq!(error.kind(), ExceptionKind::Exception);
    // Test1: the standard error interface preserves the same message.
    let error = Exception::new(MESSAGE);
    let standard: &dyn Error = &error;
    assert_eq!(standard.to_string(), MESSAGE);
    // Test2: copy while the source exists, then receive the copy after it dies.
    let copy = {
        let original = Exception::new(MESSAGE);
        let copy = original.clone();
        assert_eq!(original.what(), MESSAGE.as_bytes());
        copy
    };
    let received = copy;
    let standard: &dyn Error = &received;
    assert_eq!(standard.to_string(), MESSAGE);
}

/// Port of `OCIO_ADD_TEST(Exception, missing_file)` @ v2.5.2.
#[test]
fn missing_file() {
    const MESSAGE: &str = "Dummy error";
    let error = Exception::missing_file(MESSAGE);
    assert_eq!(error.kind(), ExceptionKind::MissingFile);
    let standard: &dyn Error = &error;
    assert_eq!(standard.to_string(), MESSAGE);
}
