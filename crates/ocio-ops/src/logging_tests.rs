// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/Logging_tests.cpp` @ v2.5.2. The wheel's side is in
//! `tests/logging_oracle.rs`.

use super::*;
use crate::unit_test_log_utils::{LogGuard, logging_test_lock};

/// The guard's output as text (every message in this test is ASCII).
fn output(guard: &LogGuard) -> String {
    String::from_utf8(guard.output()).expect("ASCII")
}

/// Port of `OCIO_ADD_TEST(Logging, message_function)` @ v2.5.2.
#[test]
fn message_function() {
    // Upstream's tests run one at a time: hold the logging state for the whole test, from
    // before the first check.
    let _lock = logging_test_lock();

    assert_eq!(get_logging_level(), LoggingLevel::DEFAULT);

    let dummy_str = "Dummy message";

    let guard = LogGuard::new();

    {
        set_logging_level(LoggingLevel::None);

        log_debug(dummy_str);
        assert!(guard.empty());

        log_info(dummy_str);
        assert!(guard.empty());

        log_warning(dummy_str);
        assert!(guard.empty());

        assert!(!is_debug_logging_enabled());
    }

    {
        set_logging_level(LoggingLevel::Warning);

        log_debug(dummy_str);
        assert!(guard.empty());

        log_info(dummy_str);
        assert!(guard.empty());

        log_warning(dummy_str);
        assert_eq!(output(&guard), "[OpenColorIO Warning]: Dummy message\n");
        guard.clear();

        log_warning("");
        assert_eq!(output(&guard), "[OpenColorIO Warning]: \n");
        guard.clear();

        assert!(!is_debug_logging_enabled());
    }

    {
        set_logging_level(LoggingLevel::Info);

        log_debug(dummy_str);
        assert!(guard.empty());

        log_info(dummy_str);
        assert_eq!(output(&guard), "[OpenColorIO Info]: Dummy message\n");
        guard.clear();

        log_warning(dummy_str);
        assert_eq!(output(&guard), "[OpenColorIO Warning]: Dummy message\n");
        guard.clear();

        assert!(!is_debug_logging_enabled());
    }

    {
        set_logging_level(LoggingLevel::Debug);

        log_debug(dummy_str);
        assert_eq!(output(&guard), "[OpenColorIO Debug]: Dummy message\n");
        guard.clear();

        log_info(dummy_str);
        assert_eq!(output(&guard), "[OpenColorIO Info]: Dummy message\n");
        guard.clear();

        log_warning(dummy_str);
        assert_eq!(output(&guard), "[OpenColorIO Warning]: Dummy message\n");
        guard.clear();

        assert!(is_debug_logging_enabled());
    }

    {
        set_logging_level(LoggingLevel::Unknown);

        log_debug(dummy_str);
        assert_eq!(output(&guard), "[OpenColorIO Debug]: Dummy message\n");
        guard.clear();

        log_info(dummy_str);
        assert_eq!(output(&guard), "[OpenColorIO Info]: Dummy message\n");
        guard.clear();

        log_warning(dummy_str);
        assert_eq!(output(&guard), "[OpenColorIO Warning]: Dummy message\n");
        guard.clear();

        assert!(is_debug_logging_enabled());
    }

    // Validate multi-line messages.

    {
        set_logging_level(LoggingLevel::Debug);

        log_debug("My first msg\nMy second msg\nMy third msg");
        assert_eq!(
            output(&guard),
            concat!(
                "[OpenColorIO Debug]: My first msg\n",
                "[OpenColorIO Debug]: My second msg\n",
                "[OpenColorIO Debug]: My third msg\n"
            )
        );
    }
}
