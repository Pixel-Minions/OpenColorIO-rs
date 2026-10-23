// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The helpers of the conversions between two configs: the interchange roles of a pair of
//! color spaces, and the heuristics that identify a config's interchange spaces and the color
//! space equivalent to one of the built-in config's.
//!
//! Port of `ConfigUtils` (src/OpenColorIO/ConfigUtils.cpp, ConfigUtils.h @ v2.5.2), in part:
//! `GetInterchangeRolesForColorSpaceConversion`, the heuristics (`IdentifyInterchangeSpace`,
//! `IdentifyBuiltinColorSpace` and their helpers) and `SuspendCacheGuard`.

use ocio_ops::exception::{Exception, Result};
use ocio_ops::image_desc::PackedImageDesc;
use ocio_ops::math_utils::{clamp, equal_with_abs_error};
use ocio_ops::open_color_types::{
    ChannelOrdering, ColorSpaceDirection, ColorSpaceVisibility, OptimizationFlags,
    ReferenceSpaceType, SearchReferenceSpaceType, TransformDirection,
};
use ocio_ops::parse_utils::{ROLE_INTERCHANGE_DISPLAY, ROLE_INTERCHANGE_SCENE};
use ocio_ops::utils::pystring::os_path;
use ocio_ops::utils::string_utils::{c_str, find, lower};

use crate::color_space::ColorSpace;
use crate::config::Config;
use crate::processor::{Processor, ProcessorCacheFlags};
use crate::transform::Transform;

/// What [`get_interchange_roles_for_color_space_conversion`] finds: the reference space type
/// of the interchange, and the interchange color spaces' names, when both configs have the
/// interchange role of that type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InterchangeRoles<'s, 'd> {
    /// `interchangeType`.
    pub(crate) interchange_type: ReferenceSpaceType,
    /// `srcInterchangeCSName` and `dstInterchangeCSName`, when the function returns true.
    pub(crate) names: Option<(&'s [u8], &'d [u8])>,
}

/// The name of the interchange role of `interchange_type`.
pub(crate) fn interchange_role_name(interchange_type: ReferenceSpaceType) -> &'static str {
    if interchange_type == ReferenceSpaceType::Scene {
        ROLE_INTERCHANGE_SCENE
    } else {
        ROLE_INTERCHANGE_DISPLAY
    }
}

/// The interchange color spaces of the conversion from the color space `src_name` of
/// `src_config` to the color space `dst_name` of `dst_config`: the display-referred
/// interchange role's when both color spaces are display-referred (or, with an empty
/// `src_name`, when the destination is), else the scene-referred one's. No heuristics.
///
/// `names` is `None` when either config lacks the role (upstream returns false); an error when
/// a color space isn't found, or the role names a color space its config doesn't have. Upstream
/// sets `srcInterchangeCSName` before it looks at the destination config, but no caller reads
/// it when the function returns false or throws.
///
/// The names are C strings (`const char *`): each up to its first NUL.
///
/// Port of `ConfigUtils::GetInterchangeRolesForColorSpaceConversion`
/// (src/OpenColorIO/ConfigUtils.cpp:59-162 @ v2.5.2).
pub(crate) fn get_interchange_roles_for_color_space_conversion<'s, 'd>(
    src_config: &'s Config,
    src_name: &[u8],
    dst_config: &'d Config,
    dst_name: &[u8],
) -> Result<InterchangeRoles<'s, 'd>> {
    let src_name = c_str(src_name);
    let dst_name = c_str(dst_name);

    let Some(dst_color_space) = dst_config.color_space(dst_name) else {
        return Err(Exception::new(
            [
                b"Could not find destination color space '".as_slice(),
                dst_name,
                b"'.",
            ]
            .concat(),
        ));
    };

    let mut interchange_type = ReferenceSpaceType::Scene;

    if src_name.is_empty() {
        // If srcName is empty, just use the reference type of the destination side. In this
        // scenario, the source color space is unknown but the assumption is that when it is
        // found it will have the same reference space type as the destination color space.
        if dst_color_space.reference_space_type() == ReferenceSpaceType::Display {
            interchange_type = ReferenceSpaceType::Display;
        }
    } else {
        let Some(src_color_space) = src_config.color_space(src_name) else {
            return Err(Exception::new(
                [
                    b"Could not find source color space '".as_slice(),
                    src_name,
                    b"'.",
                ]
                .concat(),
            ));
        };

        // Only use the display-referred reference space if both color spaces are
        // display-referred (so that swapping src and dst gives the inverse).
        if src_color_space.reference_space_type() == ReferenceSpaceType::Display
            && dst_color_space.reference_space_type() == ReferenceSpaceType::Display
        {
            interchange_type = ReferenceSpaceType::Display;
        }
    }

    let interchange_role_name = interchange_role_name(interchange_type);

    if !src_config.has_role(interchange_role_name) {
        return Ok(InterchangeRoles {
            interchange_type,
            names: None,
        });
    }
    // Get the color space name assigned to the interchange role.
    let Some(src_interchange_cs) = src_config.color_space(interchange_role_name) else {
        return Err(Exception::new(format!(
            "The role '{interchange_role_name}' refers to a color space that is missing in the \
             source config."
        )));
    };

    if !dst_config.has_role(interchange_role_name) {
        return Ok(InterchangeRoles {
            interchange_type,
            names: None,
        });
    }
    // Get the color space name assigned to the interchange role.
    let Some(dst_interchange_cs) = dst_config.color_space(interchange_role_name) else {
        return Err(Exception::new(format!(
            "The role '{interchange_role_name}' refers to a color space that is missing in the \
             destination config."
        )));
    };

    Ok(InterchangeRoles {
        interchange_type,
        names: Some((src_interchange_cs.name(), dst_interchange_cs.name())),
    })
}

// The heuristics //////////////////////////////////////////////////////////////////////////////

// The following code needs to know the names of some of the color spaces in the built-in
// default config. If the color space names of that config are ever modified, the following
// strings should be kept in sync.

/// The name of the sRGB space in the built-in config.
///
/// Port of `ConfigUtils::getSRGBColorSpaceName` (src/OpenColorIO/ConfigUtils.cpp:24-30 @
/// v2.5.2).
fn srgb_color_space_name() -> &'static [u8] {
    b"sRGB - Texture"
}

/// The candidate built-in default config reference linear color spaces that will be used when
/// searching through the source config: the one at `index`, clamped to the list.
///
/// Port of `ConfigUtils::getBuiltinLinearSpaceName` (src/OpenColorIO/ConfigUtils.cpp:32-45 @
/// v2.5.2).
fn builtin_linear_space_name(index: i32) -> &'static [u8] {
    const BUILTIN_LINEAR_SPACES: [&[u8]; 5] = [
        b"ACES2065-1",
        b"ACEScg",
        b"Linear Rec.709 (sRGB)",
        b"Linear P3-D65",
        b"Linear Rec.2020",
    ];
    // Clamp(index, 0, 4) is within the list.
    BUILTIN_LINEAR_SPACES[usize::try_from(clamp(index, 0, 4)).unwrap_or(0)]
}

/// The number of items available from [`builtin_linear_space_name`].
///
/// Port of `ConfigUtils::getNumberOfbuiltinLinearSpaces` (src/OpenColorIO/ConfigUtils.cpp:
/// 47-53 @ v2.5.2).
fn number_of_builtin_linear_spaces() -> i32 {
    5
}

/// Whether the color space's name (ignoring case) or one of its aliases (in lower case only:
/// upstream doesn't lower the aliases, docs/improvements.md I-377) contains `srgb`.
///
/// Port of `ConfigUtils::containsSRGB` (src/OpenColorIO/ConfigUtils.cpp:164-183 @ v2.5.2).
fn contains_srgb(cs: &ColorSpace) -> bool {
    let name = lower(cs.name());
    if find(&name, b"srgb").is_some() {
        return true;
    }

    let nb_of_aliases = cs.num_aliases();
    for i in 0..nb_of_aliases {
        if find(cs.alias(i), b"srgb").is_some() {
            return true;
        }
    }

    false
}

/// The first scene-referred color space, inactive ones included, that isn't a data space and
/// has neither a to-reference nor a from-reference transform; `""` for none.
///
/// Port of `ConfigUtils::getRefSpaceName` (src/OpenColorIO/ConfigUtils.cpp:185-219 @ v2.5.2).
fn ref_space_name(cfg: &Config) -> &[u8] {
    // It's important to support inactive spaces since sometimes the only reference space may
    // be inactive, e.g. the display-referred reference in the built-in configs.
    let nb_cs =
        cfg.num_color_spaces_with(SearchReferenceSpaceType::Scene, ColorSpaceVisibility::All);

    for i in 0..nb_cs {
        let csname = cfg.color_space_name_by_index_with(
            SearchReferenceSpaceType::Scene,
            ColorSpaceVisibility::All,
            i,
        );
        let cs = cfg
            .color_space(csname)
            .expect("a color space of the config");

        if cs.is_data() {
            continue;
        }
        if cs.transform(ColorSpaceDirection::ToReference).is_some() {
            continue;
        }
        if cs.transform(ColorSpaceDirection::FromReference).is_some() {
            continue;
        }

        return csname;
    }
    b""
}

/// The first scene-referred data color space, inactive ones included; `""` for none.
///
/// Port of `ConfigUtils::getDataSpaceName` (src/OpenColorIO/ConfigUtils.cpp:221-240 @ v2.5.2).
fn data_space_name(cfg: &Config) -> &[u8] {
    let nb_cs =
        cfg.num_color_spaces_with(SearchReferenceSpaceType::Scene, ColorSpaceVisibility::All);

    for i in 0..nb_cs {
        let csname = cfg.color_space_name_by_index_with(
            SearchReferenceSpaceType::Scene,
            ColorSpaceVisibility::All,
            i,
        );
        let cs = cfg
            .color_space(csname)
            .expect("a color space of the config");

        if cs.is_data() {
            return csname;
        }
    }
    b""
}

/// Whether the processor's CPU processor, without optimization, changes none of the packed
/// RGBA values `rgba_vals` by more than `abs_tolerance` (a NaN result is a change).
///
/// Port of `ConfigUtils::isIdentityTransform` (src/OpenColorIO/ConfigUtils.cpp:242-266 @
/// v2.5.2).
fn is_identity_transform(proc: &Processor, rgba_vals: &[f32], abs_tolerance: f32) -> Result<bool> {
    let mut out = vec![0f32; rgba_vals.len()];

    {
        let width = rgba_vals.len() / 4;
        let desc = PackedImageDesc::with_channel_order(rgba_vals, width, 1, ChannelOrdering::Rgba)?;
        let mut desc_dst =
            PackedImageDesc::with_channel_order(&mut out[..], width, 1, ChannelOrdering::Rgba)?;

        let cpu = proc.optimized_cpu_processor(OptimizationFlags::NONE)?;
        cpu.apply_src_dst(&desc, &mut desc_dst)?;
    }

    for (val, o) in rgba_vals.iter().zip(&out) {
        if !equal_with_abs_error(*val, *o, abs_tolerance) {
            return Ok(false);
        }
    }

    Ok(true)
}

/// Whether the processor holds a MatrixTransform with an off-diagonal RGB coefficient larger
/// than 0.1 in magnitude (an actual color primary conversion, not a scale and offset).
///
/// Port of `ConfigUtils::hasNonTrivialMatrixTransform` (src/OpenColorIO/ConfigUtils.cpp:
/// 268-302 @ v2.5.2).
fn has_non_trivial_matrix_transform(proc: &Processor) -> Result<bool> {
    let gt = proc.create_group_transform()?;
    // The result of createGroupTransform only contains transforms that correspond to ops, in
    // other words, there are no complex transforms such as File, Builtin, or ColorSpace, and
    // the only GroupTransform is the enclosing one.
    for i in 0..gt.num_transforms() {
        let transform = gt.transform(i)?;
        if let Transform::Matrix(mtx) = transform {
            // Check that there is a significant off-diagonal coefficient in the matrix.
            let values = mtx.matrix();
            for j in 0..3 {
                // only checking rgb, not alpha
                for k in 0..3 {
                    // only checking rgb, not alpha
                    if j != k && values[j * 4 + k].abs() > 0.1 {
                        return Ok(true);
                    }
                }
            }
        }
    }
    Ok(false)
}

/// Whether the transform holds a type inappropriate for the heuristics: a FileTransform of a
/// file other than `.spi1d` or `.spimtx` (the extension by the platform's `splitext`, in lower
/// case), a ColorSpaceTransform, DisplayViewTransform or LookTransform (which may hide one), a
/// Lut3DTransform; a group's children recursively.
///
/// Port of `ConfigUtils::containsBlockedTransform` (src/OpenColorIO/ConfigUtils.cpp:304-357 @
/// v2.5.2).
fn contains_blocked_transform(transform: &Transform) -> bool {
    match transform {
        // If it's a GroupTransform, need to recurse into it to check the contents.
        Transform::Group(gt) => {
            for i in 0..gt.num_transforms() {
                let tr = gt.transform(i).expect("a child of the group");
                if contains_blocked_transform(tr) {
                    return true;
                }
            }
            false
        }
        // Prevent FileTransforms from being used, except for spi1d and spimtx since these may
        // be used with OCIO v1 configs to implement the type of color spaces the heuristics are
        // designed to look for.
        Transform::File(ft) => {
            let filepath = ft.src();
            let (_root, extension) = os_path::splitext(filepath);
            let extension = lower(&extension);
            extension != b".spi1d" && extension != b".spimtx"
        }
        // Prevent transforms that may be hiding a FileTransform.
        Transform::ColorSpace(_) | Transform::DisplayView(_) | Transform::Look(_) => true,
        // Lut3D is the main type of transform to avoid (it would still be a FileTransform
        // here).
        Transform::Lut3D(_) => true,
        _ => false,
    }
}

/// Whether the heuristics skip the color space: a data space, one of another reference space
/// type than `ref_space_type`, one whose to-reference transform (else from-reference
/// transform) holds a blocked transform ([`contains_blocked_transform`]), or, with
/// `block_ref_spaces`, one without transforms.
///
/// Port of `ConfigUtils::excludeColorSpaceFromHeuristics` (src/OpenColorIO/ConfigUtils.cpp:
/// 359-411 @ v2.5.2).
fn exclude_color_space_from_heuristics(
    cs: &ColorSpace,
    ref_space_type: ReferenceSpaceType,
    block_ref_spaces: bool,
) -> bool {
    if cs.is_data() {
        return true;
    }

    if cs.reference_space_type() != ref_space_type {
        return true;
    }

    if let Some(transform) = cs.transform(ColorSpaceDirection::ToReference) {
        // The to_ref transform is what the heuristics try to use first, so if that does not
        // contain problematic transforms, it's ok to proceed without checking the from_ref
        // transform.
        return contains_blocked_transform(transform);
    }

    // There is no to_ref transform, check if the from_ref is present.
    if let Some(transform) = cs.transform(ColorSpaceDirection::FromReference) {
        return contains_blocked_transform(transform);
    }
    // Color space contains no transforms (it's a reference space).
    block_ref_spaces
}

/// The RGBA test values of the linear and sRGB heuristics: colors, black and white.
#[rustfmt::skip]
const RGBA_TEST_VALS: [f32; 20] = [
    0.7,  0.4,  0.02, 0.,
    0.02, 0.6,  0.2,  0.,
    0.3,  0.02, 0.5,  0.,
    0.,   0.,   0.,   0.,
    1.,   1.,   1.,   0.,
];

/// The index among the built-in linear spaces of the source config's reference space, found by
/// converting the linear color space `cs` to every other built-in linear space through every
/// candidate reference space: the first identity (within 1e-3) names it; -1 for none.
///
/// Port of `ConfigUtils::getReferenceSpaceFromLinearSpace` (src/OpenColorIO/ConfigUtils.cpp:
/// 413-468 @ v2.5.2).
fn reference_space_from_linear_space(
    src_config: &Config,
    src_ref_name: &[u8],
    cs: &ColorSpace,
    builtin_config: &Config,
) -> Result<i32> {
    // Define a set of (somewhat arbitrary) RGB values to test whether the combined transform is
    // enough of an identity.
    #[rustfmt::skip]
    let vals: [f32; 20] = [
        0.7,  0.4,  0.02, 0.,
        0.02, 0.6, -0.2,  0.,
        0.3,  0.02, 1.5,  0.,
        0.,   0.,   0.,   0.,
        1.,   1.,   1.,   0.,
    ];

    // Test the transform from the test color space to its reference space against all
    // combinations of the built-in linear color spaces. If one of them results in an
    // identity, that identifies what the source color space and reference space are.

    for i in 0..number_of_builtin_linear_spaces() {
        for j in 0..number_of_builtin_linear_spaces() {
            // Ensure the built-in side of the conversion is never an identity, since if both
            // the src side and built-in side are an identity, it would seem as though the
            // reference space has been identified, but in fact it would not be.
            if i != j {
                let proc = Config::processor_from_configs_with_interchange(
                    src_config,
                    cs.name(),
                    src_ref_name,
                    builtin_config,
                    builtin_linear_space_name(i),
                    builtin_linear_space_name(j),
                )?;

                if is_identity_transform(&proc, &vals, 1e-3)? {
                    return Ok(j);
                }
            }
        }
    }

    Ok(-1)
}

/// The index among the built-in linear spaces of the source config's reference space, found
/// through the color space `cs` if it is an sRGB texture space: its to-reference transform
/// (else its from-reference transform with its direction set to inverse, which leaves a
/// from-reference transform whose direction is inverse as it is, docs/improvements.md I-378)
/// must hold a non-trivial matrix and have the sRGB curve (within 1e-3, on neutral values);
/// then the conversion to the built-in sRGB texture space through each candidate reference
/// space must be an identity (within 1e-3). -1 for none.
///
/// Port of `ConfigUtils::getReferenceSpaceFromSRGBSpace` (src/OpenColorIO/ConfigUtils.cpp:
/// 470-588 @ v2.5.2).
fn reference_space_from_srgb_space(
    src_config: &Config,
    src_ref_name: &[u8],
    cs: &ColorSpace,
    builtin_config: &Config,
) -> Result<i32> {
    // Get a transform in the to-reference direction.
    let to_ref_transform = if let Some(ctransform) = cs.transform(ColorSpaceDirection::ToReference)
    {
        ctransform.clone()
    } else if let Some(ctransform) = cs.transform(ColorSpaceDirection::FromReference) {
        let mut transform = ctransform.clone();
        transform.set_direction(TransformDirection::Inverse);
        transform
    } else {
        // Don't check spaces without transforms / data spaces.
        return Ok(-1);
    };

    // First check if it has the right non-linearity. The objective is to fail quickly on color
    // spaces that are definitely not sRGB before proceeding to the longer test of guessing the
    // reference space primaries.

    // Break point is at 0.039286, so include at least one value below this.
    #[rustfmt::skip]
    let vals: [f32; 18] = [
        0.5,  0.5,  0.5,
        0.03, 0.03, 0.03,
        0.25, 0.25, 0.25,
        0.75, 0.75, 0.75,
        0.,   0.,   0.,
        1.,   1.,   1.,
    ];
    let mut out = [0f32; 18];

    let proc = src_config.processor_in_direction(&to_ref_transform, TransformDirection::Forward)?;

    // Ensure that the color space is not only an sRGB curve, it needs to have a color matrix
    // too or else the last step below could succeed by pairing the built-in sRGB space with a
    // linear space that cancels out the matrix in the built-in sRGB space. (After the
    // getProcessor call: the complex transforms that could be hiding a matrix are ops then.)
    if !has_non_trivial_matrix_transform(&proc)? {
        return Ok(-1);
    }

    {
        let desc = PackedImageDesc::with_channel_order(&vals[..], 6, 1, ChannelOrdering::Rgb)?;
        let mut desc_dst =
            PackedImageDesc::with_channel_order(&mut out[..], 6, 1, ChannelOrdering::Rgb)?;
        let cpu = proc.optimized_cpu_processor(OptimizationFlags::NONE)?;
        // Convert the non-linear values to linear.
        cpu.apply_src_dst(&desc, &mut desc_dst)?;
    }

    for (val, o) in vals.iter().zip(out.iter_mut()) {
        // Apply the sRGB function to convert the processed linear values back to non-linear
        // (GammaOpUtils.cpp; this value provides continuity at the breakpoint): float
        // arithmetic, std::pow(float, float) is powf.
        if *o <= 0.003_039_934_639_778_432_3_f32 {
            *o *= 12.923_210_180_787_857_f32;
        } else {
            *o = 1.055f32 * o.powf(1.0f32 / 2.4f32) - 0.055f32;
        }
        // Compare against the original source values. (This assumes equal channel sRGB values
        // remain so in the reference space of src config.)
        if !equal_with_abs_error(*val, *o, 1e-3f32) {
            return Ok(-1);
        }
    }

    // A (somewhat arbitrary) set of RGB values to test whether the transform is in fact
    // converting sRGB texture values to the candidate reference space. It includes 0.02 which
    // is on the sRGB linear segment, color values, and neutral values.
    let vals = RGBA_TEST_VALS;

    // At this point, cs has the expected non-linearity and a non-trivial matrix to its
    // reference space. Now try to identify the matrix.
    for i in 0..number_of_builtin_linear_spaces() {
        let proc = Config::processor_from_configs_with_interchange(
            src_config,
            cs.name(),
            src_ref_name,
            builtin_config,
            srgb_color_space_name(),
            builtin_linear_space_name(i),
        )?;
        if is_identity_transform(&proc, &vals, 1e-3)? {
            return Ok(i);
        }
    }

    Ok(-1)
}

/// Turns a config's processor cache off while it lives, and its flags back as they were when
/// it is dropped: the heuristics make many processors that won't be reused.
///
/// Upstream changes a shared config's flags through a const pointer (`setProcessorCacheFlags`
/// is const), so another thread using the config meanwhile races with the guard
/// (docs/improvements.md, U-171); the port's flags are atomic, and that thread sees the cache
/// off.
///
/// Port of `ConfigUtils::SuspendCacheGuard` (src/OpenColorIO/ConfigUtils.h:78-101 @ v2.5.2).
pub(crate) struct SuspendCacheGuard<'a> {
    config: &'a Config,
    orig_cache_flags: ProcessorCacheFlags,
}

impl<'a> SuspendCacheGuard<'a> {
    pub(crate) fn new(config: &'a Config) -> SuspendCacheGuard<'a> {
        let orig_cache_flags = config.processor_cache_flags();
        config.set_processor_cache_flags(ProcessorCacheFlags::OFF);
        SuspendCacheGuard {
            config,
            orig_cache_flags,
        }
    }
}

impl Drop for SuspendCacheGuard<'_> {
    fn drop(&mut self) {
        self.config.set_processor_cache_flags(self.orig_cache_flags);
    }
}

/// The interchange spaces of the source config and the built-in config to convert from the
/// color space `src_color_space_name` to the built-in color space `builtin_color_space_name`,
/// or the reverse: the interchange roles, when both configs have them
/// ([`get_interchange_roles_for_color_space_conversion`]); else, for a scene-referred built-in
/// color space, the source config's first reference space ([`ref_space_name`]) and the
/// built-in linear space that an active sRGB texture space of the source config identifies,
/// else its first active linear space. An error when none is found.
///
/// The names are C strings (`const char *`): each up to its first NUL.
///
/// Port of `ConfigUtils::IdentifyInterchangeSpace` (src/OpenColorIO/ConfigUtils.cpp:590-710 @
/// v2.5.2).
pub(crate) fn identify_interchange_space<'s, 'b>(
    src_config: &'s Config,
    src_color_space_name: &[u8],
    builtin_config: &'b Config,
    builtin_color_space_name: &[u8],
) -> Result<(&'s [u8], &'b [u8])> {
    let builtin_color_space_name = c_str(builtin_color_space_name);

    // Before resorting to heuristics, check if the configs already have the interchange roles
    // defined. (The color space names are used only here, to choose the scene- or
    // display-referred interchange role.)
    let roles = get_interchange_roles_for_color_space_conversion(
        src_config,
        src_color_space_name,
        builtin_config,
        builtin_color_space_name,
    )?;
    if let Some(names) = roles.names {
        // No need for the heuristics.
        return Ok(names);
    }

    // Use heuristics to try and find a color space in the source config that matches a color
    // space in the Built-in config.

    // Currently only handling scene-referred spaces in the heuristics. (The color space exists:
    // the interchange roles' lookup checks it.)
    let builtin_color_space = builtin_config
        .color_space(builtin_color_space_name)
        .expect("the destination color space the interchange roles found");
    if builtin_color_space.reference_space_type() == ReferenceSpaceType::Display {
        return Err(Exception::new(
            "The heuristics currently only support scene-referred color spaces. Please set the \
             interchange roles.",
        ));
    }

    // Identify the name of a reference space in the source config.
    let src_interchange = ref_space_name(src_config);
    if src_interchange.is_empty() {
        return Err(Exception::new(
            "The supplied config does not have a color space for the reference.",
        ));
    }

    // The heuristics need to create a lot of Processors and send RGB values through them to
    // try and identify a known color space. Turn off the Processor cache in the configs to
    // avoid polluting the cache with transforms that won't be reused and avoid the overhead of
    // maintaining the cache.
    let _src_guard = SuspendCacheGuard::new(src_config);
    let _builtin_guard = SuspendCacheGuard::new(builtin_config);

    // Check for an sRGB texture space.
    let mut ref_color_space_prims_index = -1;
    let nb_cs = src_config.num_color_spaces();
    for i in 0..nb_cs {
        let cs = src_config
            .color_space(src_config.color_space_name_by_index(i))
            .expect("an active color space of the config");

        if contains_srgb(cs) {
            // Exclude color spaces that may be too expensive to test or otherwise
            // inappropriate. Currently only handling scene-referred spaces in the heuristics.
            if exclude_color_space_from_heuristics(cs, ReferenceSpaceType::Scene, true) {
                continue;
            }

            ref_color_space_prims_index =
                reference_space_from_srgb_space(src_config, src_interchange, cs, builtin_config)?;
            // Break out when a match is found.
            if ref_color_space_prims_index > -1 {
                break;
            }
        }
    }

    if ref_color_space_prims_index < 0 {
        // Check for a scene-linear space with known primaries.
        let nb_cs = src_config.num_color_spaces();
        for i in 0..nb_cs {
            let cs = src_config
                .color_space(src_config.color_space_name_by_index(i))
                .expect("an active color space of the config");

            // Exclude color spaces that may be too expensive to test or otherwise
            // inappropriate. Currently only handling scene-referred spaces in the heuristics.
            if exclude_color_space_from_heuristics(cs, ReferenceSpaceType::Scene, true) {
                continue;
            }

            if src_config.is_color_space_linear(cs.name(), ReferenceSpaceType::Scene)? {
                ref_color_space_prims_index = reference_space_from_linear_space(
                    src_config,
                    src_interchange,
                    cs,
                    builtin_config,
                )?;
                // Break out when a match is found.
                if ref_color_space_prims_index > -1 {
                    break;
                }
            }
        }
    }

    if ref_color_space_prims_index > -1 {
        Ok((
            src_interchange,
            builtin_linear_space_name(ref_color_space_prims_index),
        ))
    } else {
        Err(Exception::new(
            "Heuristics were not able to find a known color space in the provided config. \
             Please set the interchange roles.",
        ))
    }
}

/// The name of the active color space of the source config equivalent to the built-in config's
/// color space `builtin_color_space_name`: for a data space, the source config's first data
/// space; else the first active color space of that reference space type, not excluded
/// ([`exclude_color_space_from_heuristics`]), whose conversion to the built-in color space
/// through the interchange spaces ([`identify_interchange_space`]) is an identity within 5e-3.
/// An error when none is found.
///
/// The name is a C string (`const char *`): up to its first NUL.
///
/// Port of `ConfigUtils::IdentifyBuiltinColorSpace` (src/OpenColorIO/ConfigUtils.cpp:712-805 @
/// v2.5.2).
pub(crate) fn identify_builtin_color_space<'s>(
    src_config: &'s Config,
    builtin_config: &Config,
    builtin_color_space_name: &[u8],
) -> Result<&'s [u8]> {
    let builtin_color_space_name = c_str(builtin_color_space_name);

    // Note: Technically, the built-in config could be any config, if the interchange roles are
    // set in both configs, and the supplied built-in config supports the list of color spaces
    // returned by getBuiltinLinearSpaceName.

    let Some(builtin_color_space) = builtin_config.color_space(builtin_color_space_name) else {
        return Err(Exception::new(
            [
                b"Built-in config does not contain the requested color space: ".as_slice(),
                builtin_color_space_name,
                b".",
            ]
            .concat(),
        ));
    };

    if builtin_color_space.is_data() {
        let data_name = data_space_name(src_config);
        if data_name.is_empty() {
            return Err(Exception::new(
                "The requested space is a data space but the supplied config does not have a \
                 data space.",
            ));
        }
        return Ok(data_name);
    }

    let builtin_ref_space_type = builtin_color_space.reference_space_type();

    // Identify interchange spaces. Passing an empty string for the source color space means
    // that only the builtinColorSpace will be used to determine the reference space type of
    // the interchange role. Will throw if the space cannot be found.
    let (src_interchange_name, builtin_interchange_name) =
        identify_interchange_space(src_config, b"", builtin_config, builtin_color_space_name)?;

    // The heuristics need to create a lot of Processors: turn off the Processor cache in the
    // configs (see identify_interchange_space).
    let _src_guard = SuspendCacheGuard::new(src_config);
    let _builtin_guard = SuspendCacheGuard::new(builtin_config);

    if !builtin_interchange_name.is_empty() {
        let vals = RGBA_TEST_VALS;

        // Loop over the active, non-excluded, color spaces in the source config and test if the
        // conversion to the specified space in the built-in config is an identity.
        let nb_cs = src_config.num_color_spaces();
        for i in 0..nb_cs {
            let cs = src_config
                .color_space(src_config.color_space_name_by_index(i))
                .expect("an active color space of the config");

            if exclude_color_space_from_heuristics(cs, builtin_ref_space_type, false) {
                continue;
            }

            let proc = Config::processor_from_configs_with_interchange(
                src_config,
                cs.name(),
                src_interchange_name,
                builtin_config,
                builtin_color_space_name,
                builtin_interchange_name,
            )?;
            // Tolerance is just loose enough to accept both bfd and cat02 adaptation.
            if is_identity_transform(&proc, &vals, 5e-3)? {
                return Ok(cs.name());
            }
        }
    }

    Err(Exception::new(
        [
            b"Heuristics were not able to find an equivalent to the requested color space: "
                .as_slice(),
            builtin_color_space_name,
            b".",
        ]
        .concat(),
    ))
}
