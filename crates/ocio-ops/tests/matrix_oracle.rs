// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Matrix renderers against the wheel, bit for bit: through the oracle test battery
//! (`ocio_testkit::battery`), every case in both directions, with fast math on and off (the op
//! has no fast path), on the tier's probe sets (`OCIO_RS_TIER`); and, outside it, every width
//! from 1 to 9 pixels, so that both of the Windows wheel's loops (the four-pixel one and the
//! one that finishes the last `numPixels % 4` pixels, `matrix_op_cpu.rs`) meet NaNs of
//! different signs and payloads.
//!
//! The oracle builds a MatrixTransform in a raw config and applies its CPU processor to F32
//! RGBA pixels, one row in one call. The family builds the data as the wheel does:
//! - the transform: `setMatrix`, `setOffset`, `setDirection`, then, for the Python constructor
//!   (finite values, a JSON spec), `validate`, which prefixes its errors with "MatrixTransform
//!   validation failed: " (src/bindings/python/transforms/PyMatrixTransform.cpp:24-35,
//!   src/OpenColorIO/transforms/MatrixTransform.cpp:40-53, 88-129 @ v2.5.2);
//! - `BuildMatrixOp`: `validate`, then the op in the transform's direction
//!   (src/OpenColorIO/ops/matrix/MatrixOp.cpp:338-352, 395-404);
//! - the processor finalizes the ops before optimizing them: an inverse op's data becomes its
//!   `getAsForward()` (MatrixOp.cpp:164-171; src/OpenColorIO/OpOptimizers.cpp:598-609,
//!   src/OpenColorIO/CPUProcessor.cpp:311-338);
//! - the optimizer removes a no-op (the identity within 1e-6 on the diagonal, without
//!   offsets; `RemoveNoOps`, OpOptimizers.cpp:680), and the CPU processor then adds an identity
//!   matrix op, `CreateIdentityMatrixOp` (CPUProcessor.cpp:327-332): a scale of 1, which
//!   quiets signaling NaNs. Nothing else applies to one Matrix op at F32: there is no pair to
//!   combine, no simpler op, and the separable-prefix bake only applies to integer input bit
//!   depths (OpOptimizers.cpp:559-563).
//!
//! The renderers write every channel, so no channel passes through, and they have no other
//! numeric profile.

use std::hint::black_box;

use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::ops::matrix::MatrixOpData;
use ocio_ops::ops::matrix::matrix_op_cpu::get_matrix_renderer;
use ocio_testkit::Oracle;
use ocio_testkit::battery::params::{A, B, Case, Channels, G, Params, Precision, R, Slot};
use ocio_testkit::battery::{
    self, Combo, Direction, Family, Format, Port, Spec, Validation, yaml_list,
};
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::probe::Rng;
use serde_json::json;

/// The parameters of a MatrixTransform: the matrix, row by row, and the offsets.
#[derive(Debug, Clone, PartialEq)]
struct Matrix {
    matrix: [f64; 16],
    offset: [f64; 4],
}

impl Params for Matrix {
    /// A value of row `i` of the matrix, or offset `i`, applies to output channel `i`.
    fn slots(&self) -> Vec<Slot> {
        const CHANNELS: [Channels; 4] = [R, G, B, A];
        let mut slots: Vec<Slot> = (0..16)
            .map(|k| {
                let name = format!("matrix[{}][{}]", k / 4, k % 4);
                Slot::new(name, Precision::F64AsF32, CHANNELS[k / 4])
            })
            .collect();
        slots.extend(Slot::rgba("offset", Precision::F64AsF32));
        slots
    }
    fn get(&self, i: usize) -> f64 {
        if i < 16 {
            self.matrix[i]
        } else {
            self.offset[i - 16]
        }
    }
    fn set(&mut self, i: usize, v: f64) {
        if i < 16 {
            self.matrix[i] = v;
        } else {
            self.offset[i - 16] = v;
        }
    }
}

impl Matrix {
    /// Whether every value is finite: then a JSON transform spec can hold them, and the Python
    /// constructor builds the transform.
    fn all_finite(&self) -> bool {
        self.matrix
            .iter()
            .chain(&self.offset)
            .all(|v| v.is_finite())
    }
}

/// The port's direction for the battery's.
fn port_direction(direction: Direction) -> TransformDirection {
    match direction {
        Direction::Forward => TransformDirection::Forward,
        Direction::Inverse => TransformDirection::Inverse,
    }
}

/// The renderer the wheel's CPU processor applies for a MatrixTransform of `p` in `direction`
/// (module docs), or the wheel's exception text.
fn port_renderer(
    p: &Matrix,
    direction: Direction,
) -> Result<std::sync::Arc<dyn ocio_ops::op::CpuOp>, String> {
    let mut data = MatrixOpData::new();
    data.set_rgba(&p.matrix);
    data.set_rgba_offsets(&p.offset);
    data.set_direction(port_direction(direction));
    if p.all_finite() {
        data.validate()
            .map_err(|e| format!("MatrixTransform validation failed: {}", e.message()))?;
    }
    data.validate().map_err(|e| e.message().to_string())?;
    let forward = data.get_as_forward().map_err(|e| e.message().to_string())?;
    let rendered = if forward.is_no_op() {
        MatrixOpData::create_diagonal_matrix(1.0)
    } else {
        forward
    };
    get_matrix_renderer(&black_box(rendered)).map_err(|e| e.message().to_string())
}

/// The processor of a MatrixTransform of `p` in `direction`, for the oracle.
fn matrix_spec(p: &Matrix, direction: Direction) -> Spec {
    if p.all_finite() {
        Spec::Transform(json!({
            "class": "MatrixTransform",
            "args": {
                "matrix": p.matrix,
                "offset": p.offset,
                "direction": direction.oracle_enum(),
            },
        }))
    } else {
        Spec::Yaml(format!(
            "!<MatrixTransform> {{matrix: {}, offset: {}, direction: {}}}",
            yaml_list(&p.matrix),
            yaml_list(&p.offset),
            direction.yaml()
        ))
    }
}

/// MatrixTransform.
struct MatrixFamily {
    cases: Vec<Case<Matrix>>,
    bases: Vec<Case<Matrix>>,
}

impl Family for MatrixFamily {
    type Params = Matrix;

    fn name(&self) -> String {
        "MatrixTransform".to_string()
    }
    fn cases(&self) -> Vec<Case<Matrix>> {
        self.cases.clone()
    }
    fn mutation_bases(&self) -> Vec<Case<Matrix>> {
        self.bases.clone()
    }
    fn spec(&self, p: &Matrix, direction: Direction) -> Spec {
        matrix_spec(p, direction)
    }
    fn port(&self, p: &Matrix, combo: &Combo) -> Result<Port, String> {
        let renderer = port_renderer(p, combo.direction)?;
        Ok(Port::in_place(move |px| renderer.apply(px)))
    }
    fn validation(&self) -> Validation {
        Validation::Ported
    }
}

/// A diagonal matrix.
fn diagonal(d: [f64; 4]) -> [f64; 16] {
    let mut m = [0.; 16];
    for (i, v) in d.into_iter().enumerate() {
        m[i * 5] = v;
    }
    m
}

/// A typical case of each renderer: a scale, a scale with offsets, a matrix, and a matrix with
/// offsets. The first two and the last are upstream's (tests/cpu/ops/matrix/MatrixOpCPU_tests.cpp
/// and MatrixOpData_tests.cpp:576-581 @ v2.5.2).
fn typical() -> [Case<Matrix>; 4] {
    let upstream_matrix = [
        0.9f32, 0.8, -0.7, 0.6, -0.4, 0.5, 0.3, 0.2, 0.1, -0.2, 0.4, 0.3, -0.5, 0.6, 0.7, 0.8,
    ]
    .map(f64::from);
    let upstream_offset = [-0.1f32, 0.2, -0.3, 0.4].map(f64::from);
    [
        Case::new(
            "scale",
            Matrix {
                matrix: diagonal([2.0, 0.5, 4.0, 1.5]),
                offset: [0.0; 4],
            },
        ),
        Case::new(
            "scale with offsets",
            Matrix {
                matrix: diagonal([2.0, 2.0, 2.0, 2.0]),
                offset: [1.0, 2.0, 3.0, 4.0],
            },
        ),
        Case::new(
            "matrix",
            Matrix {
                matrix: upstream_matrix,
                offset: [0.0; 4],
            },
        ),
        Case::new(
            "matrix with offsets",
            Matrix {
                matrix: upstream_matrix,
                offset: upstream_offset,
            },
        ),
    ]
}

#[test]
fn matrix_transform_matches_the_wheel() {
    let bases = typical().to_vec();
    let mut cases = bases.clone();
    // A matrix that only has crosstalk, as upstream's matrix_with_offset_renderer test makes.
    let mut crosstalk = diagonal([2.0; 4]);
    crosstalk[3] = 0.5;
    cases.push(Case::new(
        "upstream's matrix with offsets",
        Matrix {
            matrix: crosstalk,
            offset: [1.0, 2.0, 3.0, 4.0],
        },
    ));
    // RGB to XYZ (sRGB primaries), with alpha passed through by the matrix.
    cases.push(Case::new(
        "sRGB to XYZ",
        Matrix {
            matrix: [
                0.4124564, 0.3575761, 0.1804375, 0.0, 0.2126729, 0.7151522, 0.0721750, 0.0,
                0.0193339, 0.1191920, 0.9503041, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            offset: [0.0; 4],
        },
    ));
    // No-ops, which the optimizer removes: the identity, and one within 1e-6 of it.
    cases.push(Case::new(
        "identity",
        Matrix {
            matrix: diagonal([1.0; 4]),
            offset: [0.0; 4],
        },
    ));
    cases.push(Case::new(
        "near identity",
        Matrix {
            matrix: diagonal([1.0 + 5e-7, 1.0, 1.0 - 5e-7, 1.0]),
            offset: [0.0; 4],
        },
    ));
    // A singular matrix: forward it renders, inverse it is refused.
    let mut singular = diagonal([1.0, 0.0, 1.0, 1.0]);
    singular[3] = 0.2;
    cases.push(Case::new(
        "singular",
        Matrix {
            matrix: singular,
            offset: [0.0; 4],
        },
    ));
    // An explicit case on the YAML spec the generated NaN and infinite cases take, compared bit
    // for bit: the renderers follow each wheel's operand orders.
    let nan = f64::NAN;
    cases.push(
        Case::new(
            "NaN in a matrix with offsets",
            Matrix {
                matrix: [
                    0.9, 0.8, -0.7, nan, -0.4, 0.5, 0.3, 0.2, 0.1, -0.2, nan, 0.3, -0.5, 0.6, 0.7,
                    0.8,
                ],
                offset: [-0.1, nan, -0.3, 0.4],
            },
        )
        .w0002_nowhere(),
    );
    cases.push(
        Case::new(
            "NaN in a scale",
            Matrix {
                matrix: diagonal([2.0, nan, 0.5, 1.0]),
                offset: [0.0; 4],
            },
        )
        .w0002_nowhere(),
    );
    battery::run(&MatrixFamily { cases, bases });
}

/// A pixel value: a NaN of either sign with one of a few payloads, quiet or signaling, an
/// infinity, 0 of either sign, or a finite value.
fn special_value(rng: &mut Rng) -> f32 {
    let payloads = [0x40_0000u32, 0x40_1234, 0x7f_ffff, 0x00_0001, 0x2a_aaaa];
    let pick = rng.next_u64();
    match pick % 8 {
        0..=2 => {
            let payload = payloads[(pick >> 8) as usize % payloads.len()];
            let sign = ((pick >> 16) as u32 & 1) << 31;
            f32::from_bits(sign | 0x7f80_0000 | payload)
        }
        3 => f32::INFINITY,
        4 => f32::NEG_INFINITY,
        5 => {
            if pick & 0x100 == 0 {
                0.0
            } else {
                -0.0
            }
        }
        _ => ((pick >> 32) % 33) as f32 / 8.0 - 2.0,
    }
}

/// Every width from 1 to 9 pixels, and 16 and 17, of pixels full of NaNs, through each
/// renderer, with finite and with NaN parameters, compared bit for bit: the Windows wheel sums
/// the products in one order in its four-pixel loop and in another in the loop that finishes
/// the last `numPixels % 4` pixels, and multiplies blue by the scale the other way round there.
#[test]
fn every_width_matches_the_wheel_bit_for_bit() {
    let nan = f64::NAN;
    let mut matrices: Vec<Matrix> = typical().iter().map(|c| c.params().clone()).collect();
    for m in typical() {
        // NaN parameters (YAML's `.nan`, positive), in every row and, for the scales, only on
        // the diagonal, blue's included, so that the matrix keeps its renderer; and an offset,
        // where there are some.
        let mut p = m.params().clone();
        let diagonal = (0..16).all(|k| k % 5 == 0 || p.matrix[k] == 0.0);
        let at = if diagonal {
            [0, 5, 10, 15]
        } else {
            [0, 6, 9, 15]
        };
        for k in at {
            p.matrix[k] = nan;
        }
        if p.offset.iter().any(|&o| o != 0.0) {
            p.offset[1] = nan;
        }
        matrices.push(p);
    }
    let widths: Vec<usize> = (1..=9).chain([16, 17]).collect();
    let mut rng = Rng::new(0x3a71_c0de);
    let mut cases = Vec::new();
    for index in 0..matrices.len() {
        for direction in [Direction::Forward, Direction::Inverse] {
            for &width in &widths {
                let pixels: Vec<f32> = (0..4 * width).map(|_| special_value(&mut rng)).collect();
                cases.push((index, direction, pixels));
            }
        }
    }
    let combo = Combo {
        direction: Direction::Forward,
        fast_math: true,
        format: Format::F32_RGBA,
    };
    let inputs: Vec<Vec<u8>> = cases
        .iter()
        .map(|(_, _, pixels)| pixels.iter().flat_map(|v| v.to_ne_bytes()).collect())
        .collect();
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .zip(&inputs)
        .map(|((index, direction, _), input)| BatchCall {
            cmd: "cpu_apply",
            args: matrix_spec(&matrices[*index], *direction).cpu_apply_args(&combo),
            blobs: vec![input],
        })
        .collect();
    // Pixels: live, as the CPU and the math library belong to the machine.
    let results = Oracle::get().batch(&calls, false);

    let mut failures = Vec::new();
    for ((index, direction, pixels), result) in cases.iter().zip(results) {
        let result = result.unwrap_or_else(|e| panic!("{e}"));
        let matrix = &matrices[*index];
        let what = format!("{matrix:?} {direction:?}, {} pixels", pixels.len() / 4);
        let port = port_renderer(matrix, *direction);
        match (result.result.get("exception"), port) {
            (None, Ok(renderer)) => {
                let wheel: Vec<u32> = result.blobs[0]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|b| u32::from_ne_bytes(*b))
                    .collect();
                let mut port = pixels.clone();
                renderer.apply(&mut port);
                let port: Vec<u32> = port.iter().map(|v| v.to_bits()).collect();
                if port != wheel {
                    let first = port.iter().zip(&wheel).position(|(p, w)| p != w).unwrap();
                    failures.push(format!(
                        "{what}: value {first} (pixel {}, channel {}) of input {:#010x}: wheel \
                         {:#010x}, port {:#010x}",
                        first / 4,
                        first % 4,
                        pixels[first].to_bits(),
                        wheel[first],
                        port[first]
                    ));
                }
            }
            (Some(exception), Err(message)) if exception["message"] == message.as_str() => {}
            (exception, port) => failures.push(format!(
                "{what}: wheel {exception:?}, port {:?}",
                port.map(|_| ())
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}
