// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The look transform: a port of `src/OpenColorIO/transforms/LookTransform.cpp` @ v2.5.2: the
//! class, its op builder (`BuildLookOps`), `GetLooksResultColorSpace` and its
//! `CollectContextVariables`.

use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::ExceptionKind;
use ocio_ops::exception::{Exception, Result};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{
    TransformDirection, combine_transform_directions, transform_direction_to_string,
};
use ocio_ops::ops::noop::no_ops::create_look_no_op;
use ocio_ops::utils::string_utils::c_str;

use crate::color_space::ColorSpace;
use crate::config::Config;
use crate::context::Context;
use crate::look::{Look, collect_look_context_variables};
use crate::look_parse::{LookParseResult, Token, Tokens};
use crate::transform::{build_ops, put_c_str, validate_direction};
use crate::transforms::color_space_transform::{
    build_color_space_ops_between, collect_color_space_context_variables,
};

/// Looks applied between two of a config's color spaces, by name: a list of look names, each
/// optionally prefixed with `+` (forward) or `-` (inverse), separated by commas or colons.
///
/// A copy is upstream's `createEditableCopy`. Upstream gives the class no `equals`.
///
/// Port of `LookTransform` and its `Impl` (include/OpenColorIO/OpenColorTransforms.h:
/// 1740-1797, src/OpenColorIO/transforms/LookTransform.cpp:18-147 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct LookTransform {
    /// `m_dir`.
    dir: TransformDirection,
    /// `m_skipColorSpaceConversion`.
    skip_color_space_conversion: bool,
    /// `m_src`.
    src: Vec<u8>,
    /// `m_dst`.
    dst: Vec<u8>,
    /// `m_looks`.
    looks: Vec<u8>,
}

impl Default for LookTransform {
    fn default() -> Self {
        Self::new()
    }
}

impl LookTransform {
    /// A forward transform with empty names and no looks, which converts between the color
    /// spaces.
    ///
    /// Port of `LookTransform::Create` and `Impl` (LookTransform.cpp:18-60 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> LookTransform {
        LookTransform {
            dir: TransformDirection::Forward,
            skip_color_space_conversion: false,
            src: Vec::new(),
            dst: Vec::new(),
            looks: Vec::new(),
        }
    }

    /// Port of `LookTransform::getDirection` (LookTransform.cpp:75-78 @ v2.5.2).
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        self.dir
    }

    /// Port of `LookTransform::setDirection` (LookTransform.cpp:80-83 @ v2.5.2).
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.dir = dir;
    }

    /// Checks the direction ("LookTransform validation failed: " and its error), then that both
    /// color spaces are set. The looks may be empty.
    ///
    /// Port of `LookTransform::validate` (LookTransform.cpp:85-107 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        if let Err(ex) = validate_direction(self.dir) {
            return Err(Exception::new(format!(
                "LookTransform validation failed: {}",
                ex.message()
            )));
        }

        if self.src.is_empty() {
            return Err(Exception::new(
                "LookTransform: empty source color space name.",
            ));
        }

        if self.dst.is_empty() {
            return Err(Exception::new(
                "LookTransform: empty destination color space name.",
            ));
        }
        Ok(())
    }

    /// The source color space's name.
    ///
    /// Port of `LookTransform::getSrc` (LookTransform.cpp:109-112 @ v2.5.2).
    #[doc(alias = "getSrc")]
    pub fn src(&self) -> &[u8] {
        &self.src
    }

    /// Sets the source color space's name, up to its first NUL (a C string).
    ///
    /// Port of `LookTransform::setSrc` (LookTransform.cpp:114-117 @ v2.5.2). Its null pointer,
    /// which sets an empty name, is an empty slice here.
    #[doc(alias = "setSrc")]
    pub fn set_src(&mut self, src: impl AsRef<[u8]>) {
        self.src = c_str(src.as_ref()).to_vec();
    }

    /// The destination color space's name.
    ///
    /// Port of `LookTransform::getDst` (LookTransform.cpp:119-122 @ v2.5.2).
    #[doc(alias = "getDst")]
    pub fn dst(&self) -> &[u8] {
        &self.dst
    }

    /// Sets the destination color space's name, up to its first NUL.
    ///
    /// Port of `LookTransform::setDst` (LookTransform.cpp:124-127 @ v2.5.2).
    #[doc(alias = "setDst")]
    pub fn set_dst(&mut self, dst: impl AsRef<[u8]>) {
        self.dst = c_str(dst.as_ref()).to_vec();
    }

    /// Sets the looks, up to the first NUL: look names separated by commas or colons, each
    /// optionally prefixed with `+` or `-`.
    ///
    /// Port of `LookTransform::setLooks` (LookTransform.cpp:129-132 @ v2.5.2).
    #[doc(alias = "setLooks")]
    pub fn set_looks(&mut self, looks: impl AsRef<[u8]>) {
        self.looks = c_str(looks.as_ref()).to_vec();
    }

    /// The looks, as they were set.
    ///
    /// Port of `LookTransform::getLooks` (LookTransform.cpp:134-137 @ v2.5.2).
    #[doc(alias = "getLooks")]
    pub fn looks(&self) -> &[u8] {
        &self.looks
    }

    /// Whether only the looks are applied, without converting from the source color space and
    /// to the destination.
    ///
    /// Port of `LookTransform::setSkipColorSpaceConversion` (LookTransform.cpp:139-142 @
    /// v2.5.2).
    #[doc(alias = "setSkipColorSpaceConversion")]
    pub fn set_skip_color_space_conversion(&mut self, skip: bool) {
        self.skip_color_space_conversion = skip;
    }

    /// Port of `LookTransform::getSkipColorSpaceConversion` (LookTransform.cpp:144-147 @
    /// v2.5.2).
    #[doc(alias = "getSkipColorSpaceConversion")]
    pub fn skip_color_space_conversion(&self) -> bool {
        self.skip_color_space_conversion
    }

    /// Writes the transform's text to `os`.
    ///
    /// Port of `operator<<(std::ostream &, const LookTransform &)` (LookTransform.cpp:181-191 @
    /// v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        os.put_str("<LookTransform");
        os.put_str(" direction=");
        os.put_str(transform_direction_to_string(self.direction()));
        os.put_str(", src=");
        put_c_str(os, self.src());
        os.put_str(", dst=");
        put_c_str(os, self.dst());
        os.put_str(", looks=");
        put_c_str(os, self.looks());
        if self.skip_color_space_conversion() {
            os.put_str(", skipCSConversion");
        }
        os.put_str(">");
    }
}

impl fmt::Display for LookTransform {
    /// `<LookTransform direction=<dir>, src=<src>, dst=<dst>, looks=<looks>[,
    /// skipCSConversion]>`.
    ///
    /// Port of `operator<<(std::ostream &, const LookTransform &)` (LookTransform.cpp:181-191 @
    /// v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(&os.to_string_lossy())
    }
}

/// The color space that applying `looks` (forward, from no color space) ends in: the process
/// space of their last look, or of the option of fall-back looks that applies; `""` without
/// looks.
///
/// Port of `LooksResultColorSpace` (src/OpenColorIO/transforms/LookTransform.cpp:149-167 @
/// v2.5.2).
pub(crate) fn looks_result_color_space<'c>(
    config: &'c Config,
    context: &Context,
    looks: &LookParseResult,
) -> Result<&'c [u8]> {
    if !looks.is_empty() {
        // Apply looks in forward direction to update the source space.
        // Note that we cannot simply take the process space of the last look, since one of the
        // look fall-backs may be used, so the look tokens need to be run.
        let mut current_color_space = None;
        let mut tmp = OpVec::new();
        build_look_ops_from(
            &mut tmp,
            &mut current_color_space,
            false,
            config,
            context,
            looks,
        )?;
        if let Some(cs) = current_color_space {
            return Ok(cs.name());
        }
    }
    Ok(b"")
}

impl LookTransform {
    /// The color space that applying the looks `looks_str` ends in (see
    /// [`looks_result_color_space`]), or `""` for no looks.
    ///
    /// Port of `LookTransform::GetLooksResultColorSpace` (src/OpenColorIO/transforms/
    /// LookTransform.cpp:169-179 @ v2.5.2).
    #[doc(alias = "GetLooksResultColorSpace")]
    pub fn looks_result_color_space<'c>(
        config: &'c Config,
        context: &Context,
        looks_str: impl AsRef<[u8]>,
    ) -> Result<&'c [u8]> {
        let looks_str = c_str(looks_str.as_ref());
        if looks_str.is_empty() {
            return Ok(b"");
        }

        let mut looks = LookParseResult::default();
        looks.parse(looks_str)?;

        looks_result_color_space(config, context, &looks)
    }
}

/// "RunLookTokens error. The specified look, '<name>', cannot be found. " and the config's
/// looks, built out of line (docs/improvements.md, U-58).
#[cold]
#[inline(never)]
fn look_not_found(config: &Config, look_name: &[u8]) -> Result<()> {
    let mut os = b"RunLookTokens error. The specified look, '".to_vec();
    os.extend_from_slice(look_name);
    os.extend_from_slice(b"', cannot be found. ");
    if config.num_looks() == 0 {
        os.extend_from_slice(b" (No looks defined in config).");
    } else {
        os.extend_from_slice(b" (looks: ");
        for ii in 0..config.num_looks() {
            if ii != 0 {
                os.extend_from_slice(b", ");
            }
            os.extend_from_slice(config.look_name_by_index(ii));
        }
        os.extend_from_slice(b").");
    }
    Err(Exception::new(os))
}

/// "RunLookTokens error. The specified look, '<name>', requires processing in the ColorSpace,
/// '<process space>' which is not defined.", built out of line (docs/improvements.md, U-58).
#[cold]
#[inline(never)]
fn process_space_not_found(look_name: &[u8], look: &Look) -> Result<()> {
    let mut os = b"RunLookTokens error. The specified look, '".to_vec();
    os.extend_from_slice(look_name);
    os.extend_from_slice(b"', requires processing in the ColorSpace, '");
    os.extend_from_slice(look.process_space());
    os.extend_from_slice(b"' which is not defined.");
    Err(Exception::new(os))
}

/// Appends the look no-op of the look `look_name`, its name prefixed with '-' inverse, out of
/// line so that the name isn't on the stack while the look's ops build (docs/improvements.md,
/// U-58).
///
/// Part of the port of `RunLookTokens` (src/OpenColorIO/transforms/LookTransform.cpp:229-262
/// @ v2.5.2).
#[inline(never)]
fn push_look_no_op(ops: &mut OpVec, look_name: &[u8], dir: TransformDirection) {
    match dir {
        TransformDirection::Forward => create_look_no_op(ops, look_name),
        TransformDirection::Inverse => {
            let mut name = b"-".to_vec();
            name.extend_from_slice(look_name);
            create_look_no_op(ops, &name);
        }
    }
}

/// Appends the look no-op and the ops of `look` in the direction `dir`: forward, its transform,
/// or else the inverse of its inverse transform; inverse, its inverse transform, or else the
/// inverse of its transform (or nothing).
///
/// Part of the port of `RunLookTokens` (src/OpenColorIO/transforms/LookTransform.cpp:225-263
/// @ v2.5.2). Its recursive calls are tail calls, so that its frame stays small
/// (docs/improvements.md, U-58).
fn build_look_token_ops(
    tmp_ops: &mut OpVec,
    config: &Config,
    context: &Context,
    look: &Look,
    look_name: &[u8],
    dir: TransformDirection,
) -> Result<()> {
    push_look_no_op(tmp_ops, look_name, dir);
    let (first, second) = match dir {
        TransformDirection::Forward => (look.transform(), look.inverse_transform()),
        TransformDirection::Inverse => (look.inverse_transform(), look.transform()),
    };
    if let Some(t) = first {
        build_ops(tmp_ops, config, context, t, TransformDirection::Forward)
    } else if let Some(t) = second {
        build_ops(tmp_ops, config, context, t, TransformDirection::Inverse)
    } else {
        Ok(())
    }
}

/// A new empty op list, boxed and built out of line, so that it takes a pointer's room in the
/// frame of [`run_look_token`] (docs/improvements.md, U-58).
#[inline(never)]
fn new_op_vec() -> Box<OpVec> {
    Box::default()
}

/// Appends the ops of the look of `token`, after the conversion from the current color space to
/// its process space (unless `skip_color_space_conversion`), which becomes the current color
/// space.
///
/// Part of the port of `RunLookTokens` (src/OpenColorIO/transforms/LookTransform.cpp:203-305
/// @ v2.5.2), one look per call: its loop over an option's looks is in its callers,
/// [`build_look_ops_from`] for a single option and [`run_look_option`] for several, so that the
/// recursion takes one frame less. Its errors are built out of line and its recursive calls
/// return their results as they are, so that its frame stays small (docs/improvements.md,
/// U-58).
#[expect(
    clippy::question_mark,
    reason = "`?` takes several results' room in the frame of an unoptimized build (U-58)"
)]
fn run_look_token<'c>(
    ops: &mut OpVec,
    current_color_space: &mut Option<&'c ColorSpace>,
    skip_color_space_conversion: bool,
    config: &'c Config,
    context: &Context,
    token: &Token,
) -> Result<()> {
    let look_name = &token.name;

    let Some(look) = config.look(look_name) else {
        return look_not_found(config, look_name);
    };

    let mut tmp_ops = new_op_vec();
    let look_ops = build_look_token_ops(&mut tmp_ops, config, context, look, look_name, token.dir);
    if look_ops.is_err() {
        return look_ops;
    }

    let Some(process_color_space) = config.color_space(look.process_space()) else {
        return process_space_not_found(look_name, look);
    };

    let current = *current_color_space.get_or_insert(process_color_space);

    // If current color space is already the process space skip the conversion.
    if !skip_color_space_conversion && !std::ptr::eq(current, process_color_space) {
        // Default behavior is to bypass data color space.
        let conversion = build_color_space_ops_between(
            ops,
            config,
            context,
            Some(current),
            Some(process_color_space),
            true,
        );
        if conversion.is_err() {
            return conversion;
        }
        *current_color_space = Some(process_color_space);
    }

    ops.append(&tmp_ops)
}

/// What [`build_look_ops`] goes from, to and through: its source and destination color spaces
/// (swapped inverse) and its looks (reversed inverse).
struct LookOpsEnds<'c> {
    /// The color space the looks start from.
    src: &'c ColorSpace,
    /// The color space to end in.
    dst: &'c ColorSpace,
    /// The looks, in the order they apply.
    looks: LookParseResult,
}

/// The ends and looks of `look_transform` in the direction `dir`, for [`build_look_ops`], out
/// of line so that their lookups and errors aren't on the stack while the looks' ops build
/// (docs/improvements.md, U-58).
///
/// Part of the port of `BuildLookOps(OpRcPtrVec &, const Config &, const ConstContextRcPtr &,
/// const LookTransform &, TransformDirection)` (src/OpenColorIO/transforms/
/// LookTransform.cpp:309-345 @ v2.5.2).
#[inline(never)]
fn look_ops_ends<'c>(
    config: &'c Config,
    look_transform: &LookTransform,
    dir: TransformDirection,
) -> Result<LookOpsEnds<'c>> {
    let Some(mut src) = config.color_space(look_transform.src()) else {
        let mut os =
            b"BuildLookOps error.The specified lookTransform specifies a src colorspace, \x27"
                .to_vec();
        os.extend_from_slice(c_str(look_transform.src()));
        os.extend_from_slice(b"', which is not defined.");
        return Err(Exception::new(os));
    };

    let Some(mut dst) = config.color_space(look_transform.dst()) else {
        let mut os =
            b"BuildLookOps error.The specified lookTransform specifies a dst colorspace, \x27"
                .to_vec();
        os.extend_from_slice(c_str(look_transform.dst()));
        os.extend_from_slice(b"', which is not defined.");
        return Err(Exception::new(os));
    };

    let mut looks = LookParseResult::default();
    looks.parse(look_transform.looks())?;

    // The code must handle the inverse src/dst colorspace transformation explicitly.
    let combined_dir = combine_transform_directions(dir, look_transform.direction());
    if combined_dir == TransformDirection::Inverse {
        std::mem::swap(&mut src, &mut dst);
        looks.reverse();
    }

    Ok(LookOpsEnds { src, dst, looks })
}

/// Appends the ops of `look_transform` in the direction `dir`: its looks from its source color
/// space (each look in its process space), then the conversion to its destination, unless
/// `skipColorSpaceConversion`. Inverse, the source and destination swap and the looks reverse.
///
/// Port of `BuildLookOps(OpRcPtrVec &, const Config &, const ConstContextRcPtr &, const
/// LookTransform &, TransformDirection)` (src/OpenColorIO/transforms/LookTransform.cpp:309-361
/// @ v2.5.2). The ends and looks come from [`look_ops_ends`], and the recursive calls return
/// their results as they are, so that its frame stays small (docs/improvements.md, U-58).
#[expect(
    clippy::question_mark,
    reason = "`?` takes several results' room in the frame of an unoptimized build (U-58)"
)]
pub(crate) fn build_look_ops(
    ops: &mut OpVec,
    config: &Config,
    context: &Context,
    look_transform: &LookTransform,
    dir: TransformDirection,
) -> Result<()> {
    let ends = match look_ops_ends(config, look_transform, dir) {
        Ok(ends) => ends,
        Err(e) => return Err(e),
    };

    let skip_color_space_conversion = look_transform.skip_color_space_conversion();
    let mut current_color_space = Some(ends.src);
    let looks = build_look_ops_from(
        ops,
        &mut current_color_space,
        skip_color_space_conversion,
        config,
        context,
        &ends.looks,
    );
    if looks.is_err() {
        return looks;
    }

    let current = current_color_space.expect("set above, and the looks only replace it");
    // If current color space is already the dst space skip the conversion.
    if !skip_color_space_conversion && !std::ptr::eq(current, ends.dst) {
        // Default behavior is to bypass data color space.
        build_color_space_ops_between(ops, config, context, Some(current), Some(ends.dst), true)
    } else {
        Ok(())
    }
}

/// Appends the ops of the first option of `looks` whose looks' files all exist (an option fails
/// over to the next only on a missing file), from `current_color_space`, which ends as the
/// option's last process space. Without an option that works, the missing files' messages, per
/// option.
///
/// Port of `BuildLookOps(OpRcPtrVec &, ConstColorSpaceRcPtr &, bool, const Config &, const
/// ConstContextRcPtr &, const LookParseResult &)` (src/OpenColorIO/transforms/
/// LookTransform.cpp:363-435 @ v2.5.2). Several options go through
/// [`build_first_look_option`], so that this frame stays small (docs/improvements.md, U-58).
#[expect(
    clippy::question_mark,
    reason = "`?` takes several results' room in the frame of an unoptimized build (U-58)"
)]
pub(crate) fn build_look_ops_from<'c>(
    ops: &mut OpVec,
    current_color_space: &mut Option<&'c ColorSpace>,
    skip_color_space_conversion: bool,
    config: &'c Config,
    context: &Context,
    looks: &LookParseResult,
) -> Result<()> {
    let options = looks.options();

    if options.is_empty() {
        // Do nothing.
        Ok(())
    } else if options.len() == 1 {
        // As an optimization, if we only have a single look option, just push back onto the
        // final location. The loop of `RunLookTokens`, here so that the recursion takes one
        // frame less (docs/improvements.md, U-58).
        for token in &options[0] {
            if token.name.is_empty() {
                continue;
            }

            let result = run_look_token(
                ops,
                current_color_space,
                skip_color_space_conversion,
                config,
                context,
                token,
            );
            if result.is_err() {
                return result;
            }
        }
        Ok(())
    } else {
        build_first_look_option(
            ops,
            current_color_space,
            skip_color_space_conversion,
            config,
            context,
            options,
        )
    }
}

/// Whether `result` is an error of a missing file, which fails over to the next option. Out of
/// line (docs/improvements.md, U-58).
#[inline(never)]
fn is_missing_file(result: &Result<()>) -> bool {
    matches!(result, Err(e) if e.kind() == ExceptionKind::MissingFile)
}

/// Adds to `os` the message of the option `option`, the `i`-th, whose file is missing, the error
/// `result`: "(<option>) <message>", after "  ...  " but for the first option. Out of line
/// (docs/improvements.md, U-58).
///
/// Part of the port of `BuildLookOps` (src/OpenColorIO/transforms/LookTransform.cpp:410-421 @
/// v2.5.2).
#[inline(never)]
fn add_missing_file_message(os: &mut Vec<u8>, i: usize, option: &[Token], result: &Result<()>) {
    let Err(e) = result else {
        unreachable!("the error of a missing file");
    };
    if i != 0 {
        os.extend_from_slice(b"  ...  ");
    }

    os.push(b'(');
    LookParseResult::serialize(os, option);
    os.extend_from_slice(b") ");
    os.extend_from_slice(e.what());
}

/// The error of looks none of whose options' files all exist: their messages, `os`. Out of line
/// (docs/improvements.md, U-58).
#[cold]
#[inline(never)]
fn missing_files(os: Vec<u8>) -> Result<()> {
    Err(Exception::missing_file(os))
}

/// The several options' case of [`build_look_ops_from`].
///
/// Part of the port of `BuildLookOps(OpRcPtrVec &, ConstColorSpaceRcPtr &, bool, const Config
/// &, const ConstContextRcPtr &, const LookParseResult &)` (src/OpenColorIO/transforms/
/// LookTransform.cpp:385-432 @ v2.5.2). An option's looks build into an op list of their own,
/// boxed, from a copy of `current_color_space`; on success, the copy replaces it and the ops
/// are appended to `ops`. The loop over an option's looks (`RunLookTokens`'s) is here, by index,
/// and the errors' checks and messages are out of line, so that the recursion's frame stays
/// small (docs/improvements.md, U-58).
fn build_first_look_option<'c>(
    ops: &mut OpVec,
    current_color_space: &mut Option<&'c ColorSpace>,
    skip_color_space_conversion: bool,
    config: &'c Config,
    context: &Context,
    options: &[Tokens],
) -> Result<()> {
    // If we have multiple look options, try each one in order, and if we can create the ops
    // without a missing file exception, push back it's results and return i.e., take the
    // first successully created look.

    let mut os = Vec::new();

    let mut i = 0;
    while i < options.len() {
        let option = &options[i];
        let mut cs = *current_color_space;
        let mut tmp_ops = new_op_vec();

        let mut result = Ok(());
        let mut j = 0;
        while j < option.len() {
            if !option[j].name.is_empty() {
                result = run_look_token(
                    &mut tmp_ops,
                    &mut cs,
                    skip_color_space_conversion,
                    config,
                    context,
                    &option[j],
                );
                if result.is_err() {
                    break;
                }
            }
            j += 1;
        }

        if result.is_ok() {
            *current_color_space = cs;
            return ops.append(&tmp_ops);
        }
        if !is_missing_file(&result) {
            return result;
        }
        add_missing_file_message(&mut os, i, option, &result);
        i += 1;
    }

    missing_files(os)
}

/// Whether `look` uses context variables, which it adds to `used_context_vars`: its source's
/// and destination's, and those of every look of every option of its looks (rather than only
/// the option that would apply).
///
/// Port of `CollectContextVariables(const Config &, const Context &, const LookTransform &,
/// ContextRcPtr &)` (src/OpenColorIO/transforms/LookTransform.cpp:437-482 @ v2.5.2). A look
/// list it can't parse is the parser's error.
pub(crate) fn collect_look_transform_context_variables(
    config: &Config,
    context: &Context,
    look: &LookTransform,
    used_context_vars: &mut Context,
) -> Result<bool> {
    let mut found_context_vars = false;

    let src = config.color_space(look.src());
    if collect_color_space_context_variables(config, context, src, used_context_vars)? {
        found_context_vars = true;
    }

    let dst = config.color_space(look.dst());
    if collect_color_space_context_variables(config, context, dst, used_context_vars)? {
        found_context_vars = true;
    }

    let looks = c_str(look.looks());
    if !looks.is_empty() {
        let mut look_list = LookParseResult::default();
        look_list.parse(looks)?;

        // Note: For now, simply concatenating vars used by all of the options rather than trying
        // to figure out which option would be used.

        for tokens in look_list.options() {
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
#[path = "look_transform_tests.rs"]
mod tests;
