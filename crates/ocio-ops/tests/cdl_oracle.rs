// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CDL renderers against the wheel, bit for bit, through the oracle test battery
//! (`ocio_testkit::battery`): every case in both directions, with fast math on and off, on
//! the tier's probe sets (`OCIO_RS_TIER`).
//!
//! The oracle builds a `CDLTransform` in a raw config (a version 2 config, so it becomes a CDL
//! op; `BuildCDLOp`, src/OpenColorIO/ops/cdl/CDLOp.cpp:200-265 @ v2.5.2) and applies its CPU
//! processor to F32 RGBA pixels. The port builds the transform's op data (`common::cdl`) and
//! the renderer that `GetCDLCPURenderer` picks.
//!
//! The wheel refuses parameters out of `CDLOpData::validate`'s bounds, and the port gives the
//! same texts: a JSON spec's transform comes from the binding's constructor, which validates it
//! with the prefix `CDLTransform validation failed: ` (src/OpenColorIO/transforms/
//! CDLTransform.cpp:142-155); a YAML spec's transform is read without validating
//! (src/OpenColorIO/OCIOYaml.cpp:646-726), and `BuildCDLOp` validates the data without one.
//!
//! Every case has a power other than 1 in some channel: with a power of 1, the optimizer
//! replaces the CDL op with matrices and clamps (`CDLOpData::getSimplerReplacement`,
//! `OPTIMIZATION_SIMPLIFY_OPS`), which `tests/cdl_op_oracle.rs` checks. Otherwise the
//! processor's op list at F32 is that one CDL op: it is no identity, and the separable-prefix
//! bake only applies to integer input bit depths.
//!
//! The renderers never write alpha (upstream writes back the input's). They have no numeric
//! profiles: the fast-math kernels are SSE2 code, which every x86-64 CPU runs.

mod common;

use std::hint::black_box;

use common::cdl::{Cdl, yaml_style};
use ocio_ops::open_color_types::{CdlStyle, TransformDirection};
use ocio_ops::ops::cdl::cdl_op_cpu::get_cdl_cpu_renderer;
use ocio_testkit::battery::params::{A, Case, Channels, Params, Precision, RGB, Slot};
use ocio_testkit::battery::{
    self, Combo, Direction, Family, Port, Spec, Validation, yaml_list, yaml_number,
};

/// The port's direction for the battery's.
fn port_direction(direction: Direction) -> TransformDirection {
    match direction {
        Direction::Forward => TransformDirection::Forward,
        Direction::Inverse => TransformDirection::Inverse,
    }
}

impl Params for Cdl {
    fn slots(&self) -> Vec<Slot> {
        let mut s = Slot::rgb("slope", Precision::F64AsF32).to_vec();
        s.extend(Slot::rgb("offset", Precision::F64AsF32));
        s.extend(Slot::rgb("power", Precision::F64AsF32));
        s.push(Slot::new("sat", Precision::F64AsF32, RGB));
        s
    }
    fn get(&self, i: usize) -> f64 {
        match i {
            0..3 => self.slope[i],
            3..6 => self.offset[i - 3],
            6..9 => self.power[i - 6],
            _ => self.sat,
        }
    }
    fn set(&mut self, i: usize, v: f64) {
        match i {
            0..3 => self.slope[i] = v,
            3..6 => self.offset[i - 3] = v,
            6..9 => self.power[i - 6] = v,
            _ => self.sat = v,
        }
    }
}

/// Whether every parameter is finite: then a JSON transform spec can hold them.
fn all_finite(p: &Cdl) -> bool {
    (0..p.slots().len()).all(|i| p.get(i).is_finite())
}

/// CDLTransform.
struct CdlFamily {
    cases: Vec<Case<Cdl>>,
    bases: Vec<Case<Cdl>>,
}

impl Family for CdlFamily {
    type Params = Cdl;

    fn name(&self) -> String {
        "CDLTransform".to_string()
    }
    fn cases(&self) -> Vec<Case<Cdl>> {
        self.cases.clone()
    }
    fn mutation_bases(&self) -> Vec<Case<Cdl>> {
        self.bases.clone()
    }
    fn spec(&self, p: &Cdl, dir: Direction) -> Spec {
        if all_finite(p) {
            Spec::Transform(p.spec(port_direction(dir)))
        } else {
            Spec::Yaml(format!(
                "!<CDLTransform> {{slope: {}, offset: {}, power: {}, sat: {}, style: {}, \
                 direction: {}}}",
                yaml_list(&p.slope),
                yaml_list(&p.offset),
                yaml_list(&p.power),
                yaml_number(p.sat),
                yaml_style(p.style),
                dir.yaml()
            ))
        }
    }
    fn port(&self, p: &Cdl, combo: &Combo) -> Result<Port, String> {
        let data = p.op_data(port_direction(combo.direction));
        if all_finite(p) {
            // The binding's constructor validates the transform.
            data.validate()
                .map_err(|e| format!("CDLTransform validation failed: {}", e.message()))?;
        }
        // BuildCDLOp: `data.validate()`.
        data.validate().map_err(|e| e.message().to_string())?;
        let renderer = get_cdl_cpu_renderer(&black_box(data), combo.fast_math);
        Ok(Port::in_place(move |px| renderer.apply(px)))
    }
    fn pass_through(&self, _: &Cdl, _: &Combo) -> Channels {
        A
    }
    /// The clamps' bounds, zero, where the no-clamp styles pass negatives through the power,
    /// and the values the slope and offset map onto them.
    fn breakpoints(&self, p: &Cdl, _: Direction) -> Vec<f32> {
        let mut points = vec![0.0f32, 1.0];
        for c in 0..3 {
            for target in [0.0, 1.0] {
                let x = (target - p.offset[c]) / p.slope[c];
                if x.is_finite() {
                    points.push(x as f32);
                }
            }
        }
        points
    }
    fn validation(&self) -> Validation {
        Validation::Ported
    }
}

/// A CDL.
fn cdl(slope: [f64; 3], offset: [f64; 3], power: [f64; 3], sat: f64, style: CdlStyle) -> Cdl {
    Cdl {
        slope,
        offset,
        power,
        sat,
        style,
    }
}

#[test]
fn cdl_transform_matches_the_wheel() {
    let mut cases = Vec::new();
    let mut bases = Vec::new();
    for style in [CdlStyle::Asc, CdlStyle::NoClamp] {
        let list = [
            // tests/cpu/ops/cdl/CDLOp_tests.cpp:60-66, 428-434, 469-475 @ v2.5.2.
            cdl(
                [1.35, 1.1, 0.071],
                [0.05, -0.23, 0.11],
                [0.93, 0.81, 1.27],
                1.23,
                style,
            ),
            cdl(
                [1.15, 1.10, 0.9],
                [0.05, 0.02, 0.07],
                [1.2, 0.95, 1.13],
                0.87,
                style,
            ),
            cdl(
                [3.405, 1.0, 1.0],
                [-0.178, -0.178, -0.178],
                [1.095, 1.095, 1.095],
                0.99,
                style,
            ),
            // A saturation of 1 (no crosstalk), and of 0; a slope of 0; reciprocals at their
            // floor of 0.01; a power of 1 in two channels.
            cdl(
                [0.8, 1.2, 1.0],
                [0.1, 0.0, -0.1],
                [2.2, 1.0, 1.0],
                1.0,
                style,
            ),
            cdl(
                [0.0, 1.0, 0.005],
                [0.5, 0.0, 0.25],
                [0.004, 1.5, 0.5],
                0.0,
                style,
            ),
            cdl(
                [100.0, 1e-3, 2.0],
                [-50.0, 3.0, 0.0],
                [50.0, 1e-3, 0.7],
                0.005,
                style,
            ),
        ];
        for (k, c) in list.into_iter().enumerate() {
            let case = Case::new(format!("{style:?} {k} {c:?}"), c);
            if k == 0 {
                bases.push(case.clone());
            }
            cases.push(case);
        }
    }
    // Refusals on the JSON route: a negative slope, a zero power, a negative saturation.
    for c in [
        cdl([-0.9, 1.0, 1.0], [0.0; 3], [1.2; 3], 1.0, CdlStyle::Asc),
        cdl([1.0; 3], [0.0; 3], [1.2, 0.0, 1.2], 1.0, CdlStyle::NoClamp),
        cdl([1.0; 3], [0.0; 3], [1.2; 3], -1.17, CdlStyle::Asc),
    ] {
        cases.push(Case::new(format!("refused {c:?}"), c));
    }
    // The YAML route, which the generated NaN and infinite cases take: a NaN offset (accepted,
    // compared under W0002 in its channel), an infinite slope and power (accepted), and a NaN
    // power (refused).
    let nan = f64::NAN;
    let inf = f64::INFINITY;
    for c in [
        cdl(
            [1.2, 1.0, 0.9],
            [0.1, nan, 0.0],
            [1.1; 3],
            0.9,
            CdlStyle::Asc,
        ),
        cdl(
            [1.2, 1.0, 0.9],
            [nan, 0.0, -0.1],
            [0.8; 3],
            1.3,
            CdlStyle::NoClamp,
        ),
        cdl(
            [inf, 1.0, 0.9],
            [0.1, 0.0, 0.0],
            [1.1, inf, 0.9],
            0.9,
            CdlStyle::NoClamp,
        ),
        cdl(
            [1.2, 1.0, 0.9],
            [0.1, 0.0, 0.0],
            [1.1, nan, 0.9],
            0.9,
            CdlStyle::Asc,
        ),
    ] {
        cases.push(Case::new(format!("YAML {c:?}"), c).w0002_nowhere());
    }
    battery::run(&CdlFamily { cases, bases });
}
