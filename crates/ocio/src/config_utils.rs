// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The helpers of the conversions between two configs: the interchange roles of a pair of
//! color spaces.
//!
//! Port of `ConfigUtils` (src/OpenColorIO/ConfigUtils.cpp, ConfigUtils.h @ v2.5.2), in part:
//! `GetInterchangeRolesForColorSpaceConversion`.

use ocio_ops::exception::{Exception, Result};
use ocio_ops::open_color_types::ReferenceSpaceType;
use ocio_ops::parse_utils::{ROLE_INTERCHANGE_DISPLAY, ROLE_INTERCHANGE_SCENE};
use ocio_ops::utils::string_utils::c_str;

use crate::config::Config;

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
