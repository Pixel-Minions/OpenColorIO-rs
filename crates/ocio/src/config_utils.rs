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

// The reference-space converters and the color-space fingerprints (config merging) //////////

use ocio_ops::open_color_types::ViewTransformDirection;

use crate::transforms::color_space_transform::ColorSpaceTransform;
use crate::transforms::group_transform::GroupTransform;
use crate::transforms::matrix_transform::MatrixTransform;
use crate::view_transform::ViewTransform;

/// The transform of the group `gt` without nested groups and identities: its processor in the
/// raw config, optimized at `OPTIMIZATION_DEFAULT`, read back as a group, or the group's only
/// transform.
///
/// Port of `ConfigUtils::simplifyTransform` (src/OpenColorIO/ConfigUtils.cpp:843-855 @
/// v2.5.2).
// Config merging (WP 9.2) calls it.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn simplify_transform(gt: &GroupTransform) -> Result<Transform> {
    let config = Config::create_raw()?;
    let p = config.processor(&Transform::Group(gt.clone()))?;
    let opt = p.optimized_processor(OptimizationFlags::DEFAULT)?;
    let final_gt = opt.create_group_transform()?;
    if final_gt.num_transforms() == 1 {
        return Ok(final_gt.transform(0)?.clone());
    }
    Ok(Transform::Group(final_gt))
}

/// A copy of `t` whose direction is set to inverse: a transform already in the inverse
/// direction stays as it is (docs/improvements.md, I-379).
///
/// Port of `ConfigUtils::invertTransform` (src/OpenColorIO/ConfigUtils.cpp:857-862 @ v2.5.2).
// Config merging (WP 9.2) calls it.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn invert_transform(t: &Transform) -> Result<Transform> {
    let mut e_t = t.create_editable_copy()?;
    e_t.set_direction(TransformDirection::Inverse);
    Ok(e_t)
}

/// The color space's transform in the direction `dir`: its own, else its other one through
/// [`invert_transform`], else (a reference space) an identity matrix.
///
/// Port of `ConfigUtils::getTransformForDir` (src/OpenColorIO/ConfigUtils.cpp:864-893 @ v2.5.2).
// Config merging (WP 9.2) calls it.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn transform_for_dir(cs: &ColorSpace, dir: ColorSpaceDirection) -> Result<Transform> {
    let other_dir = if dir == ColorSpaceDirection::ToReference {
        ColorSpaceDirection::FromReference
    } else {
        ColorSpaceDirection::ToReference
    };

    if let Some(t) = cs.transform(dir) {
        return Ok(t.clone());
    }

    if let Some(t_other) = cs.transform(other_dir) {
        return invert_transform(t_other);
    }

    // If it's the reference space, it won't have a transform, so return an identity matrix.
    let (m44, offset4) = MatrixTransform::identity();
    let mut p = MatrixTransform::new();
    p.set_matrix(&m44);
    p.set_offset(&offset4);

    Ok(Transform::from(p))
}

/// The search type of a reference space type: `SearchReferenceSpaceType` has the same values
/// (upstream's `static_cast`).
// Config merging (WP 9.2) calls it.
#[cfg_attr(not(test), allow(dead_code))]
fn search_type(ref_type: ReferenceSpaceType) -> SearchReferenceSpaceType {
    match ref_type {
        ReferenceSpaceType::Scene => SearchReferenceSpaceType::Scene,
        ReferenceSpaceType::Display => SearchReferenceSpaceType::Display,
    }
}

/// The name of the first color space of `config` of the reference space type `ref_type`,
/// inactive and data spaces included; an error when there is none.
///
/// Port of the lambda `getColorspaceOfRefType` of `ConfigUtils::getRefSpaceConverter`
/// (src/OpenColorIO/ConfigUtils.cpp:904-918 @ v2.5.2).
// Config merging (WP 9.2) calls it.
#[cfg_attr(not(test), allow(dead_code))]
fn color_space_of_ref_type(config: &Config, ref_type: ReferenceSpaceType) -> Result<&[u8]> {
    // Just return the first one, doesn't matter if it's inactive or a data space. All that
    // matters is the reference space type.
    let search_ref_type = search_type(ref_type);
    if config.num_color_spaces_with(search_ref_type, ColorSpaceVisibility::All) > 0 {
        let name =
            config.color_space_name_by_index_with(search_ref_type, ColorSpaceVisibility::All, 0);
        let cs = config
            .color_space(name)
            .expect("a color space of the config");
        return Ok(cs.name());
    }

    Err(Exception::new(
        "Config is lacking any color spaces of the requested reference space type.",
    ))
}

/// The transform from the reference space of `src_config` to that of `dst_config`, of the
/// reference space type `ref_space_type`: each config's interchange space
/// ([`identify_interchange_space`] against the latest CG built-in config), the source one's
/// from-reference transform, the built-in config's conversion between the two built-in
/// interchange spaces, and the destination one's to-reference transform, each resolved into
/// ops, then simplified ([`simplify_transform`]).
///
/// Port of `ConfigUtils::getRefSpaceConverter` (src/OpenColorIO/ConfigUtils.cpp:895-993 @
/// v2.5.2).
// Config merging (WP 9.2) calls it.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn ref_space_converter(
    src_config: &Config,
    dst_config: &Config,
    ref_space_type: ReferenceSpaceType,
) -> Result<Transform> {
    let builtin_config = Config::create_from_builtin_config("ocio://cg-config-latest")?;

    // Identify an interchange space for the src config. Note that the interchange space will
    // always be a linear color space.
    let (src_interchange, src_builtin_interchange) = identify_interchange_space(
        src_config,
        color_space_of_ref_type(src_config, ref_space_type)?,
        &builtin_config,
        color_space_of_ref_type(&builtin_config, ref_space_type)?,
    )?;

    // Identify an interchange space for the dst config.
    let (dst_interchange, dst_builtin_interchange) = identify_interchange_space(
        dst_config,
        color_space_of_ref_type(dst_config, ref_space_type)?,
        &builtin_config,
        color_space_of_ref_type(&builtin_config, ref_space_type)?,
    )?;

    // Get the from_ref transform from the srcInterchange space. (The names come from the
    // configs' own color spaces.)
    let src_from_ref = transform_for_dir(
        src_config
            .color_space(src_interchange)
            .expect("the source interchange space"),
        ColorSpaceDirection::FromReference,
    )?;

    // Get a conversion from one builtin interchange to another. The interchange spaces are
    // never empty (identify_interchange_space returns color spaces' names or throws), so
    // upstream's null group, which appendTransform would add, can't happen.
    (!src_builtin_interchange.is_empty() && !dst_builtin_interchange.is_empty())
        .then_some(())
        .expect("identify_interchange_space returns color spaces' names or throws, never empty");
    let mut cst = ColorSpaceTransform::new();
    cst.set_src(src_builtin_interchange);
    cst.set_dst(dst_builtin_interchange);
    let src_builtin_to_dst_builtin = builtin_config
        .processor(&Transform::from(cst))?
        .create_group_transform()?;

    // Append to_ref transform from the dstInterchange space.
    let dst_to_ref = transform_for_dir(
        dst_config
            .color_space(dst_interchange)
            .expect("the destination interchange space"),
        ColorSpaceDirection::ToReference,
    )?;

    // Combine into a group transform. If the src or dst contain FileTransforms, resolve them
    // so there is no dependence on the search_path of the original configs.
    let mut gt = GroupTransform::new();
    gt.append_transform(Transform::Group(
        src_config
            .processor(&src_from_ref)?
            .create_group_transform()?,
    ));
    gt.append_transform(Transform::Group(src_builtin_to_dst_builtin));
    gt.append_transform(Transform::Group(
        dst_config
            .processor(&dst_to_ref)?
            .create_group_transform()?,
    ));

    simplify_transform(&gt)
}

/// Whether the transform is a group without transforms.
///
/// Port of `ConfigUtils::transformIsEmpty` (src/OpenColorIO/ConfigUtils.cpp:995-1006 @ v2.5.2).
// Config merging (WP 9.2) calls it.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn transform_is_empty(tr: &Transform) -> bool {
    matches!(tr, Transform::Group(gt) if gt.num_transforms() == 0)
}

/// Changes the reference space of the color space's transforms by
/// `to_new_reference_transform` (from the current reference space to the new one): appended to
/// its to-reference transform, its inverse ([`invert_transform`]) prepended to its
/// from-reference transform, or, without transforms and for a color space that isn't data, as
/// its to-reference transform; each in a new group, not simplified. An empty group changes
/// nothing. (Upstream's error for a null transform or color space can't happen with
/// references.)
///
/// Port of `ConfigUtils::updateReferenceColorspace` (src/OpenColorIO/ConfigUtils.cpp:1008-1057
/// @ v2.5.2).
// Config merging (WP 9.2) calls it.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn update_reference_colorspace(
    cs: &mut ColorSpace,
    to_new_reference_transform: &Transform,
) -> Result<()> {
    if transform_is_empty(to_new_reference_transform) {
        return Ok(());
    }

    let transform_to = cs.transform(ColorSpaceDirection::ToReference).cloned();
    if let Some(transform_to) = &transform_to {
        let mut gt = GroupTransform::new();
        gt.append_transform(transform_to.create_editable_copy()?);
        gt.append_transform(to_new_reference_transform.create_editable_copy()?);

        // NB: Don't want to call simplify_transforms on gt since it would do things like
        // expand built-in or file transforms.
        cs.set_transform(
            Some(&Transform::Group(gt)),
            ColorSpaceDirection::ToReference,
        )?;
    }

    let transform_from = cs.transform(ColorSpaceDirection::FromReference).cloned();
    if let Some(transform_from) = &transform_from {
        let inv = invert_transform(to_new_reference_transform)?;
        let mut gt = GroupTransform::new();
        gt.append_transform(inv.create_editable_copy()?);
        gt.append_transform(transform_from.create_editable_copy()?);
        cs.set_transform(
            Some(&Transform::Group(gt)),
            ColorSpaceDirection::FromReference,
        )?;
    }

    if transform_to.is_none() && transform_from.is_none() && !cs.is_data() {
        let mut gt = GroupTransform::new();
        gt.append_transform(to_new_reference_transform.create_editable_copy()?);
        cs.set_transform(
            Some(&Transform::Group(gt)),
            ColorSpaceDirection::ToReference,
        )?;
    }
    Ok(())
}

/// Changes the reference spaces of the view transform's transforms: its to-reference transform
/// gets the inverse of the display converter before it and the display converter (a
/// display-referred view transform) or the scene converter after it; its from-reference
/// transform the inverse of the display or scene converter before it and the display converter
/// after it. An empty converter is left out; with both empty nothing changes. (Upstream's error
/// for null transforms can't happen with references.)
///
/// Port of `ConfigUtils::updateReferenceView` (src/OpenColorIO/ConfigUtils.cpp:1059-1138 @
/// v2.5.2).
// Config merging (WP 9.2) calls it.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn update_reference_view(
    vt: &mut ViewTransform,
    to_new_scene_reference_transform: &Transform,
    to_new_display_reference_transform: &Transform,
) -> Result<()> {
    let empty_scene_side = transform_is_empty(to_new_scene_reference_transform);
    let empty_display_side = transform_is_empty(to_new_display_reference_transform);

    if empty_scene_side && empty_display_side {
        return Ok(());
    }

    if let Some(transform_to) = vt.transform(ViewTransformDirection::ToReference).cloned() {
        let mut gt = GroupTransform::new();
        if !empty_display_side {
            let inv = invert_transform(to_new_display_reference_transform)?;
            gt.append_transform(inv.create_editable_copy()?);
        }

        gt.append_transform(transform_to.create_editable_copy()?);

        if vt.reference_space_type() == ReferenceSpaceType::Display {
            // Use the converter to display reference on both sides.
            if !empty_display_side {
                gt.append_transform(to_new_display_reference_transform.create_editable_copy()?);
            }
        } else if !empty_scene_side {
            gt.append_transform(to_new_scene_reference_transform.create_editable_copy()?);
        }
        vt.set_transform(
            Some(&Transform::Group(gt)),
            ViewTransformDirection::ToReference,
        )?;
    }

    if let Some(transform_from) = vt.transform(ViewTransformDirection::FromReference).cloned() {
        let mut gt = GroupTransform::new();

        if vt.reference_space_type() == ReferenceSpaceType::Display {
            // Use the converter to display reference on both sides.
            if !empty_display_side {
                let inv = invert_transform(to_new_display_reference_transform)?;
                gt.append_transform(inv.create_editable_copy()?);
            }
        } else if !empty_scene_side {
            let inv = invert_transform(to_new_scene_reference_transform)?;
            gt.append_transform(inv.create_editable_copy()?);
        }

        gt.append_transform(transform_from.create_editable_copy()?);

        if !empty_display_side {
            gt.append_transform(to_new_display_reference_transform.create_editable_copy()?);
        }
        vt.set_transform(
            Some(&Transform::Group(gt)),
            ViewTransformDirection::FromReference,
        )?;
    }

    // Note that Config::addViewTransform prevents creating a view transform that has no
    // transforms, so at least one direction will be present.
    Ok(())
}

/// Whether the config has a color space of the reference space type `ref_type`, inactive ones
/// included.
///
/// Port of `ConfigUtils::hasColorSpaceRefType` (src/OpenColorIO/ConfigUtils.cpp:1140-1145 @
/// v2.5.2).
// Config merging (WP 9.2) calls it.
#[allow(dead_code)]
pub(crate) fn has_color_space_ref_type(config: &Config, ref_type: ReferenceSpaceType) -> bool {
    let search_ref_type = search_type(ref_type);
    let n = config.num_color_spaces_with(search_ref_type, ColorSpaceVisibility::All);
    n > 0
}

/// The transforms that convert the input config's scene-referred and display-referred
/// reference spaces to the base config's ([`ref_space_converter`]), each an empty group when
/// either config lacks color spaces of that type. The base config's reference space is always
/// used.
///
/// Port of `ConfigUtils::initializeRefSpaceConverters` (src/OpenColorIO/ConfigUtils.cpp:
/// 1147-1189 @ v2.5.2), returning upstream's two out-parameters `(inputToBaseGtScene,
/// inputToBaseGtDisplay)`.
// Config merging (WP 9.2) calls it.
#[allow(dead_code)]
pub(crate) fn initialize_ref_space_converters(
    base_config: &Config,
    input_config: &Config,
) -> Result<(Transform, Transform)> {
    let input_to_base_gt_scene = if has_color_space_ref_type(base_config, ReferenceSpaceType::Scene)
        && has_color_space_ref_type(input_config, ReferenceSpaceType::Scene)
    {
        ref_space_converter(input_config, base_config, ReferenceSpaceType::Scene)?
    } else {
        // Always need to initialize both transforms, even if they're empty.
        Transform::Group(GroupTransform::new())
    };

    // Only attempt to build the converter if both configs have this type of reference space.
    let input_to_base_gt_display =
        if has_color_space_ref_type(base_config, ReferenceSpaceType::Display)
            && has_color_space_ref_type(input_config, ReferenceSpaceType::Display)
        {
            ref_space_converter(input_config, base_config, ReferenceSpaceType::Display)?
        } else {
            // Always need to initialize both transforms, even if they're empty.
            Transform::Group(GroupTransform::new())
        };

    Ok((input_to_base_gt_scene, input_to_base_gt_display))
}

/// The fingerprint of a color space: its name, its reference space type, and its values of
/// the test values.
///
/// Port of `ConfigUtils::Fingerprint` (src/OpenColorIO/ConfigUtils.h:59-64 @ v2.5.2); the name
/// is owned, where upstream points into the config.
#[derive(Debug, Clone, PartialEq)]
// Config merging (WP 9.2) calls it.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct Fingerprint {
    /// `csName`.
    pub(crate) cs_name: Vec<u8>,
    /// `type`.
    pub(crate) type_: ReferenceSpaceType,
    /// `vals`.
    pub(crate) vals: Vec<f32>,
}

/// The fingerprints of a config's color spaces, and the test values they are made of.
///
/// Port of `ConfigUtils::ColorSpaceFingerprints` (src/OpenColorIO/ConfigUtils.h:66-71 @
/// v2.5.2).
#[derive(Debug, Clone, Default, PartialEq)]
// Config merging (WP 9.2) calls it.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct ColorSpaceFingerprints {
    /// `vec`.
    pub(crate) vec: Vec<Fingerprint>,
    /// `sceneRefTestVals`.
    pub(crate) scene_ref_test_vals: Vec<f32>,
    /// `displayRefTestVals`.
    pub(crate) display_ref_test_vals: Vec<f32>,
}

/// Sends the test values of the color space's reference space type through its from-reference
/// transform ([`transform_for_dir`]) into `fingerprint_vals`. Returns true when the color space
/// should not be considered: its processor or CPU processor can't be made (whatever the
/// error).
///
/// Port of `ConfigUtils::calcColorSpaceFingerprint` (src/OpenColorIO/ConfigUtils.cpp:1191-1229
/// @ v2.5.2).
// Config merging (WP 9.2) calls it.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn calc_color_space_fingerprint(
    fingerprint_vals: &mut Vec<f32>,
    fingerprints: &ColorSpaceFingerprints,
    config: &Config,
    cs: &ColorSpace,
) -> Result<bool> {
    let skip_color_space = false;

    let from_ref = transform_for_dir(cs, ColorSpaceDirection::FromReference)?;

    let cpu = match config
        .processor(&from_ref)
        .and_then(|p| p.optimized_cpu_processor(OptimizationFlags::NONE))
    {
        Ok(cpu) => cpu,
        // If the transform doesn't validate (singular matrix, etc.), don't consider it.
        Err(_) => return Ok(true),
    };

    if cs.reference_space_type() == ReferenceSpaceType::Display {
        fingerprint_vals.clone_from(&fingerprints.display_ref_test_vals);
    } else {
        fingerprint_vals.clone_from(&fingerprints.scene_ref_test_vals);
    }
    let n = fingerprint_vals.len();
    let mut desc = PackedImageDesc::with_channel_order(
        &mut fingerprint_vals[..],
        n / 4,
        1,
        ChannelOrdering::Rgba,
    )?;

    cpu.apply(&mut desc)?;

    Ok(skip_color_space)
}

/// The values `config`'s converter of `vals` gives: the color space's to-reference transform
/// ([`transform_for_dir`]) applied without optimization.
// Config merging (WP 9.2) calls it.
#[cfg_attr(not(test), allow(dead_code))]
fn to_reference_values(config: &Config, cs: &ColorSpace, vals: &[f32]) -> Result<Vec<f32>> {
    let to_ref = transform_for_dir(cs, ColorSpaceDirection::ToReference)?;
    let p = config.processor(&to_ref)?;

    let n = vals.len();
    let mut out = vec![0f32; n];

    {
        let desc_src = PackedImageDesc::with_channel_order(vals, n / 4, 1, ChannelOrdering::Rgba)?;
        let mut desc_dst =
            PackedImageDesc::with_channel_order(&mut out[..], n / 4, 1, ChannelOrdering::Rgba)?;

        let cpu = p.optimized_cpu_processor(OptimizationFlags::NONE)?;
        cpu.apply_src_dst(&desc_src, &mut desc_dst)?;
    }
    Ok(out)
}

/// The config's color space found by the first of `names` it has (a name, alias or role), else
/// the one the heuristics find equivalent to the latest CG built-in config's `builtin_name`.
// Config merging (WP 9.2) calls it.
#[cfg_attr(not(test), allow(dead_code))]
fn common_color_space<'a>(
    config: &'a Config,
    names: &[&str],
    builtin_name: &str,
) -> Result<&'a ColorSpace> {
    for name in names {
        if let Some(cs) = config.color_space(name) {
            return Ok(cs);
        }
    }
    // Otherwise, see if it's present using a different name.
    let builtin_config = Config::create_from_builtin_config("ocio://cg-config-latest")?;
    // This throws if it cannot find the requested space.
    let cs_name = identify_builtin_color_space(config, &builtin_config, builtin_name.as_bytes())?;
    Ok(config
        .color_space(cs_name)
        .expect("a color space of the config"))
}

/// Sets the test values of the fingerprints of `config`: values slightly inside the Rec.709
/// gamut in ACES2065-1 (scene-referred) and in CIE XYZ D65 (display-referred), converted to the
/// config's reference spaces through its ACES2065-1 and CIE XYZ D65 spaces (by common names,
/// else found by the heuristics); a conversion that fails, whatever the error, leaves the
/// values as they are. Without display-referred color spaces the display values aren't
/// converted.
///
/// Port of `ConfigUtils::initializeTestVals` (src/OpenColorIO/ConfigUtils.cpp:1231-1365 @
/// v2.5.2).
// Config merging (WP 9.2) calls it.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn initialize_test_vals(fingerprints: &mut ColorSpaceFingerprints, config: &Config) {
    // Define a set of test values that are slightly inside the Rec.709 gamut for the most
    // common scene-referred and display-referred reference spaces.
    #[rustfmt::skip]
    let aces_vals: Vec<f32> = vec![
        0.408_933_127_871, 0.106_169_822_808, 0.027_842_572_707, 0., // lin_rec709 {0.9, 0.03, 0.01}
        0.374_615_373_650, 0.739_417_755_017, 0.118_862_613_721, 0., // lin_rec709 {0.06, 0.9, 0.02}
        0.171_696_591_718, 0.104_272_268_468, 0.786_227_391_453, 0., // lin_rec709 {0.01, 0.02, 0.9}
        0.,                0.,                0.,                0.5,
        0.037_018_876_439, 0.030_827_687_576, 0.021_641_700_645, 0., // lin_rec709 {0.05, 0.03, 0.02}
        1.,                1.,                1.,                1.,
    ];

    #[rustfmt::skip]
    let xyz_vals: Vec<f32> = vec![
        // Adjusted to keep it inside both Rec.601 and Rec.601 PAL.
        0.383_684_057_405, 0.213_552_088_801, 0.030_478_901_760, 0., // lin_rec709 {0.9, 0.03, 0.01}
        0.350_178_969_169, 0.657_853_997_550, 0.127_445_793_983, 0., // lin_rec709 {0.06, 0.9, 0.02}
        0.173_708_304_342, 0.081_402_847_459, 0.858_056_140_808, 0., // lin_rec709 {0.01, 0.02, 0.9}
        0.,                0.,                0.,                0.5,
        0.034_956_685_913, 0.033_530_856_964, 0.023_553_027_375, 0., // lin_rec709 {0.05, 0.03, 0.02}
        0.950_455_927_052, 1.,                1.089_057_750_760, 1.,
    ];

    // Try to convert to the actual reference spaces of the config.

    fingerprints.scene_ref_test_vals = aces_vals.clone();
    fingerprints.display_ref_test_vals = xyz_vals.clone();

    // First check if the config recognizes one of the common names.
    let scene = common_color_space(
        config,
        &["aces_interchange", "ACES2065-1", "lin_ap0_scene"],
        "aces_interchange",
    )
    .and_then(|cs| to_reference_values(config, cs, &aces_vals));
    fingerprints.scene_ref_test_vals = scene.unwrap_or(aces_vals);

    let m =
        config.num_color_spaces_with(SearchReferenceSpaceType::Display, ColorSpaceVisibility::All);
    if m == 0 {
        return;
    }

    let display = common_color_space(
        config,
        &["cie_xyz_d65_interchange", "CIE-XYZ-D65", "CIE XYZ-D65"],
        "cie_xyz_d65_interchange",
    )
    .and_then(|cs| to_reference_values(config, cs, &xyz_vals));
    fingerprints.display_ref_test_vals = display.unwrap_or(xyz_vals);
}

/// The fingerprints of every color space of `config` ([`calc_color_space_fingerprint`]),
/// inactive ones included, but data spaces, those of the category `is-unique`, those with
/// both transforms, and those the fingerprint skips; with the config's processor cache off
/// meanwhile.
///
/// Port of `ConfigUtils::initializeColorSpaceFingerprints` (src/OpenColorIO/ConfigUtils.cpp:
/// 1367-1418 @ v2.5.2).
// Config merging (WP 9.2) calls it.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn initialize_color_space_fingerprints(
    fingerprints: &mut ColorSpaceFingerprints,
    config: &Config,
) -> Result<()> {
    let _src_guard = SuspendCacheGuard::new(config);

    initialize_test_vals(fingerprints, config);

    let n = config.num_color_spaces_with(SearchReferenceSpaceType::All, ColorSpaceVisibility::All);
    fingerprints.vec.clear();
    fingerprints.vec.reserve(usize::try_from(n).unwrap_or(0));
    for i in 0..n {
        let name = config.color_space_name_by_index_with(
            SearchReferenceSpaceType::All,
            ColorSpaceVisibility::All,
            i,
        );
        let Some(cs) = config.color_space(name) else {
            continue;
        };
        if cs.is_data() {
            // Don't put data color spaces in the collection.
            continue;
        }
        if cs.has_category("is-unique") {
            // Don't fingerprint color spaces with this category. This provides a way to
            // identify color spaces that must not be replaced.
            continue;
        }

        let t_from = cs.transform(ColorSpaceDirection::FromReference);
        let t_to = cs.transform(ColorSpaceDirection::ToReference);
        if t_from.is_some() && t_to.is_some() {
            // Don't bother with color spaces that have both directions defined, these are
            // more complicated and less likely to be duplicates.
            continue;
        }

        let mut fp = Vec::new();
        let skip_color_space = calc_color_space_fingerprint(&mut fp, fingerprints, config, cs)?;
        if !skip_color_space {
            fingerprints.vec.push(Fingerprint {
                cs_name: cs.name().to_vec(),
                type_: cs.reference_space_type(),
                vals: fp,
            });
        }
    }
    Ok(())
}

/// The name of the base config's color space equivalent to `input_cs` of `input_config`: the
/// first fingerprint of the same reference space type whose values are all within 5e-3 of the
/// input color space's; `""` for none, for a data space, or for a color space the fingerprint
/// skips. The input color space must use the base config's reference space
/// ([`update_reference_colorspace`]).
///
/// Port of `ConfigUtils::findEquivalentColorspace` (src/OpenColorIO/ConfigUtils.cpp:1420-1478
/// @ v2.5.2).
// Config merging (WP 9.2) calls it.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn find_equivalent_colorspace<'f>(
    fingerprints: &'f ColorSpaceFingerprints,
    input_config: &Config,
    input_cs: &ColorSpace,
) -> Result<&'f [u8]> {
    if input_cs.is_data() {
        return Ok(b"");
    }

    // Calculate the fingerprint of inputCS from inputConfig.
    let mut input_vals = Vec::new();
    let skip_color_space =
        calc_color_space_fingerprint(&mut input_vals, fingerprints, input_config, input_cs)?;
    if skip_color_space {
        return Ok(b"");
    }

    // Increased from 1e-3 to 5e-3 to allow for use of either Bradford or CAT02 adaptation.
    let abs_tolerance = 5e-3f32;

    // Compare to the fingerprints from the base config.
    for fp in &fingerprints.vec {
        // Only compare color spaces that are using the same reference space type.
        if fp.type_ != input_cs.reference_space_type() {
            continue;
        }

        let mut match_found = true;
        for (input, val) in input_vals.iter().zip(&fp.vals) {
            // Comparison is done in the color space, not the reference space. (Upstream reads
            // fp.vals[i] for i below inputVals.size(): both hold the 24 test values.)
            if !equal_with_abs_error(*input, *val, abs_tolerance) {
                match_found = false;
                continue;
            }
        }
        if match_found {
            return Ok(&fp.cs_name);
        }
    }

    Ok(b"")
}

#[cfg(test)]
#[path = "config_utils_tests.rs"]
mod tests;
