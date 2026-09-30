// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Allocations: how a color space's values are spread for the GPU's legacy 3D LUT. A port of
//! `src/OpenColorIO/ops/allocation/` @ v2.5.2.

pub mod allocation_op;

pub use allocation_op::AllocationData;
