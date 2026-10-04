// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Allocation data: a port of `src/OpenColorIO/ops/allocation/AllocationOp.h` and
//! `AllocationOp.cpp` @ v2.5.2.
//!
//! `AllocationData`, which the `AllocationNoOp` carries
//! ([`crate::ops::noop::create_gpu_allocation_no_op`], where upstream defines
//! `CreateGpuAllocationNoOp`), and [`create_allocation_ops`], which builds Fit (Matrix) ops and,
//! for the `lg2` allocation, a Log op.

use std::fmt;

use crate::cfmt::{Crt, OStringStream};
use crate::exception::{Exception, Result};
use crate::op::OpVec;
use crate::open_color_types::{Allocation, TransformDirection, allocation_to_string};
use crate::ops::log::log_op::create_log_op_from_parameters;
use crate::ops::matrix::matrix_op::create_fit_op;

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

/// Appends the ops of `data` in the direction `dir`: for `uniform`, the fit from `[vars[0],
/// vars[1]]` (or `[0, 1]`) to `[0, 1]` on RGB; for `lg2`, a base-2 log (`vars[2]` its linear
/// offset) and the fit from `[vars[0], vars[1]]` (or `[-10, 6]`), in reverse order inverse;
/// "Unsupported Allocation Type." for an unknown one.
///
/// Port of `CreateAllocationOps` (src/OpenColorIO/ops/allocation/AllocationOp.cpp:37-117 @
/// v2.5.2).
pub fn create_allocation_ops(
    ops: &mut OpVec,
    data: &AllocationData,
    dir: TransformDirection,
) -> Result<()> {
    match data.allocation {
        Allocation::Uniform => {
            let mut oldmin = [0.0, 0.0, 0.0, 0.0];
            let mut oldmax = [1.0, 1.0, 1.0, 1.0];
            let newmin = [0.0, 0.0, 0.0, 0.0];
            let newmax = [1.0, 1.0, 1.0, 1.0];

            if data.vars.len() >= 2 {
                for i in 0..3 {
                    oldmin[i] = f64::from(data.vars[0]);
                    oldmax[i] = f64::from(data.vars[1]);
                }
            }

            create_fit_op(ops, &oldmin, &oldmax, &newmin, &newmax, dir)
        }
        Allocation::Lg2 => {
            let mut oldmin = [-10.0, -10.0, -10.0, 0.0];
            let mut oldmax = [6.0, 6.0, 6.0, 1.0];
            let newmin = [0.0, 0.0, 0.0, 0.0];
            let newmax = [1.0, 1.0, 1.0, 1.0];

            if data.vars.len() >= 2 {
                for i in 0..3 {
                    oldmin[i] = f64::from(data.vars[0]);
                    oldmax[i] = f64::from(data.vars[1]);
                }
            }

            // Log Settings.
            // output = logSlope * log( linSlope * input + linOffset, base ) + logOffset

            let base = 2.0;
            let log_slope = [1.0, 1.0, 1.0];
            let lin_slope = [1.0, 1.0, 1.0];
            let mut lin_offset = [0.0, 0.0, 0.0];
            let log_offset = [0.0, 0.0, 0.0];

            if data.vars.len() >= 3 {
                for value in &mut lin_offset {
                    *value = f64::from(data.vars[2]);
                }
            }

            match dir {
                TransformDirection::Forward => {
                    create_log_op_from_parameters(
                        ops,
                        base,
                        &log_slope,
                        &log_offset,
                        &lin_slope,
                        &lin_offset,
                        dir,
                    );
                    create_fit_op(ops, &oldmin, &oldmax, &newmin, &newmax, dir)
                }
                TransformDirection::Inverse => {
                    create_fit_op(ops, &oldmin, &oldmax, &newmin, &newmax, dir)?;
                    create_log_op_from_parameters(
                        ops,
                        base,
                        &log_slope,
                        &log_offset,
                        &lin_slope,
                        &lin_offset,
                        dir,
                    );
                    Ok(())
                }
            }
        }
        Allocation::Unknown => Err(Exception::new("Unsupported Allocation Type.")),
    }
}

#[cfg(test)]
#[path = "allocation_op_tests.rs"]
mod tests;
