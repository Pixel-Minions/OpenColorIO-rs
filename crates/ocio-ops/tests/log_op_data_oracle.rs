// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `LogOpData` against the wheel, through the transforms that hold one: `LogTransform`,
//! `LogAffineTransform` and `LogCameraTransform` (src/OpenColorIO/transforms/Log*Transform.cpp
//! @ v2.5.2), each built empty and set through its setters, so the binding's constructors,
//! some of which validate, never refuse one:
//! - `validate`: each transform's `validate` runs the data's and prefixes its message with
//!   `<class> validation failed: ` (LogTransform.cpp:47-59, LogAffineTransform.cpp:48-61,
//!   LogCameraTransform.cpp:50-67, which also needs the break: always set here);
//! - `equals`: each transform's `equals` compares the data with it (LogTransform.cpp:71-75,
//!   LogAffineTransform.cpp:73-77, LogCameraTransform.cpp:79-83), through `transform_text`
//!   pairs.
//!
//! The data's cache ID is checked with the Log op's, through a CPU processor
//! (`log_op_oracle.rs`).

use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::ops::log::log_op_data::{LogAffineParameter, LogOpData};
use ocio_testkit::transform_text::{Built, TransformTextRequest};
use serde_json::{Value, json};

/// The class of the transform, and what it sets besides the base and the direction.
#[derive(Debug, Clone)]
enum Kind {
    Log,
    /// The four affine parameters, `[logSideSlope, logSideOffset, linSideSlope,
    /// linSideOffset]`.
    Affine([[f64; 3]; 4]),
    /// The four affine parameters, the break and maybe the linear slope.
    Camera([[f64; 3]; 4], [f64; 3], Option<[f64; 3]>),
}

#[derive(Debug, Clone)]
struct Case {
    kind: Kind,
    base: f64,
    dir: TransformDirection,
}

const SETTERS: [(&str, LogAffineParameter); 4] = [
    ("setLogSideSlopeValue", LogAffineParameter::LogSideSlope),
    ("setLogSideOffsetValue", LogAffineParameter::LogSideOffset),
    ("setLinSideSlopeValue", LogAffineParameter::LinSideSlope),
    ("setLinSideOffsetValue", LogAffineParameter::LinSideOffset),
];

impl Case {
    fn class(&self) -> &'static str {
        match self.kind {
            Kind::Log => "LogTransform",
            Kind::Affine(_) => "LogAffineTransform",
            Kind::Camera(..) => "LogCameraTransform",
        }
    }

    /// The transform's spec: the setters in the order the port's data gets them.
    fn spec(&self) -> Value {
        let mut calls = vec![json!(["setBase", self.base])];
        let mut args = json!({});
        let params = match &self.kind {
            Kind::Log => None,
            Kind::Affine(p) => Some(p),
            Kind::Camera(p, brk, _) => {
                args = json!({"linSideBreak": brk});
                Some(p)
            }
        };
        if let Some(params) = params {
            for ((setter, _), values) in SETTERS.iter().zip(params) {
                calls.push(json!([setter, values]));
            }
        }
        if let Kind::Camera(_, _, Some(slope)) = &self.kind {
            calls.push(json!(["setLinearSlopeValue", slope]));
        }
        let dir = match self.dir {
            TransformDirection::Forward => "TRANSFORM_DIR_FORWARD",
            TransformDirection::Inverse => "TRANSFORM_DIR_INVERSE",
        };
        calls.push(json!(["setDirection", {"enum": dir}]));
        json!({"class": self.class(), "args": args, "calls": calls})
    }

    /// The port's data, as the transform builds it: `m_data(2.0f, TRANSFORM_DIR_FORWARD)`
    /// (LogTransform.cpp:24-27, LogAffineTransform.cpp:25-28), plus the break for a camera log
    /// (LogCameraTransform.cpp:26-30), then the setters.
    fn port(&self) -> LogOpData {
        let mut data = LogOpData::new(f64::from(2.0f32), TransformDirection::Forward);
        if let Kind::Camera(_, brk, _) = &self.kind {
            data.set_value(LogAffineParameter::LinSideBreak, brk)
                .unwrap();
        }
        data.set_base(self.base);
        if let Kind::Affine(params) | Kind::Camera(params, ..) = &self.kind {
            for ((_, param), values) in SETTERS.iter().zip(params) {
                data.set_value(*param, values).unwrap();
            }
        }
        if let Kind::Camera(_, _, Some(slope)) = &self.kind {
            data.set_value(LogAffineParameter::LinearSlope, slope)
                .unwrap();
        }
        data.set_direction(self.dir);
        data
    }

    /// The port's validation, with the transform's prefix.
    fn port_validate(&self) -> Result<(), String> {
        let data = self.port();
        let checked = data.validate().and_then(|()| {
            // LogCameraTransform.cpp:56-59.
            if matches!(self.kind, Kind::Camera(..)) && data.red_params().len() < 5 {
                return Err(ocio_ops::exception::Exception::new(
                    "LinSideBreak has to be defined.",
                ));
            }
            Ok(())
        });
        checked.map_err(|e| format!("{} validation failed: {}", self.class(), e.message()))
    }
}

/// Values a slope or an offset takes: zeros of both signs, values that `IsScalarEqualToZero`
/// may or may not take for 0, ordinary ones, and large ones.
const VALUES: [f64; 12] = [
    0.0,
    -0.0,
    1.0,
    -1.0,
    0.5,
    1e-7,
    -1e-30,
    5e-324,
    1e-40,
    3.5e38,
    -1e300,
    0.123456789,
];

/// Bases: 1 and the ones at or below 0 that validation refuses, and valid ones.
const BASES: [f64; 12] = [
    1.0,
    0.0,
    -0.0,
    -2.0,
    1e-300,
    -1e-300,
    1.0f64.next_up(),
    1.0f64.next_down(),
    2.0,
    10.0,
    std::f64::consts::E,
    1e300,
];

fn identity_params() -> [[f64; 3]; 4] {
    [[1.0; 3], [0.0; 3], [1.0; 3], [0.0; 3]]
}

/// Cases for validation: each base on each class; each value of each parameter on one
/// channel and on all; channels of different values.
fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    let brk = [0.1, 0.2, 0.3];
    for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
        for base in BASES {
            out.push(Case {
                kind: Kind::Log,
                base,
                dir,
            });
            out.push(Case {
                kind: Kind::Affine(identity_params()),
                base,
                dir,
            });
            out.push(Case {
                kind: Kind::Camera(identity_params(), brk, None),
                base,
                dir,
            });
        }
        for param in 0..4 {
            for value in VALUES {
                for channel in [Some(0), Some(1), Some(2), None] {
                    let mut params = identity_params();
                    match channel {
                        Some(c) => params[param][c] = value,
                        None => params[param] = [value; 3],
                    }
                    out.push(Case {
                        kind: Kind::Affine(params),
                        base: 10.0,
                        dir,
                    });
                    out.push(Case {
                        kind: Kind::Camera(params, brk, Some([value, 1.0, 2.0])),
                        base: 2.0,
                        dir,
                    });
                }
            }
        }
        // Both slopes 0, and a base that is also invalid: the first check's message.
        out.push(Case {
            kind: Kind::Affine([[0.0; 3], [0.0; 3], [0.0; 3], [0.0; 3]]),
            base: 1.0,
            dir,
        });
        out.push(Case {
            kind: Kind::Camera([[0.0; 3], [0.0; 3], [1.0; 3], [0.0; 3]], brk, None),
            base: -1.0,
            dir,
        });
    }
    out
}

fn wheel_validate(built: &Built) -> Result<(), String> {
    match built {
        Built::Raised(e) => Err(format!("raised building it: {}", e.message)),
        Built::Text(text) => match &text.validate {
            Some(e) => Err(e.message.clone()),
            None => Ok(()),
        },
    }
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
        let wheel = wheel_validate(built);
        refused += usize::from(wheel.is_err());
        let port = case.port_validate();
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
    // Both outcomes are covered.
    assert!(refused > 0 && refused < cases.len(), "{refused}");
}

#[test]
fn equality_matches_the_wheel() {
    let brk = [0.1, 0.2, 0.3];
    let mut affine = vec![identity_params()];
    for (param, value) in [
        (0, 0.5),
        (1, -0.0),
        (1, 0.25),
        (2, 2.0),
        (3, -0.0),
        (3, 1e-300),
    ] {
        for channel in [Some(0), Some(2), None] {
            let mut params = identity_params();
            match channel {
                Some(c) => params[param][c] = value,
                None => params[param] = [value; 3],
            }
            affine.push(params);
        }
    }
    let mut cases = Vec::new();
    for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
        for base in [2.0, 10.0, -0.0, 0.0] {
            cases.push(Case {
                kind: Kind::Log,
                base,
                dir,
            });
        }
        for params in &affine {
            for base in [2.0, 10.0] {
                cases.push(Case {
                    kind: Kind::Affine(*params),
                    base,
                    dir,
                });
            }
        }
        for params in affine.iter().step_by(3) {
            for (b, slope) in [
                (brk, None),
                (brk, Some([1.0, 1.0, 1.0])),
                (brk, Some([1.0, 1.0, -0.0])),
                ([0.1, 0.2, -0.0], Some([1.0, 1.0, 0.0])),
                ([0.1, 0.2, 0.0], Some([1.0, 1.0, 0.0])),
                ([0.1, 0.1, 0.1], None),
            ] {
                cases.push(Case {
                    kind: Kind::Camera(*params, b, slope),
                    base: 2.0,
                    dir,
                });
            }
        }
    }
    // Every pair of the same class: the binding's equals() takes only its own class.
    let mut pairs = Vec::new();
    for i in 0..cases.len() {
        for j in 0..cases.len() {
            if cases[i].class() == cases[j].class() {
                pairs.push((i, j));
            }
        }
    }
    let reply = TransformTextRequest {
        transforms: cases.iter().map(Case::spec).collect(),
        pairs: pairs.clone(),
    }
    .run();

    let mut failures = Vec::new();
    let mut equal = 0;
    for ((i, j), wheel) in pairs.iter().zip(&reply.pairs) {
        let wheel =
            wheel.unwrap_or_else(|| panic!("no equals for {:?} {:?}", cases[*i], cases[*j]));
        equal += usize::from(wheel && i != j);
        let port = cases[*i].port().equals(&cases[*j].port());
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
    // Some different transforms are equal: zeros of both signs.
    assert!(equal > 0);
}
