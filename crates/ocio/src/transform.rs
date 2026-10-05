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

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::op::{Op, OpVec};
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::TransformDirection;

use crate::config::Config;
use crate::context::Context;
use crate::transforms::allocation_transform::{AllocationTransform, build_allocation_op};
use crate::transforms::cdl_transform::{CdlTransform, build_cdl_op, create_cdl_transform};
use crate::transforms::color_space_transform::ColorSpaceTransform;
use crate::transforms::display_view_transform::DisplayViewTransform;
use crate::transforms::exponent_transform::{
    ExponentTransform, build_exponent_op, create_exponent_transform,
};
use crate::transforms::exponent_with_linear_transform::{
    ExponentWithLinearTransform, build_exponent_with_linear_op, create_gamma_transform,
};
use crate::transforms::group_transform::{GroupTransform, build_group_ops};
use crate::transforms::log_affine_transform::LogAffineTransform;
use crate::transforms::log_camera_transform::LogCameraTransform;
use crate::transforms::log_transform::{LogTransform, build_log_op, create_log_transform};
use crate::transforms::look_transform::LookTransform;
use crate::transforms::lut1d_transform::{Lut1DTransform, build_lut1d_op, create_lut1d_transform};
use crate::transforms::matrix_transform::{
    MatrixTransform, build_matrix_op, create_matrix_transform,
};
use crate::transforms::range_transform::{RangeTransform, build_range_op, create_range_transform};

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
/// Port of `Transform` (include/OpenColorIO/OpenColorTransforms.h:121-142 @ v2.5.2) and its
/// subclasses.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Transform {
    /// `AllocationTransform`.
    Allocation(AllocationTransform),
    /// `CDLTransform`.
    Cdl(CdlTransform),
    /// `ColorSpaceTransform`.
    ColorSpace(ColorSpaceTransform),
    /// `DisplayViewTransform`.
    DisplayView(DisplayViewTransform),
    /// `ExponentTransform`.
    Exponent(ExponentTransform),
    /// `ExponentWithLinearTransform`.
    ExponentWithLinear(ExponentWithLinearTransform),
    /// `GroupTransform`.
    Group(GroupTransform),
    /// `LogAffineTransform`.
    LogAffine(LogAffineTransform),
    /// `LogCameraTransform`.
    LogCamera(LogCameraTransform),
    /// `LogTransform`.
    Log(LogTransform),
    /// `LookTransform`.
    Look(LookTransform),
    /// `Lut1DTransform`.
    Lut1D(Lut1DTransform),
    /// `MatrixTransform`.
    Matrix(MatrixTransform),
    /// `RangeTransform`.
    Range(RangeTransform),
}

impl From<AllocationTransform> for Transform {
    fn from(t: AllocationTransform) -> Transform {
        Transform::Allocation(t)
    }
}

impl From<CdlTransform> for Transform {
    fn from(t: CdlTransform) -> Transform {
        Transform::Cdl(t)
    }
}

impl From<ColorSpaceTransform> for Transform {
    fn from(t: ColorSpaceTransform) -> Transform {
        Transform::ColorSpace(t)
    }
}

impl From<DisplayViewTransform> for Transform {
    fn from(t: DisplayViewTransform) -> Transform {
        Transform::DisplayView(t)
    }
}

impl From<ExponentTransform> for Transform {
    fn from(t: ExponentTransform) -> Transform {
        Transform::Exponent(t)
    }
}

impl From<ExponentWithLinearTransform> for Transform {
    fn from(t: ExponentWithLinearTransform) -> Transform {
        Transform::ExponentWithLinear(t)
    }
}

impl From<GroupTransform> for Transform {
    fn from(t: GroupTransform) -> Transform {
        Transform::Group(t)
    }
}

impl From<LogAffineTransform> for Transform {
    fn from(t: LogAffineTransform) -> Transform {
        Transform::LogAffine(t)
    }
}

impl From<LogCameraTransform> for Transform {
    fn from(t: LogCameraTransform) -> Transform {
        Transform::LogCamera(t)
    }
}

impl From<LogTransform> for Transform {
    fn from(t: LogTransform) -> Transform {
        Transform::Log(t)
    }
}

impl From<LookTransform> for Transform {
    fn from(t: LookTransform) -> Transform {
        Transform::Look(t)
    }
}

impl From<Lut1DTransform> for Transform {
    fn from(t: Lut1DTransform) -> Transform {
        Transform::Lut1D(t)
    }
}

impl From<MatrixTransform> for Transform {
    fn from(t: MatrixTransform) -> Transform {
        Transform::Matrix(t)
    }
}

impl From<RangeTransform> for Transform {
    fn from(t: RangeTransform) -> Transform {
        Transform::Range(t)
    }
}

impl Transform {
    /// The transform's class.
    ///
    /// Port of `Transform::getTransformType` (include/OpenColorIO/OpenColorTransforms.h:130 @
    /// v2.5.2) and its overrides (each class's header).
    #[doc(alias = "getTransformType")]
    pub fn transform_type(&self) -> TransformType {
        match self {
            Transform::Allocation(_) => TransformType::Allocation,
            Transform::Cdl(_) => TransformType::Cdl,
            Transform::ColorSpace(_) => TransformType::ColorSpace,
            Transform::DisplayView(_) => TransformType::DisplayView,
            Transform::Exponent(_) => TransformType::Exponent,
            Transform::ExponentWithLinear(_) => TransformType::ExponentWithLinear,
            Transform::Group(_) => TransformType::Group,
            Transform::LogAffine(_) => TransformType::LogAffine,
            Transform::LogCamera(_) => TransformType::LogCamera,
            Transform::Log(_) => TransformType::Log,
            Transform::Look(_) => TransformType::Look,
            Transform::Lut1D(_) => TransformType::Lut1D,
            Transform::Matrix(_) => TransformType::Matrix,
            Transform::Range(_) => TransformType::Range,
        }
    }

    /// Port of `Transform::getDirection` (include/OpenColorIO/OpenColorTransforms.h:126 @ v2.5.2)
    /// and its overrides.
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        match self {
            Transform::Allocation(t) => t.direction(),
            Transform::Cdl(t) => t.direction(),
            Transform::ColorSpace(t) => t.direction(),
            Transform::DisplayView(t) => t.direction(),
            Transform::Exponent(t) => t.direction(),
            Transform::ExponentWithLinear(t) => t.direction(),
            Transform::Group(t) => t.direction(),
            Transform::LogAffine(t) => t.direction(),
            Transform::LogCamera(t) => t.direction(),
            Transform::Log(t) => t.direction(),
            Transform::Look(t) => t.direction(),
            Transform::Lut1D(t) => t.direction(),
            Transform::Matrix(t) => t.direction(),
            Transform::Range(t) => t.direction(),
        }
    }

    /// Port of `Transform::setDirection` (include/OpenColorIO/OpenColorTransforms.h:128 @ v2.5.2)
    /// and its overrides.
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        match self {
            Transform::Allocation(t) => t.set_direction(dir),
            Transform::Cdl(t) => t.set_direction(dir),
            Transform::ColorSpace(t) => t.set_direction(dir),
            Transform::DisplayView(t) => t.set_direction(dir),
            Transform::Exponent(t) => t.set_direction(dir),
            Transform::ExponentWithLinear(t) => t.set_direction(dir),
            Transform::Group(t) => t.set_direction(dir),
            Transform::LogAffine(t) => t.set_direction(dir),
            Transform::LogCamera(t) => t.set_direction(dir),
            Transform::Log(t) => t.set_direction(dir),
            Transform::Look(t) => t.set_direction(dir),
            Transform::Lut1D(t) => t.set_direction(dir),
            Transform::Matrix(t) => t.set_direction(dir),
            Transform::Range(t) => t.set_direction(dir),
        }
    }

    /// Checks the transform, with upstream's message for the first problem.
    ///
    /// Port of `Transform::validate` (src/OpenColorIO/Transform.cpp:30-40 @ v2.5.2) and its
    /// overrides.
    pub fn validate(&self) -> Result<()> {
        match self {
            Transform::Allocation(t) => t.validate(),
            Transform::Cdl(t) => t.validate(),
            Transform::ColorSpace(t) => t.validate(),
            Transform::DisplayView(t) => t.validate(),
            Transform::Exponent(t) => t.validate(),
            Transform::ExponentWithLinear(t) => t.validate(),
            Transform::Group(t) => t.validate(),
            Transform::LogAffine(t) => t.validate(),
            Transform::LogCamera(t) => t.validate(),
            Transform::Log(t) => t.validate(),
            Transform::Look(t) => t.validate(),
            Transform::Lut1D(t) => t.validate(),
            Transform::Matrix(t) => t.validate(),
            Transform::Range(t) => t.validate(),
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

/// `os << s` for a C string (`const char *`): the bytes up to the first NUL.
///
/// The stream holds text, so bytes that aren't UTF-8 print as U+FFFD here; upstream prints
/// them as they are, and Python can't decode such a text either (pybind11 raises
/// `UnicodeDecodeError`).
pub(crate) fn put_c_str(os: &mut OStringStream, s: &[u8]) {
    let s = ocio_ops::utils::string_utils::c_str(s);
    match std::str::from_utf8(s) {
        Ok(text) => os.put_str(text),
        Err(_) => os.put_str(&String::from_utf8_lossy(s)),
    }
}

/// `os << b` for a C++ `bool`, without `std::boolalpha`: `1` or `0`.
pub(crate) fn put_bool(os: &mut OStringStream, b: bool) {
    os.put_str(if b { "1" } else { "0" });
}

impl fmt::Display for Transform {
    /// The transform's text, as its class prints it: the base of Python's `repr()`.
    ///
    /// Port of `operator<<(std::ostream &, const Transform &)` (src/OpenColorIO/
    /// Transform.cpp:177-308 @ v2.5.2). Its "Unknown transform type for serialization" can't
    /// happen with an enum.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(os.str())
    }
}

impl Transform {
    /// Writes the transform's text to `os`, a stream that a group shares with its children:
    /// what a class changes in its state stays for what follows (a MatrixTransform's precision,
    /// docs/improvements.md, I-73).
    ///
    /// Port of `operator<<(std::ostream &, const Transform &)` (src/OpenColorIO/
    /// Transform.cpp:177-308 @ v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        match self {
            Transform::Allocation(t) => t.write_text(os),
            Transform::Cdl(t) => t.write_text(os),
            Transform::ColorSpace(t) => t.write_text(os),
            Transform::DisplayView(t) => t.write_text(os),
            Transform::Exponent(t) => t.write_text(os),
            Transform::ExponentWithLinear(t) => t.write_text(os),
            Transform::Group(t) => t.write_text(os),
            Transform::LogAffine(t) => t.write_text(os),
            Transform::LogCamera(t) => t.write_text(os),
            Transform::Log(t) => t.write_text(os),
            Transform::Look(t) => t.write_text(os),
            Transform::Lut1D(t) => t.write_text(os),
            Transform::Matrix(t) => t.write_text(os),
            Transform::Range(t) => t.write_text(os),
        }
    }
}

/// The error of a class whose op builder is not ported yet: `work_package` ports it.
fn not_ported_yet(class: &str, work_package: &str) -> Exception {
    Exception::new(format!(
        "{class}: building its ops is not ported yet ({work_package})."
    ))
}

/// Appends the ops of `transform` in the direction `dir`.
///
/// Port of `BuildOps` (src/OpenColorIO/Transform.cpp:42-175 @ v2.5.2). A null transform, which
/// upstream builds as no ops, can't be passed: the callers hold transforms. Its "Unknown
/// transform type for creation" can't happen with an enum.
///
/// Internal to the processors (crate-private, the owner's decision of 2026-10-04).
pub(crate) fn build_ops(
    ops: &mut OpVec,
    config: &Config,
    context: &Context,
    transform: &Transform,
    dir: TransformDirection,
) -> Result<()> {
    match transform {
        Transform::Allocation(allocation_transform) => {
            build_allocation_op(ops, allocation_transform, dir)
        }
        Transform::Cdl(cdl_transform) => build_cdl_op(ops, config, cdl_transform, dir),
        Transform::ColorSpace(_) => Err(not_ported_yet("ColorSpaceTransform", "WP 3.2a")),
        Transform::DisplayView(_) => Err(not_ported_yet("DisplayViewTransform", "WP 3.2c")),
        Transform::Exponent(exponent_transform) => {
            build_exponent_op(ops, config, exponent_transform, dir)
        }
        Transform::ExponentWithLinear(exponent_transform) => {
            build_exponent_with_linear_op(ops, exponent_transform, dir)
        }
        Transform::Group(group_transform) => {
            build_group_ops(ops, config, context, group_transform, dir)
        }
        Transform::LogAffine(log_transform) => build_log_op(ops, log_transform.data(), dir),
        Transform::LogCamera(log_transform) => build_log_op(ops, log_transform.data(), dir),
        Transform::Log(log_transform) => build_log_op(ops, log_transform.data(), dir),
        Transform::Look(_) => Err(not_ported_yet("LookTransform", "WP 3.2b")),
        Transform::Lut1D(lut_transform) => build_lut1d_op(ops, lut_transform, dir),
        Transform::Matrix(matrix_transform) => build_matrix_op(ops, matrix_transform, dir),
        Transform::Range(range_transform) => build_range_op(ops, range_transform, dir),
    }
}

/// Appends to `group` the transform that `op` renders: nothing for the no-op types; for each op
/// type, its class's `Create<Class>Transform`, which comes with the class.
///
/// Every op type the port has so far has its transform class; a family that adds an op type adds
/// its arm (a "not ported yet" error until its class comes). Upstream's own error for an op
/// type without one names the op's C++ class with `typeid`, which differs between the
/// wheels; every op type has a transform in 2.5.2, so it can't happen.
///
/// Port of `CreateTransform` (src/OpenColorIO/Transform.cpp:310-383 @ v2.5.2). Internal to the
/// processors (crate-private).
pub(crate) fn create_transform(group: &mut GroupTransform, op: &Op) -> Result<()> {
    // AllocationNoOp, FileNoOp, LookNoOp won't create a Transform.
    if op.is_no_op_type() {
        return Ok(());
    }

    match &**op.data() {
        OpData::Exponent(_) => create_exponent_transform(group, op),
        OpData::Gamma(_) => create_gamma_transform(group, op),
        OpData::Matrix(_) => create_matrix_transform(group, op),
        OpData::Range(_) => create_range_transform(group, op),
        OpData::Log(_) => create_log_transform(group, op),
        OpData::Cdl(_) => create_cdl_transform(group, op),
        OpData::Lut1D(_) => create_lut1d_transform(group, op),
        // No op holds a reference (the file readers replace it with the file's ops), and the
        // no-op types returned above.
        OpData::Reference(_) | OpData::NoOp(_) => {
            unreachable!("an op of a reference or no-op data has no transform")
        }
    }
}
