// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The display view transform: a port of the class in
//! `src/OpenColorIO/transforms/DisplayViewTransform.cpp` @ v2.5.2. Its op builder
//! (`BuildDisplayOps`) and its `CollectContextVariables` come with WP 3.2.

use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{
    ReferenceSpaceType, TransformDirection, ViewTransformDirection, combine_transform_directions,
    transform_direction_to_string,
};
use ocio_ops::utils::string_utils::c_str;

use crate::color_space::ColorSpace;
use crate::config::Config;
use crate::context::Context;
use crate::context_variable_utils::collect_context_variables;
use crate::display::View;
use crate::look::collect_look_context_variables;
use crate::look_parse::LookParseResult;
use crate::named_transform::NamedTransform;
use crate::transform::{build_ops, put_bool, put_c_str, validate_direction};
use crate::transforms::color_space_transform::{
    build_color_space_from_reference_ops, build_color_space_ops_between,
    build_color_space_to_reference_ops, build_reference_conversion_ops,
    collect_color_space_context_variables,
};
use crate::transforms::look_transform::{build_look_ops_from, looks_result_color_space};
use crate::view_transform::ViewTransform;

/// A conversion from a color space to one of a config's displays through one of its views,
/// by name, with the view's looks.
///
/// A copy is upstream's `createEditableCopy`. Upstream gives the class no `equals`.
///
/// Port of `DisplayViewTransform` and its `Impl` (include/OpenColorIO/OpenColorTransforms.h:
/// 376-425, src/OpenColorIO/transforms/DisplayViewTransform.cpp:18-152 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct DisplayViewTransform {
    /// `m_dir`.
    dir: TransformDirection,
    /// `m_src`.
    src: Vec<u8>,
    /// `m_display`.
    display: Vec<u8>,
    /// `m_view`.
    view: Vec<u8>,
    /// `m_looksBypass`.
    looks_bypass: bool,
    /// `m_dataBypass`.
    data_bypass: bool,
}

impl Default for DisplayViewTransform {
    fn default() -> Self {
        Self::new()
    }
}

impl DisplayViewTransform {
    /// A forward transform with empty names, which applies the view's looks and bypasses data
    /// color spaces.
    ///
    /// Port of `DisplayViewTransform::Create` and `Impl` (DisplayViewTransform.cpp:18-50 @
    /// v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> DisplayViewTransform {
        DisplayViewTransform {
            dir: TransformDirection::Forward,
            src: Vec::new(),
            display: Vec::new(),
            view: Vec::new(),
            looks_bypass: false,
            data_bypass: true,
        }
    }

    /// Port of `DisplayViewTransform::getDirection` (DisplayViewTransform.cpp:65-68 @ v2.5.2).
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        self.dir
    }

    /// Port of `DisplayViewTransform::setDirection` (DisplayViewTransform.cpp:70-73 @ v2.5.2).
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.dir = dir;
    }

    /// Checks the direction ("DisplayViewTransform validation failed: " and its error), then
    /// that the source, the display and the view are set, in that order.
    ///
    /// Port of `DisplayViewTransform::validate` (DisplayViewTransform.cpp:75-102 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        if let Err(ex) = validate_direction(self.dir) {
            return Err(Exception::new(format!(
                "DisplayViewTransform validation failed: {}",
                ex.message()
            )));
        }

        if self.src.is_empty() {
            return Err(Exception::new(
                "DisplayViewTransform: empty source color space name.",
            ));
        }

        if self.display.is_empty() {
            return Err(Exception::new("DisplayViewTransform: empty display name."));
        }

        if self.view.is_empty() {
            return Err(Exception::new("DisplayViewTransform: empty view name."));
        }
        Ok(())
    }

    /// Sets the source color space's name, up to its first NUL (a C string).
    ///
    /// Port of `DisplayViewTransform::setSrc` (DisplayViewTransform.cpp:104-107 @ v2.5.2). Its
    /// null pointer, which sets an empty name, is an empty slice here.
    #[doc(alias = "setSrc")]
    pub fn set_src(&mut self, name: impl AsRef<[u8]>) {
        self.src = c_str(name.as_ref()).to_vec();
    }

    /// The source color space's name.
    ///
    /// Port of `DisplayViewTransform::getSrc` (DisplayViewTransform.cpp:109-112 @ v2.5.2).
    #[doc(alias = "getSrc")]
    pub fn src(&self) -> &[u8] {
        &self.src
    }

    /// Sets the display's name, up to its first NUL.
    ///
    /// Port of `DisplayViewTransform::setDisplay` (DisplayViewTransform.cpp:114-117 @ v2.5.2).
    #[doc(alias = "setDisplay")]
    pub fn set_display(&mut self, display: impl AsRef<[u8]>) {
        self.display = c_str(display.as_ref()).to_vec();
    }

    /// The display's name.
    ///
    /// Port of `DisplayViewTransform::getDisplay` (DisplayViewTransform.cpp:119-122 @ v2.5.2).
    #[doc(alias = "getDisplay")]
    pub fn display(&self) -> &[u8] {
        &self.display
    }

    /// Sets the view's name, up to its first NUL.
    ///
    /// Port of `DisplayViewTransform::setView` (DisplayViewTransform.cpp:124-127 @ v2.5.2).
    #[doc(alias = "setView")]
    pub fn set_view(&mut self, view: impl AsRef<[u8]>) {
        self.view = c_str(view.as_ref()).to_vec();
    }

    /// The view's name.
    ///
    /// Port of `DisplayViewTransform::getView` (DisplayViewTransform.cpp:129-132 @ v2.5.2).
    #[doc(alias = "getView")]
    pub fn view(&self) -> &[u8] {
        &self.view
    }

    /// Whether the view's looks are skipped (not the default).
    ///
    /// Port of `DisplayViewTransform::setLooksBypass` (DisplayViewTransform.cpp:134-137 @
    /// v2.5.2).
    #[doc(alias = "setLooksBypass")]
    pub fn set_looks_bypass(&mut self, bypass: bool) {
        self.looks_bypass = bypass;
    }

    /// Port of `DisplayViewTransform::getLooksBypass` (DisplayViewTransform.cpp:139-142 @
    /// v2.5.2).
    #[doc(alias = "getLooksBypass")]
    pub fn looks_bypass(&self) -> bool {
        self.looks_bypass
    }

    /// Whether data color spaces are left unprocessed (the default).
    ///
    /// Port of `DisplayViewTransform::setDataBypass` (DisplayViewTransform.cpp:144-147 @
    /// v2.5.2).
    #[doc(alias = "setDataBypass")]
    pub fn set_data_bypass(&mut self, bypass: bool) {
        self.data_bypass = bypass;
    }

    /// Port of `DisplayViewTransform::getDataBypass` (DisplayViewTransform.cpp:149-152 @
    /// v2.5.2).
    #[doc(alias = "getDataBypass")]
    pub fn data_bypass(&self) -> bool {
        self.data_bypass
    }

    /// Writes the transform's text to `os`.
    ///
    /// Upstream ends the view with `", "` and starts each bypass with another `", "`, so the
    /// text has `", >"` at its end or `", , "` before a bypass (docs/improvements.md, I-121);
    /// a C++ `bool` prints as `0` or `1`.
    ///
    /// Port of `operator<<(std::ostream &, const DisplayViewTransform &)`
    /// (DisplayViewTransform.cpp:154-171 @ v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        os.put_str("<DisplayViewTransform ");
        os.put_str("direction=");
        os.put_str(transform_direction_to_string(self.direction()));
        os.put_str(", ");
        os.put_str("src=");
        put_c_str(os, self.src());
        os.put_str(", ");
        os.put_str("display=");
        put_c_str(os, self.display());
        os.put_str(", ");
        os.put_str("view=");
        put_c_str(os, self.view());
        os.put_str(", ");
        if self.looks_bypass() {
            os.put_str(", looksBypass=");
            put_bool(os, self.looks_bypass());
        }
        if !self.data_bypass() {
            os.put_str(", dataBypass=");
            put_bool(os, self.data_bypass());
        }
        os.put_str(">");
    }
}

impl fmt::Display for DisplayViewTransform {
    /// `<DisplayViewTransform direction=<dir>, src=<src>, display=<display>, view=<view>, [,
    /// looksBypass=1][, dataBypass=0]>`.
    ///
    /// Port of `operator<<(std::ostream &, const DisplayViewTransform &)`
    /// (DisplayViewTransform.cpp:154-171 @ v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(&os.to_string_lossy())
    }
}

/// "View transform named '<name>' needs either a transform from or to reference."
fn view_transform_without_transform(view_transform: &ViewTransform) -> Exception {
    let mut os = b"View transform named '".to_vec();
    os.extend_from_slice(view_transform.name());
    os.extend_from_slice(b"' needs either a transform from or to reference.");
    Exception::new(os)
}

/// The reference space type of the source color space of a view transform, which upstream reads
/// through a pointer that is null when the looks' first option names no look and so leaves no
/// color space (docs/improvements.md, U-57): an error instead.
fn source_reference_space(source_cs: Option<&ColorSpace>) -> Result<ReferenceSpaceType> {
    source_cs
        .map(ColorSpace::reference_space_type)
        .ok_or_else(|| {
            Exception::new(
                "DisplayViewTransform error. The looks leave no color space to apply the view \
             transform from.",
            )
        })
}

/// Appends the ops from `source_cs` to the display color space `display_cs` through
/// `view_transform`: to the source's reference space, to the view transform's, the view
/// transform, from the display reference space.
///
/// Port of `BuildSourceToDisplay` (src/OpenColorIO/transforms/DisplayViewTransform.cpp:
/// 177-218 @ v2.5.2).
fn build_source_to_display(
    ops: &mut OpVec,
    config: &Config,
    context: &Context,
    source_cs: &ColorSpace,
    view_transform: &ViewTransform,
    display_cs: Option<&ColorSpace>,
    data_bypass: bool,
) -> Result<()> {
    // DisplayCS is display-referred.

    // Convert the current color space to its reference space.
    build_color_space_to_reference_ops(ops, config, context, Some(source_cs), data_bypass)?;

    // If necessary, convert to the type of reference space used by the view transform.
    let vt_ref = view_transform.reference_space_type();
    let cur_cs_ref = source_cs.reference_space_type();
    build_reference_conversion_ops(ops, config, context, cur_cs_ref, vt_ref)?;

    // Apply view transform.
    if let Some(t) = view_transform.transform(ViewTransformDirection::FromReference) {
        build_ops(ops, config, context, t, TransformDirection::Forward)?;
    } else if let Some(t) = view_transform.transform(ViewTransformDirection::ToReference) {
        build_ops(ops, config, context, t, TransformDirection::Inverse)?;
    } else {
        return Err(view_transform_without_transform(view_transform));
    }

    // Convert from the display-referred reference space to the displayCS.
    build_color_space_from_reference_ops(ops, config, context, display_cs, data_bypass)
}

/// Appends the ops from the display color space `display_cs` to `source_cs` through the inverse
/// of `view_transform`.
///
/// Port of `BuildDisplayToSource` (src/OpenColorIO/transforms/DisplayViewTransform.cpp:
/// 222-262 @ v2.5.2).
fn build_display_to_source(
    ops: &mut OpVec,
    config: &Config,
    context: &Context,
    display_cs: Option<&ColorSpace>,
    view_transform: &ViewTransform,
    source_cs: Option<&ColorSpace>,
    data_bypass: bool,
) -> Result<()> {
    // Convert to the display-referred reference space from the displayColorSpace.
    build_color_space_to_reference_ops(ops, config, context, display_cs, data_bypass)?;

    // Apply view transform inverted.
    if let Some(t) = view_transform.transform(ViewTransformDirection::ToReference) {
        build_ops(ops, config, context, t, TransformDirection::Forward)?;
    } else if let Some(t) = view_transform.transform(ViewTransformDirection::FromReference) {
        build_ops(ops, config, context, t, TransformDirection::Inverse)?;
    } else {
        return Err(view_transform_without_transform(view_transform));
    }

    // If necessary, convert from the type of reference space used by the view transform to the
    // reference space of the source color space.
    let vt_ref = view_transform.reference_space_type();
    let in_cs_ref = source_reference_space(source_cs)?;
    build_reference_conversion_ops(ops, config, context, vt_ref, in_cs_ref)?;

    // Convert from the reference space back to the source color space.
    build_color_space_from_reference_ops(ops, config, context, source_cs, data_bypass)
}

/// Appends the named transform of a view, forward, then the conversion from the display
/// reference space to `display_cs`.
///
/// Port of `BuildNamedTransformToDisplay` (src/OpenColorIO/transforms/
/// DisplayViewTransform.cpp:264-277 @ v2.5.2).
fn build_named_transform_to_display(
    ops: &mut OpVec,
    config: &Config,
    context: &Context,
    view_named_transform: &NamedTransform,
    display_cs: Option<&ColorSpace>,
    data_bypass: bool,
) -> Result<()> {
    // Apply view named transform.
    let transform =
        NamedTransform::get_transform(view_named_transform, TransformDirection::Forward)?;
    build_ops(
        ops,
        config,
        context,
        &transform,
        TransformDirection::Forward,
    )?;

    // Convert from the display-referred reference space to the displayCS.
    build_color_space_from_reference_ops(ops, config, context, display_cs, data_bypass)
}

/// Appends the conversion from `display_cs` to the display reference space, then the named
/// transform of a view, inverse.
///
/// Port of `BuildDisplayToNamedTransform` (src/OpenColorIO/transforms/
/// DisplayViewTransform.cpp:279-292 @ v2.5.2).
fn build_display_to_named_transform(
    ops: &mut OpVec,
    config: &Config,
    context: &Context,
    display_cs: Option<&ColorSpace>,
    view_named_transform: &NamedTransform,
    data_bypass: bool,
) -> Result<()> {
    // Convert to the display-referred reference space from the displayColorSpace.
    build_color_space_to_reference_ops(ops, config, context, display_cs, data_bypass)?;

    // Apply view named transform.
    let transform =
        NamedTransform::get_transform(view_named_transform, TransformDirection::Inverse)?;
    build_ops(
        ops,
        config,
        context,
        &transform,
        TransformDirection::Forward,
    )
}

/// "DisplayViewTransform error." and `rest`.
fn dvt_error(rest: &[&[u8]]) -> Exception {
    let mut os = b"DisplayViewTransform error.".to_vec();
    for part in rest {
        os.extend_from_slice(part);
    }
    Exception::new(os)
}

/// Appends the ops of `display_view_transform` in the direction `dir`: from its source color
/// space, through the view's looks, to the view's color space or through its view transform (or
/// named transform) to its display color space; inverse, the other way.
///
/// Port of `BuildDisplayOps` (src/OpenColorIO/transforms/DisplayViewTransform.cpp:295-528 @
/// v2.5.2).
pub(crate) fn build_display_ops(
    ops: &mut OpVec,
    config: &Config,
    context: &Context,
    display_view_transform: &DisplayViewTransform,
    dir: TransformDirection,
) -> Result<()> {
    // There are two ways of specifying a DisplayViewTransform: either with a colorspace, or with
    // a view_transform plus display_colorspace.  It is permitted to substitute a named_transform
    // for either the colorspace or the view_transform.

    // Most of the error checking below is also done in Config::validate(), but it's useful to
    // do here since there is no guarantee that validate will be called.

    // Validate src color space.
    let src_color_space_name = c_str(display_view_transform.src());
    let Some(src_color_space) = config.color_space(src_color_space_name) else {
        return Err(if src_color_space_name.is_empty() {
            dvt_error(&[b" The source color space is unspecified."])
        } else {
            dvt_error(&[
                b" Cannot find source color space named '",
                src_color_space_name,
                b"'.",
            ])
        });
    };

    let display = c_str(display_view_transform.display());
    if config.num_views(display) == 0 {
        return Err(dvt_error(&[b" Display '", display, b"' not found."]));
    }
    let view = c_str(display_view_transform.view());

    // Get the view transform if any: if it exists, it can be a view transform or a named
    // transform.
    let view_transform_name = config.display_view_transform_name(display, view);
    let mut view_transform = None;
    let mut view_named_transform = None;
    if !view_transform_name.is_empty() {
        view_transform = config.view_transform(view_transform_name);
        if view_transform.is_none() {
            view_named_transform = config.named_transform(view_transform_name);
            if view_named_transform.is_none() {
                // Config::validate catch this.
                return Err(dvt_error(&[
                    b" The view transform '",
                    view_transform_name,
                    b"' is neither a view transform nor a named transform.",
                ]));
            }
        }
    }

    // Get the color space associated to the (display, view) pair.
    // (Returns an empty string if the view does not exist.  This is trapped below.)
    let cs_name = config.display_view_color_space_name(display, view);

    // A shared view containing a view transform may set the color space to <USE_DISPLAY_NAME>,
    // in which case we look for a display color space with the same name as the display.
    let display_color_space_name = if View::use_display_name(cs_name) {
        display
    } else {
        c_str(cs_name)
    };

    let display_color_space = config.color_space(display_color_space_name);
    let mut cs_named_transform = None;
    if display_color_space.is_none() {
        if display_color_space_name.is_empty() {
            return Err(dvt_error(&[
                b" The display '",
                display,
                b"' does not have view '",
                view,
                b"'.",
            ]));
        }

        // Note: If there is a view transform, the display color space isn't allowed to be a
        // named transform, hence the error message only says "color space".
        if view_transform.is_some() || view_named_transform.is_some() {
            return Err(dvt_error(&[
                b" The view '",
                view,
                b"' refers to a display color space '",
                display_color_space_name,
                b"' that can't be found.",
            ]));
        }

        // At this point, there is no view transform, so displayColorSpaceName is typically
        // from the "colorspace" attribute of the View, not the "display_colorspace". The
        // former may be a named transform but the latter may not (since a view transform
        // is present.)
        cs_named_transform = config.named_transform(display_color_space_name);
        if cs_named_transform.is_none() {
            return Err(dvt_error(&[
                b" Cannot find color space or named transform with name '",
                display_color_space_name,
                b"'.",
            ]));
        }
    }

    // By default, data color spaces are not processed.
    let data_bypass = display_view_transform.data_bypass();
    let src_data = src_color_space.is_data();
    let display_data = display_color_space.is_some_and(ColorSpace::is_data);
    if data_bypass && (src_data || display_data) {
        return Ok(());
    }

    // Get looks to be applied, if specified.
    let mut looks = LookParseResult::default();
    if !display_view_transform.looks_bypass() {
        looks.parse(config.display_view_looks(display, view))?;
    }

    // Now that all the inputs are found and validated, the following code builds the list of
    // ops for the forward or the inverse direction.

    let combined_dir = combine_transform_directions(dir, display_view_transform.direction());
    match combined_dir {
        TransformDirection::Forward => {
            // Start from src color space.
            let mut current_cs = Some(src_color_space);

            // Apply looks if needed.
            if !looks.is_empty() {
                // Note that this updates the currentCS to be the process space of the last look
                // applied.
                build_look_ops_from(ops, &mut current_cs, false, config, context, &looks)?;
            }
            let current_cs = current_cs.expect("the looks only replace it");

            if let Some(cs_named_transform) = cs_named_transform {
                // Ignore currentCS.  The forward direction NamedTransform is used for the
                // forward direction DisplayViewTransform.
                let transform =
                    NamedTransform::get_transform(cs_named_transform, TransformDirection::Forward)?;
                build_ops(
                    ops,
                    config,
                    context,
                    &transform,
                    TransformDirection::Forward,
                )?;
            } else if let Some(view_named_transform) = view_named_transform {
                build_named_transform_to_display(
                    ops,
                    config,
                    context,
                    view_named_transform,
                    display_color_space,
                    data_bypass,
                )?;
            } else if let Some(view_transform) = view_transform {
                build_source_to_display(
                    ops,
                    config,
                    context,
                    current_cs,
                    view_transform,
                    display_color_space,
                    data_bypass,
                )?;
            } else {
                // This is the simple case where the View only had the "colorspace" attribute,
                // which is referred to here as displayColorSpace.  Apply the conversion to there
                // from the current color space (which may vary if a look was applied).
                build_color_space_ops_between(
                    ops,
                    config,
                    context,
                    Some(current_cs),
                    display_color_space,
                    data_bypass,
                )?;
            }
        }
        TransformDirection::Inverse => {
            // The source color space of the view transform might need to be computed. In
            // forward, looks (if present) are applied and will change the current color space
            // that is used as the starting point of the view transform. Looks need to be used in
            // order to find what color space to use for the view transform in the inverse
            // direction.
            let mut vt_source_cs = Some(src_color_space);
            if !looks.is_empty() {
                // Get the result color space of applying the looks in the forward direction.
                let cs_res = looks_result_color_space(config, context, &looks)?;
                vt_source_cs = config.color_space(cs_res);
            }

            if let Some(cs_named_transform) = cs_named_transform {
                // Ignore currentCS.
                let transform =
                    NamedTransform::get_transform(cs_named_transform, TransformDirection::Inverse)?;
                build_ops(
                    ops,
                    config,
                    context,
                    &transform,
                    TransformDirection::Forward,
                )?;
            } else if let Some(view_named_transform) = view_named_transform {
                build_display_to_named_transform(
                    ops,
                    config,
                    context,
                    display_color_space,
                    view_named_transform,
                    data_bypass,
                )?;
            } else if let Some(view_transform) = view_transform {
                build_display_to_source(
                    ops,
                    config,
                    context,
                    display_color_space,
                    view_transform,
                    vt_source_cs,
                    data_bypass,
                )?;
            } else {
                // Apply the conversion from the display color space to the vtSourceCS.
                build_color_space_ops_between(
                    ops,
                    config,
                    context,
                    display_color_space,
                    vt_source_cs,
                    data_bypass,
                )?;
            }

            if !looks.is_empty() {
                // Apply looks in inverse direction.  Note that vtSourceCS is updated.
                looks.reverse();
                build_look_ops_from(ops, &mut vt_source_cs, false, config, context, &looks)?;

                // End in srcColorSpace.
                build_color_space_ops_between(
                    ops,
                    config,
                    context,
                    vt_source_cs,
                    Some(src_color_space),
                    data_bypass,
                )?;
            }
        }
    }
    Ok(())
}

/// Whether `tr` uses context variables, which it adds to `used_context_vars`: those of its
/// source color space, the view's color space, the view transform's transforms, and the looks
/// of every option of the view's looks (unless they are bypassed). It may count variables the
/// direction in use doesn't need.
///
/// Port of `CollectContextVariables(const Config &, const Context &, const DisplayViewTransform
/// &, ContextRcPtr &)` (src/OpenColorIO/transforms/DisplayViewTransform.cpp:530-600 @ v2.5.2).
pub(crate) fn collect_display_view_transform_context_variables(
    config: &Config,
    context: &Context,
    tr: &DisplayViewTransform,
    used_context_vars: &mut Context,
) -> Result<bool> {
    let mut found_context_vars = false;

    // NB: The search could return false positive but should not miss anything i.e. it looks
    // in both directions even if only one will be used, and it only roughly mimics the op
    // creation.

    let src = config.color_space(tr.src());
    if collect_color_space_context_variables(config, context, src, used_context_vars)? {
        found_context_vars = true;
    }

    let cs_name = c_str(config.display_view_color_space_name(tr.display(), tr.view()));
    if !cs_name.is_empty() {
        let src = config.color_space(cs_name);
        if collect_color_space_context_variables(config, context, src, used_context_vars)? {
            found_context_vars = true;
        }
    }

    let vt_name = c_str(config.display_view_transform_name(tr.display(), tr.view()));
    if !vt_name.is_empty()
        && let Some(vt) = config.view_transform(vt_name)
    {
        for dir in [
            ViewTransformDirection::ToReference,
            ViewTransformDirection::FromReference,
        ] {
            if let Some(t) = vt.transform(dir)
                && collect_context_variables(config, context, t, used_context_vars)?
            {
                found_context_vars = true;
            }
        }
    }

    // TODO: The LooksBypass must be a DynamicProperty to allow live on/off.
    if !tr.looks_bypass() {
        let looks_str = config.display_view_looks(tr.display(), tr.view());
        let mut looks = LookParseResult::default();
        looks.parse(looks_str)?;

        for tokens in looks.options() {
            for token in tokens {
                if let Some(look) = config.look(&token.name)
                    && collect_look_context_variables(
                        config,
                        context,
                        token.dir,
                        look,
                        used_context_vars,
                    )?
                {
                    found_context_vars = true;
                }
            }
        }
    }

    Ok(found_context_vars)
}

#[cfg(test)]
#[path = "display_view_transform_tests.rs"]
mod tests;
