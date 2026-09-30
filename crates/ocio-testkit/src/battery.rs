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
//! - [`Family`]: what an op family supplies (cases, the oracle's [`Spec`], the port's
//!   [`Port`]); [`run`] and [`run_with`] run every combination against the wheel, batching
//!   the oracle calls, and report failures per case ([`Summary`]).

mod engine;
pub mod params;

use std::fmt;
use std::time::Duration;

use serde_json::{Value, json};

use crate::probe::{ProbeSet, RandomRange};
use params::{Case, Channels, Params};

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

/// `OPTIMIZATION_DEFAULT` is `OPTIMIZATION_VERY_GOOD`: `OPTIMIZATION_LOSSLESS |
/// OPTIMIZATION_COMP_LUT1D | OPTIMIZATION_LUT_INV_FAST | OPTIMIZATION_FAST_LOG_EXP_POW |
/// OPTIMIZATION_COMP_SEPARABLE_PREFIX` (include/OpenColorIO/OpenColorTypes.h:711-722 @ v2.5.2).
/// These are the same flags without `OPTIMIZATION_FAST_LOG_EXP_POW`; the oracle ORs them.
const DEFAULT_WITHOUT_FAST_MATH: [&str; 4] = [
    "OPTIMIZATION_LOSSLESS",
    "OPTIMIZATION_COMP_LUT1D",
    "OPTIMIZATION_LUT_INV_FAST",
    "OPTIMIZATION_COMP_SEPARABLE_PREFIX",
];

/// A version 2.1 config with one colour space, `raw`, the processor's source; a
/// [`Spec::Yaml`] adds the destination colour space `cs`.
const RAW_CONFIG_HEAD: &str = "ocio_profile_version: 2.1
roles:
  default: raw
file_rules:
  - !<Rule> {name: Default, colorspace: raw}
displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
    name: raw
";

/// The oracle's side of a case: the processor the wheel builds.
#[derive(Debug, Clone, PartialEq)]
pub enum Spec {
    /// A transform spec (`oracle/ocio_oracle/spec.py`), applied with a raw config's
    /// `getProcessor(transform)`. JSON can't hold NaN or ±Inf (serde_json writes them as
    /// `null`, which the battery refuses): use [`Spec::Yaml`] for those.
    Transform(Value),
    /// One transform in the config's YAML syntax, e.g. `!<LogTransform> {base: .nan}`, as the
    /// `from_scene_reference` of a colour space `cs` in a raw version 2.1 config: the
    /// processor from `raw` to `cs`. YAML can hold NaN and infinite parameters (`.nan`,
    /// `.inf`, `-.inf`; see [`yaml_number`]).
    Yaml(String),
}

impl Spec {
    /// The oracle's `cpu_apply` arguments for this processor in `combo`: the default CPU
    /// processor with fast math on, the default flags without `OPTIMIZATION_FAST_LOG_EXP_POW`
    /// with fast math off, and the bit depths and channel count of other formats.
    pub fn cpu_apply_args(&self, combo: &Combo) -> Value {
        let mut args = match self {
            Spec::Transform(transform) => {
                if let Some(path) = null_path(transform, "") {
                    panic!(
                        "transform spec {transform} has null at {path}: JSON can't hold NaN or \
                         ±Inf parameters (serde_json writes them as null); use Spec::Yaml"
                    );
                }
                json!({ "transform": transform })
            }
            Spec::Yaml(transform) => json!({
                "config": {"yaml": format!(
                    "{RAW_CONFIG_HEAD}  - !<ColorSpace>\n    name: cs\n    from_scene_reference: {transform}\n"
                )},
                "src": "raw",
                "dst": "cs",
            }),
        };
        if !combo.fast_math {
            args["optimization"] = json!(DEFAULT_WITHOUT_FAST_MATH);
        }
        if combo.format != Format::F32_RGBA {
            args["in_bitdepth"] = json!(combo.format.input.oracle_name());
            args["out_bitdepth"] = json!(combo.format.output.oracle_name());
            let channels = match combo.format.layout {
                Layout::PackedRgba | Layout::PlanarRgba => 4,
                Layout::PackedRgb | Layout::PlanarRgb => 3,
            };
            args["channels"] = json!(channels);
        }
        args
    }
}

/// Where a JSON value holds `null`, if anywhere.
fn null_path(value: &Value, at: &str) -> Option<String> {
    match value {
        Value::Null => Some(if at.is_empty() { "/".into() } else { at.into() }),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .find_map(|(i, v)| null_path(v, &format!("{at}/{i}"))),
        Value::Object(fields) => fields
            .iter()
            .find_map(|(k, v)| null_path(v, &format!("{at}/{k}"))),
        _ => None,
    }
}

/// `value` as a YAML number that OCIO's config reader (yaml-cpp) reads back as the same
/// `double` on both reference platforms: `.nan`, `.inf`, `-.inf`, or Rust's shortest
/// round-trip form, in exponent notation for large and tiny magnitudes (`2.2`, `-0`, `1e39`,
/// `5e-324`). The battery's tests check that a YAML spec gives the same pixels as a JSON one.
pub fn yaml_number(value: f64) -> String {
    if value.is_nan() {
        ".nan".to_string()
    } else if value == f64::INFINITY {
        ".inf".to_string()
    } else if value == f64::NEG_INFINITY {
        "-.inf".to_string()
    } else if value != 0.0 && (value.abs() >= 1e16 || value.abs() < 1e-5) {
        format!("{value:e}")
    } else {
        value.to_string()
    }
}

/// `values` as a YAML flow sequence of [`yaml_number`]s: `[0.18, .nan, 1]`.
pub fn yaml_list(values: &[f64]) -> String {
    let items: Vec<String> = values.iter().map(|&v| yaml_number(v)).collect();
    format!("[{}]", items.join(", "))
}

/// A renderer that works in place on packed F32 RGBA pixels, like
/// `ocio_ops::op::CpuOp::apply`.
pub type InPlaceRenderer = Box<dyn Fn(&mut [f32]) + Send + Sync>;

/// The port's side of a combination: a renderer to compare with the wheel.
///
/// Extension point: when the port's CPU engine can apply other bit depths and layouts
/// (WP 1.1, 1.2d), a variant taking typed input and output buffers joins this one, and
/// [`Family::formats`] can list more than [`Format::F32_RGBA`].
pub enum Port {
    /// A renderer that works in place on packed F32 RGBA pixels.
    InPlaceRgbaF32(InPlaceRenderer),
}

impl Port {
    /// A port from an in-place F32 RGBA renderer, typically
    /// `Port::in_place(move |px| renderer.apply(px))`.
    pub fn in_place(render: impl Fn(&mut [f32]) + Send + Sync + 'static) -> Port {
        Port::InPlaceRgbaF32(Box::new(render))
    }

    /// The port's output for `input` pixels.
    fn apply(&self, input: &[f32]) -> Vec<f32> {
        match self {
            Port::InPlaceRgbaF32(render) => {
                let mut pixels = input.to_vec();
                render(&mut pixels);
                pixels
            }
        }
    }
}

impl fmt::Debug for Port {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Port::InPlaceRgbaF32(_) => f.write_str("Port::InPlaceRgbaF32"),
        }
    }
}

/// Whether the port refuses parameters the way the wheel does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Validation {
    /// [`Family::port`] returns the port's exception text for parameters it refuses; where the
    /// wheel refuses too, the two texts must be identical, byte for byte.
    Ported,
    /// The family's op data has no `validate` yet; `card` ports it. A generated case the
    /// wheel refuses is left out (the [`Summary`] lists it); an explicit case the wheel
    /// refuses fails.
    NotPorted {
        /// The card that ports the validation, for the summary.
        card: &'static str,
    },
}

/// What an op family supplies to the battery.
///
/// The battery runs every case ([`Family::cases`] and the generated mutations of
/// [`Family::mutation_bases`]) in every combination of [`Family::directions`], fast math on
/// and off, and [`Family::formats`], on every probe buffer of the [`Plan`] plus the
/// neighbourhoods of [`Family::breakpoints`].
pub trait Family {
    /// The family's parameters.
    type Params: Params;

    /// The family's name, for reports: `"LogCameraTransform"`.
    fn name(&self) -> String;

    /// The explicit cases: typical parameters, and hand-written extreme or non-finite ones.
    fn cases(&self) -> Vec<Case<Self::Params>>;

    /// The cases to generate extreme-finite, NaN and ±Inf mutations from
    /// ([`params::mutations`]); none by default.
    fn mutation_bases(&self) -> Vec<Case<Self::Params>> {
        Vec::new()
    }

    /// The directions to run; both by default.
    fn directions(&self) -> Vec<Direction> {
        Direction::BOTH.to_vec()
    }

    /// The pixel formats to run. Only [`Format::F32_RGBA`] until the port's CPU engine and
    /// packing exist (WP 1.1, 1.2d) and the oracle takes planar images (O1.2).
    fn formats(&self) -> Vec<Format> {
        vec![Format::F32_RGBA]
    }

    /// The oracle's side: the processor for `params` in `direction`.
    fn spec(&self, params: &Self::Params, direction: Direction) -> Spec;

    /// The port's side: the renderer that this machine's dispatch picks for `params` in
    /// `combo`, as the wheel's does; or, for parameters the port refuses, its exception text.
    fn port(&self, params: &Self::Params, combo: &Combo) -> Result<Port, String>;

    /// The renderers of the numeric profiles that this machine's dispatch doesn't pick (for
    /// example the SSE2, AVX and AVX2 Lut3D kernels on an AVX-512 machine), with their names.
    /// Their [`Family::pass_through`] channels are compared with the wheel's output bit for
    /// bit: those channels don't depend on the kernel, so the wheel's dispatched kernel is an
    /// oracle for them. Their other channels need the CPU emulation of card T5b. None by
    /// default.
    fn other_profiles(&self, params: &Self::Params, combo: &Combo) -> Vec<(String, Port)> {
        let _ = (params, combo);
        Vec::new()
    }

    /// The channels that the renderers pass through: they never write them. The battery
    /// checks that the wheel leaves them unchanged too, and compares them on every profile of
    /// [`Family::other_profiles`]. None by default.
    fn pass_through(&self, params: &Self::Params, combo: &Combo) -> Channels {
        let _ = (params, combo);
        [false; 4]
    }

    /// Points whose ±N ulp neighbourhoods to probe for `params` in `direction`, such as break
    /// points; none by default.
    fn breakpoints(&self, params: &Self::Params, direction: Direction) -> Vec<f32> {
        let _ = (params, direction);
        Vec::new()
    }

    /// Whether the port refuses parameters the way the wheel does.
    fn validation(&self) -> Validation {
        Validation::Ported
    }
}

/// Which generated cases a [`Plan`] runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mutations {
    /// None.
    None,
    /// [`params::sampled_mutations`]: two per slot.
    Sampled,
    /// [`params::mutations`]: every slot with every value.
    All,
}

/// What a battery run probes, and how it calls the oracle.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// The plan's name, for reports.
    pub name: String,
    /// The probe sets of the explicit cases.
    pub probes: Vec<ProbeSet>,
    /// The probe sets of the generated cases.
    pub generated_probes: Vec<ProbeSet>,
    /// The size of the neighbourhoods of [`Family::breakpoints`], in ulps on each side; 0 for
    /// none.
    pub breakpoint_ulps: u32,
    /// Which generated cases to run.
    pub mutations: Mutations,
    /// The most pixel bytes one oracle process returns; calls beyond it go to the next
    /// process.
    pub batch_bytes: usize,
    /// Whether to cache the oracle's responses (`<target>/oracle-cache`).
    pub cache: bool,
}

impl Plan {
    /// A small plan that runs in seconds: every 61st half value, the specials, 256 random
    /// values of every [`RandomRange`], ±3 ulp around the break points, NaN buffers of 1 to
    /// 24 pixels, and two generated cases per parameter slot.
    pub fn quick() -> Plan {
        let probes = vec![
            ProbeSet::Halves { stride: 61 },
            ProbeSet::Specials,
            ProbeSet::Random {
                name: "of every range",
                seed: 0x0b47_7e27_0001,
                ranges: RandomRange::ALL.iter().map(|&r| (r, 256)).collect(),
            },
            ProbeSet::NanBuffers { max_pixels: 24 },
        ];
        Plan {
            name: "quick".to_string(),
            generated_probes: probes.clone(),
            probes,
            breakpoint_ulps: 3,
            mutations: Mutations::Sampled,
            batch_bytes: 256 << 20,
            cache: true,
        }
    }
}

/// What a battery run compared, and the failures.
#[derive(Debug, Clone, Default)]
pub struct Summary {
    /// The family.
    pub family: String,
    /// The plan.
    pub plan: String,
    /// The explicit cases.
    pub explicit_cases: usize,
    /// The generated cases.
    pub generated_cases: usize,
    /// Case and combination pairs.
    pub groups: usize,
    /// Pixel buffers of the dispatched renderer compared with the wheel's.
    pub comparisons: usize,
    /// `f32` values in those buffers.
    pub values: usize,
    /// Buffers of other profiles compared on their pass-through channels.
    pub pass_through_checks: usize,
    /// Case and combination pairs that the wheel and the port both refuse, with identical
    /// exception texts.
    pub refusals_compared: usize,
    /// Generated cases the wheel refuses, left out while the family's validation isn't ported,
    /// with the wheel's exception text.
    pub left_out: Vec<(String, String)>,
    /// The card that ports the family's validation, when it isn't ported.
    pub validation_card: Option<&'static str>,
    /// Comparisons under waiver W0002.
    pub w0002_comparisons: usize,
    /// NaN values that matched only as NaN under W0002, per case and combination.
    pub w0002_waived: Vec<(String, usize)>,
    /// Oracle calls.
    pub oracle_calls: usize,
    /// Oracle processes (batches).
    pub oracle_batches: usize,
    /// Wall-clock time.
    pub elapsed: Duration,
    /// The failures, each with its report.
    pub failures: Vec<String>,
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let waived: usize = self.w0002_waived.iter().map(|(_, n)| n).sum();
        writeln!(f, "battery {} (plan {}):", self.family, self.plan)?;
        writeln!(
            f,
            "  cases: {} ({} explicit, {} generated); case/combination pairs: {}",
            self.explicit_cases + self.generated_cases,
            self.explicit_cases,
            self.generated_cases,
            self.groups
        )?;
        writeln!(
            f,
            "  comparisons: {} buffers, {} values; under W0002: {} buffers, {waived} NaN values \
             differing in sign or payload bits only",
            self.comparisons, self.values, self.w0002_comparisons
        )?;
        writeln!(
            f,
            "  pass-through checks of other profiles: {}; refusals compared: {}",
            self.pass_through_checks, self.refusals_compared
        )?;
        if !self.left_out.is_empty() {
            const LISTED: usize = 5;
            let labels: Vec<&str> = self
                .left_out
                .iter()
                .take(LISTED)
                .map(|(l, _)| l.as_str())
                .collect();
            writeln!(
                f,
                "  left out until {} ports the validation: {} generated cases the wheel refuses \
                 ({}{})",
                self.validation_card.unwrap_or("the validation's card"),
                self.left_out.len(),
                labels.join("; "),
                if self.left_out.len() > LISTED {
                    "; ..."
                } else {
                    ""
                }
            )?;
        }
        for (label, n) in &self.w0002_waived {
            writeln!(f, "  W0002: {label}: {n} NaN values")?;
        }
        write!(
            f,
            "  oracle: {} calls, {} processes; time: {:.1} s; failures: {}",
            self.oracle_calls,
            self.oracle_batches,
            self.elapsed.as_secs_f64(),
            self.failures.len()
        )
    }
}

/// Runs `family` with [`Plan::quick`]. See [`run_with`].
#[track_caller]
pub fn run<F: Family>(family: &F) -> Summary {
    run_with(family, &Plan::quick())
}

/// Runs every case of `family` in every combination on every probe buffer of `plan`, against
/// the wheel, and returns what it compared. Prints the summary; panics with a report per
/// failing case, combination and probe if anything differs.
#[track_caller]
pub fn run_with<F: Family>(family: &F, plan: &Plan) -> Summary {
    let summary = engine::run(family, plan);
    println!("{summary}");
    if !summary.failures.is_empty() {
        const LISTED: usize = 20;
        let listed: Vec<&str> = summary
            .failures
            .iter()
            .take(LISTED)
            .map(String::as_str)
            .collect();
        panic!(
            "{summary}\n\n{} failures{}:\n\n{}",
            summary.failures.len(),
            if summary.failures.len() > LISTED {
                format!(", the first {LISTED} listed")
            } else {
                String::new()
            },
            listed.join("\n\n")
        );
    }
    summary
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
