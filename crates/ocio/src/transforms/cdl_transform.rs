// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CDL transform: a port of `src/OpenColorIO/transforms/CDLTransform.h` and
//! `CDLTransform.cpp` @ v2.5.2, without its files (`CreateFromFile`, `CreateGroupFromFile` and
//! `GetCDL`, which read .cc, .ccc and .cdl files: Phase 4), with its op glue from
//! `src/OpenColorIO/ops/cdl/CDLOp.cpp` (`CreateCDLTransform`, `BuildCDLOp`).

use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::op::{Op, OpVec};
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::{
    CdlStyle, TransformDirection, cdl_style_to_string, combine_transform_directions,
    transform_direction_to_string,
};
use ocio_ops::ops::cdl::cdl_op::create_cdl_op;
use ocio_ops::ops::cdl::{CdlOpData, ChannelParams};
use ocio_ops::ops::exponent::exponent_op::create_exponent_op_from_values;
use ocio_ops::ops::matrix::matrix_op::{create_saturation_op, create_scale_offset_op};
use ocio_ops::utils::string_utils::c_str;

use crate::config::Config;
use crate::transform::validate_direction;
use crate::transforms::group_transform::GroupTransform;

// The CDL metadata's element names: private, as in upstream's CDLTransform.h (not part of
// the public headers). The CDL and CTF readers and writers use the others (Phase 4).

/// `METADATA_INPUT_DESCRIPTION` (src/OpenColorIO/transforms/CDLTransform.h:17 @ v2.5.2).
#[allow(dead_code)]
pub(crate) const METADATA_INPUT_DESCRIPTION: &[u8] = b"InputDescription";
/// `METADATA_VIEWING_DESCRIPTION` (src/OpenColorIO/transforms/CDLTransform.h:18 @ v2.5.2).
#[allow(dead_code)]
pub(crate) const METADATA_VIEWING_DESCRIPTION: &[u8] = b"ViewingDescription";
/// `METADATA_SOP_DESCRIPTION` (src/OpenColorIO/transforms/CDLTransform.h:19 @ v2.5.2): the name
/// of the metadata's children that hold the slope, offset and power's descriptions.
pub(crate) const METADATA_SOP_DESCRIPTION: &[u8] = b"SOPDescription";
/// `METADATA_SAT_DESCRIPTION` (src/OpenColorIO/transforms/CDLTransform.h:20 @ v2.5.2).
#[allow(dead_code)]
pub(crate) const METADATA_SAT_DESCRIPTION: &[u8] = b"SATDescription";

/// An ASC Color Decision List: `out = clamp((in * slope + offset) ^ power)`, then the
/// saturation around Rec. 709 luma, per the style (ASC v1.2, which clamps, or without
/// clamping), with an ID and descriptions in its metadata.
///
/// The transform is its op data, a `CDLOpData`, as upstream's `CDLTransformImpl` holds it; the
/// data's style carries the direction. A copy is upstream's `createEditableCopy`.
///
/// Port of `CDLTransform` and `CDLTransformImpl` (include/OpenColorIO/
/// OpenColorTransforms.h:244-330, src/OpenColorIO/transforms/CDLTransform.h,
/// CDLTransform.cpp:23-32, 120-398 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct CdlTransform {
    /// `m_data`.
    data: CdlOpData,
}

impl Default for CdlTransform {
    fn default() -> Self {
        Self::new()
    }
}

impl CdlTransform {
    /// The identity CDL: slope 1, offset 0, power 1, saturation 1, without clamping, forward.
    ///
    /// Port of `CDLTransform::Create` and `CDLTransformImpl::Create` (CDLTransform.cpp:23-32 @
    /// v2.5.2), with the default `CDLOpData` (CDLTransform.h:82).
    #[doc(alias = "Create")]
    pub fn new() -> CdlTransform {
        CdlTransform {
            data: CdlOpData::new_default(),
        }
    }

    /// The op data the transform holds.
    ///
    /// Port of `CDLTransformImpl::data() const` (CDLTransform.h:76 @ v2.5.2).
    pub(crate) fn data(&self) -> &CdlOpData {
        &self.data
    }

    /// The direction, which the data's style carries.
    ///
    /// Port of `CDLTransformImpl::getDirection` (CDLTransform.cpp:132-135 @ v2.5.2).
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        self.data.get_direction()
    }

    /// Port of `CDLTransformImpl::setDirection` (CDLTransform.cpp:137-140 @ v2.5.2).
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.data.set_direction(dir);
    }

    /// Checks the direction and the data: "CDLTransform validation failed: " and the first
    /// problem.
    ///
    /// Port of `CDLTransformImpl::validate` (CDLTransform.cpp:142-155 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        let checked = validate_direction(self.direction()).and_then(|()| self.data.validate());
        checked.map_err(|ex| {
            Exception::new(format!("CDLTransform validation failed: {}", ex.message()))
        })
    }

    /// Port of `CDLTransformImpl::getFormatMetadata() const` (CDLTransform.cpp:162-165 @
    /// v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata(&self) -> &FormatMetadataImpl {
        self.data.get_format_metadata()
    }

    /// Port of `CDLTransformImpl::getFormatMetadata()` (CDLTransform.cpp:157-160 @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        self.data.get_format_metadata_mut()
    }

    /// Whether `other` has the same data: the style and the parameters, within the data's
    /// tolerance of 1e-9; the metadata is ignored. A transform equals itself.
    ///
    /// Port of `CDLTransformImpl::equals` (CDLTransform.cpp:170-175 @ v2.5.2).
    pub fn equals(&self, other: &CdlTransform) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        // NB: A tolerance of 1e-9 is used when comparing the parameters.
        self.data == other.data
    }

    /// Whether the CDL clamps (ASC v1.2) or not.
    ///
    /// Port of `CDLTransformImpl::getStyle` (CDLTransform.cpp:177-180 @ v2.5.2).
    #[doc(alias = "getStyle")]
    pub fn style(&self) -> CdlStyle {
        CdlOpData::convert_style_to_transform(self.data.get_style())
    }

    /// Sets the style, in the current direction.
    ///
    /// Port of `CDLTransformImpl::setStyle` (CDLTransform.cpp:182-186 @ v2.5.2).
    #[doc(alias = "setStyle")]
    pub fn set_style(&mut self, style: CdlStyle) {
        let cur_dir = self.direction();
        self.data
            .set_style(CdlOpData::convert_style(style, cur_dir));
    }

    /// Sets the R, G, B slopes. (Upstream's error for a null pointer can't happen with an
    /// array.)
    ///
    /// Port of `CDLTransformImpl::setSlope` (CDLTransform.cpp:188-196 @ v2.5.2).
    #[doc(alias = "setSlope")]
    pub fn set_slope(&mut self, rgb: &[f64; 3]) {
        self.data
            .set_slope_params(ChannelParams::new(rgb[0], rgb[1], rgb[2]));
    }

    /// The R, G, B slopes.
    ///
    /// Port of `CDLTransformImpl::getSlope` (CDLTransform.cpp:198-209 @ v2.5.2).
    #[doc(alias = "getSlope")]
    pub fn slope(&self) -> [f64; 3] {
        self.data.get_slope_params().get_rgb()
    }

    /// Sets the R, G, B offsets.
    ///
    /// Port of `CDLTransformImpl::setOffset` (CDLTransform.cpp:211-219 @ v2.5.2).
    #[doc(alias = "setOffset")]
    pub fn set_offset(&mut self, rgb: &[f64; 3]) {
        self.data
            .set_offset_params(ChannelParams::new(rgb[0], rgb[1], rgb[2]));
    }

    /// The R, G, B offsets.
    ///
    /// Port of `CDLTransformImpl::getOffset` (CDLTransform.cpp:221-232 @ v2.5.2).
    #[doc(alias = "getOffset")]
    pub fn offset(&self) -> [f64; 3] {
        self.data.get_offset_params().get_rgb()
    }

    /// Sets the R, G, B powers.
    ///
    /// Port of `CDLTransformImpl::setPower` (CDLTransform.cpp:234-242 @ v2.5.2).
    #[doc(alias = "setPower")]
    pub fn set_power(&mut self, rgb: &[f64; 3]) {
        self.data
            .set_power_params(ChannelParams::new(rgb[0], rgb[1], rgb[2]));
    }

    /// The R, G, B powers.
    ///
    /// Port of `CDLTransformImpl::getPower` (CDLTransform.cpp:244-255 @ v2.5.2).
    #[doc(alias = "getPower")]
    pub fn power(&self) -> [f64; 3] {
        self.data.get_power_params().get_rgb()
    }

    /// Sets the slopes, offsets and powers: R, G, B each.
    ///
    /// Port of `CDLTransformImpl::setSOP` (CDLTransform.cpp:257-267 @ v2.5.2).
    #[doc(alias = "setSOP")]
    pub fn set_sop(&mut self, vec9: &[f64; 9]) {
        self.data
            .set_slope_params(ChannelParams::new(vec9[0], vec9[1], vec9[2]));
        self.data
            .set_offset_params(ChannelParams::new(vec9[3], vec9[4], vec9[5]));
        self.data
            .set_power_params(ChannelParams::new(vec9[6], vec9[7], vec9[8]));
    }

    /// The slopes, offsets and powers: R, G, B each.
    ///
    /// Port of `CDLTransformImpl::getSOP` (CDLTransform.cpp:269-290 @ v2.5.2).
    #[doc(alias = "getSOP")]
    pub fn sop(&self) -> [f64; 9] {
        let [s0, s1, s2] = self.slope();
        let [o0, o1, o2] = self.offset();
        let [p0, p1, p2] = self.power();
        [s0, s1, s2, o0, o1, o2, p0, p1, p2]
    }

    /// Port of `CDLTransformImpl::setSat` (CDLTransform.cpp:292-295 @ v2.5.2).
    #[doc(alias = "setSat")]
    pub fn set_sat(&mut self, sat: f64) {
        self.data.set_saturation(sat);
    }

    /// Port of `CDLTransformImpl::getSat` (CDLTransform.cpp:297-300 @ v2.5.2).
    #[doc(alias = "getSat")]
    pub fn sat(&self) -> f64 {
        self.data.get_saturation()
    }

    /// The luma coefficients of the saturation: Rec. 709's.
    ///
    /// Port of `CDLTransformImpl::getSatLumaCoefs` (CDLTransform.cpp:302-312 @ v2.5.2).
    #[doc(alias = "getSatLumaCoefs")]
    pub fn sat_luma_coefs(&self) -> [f64; 3] {
        [0.2126, 0.7152, 0.0722]
    }

    /// The ID: the metadata's `id` attribute.
    ///
    /// Port of `CDLTransformImpl::getID` (CDLTransform.cpp:314-317 @ v2.5.2).
    #[doc(alias = "getID")]
    pub fn id(&self) -> &[u8] {
        self.data.get_id()
    }

    /// Sets the ID, up to its first NUL (a C string).
    ///
    /// Port of `CDLTransformImpl::setID` (CDLTransform.cpp:319-322 @ v2.5.2).
    #[doc(alias = "setID")]
    pub fn set_id(&mut self, id: &[u8]) {
        self.data.set_id(id);
    }

    /// The value of the metadata's first `SOPDescription` child, or empty.
    ///
    /// Port of `CDLTransformImpl::getFirstSOPDescription` (CDLTransform.cpp:331-343 @ v2.5.2).
    #[doc(alias = "getFirstSOPDescription")]
    pub fn first_sop_description(&self) -> &[u8] {
        let info = self.data.get_format_metadata();
        match info.get_first_child_index(METADATA_SOP_DESCRIPTION) {
            None => b"",
            Some(desc_index) => info.get_children_elements()[desc_index].get_element_value(),
        }
    }

    /// Sets the value of the metadata's first `SOPDescription` child (up to the description's
    /// first NUL, a C string): it adds the child if there is none, and an empty description
    /// removes it.
    ///
    /// Port of `CDLTransformImpl::setFirstSOPDescription` (CDLTransform.cpp:345-367 @ v2.5.2).
    #[doc(alias = "setFirstSOPDescription")]
    pub fn set_first_sop_description(&mut self, description: &[u8]) {
        let description = c_str(description);
        let info = self.data.get_format_metadata_mut();
        match info.get_first_child_index(METADATA_SOP_DESCRIPTION) {
            None => {
                if !description.is_empty() {
                    let child = FormatMetadataImpl::new(METADATA_SOP_DESCRIPTION, description)
                        .expect("SOPDescription is a valid element name");
                    info.get_children_elements_mut().push(child);
                }
            }
            Some(desc_index) => {
                if !description.is_empty() {
                    info.get_children_elements_mut()[desc_index]
                        .set_element_value(Some(description))
                        .expect("a child element can have a value");
                } else {
                    info.get_children_elements_mut().remove(desc_index);
                }
            }
        }
    }

    /// Writes the transform's text to `os`, the numbers with the stream's precision: 6 on a new
    /// stream, 16 after a MatrixTransform in the same group (docs/improvements.md, I-73).
    ///
    /// Port of `operator<<(std::ostream &, const CDLTransform &)` (CDLTransform.cpp:369-398 @
    /// v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        let sop = self.sop();

        os.put_str("<CDLTransform");
        os.put_str(" direction=");
        os.put_str(transform_direction_to_string(self.direction()));
        let put_three = |os: &mut OStringStream, values: &[f64]| {
            for (i, value) in values.iter().enumerate() {
                if i != 0 {
                    os.put_str(", ");
                }
                os.put_f64(*value);
            }
        };
        os.put_str(", slope=[");
        put_three(os, &sop[0..3]);
        os.put_str("], offset=[");
        put_three(os, &sop[3..6]);
        os.put_str("], power=[");
        put_three(os, &sop[6..9]);
        os.put_str("], sat=");
        os.put_f64(self.sat());
        os.put_str(", style=");
        os.put_str(cdl_style_to_string(self.style()));
        os.put_str(">");
    }
}

impl PartialEq for CdlTransform {
    /// [`CdlTransform::equals`].
    fn eq(&self, other: &CdlTransform) -> bool {
        self.equals(other)
    }
}

impl fmt::Display for CdlTransform {
    /// `<CDLTransform direction=<dir>, slope=[<3>], offset=[<3>], power=[<3>], sat=<sat>,
    /// style=<style>>`.
    ///
    /// Port of `operator<<(std::ostream &, const CDLTransform &)` (CDLTransform.cpp:369-398 @
    /// v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(os.str())
    }
}

/// Appends to `group` the transform of the CDL op `op`, holding a copy of its data.
///
/// Port of `CreateCDLTransform` (src/OpenColorIO/ops/cdl/CDLOp.cpp:185-198 @ v2.5.2).
pub(crate) fn create_cdl_transform(group: &mut GroupTransform, op: &Op) -> Result<()> {
    let OpData::Cdl(cdl_data) = &**op.data() else {
        return Err(Exception::new("CreateCDLTransform: op has to be a CDLOp"));
    };
    let cdl_transform = CdlTransform {
        data: cdl_data.clone(),
    };
    group.append_transform(cdl_transform.into());
    Ok(())
}

/// Appends the ops of `cdl_transform` in the direction `dir`. In a version 1 config, in the
/// direction combined with the transform's: forward, a scale and offset, an exponent (the
/// powers) and a saturation, Matrix and Exponent ops that ignore the style; inverse, the
/// inverses in the reverse order. Otherwise, after validating the data, a CDL op of a copy of
/// it in the direction `dir` combined with the data's.
///
/// Port of `BuildCDLOp` (src/OpenColorIO/ops/cdl/CDLOp.cpp:200-265 @ v2.5.2).
pub(crate) fn build_cdl_op(
    ops: &mut OpVec,
    config: &Config,
    cdl_transform: &CdlTransform,
    dir: TransformDirection,
) -> Result<()> {
    if config.major_version() == 1 {
        let combined_dir = combine_transform_directions(dir, cdl_transform.direction());

        let mut slope4 = [1.0, 1.0, 1.0, 1.0];
        slope4[..3].copy_from_slice(&cdl_transform.slope());

        let mut offset4 = [0.0, 0.0, 0.0, 0.0];
        offset4[..3].copy_from_slice(&cdl_transform.offset());

        let mut power4 = [1.0, 1.0, 1.0, 1.0];
        power4[..3].copy_from_slice(&cdl_transform.power());

        let luma_coef3 = cdl_transform.sat_luma_coefs();

        let sat = cdl_transform.sat();

        match combined_dir {
            TransformDirection::Forward => {
                // 1) Scale + Offset
                create_scale_offset_op(ops, &slope4, &offset4, TransformDirection::Forward);

                // 2) Power + Clamp at 0 (NB: This is not in accord with the ASC v1.2 spec
                //    since it also requires clamping at 1.)
                create_exponent_op_from_values(ops, &power4, TransformDirection::Forward)?;

                // 3) Saturation (NB: Does not clamp at 0 and 1 as per ASC v1.2 spec)
                create_saturation_op(ops, sat, &luma_coef3, TransformDirection::Forward);
            }
            TransformDirection::Inverse => {
                // 3) Saturation (NB: Does not clamp at 0 and 1 as per ASC v1.2 spec)
                create_saturation_op(ops, sat, &luma_coef3, TransformDirection::Inverse);

                // 2) Power + Clamp at 0 (NB: This is not in accord with the ASC v1.2 spec
                //    since it also requires clamping at 1.)
                create_exponent_op_from_values(ops, &power4, TransformDirection::Inverse)?;

                // 1) Scale + Offset
                create_scale_offset_op(ops, &slope4, &offset4, TransformDirection::Inverse);
            }
        }
        Ok(())
    } else {
        // Starting with the version 2, OCIO is now using a CDL Op complying with the Common LUT
        // Format (i.e. CLF) specification.
        let data = cdl_transform.data();
        data.validate()?;

        let cdl = data.clone();
        create_cdl_op(ops, cdl, dir);
        Ok(())
    }
}

#[cfg(test)]
#[path = "cdl_transform_tests.rs"]
mod tests;
