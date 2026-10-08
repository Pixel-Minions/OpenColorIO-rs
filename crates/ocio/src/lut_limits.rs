// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The largest LUTs the readers accept: a port of `src/OpenColorIO/LutLimits.h` @ v2.5.2.

use std::ffi::c_ulong;

/// Maximum number of entries supported in a 1D LUT.
///
/// Port of `Max1DLUTLength` (LutLimits.h:11 @ v2.5.2).
pub const MAX_1D_LUT_LENGTH: c_ulong = 300000;

/// Maximum grid size supported for a 3D LUT. 129 allows for a MESH dimension of 7 in the 3dl
/// file format.
///
/// Port of `Max3DLUTLength` (LutLimits.h:15 @ v2.5.2).
pub const MAX_3D_LUT_LENGTH: c_ulong = 129;
