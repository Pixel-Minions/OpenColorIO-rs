// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Ports of upstream's test support code (`tests/testutils/` @ v2.5.2), for ported upstream
//! tests only.
//!
//! This is the one place where a tolerance may appear, and only as upstream's own check wrote
//! it (CLAUDE.md rule 3). Each helper cites the upstream macro or function it ports.

pub mod math_utils;
pub mod unit_test;
