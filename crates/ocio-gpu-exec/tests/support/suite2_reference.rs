// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Reuse the verified static wheel transport, including full finite companions.
//! Test data paths match the pinned OCIO_UNIT_TEST_FILES_DIR.

use ocio_gpu_exec::gpu_unit_test::{GpuUnitTest, ReferenceRun};
use serde_json::Value;

#[path = "suite1_reference.rs"]
mod shared;

pub(super) fn batch(tests: &[GpuUnitTest], processors: &[Value]) -> Vec<ReferenceRun> {
    shared::batch(tests, processors)
}

pub(super) fn files_directory() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../upstream/OpenColorIO/tests/data/files")
        .canonicalize()
        .unwrap();
    assert!(path.is_absolute());
    assert!(path.is_dir());
    path.to_str().unwrap().to_owned()
}
