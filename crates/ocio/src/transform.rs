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

use std::cell::Cell;
use std::fmt;
use std::thread::LocalKey;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::op::{Op, OpVec};
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::TransformDirection;

use crate::config::Config;
use crate::context::Context;
use crate::transforms::allocation_transform::{AllocationTransform, build_allocation_op};
use crate::transforms::builtin_transform::{BuiltinTransform, build_builtin_ops};
use crate::transforms::cdl_transform::{CdlTransform, build_cdl_op, create_cdl_transform};
use crate::transforms::color_space_transform::{ColorSpaceTransform, build_color_space_ops};
use crate::transforms::display_view_transform::{DisplayViewTransform, build_display_ops};
use crate::transforms::exponent_transform::{
    ExponentTransform, build_exponent_op, create_exponent_transform,
};
use crate::transforms::exponent_with_linear_transform::{
    ExponentWithLinearTransform, build_exponent_with_linear_op, create_gamma_transform,
};
use crate::transforms::exposure_contrast_transform::{
    ExposureContrastTransform, build_exposure_contrast_op, create_exposure_contrast_transform,
};
use crate::transforms::file_transform::{FileTransform, build_file_transform_ops};
use crate::transforms::fixed_function_transform::{
    FixedFunctionTransform, build_fixed_function_op, create_fixed_function_transform,
};
use crate::transforms::grading_primary_transform::{
    GradingPrimaryTransform, build_grading_primary_op, create_grading_primary_transform,
};
use crate::transforms::grading_rgb_curve_transform::{
    GradingRgbCurveTransform, build_grading_rgb_curve_op, create_grading_rgb_curve_transform,
};
use crate::transforms::group_transform::{GroupTransform, build_group_ops};
use crate::transforms::log_affine_transform::LogAffineTransform;
use crate::transforms::log_camera_transform::LogCameraTransform;
use crate::transforms::log_transform::{LogTransform, build_log_op, create_log_transform};
use crate::transforms::look_transform::{LookTransform, build_look_ops};
use crate::transforms::lut1d_transform::{Lut1DTransform, build_lut1d_op, create_lut1d_transform};
use crate::transforms::lut3d_transform::{Lut3DTransform, build_lut3d_op, create_lut3d_transform};
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
    /// `BuiltinTransform`.
    Builtin(BuiltinTransform),
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
    /// `ExposureContrastTransform`.
    ExposureContrast(ExposureContrastTransform),
    /// `FileTransform`.
    File(FileTransform),
    /// `FixedFunctionTransform`.
    FixedFunction(FixedFunctionTransform),
    /// `GradingPrimaryTransform`.
    GradingPrimary(GradingPrimaryTransform),
    /// `GradingRGBCurveTransform`.
    GradingRgbCurve(GradingRgbCurveTransform),
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
    /// `Lut3DTransform`.
    Lut3D(Lut3DTransform),
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

impl From<BuiltinTransform> for Transform {
    fn from(t: BuiltinTransform) -> Transform {
        Transform::Builtin(t)
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

impl From<ExposureContrastTransform> for Transform {
    fn from(t: ExposureContrastTransform) -> Transform {
        Transform::ExposureContrast(t)
    }
}

impl From<FileTransform> for Transform {
    fn from(t: FileTransform) -> Transform {
        Transform::File(t)
    }
}

impl From<FixedFunctionTransform> for Transform {
    fn from(t: FixedFunctionTransform) -> Transform {
        Transform::FixedFunction(t)
    }
}

impl From<GradingPrimaryTransform> for Transform {
    fn from(t: GradingPrimaryTransform) -> Transform {
        Transform::GradingPrimary(t)
    }
}

impl From<GradingRgbCurveTransform> for Transform {
    fn from(t: GradingRgbCurveTransform) -> Transform {
        Transform::GradingRgbCurve(t)
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

impl From<Lut3DTransform> for Transform {
    fn from(t: Lut3DTransform) -> Transform {
        Transform::Lut3D(t)
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
    /// A copy made as upstream's `createEditableCopy` makes it, which the config's objects keep
    /// of a transform they are given: a FixedFunctionTransform validates its style and
    /// parameters ([`FixedFunctionTransform::create_editable_copy`]), the only class whose copy
    /// can fail; the others copy as they are. A group copies its children without validating
    /// them, as upstream's shares them (docs/improvements.md, I-11).
    ///
    /// Port of `Transform::createEditableCopy` (include/OpenColorIO/OpenColorTransforms.h:124 @
    /// v2.5.2) and its overrides (each class's `createEditableCopy`).
    #[doc(alias = "createEditableCopy")]
    pub fn create_editable_copy(&self) -> Result<Transform> {
        match self {
            Transform::FixedFunction(t) => Ok(t.create_editable_copy()?.into()),
            other => Ok(other.clone()),
        }
    }

    /// The transform's class.
    ///
    /// Port of `Transform::getTransformType` (include/OpenColorIO/OpenColorTransforms.h:130 @
    /// v2.5.2) and its overrides (each class's header).
    #[doc(alias = "getTransformType")]
    pub fn transform_type(&self) -> TransformType {
        match self {
            Transform::Allocation(_) => TransformType::Allocation,
            Transform::Builtin(_) => TransformType::Builtin,
            Transform::Cdl(_) => TransformType::Cdl,
            Transform::ColorSpace(_) => TransformType::ColorSpace,
            Transform::DisplayView(_) => TransformType::DisplayView,
            Transform::Exponent(_) => TransformType::Exponent,
            Transform::ExponentWithLinear(_) => TransformType::ExponentWithLinear,
            Transform::ExposureContrast(_) => TransformType::ExposureContrast,
            Transform::File(_) => TransformType::File,
            Transform::FixedFunction(_) => TransformType::FixedFunction,
            Transform::GradingPrimary(_) => TransformType::GradingPrimary,
            Transform::GradingRgbCurve(_) => TransformType::GradingRgbCurve,
            Transform::Group(_) => TransformType::Group,
            Transform::LogAffine(_) => TransformType::LogAffine,
            Transform::LogCamera(_) => TransformType::LogCamera,
            Transform::Log(_) => TransformType::Log,
            Transform::Look(_) => TransformType::Look,
            Transform::Lut1D(_) => TransformType::Lut1D,
            Transform::Lut3D(_) => TransformType::Lut3D,
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
            Transform::Builtin(t) => t.direction(),
            Transform::Cdl(t) => t.direction(),
            Transform::ColorSpace(t) => t.direction(),
            Transform::DisplayView(t) => t.direction(),
            Transform::Exponent(t) => t.direction(),
            Transform::ExponentWithLinear(t) => t.direction(),
            Transform::ExposureContrast(t) => t.direction(),
            Transform::File(t) => t.direction(),
            Transform::FixedFunction(t) => t.direction(),
            Transform::GradingPrimary(t) => t.direction(),
            Transform::GradingRgbCurve(t) => t.direction(),
            Transform::Group(t) => t.direction(),
            Transform::LogAffine(t) => t.direction(),
            Transform::LogCamera(t) => t.direction(),
            Transform::Log(t) => t.direction(),
            Transform::Look(t) => t.direction(),
            Transform::Lut1D(t) => t.direction(),
            Transform::Lut3D(t) => t.direction(),
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
            Transform::Builtin(t) => t.set_direction(dir),
            Transform::Cdl(t) => t.set_direction(dir),
            Transform::ColorSpace(t) => t.set_direction(dir),
            Transform::DisplayView(t) => t.set_direction(dir),
            Transform::Exponent(t) => t.set_direction(dir),
            Transform::ExponentWithLinear(t) => t.set_direction(dir),
            Transform::ExposureContrast(t) => t.set_direction(dir),
            Transform::File(t) => t.set_direction(dir),
            Transform::FixedFunction(t) => t.set_direction(dir),
            Transform::GradingPrimary(t) => t.set_direction(dir),
            Transform::GradingRgbCurve(t) => t.set_direction(dir),
            Transform::Group(t) => t.set_direction(dir),
            Transform::LogAffine(t) => t.set_direction(dir),
            Transform::LogCamera(t) => t.set_direction(dir),
            Transform::Log(t) => t.set_direction(dir),
            Transform::Look(t) => t.set_direction(dir),
            Transform::Lut1D(t) => t.set_direction(dir),
            Transform::Lut3D(t) => t.set_direction(dir),
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
            Transform::Builtin(t) => t.validate(),
            Transform::Cdl(t) => t.validate(),
            Transform::ColorSpace(t) => t.validate(),
            Transform::DisplayView(t) => t.validate(),
            Transform::Exponent(t) => t.validate(),
            Transform::ExponentWithLinear(t) => t.validate(),
            Transform::ExposureContrast(t) => t.validate(),
            Transform::File(t) => t.validate(),
            Transform::FixedFunction(t) => t.validate(),
            Transform::GradingPrimary(t) => t.validate(),
            Transform::GradingRgbCurve(t) => t.validate(),
            Transform::Group(t) => t.validate(),
            Transform::LogAffine(t) => t.validate(),
            Transform::LogCamera(t) => t.validate(),
            Transform::Log(t) => t.validate(),
            Transform::Look(t) => t.validate(),
            Transform::Lut1D(t) => t.validate(),
            Transform::Lut3D(t) => t.validate(),
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

/// `os << s` for a C string (`const char *`): the bytes up to the first NUL, as they are
/// (the stream holds bytes, so names that aren't UTF-8 print exactly as upstream prints them).
pub(crate) fn put_c_str(os: &mut OStringStream, s: &[u8]) {
    os.put_c_str(s);
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
        f.write_str(&String::from_utf8_lossy(&self.to_bytes()))
    }
}

impl Transform {
    /// The transform's text, as its class prints it, in bytes: names and paths in it pass
    /// through unchanged, where `Display` replaces what isn't UTF-8.
    ///
    /// Port of `operator<<(std::ostream &, const Transform &)` (src/OpenColorIO/
    /// Transform.cpp:177-308 @ v2.5.2), on a new stream.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        os.into_bytes()
    }

    /// Writes the transform's text to `os`, a stream that a group shares with its children:
    /// what a class changes in its state stays for what follows (a MatrixTransform's precision,
    /// docs/improvements.md, I-73).
    ///
    /// Port of `operator<<(std::ostream &, const Transform &)` (src/OpenColorIO/
    /// Transform.cpp:177-308 @ v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        match self {
            Transform::Allocation(t) => t.write_text(os),
            Transform::Builtin(t) => t.write_text(os),
            Transform::Cdl(t) => t.write_text(os),
            Transform::ColorSpace(t) => t.write_text(os),
            Transform::DisplayView(t) => t.write_text(os),
            Transform::Exponent(t) => t.write_text(os),
            Transform::ExponentWithLinear(t) => t.write_text(os),
            Transform::ExposureContrast(t) => t.write_text(os),
            Transform::File(t) => t.write_text(os),
            Transform::FixedFunction(t) => t.write_text(os),
            Transform::GradingPrimary(t) => t.write_text(os),
            Transform::GradingRgbCurve(t) => t.write_text(os),
            Transform::Group(t) => t.write_text(os),
            Transform::LogAffine(t) => t.write_text(os),
            Transform::LogCamera(t) => t.write_text(os),
            Transform::Log(t) => t.write_text(os),
            Transform::Look(t) => t.write_text(os),
            Transform::Lut1D(t) => t.write_text(os),
            Transform::Lut3D(t) => t.write_text(os),
            Transform::Matrix(t) => t.write_text(os),
            Transform::Range(t) => t.write_text(os),
        }
    }
}

/// The deepest nesting of transforms that [`build_ops`] and
/// [`collect_context_variables`](crate::context_variable_utils::collect_context_variables)
/// follow: a color space, look, view transform or named transform whose transform refers back
/// to it nests without end. Group transforms don't count here: they name nothing, and the
/// loader refuses groups nested more than 590 deep (docs/improvements.md, U-60); they count in
/// [`MAX_NESTING_WEIGHT`].
///
/// Upstream has no limit: it recurses until the stack overflows, which ends the process (the
/// wheel at 573 nested display view transforms on Windows, docs/improvements.md U-58). The port
/// refuses deeper nesting with an error instead (the owner's general rule), about half the
/// shallowest depth at which the wheel crashes. The functions [`build_ops`] recurses through
/// keep their frames small, so that the longest chains it accepts build on a 1 MiB thread at
/// every optimization level (U-58).
pub(crate) const MAX_TRANSFORM_NESTING: usize = 256;

/// A level's weight in [`MAX_NESTING_WEIGHT`], in hundredths of a color space transform's: what
/// a level of each kind costs the wheel's stack, from the depths at which the Windows wheel
/// crashes (docs/improvements.md, U-58): 1,363 color space transforms; 618 look transforms
/// (2.21 times as much each); 573 display view transforms (2.38); 794 color space transforms
/// each in a group (a group 0.72); 1,193 color space transforms before 256 CTF files (a file
/// 0.66; 2,125 files alone, 0.64). Each is rounded up. A transform that names nothing weighs as
/// a color space transform: it ends the chain.
const COLOR_SPACE_WEIGHT: usize = 100;
/// See [`COLOR_SPACE_WEIGHT`].
const LOOK_WEIGHT: usize = 221;
/// See [`COLOR_SPACE_WEIGHT`].
const DISPLAY_VIEW_WEIGHT: usize = 238;
/// See [`COLOR_SPACE_WEIGHT`].
const GROUP_WEIGHT: usize = 72;
/// See [`COLOR_SPACE_WEIGHT`]: a file of a chain of CTF references, and a file transform's own
/// level of [`build_ops`]. A file transform reached through [`build_ops`] so counts twice: as a
/// transform, and as the first file of its chain.
const FILE_WEIGHT: usize = 67;

/// The heaviest nesting of everything [`build_ops`] and
/// [`collect_context_variables`](crate::context_variable_utils::collect_context_variables)
/// recurse through, each level weighed by its kind ([`COLOR_SPACE_WEIGHT`] and siblings),
/// counting group transforms and (for [`build_ops`]) the files of a chain of CTF references
/// too: within [`MAX_TRANSFORM_NESTING`] transforms, groups multiply the depth of a chain, and
/// a chain of transforms can end in a chain of files.
///
/// Upstream has no limit: the wheel's stack overflows (docs/improvements.md, U-58). The port
/// refuses deeper nesting with an error, at half the wheel's stack as the weights measure it
/// (half of 1,363 color space transforms), so that every kind of chain, groups and files
/// included, stops at about half the depth at which the wheel crashes, or before. The deepest
/// group the loader reads (590 groups and the transform they hold, U-60) weighs 42,580.
pub(crate) const MAX_NESTING_WEIGHT: usize = 1_363 * COLOR_SPACE_WEIGHT / 2;

/// The weight of a level of `transform` (see [`COLOR_SPACE_WEIGHT`]).
fn nesting_weight(transform: &Transform) -> usize {
    match transform {
        Transform::Group(_) => GROUP_WEIGHT,
        Transform::Look(_) => LOOK_WEIGHT,
        Transform::DisplayView(_) => DISPLAY_VIEW_WEIGHT,
        Transform::File(_) => FILE_WEIGHT,
        _ => COLOR_SPACE_WEIGHT,
    }
}

thread_local! {
    /// How many transforms other than groups [`build_ops`] nests on this thread.
    static BUILD_NESTING: Cell<usize> = const { Cell::new(0) };
    /// How many transforms other than groups `collect_context_variables` nests on this thread.
    static COLLECT_NESTING: Cell<usize> = const { Cell::new(0) };
    /// The weight of the levels of [`build_ops`] (groups included) and of CTF references that
    /// nest on this thread.
    static BUILD_WEIGHT: Cell<usize> = const { Cell::new(0) };
    /// The weight of the levels of `collect_context_variables` (groups included) that nest on
    /// this thread.
    static COLLECT_WEIGHT: Cell<usize> = const { Cell::new(0) };
}

/// One level of a nesting counter, of a weight, given back when it is dropped.
pub(crate) struct Level(&'static LocalKey<Cell<usize>>, usize);

impl Level {
    /// `weight` more of `counter`; `None` past `limit`.
    fn enter(
        counter: &'static LocalKey<Cell<usize>>,
        weight: usize,
        limit: usize,
    ) -> Option<Level> {
        let total = counter.get() + weight;
        if total > limit {
            return None;
        }
        counter.set(total);
        Some(Level(counter, weight))
    }

    /// One more file of a chain of CTF references in [`MAX_NESTING_WEIGHT`]'s count of
    /// [`build_ops`]; `None` past it, whose error is [`total_too_deep`].
    pub(crate) fn file() -> Option<Level> {
        Level::enter(&BUILD_WEIGHT, FILE_WEIGHT, MAX_NESTING_WEIGHT)
    }
}

impl Drop for Level {
    fn drop(&mut self) {
        self.0.set(self.0.get() - self.1);
    }
}

/// The limit a level passed.
#[derive(Debug, Clone, Copy)]
pub(crate) enum TooDeep {
    /// [`MAX_TRANSFORM_NESTING`].
    Transforms,
    /// [`MAX_NESTING_WEIGHT`].
    Total,
}

impl TooDeep {
    /// The error of a level past the limit.
    #[cold]
    #[inline(never)]
    pub(crate) fn error<T>(self) -> Result<T> {
        match self {
            TooDeep::Transforms => Err(Exception::new(format!(
                "The transforms nest more than {MAX_TRANSFORM_NESTING} levels deep: a color \
                 space, look, view transform or named transform may refer to itself."
            ))),
            TooDeep::Total => total_too_deep(),
        }
    }
}

/// The error of a level past [`MAX_NESTING_WEIGHT`].
#[cold]
#[inline(never)]
pub(crate) fn total_too_deep<T>() -> Result<T> {
    Err(Exception::new(
        "The transforms nest too deep, counting groups and CTF references: upstream's stack \
         overflows.",
    ))
}

/// One level of nesting of [`build_ops`] or `collect_context_variables`: of the transforms
/// other than groups, and of the weight, given back when it is dropped.
pub(crate) struct Nesting {
    /// The level of [`MAX_TRANSFORM_NESTING`], but for a group.
    _transform: Option<Level>,
    /// The level of [`MAX_NESTING_WEIGHT`].
    _total: Level,
}

impl Nesting {
    /// One more level of `transforms` (unless `transform` is a group) and its weight in
    /// `total`; the limit it passes, [`MAX_TRANSFORM_NESTING`] first. Not a `Result` of an
    /// error, so that the frames of the recursion don't hold an error's room
    /// (docs/improvements.md, U-58).
    fn enter(
        transforms: &'static LocalKey<Cell<usize>>,
        total: &'static LocalKey<Cell<usize>>,
        transform: &Transform,
    ) -> std::result::Result<Nesting, TooDeep> {
        let level = if matches!(transform, Transform::Group(_)) {
            None
        } else {
            match Level::enter(transforms, 1, MAX_TRANSFORM_NESTING) {
                Some(level) => Some(level),
                None => return Err(TooDeep::Transforms),
            }
        };
        match Level::enter(total, nesting_weight(transform), MAX_NESTING_WEIGHT) {
            Some(total) => Ok(Nesting {
                _transform: level,
                _total: total,
            }),
            None => Err(TooDeep::Total),
        }
    }

    /// One more level of [`build_ops`], of `transform`.
    pub(crate) fn build(transform: &Transform) -> std::result::Result<Nesting, TooDeep> {
        Nesting::enter(&BUILD_NESTING, &BUILD_WEIGHT, transform)
    }

    /// One more level of `collect_context_variables`, of `transform`.
    pub(crate) fn collect(transform: &Transform) -> std::result::Result<Nesting, TooDeep> {
        Nesting::enter(&COLLECT_NESTING, &COLLECT_WEIGHT, transform)
    }
}

/// `Err` of an exception of `message`, built out of line: the functions [`build_ops`] recurses
/// through keep their frames small, so that the deepest chains they accept fit the stack
/// (docs/improvements.md, U-58).
#[cold]
#[inline(never)]
pub(crate) fn build_error(message: &'static str) -> Result<()> {
    Err(Exception::new(message))
}

/// Appends the ops of `transform` in the direction `dir`.
///
/// Port of `BuildOps` (src/OpenColorIO/Transform.cpp:42-175 @ v2.5.2). A null transform, which
/// upstream builds as no ops, can't be passed: the callers hold transforms. Its "Unknown
/// transform type for creation" can't happen with an enum. Transforms nested deeper than
/// [`MAX_TRANSFORM_NESTING`] are an error, where upstream overflows the stack
/// (docs/improvements.md, U-58).
///
/// Internal to the processors (crate-private, the owner's decision of 2026-10-04).
pub(crate) fn build_ops(
    ops: &mut OpVec,
    config: &Config,
    context: &Context,
    transform: &Transform,
    dir: TransformDirection,
) -> Result<()> {
    // A group names nothing, so it can't refer to itself, and the loader bounds its nesting
    // (U-60): it counts only in the total, which groups multiply.
    let _nesting = match Nesting::build(transform) {
        Ok(nesting) => nesting,
        Err(too_deep) => return too_deep.error(),
    };
    match transform {
        Transform::ColorSpace(color_space_transform) => {
            build_color_space_ops(ops, config, context, color_space_transform, dir)
        }
        Transform::DisplayView(display_view_transform) => {
            build_display_ops(ops, config, context, display_view_transform, dir)
        }
        Transform::File(file_transform) => {
            build_file_transform_ops(ops, config, context, file_transform, dir)
        }
        Transform::Group(group_transform) => {
            build_group_ops(ops, config, context, group_transform, dir)
        }
        Transform::Look(look_transform) => {
            build_look_ops(ops, config, context, look_transform, dir)
        }
        _ => build_leaf_ops(ops, config, transform, dir),
    }
}

/// Appends the ops of `transform`, one that names nothing, in the direction `dir`: the arms of
/// [`build_ops`] that don't recurse, out of line so that its frame stays small
/// (docs/improvements.md, U-58).
///
/// Part of the port of `BuildOps` (src/OpenColorIO/Transform.cpp:42-175 @ v2.5.2).
#[inline(never)]
fn build_leaf_ops(
    ops: &mut OpVec,
    config: &Config,
    transform: &Transform,
    dir: TransformDirection,
) -> Result<()> {
    match transform {
        Transform::Allocation(allocation_transform) => {
            build_allocation_op(ops, allocation_transform, dir)
        }
        Transform::Builtin(builtin_transform) => build_builtin_ops(ops, builtin_transform, dir),
        Transform::Cdl(cdl_transform) => build_cdl_op(ops, config, cdl_transform, dir),
        Transform::Exponent(exponent_transform) => {
            build_exponent_op(ops, config, exponent_transform, dir)
        }
        Transform::ExponentWithLinear(exponent_transform) => {
            build_exponent_with_linear_op(ops, exponent_transform, dir)
        }
        Transform::ExposureContrast(ec_transform) => {
            build_exposure_contrast_op(ops, ec_transform, dir)
        }
        Transform::FixedFunction(ff_transform) => build_fixed_function_op(ops, ff_transform, dir),
        Transform::LogAffine(log_transform) => build_log_op(ops, log_transform.data(), dir),
        Transform::LogCamera(log_transform) => build_log_op(ops, log_transform.data(), dir),
        Transform::Log(log_transform) => build_log_op(ops, log_transform.data(), dir),
        Transform::GradingPrimary(t) => build_grading_primary_op(ops, t, dir),
        Transform::GradingRgbCurve(t) => build_grading_rgb_curve_op(ops, t, dir),
        Transform::Lut1D(lut_transform) => build_lut1d_op(ops, lut_transform, dir),
        Transform::Lut3D(lut_transform) => build_lut3d_op(ops, lut_transform, dir),
        Transform::Matrix(matrix_transform) => build_matrix_op(ops, matrix_transform, dir),
        Transform::Range(range_transform) => build_range_op(ops, range_transform, dir),
        Transform::ColorSpace(_)
        | Transform::DisplayView(_)
        | Transform::File(_)
        | Transform::Group(_)
        | Transform::Look(_) => unreachable!("build_ops builds the transforms that name others"),
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
        OpData::FixedFunction(_) => create_fixed_function_transform(group, op),
        OpData::GradingRgbCurve(_) => create_grading_rgb_curve_transform(group, op),
        OpData::Lut3D(_) => create_lut3d_transform(group, op),
        // `CreateGradingHueCurveTransform` (src/OpenColorIO/ops/gradinghuecurve/
        // GradingHueCurveOp.cpp:219-232 @ v2.5.2) comes with `GradingHueCurveTransform` (WP
        // 5.5e).
        OpData::GradingHueCurve(_) => Err(Exception::new(
            "CreateGradingHueCurveTransform is not ported yet (WP 5.5e).",
        )),
        // `CreateGradingToneTransform` (src/OpenColorIO/ops/gradingtone/GradingToneOp.cpp:
        // 217-230 @ v2.5.2) comes with `GradingToneTransform` (WP 5.5d).
        OpData::GradingTone(_) => Err(Exception::new(
            "CreateGradingToneTransform is not ported yet (WP 5.5d).",
        )),
        OpData::ExposureContrast(_) => create_exposure_contrast_transform(group, op),
        OpData::GradingPrimary(_) => create_grading_primary_transform(group, op),
        // No op holds a reference (the file readers replace it with the file's ops), and the
        // no-op types returned above.
        OpData::Reference(_) | OpData::NoOp(_) => {
            unreachable!("an op of a reference or no-op data has no transform")
        }
    }
}

#[cfg(test)]
#[path = "transform_tests.rs"]
mod tests;
