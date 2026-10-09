// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The color space transform: a port of the class in
//! `src/OpenColorIO/transforms/ColorSpaceTransform.cpp` @ v2.5.2, with its op builder
//! (`BuildColorSpaceOps`, the reference-space conversions) and its `CollectContextVariables`
//! (and those of color spaces and named transforms).

use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{
    ColorSpaceDirection, ReferenceSpaceType, TransformDirection, ViewTransformDirection,
    combine_transform_directions, transform_direction_to_string,
};
use ocio_ops::ops::allocation::allocation_op::AllocationData;
use ocio_ops::ops::noop::no_ops::create_gpu_allocation_no_op;
use ocio_ops::utils::string_utils::{c_str, compare};

use crate::color_space::ColorSpace;
use crate::config::Config;
use crate::context::Context;
use crate::context_variable_utils::collect_context_variables;
use crate::named_transform::{NamedTransform, get_transform};
use crate::transform::{build_ops, put_bool, put_c_str, validate_direction};

/// A conversion from one of a config's color spaces to another, by name.
///
/// A copy is upstream's `createEditableCopy`. Upstream gives the class no `equals`.
///
/// Port of `ColorSpaceTransform` and its `Impl` (include/OpenColorIO/OpenColorTransforms.h:
/// 333-371, src/OpenColorIO/transforms/ColorSpaceTransform.cpp:18-136 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct ColorSpaceTransform {
    /// `m_dir`.
    dir: TransformDirection,
    /// `m_src`.
    src: Vec<u8>,
    /// `m_dst`.
    dst: Vec<u8>,
    /// `m_dataBypass`.
    data_bypass: bool,
}

impl Default for ColorSpaceTransform {
    fn default() -> Self {
        Self::new()
    }
}

impl ColorSpaceTransform {
    /// A forward transform with empty names, which bypasses data color spaces.
    ///
    /// Port of `ColorSpaceTransform::Create` and `Impl` (ColorSpaceTransform.cpp:18-59 @
    /// v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> ColorSpaceTransform {
        ColorSpaceTransform {
            dir: TransformDirection::Forward,
            src: Vec::new(),
            dst: Vec::new(),
            data_bypass: true,
        }
    }

    /// Port of `ColorSpaceTransform::getDirection` (ColorSpaceTransform.cpp:74-77 @ v2.5.2).
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        self.dir
    }

    /// Port of `ColorSpaceTransform::setDirection` (ColorSpaceTransform.cpp:79-82 @ v2.5.2).
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.dir = dir;
    }

    /// Checks the direction ("ColorSpaceTransform validation failed: " and its error), then
    /// that both names are set.
    ///
    /// Port of `ColorSpaceTransform::validate` (ColorSpaceTransform.cpp:84-106 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        if let Err(ex) = validate_direction(self.dir) {
            return Err(Exception::new(format!(
                "ColorSpaceTransform validation failed: {}",
                ex.message()
            )));
        }

        if self.src.is_empty() {
            return Err(Exception::new(
                "ColorSpaceTransform: empty source color space name.",
            ));
        }

        if self.dst.is_empty() {
            return Err(Exception::new(
                "ColorSpaceTransform: empty destination color space name.",
            ));
        }
        Ok(())
    }

    /// The source color space's name.
    ///
    /// Port of `ColorSpaceTransform::getSrc` (ColorSpaceTransform.cpp:108-111 @ v2.5.2).
    #[doc(alias = "getSrc")]
    pub fn src(&self) -> &[u8] {
        &self.src
    }

    /// Sets the source color space's name, up to its first NUL (a C string).
    ///
    /// Port of `ColorSpaceTransform::setSrc` (ColorSpaceTransform.cpp:113-116 @ v2.5.2). Its
    /// null pointer, which sets an empty name, is an empty slice here.
    #[doc(alias = "setSrc")]
    pub fn set_src(&mut self, src: impl AsRef<[u8]>) {
        self.src = c_str(src.as_ref()).to_vec();
    }

    /// The destination color space's name.
    ///
    /// Port of `ColorSpaceTransform::getDst` (ColorSpaceTransform.cpp:118-121 @ v2.5.2).
    #[doc(alias = "getDst")]
    pub fn dst(&self) -> &[u8] {
        &self.dst
    }

    /// Sets the destination color space's name, up to its first NUL (a C string).
    ///
    /// Port of `ColorSpaceTransform::setDst` (ColorSpaceTransform.cpp:123-126 @ v2.5.2).
    #[doc(alias = "setDst")]
    pub fn set_dst(&mut self, dst: impl AsRef<[u8]>) {
        self.dst = c_str(dst.as_ref()).to_vec();
    }

    /// Whether data color spaces are left unprocessed (the default).
    ///
    /// Port of `ColorSpaceTransform::getDataBypass` (ColorSpaceTransform.cpp:128-131 @ v2.5.2).
    #[doc(alias = "getDataBypass")]
    pub fn data_bypass(&self) -> bool {
        self.data_bypass
    }

    /// Port of `ColorSpaceTransform::setDataBypass` (ColorSpaceTransform.cpp:133-136 @ v2.5.2).
    #[doc(alias = "setDataBypass")]
    pub fn set_data_bypass(&mut self, bypass: bool) {
        self.data_bypass = bypass;
    }

    /// Writes the transform's text to `os`.
    ///
    /// Upstream prints `dataBypass=0` straight after the destination's name, without a
    /// separator (docs/improvements.md, I-120), and a C++ `bool` as `0` or `1`.
    ///
    /// Port of `operator<<(std::ostream &, const ColorSpaceTransform &)`
    /// (ColorSpaceTransform.cpp:138-151 @ v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        os.put_str("<ColorSpaceTransform ");
        os.put_str("direction=");
        os.put_str(transform_direction_to_string(self.direction()));
        os.put_str(", ");
        os.put_str("src=");
        put_c_str(os, self.src());
        os.put_str(", ");
        os.put_str("dst=");
        put_c_str(os, self.dst());
        let bypass = self.data_bypass();
        if !bypass {
            os.put_str("dataBypass=");
            put_bool(os, bypass);
        }
        os.put_str(">");
    }
}

impl fmt::Display for ColorSpaceTransform {
    /// `<ColorSpaceTransform direction=<dir>, src=<src>, dst=<dst>[dataBypass=0]>`.
    ///
    /// Port of `operator<<(std::ostream &, const ColorSpaceTransform &)`
    /// (ColorSpaceTransform.cpp:138-151 @ v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(&os.to_string_lossy())
    }
}

/// "Color space '<cs>' could not be found."
///
/// Port of `ThrowMissingCS` (src/OpenColorIO/transforms/ColorSpaceTransform.cpp:157-162 @
/// v2.5.2).
fn missing_cs(cs: &[u8]) -> Exception {
    let mut os = b"Color space '".to_vec();
    os.extend_from_slice(cs);
    os.extend_from_slice(b"' could not be found.");
    Exception::new(os)
}

/// Appends the ops of `color_space_transform` in the direction `dir`: from its source to its
/// destination (each a color space or a named transform of `config`, by its name with the
/// context's variables resolved), or the reverse.
///
/// Port of `BuildColorSpaceOps(OpRcPtrVec &, const Config &, const ConstContextRcPtr &, const
/// ColorSpaceTransform &, TransformDirection)` (src/OpenColorIO/transforms/
/// ColorSpaceTransform.cpp:164-220 @ v2.5.2).
pub(crate) fn build_color_space_ops(
    ops: &mut OpVec,
    config: &Config,
    context: &Context,
    color_space_transform: &ColorSpaceTransform,
    dir: TransformDirection,
) -> Result<()> {
    let combined_dir = combine_transform_directions(dir, color_space_transform.direction());
    let forward = combined_dir == TransformDirection::Forward;

    let trans_src_name = color_space_transform.src();
    let trans_dst_name = color_space_transform.dst();
    let src_name = if forward {
        trans_src_name
    } else {
        trans_dst_name
    };
    let dst_name = if forward {
        trans_dst_name
    } else {
        trans_src_name
    };

    let src = config.color_space(context.resolve_string_var(src_name));
    let dst = config.color_space(context.resolve_string_var(dst_name));

    let mut src_named_transform = None;
    let mut dst_named_transform = None;

    if src.is_none() {
        src_named_transform = config.named_transform(context.resolve_string_var(src_name));
        if src_named_transform.is_none() {
            return Err(missing_cs(src_name));
        }
    }
    if dst.is_none() {
        dst_named_transform = config.named_transform(context.resolve_string_var(dst_name));
        if dst_named_transform.is_none() {
            return Err(missing_cs(dst_name));
        }
    }

    // There are 4 cases:
    // * (srcNamedTransform, dstNamedTransform): srcNamedTransform->forward +
    //                                           dstNamedTransform->inverse.
    // * (srcNamedTransform, dst): srcNamedTransform->forward (dst ignored).
    // * (src, dstNamedTransform): dstNamedTransform->inverse (src ignored).
    // * (src, dst): full color space transform.

    if src_named_transform.is_some() || dst_named_transform.is_some() {
        let tr = get_transform(src_named_transform, dst_named_transform)?;
        return build_ops(ops, config, context, &tr, TransformDirection::Forward);
    }

    build_color_space_ops_between(
        ops,
        config,
        context,
        src.expect("found above"),
        dst.expect("found above"),
        color_space_transform.data_bypass(),
    )
}

/// Whether two color spaces are the same (by name: one may be a copy) or in the same non-empty
/// equality group.
///
/// Port of `AreColorSpacesInSameEqualityGroup` (src/OpenColorIO/transforms/
/// ColorSpaceTransform.cpp:224-235 @ v2.5.2).
fn are_color_spaces_in_same_equality_group(csa: &ColorSpace, csb: &ColorSpace) -> bool {
    // See issue #602. Using names in case one of the color space would be a copy.
    if compare(csa.name(), csb.name()) {
        return true;
    }

    let a = csa.equality_group();
    let b = csb.equality_group();

    if !a.is_empty() {
        return a == b;
    }
    false
}

/// Appends the ops from the color space `src_color_space` to `dst_color_space`: none when they
/// are equal or, with `data_bypass`, when either is data; else to the reference space, between
/// the reference spaces, and from it.
///
/// Port of `BuildColorSpaceOps(OpRcPtrVec &, const Config &, const ConstContextRcPtr &, const
/// ConstColorSpaceRcPtr &, const ConstColorSpaceRcPtr &, bool)` (src/OpenColorIO/transforms/
/// ColorSpaceTransform.cpp:239-276 @ v2.5.2). Its errors for null color spaces can't happen.
pub(crate) fn build_color_space_ops_between(
    ops: &mut OpVec,
    config: &Config,
    context: &Context,
    src_color_space: &ColorSpace,
    dst_color_space: &ColorSpace,
    data_bypass: bool,
) -> Result<()> {
    if are_color_spaces_in_same_equality_group(src_color_space, dst_color_space) {
        return Ok(());
    }
    if data_bypass && (dst_color_space.is_data() || src_color_space.is_data()) {
        return Ok(());
    }

    // Consider dt8 -> vd8?
    // One would have to explode the srcColorSpace->getTransform(COLORSPACE_DIR_TO_REFERENCE);
    // result, and walk through it step by step.  If the dstColorspace family were
    // ever encountered in transit, we'd want to short circuit the result.

    // Go from the srcColorSpace to the reference space.
    build_color_space_to_reference_ops(ops, config, context, src_color_space, data_bypass)?;

    // There are two possible reference spaces, the main (scene-referred) one and the
    // display-referred one.  If the src and dst use different reference spaces, use the
    // default ViewTransform to convert between them.
    build_reference_conversion_ops(
        ops,
        config,
        context,
        src_color_space.reference_space_type(),
        dst_color_space.reference_space_type(),
    )?;

    // Go from the reference space to dstColorSpace.
    build_color_space_from_reference_ops(ops, config, context, dst_color_space, data_bypass)
}

/// The allocation of `color_space`, for its GPU allocation no-op.
fn allocation_of(color_space: &ColorSpace) -> AllocationData {
    AllocationData {
        allocation: color_space.allocation(),
        vars: color_space.allocation_vars().to_vec(),
    }
}

/// Appends the ops from `src_color_space` to its reference space: its allocation's no-op, then
/// its transform to the reference, or the inverse of its transform from it (or nothing). None
/// with `data_bypass` for a data color space.
///
/// Port of `BuildColorSpaceToReferenceOps` (src/OpenColorIO/transforms/
/// ColorSpaceTransform.cpp:278-311 @ v2.5.2). Its error for a null color space can't happen.
pub(crate) fn build_color_space_to_reference_ops(
    ops: &mut OpVec,
    config: &Config,
    context: &Context,
    src_color_space: &ColorSpace,
    data_bypass: bool,
) -> Result<()> {
    if data_bypass && src_color_space.is_data() {
        return Ok(());
    }

    create_gpu_allocation_no_op(ops, &allocation_of(src_color_space));

    // Go to the reference space, either by using:
    // * cs->ref in the forward direction.
    // * ref->cs in the inverse direction.
    if let Some(t) = src_color_space.transform(ColorSpaceDirection::ToReference) {
        build_ops(ops, config, context, t, TransformDirection::Forward)?;
    } else if let Some(t) = src_color_space.transform(ColorSpaceDirection::FromReference) {
        build_ops(ops, config, context, t, TransformDirection::Inverse)?;
    }
    // Otherwise, both are not defined so its a no-op. This is not an error condition.
    Ok(())
}

/// Appends the ops from the reference space to `dst_color_space`: its transform from the
/// reference, or the inverse of its transform to it (or nothing), then its allocation's no-op.
/// None with `data_bypass` for a data color space.
///
/// Port of `BuildColorSpaceFromReferenceOps` (src/OpenColorIO/transforms/
/// ColorSpaceTransform.cpp:313-345 @ v2.5.2). Its error for a null color space can't happen.
pub(crate) fn build_color_space_from_reference_ops(
    ops: &mut OpVec,
    config: &Config,
    context: &Context,
    dst_color_space: &ColorSpace,
    data_bypass: bool,
) -> Result<()> {
    if data_bypass && dst_color_space.is_data() {
        return Ok(());
    }

    // Go from the reference space, either by using:
    // * ref->cs in the forward direction.
    // * cs->ref in the inverse direction.
    if let Some(t) = dst_color_space.transform(ColorSpaceDirection::FromReference) {
        build_ops(ops, config, context, t, TransformDirection::Forward)?;
    } else if let Some(t) = dst_color_space.transform(ColorSpaceDirection::ToReference) {
        build_ops(ops, config, context, t, TransformDirection::Inverse)?;
    }
    // Otherwise, both are not defined so its a no-op. This is not an error condition.

    create_gpu_allocation_no_op(ops, &allocation_of(dst_color_space));
    Ok(())
}

/// Appends the ops between the reference spaces `src_reference_space` and
/// `dst_reference_space`: none when they are the same, else the config's default view transform
/// (scene to display: its transform from the scene reference, or the inverse of its transform to
/// it; display to scene the other way), or an error without one.
///
/// Port of `BuildReferenceConversionOps` (src/OpenColorIO/transforms/
/// ColorSpaceTransform.cpp:347-394 @ v2.5.2).
pub(crate) fn build_reference_conversion_ops(
    ops: &mut OpVec,
    config: &Config,
    context: &Context,
    src_reference_space: ReferenceSpaceType,
    dst_reference_space: ReferenceSpaceType,
) -> Result<()> {
    if src_reference_space != dst_reference_space {
        let Some(view) = config.default_scene_to_display_view_transform() else {
            // Can not be the case for a valid config.
            return Err(Exception::new(
                "There is no view transform between the main scene-referred space and the \
                 display-referred space.",
            ));
        };
        let from = view.transform(ViewTransformDirection::FromReference);
        let to = view.transform(ViewTransformDirection::ToReference);
        // convert scene-referred to display-referred, or display-referred to scene-referred
        let (first, second) = if src_reference_space == ReferenceSpaceType::Scene {
            (from, to)
        } else {
            (to, from)
        };
        if let Some(t) = first {
            build_ops(ops, config, context, t, TransformDirection::Forward)?;
        } else if let Some(t) = second {
            build_ops(ops, config, context, t, TransformDirection::Inverse)?;
        }
    }
    Ok(())
}

/// Whether the named transform `nt` uses context variables, which it adds to
/// `used_context_vars`: its forward and inverse transforms'.
///
/// Port of `CollectContextVariables(const Config &, const Context &, ConstNamedTransformRcPtr &,
/// ContextRcPtr &)` (src/OpenColorIO/transforms/ColorSpaceTransform.cpp:396-419 @ v2.5.2).
pub(crate) fn collect_named_transform_context_variables(
    config: &Config,
    context: &Context,
    nt: Option<&NamedTransform>,
    used_context_vars: &mut Context,
) -> Result<bool> {
    let mut found_context_vars = false;

    if let Some(nt) = nt {
        for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
            if let Some(t) = nt.transform(dir)
                && collect_context_variables(config, context, t, used_context_vars)?
            {
                found_context_vars = true;
            }
        }
    }

    Ok(found_context_vars)
}

/// Whether the color space `cs` uses context variables, which it adds to `used_context_vars`:
/// its transforms to and from the reference.
///
/// Port of `CollectContextVariables(const Config &, const Context &, ConstColorSpaceRcPtr &,
/// ContextRcPtr &)` (src/OpenColorIO/transforms/ColorSpaceTransform.cpp:421-444 @ v2.5.2).
pub(crate) fn collect_color_space_context_variables(
    config: &Config,
    context: &Context,
    cs: Option<&ColorSpace>,
    used_context_vars: &mut Context,
) -> Result<bool> {
    let mut found_context_vars = false;

    if let Some(cs) = cs {
        for dir in [
            ColorSpaceDirection::ToReference,
            ColorSpaceDirection::FromReference,
        ] {
            if let Some(t) = cs.transform(dir)
                && collect_context_variables(config, context, t, used_context_vars)?
            {
                found_context_vars = true;
            }
        }
    }

    Ok(found_context_vars)
}

/// Whether `tr` uses context variables, which it adds to `used_context_vars`: those its source
/// and destination names resolve with, and those of the color spaces or named transforms they
/// name. It may count variables the direction in use doesn't need.
///
/// Port of `CollectContextVariables(const Config &, const Context &, const ColorSpaceTransform &,
/// ContextRcPtr &)` (src/OpenColorIO/transforms/ColorSpaceTransform.cpp:446-503 @ v2.5.2).
pub(crate) fn collect_color_space_transform_context_variables(
    config: &Config,
    context: &Context,
    tr: &ColorSpaceTransform,
    used_context_vars: &mut Context,
) -> Result<bool> {
    let mut found_context_vars = false;

    // NB: The search could return false positive but should not miss anything i.e. it looks
    // for context variables in both directions even if only one will be used.

    let src_name = context.resolve_string_var_with_used(tr.src(), used_context_vars);
    if src_name.as_slice() != c_str(tr.src()) {
        found_context_vars = true;
    }

    let dst_name = context.resolve_string_var_with_used(tr.dst(), used_context_vars);
    if dst_name.as_slice() != c_str(tr.dst()) {
        found_context_vars = true;
    }

    for name in [&src_name, &dst_name] {
        let found = match config.color_space(name) {
            Some(cs) => {
                collect_color_space_context_variables(config, context, Some(cs), used_context_vars)?
            }
            None => collect_named_transform_context_variables(
                config,
                context,
                config.named_transform(name),
                used_context_vars,
            )?,
        };
        if found {
            found_context_vars = true;
        }
    }

    Ok(found_context_vars)
}

#[cfg(test)]
#[path = "color_space_transform_tests.rs"]
mod tests;
