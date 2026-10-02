// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `Lut1DOpData` against the wheel, through the `Lut1DTransform` that holds one
//! (src/OpenColorIO/transforms/Lut1DTransform.cpp @ v2.5.2), built empty (`m_data{ 2 }`,
//! Lut1DTransform.h:64) and set through its setters:
//! - `setLength` replaces the array with an identity of that length,
//!   `Lut3by1DArray(halfFlags, 3, length, false)` (Lut1DTransform.cpp:101-106), which raises
//!   for lengths under 2 and over 1024x1024;
//! - `setHueAdjust` raises for `HUE_WYPN` (`Lut1DOpData::setHueAdjust`);
//! - `validate` runs the data's and prefixes its message with `Lut1DTransform validation
//!   failed: ` (Lut1DTransform.cpp:59-73);
//! - `equals` compares the data with it (Lut1DTransform.cpp:95-99), through `transform_text`
//!   pairs.
//!
//! The data's cache ID is checked with the Lut1D op's, through a CPU processor
//! (`lut1d_op_oracle.rs`).

use ocio_ops::exception::Exception;
use ocio_ops::open_color_types::{Lut1DHueAdjust, TransformDirection};
use ocio_ops::ops::lut1d::Lut1DOpData;
use ocio_ops::ops::lut1d::lut1d_op_data::Lut3by1DArray;
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_testkit::transform_text::{Built, TransformTextRequest};
use serde_json::{Value, json};

/// A `Lut1DTransform`'s settings, applied in this order.
#[derive(Debug, Clone)]
struct Case {
    half_domain: bool,
    raw_halfs: bool,
    length: core::ffi::c_ulong,
    /// Entries to set: index and RGB.
    values: Vec<(u64, [f32; 3])>,
    interpolation: Interpolation,
    hue: Lut1DHueAdjust,
    dir: TransformDirection,
}

impl Default for Case {
    fn default() -> Self {
        Case {
            half_domain: false,
            raw_halfs: false,
            length: 17,
            values: Vec::new(),
            interpolation: Interpolation::Default,
            hue: Lut1DHueAdjust::None,
            dir: TransformDirection::Forward,
        }
    }
}

fn interp_enum(i: Interpolation) -> &'static str {
    match i {
        Interpolation::Unknown => "INTERP_UNKNOWN",
        Interpolation::Nearest => "INTERP_NEAREST",
        Interpolation::Linear => "INTERP_LINEAR",
        Interpolation::Tetrahedral => "INTERP_TETRAHEDRAL",
        Interpolation::Cubic => "INTERP_CUBIC",
        Interpolation::Default => "INTERP_DEFAULT",
        Interpolation::Best => "INTERP_BEST",
    }
}

fn hue_enum(h: Lut1DHueAdjust) -> &'static str {
    match h {
        Lut1DHueAdjust::None => "HUE_NONE",
        Lut1DHueAdjust::Dw3 => "HUE_DW3",
        Lut1DHueAdjust::Wypn => "HUE_WYPN",
    }
}

impl Case {
    /// The transform's spec: the half domain first, so that `setLength` fills the domain it
    /// sets.
    fn spec(&self) -> Value {
        let dir = match self.dir {
            TransformDirection::Forward => "TRANSFORM_DIR_FORWARD",
            TransformDirection::Inverse => "TRANSFORM_DIR_INVERSE",
        };
        let mut calls = vec![
            json!(["setInputHalfDomain", self.half_domain]),
            json!(["setOutputRawHalfs", self.raw_halfs]),
            json!(["setLength", self.length]),
        ];
        for (index, rgb) in &self.values {
            calls.push(json!(["setValue", index, rgb[0], rgb[1], rgb[2]]));
        }
        calls.push(json!(["setInterpolation", {"enum": interp_enum(self.interpolation)}]));
        calls.push(json!(["setHueAdjust", {"enum": hue_enum(self.hue)}]));
        calls.push(json!(["setDirection", {"enum": dir}]));
        json!({"class": "Lut1DTransform", "args": {}, "calls": calls})
    }

    /// The port's data, as the transform's setters make it; the error of a setter that
    /// raises.
    fn port(&self) -> Result<Lut1DOpData, Exception> {
        let mut data = Lut1DOpData::new(2)?;
        data.set_input_half_domain(self.half_domain);
        data.set_output_raw_halfs(self.raw_halfs);
        // `Lut1DTransformImpl::setLength`.
        *data.get_array_mut() = Lut3by1DArray::new(data.get_half_flags(), 3, self.length, false)?;
        for (index, rgb) in &self.values {
            let i = 3 * *index as usize;
            data.get_array_mut()[i] = rgb[0];
            data.get_array_mut()[i + 1] = rgb[1];
            data.get_array_mut()[i + 2] = rgb[2];
        }
        data.set_interpolation(self.interpolation);
        data.set_hue_adjust(self.hue)?;
        data.set_direction(self.dir);
        Ok(data)
    }
}

const PREFIX: &str = "Lut1DTransform validation failed: ";

fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    for length in [
        0,
        1,
        2,
        3,
        17,
        1024,
        65535,
        65536,
        65537,
        1024 * 1024,
        1024 * 1024 + 1,
    ] {
        for half_domain in [false, true] {
            out.push(Case {
                length,
                half_domain,
                ..Case::default()
            });
        }
    }
    for interpolation in [
        Interpolation::Unknown,
        Interpolation::Nearest,
        Interpolation::Linear,
        Interpolation::Tetrahedral,
        Interpolation::Cubic,
        Interpolation::Default,
        Interpolation::Best,
    ] {
        out.push(Case {
            interpolation,
            ..Case::default()
        });
    }
    for hue in [
        Lut1DHueAdjust::None,
        Lut1DHueAdjust::Dw3,
        Lut1DHueAdjust::Wypn,
    ] {
        for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
            out.push(Case {
                hue,
                dir,
                raw_halfs: true,
                values: vec![(3, [0.5, -1.0, 2.0])],
                ..Case::default()
            });
        }
    }
    out
}

#[test]
fn validation_matches_the_wheel() {
    let cases = cases();
    let reply = TransformTextRequest {
        transforms: cases.iter().map(Case::spec).collect(),
        pairs: Vec::new(),
    }
    .run();

    let mut failures = Vec::new();
    let mut refused = 0;
    for (case, built) in cases.iter().zip(&reply.transforms) {
        let wheel = match built {
            Built::Raised(e) => Err(e.message.clone()),
            Built::Text(text) => match &text.validate {
                Some(e) => Err(e.message.clone()),
                None => Ok(()),
            },
        };
        refused += usize::from(wheel.is_err());
        // A setter's error comes as it is; validation's with the transform's prefix.
        let port = match case.port() {
            Err(e) => Err(e.message().to_string()),
            Ok(data) => data
                .validate()
                .map_err(|e| format!("{PREFIX}{}", e.message())),
        };
        if port != wheel {
            failures.push(format!("{case:?}\n  wheel {wheel:?}\n  port  {port:?}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
    assert!(refused > 0 && refused < cases.len(), "{refused}");
}

#[test]
fn equality_matches_the_wheel() {
    let base = Case::default();
    let with = |f: &dyn Fn(&mut Case)| {
        let mut c = base.clone();
        f(&mut c);
        c
    };
    let cases = vec![
        base.clone(),
        with(&|c| c.interpolation = Interpolation::Nearest),
        with(&|c| c.interpolation = Interpolation::Cubic),
        with(&|c| c.dir = TransformDirection::Inverse),
        with(&|c| c.hue = Lut1DHueAdjust::Dw3),
        with(&|c| c.raw_halfs = true),
        with(&|c| c.length = 18),
        with(&|c| c.values = vec![(5, [0.25, 0.3125, 0.375])]),
        with(&|c| c.values = vec![(5, [0.25, 0.3125, 0.4])]),
        // A value equal to the identity's, and zeros of both signs.
        with(&|c| c.values = vec![(0, [0.0, 0.0, 0.0])]),
        with(&|c| c.values = vec![(0, [-0.0, -0.0, -0.0])]),
        // Two half domains: their NaN codes make them unequal, except to themselves.
        with(&|c| {
            c.half_domain = true;
            c.length = 65536;
        }),
        with(&|c| {
            c.half_domain = true;
            c.length = 65536;
        }),
        with(&|c| {
            c.half_domain = true;
            c.length = 65536;
            c.values = vec![(0x3c00, [1.0, 1.0, 1.0f32.next_up()])];
        }),
    ];
    let mut pairs = Vec::new();
    for i in 0..cases.len() {
        for j in 0..cases.len() {
            pairs.push((i, j));
        }
    }
    let reply = TransformTextRequest {
        transforms: cases.iter().map(Case::spec).collect(),
        pairs: pairs.clone(),
    }
    .run();

    let ports: Vec<Lut1DOpData> = cases.iter().map(|c| c.port().unwrap()).collect();
    let mut failures = Vec::new();
    let mut equal = 0;
    for ((i, j), wheel) in pairs.iter().zip(&reply.pairs) {
        let wheel = wheel.unwrap_or_else(|| panic!("no equals for {i} {j}"));
        equal += usize::from(wheel && i != j);
        // The same transform for i == j: its data, the same object.
        let port = ports[*i].equals(&ports[*j]);
        if port != wheel {
            failures.push(format!("{:?} == {:?}: wheel {wheel}", cases[*i], cases[*j]));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} pairs differ:\n{}",
        failures.len(),
        pairs.len(),
        failures.join("\n")
    );
    assert!(equal > 0);
}
