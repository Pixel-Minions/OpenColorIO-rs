// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Lut1D op in processors against the wheel, through the oracle's `processor_ops`: lists of
//! `Lut1DTransform`s (built empty and set, as in `lut1d_op_data_oracle.rs`), with a
//! `RangeTransform` or a `MatrixTransform` next to them, at three optimization levels, F32 in
//! and out. Each processor's cache ID is `CacheIDHash` of its ops' cache IDs
//! (`Processor::Impl::getCacheID`, src/OpenColorIO/Processor.cpp:331-348 @ v2.5.2;
//! `OpRcPtrVec::getCacheID`, src/OpenColorIO/Op.cpp:448-466), and a Lut1D op's is `<Lut1D `,
//! its data's (which hashes the values' bytes), `>`. So the processor's cache ID checks the
//! ops the wheel builds and finalizes, and the optimized processor's (`getOptimizedProcessor`,
//! Processor.cpp:382-399: finalize, optimize, optimizeForBitdepth) the ops the optimizer
//! leaves: an identity LUT replaced by a [0, 1] Range or removed, an identity range before a
//! LUT removed (`RangeOp::canCombineWith`, src/OpenColorIO/ops/range/RangeOp.cpp:100-148).
//!
//! The port builds the ops as the wheel does: a `GroupTransform` builds each child forward;
//! `BuildLut1DOp` validates the data and creates a Lut1D op with a copy of it
//! (src/OpenColorIO/ops/lut1d/Lut1DOp.cpp:244-253), `BuildRangeOp` and `BuildMatrixOp`
//! likewise; the processor finalizes them (Processor.cpp:623-641).
//!
//! Composing two LUTs and inverse LUTs wait for Phase 2 (WP 2.5);
//! `phase_2_cases_wait_for_their_ports` pins the port's refusals where the wheel succeeds.

use core::ffi::c_ulong;

use ocio_ops::exception::{Exception, Result};
use ocio_ops::hash_utils::cache_id_hash;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{BitDepth, Lut1DHueAdjust, OptimizationFlags, TransformDirection};
use ocio_ops::ops::lut1d::Lut1DOpData;
use ocio_ops::ops::lut1d::lut1d_op::{NOT_PORTED_COMPOSE, create_lut1d_op};
use ocio_ops::ops::lut1d::lut1d_op_data::Lut3by1DArray;
use ocio_ops::ops::matrix::MatrixOpData;
use ocio_ops::ops::matrix::matrix_op::create_matrix_op;
use ocio_ops::ops::range::RangeOpData;
use ocio_ops::ops::range::range_op::create_range_op;
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use serde_json::{Value, json};

use TransformDirection::{Forward as F, Inverse as I};

/// A `Lut1DTransform`'s settings: the half domain, the length, entries set to `f(x)` of the
/// identity's value `x` (or left as they are), the hue adjust and the direction.
#[derive(Debug, Clone, Copy)]
struct Lut {
    half_domain: bool,
    length: c_ulong,
    /// `None`: the identity; otherwise each entry's RGB from the identity's value.
    curve: Option<fn(f32) -> [f32; 3]>,
    hue: Lut1DHueAdjust,
    dir: TransformDirection,
}

/// A transform of the lists.
#[derive(Debug, Clone, Copy)]
enum T {
    Lut(Lut),
    /// A clamping `RangeTransform`: `[minIn, maxIn, minOut, maxOut]`.
    Range([f64; 4]),
    /// A `MatrixTransform` scaling RGB by 2 with an offset of 0.1.
    Matrix,
}

const SCALE: [f64; 16] = [
    2., 0., 0., 0., 0., 2., 0., 0., 0., 0., 2., 0., 0., 0., 0., 1.,
];
const OFFSET: [f64; 4] = [0.1, 0.1, 0.1, 0.];

impl Lut {
    fn std(length: c_ulong, curve: Option<fn(f32) -> [f32; 3]>) -> Lut {
        Lut {
            half_domain: false,
            length,
            curve,
            hue: Lut1DHueAdjust::None,
            dir: F,
        }
    }

    /// The identity's value of each entry, and the values the curve sets, as `setLength`
    /// fills them and `setValue` sets them.
    fn values(&self) -> Result<Lut3by1DArray> {
        let mut data = Lut1DOpData::new(2)?;
        data.set_input_half_domain(self.half_domain);
        Lut3by1DArray::new(data.get_half_flags(), 3, self.length, false)
    }

    fn entries(&self) -> Vec<(usize, [f32; 3])> {
        let Some(curve) = self.curve else {
            return Vec::new();
        };
        let identity = self.values().expect("a valid length");
        (0..self.length as usize)
            .map(|i| (i, curve(identity[3 * i])))
            .filter(|(_, rgb)| rgb.iter().all(|v| v.is_finite()))
            .collect()
    }

    fn spec(&self) -> Value {
        let mut calls = vec![
            json!(["setInputHalfDomain", self.half_domain]),
            json!(["setLength", self.length]),
        ];
        for (index, rgb) in self.entries() {
            calls.push(json!(["setValue", index, rgb[0], rgb[1], rgb[2]]));
        }
        let hue = match self.hue {
            Lut1DHueAdjust::None => "HUE_NONE",
            Lut1DHueAdjust::Dw3 => "HUE_DW3",
            Lut1DHueAdjust::Wypn => "HUE_WYPN",
        };
        calls.push(json!(["setHueAdjust", {"enum": hue}]));
        calls.push(json!(["setDirection", {"enum": dir_name(self.dir)}]));
        json!({"class": "Lut1DTransform", "args": {}, "calls": calls})
    }

    /// The port's data, as the transform's setters make it.
    fn port(&self) -> Result<Lut1DOpData> {
        let mut data = Lut1DOpData::new(2)?;
        data.set_input_half_domain(self.half_domain);
        *data.get_array_mut() = self.values()?;
        for (index, rgb) in self.entries() {
            for (c, v) in rgb.into_iter().enumerate() {
                data.get_array_mut()[3 * index + c] = v;
            }
        }
        data.set_hue_adjust(self.hue)?;
        data.set_direction(self.dir);
        Ok(data)
    }
}

fn dir_name(dir: TransformDirection) -> &'static str {
    match dir {
        F => "TRANSFORM_DIR_FORWARD",
        I => "TRANSFORM_DIR_INVERSE",
    }
}

fn transform(t: &T) -> Value {
    match t {
        T::Lut(lut) => lut.spec(),
        T::Range(b) => json!({"class": "RangeTransform", "args": {
            "minInValue": b[0], "maxInValue": b[1], "minOutValue": b[2], "maxOutValue": b[3],
        }}),
        T::Matrix => json!({"class": "MatrixTransform",
            "args": {"matrix": SCALE.to_vec(), "offset": OFFSET.to_vec()}}),
    }
}

/// The port's ops of `chain`, before the processor finalizes them.
fn port_raw_ops(chain: &[T]) -> Result<OpVec> {
    let mut raw = OpVec::new();
    for t in chain {
        match t {
            T::Lut(lut) => {
                // `BuildLut1DOp`: the transform's data, validated, then copied.
                let data = lut.port()?;
                data.validate()
                    .map_err(|e| Exception::new(e.message().to_string()))?;
                create_lut1d_op(&mut raw, data, F);
            }
            T::Range(b) => {
                let data = RangeOpData::with_values(b[0], b[1], b[2], b[3])?;
                create_range_op(&mut raw, data, F)?;
            }
            T::Matrix => {
                let mut data = MatrixOpData::new();
                data.set_rgba(&SCALE);
                data.set_rgba_offsets(&OFFSET);
                data.validate()?;
                create_matrix_op(&mut raw, data, F);
            }
        }
    }
    Ok(raw)
}

/// `Processor::Impl::getCacheID`: `<NOOP>` without ops, else the hash of their cache IDs.
fn processor_cache_id(ops: &OpVec) -> Result<String> {
    if ops.is_empty() {
        return Ok("<NOOP>".to_string());
    }
    Ok(cache_id_hash(&ops.get_cache_id()?))
}

/// The port's processor and optimized processor cache IDs.
fn port_cache_ids(chain: &[T], flags: OptimizationFlags) -> Result<(String, String)> {
    let mut ops = port_raw_ops(chain)?;
    ops.finalize()?;
    let processor = processor_cache_id(&ops)?;
    // `getOptimizedProcessor`: a copy of the processor's ops.
    let mut optimized = ops.clone();
    optimized.finalize()?;
    optimized.optimize(flags)?;
    optimized.optimize_for_bitdepth(BitDepth::F32, BitDepth::F32, flags)?;
    Ok((processor, processor_cache_id(&optimized)?))
}

fn wheel_cache_ids(
    chains: &[(Vec<T>, &str)],
) -> Vec<std::result::Result<(String, String), String>> {
    let calls: Vec<BatchCall<'_>> = chains
        .iter()
        .map(|(chain, flags)| BatchCall {
            cmd: "processor_ops",
            args: json!({
                "transform": {"class": "GroupTransform",
                    "children": chain.iter().map(transform).collect::<Vec<_>>()},
                "optimization": flags,
            }),
            blobs: vec![],
        })
        .collect();
    Oracle::get()
        .batch(&calls, true)
        .into_iter()
        .map(|r| {
            let result = r.unwrap_or_else(|e| panic!("{e}")).result;
            match result.get("exception") {
                Some(e) => Err(e["message"].as_str().unwrap().to_string()),
                None => Ok((
                    result["processor"]["cache_id"]
                        .as_str()
                        .unwrap()
                        .to_string(),
                    result["optimized"]["cache_id"]
                        .as_str()
                        .unwrap()
                        .to_string(),
                )),
            }
        })
        .collect()
}

const FLAGS: [(&str, OptimizationFlags); 3] = [
    ("OPTIMIZATION_NONE", OptimizationFlags::NONE),
    ("OPTIMIZATION_DEFAULT", OptimizationFlags::DEFAULT),
    ("OPTIMIZATION_ALL", OptimizationFlags::ALL),
];

fn square(x: f32) -> [f32; 3] {
    [x * x, x * x * x, x.sqrt()]
}

fn scale_half(x: f32) -> [f32; 3] {
    [x * 0.5, x * 0.25, x * 2.0]
}

fn chains() -> Vec<Vec<T>> {
    let sq = T::Lut(Lut::std(17, Some(square)));
    let identity = T::Lut(Lut::std(17, None));
    let half_identity = T::Lut(Lut {
        half_domain: true,
        ..Lut::std(65536, None)
    });
    let half_scale = T::Lut(Lut {
        half_domain: true,
        ..Lut::std(65536, Some(scale_half))
    });
    let hue = T::Lut(Lut {
        hue: Lut1DHueAdjust::Dw3,
        ..Lut::std(33, Some(square))
    });
    let unit = T::Range([0., 1., 0., 1.]);
    let narrow = T::Range([0.1, 0.9, 0.1, 0.9]);
    vec![
        vec![sq],
        vec![T::Lut(Lut::std(1024, Some(square)))],
        vec![identity],
        vec![half_identity],
        vec![half_scale],
        vec![hue],
        vec![unit, sq],
        vec![unit, half_scale],
        vec![narrow, sq],
        vec![sq, T::Matrix],
        vec![T::Matrix, sq, T::Matrix],
        vec![unit, identity],
        vec![sq, unit],
    ]
}

#[test]
fn cache_ids_match_the_wheel() {
    let mut cases = Vec::new();
    for chain in chains() {
        for (name, flags) in FLAGS {
            cases.push((chain.clone(), name, flags));
        }
    }
    let wheel = wheel_cache_ids(
        &cases
            .iter()
            .map(|(chain, name, _)| (chain.clone(), *name))
            .collect::<Vec<_>>(),
    );
    let mut failures = Vec::new();
    let mut optimized_differs = 0;
    for ((chain, name, flags), wheel) in cases.iter().zip(wheel) {
        optimized_differs += usize::from(wheel.as_ref().is_ok_and(|(p, o)| p != o));
        let port = port_cache_ids(chain, *flags).map_err(|e| e.message().to_string());
        if port != wheel {
            failures.push(format!(
                "{chain:?} {name}\n  wheel {wheel:?}\n  port  {port:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
    // The optimizer changes some.
    assert!(optimized_differs > 0);
}

/// Two LUTs that compose, and an inverse LUT: the wheel builds and optimizes them, the port
/// refuses until Phase 2 (WP 2.5). When it ports them, this test fails: compare these cases
/// like the others.
#[test]
fn phase_2_cases_wait_for_their_ports() {
    let sq = T::Lut(Lut::std(17, Some(square)));
    let inverse = T::Lut(Lut {
        dir: I,
        ..Lut::std(17, Some(square))
    });
    let cases = [
        (
            vec![sq, sq],
            "OPTIMIZATION_DEFAULT",
            OptimizationFlags::DEFAULT,
            NOT_PORTED_COMPOSE,
        ),
        (
            vec![inverse],
            "OPTIMIZATION_NONE",
            OptimizationFlags::NONE,
            "Lut1D: the inverse 1D LUT is not ported yet (WP 2.1).",
        ),
    ];
    let wheel = wheel_cache_ids(
        &cases
            .iter()
            .map(|(chain, name, _, _)| (chain.clone(), *name))
            .collect::<Vec<_>>(),
    );
    for ((chain, _, flags, message), wheel) in cases.iter().zip(wheel) {
        assert!(wheel.is_ok(), "{chain:?}: {wheel:?}");
        let port = port_cache_ids(chain, *flags);
        assert_eq!(
            port.map_err(|e| e.message().to_string()),
            Err(message.to_string()),
            "{chain:?}"
        );
    }
}
