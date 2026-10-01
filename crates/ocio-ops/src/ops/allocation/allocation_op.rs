// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Allocation data: a port of `src/OpenColorIO/ops/allocation/AllocationOp.h` and
//! `AllocationOp.cpp` @ v2.5.2.
//!
//! So far `AllocationData`, which the `AllocationNoOp` carries
//! ([`crate::ops::noop::create_gpu_allocation_no_op`], where upstream defines
//! `CreateGpuAllocationNoOp`). `CreateAllocationOps` builds Fit (Matrix) and Log ops; it comes
//! with the Matrix op's `CreateFitOp` and the Log op.

use std::fmt;

use crate::cfmt::{Crt, OStringStream};
use crate::open_color_types::{Allocation, allocation_to_string};

/// A color space's allocation and its variables: the range it spreads over (2 values, or a
/// third for `lg2`'s offset).
///
/// Port of `AllocationData` (src/OpenColorIO/ops/allocation/AllocationOp.h:14-24 @ v2.5.2).
#[derive(Debug, Clone, PartialEq)]
pub struct AllocationData {
    /// `allocation`.
    pub allocation: Allocation,
    /// `vars`.
    pub vars: Vec<f32>,
}

impl Default for AllocationData {
    /// A uniform allocation, with no variables.
    ///
    /// Port of `AllocationData::AllocationData()` (AllocationOp.h:19-21 @ v2.5.2).
    fn default() -> Self {
        AllocationData {
            allocation: Allocation::Uniform,
            vars: Vec::new(),
        }
    }
}

impl AllocationData {
    /// The allocation's name, then each variable with 7 significant digits, each followed
    /// by a space: `lg2 -8 8 `.
    ///
    /// Port of `AllocationData::getCacheID` (src/OpenColorIO/ops/allocation/AllocationOp.cpp:
    /// 15-29 @ v2.5.2).
    pub fn get_cache_id(&self) -> String {
        const FLOAT_DECIMALS: i64 = 7;

        let mut os = OStringStream::new(Crt::NATIVE);
        os.precision = FLOAT_DECIMALS;
        os.put_str(allocation_to_string(self.allocation));
        os.put_str(" ");

        for &var in &self.vars {
            os.put_f32(var);
            os.put_str(" ");
        }

        os.into_string()
    }
}

impl fmt::Display for AllocationData {
    /// The [cache ID](AllocationData::get_cache_id).
    ///
    /// Port of `operator<<(std::ostream &, const AllocationData &)`
    /// (src/OpenColorIO/ops/allocation/AllocationOp.cpp:31-35 @ v2.5.2).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.get_cache_id())
    }
}

#[cfg(test)]
#[path = "allocation_op_tests.rs"]
mod tests;
