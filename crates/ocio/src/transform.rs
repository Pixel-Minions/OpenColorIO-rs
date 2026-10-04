// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Transforms: a port of `src/OpenColorIO/Transform.cpp` @ v2.5.2 (the base `Transform`'s
//! validation, `BuildOps`, `operator<<` and `CreateTransform`) and of the `Transform` base class
//! (include/OpenColorIO/OpenColorTransforms.h), with the declarations of
//! `src/OpenColorIO/OpBuilders.h`.
//!
//! Upstream's `Transform` is a class hierarchy that `DynamicPtrCast` dispatches on. Here it is
//! [`Transform`], an enum with a variant per class (docs/architecture.md, "Public API"); each
//! dispatch below is an exhaustive `match`, so a new transform class adds its variant and its
//! arm in each: [`Transform::transform_type`], [`Transform::direction`],
//! [`Transform::set_direction`], [`Transform::validate`], `Display`, [`build_ops`] and, for its
//! op data, [`create_transform`].

use std::fmt;

use ocio_ops::exception::{Exception, Result};
use ocio_ops::op::{Op, OpVec};
use ocio_ops::op_data::{OpData, OpDataType, get_type_name};
use ocio_ops::open_color_types::TransformDirection;

use crate::config::Config;
use crate::context::Context;
use crate::transforms::group_transform::{GroupTransform, build_group_ops};

/// The class of a transform.
///
/// Port of `enum TransformType` (include/OpenColorIO/OpenColorTypes.h:361-386 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransformType {
    /// `TRANSFORM_TYPE_ALLOCATION`.
    Allocation = 0,
    /// `TRANSFORM_TYPE_BUILTIN`.
    Builtin,
    /// `TRANSFORM_TYPE_CDL`.
    Cdl,
    /// `TRANSFORM_TYPE_COLORSPACE`.
    ColorSpace,
    /// `TRANSFORM_TYPE_DISPLAY_VIEW`.
    DisplayView,
    /// `TRANSFORM_TYPE_EXPONENT`.
    Exponent,
    /// `TRANSFORM_TYPE_EXPONENT_WITH_LINEAR`.
    ExponentWithLinear,
    /// `TRANSFORM_TYPE_EXPOSURE_CONTRAST`.
    ExposureContrast,
    /// `TRANSFORM_TYPE_FILE`.
    File,
    /// `TRANSFORM_TYPE_FIXED_FUNCTION`.
    FixedFunction,
    /// `TRANSFORM_TYPE_GRADING_HUE_CURVE`.
    GradingHueCurve,
    /// `TRANSFORM_TYPE_GRADING_PRIMARY`.
    GradingPrimary,
    /// `TRANSFORM_TYPE_GRADING_RGB_CURVE`.
    GradingRgbCurve,
    /// `TRANSFORM_TYPE_GRADING_TONE`.
    GradingTone,
    /// `TRANSFORM_TYPE_GROUP`.
    Group,
    /// `TRANSFORM_TYPE_LOG_AFFINE`.
    LogAffine,
    /// `TRANSFORM_TYPE_LOG_CAMERA`.
    LogCamera,
    /// `TRANSFORM_TYPE_LOG`.
    Log,
    /// `TRANSFORM_TYPE_LOOK`.
    Look,
    /// `TRANSFORM_TYPE_LUT1D`.
    Lut1D,
    /// `TRANSFORM_TYPE_LUT3D`.
    Lut3D,
    /// `TRANSFORM_TYPE_MATRIX`.
    Matrix,
    /// `TRANSFORM_TYPE_RANGE`.
    Range,
}

/// A transform: one of OCIO's transform classes, as a value. A copy is upstream's
/// `createEditableCopy`.
///
/// The variants come with their classes (`p1-transforms` and later phases), so the enum is
/// `#[non_exhaustive]`.
///
/// Port of `Transform` (include/OpenColorIO/OpenColorTransforms.h @ v2.5.2) and its subclasses.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Transform {
    /// `GroupTransform`.
    Group(GroupTransform),
}

impl From<GroupTransform> for Transform {
    fn from(t: GroupTransform) -> Transform {
        Transform::Group(t)
    }
}

impl Transform {
    /// The transform's class.
    ///
    /// Port of `Transform::getTransformType` and its overrides (each class's header @ v2.5.2).
    #[doc(alias = "getTransformType")]
    pub fn transform_type(&self) -> TransformType {
        match self {
            Transform::Group(_) => TransformType::Group,
        }
    }

    /// Port of `Transform::getDirection` and its overrides.
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        match self {
            Transform::Group(t) => t.direction(),
        }
    }

    /// Port of `Transform::setDirection` and its overrides.
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        match self {
            Transform::Group(t) => t.set_direction(dir),
        }
    }

    /// Checks the transform, with upstream's message for the first problem.
    ///
    /// Port of `Transform::validate` and its overrides.
    pub fn validate(&self) -> Result<()> {
        match self {
            Transform::Group(t) => t.validate(),
        }
    }
}

/// The base class's check of the direction.
///
/// Upstream throws `"<typeid name>: invalid direction."` for a direction other than forward and
/// inverse; the name is the compiler's (MSVC's `class OpenColorIO_v2_5::MatrixTransformImpl`,
/// GCC's mangled one), so the text differs between the wheels. Neither a Rust
/// [`TransformDirection`] nor Python's enum can hold such a direction, so the check always
/// passes here (the owner's decision, 2026-10-01).
///
/// Port of `Transform::validate` (src/OpenColorIO/Transform.cpp:30-40 @ v2.5.2).
pub(crate) fn validate_direction(dir: TransformDirection) -> Result<()> {
    match dir {
        TransformDirection::Forward | TransformDirection::Inverse => Ok(()),
    }
}

impl fmt::Display for Transform {
    /// The transform's text, as its class prints it: the base of Python's `repr()`.
    ///
    /// Port of `operator<<(std::ostream &, const Transform &)` (src/OpenColorIO/
    /// Transform.cpp:177-308 @ v2.5.2). Its "Unknown transform type for serialization" can't
    /// happen with an enum.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Transform::Group(t) => t.fmt(f),
        }
    }
}

/// Appends the ops of `transform` in the direction `dir`.
///
/// Port of `BuildOps` (src/OpenColorIO/Transform.cpp:42-175 @ v2.5.2). A null transform, which
/// upstream builds as no ops, can't be passed: the callers hold transforms. Its "Unknown
/// transform type for creation" can't happen with an enum.
///
/// Internal to the processors: public for the port's tests only.
#[doc(hidden)]
pub fn build_ops(
    ops: &mut OpVec,
    config: &Config,
    context: &Context,
    transform: &Transform,
    dir: TransformDirection,
) -> Result<()> {
    match transform {
        Transform::Group(group_transform) => {
            build_group_ops(ops, config, context, group_transform, dir)
        }
    }
}

/// Appends to `group` the transform that `op` renders: nothing for the no-op types; for each op
/// type, its class's `Create<Class>Transform`, which comes with the class.
///
/// The op types whose transform class isn't ported yet are an error ("CreateTransform: the
/// transform of a <type> op is not ported yet."); upstream has them all. Upstream's own error
/// for an op type without one names the op's C++ class with `typeid`, which differs between the
/// wheels; every op type has a transform in 2.5.2, so it can't happen.
///
/// Port of `CreateTransform` (src/OpenColorIO/Transform.cpp:310-383 @ v2.5.2). Internal to the
/// processors: public for the port's tests only.
#[doc(hidden)]
pub fn create_transform(_group: &mut GroupTransform, op: &Op) -> Result<()> {
    // AllocationNoOp, FileNoOp, LookNoOp won't create a Transform.
    if op.is_no_op_type() {
        return Ok(());
    }

    let not_ported = |op_type: OpDataType| -> Result<()> {
        Err(Exception::new(format!(
            "CreateTransform: the transform of a {} op is not ported yet.",
            get_type_name(op_type)?
        )))
    };
    match &**op.data() {
        data @ (OpData::Cdl(_)
        | OpData::Gamma(_)
        | OpData::Log(_)
        | OpData::Matrix(_)
        | OpData::Range(_)
        | OpData::Exponent(_)
        | OpData::Lut1D(_)) => not_ported(data.get_type()),
        // No op holds a reference (the file readers replace it with the file's ops), and the
        // no-op types returned above.
        OpData::Reference(_) | OpData::NoOp(_) => {
            unreachable!("an op of a reference or no-op data has no transform")
        }
    }
}
