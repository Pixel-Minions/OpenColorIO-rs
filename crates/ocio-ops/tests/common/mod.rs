// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Shared helpers of the integration tests. The oracle tests of the op renderers use the
//! oracle test battery (`ocio_testkit::battery`) instead.

#![allow(dead_code)] // Each test crate uses a subset.

pub(crate) mod image;
pub(crate) mod log_chain;
pub(crate) mod matrix;
pub(crate) mod numbers;
