// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The allocation transform: a port of `src/OpenColorIO/transforms/AllocationTransform.cpp`
//! @ v2.5.2, with its op builder `BuildAllocationOp`.

use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{
    Allocation, TransformDirection, allocation_to_string, combine_transform_directions,
    transform_direction_to_string,
};
use ocio_ops::ops::allocation::AllocationData;
use ocio_ops::ops::allocation::allocation_op::create_allocation_ops;

use crate::transform::validate_direction;

/// How a color space's values are spread over the range the GPU's legacy 3D LUT samples: an
/// allocation (uniform or log2) and its variables (the range, and for log2 an offset).
///
/// A copy is upstream's `createEditableCopy`. Upstream gives the class no `equals`, no format
/// metadata and no file bit depths.
///
/// Port of `AllocationTransform` and its `Impl` (include/OpenColorIO/OpenColorTransforms.h:
/// 152-190, src/OpenColorIO/transforms/AllocationTransform.cpp:15-157 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct AllocationTransform {
    /// `m_dir`.
    dir: TransformDirection,
    /// `m_allocation`.
    allocation: Allocation,
    /// `m_vars`.
    vars: Vec<f32>,
}

impl Default for AllocationTransform {
    fn default() -> Self {
        Self::new()
    }
}

impl AllocationTransform {
    /// A forward uniform allocation without variables.
    ///
    /// Port of `AllocationTransform::Create` and `Impl::Impl` (AllocationTransform.cpp:15-35 @
    /// v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> AllocationTransform {
        AllocationTransform {
            dir: TransformDirection::Forward,
            allocation: Allocation::Uniform,
            vars: Vec::new(),
        }
    }

    /// Port of `AllocationTransform::getDirection` (AllocationTransform.cpp:75-78 @ v2.5.2).
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        self.dir
    }

    /// Port of `AllocationTransform::setDirection` (AllocationTransform.cpp:80-83 @ v2.5.2).
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.dir = dir;
    }

    /// Checks the direction ("AllocationTransform validation failed: " and its error), then the
    /// number of variables for the allocation: 0 or 2 for `uniform`, 0, 2 or 3 for `lg2`; an
    /// unknown allocation is an error.
    ///
    /// Port of `AllocationTransform::validate` (AllocationTransform.cpp:85-116 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        if let Err(ex) = validate_direction(self.dir) {
            return Err(Exception::new(format!(
                "AllocationTransform validation failed: {}",
                ex.message()
            )));
        }

        let n = self.vars.len();
        match self.allocation {
            Allocation::Uniform => {
                if n != 2 && n != 0 {
                    return Err(Exception::new(
                        "AllocationTransform: wrong number of values for the uniform allocation",
                    ));
                }
            }
            Allocation::Lg2 => {
                if n != 3 && n != 2 && n != 0 {
                    return Err(Exception::new(
                        "AllocationTransform: wrong number of values for the logarithmic \
                         allocation",
                    ));
                }
            }
            Allocation::Unknown => {
                return Err(Exception::new(
                    "AllocationTransform: invalid allocation type",
                ));
            }
        }
        Ok(())
    }

    /// Port of `AllocationTransform::getAllocation` (AllocationTransform.cpp:118-121 @ v2.5.2).
    #[doc(alias = "getAllocation")]
    pub fn allocation(&self) -> Allocation {
        self.allocation
    }

    /// Port of `AllocationTransform::setAllocation` (AllocationTransform.cpp:123-126 @ v2.5.2).
    #[doc(alias = "setAllocation")]
    pub fn set_allocation(&mut self, allocation: Allocation) {
        self.allocation = allocation;
    }

    /// The number of variables.
    ///
    /// Port of `AllocationTransform::getNumVars` (AllocationTransform.cpp:128-131 @ v2.5.2).
    #[doc(alias = "getNumVars")]
    pub fn num_vars(&self) -> i32 {
        self.vars.len() as i32
    }

    /// The variables.
    ///
    /// Port of `AllocationTransform::getVars` (AllocationTransform.cpp:133-141 @ v2.5.2), which
    /// copies them to the caller's array.
    #[doc(alias = "getVars")]
    pub fn vars(&self) -> &[f32] {
        &self.vars
    }

    /// Replaces the variables. Upstream's error for a null pointer with a non-zero count can't
    /// happen with a slice.
    ///
    /// Port of `AllocationTransform::setVars` (AllocationTransform.cpp:143-157 @ v2.5.2).
    #[doc(alias = "setVars")]
    pub fn set_vars(&mut self, vars: &[f32]) {
        self.vars = vars.to_vec();
    }

    /// Writes the transform's text to `os`: the allocation and the variables only when there
    /// are variables (docs/improvements.md, I-76), each with the stream's precision (6 on a new
    /// stream, 16 after a MatrixTransform in the same group, I-73).
    ///
    /// Port of `operator<<(std::ostream &, const AllocationTransform &)`
    /// (AllocationTransform.cpp:159-183 @ v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        let allocation = self.allocation();
        let vars = self.vars();

        os.put_str("<AllocationTransform ");
        os.put_str("direction=");
        os.put_str(transform_direction_to_string(self.direction()));
        if !vars.is_empty() {
            os.put_str(", allocation=");
            os.put_str(allocation_to_string(allocation));
            os.put_str(", ");
            os.put_str("vars=");
            os.put_f32(vars[0]);
            for &var in &vars[1..] {
                os.put_str(" ");
                os.put_f32(var);
            }
        }
        os.put_str(">");
    }
}

impl fmt::Display for AllocationTransform {
    /// `<AllocationTransform direction=<dir>`, then `, allocation=<name>, vars=<v> <v>...` when
    /// there are variables, then `>`.
    ///
    /// Port of `operator<<(std::ostream &, const AllocationTransform &)`
    /// (AllocationTransform.cpp:159-183 @ v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(os.str())
    }
}

/// Appends the ops of the allocation in the direction `dir` combined with the transform's
/// ([`create_allocation_ops`]: so far the `lg2` allocation is an error, until the Log op).
///
/// Port of `BuildAllocationOp` (src/OpenColorIO/transforms/AllocationTransform.cpp:189-205 @
/// v2.5.2).
pub(crate) fn build_allocation_op(
    ops: &mut OpVec,
    allocation_transform: &AllocationTransform,
    dir: TransformDirection,
) -> Result<()> {
    let combined_dir = combine_transform_directions(dir, allocation_transform.direction());

    let data = AllocationData {
        allocation: allocation_transform.allocation(),
        vars: allocation_transform.vars().to_vec(),
    };

    create_allocation_ops(ops, &data, combined_dir)
}

#[cfg(test)]
#[path = "allocation_transform_tests.rs"]
mod tests;
