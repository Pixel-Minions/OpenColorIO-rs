// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The `CDLTransform`, which holds a `CDLOpData`, as the wheel's binding builds it and as the
//! port builds its op data, until the transform is ported (WP 1.8). In a version 2 config, it
//! builds a CDL op with a copy of the data (`BuildCDLOp`,
//! src/OpenColorIO/ops/cdl/CDLOp.cpp:200-265 @ v2.5.2).

use ocio_ops::open_color_types::{CdlStyle, TransformDirection};
use ocio_ops::ops::cdl::{CdlOpData, ChannelParams};
use serde_json::{Value, json};

/// A `CDLTransform`'s parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Cdl {
    pub(crate) slope: [f64; 3],
    pub(crate) offset: [f64; 3],
    pub(crate) power: [f64; 3],
    pub(crate) sat: f64,
    pub(crate) style: CdlStyle,
}

/// The style's enum in an oracle transform spec.
pub(crate) fn style_enum(style: CdlStyle) -> Value {
    match style {
        CdlStyle::Asc => json!({"enum": "CDL_ASC"}),
        CdlStyle::NoClamp => json!({"enum": "CDL_NO_CLAMP"}),
    }
}

/// The style in the config's YAML syntax (`CDLStyleFromString`,
/// src/OpenColorIO/ParseUtils.cpp @ v2.5.2).
pub(crate) fn yaml_style(style: CdlStyle) -> &'static str {
    match style {
        CdlStyle::Asc => "asc",
        CdlStyle::NoClamp => "noclamp",
    }
}

impl Cdl {
    /// The JSON spec of `CDLTransform(slope, offset, power, sat, direction=dir)`, then
    /// `setStyle(style)`; finite values only.
    pub(crate) fn spec(&self, dir: TransformDirection) -> Value {
        let dir = match dir {
            TransformDirection::Forward => json!({"enum": "TRANSFORM_DIR_FORWARD"}),
            TransformDirection::Inverse => json!({"enum": "TRANSFORM_DIR_INVERSE"}),
        };
        json!({
            "class": "CDLTransform",
            "args": {
                "slope": self.slope, "offset": self.offset, "power": self.power,
                "sat": self.sat, "direction": dir,
            },
            "calls": [["setStyle", style_enum(self.style)]],
        })
    }

    /// The op data of the transform, not validated.
    ///
    /// `CDLTransformImpl` holds a default `CDLOpData` (`CDL_NO_CLAMP_FWD`, an identity;
    /// src/OpenColorIO/ops/cdl/CDLOpData.cpp:127-135 @ v2.5.2). The Python constructor calls
    /// `setSlope`, `setOffset`, `setPower`, `setSat` and `setDirection` (which inverts the style
    /// if needed), then `validate` (src/bindings/python/transforms/PyCDLTransform.cpp:37-62);
    /// `setStyle` then converts the style for the current direction
    /// (src/OpenColorIO/transforms/CDLTransform.cpp:137-140, 182-295). A config's YAML calls the
    /// same setters in the order of its keys, without validating (src/OpenColorIO/OCIOYaml.cpp:
    /// 646-726); the specs here give the style, then the direction.
    pub(crate) fn op_data(&self, dir: TransformDirection) -> CdlOpData {
        let mut data = CdlOpData::default();
        data.set_slope_params(ChannelParams::new(
            self.slope[0],
            self.slope[1],
            self.slope[2],
        ));
        data.set_offset_params(ChannelParams::new(
            self.offset[0],
            self.offset[1],
            self.offset[2],
        ));
        data.set_power_params(ChannelParams::new(
            self.power[0],
            self.power[1],
            self.power[2],
        ));
        data.set_saturation(self.sat);
        data.set_direction(dir);
        let cur_dir = data.get_direction();
        data.set_style(CdlOpData::convert_style(self.style, cur_dir));
        data
    }
}
