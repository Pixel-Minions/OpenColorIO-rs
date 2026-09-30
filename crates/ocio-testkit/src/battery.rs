// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The standard oracle test battery (card T3): an op family's CPU renderers against the wheel,
//! bit for bit, over the same parameter cases, probe sets, directions and fast-math settings
//! for every family.
//!
//! - [`params`]: parameter cases, their generators (extreme finite, NaN and ±Inf values), and
//!   the comparison that applies waiver W0002 to the channels of NaN and infinite parameters
//!   and nothing else.
//! - The vocabulary of the battery's dimensions: [`Direction`], and [`Format`] ([`BitDepth`],
//!   [`Layout`]) with [`Combo`] tying them together.

pub mod params;

use std::fmt;

use serde_json::{Value, json};

/// A transform direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// `TRANSFORM_DIR_FORWARD`.
    Forward,
    /// `TRANSFORM_DIR_INVERSE`.
    Inverse,
}

impl Direction {
    /// Both directions, forward first.
    pub const BOTH: [Direction; 2] = [Direction::Forward, Direction::Inverse];

    /// The direction in an oracle transform spec: `{"enum": "TRANSFORM_DIR_FORWARD"}`.
    pub fn oracle_enum(self) -> Value {
        match self {
            Direction::Forward => json!({"enum": "TRANSFORM_DIR_FORWARD"}),
            Direction::Inverse => json!({"enum": "TRANSFORM_DIR_INVERSE"}),
        }
    }

    /// The direction in a config's YAML: `forward` or `inverse`.
    pub fn yaml(self) -> &'static str {
        match self {
            Direction::Forward => "forward",
            Direction::Inverse => "inverse",
        }
    }
}

/// A pixel bit depth: OCIO's `BitDepth`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BitDepth {
    /// `BIT_DEPTH_UINT8`.
    Uint8,
    /// `BIT_DEPTH_UINT10`.
    Uint10,
    /// `BIT_DEPTH_UINT12`.
    Uint12,
    /// `BIT_DEPTH_UINT16`.
    Uint16,
    /// `BIT_DEPTH_F16`.
    F16,
    /// `BIT_DEPTH_F32`.
    F32,
}

impl BitDepth {
    /// The name the oracle takes (`cpu_apply`'s `in_bitdepth` and `out_bitdepth`).
    pub fn oracle_name(self) -> &'static str {
        match self {
            BitDepth::Uint8 => "BIT_DEPTH_UINT8",
            BitDepth::Uint10 => "BIT_DEPTH_UINT10",
            BitDepth::Uint12 => "BIT_DEPTH_UINT12",
            BitDepth::Uint16 => "BIT_DEPTH_UINT16",
            BitDepth::F16 => "BIT_DEPTH_F16",
            BitDepth::F32 => "BIT_DEPTH_F32",
        }
    }
}

/// How the pixels of a buffer are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layout {
    /// Packed RGBA: `PackedImageDesc` with 4 channels.
    PackedRgba,
    /// Packed RGB: `PackedImageDesc` with 3 channels.
    PackedRgb,
    /// Planar RGBA: `PlanarImageDesc` with an alpha plane (oracle chunk O1.2).
    PlanarRgba,
    /// Planar RGB: `PlanarImageDesc` without alpha (oracle chunk O1.2).
    PlanarRgb,
}

/// The pixel format of a combination: the processor's input and output bit depths, and the
/// layout of both buffers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Format {
    /// The input bit depth.
    pub input: BitDepth,
    /// The output bit depth.
    pub output: BitDepth,
    /// The layout.
    pub layout: Layout,
}

impl Format {
    /// F32 in, F32 out, packed RGBA: the format the op renderers work in.
    pub const F32_RGBA: Format = Format {
        input: BitDepth::F32,
        output: BitDepth::F32,
        layout: Layout::PackedRgba,
    };
}

/// One combination of the battery's dimensions besides the parameter case and the probe:
/// direction, fast math, and pixel format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Combo {
    /// The transform direction.
    pub direction: Direction,
    /// Whether OCIO's fast-math approximations are on (`OPTIMIZATION_FAST_LOG_EXP_POW`, part of
    /// the default optimization).
    pub fast_math: bool,
    /// The pixel format.
    pub format: Format,
}

impl fmt::Display for Combo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}, fast math {}",
            self.direction.yaml(),
            if self.fast_math { "on" } else { "off" }
        )?;
        if self.format != Format::F32_RGBA {
            write!(
                f,
                ", {} in, {} out, {:?}",
                self.format.input.oracle_name(),
                self.format.output.oracle_name(),
                self.format.layout
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combos_describe_themselves() {
        let combo = Combo {
            direction: Direction::Inverse,
            fast_math: false,
            format: Format::F32_RGBA,
        };
        assert_eq!(combo.to_string(), "inverse, fast math off");
        let combo = Combo {
            format: Format {
                input: BitDepth::Uint8,
                output: BitDepth::F16,
                layout: Layout::PackedRgb,
            },
            ..combo
        };
        assert_eq!(
            combo.to_string(),
            "inverse, fast math off, BIT_DEPTH_UINT8 in, BIT_DEPTH_F16 out, PackedRgb"
        );
        assert_eq!(
            Direction::Forward.oracle_enum(),
            json!({"enum": "TRANSFORM_DIR_FORWARD"})
        );
    }
}
