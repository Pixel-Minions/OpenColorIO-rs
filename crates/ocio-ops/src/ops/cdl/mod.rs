// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CDL op family: a port of `src/OpenColorIO/ops/cdl/` @ v2.5.2.

pub mod cdl_op;
pub mod cdl_op_cpu;
#[cfg(target_arch = "x86_64")]
mod cdl_op_cpu_sse2;
pub mod cdl_op_data;

pub use cdl_op_data::{CdlOpData, CdlOpStyle, ChannelParams};
